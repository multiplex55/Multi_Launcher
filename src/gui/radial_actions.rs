use crate::radial::bindings::RadialBindingResolver;
use crate::radial::bindings::{
    BindingUnavailable, PreparedBinding, PreparedCell, RadialPrepareEnvelope, RadialPrepareReply,
    project_menu_frame,
};
use crate::radial::context::{CompiledContextRules, InvocationContext};
use crate::radial::dynamic::{
    DynamicCandidate, DynamicSnapshots, FrozenAvailability, FrozenBinding,
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
        let mut entries: Vec<_> = self
            .actions
            .iter()
            .chain(self.results.iter())
            .map(|action| resolver.resolve(action, &resolver_context))
            .collect();
        let dashboard = self.dashboard_data_cache.snapshot();
        entries.extend(
            dashboard
                .clipboard_history
                .iter()
                .enumerate()
                .map(
                    |(index, text)| crate::universal_actions::ResolvedActionTarget {
                        target: crate::universal_actions::ActionTarget::ClipboardEntry { index },
                        selected_action: crate::actions::Action {
                            label: text.clone(),
                            desc: "Clipboard".into(),
                            action: format!("clipboard:copy:{index}"),
                            args: None,
                        },
                        custom_action_index: None,
                    },
                ),
        );
        entries.extend(dashboard.snippets.iter().map(|snippet| {
            crate::universal_actions::ResolvedActionTarget {
                target: crate::universal_actions::ActionTarget::Snippet {
                    alias: snippet.alias.clone(),
                },
                selected_action: crate::actions::Action {
                    label: snippet.alias.clone(),
                    desc: "Snippet".into(),
                    action: format!("clipboard:{}", snippet.text),
                    args: None,
                },
                custom_action_index: None,
            }
        }));
        entries.extend(dashboard.notes.iter().map(|note| {
            crate::universal_actions::ResolvedActionTarget {
                target: crate::universal_actions::ActionTarget::Note {
                    slug: note.slug.clone(),
                },
                selected_action: crate::actions::Action {
                    label: note.title.clone(),
                    desc: "Note".into(),
                    action: format!("note:open:{}", note.slug),
                    args: None,
                },
                custom_action_index: None,
            }
        }));
        let macro_snapshot = self.plugins.internal_services().mkmacro_store.snapshot();
        entries.extend(
            macro_snapshot
                .macros
                .iter()
                .filter(|value| value.id != 0)
                .map(|value| crate::universal_actions::ResolvedActionTarget {
                    target: crate::universal_actions::ActionTarget::MkMacro { id: value.id },
                    selected_action: crate::actions::Action {
                        label: value.name.clone(),
                        desc: if value.description.is_empty() {
                            "Mouse/keyboard macro".into()
                        } else {
                            value.description.clone()
                        },
                        action: format!("mkmacro:run:{}", value.id),
                        args: None,
                    },
                    custom_action_index: None,
                }),
        );
        entries.extend(
            self.plugins
                .internal_services()
                .window_catalog
                .snapshot()
                .iter()
                .map(|window| crate::universal_actions::ResolvedActionTarget {
                    target: crate::universal_actions::ActionTarget::Window {
                        hwnd: window.hwnd as isize,
                    },
                    selected_action: crate::actions::Action {
                        label: window.title.clone(),
                        desc: "Windows".into(),
                        action: format!("window:switch:{}", window.hwnd),
                        args: None,
                    },
                    custom_action_index: None,
                }),
        );
        let recent_entries: Vec<_> = crate::history::with_history(|history| {
            history
                .iter()
                .rev()
                .take(32)
                .map(|entry| {
                    (
                        resolver.resolve(&entry.action, &resolver_context),
                        entry.query.clone(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
        entries.extend(recent_entries.iter().map(|(entry, _)| entry.clone()));
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
            entries.extend(resolved.iter().cloned());
            query_entries.insert(query.clone(), resolved);
        }
        let captured_result_entries: Vec<_> = captured_results
            .iter()
            .map(|action| resolver.resolve(action, &resolver_context))
            .collect();
        entries.extend(captured_result_entries.iter().cloned());
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
                .filter_map(|favorite| {
                    self.actions.iter().find(|action| {
                        action.action == favorite.action && action.args == favorite.args
                    })
                })
                .map(|action| dynamic_candidate(&resolver.resolve(action, &resolver_context)))
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
                    unavailable_reason: None,
                    history_query: None,
                })
                .collect();
            snapshots.recent = recent_entries
                .iter()
                .map(|(entry, query)| {
                    let mut candidate = dynamic_candidate(entry);
                    candidate.history_query = Some(query.clone());
                    candidate
                })
                .collect();
            for entry in &entries {
                let candidate = dynamic_candidate(entry);
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
            for (query, resolved) in &query_entries {
                snapshots.launcher_queries.insert(
                    query.clone(),
                    resolved.iter().map(dynamic_candidate).collect(),
                );
            }
            snapshots.launcher_results = captured_result_entries
                .iter()
                .map(dynamic_candidate)
                .collect();
            for cell in menu.rings.iter().flat_map(|ring| &ring.cells) {
                match &cell.content {
                    CellContent::Action { binding } => {
                        match binding_resolver.resolve(
                            binding,
                            &request.context,
                            &request.invocation_query,
                        ) {
                            Ok(prepared) => {
                                static_cells.insert(
                                    cell.id.clone(),
                                    PreparedCell {
                                        binding: FrozenBinding::Stable(binding.clone()),
                                        availability: FrozenAvailability::Available,
                                        requirement: prepared.requirement,
                                        after_action: effective_after_action(
                                            &request.document,
                                            menu,
                                            cell.after_action,
                                        ),
                                        history_query: request.invocation_query.clone(),
                                    },
                                );
                            }
                            Err(reason) => {
                                unavailable.insert(cell.id.clone(), reason.clone());
                                static_cells.insert(
                                    cell.id.clone(),
                                    PreparedCell {
                                        binding: FrozenBinding::Stable(binding.clone()),
                                        availability: FrozenAvailability::Unavailable {
                                            reason: format!("{reason:?}"),
                                        },
                                        requirement: InteractionRequirement::None,
                                        after_action: effective_after_action(
                                            &request.document,
                                            menu,
                                            cell.after_action,
                                        ),
                                        history_query: request.invocation_query.clone(),
                                    },
                                );
                            }
                        }
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
                        PreparedCell {
                            binding: FrozenBinding::Stable(binding.clone()),
                            availability: resolved.as_ref().map_or_else(
                                |reason| FrozenAvailability::Unavailable {
                                    reason: format!("{reason:?}"),
                                },
                                |_| FrozenAvailability::Available,
                            ),
                            requirement: resolved
                                .as_ref()
                                .map_or(InteractionRequirement::None, |prepared| {
                                    prepared.requirement
                                }),
                            after_action: effective_after_action(
                                &request.document,
                                menu,
                                if id == "__center" {
                                    menu.center_primary_after_action
                                } else {
                                    menu.background_primary_after_action
                                },
                            ),
                            history_query: request.invocation_query.clone(),
                        },
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
        let mut frame = project_menu_frame(&menu, static_cells.clone(), &dynamic, 0, 12);
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
                        effective_after_action(&request.document, &menu, alternate.after_action),
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
                            PreparedCell {
                                binding: FrozenBinding::Stable(binding.clone()),
                                availability: resolved.as_ref().map_or_else(
                                    |reason| FrozenAvailability::Unavailable {
                                        reason: format!("{reason:?}"),
                                    },
                                    |_| FrozenAvailability::Available,
                                ),
                                requirement: resolved
                                    .as_ref()
                                    .map_or(InteractionRequirement::None, |prepared| {
                                        prepared.requirement
                                    }),
                                after_action: effective_after_action(
                                    &request.document,
                                    child,
                                    cell.after_action,
                                ),
                                history_query: request.invocation_query.clone(),
                            },
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
                            effective_after_action(&request.document, child, policy),
                            &request.invocation_query,
                        ),
                    );
                }
            }
            prepare_dynamic_bindings(&mut child_dynamic, &binding_resolver, &request.context);
            let mut child_frame = project_menu_frame(child, child_static, &child_dynamic, 0, 12);
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
                            effective_after_action(
                                &request.document,
                                child,
                                alternate.after_action,
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
        let resolver = ActionTargetResolver;
        let custom_len = self.custom_len.min(self.actions.len());
        let context = ActionTargetResolverContext::new(
            &self.folder_aliases,
            &self.bookmark_aliases,
            &self.actions[..custom_len],
        );
        let mut entries: Vec<_> = self
            .actions
            .iter()
            .chain(self.results.iter())
            .map(|action| resolver.resolve(action, &context))
            .collect();
        let dashboard = self.dashboard_data_cache.snapshot();
        entries.extend(
            dashboard
                .clipboard_history
                .iter()
                .enumerate()
                .map(
                    |(index, text)| crate::universal_actions::ResolvedActionTarget {
                        target: crate::universal_actions::ActionTarget::ClipboardEntry { index },
                        selected_action: crate::actions::Action {
                            label: text.clone(),
                            desc: "Clipboard".into(),
                            action: format!("clipboard:copy:{index}"),
                            args: None,
                        },
                        custom_action_index: None,
                    },
                ),
        );
        entries.extend(dashboard.snippets.iter().map(|snippet| {
            crate::universal_actions::ResolvedActionTarget {
                target: crate::universal_actions::ActionTarget::Snippet {
                    alias: snippet.alias.clone(),
                },
                selected_action: crate::actions::Action {
                    label: snippet.alias.clone(),
                    desc: "Snippet".into(),
                    action: format!("clipboard:{}", snippet.text),
                    args: None,
                },
                custom_action_index: None,
            }
        }));
        entries.extend(dashboard.notes.iter().map(|note| {
            crate::universal_actions::ResolvedActionTarget {
                target: crate::universal_actions::ActionTarget::Note {
                    slug: note.slug.clone(),
                },
                selected_action: crate::actions::Action {
                    label: note.title.clone(),
                    desc: "Note".into(),
                    action: format!("note:open:{}", note.slug),
                    args: None,
                },
                custom_action_index: None,
            }
        }));
        let macro_snapshot = self.plugins.internal_services().mkmacro_store.snapshot();
        entries.extend(
            macro_snapshot
                .macros
                .iter()
                .filter(|value| value.id != 0)
                .map(|value| crate::universal_actions::ResolvedActionTarget {
                    target: crate::universal_actions::ActionTarget::MkMacro { id: value.id },
                    selected_action: crate::actions::Action {
                        label: value.name.clone(),
                        desc: if value.description.is_empty() {
                            "Mouse/keyboard macro".into()
                        } else {
                            value.description.clone()
                        },
                        action: format!("mkmacro:run:{}", value.id),
                        args: None,
                    },
                    custom_action_index: None,
                }),
        );
        entries.extend(
            self.plugins
                .internal_services()
                .window_catalog
                .snapshot()
                .iter()
                .map(|window| crate::universal_actions::ResolvedActionTarget {
                    target: crate::universal_actions::ActionTarget::Window {
                        hwnd: window.hwnd as isize,
                    },
                    selected_action: crate::actions::Action {
                        label: window.title.clone(),
                        desc: "Windows".into(),
                        action: format!("window:switch:{}", window.hwnd),
                        args: None,
                    },
                    custom_action_index: None,
                }),
        );
        entries.extend(
            crate::history::with_history(|history| {
                history
                    .iter()
                    .rev()
                    .take(32)
                    .map(|entry| resolver.resolve(&entry.action, &context))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        );
        let catalog = PersistedActionCatalog::new(entries);
        let registry = UniversalActionRegistry;
        RadialBindingResolver {
            catalog: &catalog,
            registry: &registry,
        }
        .resolve_frozen(&request.binding, &request.context, &request.history_query)
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
    after_action: crate::radial::model::AfterActionPolicy,
    history_query: &str,
) -> PreparedCell {
    PreparedCell {
        binding: FrozenBinding::Stable(binding.clone()),
        availability: resolved.as_ref().map_or_else(
            |reason| FrozenAvailability::Unavailable {
                reason: format!("{reason:?}"),
            },
            |_| FrozenAvailability::Available,
        ),
        requirement: resolved
            .as_ref()
            .map_or(InteractionRequirement::None, |prepared| {
                prepared.requirement
            }),
        after_action,
        history_query: history_query.to_owned(),
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
        unavailable_reason: None,
        history_query: None,
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
}
