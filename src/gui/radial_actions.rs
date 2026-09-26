use crate::radial::bindings::RadialBindingResolver;
use crate::radial::bindings::{
    BindingUnavailable, PreparedBinding, PreparedCell, RadialPrepareEnvelope, RadialPrepareReply,
    prepare_deferred_binding, project_menu_frame_with_style,
};
use crate::radial::context::{CompiledContextRules, InvocationContext};
use crate::radial::dynamic::{
    DeferredBindingKind, DynamicCandidate, DynamicSnapshots, DynamicSourceState,
    ExactCommandDisposition, FrozenAvailability, FrozenBinding, FrozenEntryKind,
};
use crate::radial::handoff::{
    DeferredDispatchOrigin, DeferredResolutionEnvelope, DeferredResolutionReply,
    DeferredResolutionResult, InteractionRequirement, RadialDispatchRequest,
    ResolvedDeferredSelection, interaction_requirement,
};
use crate::radial::model::CellContent;
use crate::universal_actions::{
    ActionSurface, ActionTargetResolver, ActionTargetResolverContext, PersistedActionCatalog,
    RootLauncherPolicy, UniversalActionInvocationContext, UniversalActionRegistry,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::{ActivationSource, DestructiveAction, LauncherApp};

const RADIAL_DISPATCH_TOMBSTONE_LIMIT: usize = 64;
const ACCEPTANCE_RUNTIME_PREPARE_HOLD_ENV: &str =
    "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_PREPARE_HOLD_FILE";
const ACCEPTANCE_RUNTIME_PREPARE_HOLD_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(4);

fn radial_acceptance_digest(parts: &[&str]) -> u64 {
    parts
        .iter()
        .flat_map(|part| part.bytes().chain([0]))
        .fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        })
}

fn radial_acceptance_id_digest(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn radial_acceptance_action_digest(action: &crate::actions::Action) -> u64 {
    radial_acceptance_digest(&[
        action.label.as_str(),
        action.desc.as_str(),
        action.action.as_str(),
        action.args.as_deref().unwrap_or_default(),
    ])
}

fn radial_acceptance_requirement_name(requirement: InteractionRequirement) -> &'static str {
    match requirement {
        InteractionRequirement::None => "none",
        InteractionRequirement::Deferred => "deferred",
        InteractionRequirement::Confirmation => "confirmation",
        InteractionRequirement::LauncherUi => "launcher_ui",
        InteractionRequirement::ExternalInput => "external_input",
        InteractionRequirement::ExclusiveCapture => "exclusive_capture",
    }
}

fn hold_acceptance_runtime_preparation(request: &crate::radial::bindings::RadialPrepareRequest) {
    use crate::radial::authoring::{
        RuntimePreparationTraceState as State, trace_runtime_preparation,
    };

    let Some(path) = std::env::var_os(ACCEPTANCE_RUNTIME_PREPARE_HOLD_ENV) else {
        return;
    };
    let path = std::path::Path::new(&path);
    if !path.is_file() {
        return;
    }

    trace_runtime_preparation(request.invocation_id, request.generation, State::GateHeld);
    let deadline = std::time::Instant::now() + ACCEPTANCE_RUNTIME_PREPARE_HOLD_TIMEOUT;
    while path.is_file() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    trace_runtime_preparation(
        request.invocation_id,
        request.generation,
        if path.is_file() {
            State::GateTimedOut
        } else {
            State::GateReleased
        },
    );
}

impl LauncherApp {
    fn radial_confirmation_enabled(&self) -> bool {
        self.require_confirm_destructive
            || self.radial_feature_settings.safety_policy
                == crate::radial::model::RadialSafetyPolicy::AlwaysConfirmDestructive
    }

    fn radial_interaction_requirement(
        &self,
        action: &crate::universal_actions::UniversalAction,
        requirement: InteractionRequirement,
    ) -> InteractionRequirement {
        radial_requirement_for_settings(action, requirement, self.radial_confirmation_enabled())
    }

    pub(super) fn invalidate_radial_leases(&mut self) {
        self.radial_current_preparation = None;
        self.radial_preparations.clear();
        self.radial_consumed_dispatches.clear();
        if self
            .pending_universal_confirm
            .as_ref()
            .is_some_and(|pending| pending.radial_request.is_some())
        {
            self.pending_universal_confirm = None;
            self.confirm_modal.close();
        }
    }
    pub(super) fn radial_lease_is_current(&self, request: &RadialDispatchRequest) -> bool {
        self.radial_preparations
            .get(&request.identity.invocation_id)
            == Some(&(
                request.identity.preparation_generation,
                request.identity.config_revision,
            ))
            && self.radial_consumed_dispatches.contains(&request.identity)
            && self.radial_current_preparation
                == Some((
                    request.identity.invocation_id,
                    request.identity.preparation_generation,
                    request.identity.config_revision,
                ))
    }
    pub(super) fn prepare_radial(&mut self, envelope: RadialPrepareEnvelope) {
        hold_acceptance_runtime_preparation(&envelope.request);
        let confirmation_required = self.radial_confirmation_enabled();
        let mut captured_request = envelope.request.clone();
        captured_request.invocation_query = self.query.clone();
        let request = &captured_request;
        let captured_query = request.invocation_query.clone();
        let captured_results = self.results.clone();
        self.radial_preparations.clear();
        self.radial_preparations.insert(
            request.invocation_id,
            (request.generation, request.document.revision),
        );
        self.radial_current_preparation = Some((
            request.invocation_id,
            request.generation,
            request.document.revision,
        ));
        let menu_id = request
            .allow_context_rules
            .then(|| CompiledContextRules::compile(&request.document.context_rules).ok())
            .flatten()
            .and_then(|rules| rules.select_menu(&request.context).cloned())
            .unwrap_or_else(|| request.requested_menu_id.clone());
        let resolver = ActionTargetResolver;
        let custom_len = self.custom_len.min(self.actions.len());
        let resolver_context = ActionTargetResolverContext::new(
            &self.folder_aliases,
            &self.bookmark_aliases,
            &self.actions[..custom_len],
        );
        let mut action_snapshot = self.universal_action_catalog_snapshot();
        let window_catalog = Arc::clone(&self.plugins.internal_services().window_catalog);
        let dashboard = action_snapshot.dashboard.clone();
        let recent_entries = action_snapshot.recent_entries.clone();
        let configured_queries: BTreeSet<_> = request
            .document
            .menus
            .iter()
            .flat_map(|menu| &menu.rings)
            .flat_map(|ring| &ring.cells)
            .filter_map(|cell| match &cell.content {
                CellContent::Dynamic {
                    source: crate::radial::model::DynamicSource::LauncherQuery { query, .. },
                } => Some(query.clone()),
                _ => None,
            })
            .collect();
        let mut query_entries = BTreeMap::new();
        for query in &configured_queries {
            let resolved: Vec<_> = self
                .search_read_only(query)
                .iter()
                .map(|action| resolver.resolve(action, &resolver_context))
                .collect();
            action_snapshot.extend(resolved.iter().cloned());
            query_entries.insert(query.clone(), resolved);
        }
        let captured_result_entries: Vec<_> = captured_results
            .iter()
            .map(|action| resolver.resolve(action, &resolver_context))
            .collect();
        // `actions` is the full stable launcher/application catalog. `custom_len`
        // only identifies which prefix the resolver should type as CustomAction;
        // it must never truncate the Applications dynamic source.
        let application_entries: Vec<_> = self
            .actions
            .iter()
            .map(|action| resolver.resolve(action, &resolver_context))
            .collect();
        action_snapshot.extend(captured_result_entries.iter().cloned());
        let entries = action_snapshot.entries;
        let catalog = PersistedActionCatalog::new(entries.clone());
        let registry = UniversalActionRegistry;
        let binding_resolver = RadialBindingResolver {
            catalog: &catalog,
            registry: &registry,
        };
        let mut unavailable = BTreeMap::new();
        let mut dynamic = BTreeMap::new();
        let mut static_cells = BTreeMap::new();
        let mut snapshots = DynamicSnapshots {
            generation: request.generation.0,
            ..Default::default()
        };
        if let Some(menu) = request
            .document
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
        {
            snapshots.favorites = dashboard
                .favorites
                .iter()
                .map(|favorite| crate::actions::Action {
                    label: favorite.label.clone(),
                    desc: "Fav".into(),
                    action: favorite.action.clone(),
                    args: favorite.args.clone(),
                })
                .map(|action| resolver.resolve(&action, &resolver_context))
                .map(|entry| dynamic_candidate_with_window_identity(&entry, &window_catalog))
                .collect();
            snapshots.clipboard = dashboard
                .clipboard_history
                .iter()
                .enumerate()
                .map(|(index, text)| DynamicCandidate {
                    stable_id: format!("clipboard:{index}"),
                    label: text.clone(),
                    action: None,
                    runtime_action: Some((
                        crate::universal_actions::ActionTarget::ClipboardEntry { index },
                        crate::actions::Action {
                            label: text.clone(),
                            desc: "Clipboard".into(),
                            action: format!("clipboard:copy:{index}"),
                            args: None,
                        },
                        crate::universal_actions::action_ids::RESULT_EXECUTE,
                    )),
                    runtime_identity: None,
                    unavailable_reason: None,
                    history_query: None,
                    kind: FrozenEntryKind::Action,
                })
                .collect();
            snapshots.recent = recent_entries
                .iter()
                .map(|(entry, query)| {
                    let mut candidate =
                        dynamic_candidate_with_window_identity(entry, &window_catalog);
                    candidate.history_query = Some(query.clone());
                    candidate
                })
                .collect();
            snapshots.applications = application_entries
                .iter()
                .map(|entry| dynamic_candidate_with_window_identity(entry, &window_catalog))
                .collect();
            snapshots.dashboard = entries
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.target,
                        crate::universal_actions::ActionTarget::Note { .. }
                            | crate::universal_actions::ActionTarget::Snippet { .. }
                    )
                })
                .map(|entry| dynamic_candidate_with_window_identity(entry, &window_catalog))
                .collect();
            snapshots
                .dashboard
                .extend(snapshots.favorites.iter().cloned().map(|mut candidate| {
                    candidate.stable_id = format!("dashboard:favorite:{}", candidate.stable_id);
                    candidate
                }));
            snapshots
                .dashboard
                .extend(snapshots.clipboard.iter().cloned().map(|mut candidate| {
                    candidate.stable_id = format!("dashboard:{}", candidate.stable_id);
                    candidate
                }));
            snapshots
                .dashboard
                .extend(dashboard.todos.iter().map(|todo| {
                    dashboard_status_candidate(
                        format!("dashboard:todo:{}", todo.id),
                        format!("Todo: {}", todo.text),
                        "Open Dashboard or Todo View to edit this stable todo",
                    )
                }));
            snapshots
                .dashboard
                .extend(
                    dashboard
                        .calendar
                        .event_titles
                        .iter()
                        .map(|(event_id, title)| {
                            dashboard_status_candidate(
                                format!("dashboard:calendar:{event_id}"),
                                format!("Calendar: {title}"),
                                "Open Dashboard or Calendar to act on this event",
                            )
                        }),
                );
            snapshots
                .dashboard
                .extend(dashboard.gestures.db.gestures.iter().map(|gesture| {
                    dashboard_status_candidate(
                        format!("dashboard:gesture:{}", gesture.tokens),
                        format!("Gesture: {}", gesture.label),
                        "Open Mouse Gesture Settings to edit this gesture",
                    )
                }));
            if let Some(status) = dashboard.system_status.as_ref() {
                snapshots.dashboard.push(dashboard_status_candidate(
                    "dashboard:system-status".into(),
                    format!(
                        "System: CPU {:.0}% · memory {:.0}% · disk {:.0}%",
                        status.cpu_percent, status.mem_percent, status.disk_percent
                    ),
                    "System status is informational",
                ));
            } else {
                snapshots.dashboard.push(dashboard_status_candidate(
                    "dashboard:system-status-unavailable".into(),
                    "System status unavailable".into(),
                    "Dashboard system status has not loaded",
                ));
            }
            if let Some(recycle) = dashboard.recycle_bin.as_ref() {
                snapshots.dashboard.push(dashboard_status_candidate(
                    "dashboard:recycle-bin".into(),
                    format!("Recycle Bin: {} item(s)", recycle.items),
                    "Open Dashboard or Recycle Bin commands to manage these items",
                ));
            } else {
                snapshots.dashboard.push(dashboard_status_candidate(
                    "dashboard:recycle-bin-unavailable".into(),
                    "Recycle Bin status unavailable".into(),
                    "Dashboard recycle-bin status has not loaded",
                ));
            }
            snapshots.dashboard.extend(
                dashboard
                    .processes
                    .iter()
                    .map(|action| resolver.resolve(action, &resolver_context))
                    .map(|entry| dynamic_candidate_with_window_identity(&entry, &window_catalog)),
            );
            if let Some(action) = self
                .command_cache
                .iter()
                .find(|action| action.action == "dashboard:settings")
            {
                let mut candidate = dynamic_candidate(&resolver.resolve(action, &resolver_context));
                candidate.kind = FrozenEntryKind::Manage;
                snapshots.dashboard.push(candidate);
            }
            if !self.dashboard_enabled {
                snapshots.dashboard_state = DynamicSourceState::Unavailable {
                    reason: "Dashboard is disabled; use Dashboard Settings to enable it".into(),
                };
            }
            for entry in &entries {
                let candidate = dynamic_candidate_with_window_identity(entry, &window_catalog);
                match &entry.target {
                    crate::universal_actions::ActionTarget::Snippet { .. } => {
                        snapshots.snippets.push(candidate)
                    }
                    crate::universal_actions::ActionTarget::Note { .. } => {
                        snapshots.notes.push(candidate)
                    }
                    crate::universal_actions::ActionTarget::Window { .. } => {
                        snapshots.windows.push(candidate)
                    }
                    crate::universal_actions::ActionTarget::MkMacro { .. } => {
                        snapshots.macros.push(candidate)
                    }
                    _ => {}
                }
            }
            for (command, target) in [
                ("fav:dialog:", &mut snapshots.favorites),
                ("mkmacro:dialog", &mut snapshots.macros),
                ("note:dialog", &mut snapshots.notes),
                ("snippet:dialog", &mut snapshots.snippets),
                ("clipboard:dialog", &mut snapshots.clipboard),
            ] {
                if let Some(entry) = entries
                    .iter()
                    .find(|entry| entry.selected_action.action == command)
                {
                    let mut candidate = dynamic_candidate(entry);
                    candidate.kind = FrozenEntryKind::Manage;
                    target.push(candidate);
                }
            }
            for (query, resolved) in &query_entries {
                snapshots.launcher_queries.insert(
                    query.clone(),
                    resolved
                        .iter()
                        .map(|entry| dynamic_candidate_with_window_identity(entry, &window_catalog))
                        .collect(),
                );
            }
            snapshots.launcher_results = captured_result_entries
                .iter()
                .map(|entry| dynamic_candidate_with_window_identity(entry, &window_catalog))
                .collect();
            for cell in menu.rings.iter().flat_map(|ring| &ring.cells) {
                match &cell.content {
                    CellContent::Action { binding } => {
                        let resolved = binding_resolver.resolve(
                            binding,
                            &request.context,
                            &request.invocation_query,
                        );
                        if let Err(reason) = &resolved {
                            unavailable.insert(cell.id.clone(), reason.clone());
                        }
                        static_cells.insert(
                            cell.id.clone(),
                            prepared_cell(
                                binding,
                                resolved,
                                &request.context,
                                effective_after_action(&request.document, menu, cell.after_action),
                                &request.invocation_query,
                            ),
                        );
                    }
                    CellContent::Dynamic { source } => {
                        dynamic.insert(
                            cell.id.clone(),
                            snapshots.freeze(source, Some(&request.invocation_query)),
                        );
                    }
                    _ => {}
                }
            }
            for (id, binding) in [
                ("__center", menu.center_action.as_ref()),
                ("__background", menu.background_action.as_ref()),
            ] {
                if let Some(binding) = binding {
                    let resolved = binding_resolver.resolve(
                        binding,
                        &request.context,
                        &request.invocation_query,
                    );
                    static_cells.insert(
                        crate::radial::model::CellId::new(id),
                        prepared_cell(
                            binding,
                            resolved,
                            &request.context,
                            effective_after_action(
                                &request.document,
                                menu,
                                if id == "__center" {
                                    menu.center_primary_after_action
                                } else {
                                    menu.background_primary_after_action
                                },
                            ),
                            &request.invocation_query,
                        ),
                    );
                }
            }
        }
        let menu = request
            .document
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .cloned()
            .unwrap_or_else(|| request.document.menus[0].clone());
        prepare_dynamic_bindings(
            &mut dynamic,
            &binding_resolver,
            &request.context,
            confirmation_required,
        );
        let effective_style = crate::radial::skin::compile_menu_tree(&request.document, &menu).ok();
        let mut frame = project_menu_frame_with_style(
            &menu,
            static_cells.clone(),
            &dynamic,
            0,
            effective_style.as_ref(),
        );
        for (id, binding) in [
            ("__center", menu.center_secondary_action.as_ref()),
            ("__background", menu.background_secondary_action.as_ref()),
        ] {
            if let Some(binding) = binding {
                let resolved = binding_resolver.resolve(binding, &request.context, &captured_query);
                let policy = if id == "__center" {
                    menu.center_secondary_after_action
                } else {
                    menu.background_secondary_after_action
                };
                frame.alternates.insert(
                    (
                        crate::radial::model::CellId::new(id),
                        crate::radial::model::ClickGesture::Secondary,
                    ),
                    prepared_cell(
                        binding,
                        resolved,
                        &request.context,
                        effective_after_action(&request.document, &menu, policy),
                        &captured_query,
                    ),
                );
            }
        }
        for cell in menu.rings.iter().flat_map(|ring| &ring.cells) {
            for alternate in &cell.alternate_clicks {
                let resolved = binding_resolver.resolve(
                    &alternate.action,
                    &request.context,
                    &request.invocation_query,
                );
                frame.alternates.insert(
                    (cell.id.clone(), alternate.gesture),
                    prepared_cell(
                        &alternate.action,
                        resolved,
                        &request.context,
                        effective_after_action(
                            &request.document,
                            &menu,
                            if alternate.gesture == crate::radial::model::ClickGesture::Secondary
                                && alternate.after_action
                                    == crate::radial::model::AfterActionPolicy::Inherit
                            {
                                cell.secondary_after_action
                            } else {
                                alternate.after_action
                            },
                        ),
                        &request.invocation_query,
                    ),
                );
            }
        }
        for prepared in frame.cells.values_mut() {
            if prepared.after_action == crate::radial::model::AfterActionPolicy::Inherit {
                prepared.after_action = effective_after_action(
                    &request.document,
                    &menu,
                    crate::radial::model::AfterActionPolicy::Inherit,
                );
            }
            if prepared.availability == FrozenAvailability::Available {
                match binding_resolver.resolve_frozen(
                    &prepared.binding,
                    &request.context,
                    &prepared.history_query,
                ) {
                    Ok(binding) => {
                        prepared.requirement = self
                            .radial_interaction_requirement(&binding.action, binding.requirement)
                    }
                    Err(reason) => {
                        prepared.availability = FrozenAvailability::Unavailable {
                            reason: format!("{reason:?}"),
                        }
                    }
                }
            }
            reject_deferred_confirmation_keep_open(prepared, confirmation_required);
            apply_keep_open_compatibility(prepared);
        }
        finalize_alternates(
            &mut frame,
            &binding_resolver,
            &request.context,
            confirmation_required,
        );
        sync_frame_static_cells(&mut frame);
        static_cells.clone_from(&frame.static_cells);
        let mut frames = BTreeMap::new();
        frames.insert(menu_id.clone(), frame.clone());
        for child in request
            .document
            .menus
            .iter()
            .filter(|candidate| candidate.id != menu_id)
        {
            let mut child_static = BTreeMap::new();
            let mut child_dynamic = BTreeMap::new();
            for cell in child.rings.iter().flat_map(|ring| &ring.cells) {
                match &cell.content {
                    CellContent::Action { binding } => {
                        let resolved = binding_resolver.resolve(
                            binding,
                            &request.context,
                            &request.invocation_query,
                        );
                        child_static.insert(
                            cell.id.clone(),
                            prepared_cell(
                                binding,
                                resolved,
                                &request.context,
                                effective_after_action(&request.document, child, cell.after_action),
                                &request.invocation_query,
                            ),
                        );
                    }
                    CellContent::Dynamic { source } => {
                        child_dynamic.insert(
                            cell.id.clone(),
                            snapshots.freeze(source, Some(&request.invocation_query)),
                        );
                    }
                    _ => {}
                }
            }
            for (id, binding) in [
                ("__center", child.center_action.as_ref()),
                ("__background", child.background_action.as_ref()),
            ] {
                if let Some(binding) = binding {
                    let resolved = binding_resolver.resolve(
                        binding,
                        &request.context,
                        &request.invocation_query,
                    );
                    let policy = if id == "__center" {
                        child.center_primary_after_action
                    } else {
                        child.background_primary_after_action
                    };
                    child_static.insert(
                        crate::radial::model::CellId::new(id),
                        prepared_cell(
                            binding,
                            resolved,
                            &request.context,
                            effective_after_action(&request.document, child, policy),
                            &request.invocation_query,
                        ),
                    );
                }
            }
            prepare_dynamic_bindings(
                &mut child_dynamic,
                &binding_resolver,
                &request.context,
                confirmation_required,
            );
            let effective_style =
                crate::radial::skin::compile_menu_tree(&request.document, child).ok();
            let mut child_frame = project_menu_frame_with_style(
                child,
                child_static,
                &child_dynamic,
                0,
                effective_style.as_ref(),
            );
            for (id, binding) in [
                ("__center", child.center_secondary_action.as_ref()),
                ("__background", child.background_secondary_action.as_ref()),
            ] {
                if let Some(binding) = binding {
                    let resolved =
                        binding_resolver.resolve(binding, &request.context, &captured_query);
                    let policy = if id == "__center" {
                        child.center_secondary_after_action
                    } else {
                        child.background_secondary_after_action
                    };
                    child_frame.alternates.insert(
                        (
                            crate::radial::model::CellId::new(id),
                            crate::radial::model::ClickGesture::Secondary,
                        ),
                        prepared_cell(
                            binding,
                            resolved,
                            &request.context,
                            effective_after_action(&request.document, child, policy),
                            &captured_query,
                        ),
                    );
                }
            }
            for cell in child.rings.iter().flat_map(|ring| &ring.cells) {
                for alternate in &cell.alternate_clicks {
                    let resolved = binding_resolver.resolve(
                        &alternate.action,
                        &request.context,
                        &request.invocation_query,
                    );
                    child_frame.alternates.insert(
                        (cell.id.clone(), alternate.gesture),
                        prepared_cell(
                            &alternate.action,
                            resolved,
                            &request.context,
                            effective_after_action(
                                &request.document,
                                child,
                                if alternate.gesture
                                    == crate::radial::model::ClickGesture::Secondary
                                    && alternate.after_action
                                        == crate::radial::model::AfterActionPolicy::Inherit
                                {
                                    cell.secondary_after_action
                                } else {
                                    alternate.after_action
                                },
                            ),
                            &request.invocation_query,
                        ),
                    );
                }
            }
            for prepared in child_frame.cells.values_mut() {
                if prepared.after_action == crate::radial::model::AfterActionPolicy::Inherit {
                    prepared.after_action = effective_after_action(
                        &request.document,
                        child,
                        crate::radial::model::AfterActionPolicy::Inherit,
                    );
                }
                if prepared.availability == FrozenAvailability::Available {
                    if let Ok(binding) = binding_resolver.resolve_frozen(
                        &prepared.binding,
                        &request.context,
                        &prepared.history_query,
                    ) {
                        prepared.requirement = radial_requirement_for_settings(
                            &binding.action,
                            binding.requirement,
                            confirmation_required,
                        );
                    }
                }
                reject_deferred_confirmation_keep_open(prepared, confirmation_required);
                apply_keep_open_compatibility(prepared);
            }
            finalize_alternates(
                &mut child_frame,
                &binding_resolver,
                &request.context,
                confirmation_required,
            );
            sync_frame_static_cells(&mut child_frame);
            frames.insert(child.id.clone(), child_frame);
        }
        let reply_sent = envelope.reply.send(RadialPrepareReply {
            generation: request.generation,
            invocation_id: request.invocation_id,
            menu_id,
            unavailable,
            dynamic,
            frame,
            static_cells,
            frames,
        });
        if reply_sent.is_ok() {
            crate::radial::authoring::trace_runtime_preparation(
                request.invocation_id,
                request.generation,
                crate::radial::authoring::RuntimePreparationTraceState::ReplyQueued,
            );
            let _ = envelope.wake.send(());
        }
    }

    pub(super) fn resolve_deferred_radial(&mut self, envelope: DeferredResolutionEnvelope) {
        if !self.deferred_resolution_is_current(&envelope)
            || envelope.cancellation.load(Ordering::Acquire)
        {
            self.reply_deferred_resolution(envelope, DeferredResolutionResult::Cancelled);
            return;
        }
        if envelope.force_open_query.is_some() {
            let result = self.resolve_deferred_binding(&envelope);
            self.reply_deferred_resolution(envelope, result);
            return;
        }
        let query = match &envelope.binding {
            FrozenBinding::Deferred {
                binding: crate::radial::model::ActionBinding::LauncherQuery { query, mode },
                ..
            } if *mode == crate::radial::model::QueryRunMode::ExecuteFirst => query.clone(),
            _ => {
                let result = self.resolve_deferred_binding(&envelope);
                self.reply_deferred_resolution(envelope, result);
                return;
            }
        };
        let provider_revision = self.plugins.search_generation();
        if envelope.wait_for_provider_change
            && envelope.last_provider_revision == Some(provider_revision)
        {
            self.reply_deferred_resolution(
                envelope,
                DeferredResolutionResult::Pending {
                    provider_revision,
                    wait_for_change: true,
                },
            );
            return;
        }
        let Some(permit) = self.radial_provider_search_capacity.try_acquire() else {
            self.reply_deferred_resolution(
                envelope,
                DeferredResolutionResult::Failed(
                    "A previous launcher provider search is still running".into(),
                ),
            );
            return;
        };

        let snapshot = self.plugins.search_snapshot(
            self.enabled_plugins.as_ref(),
            self.enabled_capabilities.as_ref(),
        );
        let event_tx = self.event_tx.clone();
        let repaint = self.egui_ctx.clone();
        let failed_envelope = envelope.clone();
        let spawn = std::thread::Builder::new()
            .name("radial-query-provider-search".into())
            .spawn(move || {
                let _permit = permit;
                if envelope.cancellation.load(Ordering::Acquire) {
                    return;
                }
                let searched = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    snapshot.search(&query)
                }));
                if envelope.cancellation.load(Ordering::Acquire) {
                    return;
                }
                match searched {
                    Ok(result) => {
                        if event_tx
                            .send(crate::gui::WatchEvent::RadialDeferredSearchReady {
                                envelope,
                                result,
                            })
                            .is_ok()
                        {
                            repaint.request_repaint();
                        }
                    }
                    Err(_) => {
                        let identity = envelope.identity.clone();
                        if envelope
                            .reply
                            .send(DeferredResolutionReply {
                                identity,
                                attempt: envelope.attempt,
                                result: DeferredResolutionResult::Failed(
                                    "A launcher search provider failed while resolving this saved query".into(),
                                ),
                            })
                            .is_ok()
                        {
                            let _ = envelope.wake.send(());
                        }
                    }
                }
            });
        if let Err(error) = spawn {
            self.reply_deferred_resolution(
                failed_envelope,
                DeferredResolutionResult::Failed(format!(
                    "Could not start bounded launcher provider search: {error}"
                )),
            );
        }
    }

    pub(super) fn complete_deferred_radial_search(
        &mut self,
        envelope: DeferredResolutionEnvelope,
        result: crate::plugin::PluginSearchSnapshotResult,
    ) {
        if envelope.cancellation.load(Ordering::Acquire)
            || !self.deferred_resolution_is_current(&envelope)
        {
            self.reply_deferred_resolution(envelope, DeferredResolutionResult::Cancelled);
            return;
        }
        let Some((query, mode)) = (match &envelope.binding {
            FrozenBinding::Deferred {
                binding: crate::radial::model::ActionBinding::LauncherQuery { query, mode },
                ..
            } => Some((query.clone(), *mode)),
            _ => None,
        }) else {
            self.reply_deferred_resolution(
                envelope,
                DeferredResolutionResult::Failed(
                    "Provider result no longer has a saved launcher query".into(),
                ),
            );
            return;
        };
        let query_digest = radial_acceptance_digest(&[query.as_str()]);
        let start_revision = result.start_revision;
        let provider_revision = result.provider_revision;
        let result_catalog_versions = result.catalog_versions;
        let catalog_versions_stable = result.catalog_versions_at_start == result_catalog_versions;
        let outcome = self.search_read_only_outcome_with_plugin_snapshot(&query, result);
        let result_count = outcome.actions.len();
        let result_digest = outcome
            .actions
            .iter()
            .fold(0xcbf29ce484222325, |digest, action| {
                let action_digest = radial_acceptance_action_digest(action);
                (digest ^ action_digest).wrapping_mul(0x100000001b3)
            });
        let selected_digest = outcome.actions.first().map(radial_acceptance_action_digest);
        let pending = outcome.state == super::search::LauncherSearchState::Pending;
        let resolution = if start_revision != provider_revision {
            DeferredResolutionResult::Pending {
                provider_revision,
                wait_for_change: false,
            }
        } else if !catalog_versions_stable
            || result_catalog_versions
                != crate::radial::dynamic::MutableResultCatalogVersions::current()
        {
            DeferredResolutionResult::Pending {
                provider_revision,
                wait_for_change: false,
            }
        } else if outcome.state == super::search::LauncherSearchState::Pending {
            DeferredResolutionResult::Pending {
                provider_revision: outcome.provider_revision,
                wait_for_change: true,
            }
        } else if let Some(selected) = outcome.actions.into_iter().next() {
            match self.freeze_search_result(&selected, &query, Some(result_catalog_versions)) {
                Ok((binding, requirement)) => {
                    DeferredResolutionResult::Ready(ResolvedDeferredSelection {
                        binding,
                        requirement,
                        origin: DeferredDispatchOrigin::Query {
                            query,
                            mode,
                            selected_action: Some(selected),
                            provider_revision: Some(provider_revision),
                            result_catalog_versions: Some(result_catalog_versions),
                            explanation: None,
                        },
                    })
                }
                Err(reason) => open_query_selection(query, mode, reason, Some(provider_revision)),
            }
        } else {
            open_query_selection(
                query.clone(),
                mode,
                format!("No launcher result is currently available for {query:?}"),
                Some(provider_revision),
            )
        };
        if crate::radial::acceptance_trace::enabled() {
            let (state, requirement) = match &resolution {
                DeferredResolutionResult::Ready(selection) => (
                    if matches!(
                        selection.origin,
                        DeferredDispatchOrigin::Query {
                            selected_action: None,
                            ..
                        }
                    ) {
                        "query_fallback"
                    } else {
                        "ready"
                    },
                    radial_acceptance_requirement_name(selection.requirement),
                ),
                DeferredResolutionResult::Pending { .. } => ("pending", "deferred"),
                DeferredResolutionResult::Cancelled => ("cancelled", "none"),
                DeferredResolutionResult::Failed(_) => ("failed", "none"),
            };
            let identity = &envelope.identity;
            crate::radial::acceptance_trace::emit(
                crate::radial::acceptance_trace::Event::RadialQueryResolution {
                    invocation_id: identity.invocation_id.0,
                    session_digest: radial_acceptance_id_digest(identity.session_id.as_str()),
                    cell_digest: radial_acceptance_id_digest(&identity.selected_cell_id),
                    session_generation: identity.session_generation,
                    config_revision: identity.config_revision.0,
                    preparation_generation: identity.preparation_generation.0,
                    query_digest,
                    mode: match mode {
                        crate::radial::model::QueryRunMode::OpenLauncher => "open_launcher",
                        crate::radial::model::QueryRunMode::ExecuteFirst => "execute_first",
                    },
                    state: if pending { "pending" } else { state },
                    provider_revision: Some(provider_revision),
                    result_count,
                    result_digest,
                    selected_digest,
                    interaction_requirement: requirement,
                },
            );
        }
        self.reply_deferred_resolution(envelope, resolution);
    }

    fn deferred_resolution_is_current(&self, envelope: &DeferredResolutionEnvelope) -> bool {
        let identity = &envelope.identity;
        self.radial_feature_settings.enabled
            && self.radial_preparations.get(&identity.invocation_id)
                == Some(&(identity.preparation_generation, identity.config_revision))
            && self.radial_current_preparation
                == Some((
                    identity.invocation_id,
                    identity.preparation_generation,
                    identity.config_revision,
                ))
    }

    fn reply_deferred_resolution(
        &self,
        envelope: DeferredResolutionEnvelope,
        result: DeferredResolutionResult,
    ) {
        if envelope
            .reply
            .send(DeferredResolutionReply {
                identity: envelope.identity,
                attempt: envelope.attempt,
                result,
            })
            .is_ok()
        {
            let _ = envelope.wake.send(());
        }
    }

    fn resolve_deferred_binding(
        &self,
        envelope: &DeferredResolutionEnvelope,
    ) -> DeferredResolutionResult {
        if let Some(reason) = envelope.force_open_query.as_ref() {
            let Some(query) = deferred_query_from_binding(&envelope.binding) else {
                return DeferredResolutionResult::Failed(reason.clone());
            };
            return open_query_selection(
                query,
                query_mode_from_binding(&envelope.binding)
                    .unwrap_or(crate::radial::model::QueryRunMode::ExecuteFirst),
                reason.clone(),
                None,
            );
        }
        match &envelope.binding {
            FrozenBinding::Deferred {
                binding: crate::radial::model::ActionBinding::LauncherQuery { query, mode },
                ..
            } => match mode {
                crate::radial::model::QueryRunMode::OpenLauncher => {
                    if crate::radial::acceptance_trace::enabled() {
                        let identity = &envelope.identity;
                        crate::radial::acceptance_trace::emit(
                            crate::radial::acceptance_trace::Event::RadialQueryResolution {
                                invocation_id: identity.invocation_id.0,
                                session_digest: radial_acceptance_id_digest(
                                    identity.session_id.as_str(),
                                ),
                                cell_digest: radial_acceptance_id_digest(
                                    &identity.selected_cell_id,
                                ),
                                session_generation: identity.session_generation,
                                config_revision: identity.config_revision.0,
                                preparation_generation: identity.preparation_generation.0,
                                query_digest: radial_acceptance_digest(&[query.as_str()]),
                                mode: "open_launcher",
                                state: "manual_ui",
                                provider_revision: None,
                                result_count: 0,
                                result_digest: 0,
                                selected_digest: None,
                                interaction_requirement: "launcher_ui",
                            },
                        );
                    }
                    open_query_selection(query.clone(), *mode, String::new(), None)
                }
                crate::radial::model::QueryRunMode::ExecuteFirst => {
                    DeferredResolutionResult::Failed(
                        "Deferred first-result search must run through the bounded provider worker"
                            .into(),
                    )
                }
            },
            FrozenBinding::Deferred {
                binding: crate::radial::model::ActionBinding::ExactCommand { command, args: _ },
                kind:
                    DeferredBindingKind::ExactCommand {
                        disposition: ExactCommandDisposition::Invalid,
                        ..
                    },
                ..
            } => DeferredResolutionResult::Failed(format!(
                "Saved exact command could not be parsed: {command}"
            )),
            FrozenBinding::Deferred {
                binding: crate::radial::model::ActionBinding::ExactCommand { command, args },
                kind: DeferredBindingKind::ExactCommand { .. },
                parsed_command: Some(parsed),
            } => {
                let action = crate::actions::Action {
                    label: command.clone(),
                    desc: "Saved exact command".into(),
                    action: command.clone(),
                    args: args.clone(),
                };
                let Ok(reparsed) = crate::commands::parse_action(&action) else {
                    return DeferredResolutionResult::Failed(
                        "Saved exact command is no longer valid".into(),
                    );
                };
                if &reparsed != parsed {
                    return DeferredResolutionResult::Failed(
                        "Saved exact command changed after radial preparation".into(),
                    );
                }
                let resolved = ActionTargetResolver.resolve(
                    &action,
                    &ActionTargetResolverContext::new(
                        &self.folder_aliases,
                        &self.bookmark_aliases,
                        &self.actions[..self.custom_len.min(self.actions.len())],
                    ),
                );
                let registry = UniversalActionRegistry;
                let available = registry.resolve(
                    &resolved,
                    &self.action_resolution_context_for_target(
                        &resolved.target,
                        ActionSurface::RadialMenu,
                        &envelope.history_query,
                    ),
                );
                let exact_primary = available.iter().find(|candidate| {
                    matches!(
                        &candidate.operation,
                        crate::universal_actions::UniversalActionOperation::Command {
                            command,
                            original_action,
                        } if command == parsed && original_action == &action
                    )
                });
                let selected_universal_action = exact_primary.or_else(|| {
                    available.iter().find(|candidate| {
                        candidate.id == crate::universal_actions::action_ids::RESULT_EXECUTE
                    })
                });
                let Some(universal_action) = selected_universal_action else {
                    return DeferredResolutionResult::Failed(
                        "Saved exact command has no safe radial execution route".into(),
                    );
                };
                if let Some(reason) = universal_action.availability.disabled_reason() {
                    return DeferredResolutionResult::Failed(reason.to_owned());
                }
                let identity = runtime_identity_for_target(&resolved.target, &self.plugins, None)
                    .map_err(|reason| reason.to_owned());
                let identity = match identity {
                    Ok(identity) => identity,
                    Err(reason) => return DeferredResolutionResult::Failed(reason),
                };
                if crate::radial::acceptance_trace::enabled() {
                    let request_identity = &envelope.identity;
                    let selected_digest = radial_acceptance_action_digest(&action);
                    crate::radial::acceptance_trace::emit(
                        crate::radial::acceptance_trace::Event::RadialQueryResolution {
                            invocation_id: request_identity.invocation_id.0,
                            session_digest: radial_acceptance_id_digest(
                                request_identity.session_id.as_str(),
                            ),
                            cell_digest: radial_acceptance_id_digest(
                                &request_identity.selected_cell_id,
                            ),
                            session_generation: request_identity.session_generation,
                            config_revision: request_identity.config_revision.0,
                            preparation_generation: request_identity.preparation_generation.0,
                            query_digest: radial_acceptance_digest(&[command.as_str()]),
                            mode: "exact_command",
                            state: "ready",
                            provider_revision: None,
                            result_count: 1,
                            result_digest: selected_digest,
                            selected_digest: Some(selected_digest),
                            interaction_requirement: "resolved",
                        },
                    );
                }
                DeferredResolutionResult::Ready(ResolvedDeferredSelection {
                    binding: runtime_binding(
                        resolved.target,
                        resolved.selected_action,
                        universal_action.id.clone(),
                        identity,
                    ),
                    requirement: self.radial_interaction_requirement(
                        universal_action,
                        interaction_requirement(universal_action),
                    ),
                    origin: DeferredDispatchOrigin::ExactCommand {
                        command: command.clone(),
                        args: args.clone(),
                    },
                })
            }
            FrozenBinding::Deferred { kind, .. } => DeferredResolutionResult::Failed(format!(
                "Deferred radial binding is not executable: {}",
                kind.reason()
            )),
            _ => DeferredResolutionResult::Failed(
                "Radial resolution request did not contain a deferred binding".into(),
            ),
        }
    }

    fn freeze_search_result(
        &self,
        selected: &crate::actions::Action,
        query: &str,
        catalog_versions: Option<crate::radial::dynamic::MutableResultCatalogVersions>,
    ) -> Result<(FrozenBinding, InteractionRequirement), String> {
        let custom_len = self.custom_len.min(self.actions.len());
        let resolver_context = ActionTargetResolverContext::new(
            &self.folder_aliases,
            &self.bookmark_aliases,
            &self.actions[..custom_len],
        );
        let resolved = ActionTargetResolver.resolve(selected, &resolver_context);
        let resolution_context = self.action_resolution_context_for_target(
            &resolved.target,
            ActionSurface::RadialMenu,
            query,
        );
        let actions = UniversalActionRegistry.resolve(&resolved, &resolution_context);
        let parsed = crate::commands::parse_action(selected).ok();
        let universal_action = actions
            .iter()
            .find(|candidate| {
                matches!(
                    (&candidate.operation, parsed.as_ref()),
                    (
                        crate::universal_actions::UniversalActionOperation::Command {
                            command,
                            original_action,
                        },
                        Some(parsed),
                    ) if command == parsed && original_action == selected
                )
            })
            .or_else(|| {
                actions.iter().find(|action| {
                    action.id == crate::universal_actions::action_ids::RESULT_EXECUTE
                })
            })
            .ok_or_else(|| {
                "The first launcher result has no executable primary action".to_owned()
            })?;
        if let Some(reason) = universal_action.availability.disabled_reason() {
            return Err(reason.to_owned());
        }
        let identity =
            runtime_identity_for_target(&resolved.target, &self.plugins, catalog_versions)?;
        Ok((
            runtime_binding(
                resolved.target,
                resolved.selected_action,
                universal_action.id.clone(),
                identity,
            ),
            self.radial_interaction_requirement(
                &universal_action,
                interaction_requirement(&universal_action),
            ),
        ))
    }
    /// Resolve a stable radial binding against the catalogs that are current
    /// at dispatch time. This intentionally rebuilds exact identities; stored
    /// list indices are never used as a dispatch address.
    pub(super) fn execute_radial_dispatch(&mut self, request: RadialDispatchRequest) {
        let lease = self
            .radial_preparations
            .get(&request.identity.invocation_id);
        if lease
            != Some(&(
                request.identity.preparation_generation,
                request.identity.config_revision,
            ))
            || self.radial_current_preparation
                != Some((
                    request.identity.invocation_id,
                    request.identity.preparation_generation,
                    request.identity.config_revision,
                ))
            || self.radial_consumed_dispatches.contains(&request.identity)
        {
            self.report_error_message(
                "radial_action",
                "Rejected stale or duplicate radial dispatch lease",
            );
            return;
        }
        self.radial_consumed_dispatches
            .push_back(request.identity.clone());
        while self.radial_consumed_dispatches.len() > RADIAL_DISPATCH_TOMBSTONE_LIMIT {
            self.radial_consumed_dispatches.pop_front();
        }
        if let Some(DeferredDispatchOrigin::Query {
            query,
            mode: crate::radial::model::QueryRunMode::ExecuteFirst,
            selected_action: Some(_),
            provider_revision,
            result_catalog_versions,
            ..
        }) = request.deferred_origin.as_ref()
            && !self.deferred_query_result_is_current(*provider_revision, *result_catalog_versions)
        {
            self.fallback_deferred_query_or_report(
                &request,
                query,
                "The first result changed before dispatch; opening the saved query instead",
            );
            return;
        }
        if let Some(DeferredDispatchOrigin::Query {
            query,
            explanation: Some(explanation),
            ..
        }) = request.deferred_origin.as_ref()
        {
            self.report_error_message(
                "radial_query",
                format!("{explanation}; opening the saved query"),
            );
            let _ = query;
        }
        match self.resolve_radial_action(&request) {
            Ok(prepared) => {
                if prepared.requirement != request.requirement
                    || prepared.requirement
                        != self.radial_interaction_requirement(
                            &prepared.action,
                            interaction_requirement(&prepared.action),
                        )
                {
                    self.report_error_message(
                        "radial_action",
                        "Radial action interaction requirements changed before dispatch",
                    );
                    return;
                }
                let stable_request = match &request.binding {
                    FrozenBinding::Stable(crate::radial::model::ActionBinding::Persisted {
                        action,
                    }) => Some(action.clone()),
                    _ => None,
                };
                let explicit_root_mutation = match &prepared.action.operation {
                    crate::universal_actions::UniversalActionOperation::Command {
                        command, ..
                    } => explicit_launcher_root_mutation(command),
                    crate::universal_actions::UniversalActionOperation::InvokePrimary(action) => {
                        crate::commands::parse_action(action)
                            .is_ok_and(|command| explicit_launcher_root_mutation(&command))
                    }
                    crate::universal_actions::UniversalActionOperation::UiIntent(_) => false,
                };
                let root_policy = if explicit_root_mutation
                    || matches!(
                        prepared.requirement,
                        InteractionRequirement::LauncherUi
                            | InteractionRequirement::ExclusiveCapture
                    ) {
                    RootLauncherPolicy::Legacy
                } else {
                    RootLauncherPolicy::PreserveOrdinaryState
                };
                let fallback_query = match request.deferred_origin.as_ref() {
                    Some(DeferredDispatchOrigin::Query {
                        query,
                        selected_action: None,
                        explanation: Some(_),
                        ..
                    }) => Some(query.clone()),
                    _ => None,
                };
                if let Some(query) = fallback_query.as_ref() {
                    // Failed/empty deferred resolution already attempted the bounded
                    // provider path. Reopening its query must not synchronously invoke
                    // that provider again from the GUI dispatch path.
                    self.radial_suppressed_provider_query = Some(query.clone());
                }
                let request_identity = request.identity.clone();
                let trace_requirement = prepared.requirement;
                let trace_dispatch = match request.deferred_origin.as_ref() {
                    Some(DeferredDispatchOrigin::Query {
                        query,
                        mode,
                        selected_action: Some(selected),
                        ..
                    }) => Some((
                        match mode {
                            crate::radial::model::QueryRunMode::OpenLauncher => "open_launcher",
                            crate::radial::model::QueryRunMode::ExecuteFirst => "execute_first",
                        },
                        query.clone(),
                        radial_acceptance_action_digest(selected),
                    )),
                    Some(DeferredDispatchOrigin::Query {
                        query,
                        mode,
                        selected_action: None,
                        ..
                    }) => Some((
                        match mode {
                            crate::radial::model::QueryRunMode::OpenLauncher => "open_launcher",
                            crate::radial::model::QueryRunMode::ExecuteFirst => "execute_first",
                        },
                        query.clone(),
                        0,
                    )),
                    Some(DeferredDispatchOrigin::ExactCommand { command, args }) => Some((
                        "exact_command",
                        command.clone(),
                        radial_acceptance_digest(&[
                            command.as_str(),
                            "Saved exact command",
                            command.as_str(),
                            args.as_deref().unwrap_or_default(),
                        ]),
                    )),
                    None if matches!(
                        &request.binding,
                        FrozenBinding::Stable(
                            crate::radial::model::ActionBinding::Persisted { action }
                        ) if action.action_id == crate::universal_actions::action_ids::RESULT_EXECUTE
                    ) =>
                    {
                        let selected_digest = match &prepared.action.operation {
                            crate::universal_actions::UniversalActionOperation::InvokePrimary(
                                action,
                            ) => radial_acceptance_action_digest(action),
                            crate::universal_actions::UniversalActionOperation::Command {
                                original_action,
                                ..
                            } => radial_acceptance_action_digest(original_action),
                            crate::universal_actions::UniversalActionOperation::UiIntent(_) => 0,
                        };
                        Some((
                            "pinned_action",
                            request.history_query.clone(),
                            selected_digest,
                        ))
                    }
                    _ => None,
                };
                if crate::radial::acceptance_trace::enabled()
                    && request.deferred_origin.is_none()
                    && let Some(("pinned_action", query, selected_digest)) = trace_dispatch.as_ref()
                {
                    crate::radial::acceptance_trace::emit(
                        crate::radial::acceptance_trace::Event::RadialQueryResolution {
                            invocation_id: request.identity.invocation_id.0,
                            session_digest: radial_acceptance_id_digest(
                                request.identity.session_id.as_str(),
                            ),
                            cell_digest: radial_acceptance_id_digest(
                                &request.identity.selected_cell_id,
                            ),
                            session_generation: request.identity.session_generation,
                            config_revision: request.identity.config_revision.0,
                            preparation_generation: request.identity.preparation_generation.0,
                            query_digest: radial_acceptance_digest(&[query.as_str()]),
                            mode: "pinned_action",
                            state: "ready",
                            provider_revision: None,
                            result_count: 1,
                            result_digest: *selected_digest,
                            selected_digest: Some(*selected_digest),
                            interaction_requirement: radial_acceptance_requirement_name(
                                prepared.requirement,
                            ),
                        },
                    );
                }
                let selected_action = match &prepared.action.operation {
                    crate::universal_actions::UniversalActionOperation::Command {
                        original_action,
                        ..
                    }
                    | crate::universal_actions::UniversalActionOperation::InvokePrimary(
                        original_action,
                    ) => Some(original_action),
                    crate::universal_actions::UniversalActionOperation::UiIntent(_) => None,
                };
                if let Some(selected_action) = selected_action {
                    let query = trace_dispatch
                        .as_ref()
                        .map(|(_, query, _)| query.as_str())
                        .unwrap_or(request.history_query.as_str());
                    self.radial_query_observation.bind_selection(
                        &request.identity,
                        radial_acceptance_digest(&[query]),
                        &request.history_query,
                        selected_action,
                        request.source,
                    );
                }
                let execution = self.execute_universal_action_with_context(
                    prepared.action,
                    UniversalActionInvocationContext {
                        surface: ActionSurface::RadialMenu,
                        source: request.source,
                        stable_request,
                        history_query: request.history_query.clone(),
                        root_policy,
                        primary_invocation: request.deferred_origin.is_some(),
                    },
                    Some(request),
                );
                if crate::radial::acceptance_trace::enabled()
                    && let Some((mode, query, selected_digest)) = trace_dispatch
                {
                    let outcome = match execution {
                        super::universal_action_executor::UniversalActionExecution::Executed => {
                            "executed"
                        }
                        super::universal_action_executor::UniversalActionExecution::ConfirmationRequired => {
                            "confirmation_required"
                        }
                        super::universal_action_executor::UniversalActionExecution::Unavailable => {
                            "unavailable"
                        }
                    };
                    crate::radial::acceptance_trace::emit(
                        crate::radial::acceptance_trace::Event::RadialQueryDispatch {
                            invocation_id: request_identity.invocation_id.0,
                            session_digest: radial_acceptance_id_digest(
                                request_identity.session_id.as_str(),
                            ),
                            cell_digest: radial_acceptance_id_digest(
                                &request_identity.selected_cell_id,
                            ),
                            session_generation: request_identity.session_generation,
                            config_revision: request_identity.config_revision.0,
                            mode,
                            query_digest: radial_acceptance_digest(&[query.as_str()]),
                            selected_digest,
                            interaction_requirement: radial_acceptance_requirement_name(
                                trace_requirement,
                            ),
                            root_policy: if root_policy == RootLauncherPolicy::Legacy {
                                "legacy"
                            } else {
                                "preserve"
                            },
                            outcome,
                        },
                    );
                }
                if let Some(query) = fallback_query
                    && self.radial_suppressed_provider_query.as_deref() == Some(query.as_str())
                {
                    self.radial_suppressed_provider_query = None;
                }
            }
            Err(reason) => {
                if let Some(DeferredDispatchOrigin::Query {
                    query,
                    mode: crate::radial::model::QueryRunMode::ExecuteFirst,
                    ..
                }) = request.deferred_origin.as_ref()
                {
                    self.fallback_deferred_query_or_report(
                        &request,
                        query,
                        &format!("The selected result is no longer available ({reason:?})"),
                    );
                } else {
                    self.report_error_message(
                        "radial_action",
                        format!("Radial action is no longer available: {reason:?}"),
                    );
                }
            }
        }
    }

    pub(super) fn resolve_radial_action(
        &self,
        request: &RadialDispatchRequest,
    ) -> Result<PreparedBinding, BindingUnavailable> {
        if request.deferred_origin.is_some() {
            return self.resolve_deferred_runtime_action(request);
        }
        if let Some(action_id) = frozen_window_action_id(&request.binding) {
            let window_catalog = &self.plugins.internal_services().window_catalog;
            if !frozen_window_identity_is_current(&request.binding, window_catalog) {
                return Err(BindingUnavailable::ContextActionMissing {
                    action_id: action_id.clone(),
                });
            }
        }
        let snapshot = self.universal_action_catalog_snapshot();
        let catalog = snapshot.persisted_catalog();
        let registry = UniversalActionRegistry;
        let mut prepared = RadialBindingResolver {
            catalog: &catalog,
            registry: &registry,
        }
        .resolve_frozen(&request.binding, &request.context, &request.history_query)?;
        prepared.requirement =
            self.radial_interaction_requirement(&prepared.action, prepared.requirement);
        Ok(prepared)
    }

    fn resolve_deferred_runtime_action(
        &self,
        request: &RadialDispatchRequest,
    ) -> Result<PreparedBinding, BindingUnavailable> {
        let FrozenBinding::Runtime {
            target,
            selected_action,
            action_id,
            identity,
        } = &request.binding
        else {
            return Err(BindingUnavailable::Informational);
        };
        let current_result_is_valid = match request.deferred_origin.as_ref() {
            Some(DeferredDispatchOrigin::ExactCommand { command, args }) => {
                selected_action.action == *command && selected_action.args == *args
            }
            Some(DeferredDispatchOrigin::Query {
                selected_action: Some(expected),
                ..
            }) => expected == selected_action,
            Some(DeferredDispatchOrigin::Query {
                query,
                selected_action: None,
                ..
            }) => selected_action.action == format!("query:{query}"),
            None => false,
        };
        if !current_result_is_valid {
            return Err(BindingUnavailable::Informational);
        }
        let custom_len = self.custom_len.min(self.actions.len());
        let resolver_context = ActionTargetResolverContext::new(
            &self.folder_aliases,
            &self.bookmark_aliases,
            &self.actions[..custom_len],
        );
        let resolved = ActionTargetResolver.resolve(selected_action, &resolver_context);
        if &resolved.target != target
            || !runtime_identity_is_current(target, identity.as_ref(), &self.plugins)
        {
            return Err(BindingUnavailable::ContextActionMissing {
                action_id: action_id.clone(),
            });
        }
        let resolution_context = self.action_resolution_context_for_target(
            target,
            ActionSurface::RadialMenu,
            &request.history_query,
        );
        let action = UniversalActionRegistry
            .resolve(&resolved, &resolution_context)
            .into_iter()
            .find(|action| &action.id == action_id)
            .ok_or_else(|| BindingUnavailable::ContextActionMissing {
                action_id: action_id.clone(),
            })?;
        Ok(PreparedBinding {
            binding: crate::radial::model::ActionBinding::Contextual {
                selector: crate::radial::model::TargetSelector::CapturedForeground,
                action_id: action_id.clone(),
            },
            requirement: self
                .radial_interaction_requirement(&action, interaction_requirement(&action)),
            action,
        })
    }

    fn action_resolution_context_for_target<'a>(
        &self,
        target: &crate::universal_actions::ActionTarget,
        surface: ActionSurface,
        query: &'a str,
    ) -> crate::universal_actions::ActionResolutionContext<'a> {
        let mut context = crate::universal_actions::ActionResolutionContext::new(surface, query);
        match target {
            crate::universal_actions::ActionTarget::Timer { id } => {
                context.timer_paused = crate::plugins::timer::timer_paused(*id);
            }
            crate::universal_actions::ActionTarget::Stopwatch { id } => {
                context.stopwatch_paused = crate::plugins::stopwatch::stopwatch_paused(*id);
            }
            _ => {}
        }
        context
    }

    pub(super) fn deferred_query_result_is_current(
        &self,
        provider_revision: Option<u64>,
        catalog_versions: Option<crate::radial::dynamic::MutableResultCatalogVersions>,
    ) -> bool {
        provider_revision.is_some_and(|revision| revision == self.plugins.search_generation())
            && catalog_versions.is_some_and(|versions| {
                versions == crate::radial::dynamic::MutableResultCatalogVersions::current()
            })
    }

    pub(super) fn fallback_deferred_query_or_report(
        &mut self,
        request: &RadialDispatchRequest,
        query: &str,
        reason: &str,
    ) {
        if request.after_action == crate::radial::model::AfterActionPolicy::CloseTree {
            self.open_deferred_query_fallback(query, request.source, reason);
        } else {
            self.report_error_message(
                "radial_query",
                format!(
                    "{reason}; the radial surface remained open, so the query was not opened over it"
                ),
            );
        }
    }

    pub(super) fn open_deferred_query_fallback(
        &mut self,
        query: &str,
        source: ActivationSource,
        reason: &str,
    ) {
        self.report_error_message("radial_query", reason);
        let action = crate::actions::Action {
            label: query.into(),
            desc: "Saved launcher query".into(),
            action: format!("query:{query}"),
            args: None,
        };
        let Ok(invocation) = crate::commands::parse_command(action, None, source) else {
            self.report_error_message("radial_query", "Could not reopen the saved query");
            return;
        };
        let previous_policy =
            std::mem::replace(&mut self.command_root_policy, RootLauncherPolicy::Legacy);
        self.last_results_valid = false;
        self.radial_suppressed_provider_query = Some(query.to_owned());
        self.dispatch_command_invocation_with_history(invocation, Some(query));
        // If the query command reused an already-valid local result set,
        // `search()` may have returned through its cache fast path. Clear any
        // unconsumed suppression so later user-authored edits use normal
        // provider search policy.
        if self.radial_suppressed_provider_query.as_deref() == Some(query) {
            self.radial_suppressed_provider_query = None;
        }
        self.command_root_policy = previous_policy;
    }
}

fn frozen_window_action_id(binding: &FrozenBinding) -> Option<&crate::universal_actions::ActionId> {
    match binding {
        FrozenBinding::Contextual { action_id, .. }
        | FrozenBinding::Stable(crate::radial::model::ActionBinding::Contextual {
            action_id,
            ..
        })
        | FrozenBinding::Runtime {
            target: crate::universal_actions::ActionTarget::Window { .. },
            action_id,
            ..
        } => Some(action_id),
        _ => None,
    }
}

fn runtime_binding(
    target: crate::universal_actions::ActionTarget,
    selected_action: crate::actions::Action,
    action_id: crate::universal_actions::ActionId,
    identity: Option<crate::radial::dynamic::RuntimeTargetIdentity>,
) -> FrozenBinding {
    FrozenBinding::Runtime {
        target,
        selected_action,
        action_id,
        identity,
    }
}

fn runtime_identity_for_target(
    target: &crate::universal_actions::ActionTarget,
    plugins: &crate::plugin::PluginManager,
    result_catalog_versions: Option<crate::radial::dynamic::MutableResultCatalogVersions>,
) -> Result<Option<crate::radial::dynamic::RuntimeTargetIdentity>, &'static str> {
    let catalog_versions = result_catalog_versions
        .unwrap_or_else(crate::radial::dynamic::MutableResultCatalogVersions::current);
    match target {
        crate::universal_actions::ActionTarget::Window { hwnd } => {
            let hwnd = usize::try_from(*hwnd)
                .map_err(|_| "The first launcher result has an invalid window target")?;
            let catalog = &plugins.internal_services().window_catalog;
            let window = catalog
                .describe_current(hwnd)
                .ok_or("The first window result is no longer available")?;
            Ok(Some(
                crate::radial::dynamic::RuntimeTargetIdentity::Window {
                    target: crate::window_catalog::WindowTargetIdentity::from_descriptor(&window),
                    catalog_generation: catalog.generation(),
                },
            ))
        }
        crate::universal_actions::ActionTarget::ClipboardEntry { .. } => Ok(Some(
            crate::radial::dynamic::RuntimeTargetIdentity::ClipboardEntry {
                catalog_version: catalog_versions.clipboard,
            },
        )),
        crate::universal_actions::ActionTarget::Todo { .. } => {
            Ok(Some(crate::radial::dynamic::RuntimeTargetIdentity::Todo {
                catalog_version: catalog_versions.todo,
            }))
        }
        crate::universal_actions::ActionTarget::Note { .. } => {
            Ok(Some(crate::radial::dynamic::RuntimeTargetIdentity::Note {
                catalog_version: catalog_versions.notes,
            }))
        }
        _ => Ok(None),
    }
}

fn explicit_launcher_root_mutation(command: &crate::commands::Command) -> bool {
    matches!(
        command,
        crate::commands::Command::Launcher(_) | crate::commands::Command::Query(_)
    )
}

fn deferred_query_from_binding(binding: &FrozenBinding) -> Option<String> {
    match binding {
        FrozenBinding::Deferred {
            binding: crate::radial::model::ActionBinding::LauncherQuery { query, .. },
            ..
        } => Some(query.clone()),
        _ => None,
    }
}

fn query_mode_from_binding(binding: &FrozenBinding) -> Option<crate::radial::model::QueryRunMode> {
    match binding {
        FrozenBinding::Deferred {
            binding: crate::radial::model::ActionBinding::LauncherQuery { mode, .. },
            ..
        } => Some(*mode),
        _ => None,
    }
}

fn open_query_selection(
    query: String,
    mode: crate::radial::model::QueryRunMode,
    explanation: String,
    provider_revision: Option<u64>,
) -> DeferredResolutionResult {
    let action = crate::actions::Action {
        label: query.clone(),
        desc: "Saved launcher query".into(),
        action: format!("query:{query}"),
        args: None,
    };
    DeferredResolutionResult::Ready(ResolvedDeferredSelection {
        binding: runtime_binding(
            crate::universal_actions::ActionTarget::Generic {
                action: action.clone(),
            },
            action,
            crate::universal_actions::action_ids::RESULT_EXECUTE.clone(),
            None,
        ),
        requirement: InteractionRequirement::LauncherUi,
        origin: DeferredDispatchOrigin::Query {
            query,
            mode,
            selected_action: None,
            provider_revision,
            result_catalog_versions: None,
            explanation: (!explanation.is_empty()).then_some(explanation),
        },
    })
}

fn frozen_window_identity_is_current(
    binding: &FrozenBinding,
    window_catalog: &crate::window_catalog::WindowCatalog,
) -> bool {
    match binding {
        FrozenBinding::Contextual { identity, .. } => window_catalog
            .describe_current(identity.hwnd)
            .is_some_and(|window| identity.matches(&window)),
        FrozenBinding::Stable(crate::radial::model::ActionBinding::Contextual { .. }) => false,
        FrozenBinding::Runtime {
            target: crate::universal_actions::ActionTarget::Window { hwnd },
            identity: Some(crate::radial::dynamic::RuntimeTargetIdentity::Window { target, .. }),
            ..
        } => usize::try_from(*hwnd).is_ok_and(|hwnd| {
            hwnd == target.hwnd
                && window_catalog
                    .describe_current(hwnd)
                    .is_some_and(|window| target.matches(&window))
        }),
        FrozenBinding::Runtime {
            target: crate::universal_actions::ActionTarget::Window { .. },
            identity: None,
            ..
        } => false,
        FrozenBinding::Runtime {
            identity: Some(crate::radial::dynamic::RuntimeTargetIdentity::Window { .. }),
            ..
        } => false,
        _ => true,
    }
}

fn runtime_identity_is_current(
    target: &crate::universal_actions::ActionTarget,
    identity: Option<&crate::radial::dynamic::RuntimeTargetIdentity>,
    plugins: &crate::plugin::PluginManager,
) -> bool {
    use crate::radial::dynamic::RuntimeTargetIdentity as Identity;
    match (target, identity) {
        (
            crate::universal_actions::ActionTarget::Window { hwnd },
            Some(Identity::Window {
                target: identity, ..
            }),
        ) => usize::try_from(*hwnd).is_ok_and(|hwnd| {
            hwnd == identity.hwnd
                && plugins
                    .internal_services()
                    .window_catalog
                    .describe_current(hwnd)
                    .is_some_and(|window| identity.matches(&window))
        }),
        (
            crate::universal_actions::ActionTarget::ClipboardEntry { .. },
            Some(Identity::ClipboardEntry { catalog_version }),
        ) => *catalog_version == crate::plugins::clipboard::clipboard_version(),
        (
            crate::universal_actions::ActionTarget::Todo { .. },
            Some(Identity::Todo { catalog_version }),
        ) => *catalog_version == crate::plugins::todo::todo_version(),
        (
            crate::universal_actions::ActionTarget::Note { .. },
            Some(Identity::Note { catalog_version }),
        ) => *catalog_version == crate::plugins::note::note_version(),
        (
            crate::universal_actions::ActionTarget::Window { .. }
            | crate::universal_actions::ActionTarget::ClipboardEntry { .. }
            | crate::universal_actions::ActionTarget::Todo { .. }
            | crate::universal_actions::ActionTarget::Note { .. },
            _,
        ) => false,
        (_, Some(_)) => false,
        (_, None) => true,
    }
}

fn prepare_dynamic_bindings(
    dynamic: &mut BTreeMap<
        crate::radial::model::CellId,
        crate::radial::dynamic::FrozenDynamicFrame,
    >,
    resolver: &RadialBindingResolver<'_>,
    context: &InvocationContext,
    confirmation_required: bool,
) {
    for frame in dynamic.values_mut() {
        for entry in &mut frame.entries {
            let Some(binding) = entry.binding.as_ref() else {
                continue;
            };
            match resolver.resolve_frozen(binding, context, &entry.history_query) {
                Ok(prepared) => {
                    entry.requirement = radial_requirement_for_settings(
                        &prepared.action,
                        prepared.requirement,
                        confirmation_required,
                    )
                }
                Err(reason) => {
                    entry.availability = FrozenAvailability::Unavailable {
                        reason: format!("{reason:?}"),
                    }
                }
            }
        }
    }
}

fn prepared_cell(
    binding: &crate::radial::model::ActionBinding,
    resolved: Result<PreparedBinding, BindingUnavailable>,
    invocation: &InvocationContext,
    after_action: crate::radial::model::AfterActionPolicy,
    history_query: &str,
) -> PreparedCell {
    if let Some(deferred) = prepare_deferred_binding(binding) {
        return PreparedCell {
            binding: deferred.binding,
            availability: deferred.availability,
            requirement: deferred.requirement,
            after_action,
            history_query: history_query.to_owned(),
        };
    }
    let frozen = freeze_action_binding(binding, invocation);
    let unavailable = resolved
        .as_ref()
        .err()
        .map(|reason| format!("{reason:?}"))
        .or_else(|| frozen.as_ref().err().map(|reason| format!("{reason:?}")));
    let is_available = unavailable.is_none();
    PreparedCell {
        binding: frozen.unwrap_or(FrozenBinding::Informational),
        availability: unavailable.map_or(FrozenAvailability::Available, |reason| {
            FrozenAvailability::Unavailable { reason }
        }),
        requirement: if is_available {
            resolved
                .as_ref()
                .map_or(InteractionRequirement::None, |prepared| {
                    prepared.requirement
                })
        } else {
            InteractionRequirement::None
        },
        after_action,
        history_query: history_query.to_owned(),
    }
}

fn freeze_action_binding(
    binding: &crate::radial::model::ActionBinding,
    invocation: &InvocationContext,
) -> Result<FrozenBinding, BindingUnavailable> {
    match binding {
        crate::radial::model::ActionBinding::Persisted { .. } => {
            Ok(FrozenBinding::Stable(binding.clone()))
        }
        crate::radial::model::ActionBinding::LauncherQuery { .. }
        | crate::radial::model::ActionBinding::ExactCommand { .. } => {
            prepare_deferred_binding(binding)
                .map(|deferred| deferred.binding)
                .ok_or(BindingUnavailable::Informational)
        }
        crate::radial::model::ActionBinding::Contextual {
            selector,
            action_id,
        } => {
            let window = match selector {
                crate::radial::model::TargetSelector::CapturedForeground => {
                    invocation.foreground.as_ref()
                }
                crate::radial::model::TargetSelector::UnderPointer => {
                    invocation.under_pointer.as_ref()
                }
                crate::radial::model::TargetSelector::LastExternal => {
                    invocation.last_external.as_ref()
                }
            }
            .ok_or_else(|| BindingUnavailable::ContextTargetMissing {
                selector: selector.clone(),
            })?;
            Ok(FrozenBinding::Contextual {
                selector: selector.clone(),
                action_id: action_id.clone(),
                identity: crate::window_catalog::WindowTargetIdentity {
                    hwnd: window.hwnd,
                    pid: window.pid,
                    executable: window.process_name.clone(),
                    process_path: window.process_path.clone(),
                    class_name: window.class_name.clone(),
                },
            })
        }
    }
}

fn finalize_alternates(
    frame: &mut crate::radial::bindings::PreparedMenuFrame,
    resolver: &RadialBindingResolver<'_>,
    context: &InvocationContext,
    confirmation_required: bool,
) {
    for prepared in frame.alternates.values_mut() {
        if prepared.availability == FrozenAvailability::Available {
            match resolver.resolve_frozen(&prepared.binding, context, &prepared.history_query) {
                Ok(binding) => {
                    prepared.requirement = radial_requirement_for_settings(
                        &binding.action,
                        binding.requirement,
                        confirmation_required,
                    )
                }
                Err(reason) => {
                    prepared.availability = FrozenAvailability::Unavailable {
                        reason: format!("{reason:?}"),
                    };
                }
            }
        }
        reject_deferred_confirmation_keep_open(prepared, confirmation_required);
        apply_keep_open_compatibility(prepared);
    }
}

fn radial_requirement_for_settings(
    action: &crate::universal_actions::UniversalAction,
    requirement: InteractionRequirement,
    confirmation_required: bool,
) -> InteractionRequirement {
    if requirement == InteractionRequirement::None
        && confirmation_required
        && DestructiveAction::from_radial_operation(action).is_some()
    {
        InteractionRequirement::Confirmation
    } else {
        requirement
    }
}

fn sync_frame_static_cells(frame: &mut crate::radial::bindings::PreparedMenuFrame) {
    for (cell_id, prepared) in &mut frame.static_cells {
        if let Some(updated) = frame.cells.get(cell_id) {
            *prepared = updated.clone();
        }
    }
}

fn apply_keep_open_compatibility(prepared: &mut PreparedCell) {
    if prepared.availability == FrozenAvailability::Available
        && prepared.after_action == crate::radial::model::AfterActionPolicy::KeepOpen
        && prepared.requirement != InteractionRequirement::None
    {
        prepared.availability = FrozenAvailability::Unavailable {
            reason: format!("KeepOpen is incompatible with {:?}", prepared.requirement),
        };
    }
}

fn reject_deferred_confirmation_keep_open(
    prepared: &mut PreparedCell,
    confirmation_required: bool,
) {
    if !confirmation_required
        || prepared.after_action != crate::radial::model::AfterActionPolicy::KeepOpen
        || !matches!(prepared.availability, FrozenAvailability::Deferred { .. })
    {
        return;
    }
    let FrozenBinding::Deferred {
        parsed_command: Some(command),
        ..
    } = &prepared.binding
    else {
        return;
    };
    if DestructiveAction::from_command(command).is_some() {
        prepared.availability = FrozenAvailability::Unavailable {
            reason: "KeepOpen is incompatible with Confirmation".into(),
        };
        prepared.requirement = InteractionRequirement::Confirmation;
    }
}

fn effective_after_action(
    document: &crate::radial::model::RadialDocument,
    menu: &crate::radial::model::MenuDefinition,
    cell: crate::radial::model::AfterActionPolicy,
) -> crate::radial::model::AfterActionPolicy {
    crate::radial::model::effective_after_action(document, menu, cell)
}

fn dynamic_candidate(entry: &crate::universal_actions::ResolvedActionTarget) -> DynamicCandidate {
    let stable = entry.target.persistent_ref();
    let action_id = match entry.target {
        crate::universal_actions::ActionTarget::Window { .. } => {
            crate::universal_actions::action_ids::WINDOW_ACTIVATE
        }
        crate::universal_actions::ActionTarget::ClipboardEntry { .. } => {
            crate::universal_actions::action_ids::RESULT_EXECUTE
        }
        crate::universal_actions::ActionTarget::MkMacro { .. } => {
            crate::universal_actions::action_ids::MKMACRO_RUN
        }
        crate::universal_actions::ActionTarget::Note { .. } => {
            crate::universal_actions::action_ids::RESULT_EXECUTE
        }
        _ => crate::universal_actions::action_ids::RESULT_EXECUTE,
    };
    DynamicCandidate {
        stable_id: format!(
            "{:?}",
            stable.as_ref().unwrap_or(
                &crate::universal_actions::PersistableActionTargetRef::LegacyAction {
                    action: entry.selected_action.clone()
                }
            )
        ),
        label: entry.selected_action.label.clone(),
        action: stable.clone().map(|target| {
            crate::universal_actions::PersistedUniversalActionRef {
                target: Some(target),
                action_id: action_id.clone(),
            }
        }),
        runtime_action: stable.is_none().then(|| {
            (
                entry.target.clone(),
                entry.selected_action.clone(),
                action_id,
            )
        }),
        runtime_identity: None,
        unavailable_reason: None,
        history_query: None,
        kind: FrozenEntryKind::Action,
    }
}

fn dynamic_candidate_with_window_identity(
    entry: &crate::universal_actions::ResolvedActionTarget,
    window_catalog: &crate::window_catalog::WindowCatalog,
) -> DynamicCandidate {
    let mut candidate = dynamic_candidate(entry);
    if candidate.runtime_action.is_some()
        && let crate::universal_actions::ActionTarget::Window { hwnd } = &entry.target
    {
        if let Ok(hwnd) = usize::try_from(*hwnd)
            && let Some(window) = window_catalog.describe_current(hwnd)
        {
            candidate.runtime_identity =
                Some(crate::radial::dynamic::RuntimeTargetIdentity::Window {
                    target: crate::window_catalog::WindowTargetIdentity::from_descriptor(&window),
                    catalog_generation: window_catalog.generation(),
                });
        } else {
            candidate.unavailable_reason = Some(
                "Window identity could not be captured; refresh the menu before using this item"
                    .into(),
            );
        }
    }
    candidate
}

fn dashboard_status_candidate(
    stable_id: String,
    label: String,
    reason: impl Into<String>,
) -> DynamicCandidate {
    DynamicCandidate {
        stable_id,
        label,
        action: None,
        runtime_action: None,
        runtime_identity: None,
        unavailable_reason: Some(reason.into()),
        history_query: None,
        kind: FrozenEntryKind::Manage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::bindings::BindingUnavailable;
    use crate::radial::context::InvocationContext;
    use crate::radial::model::{ActionBinding, QueryRunMode, TargetSelector};
    use crate::universal_actions::ActionId;
    use std::sync::{
        Mutex, RwLock,
        atomic::{AtomicUsize, Ordering as AtomicOrdering},
        mpsc,
    };

    struct MutableSearchPlugin(Arc<RwLock<Vec<crate::actions::Action>>>);

    impl crate::plugin::Plugin for MutableSearchPlugin {
        fn search(&self, _query: &str) -> Vec<crate::actions::Action> {
            self.0.read().unwrap().clone()
        }

        fn name(&self) -> &str {
            "radial_m3_mutable_search"
        }

        fn description(&self) -> &str {
            "mutable radial M3 search fixture"
        }

        fn capabilities(&self) -> &[&str] {
            &["search"]
        }

        fn always_search(&self) -> bool {
            true
        }
    }

    struct CountingSearchPlugin {
        result: crate::actions::Action,
        searches: Arc<AtomicUsize>,
    }

    impl crate::plugin::Plugin for CountingSearchPlugin {
        fn search(&self, _query: &str) -> Vec<crate::actions::Action> {
            self.searches.fetch_add(1, AtomicOrdering::SeqCst);
            vec![self.result.clone()]
        }

        fn name(&self) -> &str {
            "radial_m3_counting_search"
        }

        fn description(&self) -> &str {
            "counting radial M3 search fixture"
        }

        fn capabilities(&self) -> &[&str] {
            &["search"]
        }

        fn always_search(&self) -> bool {
            true
        }
    }

    struct BlockingSearchPlugin {
        started: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
        finished: mpsc::Sender<()>,
        result: crate::actions::Action,
    }

    impl crate::plugin::Plugin for BlockingSearchPlugin {
        fn search(&self, _query: &str) -> Vec<crate::actions::Action> {
            self.started.send(()).unwrap();
            self.release.lock().unwrap().recv().unwrap();
            self.finished.send(()).unwrap();
            vec![self.result.clone()]
        }

        fn name(&self) -> &str {
            "radial_m3_blocking_search"
        }

        fn description(&self) -> &str {
            "channel-gated radial M3 search fixture"
        }

        fn capabilities(&self) -> &[&str] {
            &["search"]
        }

        fn always_search(&self) -> bool {
            true
        }
    }

    fn result_action(label: &str, action: &str) -> crate::actions::Action {
        crate::actions::Action {
            label: label.into(),
            desc: "needle radial result".into(),
            action: action.into(),
            args: None,
        }
    }

    fn register_test_lease(app: &mut LauncherApp, request: &RadialDispatchRequest) {
        app.radial_preparations.insert(
            request.identity.invocation_id,
            (
                request.identity.preparation_generation,
                request.identity.config_revision,
            ),
        );
        app.radial_current_preparation = Some((
            request.identity.invocation_id,
            request.identity.preparation_generation,
            request.identity.config_revision,
        ));
    }

    fn deferred_query_envelope(
        app: &mut LauncherApp,
        query: &str,
    ) -> (
        DeferredResolutionEnvelope,
        mpsc::Receiver<DeferredResolutionReply>,
        mpsc::Receiver<()>,
    ) {
        let request = leased_request();
        register_test_lease(app, &request);
        let prepared = prepare_deferred_binding(&ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::ExecuteFirst,
        })
        .expect("saved query should be prepared as deferred work");
        let (reply, replies) = mpsc::channel();
        let (wake, wakes) = mpsc::channel();
        let envelope = DeferredResolutionEnvelope {
            identity: request.identity.clone(),
            attempt: 1,
            binding: prepared.binding,
            history_query: query.into(),
            last_provider_revision: None,
            wait_for_provider_change: false,
            force_open_query: None,
            cancellation: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            reply,
            wake,
        };
        (envelope, replies, wakes)
    }

    fn plugin_snapshot_result(
        app: &LauncherApp,
        actions: Vec<crate::actions::Action>,
    ) -> crate::plugin::PluginSearchSnapshotResult {
        let revision = app.plugins.search_generation();
        let catalogs = crate::radial::dynamic::MutableResultCatalogVersions::current();
        crate::plugin::PluginSearchSnapshotResult {
            actions,
            pending: false,
            start_revision: revision,
            provider_revision: revision,
            catalog_versions_at_start: catalogs,
            catalog_versions: catalogs,
        }
    }

    fn dispatch_request_for_resolution(
        envelope: &DeferredResolutionEnvelope,
        resolved: ResolvedDeferredSelection,
    ) -> RadialDispatchRequest {
        RadialDispatchRequest {
            identity: envelope.identity.clone(),
            binding: resolved.binding,
            requirement: resolved.requirement,
            deferred_origin: Some(resolved.origin),
            history_query: envelope.history_query.clone(),
            context: InvocationContext::empty(envelope.identity.session_generation),
            after_action: crate::radial::model::AfterActionPolicy::CloseTree,
            source: ActivationSource::Click,
        }
    }

    fn complete_query_resolution(
        app: &mut LauncherApp,
        envelope: DeferredResolutionEnvelope,
        result: crate::plugin::PluginSearchSnapshotResult,
        replies: &mpsc::Receiver<DeferredResolutionReply>,
    ) -> ResolvedDeferredSelection {
        app.complete_deferred_radial_search(envelope, result);
        let reply = replies
            .try_recv()
            .expect("deferred search completion should reply through its envelope");
        let DeferredResolutionResult::Ready(resolved) = reply.result else {
            panic!(
                "expected a ready deferred search result, received {:?}",
                reply.result
            );
        };
        resolved
    }

    struct FallbackSearchPlugin(Arc<std::sync::atomic::AtomicBool>);

    impl crate::plugin::Plugin for FallbackSearchPlugin {
        fn search(&self, _query: &str) -> Vec<crate::actions::Action> {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            vec![crate::actions::Action {
                label: "Provider result".into(),
                desc: "needle".into(),
                action: "fallback:result".into(),
                args: None,
            }]
        }
        fn name(&self) -> &str {
            "fallback_provider"
        }
        fn description(&self) -> &str {
            "fallback provider test"
        }
        fn capabilities(&self) -> &[&str] {
            &["search"]
        }
        fn always_search(&self) -> bool {
            true
        }
    }

    #[test]
    fn deferred_failure_opens_manual_query_without_provider_retry() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        let searched = Arc::new(std::sync::atomic::AtomicBool::new(false));
        app.plugins
            .register(Box::new(FallbackSearchPlugin(Arc::clone(&searched))));
        assert!(!app.radial_provider_search_capacity.is_occupied());

        app.open_deferred_query_fallback("needle", ActivationSource::Click, "provider timed out");

        assert_eq!(app.query, "needle");
        assert!(!searched.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(app.radial_suppressed_provider_query, None);
    }

    #[test]
    fn failed_deferred_dispatch_suppresses_provider_retry_with_free_capacity() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        let searched = Arc::new(std::sync::atomic::AtomicBool::new(false));
        app.plugins
            .register(Box::new(FallbackSearchPlugin(Arc::clone(&searched))));
        assert!(!app.radial_provider_search_capacity.is_occupied());

        let mut request = leased_request();
        request.after_action = crate::radial::model::AfterActionPolicy::CloseTree;
        request.history_query = "needle".into();
        app.radial_preparations.insert(
            request.identity.invocation_id,
            (
                request.identity.preparation_generation,
                request.identity.config_revision,
            ),
        );
        app.radial_current_preparation = Some((
            request.identity.invocation_id,
            request.identity.preparation_generation,
            request.identity.config_revision,
        ));

        // This is the correlated Ready payload produced after a Failed or
        // NoResults deferred reply is converted to an explained manual query.
        let DeferredResolutionResult::Ready(resolved) = open_query_selection(
            "needle".into(),
            QueryRunMode::ExecuteFirst,
            "provider resolution failed; opening the saved query".into(),
            Some(app.plugins.search_generation()),
        ) else {
            panic!("manual fallback should resolve to a launcher query");
        };
        request.binding = resolved.binding;
        request.requirement = resolved.requirement;
        request.deferred_origin = Some(resolved.origin);

        // `execute_radial_dispatch` is the GUI-side endpoint reached only
        // after the controller's close/release handoff has completed.
        app.execute_radial_dispatch(request);

        assert_eq!(app.query, "needle");
        assert!(
            !searched.load(std::sync::atomic::Ordering::SeqCst),
            "manual fallback must not reenter the provider even when capacity is free"
        );
        assert_eq!(app.radial_suppressed_provider_query, None);
    }

    #[test]
    fn deferred_execute_first_uses_the_current_shared_search_order() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        app.usage_weight = 1.0;
        let actions = Arc::new(RwLock::new(vec![result_action("needle A", "help:show")]));
        app.plugins
            .register(Box::new(MutableSearchPlugin(Arc::clone(&actions))));

        let (envelope, replies, wakes) = deferred_query_envelope(&mut app, "needle");
        let authored_binding = envelope.binding.clone();
        let before = app.search_read_only_outcome("needle");
        assert_eq!(before.actions[0].action, "help:show");

        let action_b = result_action("needle B", "query:changed");
        *actions.write().unwrap() = vec![result_action("needle A", "help:show"), action_b.clone()];
        app.usage.insert(action_b.action.clone(), 100);
        let ordinary_search = app.search_read_only_outcome("needle");
        assert_eq!(ordinary_search.actions.first(), Some(&action_b));

        let result = app.plugins.search_snapshot(None, None).search("needle");
        let resolved = complete_query_resolution(&mut app, envelope, result, &replies);
        assert_eq!(
            wakes.try_recv(),
            Ok(()),
            "completion should wake the controller after queuing its reply"
        );
        assert_eq!(
            authored_binding,
            prepare_deferred_binding(&ActionBinding::LauncherQuery {
                query: "needle".into(),
                mode: QueryRunMode::ExecuteFirst,
            })
            .unwrap()
            .binding
        );
        assert!(matches!(
            resolved.origin,
            DeferredDispatchOrigin::Query {
                query,
                mode: QueryRunMode::ExecuteFirst,
                selected_action: Some(selected),
                ..
            } if query == "needle" && selected == action_b
        ));
    }

    #[test]
    fn unfreezable_first_result_opens_query_without_activating_second_result() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        app.test_skip_history_persistence = true;
        app.usage_weight = 1.0;
        let unfreezable = result_action("needle unavailable window", "window:close:999");
        let valid_second = result_action("needle help", "help:show");
        app.usage.insert(unfreezable.action.clone(), 1_000);
        assert!(
            app.freeze_search_result(
                &unfreezable,
                "needle",
                Some(crate::radial::dynamic::MutableResultCatalogVersions::current()),
            )
            .is_err()
        );

        let (envelope, replies, _) = deferred_query_envelope(&mut app, "needle");
        let result = plugin_snapshot_result(&app, vec![unfreezable.clone(), valid_second.clone()]);
        let ranked = app.search_read_only_outcome_with_plugin_snapshot("needle", result.clone());
        assert_eq!(ranked.actions.first(), Some(&unfreezable));
        assert_eq!(ranked.actions.get(1), Some(&valid_second));

        let resolved = complete_query_resolution(&mut app, envelope.clone(), result, &replies);
        let explanation = match &resolved.origin {
            DeferredDispatchOrigin::Query {
                query,
                mode: QueryRunMode::ExecuteFirst,
                selected_action: None,
                explanation: Some(explanation),
                ..
            } => {
                assert_eq!(query, "needle");
                explanation.clone()
            }
            other => panic!("expected an explained manual-query fallback, got {other:?}"),
        };
        assert!(!explanation.is_empty());
        let request = dispatch_request_for_resolution(&envelope, resolved);
        app.execute_radial_dispatch(request);

        assert_eq!(app.query, "needle");
        assert!(
            !app.help_window.open,
            "the valid second result must not run"
        );
        let [(activated, source)] = app.test_activation_trace.as_slice() else {
            panic!("the manual query fallback should dispatch exactly once");
        };
        assert_eq!(activated.action, "query:needle");
        assert_eq!(*source, ActivationSource::Click);
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(!app.usage.contains_key(&valid_second.action));
    }

    #[test]
    fn deferred_query_command_navigates_without_leaf_execution() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        app.test_skip_history_persistence = true;
        let selected = result_action("Open destination query", "query:destination");
        let (envelope, replies, _) = deferred_query_envelope(&mut app, "destination");
        let result = plugin_snapshot_result(&app, vec![selected.clone()]);
        let resolved = complete_query_resolution(&mut app, envelope.clone(), result, &replies);
        let request = dispatch_request_for_resolution(&envelope, resolved);

        app.execute_radial_dispatch(request);

        assert_eq!(app.query, "destination");
        assert_eq!(
            app.test_activation_trace,
            [(selected, ActivationSource::Click)],
            "navigation should dispatch only the selected query command"
        );
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.usage.is_empty());
    }

    #[test]
    fn deferred_queryexec_cycle_is_bounded_at_the_radial_execution_boundary() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        app.test_skip_history_persistence = true;
        let cyclic = crate::actions::Action {
            label: "Destination recursive query".into(),
            desc: "destination".into(),
            action: "queryexec:destination".into(),
            args: None,
        };
        let searches = Arc::new(AtomicUsize::new(0));
        app.plugins.register(Box::new(CountingSearchPlugin {
            result: cyclic.clone(),
            searches: Arc::clone(&searches),
        }));

        let (envelope, replies, _) = deferred_query_envelope(&mut app, "destination");
        let result = app
            .plugins
            .search_snapshot(None, None)
            .search("destination");
        assert_eq!(searches.load(AtomicOrdering::SeqCst), 1);
        let resolved = complete_query_resolution(&mut app, envelope.clone(), result, &replies);
        let request = dispatch_request_for_resolution(&envelope, resolved);

        app.execute_radial_dispatch(request);

        assert_eq!(searches.load(AtomicOrdering::SeqCst), 2);
        assert_eq!(
            app.test_activation_trace,
            [
                (cyclic.clone(), ActivationSource::Click),
                (cyclic, ActivationSource::Click),
            ],
            "the radial queryexec dispatch and one nested activation are bounded"
        );
        assert!(
            app.error
                .as_deref()
                .is_some_and(|message| message.contains("Nested queryexec stopped"))
        );
        assert!(
            !app.test_activation_trace
                .iter()
                .any(|(action, _)| action.action == "help:show")
        );
    }

    #[test]
    fn cancelled_blocked_provider_keeps_its_permit_until_return_and_emits_no_result() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        app.plugins.register(Box::new(BlockingSearchPlugin {
            started: started_tx,
            release: Mutex::new(release_rx),
            finished: finished_tx,
            result: result_action("Blocked result", "help:show"),
        }));
        let (envelope, replies, _) = deferred_query_envelope(&mut app, "needle");

        app.resolve_deferred_radial(envelope.clone());
        started_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("bounded provider worker should enter the channel-gated plugin");
        assert!(app.radial_provider_search_capacity.is_occupied());

        envelope.cancellation.store(true, Ordering::Release);
        assert!(
            app.radial_provider_search_capacity.is_occupied(),
            "cancelling a blocked provider cannot return its permit before plugin code exits"
        );
        release_tx.send(()).unwrap();
        finished_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("provider should return after its explicit release signal");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while app.radial_provider_search_capacity.is_occupied() {
            assert!(
                std::time::Instant::now() < deadline,
                "provider permit should drop after the worker returns"
            );
            std::thread::yield_now();
        }
        assert!(replies.try_recv().is_err());
        let mut late_search_results = Vec::new();
        while let Ok(event) = app.rx.try_recv() {
            if matches!(
                event,
                crate::gui::WatchEvent::RadialDeferredSearchReady { .. }
            ) {
                late_search_results.push(event);
            }
        }
        assert!(
            late_search_results.is_empty(),
            "cancelled provider results must not reopen or execute the saved query"
        );
    }

    fn leased_request() -> RadialDispatchRequest {
        let identity = crate::radial::handoff::RadialDispatchIdentity {
            session_id: crate::radial::model::SessionId::new("lease-session"),
            selected_cell_id: "lease-cell".into(),
            invocation_id: crate::radial::model::InvocationId(42),
            session_generation: 1,
            token: crate::radial::session::DispatchToken {
                session_generation: 1,
                ordinal: 1,
            },
            config_revision: crate::radial::model::ConfigRevision(7),
            preparation_generation: crate::radial::bindings::PreparationGeneration(9),
        };
        RadialDispatchRequest {
            identity,
            binding: FrozenBinding::Stable(ActionBinding::Persisted {
                action: crate::universal_actions::PersistedUniversalActionRef {
                    target: None,
                    action_id: ActionId::new("missing"),
                },
            }),
            requirement: InteractionRequirement::None,
            deferred_origin: None,
            history_query: "captured".into(),
            context: InvocationContext::empty(1),
            after_action: crate::radial::model::AfterActionPolicy::KeepOpen,
            source: ActivationSource::Enter,
        }
    }

    #[test]
    fn runtime_nonlegacy_keep_open_is_unavailable_before_dispatch() {
        let mut prepared = PreparedCell {
            binding: FrozenBinding::Runtime {
                target: crate::universal_actions::ActionTarget::Window { hwnd: 44 },
                selected_action: crate::actions::Action {
                    label: "Window".into(),
                    desc: "Windows".into(),
                    action: "window:switch:44".into(),
                    args: None,
                },
                action_id: crate::universal_actions::action_ids::WINDOW_ACTIVATE,
                identity: None,
            },
            availability: FrozenAvailability::Available,
            requirement: InteractionRequirement::ExternalInput,
            after_action: crate::radial::model::AfterActionPolicy::KeepOpen,
            history_query: String::new(),
        };
        apply_keep_open_compatibility(&mut prepared);
        assert!(matches!(
            prepared.availability,
            FrozenAvailability::Unavailable { ref reason }
                if reason.contains("ExternalInput") && reason.contains("KeepOpen")
        ));
    }

    #[test]
    fn destructive_exact_keep_open_is_rejected_only_when_confirmation_is_enabled() {
        let binding = ActionBinding::ExactCommand {
            command: "note:remove:__radial_confirmation_test__".into(),
            args: Some("captured-argument".into()),
        };
        let mut prepared = prepared_cell(
            &binding,
            Err(BindingUnavailable::Deferred(
                crate::radial::dynamic::DeferredBindingKind::ExactCommand {
                    disposition: ExactCommandDisposition::Recognized,
                    required_interaction: InteractionRequirement::None,
                },
            )),
            &InvocationContext::empty(1),
            crate::radial::model::AfterActionPolicy::KeepOpen,
            "captured root query",
        );

        reject_deferred_confirmation_keep_open(&mut prepared, false);
        assert!(matches!(
            prepared.availability,
            FrozenAvailability::Deferred { .. }
        ));

        reject_deferred_confirmation_keep_open(&mut prepared, true);
        assert!(matches!(
            prepared.availability,
            FrozenAvailability::Unavailable { ref reason }
                if reason.contains("Confirmation")
        ));
        assert_eq!(prepared.requirement, InteractionRequirement::Confirmation);
    }

    #[test]
    fn saved_query_preparation_stays_deferred_and_keeps_the_invocation_query() {
        let binding = ActionBinding::LauncherQuery {
            query: "saved".into(),
            mode: QueryRunMode::OpenLauncher,
        };
        let prepared = prepared_cell(
            &binding,
            Err(BindingUnavailable::Deferred(
                crate::radial::dynamic::DeferredBindingKind::LauncherQuery {
                    mode: QueryRunMode::OpenLauncher,
                },
            )),
            &InvocationContext::empty(1),
            crate::radial::model::AfterActionPolicy::CloseTree,
            "root query captured at invocation",
        );

        assert!(matches!(
            prepared.binding,
            FrozenBinding::Deferred {
                binding: ActionBinding::LauncherQuery { ref query, .. },
                ..
            } if query == "saved"
        ));
        assert!(matches!(
            prepared.availability,
            FrozenAvailability::Deferred { .. }
        ));
        assert_eq!(prepared.requirement, InteractionRequirement::Deferred);
        assert_eq!(prepared.history_query, "root query captured at invocation");
    }

    fn install_test_window_catalog(app: &mut LauncherApp) {
        let descriptor = crate::window_catalog::WindowDescriptor {
            title: "Editor".into(),
            hwnd: 44,
            pid: 7,
            executable: Some("editor.exe".into()),
            process_path: Some("C:\\Apps\\editor.exe".into()),
            class_name: Some("EditorWindow".into()),
        };
        let live = Arc::new(std::sync::Mutex::new(Some(descriptor.clone())));
        let provider = Arc::clone(&live);
        let catalog = crate::window_catalog::WindowCatalog::from_snapshot_with_descriptor(
            vec![descriptor.clone()],
            move |hwnd| {
                provider
                    .lock()
                    .ok()
                    .and_then(|window| window.clone())
                    .filter(|window| window.hwnd == hwnd)
            },
        );
        app.plugins.set_window_catalog_for_test(catalog);
    }

    fn install_test_window_catalog_with_live_target(
        app: &mut LauncherApp,
    ) -> Arc<Mutex<Option<crate::window_catalog::WindowDescriptor>>> {
        let descriptor = crate::window_catalog::WindowDescriptor {
            title: "Editor".into(),
            hwnd: 44,
            pid: 7,
            executable: Some("editor.exe".into()),
            process_path: Some("C:\\Apps\\editor.exe".into()),
            class_name: Some("EditorWindow".into()),
        };
        let live = Arc::new(Mutex::new(Some(descriptor.clone())));
        let provider = Arc::clone(&live);
        let catalog = crate::window_catalog::WindowCatalog::from_snapshot_with_descriptor(
            vec![descriptor],
            move |hwnd| {
                provider
                    .lock()
                    .ok()
                    .and_then(|window| window.clone())
                    .filter(|window| window.hwnd == hwnd)
            },
        );
        app.plugins.set_window_catalog_for_test(catalog);
        live
    }

    fn frozen_window_query_request(app: &mut LauncherApp, query: &str) -> RadialDispatchRequest {
        app.test_skip_history_persistence = true;
        app.require_confirm_destructive = true;
        let selected = crate::actions::Action {
            label: "Close Editor".into(),
            desc: "Window".into(),
            action: "window:close:44".into(),
            args: None,
        };
        let versions = crate::radial::dynamic::MutableResultCatalogVersions::current();
        let (binding, requirement) = app
            .freeze_search_result(&selected, query, Some(versions))
            .expect("captured window result should be executable");
        assert_eq!(requirement, InteractionRequirement::Confirmation);

        let mut request = leased_request();
        request.binding = binding;
        request.requirement = requirement;
        request.after_action = crate::radial::model::AfterActionPolicy::CloseTree;
        request.history_query = query.into();
        request.deferred_origin = Some(DeferredDispatchOrigin::Query {
            query: query.into(),
            mode: QueryRunMode::ExecuteFirst,
            selected_action: Some(selected),
            provider_revision: Some(app.plugins.search_generation()),
            result_catalog_versions: Some(versions),
            explanation: None,
        });
        register_test_lease(app, &request);
        request
    }

    fn install_window_close_execution_hook(calls: Arc<AtomicUsize>) {
        crate::gui::set_execute_action_hook(Some(Box::new(move |action| {
            assert_eq!(action.action, "window:close:44");
            calls.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(())
        })));
    }

    fn confirm_with_window_close_hook(app: &mut LauncherApp, calls: Arc<AtomicUsize>) {
        install_window_close_execution_hook(calls);
        app.resolve_pending_confirmation(true);
        crate::gui::set_execute_action_hook(None);
    }

    #[test]
    fn first_result_freeze_uses_the_actual_window_primary_command() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        install_test_window_catalog(&mut app);
        app.require_confirm_destructive = false;
        let selected = crate::actions::Action {
            label: "Close Editor".into(),
            desc: "Close this window".into(),
            action: "window:close:44".into(),
            args: None,
        };

        let (binding, requirement) = app.freeze_search_result(&selected, "editor", None).unwrap();
        assert_eq!(requirement, InteractionRequirement::None);
        assert!(matches!(
            binding,
            FrozenBinding::Runtime {
                action_id,
                selected_action,
                ..
            } if action_id == crate::universal_actions::action_ids::WINDOW_CLOSE
                && selected_action.action == "window:close:44"
        ));

        app.require_confirm_destructive = true;
        let (_, requirement) = app
            .freeze_search_result(&selected, "editor", None)
            .expect("window close should resolve under confirmation policy");
        assert_eq!(requirement, InteractionRequirement::Confirmation);
    }

    #[test]
    fn deferred_first_result_expires_when_clipboard_index_order_changes() {
        let ctx = eframe::egui::Context::default();
        let app = crate::gui::actions::tests::new_app(&ctx);
        let target = crate::universal_actions::ActionTarget::ClipboardEntry { index: 0 };
        let captured_versions = crate::radial::dynamic::MutableResultCatalogVersions::current();
        let identity =
            runtime_identity_for_target(&target, &app.plugins, Some(captured_versions)).unwrap();
        assert!(runtime_identity_is_current(
            &target,
            identity.as_ref(),
            &app.plugins
        ));
        assert!(app.deferred_query_result_is_current(
            Some(app.plugins.search_generation()),
            Some(captured_versions)
        ));

        // Re-publish a clipboard history whose index zero now names a new
        // value. The stored command still contains index zero, so its frozen
        // catalog version must prevent it from silently retargeting.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("clipboard-history.json");
        crate::plugins::clipboard::save_history(
            path.to_str().unwrap(),
            &std::collections::VecDeque::from(["new first entry".to_string()]),
        )
        .unwrap();

        assert!(!runtime_identity_is_current(
            &target,
            identity.as_ref(),
            &app.plugins
        ));
        assert!(!app.deferred_query_result_is_current(
            Some(app.plugins.search_generation()),
            Some(captured_versions)
        ));
    }

    #[test]
    fn exact_command_freeze_retains_typed_target_and_separate_args() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        install_test_window_catalog(&mut app);
        let preparation = prepare_deferred_binding(&ActionBinding::ExactCommand {
            command: "window:close:44".into(),
            args: Some("recorded argument".into()),
        })
        .unwrap();
        let (reply, _replies) = std::sync::mpsc::channel();
        let (wake, _wakes) = std::sync::mpsc::channel();
        let envelope = DeferredResolutionEnvelope {
            identity: crate::radial::handoff::RadialDispatchIdentity {
                session_id: crate::radial::model::SessionId::new("exact-command"),
                selected_cell_id: "exact-cell".into(),
                invocation_id: crate::radial::model::InvocationId(3),
                session_generation: 1,
                token: crate::radial::session::DispatchToken {
                    session_generation: 1,
                    ordinal: 1,
                },
                config_revision: crate::radial::model::ConfigRevision(2),
                preparation_generation: crate::radial::bindings::PreparationGeneration(4),
            },
            attempt: 1,
            binding: preparation.binding,
            history_query: "captured query".into(),
            last_provider_revision: None,
            wait_for_provider_change: false,
            force_open_query: None,
            cancellation: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            reply,
            wake,
        };
        let DeferredResolutionResult::Ready(resolved) = app.resolve_deferred_binding(&envelope)
        else {
            panic!("valid recognized exact command should resolve");
        };
        assert_eq!(resolved.requirement, InteractionRequirement::Confirmation);
        assert!(matches!(
            resolved.binding,
            FrozenBinding::Runtime {
                target: crate::universal_actions::ActionTarget::Window { hwnd: 44 },
                action_id,
                selected_action,
                ..
            } if action_id == crate::universal_actions::action_ids::WINDOW_CLOSE
                && selected_action.args.as_deref() == Some("recorded argument")
        ));
    }

    #[test]
    fn radial_root_policy_distinguishes_explicit_launcher_mutations() {
        assert!(explicit_launcher_root_mutation(
            &crate::commands::Command::Launcher(crate::commands::LauncherCommand::Hide)
        ));
        assert!(explicit_launcher_root_mutation(
            &crate::commands::Command::Query(crate::commands::QueryCommand::Set {
                query: "saved".into(),
                argument: None,
            })
        ));
        assert!(!explicit_launcher_root_mutation(
            &crate::commands::Command::VirtualDesktop(
                crate::commands::VirtualDesktopCommand::Create
            )
        ));
    }

    #[test]
    fn confirmed_non_ui_radial_command_preserves_visible_root_with_hide_preferences() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        install_test_window_catalog(&mut app);
        app.test_skip_history_persistence = true;
        app.require_confirm_destructive = true;
        app.hide_after_run = true;
        app.clear_query_after_run = true;
        app.query = "ordinary visible query".into();
        app.visible_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let selected = crate::actions::Action {
            label: "Close Editor".into(),
            desc: "Window".into(),
            action: "window:close:44".into(),
            args: None,
        };
        let (binding, requirement) = app
            .freeze_search_result(&selected, "captured radial query", None)
            .expect("window close should resolve as a radial primary");
        assert_eq!(requirement, InteractionRequirement::Confirmation);

        let mut request = leased_request();
        request.binding = binding;
        request.requirement = requirement;
        request.after_action = crate::radial::model::AfterActionPolicy::CloseTree;
        request.history_query = "captured radial query".into();
        request.deferred_origin = Some(DeferredDispatchOrigin::ExactCommand {
            command: selected.action.clone(),
            args: selected.args.clone(),
        });
        app.radial_preparations.insert(
            request.identity.invocation_id,
            (
                request.identity.preparation_generation,
                request.identity.config_revision,
            ),
        );
        app.radial_current_preparation = Some((
            request.identity.invocation_id,
            request.identity.preparation_generation,
            request.identity.config_revision,
        ));

        app.execute_radial_dispatch(request);
        let pending = app
            .pending_universal_confirm
            .as_ref()
            .expect("the radial action must open its confirmation");
        assert_eq!(
            pending.context.root_policy,
            RootLauncherPolicy::PreserveOrdinaryState,
            "confirmation changes the handoff requirement, not the command's ROOT policy"
        );
        assert_eq!(pending.context.history_query, "captured radial query");
        crate::gui::set_execute_action_hook(Some(Box::new(|_| Ok(()))));
        app.resolve_pending_confirmation(true);
        crate::gui::set_execute_action_hook(None);

        assert_eq!(app.query, "ordinary visible query");
        assert!(app.visible_flag.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(app.test_recorded_history_queries, ["captured radial query"]);
        assert_eq!(app.usage.get("window:close:44"), Some(&1));
    }

    #[test]
    fn radial_query_confirmation_keeps_its_frozen_request_and_executes_once() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        install_test_window_catalog(&mut app);
        app.query = "ordinary ROOT query".into();
        let request = frozen_window_query_request(&mut app, "captured radial query");

        app.execute_radial_dispatch(request.clone());
        let pending = app
            .pending_universal_confirm
            .as_ref()
            .expect("radial query selection should enter confirmation");
        assert_eq!(pending.radial_request.as_ref(), Some(&request));
        assert!(matches!(
            pending.radial_request.as_ref().and_then(|request| request.deferred_origin.as_ref()),
            Some(DeferredDispatchOrigin::Query {
                query,
                selected_action: Some(action),
                ..
            }) if query == "captured radial query" && action.action == "window:close:44"
        ));

        app.execute_radial_dispatch(request.clone());
        assert!(
            app.error
                .as_deref()
                .is_some_and(|message| message.contains("stale or duplicate"))
        );
        let calls = Arc::new(AtomicUsize::new(0));
        confirm_with_window_close_hook(&mut app, Arc::clone(&calls));

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(app.test_recorded_history_queries, ["captured radial query"]);
        assert_eq!(app.usage.get("window:close:44"), Some(&1));
        assert!(!app.resolve_pending_universal_action_confirmation(true));

        app.invalidate_radial_leases();
        app.execute_radial_dispatch(request);
        assert!(app.pending_universal_confirm.is_none());
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(app.test_recorded_history_queries, ["captured radial query"]);
        assert_eq!(app.usage.get("window:close:44"), Some(&1));
        confirm_with_window_close_hook(&mut app, Arc::clone(&calls));
        assert!(app.pending_universal_confirm.is_none());
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(app.test_recorded_history_queries, ["captured radial query"]);
        assert_eq!(app.usage.get("window:close:44"), Some(&1));
    }

    #[test]
    fn changed_provider_revision_before_confirmation_opens_query_without_retargeting() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        install_test_window_catalog(&mut app);
        let request = frozen_window_query_request(&mut app, "captured radial query");
        let captured_provider_revision = app.plugins.search_generation();
        app.execute_radial_dispatch(request);
        assert!(
            app.pending_universal_confirm
                .as_ref()
                .is_some_and(|pending| pending.radial_request.is_some())
        );

        let searches = Arc::new(AtomicUsize::new(0));
        app.plugins.register(Box::new(CountingSearchPlugin {
            result: result_action("A newly ranked result", "help:show"),
            searches: Arc::clone(&searches),
        }));
        app.plugins
            .notify_search_update_for_test("radial_m3_confirmation");
        assert_ne!(
            app.plugins.search_generation(),
            captured_provider_revision,
            "a provider publication should invalidate the captured search revision"
        );
        let calls = Arc::new(AtomicUsize::new(0));
        confirm_with_window_close_hook(&mut app, Arc::clone(&calls));

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(searches.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(app.query, "captured radial query");
        assert!(app
            .error
            .as_deref()
            .is_some_and(|message| message.contains("first result changed before confirmation")));
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(!app.usage.contains_key("window:close:44"));
        assert!(!app.help_window.open);
    }

    #[test]
    fn changed_mutable_catalog_version_before_confirmation_opens_saved_query() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        install_test_window_catalog(&mut app);
        let request = frozen_window_query_request(&mut app, "captured radial query");
        app.execute_radial_dispatch(request);
        let pending = app
            .pending_universal_confirm
            .as_mut()
            .expect("radial request should remain leased through confirmation");
        let captured = match pending
            .radial_request
            .as_ref()
            .and_then(|request| request.deferred_origin.as_ref())
        {
            Some(DeferredDispatchOrigin::Query {
                result_catalog_versions: Some(captured),
                ..
            }) => *captured,
            _ => panic!("pending radial request should retain mutable catalog versions"),
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("clipboard-history.json");
        crate::plugins::clipboard::save_history(
            path.to_str().unwrap(),
            &std::collections::VecDeque::from(["new clipboard entry".to_owned()]),
        )
        .unwrap();
        assert_ne!(
            crate::radial::dynamic::MutableResultCatalogVersions::current(),
            captured,
            "publishing clipboard history should advance a mutable result catalog version"
        );

        let calls = Arc::new(AtomicUsize::new(0));
        confirm_with_window_close_hook(&mut app, Arc::clone(&calls));

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(app.query, "captured radial query");
        assert!(app
            .error
            .as_deref()
            .is_some_and(|message| message.contains("first result changed before confirmation")));
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(!app.usage.contains_key("window:close:44"));
    }

    #[test]
    fn invalidated_query_selection_identity_before_confirmation_opens_saved_query() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        let live_target = install_test_window_catalog_with_live_target(&mut app);
        let request = frozen_window_query_request(&mut app, "captured radial query");
        app.execute_radial_dispatch(request);
        *live_target.lock().unwrap() = None;

        let calls = Arc::new(AtomicUsize::new(0));
        confirm_with_window_close_hook(&mut app, Arc::clone(&calls));

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(app.query, "captured radial query");
        assert!(
            app.error
                .as_deref()
                .is_some_and(|message| message.contains("selected result is no longer available"))
        );
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(!app.usage.contains_key("window:close:44"));
    }

    #[test]
    fn expired_radial_lease_before_confirmation_rejects_without_fallback_execution() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        install_test_window_catalog(&mut app);
        let request = frozen_window_query_request(&mut app, "captured radial query");
        app.execute_radial_dispatch(request.clone());
        app.radial_preparations
            .remove(&request.identity.invocation_id);

        let calls = Arc::new(AtomicUsize::new(0));
        confirm_with_window_close_hook(&mut app, Arc::clone(&calls));

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
        assert!(
            app.error
                .as_deref()
                .is_some_and(|message| message.contains("lease expired before confirmation"))
        );
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(!app.usage.contains_key("window:close:44"));
        assert_ne!(app.query, "captured radial query");
    }

    #[test]
    fn ephemeral_window_favorite_requires_fresh_exact_identity() {
        let entry = crate::universal_actions::ResolvedActionTarget {
            target: crate::universal_actions::ActionTarget::Window { hwnd: 44 },
            selected_action: crate::actions::Action {
                label: "Favorite window".into(),
                desc: "Fav".into(),
                action: "window:switch:44".into(),
                args: None,
            },
            custom_action_index: None,
        };
        let descriptor = crate::window_catalog::WindowDescriptor {
            title: "Favorite window".into(),
            hwnd: 44,
            pid: 7,
            executable: Some("editor.exe".into()),
            process_path: Some("C:\\Apps\\editor.exe".into()),
            class_name: Some("EditorWindow".into()),
        };
        let live = Arc::new(std::sync::Mutex::new(Some(descriptor.clone())));
        let live_provider = Arc::clone(&live);
        let catalog = crate::window_catalog::WindowCatalog::from_snapshot_with_descriptor(
            vec![descriptor.clone()],
            move |hwnd| {
                live_provider
                    .lock()
                    .ok()
                    .and_then(|window| window.clone())
                    .filter(|window| window.hwnd == hwnd)
            },
        );
        let candidate = dynamic_candidate_with_window_identity(&entry, &catalog);
        let frame = DynamicSnapshots {
            favorites: vec![candidate],
            ..Default::default()
        }
        .freeze(&crate::radial::model::DynamicSource::Favorites, None);
        let binding = frame.entries[0].binding.as_ref().unwrap();
        assert!(matches!(
            binding,
            FrozenBinding::Runtime {
                identity: Some(_),
                ..
            }
        ));
        assert!(frozen_window_identity_is_current(binding, &catalog));

        // The published snapshot still contains PID 7, but dispatch must query
        // the current single-HWND descriptor and reject reuse by PID 99.
        *live.lock().unwrap() = Some(crate::window_catalog::WindowDescriptor {
            pid: 99,
            ..descriptor.clone()
        });
        assert!(!frozen_window_identity_is_current(binding, &catalog));
        *live.lock().unwrap() = None;
        assert!(!frozen_window_identity_is_current(binding, &catalog));

        let missing = dynamic_candidate_with_window_identity(&entry, &catalog);
        assert!(missing.runtime_identity.is_none());
        assert!(missing.unavailable_reason.is_some());
        let missing_frame = DynamicSnapshots {
            favorites: vec![missing],
            ..Default::default()
        }
        .freeze(&crate::radial::model::DynamicSource::Favorites, None);
        assert!(matches!(
            missing_frame.entries[0].availability,
            FrozenAvailability::Unavailable { .. }
        ));
        assert!(!frozen_window_identity_is_current(
            missing_frame.entries[0].binding.as_ref().unwrap(),
            &catalog
        ));
    }

    #[test]
    fn missing_context_target_is_typed_before_execution() {
        let binding = ActionBinding::Contextual {
            selector: TargetSelector::UnderPointer,
            action_id: ActionId::new("window.close"),
        };
        let catalog = crate::universal_actions::PersistedActionCatalog::new(vec![]);
        let registry = crate::universal_actions::UniversalActionRegistry;
        assert!(matches!(
            crate::radial::bindings::RadialBindingResolver {
                catalog: &catalog,
                registry: &registry
            }
            .resolve(&binding, &InvocationContext::empty(1), ""),
            Err(BindingUnavailable::ContextTargetMissing {
                selector: TargetSelector::UnderPointer
            })
        ));
    }

    #[test]
    fn contextual_cells_alternates_and_special_surfaces_freeze_exact_identity() {
        let invocation = InvocationContext {
            foreground: Some(crate::radial::context::WindowIdentity {
                hwnd: 44,
                pid: 7,
                process_name: Some("editor.exe".into()),
                process_path: Some("C:\\Apps\\editor.exe".into()),
                class_name: Some("EditorWindow".into()),
                title: "Editor".into(),
            }),
            ..InvocationContext::empty(1)
        };
        let binding = ActionBinding::Contextual {
            selector: TargetSelector::CapturedForeground,
            action_id: crate::universal_actions::action_ids::WINDOW_ACTIVATE,
        };
        let selected_action = crate::actions::Action {
            label: "Editor".into(),
            desc: "Windows".into(),
            action: "window:switch:44".into(),
            args: None,
        };
        let action_catalog = crate::universal_actions::PersistedActionCatalog::new(vec![
            crate::universal_actions::ResolvedActionTarget {
                target: crate::universal_actions::ActionTarget::Window { hwnd: 44 },
                selected_action,
                custom_action_index: None,
            },
        ]);
        let registry = crate::universal_actions::UniversalActionRegistry;
        let resolver = RadialBindingResolver {
            catalog: &action_catalog,
            registry: &registry,
        };
        let descriptor = crate::window_catalog::WindowDescriptor {
            title: "Editor".into(),
            hwnd: 44,
            pid: 7,
            executable: Some("editor.exe".into()),
            process_path: Some("C:\\Apps\\editor.exe".into()),
            class_name: Some("EditorWindow".into()),
        };
        let live = Arc::new(std::sync::Mutex::new(Some(descriptor.clone())));
        let provider = Arc::clone(&live);
        let window_catalog = crate::window_catalog::WindowCatalog::from_snapshot_with_descriptor(
            vec![descriptor.clone()],
            move |hwnd| {
                provider
                    .lock()
                    .ok()
                    .and_then(|window| window.clone())
                    .filter(|window| window.hwnd == hwnd)
            },
        );
        for surface in ["cell", "alternate", "center", "background"] {
            let prepared = prepared_cell(
                &binding,
                resolver.resolve(&binding, &invocation, surface),
                &invocation,
                crate::radial::model::AfterActionPolicy::CloseTree,
                surface,
            );
            assert!(matches!(
                &prepared.binding,
                FrozenBinding::Contextual { identity, .. }
                    if identity.hwnd == 44 && identity.pid == 7
            ));
            assert!(frozen_window_identity_is_current(
                &prepared.binding,
                &window_catalog
            ));
        }
        *live.lock().unwrap() = Some(crate::window_catalog::WindowDescriptor {
            pid: 99,
            ..descriptor
        });
        let frozen = freeze_action_binding(&binding, &invocation).unwrap();
        assert!(!frozen_window_identity_is_current(&frozen, &window_catalog));
    }

    #[test]
    fn watch_route_accepts_an_exact_dispatch_lease_only_once() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        let request = leased_request();
        app.radial_preparations.insert(
            request.identity.invocation_id,
            (
                request.identity.preparation_generation,
                request.identity.config_revision,
            ),
        );
        app.radial_current_preparation = Some((
            request.identity.invocation_id,
            request.identity.preparation_generation,
            request.identity.config_revision,
        ));
        app.event_tx
            .send(crate::gui::WatchEvent::RadialDispatch(request.clone()))
            .unwrap();
        app.process_watch_events();
        assert!(app.radial_consumed_dispatches.contains(&request.identity));
        app.event_tx
            .send(crate::gui::WatchEvent::RadialDispatch(request))
            .unwrap();
        app.process_watch_events();
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("stale or duplicate"))
        );
    }

    #[test]
    fn dispatch_tombstones_and_active_preparations_are_bounded() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        for ordinal in 0..(RADIAL_DISPATCH_TOMBSTONE_LIMIT + 5) {
            let mut request = leased_request();
            request.identity.token.ordinal = ordinal as u64;
            app.radial_consumed_dispatches.push_back(request.identity);
            while app.radial_consumed_dispatches.len() > RADIAL_DISPATCH_TOMBSTONE_LIMIT {
                app.radial_consumed_dispatches.pop_front();
            }
        }
        assert_eq!(
            app.radial_consumed_dispatches.len(),
            RADIAL_DISPATCH_TOMBSTONE_LIMIT
        );
        app.radial_preparations.insert(
            crate::radial::model::InvocationId(1),
            (
                crate::radial::bindings::PreparationGeneration(1),
                crate::radial::model::ConfigRevision(1),
            ),
        );
        app.radial_preparations.clear();
        app.radial_preparations.insert(
            crate::radial::model::InvocationId(2),
            (
                crate::radial::bindings::PreparationGeneration(2),
                crate::radial::model::ConfigRevision(2),
            ),
        );
        assert_eq!(app.radial_preparations.len(), 1);
    }

    #[test]
    fn production_prepare_uses_full_application_catalog_beyond_custom_prefix() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        app.actions = std::sync::Arc::new(vec![
            crate::actions::Action {
                label: "Custom".into(),
                desc: "Custom".into(),
                action: "custom:first".into(),
                args: None,
            },
            crate::actions::Action {
                label: "Installed app".into(),
                desc: "Application".into(),
                action: "C:\\Apps\\installed.exe".into(),
                args: None,
            },
        ]);
        app.custom_len = 1;
        let document = std::sync::Arc::new(crate::radial::model::RadialDocument::starter());
        let (applications_menu, applications) = document
            .menus
            .iter()
            .find_map(|menu| {
                menu.rings
                    .iter()
                    .flat_map(|ring| &ring.cells)
                    .find_map(|cell| {
                        matches!(
                            cell.content,
                            crate::radial::model::CellContent::Dynamic {
                                source: crate::radial::model::DynamicSource::Applications
                            }
                        )
                        .then(|| (menu.id.clone(), cell.id.clone()))
                    })
            })
            .unwrap();
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        let (wake_tx, _wake_rx) = std::sync::mpsc::channel();
        app.prepare_radial(crate::radial::bindings::RadialPrepareEnvelope {
            request: crate::radial::bindings::RadialPrepareRequest {
                generation: crate::radial::bindings::PreparationGeneration(1),
                invocation_id: crate::radial::model::InvocationId(1),
                requested_menu_id: applications_menu,
                document,
                context: InvocationContext::empty(1),
                invocation_query: String::new(),
                allow_context_rules: false,
            },
            reply: reply_tx,
            wake: wake_tx,
        });
        let reply = reply_rx.recv().unwrap();
        let frame = reply.dynamic.get(&applications).unwrap();
        assert!(
            frame
                .entries
                .iter()
                .any(|entry| entry.label == "Installed app")
        );
    }

    #[test]
    fn production_prepare_materializes_favorites_and_complete_dashboard_status_families() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        app.dashboard_data_cache
            .set_snapshot_for_test(crate::dashboard::DashboardDataSnapshot {
                favorites: std::sync::Arc::new(vec![crate::plugins::fav::FavEntry {
                    label: "Favorite window".into(),
                    action: "window:switch:44".into(),
                    args: None,
                }]),
                clipboard_history: std::sync::Arc::new(vec!["Clipboard sample".into()]),
                system_status: Some(crate::dashboard::data_cache::SystemStatusSnapshot::default()),
                recycle_bin: Some(crate::dashboard::data_cache::RecycleBinSnapshot {
                    size_bytes: 10,
                    items: 1,
                }),
                ..Default::default()
            });
        let document = std::sync::Arc::new(crate::radial::model::RadialDocument::starter());
        for (source, expected) in [
            (
                crate::radial::model::DynamicSource::Favorites,
                vec!["Favorite window"],
            ),
            (
                crate::radial::model::DynamicSource::Dashboard,
                vec![
                    "Favorite window",
                    "Clipboard sample",
                    "System",
                    "Recycle Bin",
                ],
            ),
        ] {
            let (menu_id, cell_id) = document
                .menus
                .iter()
                .find_map(|menu| {
                    menu.rings.iter().flat_map(|ring| &ring.cells).find_map(|cell| {
                        matches!(&cell.content, CellContent::Dynamic { source: candidate } if candidate == &source)
                            .then(|| (menu.id.clone(), cell.id.clone()))
                    })
                })
                .unwrap();
            let (reply_tx, reply_rx) = std::sync::mpsc::channel();
            let (wake_tx, _wake_rx) = std::sync::mpsc::channel();
            app.prepare_radial(crate::radial::bindings::RadialPrepareEnvelope {
                request: crate::radial::bindings::RadialPrepareRequest {
                    generation: crate::radial::bindings::PreparationGeneration(1),
                    invocation_id: crate::radial::model::InvocationId(77),
                    requested_menu_id: menu_id,
                    document: document.clone(),
                    context: InvocationContext::empty(77),
                    invocation_query: String::new(),
                    allow_context_rules: false,
                },
                reply: reply_tx,
                wake: wake_tx,
            });
            let frame = reply_rx.recv().unwrap().dynamic.remove(&cell_id).unwrap();
            let labels = frame
                .entries
                .iter()
                .map(|entry| entry.label.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                expected.iter().all(|needle| labels.contains(needle)),
                "{source:?} lacked a truthful production row: {labels}"
            );
        }
    }
}
