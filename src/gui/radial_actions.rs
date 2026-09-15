use crate::radial::bindings::RadialBindingResolver;
use crate::radial::bindings::{
    BindingUnavailable, PreparedBinding, PreparedCell, RadialPrepareEnvelope, RadialPrepareReply,
    project_menu_frame_with_style,
};
use crate::radial::context::{CompiledContextRules, InvocationContext};
use crate::radial::dynamic::{
    DynamicCandidate, DynamicSnapshots, DynamicSourceState, FrozenAvailability, FrozenBinding,
    FrozenEntryKind,
};
use crate::radial::handoff::{
    InteractionRequirement, RadialDispatchRequest, interaction_requirement,
};
use crate::radial::model::CellContent;
use crate::universal_actions::{
    ActionSurface, ActionTargetResolver, ActionTargetResolverContext, PersistedActionCatalog,
    RootLauncherPolicy, UniversalActionInvocationContext, UniversalActionRegistry,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::{ActivationSource, LauncherApp};

const RADIAL_DISPATCH_TOMBSTONE_LIMIT: usize = 64;

impl LauncherApp {
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
        prepare_dynamic_bindings(&mut dynamic, &binding_resolver, &request.context);
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
                    Ok(binding) => prepared.requirement = binding.requirement,
                    Err(reason) => {
                        prepared.availability = FrozenAvailability::Unavailable {
                            reason: format!("{reason:?}"),
                        }
                    }
                }
            }
            apply_keep_open_compatibility(prepared);
        }
        finalize_alternates(&mut frame, &binding_resolver, &request.context);
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
            prepare_dynamic_bindings(&mut child_dynamic, &binding_resolver, &request.context);
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
                        prepared.requirement = binding.requirement;
                    }
                }
                apply_keep_open_compatibility(prepared);
            }
            finalize_alternates(&mut child_frame, &binding_resolver, &request.context);
            frames.insert(child.id.clone(), child_frame);
        }
        let _ = envelope.reply.send(RadialPrepareReply {
            generation: request.generation,
            invocation_id: request.invocation_id,
            menu_id,
            unavailable,
            dynamic,
            frame,
            static_cells,
            frames,
        });
        let _ = envelope.wake.send(());
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
        match self.resolve_radial_action(&request) {
            Ok(prepared) => {
                if prepared.requirement != request.requirement
                    || prepared.requirement != interaction_requirement(&prepared.action)
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
                let root_policy = if matches!(
                    prepared.requirement,
                    InteractionRequirement::LauncherUi | InteractionRequirement::ExclusiveCapture
                ) {
                    RootLauncherPolicy::Legacy
                } else {
                    RootLauncherPolicy::PreserveOrdinaryState
                };
                self.execute_universal_action_with_context(
                    prepared.action,
                    UniversalActionInvocationContext {
                        surface: ActionSurface::RadialMenu,
                        source: request.source,
                        stable_request,
                        history_query: request.history_query.clone(),
                        root_policy,
                    },
                    Some(request),
                );
            }
            Err(reason) => self.report_error_message(
                "radial_action",
                format!("Radial action is no longer available: {reason:?}"),
            ),
        }
    }

    pub(super) fn resolve_radial_action(
        &self,
        request: &RadialDispatchRequest,
    ) -> Result<PreparedBinding, BindingUnavailable> {
        if let Some(action_id) = frozen_window_action_id(&request.binding) {
            let window_catalog = &self.plugins.internal_services().window_catalog;
            if !frozen_window_identity_is_current(&request.binding, window_catalog) {
                return Err(BindingUnavailable::ContextActionMissing {
                    action_id: action_id.clone(),
                });
            }
        }
        let catalog = self.universal_action_catalog_snapshot().persisted_catalog();
        let registry = UniversalActionRegistry;
        RadialBindingResolver {
            catalog: &catalog,
            registry: &registry,
        }
        .resolve_frozen(&request.binding, &request.context, &request.history_query)
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

fn prepare_dynamic_bindings(
    dynamic: &mut BTreeMap<
        crate::radial::model::CellId,
        crate::radial::dynamic::FrozenDynamicFrame,
    >,
    resolver: &RadialBindingResolver<'_>,
    context: &InvocationContext,
) {
    for frame in dynamic.values_mut() {
        for entry in &mut frame.entries {
            let Some(binding) = entry.binding.as_ref() else {
                continue;
            };
            match resolver.resolve_frozen(binding, context, &entry.history_query) {
                Ok(prepared) => entry.requirement = prepared.requirement,
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
) {
    for prepared in frame.alternates.values_mut() {
        if prepared.availability == FrozenAvailability::Available {
            match resolver.resolve_frozen(&prepared.binding, context, &prepared.history_query) {
                Ok(binding) => prepared.requirement = binding.requirement,
                Err(reason) => {
                    prepared.availability = FrozenAvailability::Unavailable {
                        reason: format!("{reason:?}"),
                    };
                }
            }
        }
        apply_keep_open_compatibility(prepared);
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
    use crate::radial::model::{ActionBinding, TargetSelector};
    use crate::universal_actions::ActionId;

    fn leased_request() -> RadialDispatchRequest {
        let identity = crate::radial::handoff::RadialDispatchIdentity {
            session_id: crate::radial::model::SessionId::new("lease-session"),
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
