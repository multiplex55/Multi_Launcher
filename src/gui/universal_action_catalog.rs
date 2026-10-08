use crate::actions::Action;
use crate::dashboard::DashboardDataSnapshot;
use crate::radial::bindings::RadialBindingResolver;
use crate::radial::context::{InvocationContext, WindowIdentity};
use crate::radial::handoff::{InteractionRequirement, interaction_requirement};
use crate::radial::model::{ActionBinding, AfterActionPolicy, TargetSelector};
use crate::universal_actions::{
    ActionAvailability, ActionResolutionContext, ActionSafety, ActionSurface, ActionTarget,
    ActionTargetResolver, ActionTargetResolverContext, EffectiveActionPresentation,
    PersistedActionCatalog, PersistedUniversalActionRef, ResolvedActionTarget, RootLauncherPolicy,
    UniversalAction, UniversalActionInvocationContext, UniversalActionRegistry,
};
use std::hash::{Hash, Hasher};

use super::{ActivationSource, LauncherApp};

/// One immutable view of all already-loaded targets used by radial preparation,
/// dispatch-time revalidation, and the authoring action picker.
///
/// No runtime index from this snapshot is itself persistable. Assignment always
/// goes through [`UniversalActionPickerRow::assignment`].
#[derive(Clone)]
pub(crate) struct UniversalActionCatalogSnapshot {
    pub(super) entries: Vec<ResolvedActionTarget>,
    pub(super) recent_entries: Vec<(ResolvedActionTarget, String)>,
    pub(super) dashboard: std::sync::Arc<DashboardDataSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AuthoringCatalogDemand(u64);

pub(super) struct CachedAuthoringCatalog {
    pub(super) demand: AuthoringCatalogDemand,
    pub(super) snapshot: std::sync::Arc<UniversalActionCatalogSnapshot>,
    provider_deferral: super::search::ProviderSearchDeferral,
}

pub(super) type AuthoringCatalogCache = Option<CachedAuthoringCatalog>;

impl UniversalActionCatalogSnapshot {
    pub(crate) fn trace_unfiltered_custom_action_ranks(
        &self,
        invocation: &InvocationContext,
        session_id: u64,
        generation: u64,
    ) {
        if !crate::radial::acceptance_trace::enabled() {
            return;
        }
        self.emit_unfiltered_custom_action_ranks_with(
            invocation,
            session_id,
            generation,
            crate::radial::acceptance_trace::emit_designer_action_catalog_rank,
        );
    }

    fn emit_unfiltered_custom_action_ranks_with(
        &self,
        invocation: &InvocationContext,
        session_id: u64,
        generation: u64,
        mut emit: impl FnMut(usize, usize, usize, u64, u64),
    ) {
        if session_id == 0 || generation == 0 {
            return;
        }
        // Use the same full snapshot and catalog owner as this editor visit.
        // Filtered launcher result ordinals and custom source indexes are not
        // positions in this unfiltered target/action catalog.
        let catalog = UniversalActionAuthoringCatalog::build(self, invocation, "");
        for (rank, row) in catalog.rows().iter().enumerate() {
            if row.action_id == crate::universal_actions::action_ids::RESULT_EXECUTE
                && let Some(source_index) = row.custom_action_index
            {
                emit(
                    source_index,
                    rank,
                    catalog.rows().len(),
                    session_id,
                    generation,
                );
            }
        }
    }

    pub(crate) fn empty() -> Self {
        Self {
            entries: Vec::new(),
            recent_entries: Vec::new(),
            dashboard: std::sync::Arc::new(DashboardDataSnapshot::default()),
        }
    }

    pub(super) fn persisted_catalog(&self) -> PersistedActionCatalog {
        PersistedActionCatalog::new(self.entries.clone())
    }

    pub(crate) fn resolve_persisted_action(
        &self,
        reference: &PersistedUniversalActionRef,
        surface: ActionSurface,
        query: &str,
    ) -> Result<
        crate::universal_actions::ResolvedPersistedAction,
        crate::universal_actions::PersistedActionUnavailable,
    > {
        PersistedActionCatalog::resolve_from_entries_with_context(
            &self.entries,
            reference,
            &UniversalActionRegistry,
            |target| action_resolution_context_for_target(target, surface, query),
        )
    }

    pub(super) fn extend(&mut self, entries: impl IntoIterator<Item = ResolvedActionTarget>) {
        self.entries.extend(entries);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PickerPersistence {
    Persisted,
    Contextual,
    Ephemeral,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AfterActionCompatibility {
    pub(crate) close_current_menu: bool,
    pub(crate) close_tree: bool,
    pub(crate) keep_open: bool,
    pub(crate) keep_open_reason: Option<String>,
}

impl AfterActionCompatibility {
    fn for_requirement(requirement: InteractionRequirement) -> Self {
        let keep_open = requirement == InteractionRequirement::None;
        Self {
            close_current_menu: true,
            close_tree: true,
            keep_open,
            keep_open_reason: (!keep_open)
                .then(|| format!("KeepOpen is incompatible with {requirement:?}")),
        }
    }

    pub(crate) fn supports(&self, policy: AfterActionPolicy) -> bool {
        match policy {
            AfterActionPolicy::Inherit => true,
            AfterActionPolicy::CloseCurrentMenu => self.close_current_menu,
            AfterActionPolicy::CloseTree => self.close_tree,
            AfterActionPolicy::KeepOpen => self.keep_open,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UniversalActionPickerRow {
    /// Exact legacy command identifying the target shown in this row.
    pub(crate) target_command: String,
    /// Human-readable target identity, rendered inline so duplicate action
    /// names never depend on a hover tooltip for disambiguation.
    pub(crate) target_title: String,
    pub(crate) target_type: String,
    pub(crate) target_disambiguator: String,
    /// Runtime-only source position used by the native acceptance harness to
    /// identify a custom action without exposing its private label or command.
    pub(crate) custom_action_index: Option<usize>,
    search_text: String,
    /// Exact semantic action identifier resolved by the runtime provider.
    pub(crate) action_id: crate::universal_actions::ActionId,
    pub(crate) binding: Option<ActionBinding>,
    pub(crate) persistence: PickerPersistence,
    pub(crate) presentation: EffectiveActionPresentation,
    pub(crate) availability: ActionAvailability,
    pub(crate) unavailable_reason: Option<String>,
    pub(crate) destructive: bool,
    pub(crate) interaction: InteractionRequirement,
    pub(crate) after_action: AfterActionCompatibility,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NonPersistableAction {
    pub(crate) reason: String,
}

impl UniversalActionPickerRow {
    /// Return the only value an editor may put in a radial document.
    /// Runtime HWNDs, list indexes, browser identifiers, and clipboard slots
    /// deliberately have no conversion path through this API.
    pub(crate) fn assignment(&self) -> Result<ActionBinding, NonPersistableAction> {
        self.binding.clone().ok_or_else(|| NonPersistableAction {
            reason: self.unavailable_reason.clone().unwrap_or_else(|| {
                "This live target cannot be saved; choose a contextual binding".into()
            }),
        })
    }

    pub(crate) fn display_label(&self) -> String {
        target_action_display_label(
            &self.target_title,
            &self.target_type,
            &self.target_disambiguator,
            &self.presentation.label,
        )
    }

    #[cfg(test)]
    pub(crate) fn fixture(binding: ActionBinding) -> Self {
        let requirement = InteractionRequirement::None;
        Self {
            target_command: "fixture:pin".into(),
            target_title: "Fixture target".into(),
            target_type: "Fixture".into(),
            target_disambiguator: "pin-flow".into(),
            custom_action_index: None,
            search_text: "fixture pin flow".into(),
            action_id: crate::universal_actions::action_ids::RESULT_EXECUTE,
            binding: Some(binding),
            persistence: PickerPersistence::Persisted,
            presentation: EffectiveActionPresentation {
                label: "Fixture action".into(),
                short_label: None,
                description: Some("Retained Properties Pin fixture".into()),
                icon: None,
                group: Default::default(),
                priority: Default::default(),
                visible: true,
            },
            availability: ActionAvailability::Available,
            unavailable_reason: None,
            destructive: false,
            interaction: requirement,
            after_action: AfterActionCompatibility::for_requirement(requirement),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct UniversalActionAuthoringCatalog {
    rows: Vec<UniversalActionPickerRow>,
}

impl UniversalActionAuthoringCatalog {
    pub(crate) fn rows(&self) -> &[UniversalActionPickerRow] {
        &self.rows
    }

    pub(crate) fn build(
        snapshot: &UniversalActionCatalogSnapshot,
        invocation: &InvocationContext,
        query: &str,
    ) -> Self {
        let registry = UniversalActionRegistry;
        let mut rows = Vec::new();
        for target in &snapshot.entries {
            let context = action_resolution_context_for_target(
                &target.target,
                ActionSurface::RadialMenu,
                query,
            );
            rows.extend(rows_for_target(target, &registry, &context));
        }

        rows.extend(Self::contextual_window_rows(invocation));

        rows.retain(|row| matches_query(row, query));
        rows.sort_by(|left, right| {
            left.presentation
                .label
                .to_lowercase()
                .cmp(&right.presentation.label.to_lowercase())
                .then_with(|| left.target_command.cmp(&right.target_command))
                .then_with(|| left.action_id.cmp(&right.action_id))
        });
        rows.dedup_by(|left, right| {
            left.binding == right.binding
                && left.target_command == right.target_command
                && left.action_id == right.action_id
        });
        Self { rows }
    }

    pub(crate) fn contextual_window_rows(
        invocation: &InvocationContext,
    ) -> Vec<UniversalActionPickerRow> {
        let registry = UniversalActionRegistry;
        let context = ActionResolutionContext::new(ActionSurface::RadialMenu, "");
        let mut rows = Vec::new();
        for selector in [
            TargetSelector::CapturedForeground,
            TargetSelector::UnderPointer,
            TargetSelector::LastExternal,
        ] {
            let window = selected_window(invocation, &selector);
            let target = window.map_or_else(
                || ResolvedActionTarget {
                    target: ActionTarget::Window { hwnd: 0 },
                    selected_action: Action {
                        label: selector_label(&selector).into(),
                        desc: "Contextual Windows target".into(),
                        action: selector_command(&selector).into(),
                        args: None,
                    },
                    custom_action_index: None,
                },
                resolved_window,
            );
            for action in registry.resolve(&target, &context) {
                let binding = Some(ActionBinding::Contextual {
                    selector: selector.clone(),
                    action_id: action.id.clone(),
                });
                let mut row = picker_row(
                    selector_command(&selector).into(),
                    None,
                    selector_label(&selector).into(),
                    action,
                    binding,
                    PickerPersistence::Contextual,
                );
                row.target_title = target.selected_action.label.clone();
                row.target_type = "Contextual window".into();
                row.target_disambiguator = window.map_or_else(
                    || selector_label(&selector).to_owned(),
                    |window| format!("{} · PID {}", selector_label(&selector), window.pid),
                );
                if window.is_none() {
                    let reason = format!(
                        "{} is not available in the current preview context",
                        selector_label(&selector)
                    );
                    row.availability = ActionAvailability::Disabled {
                        reason: reason.clone(),
                    };
                    row.unavailable_reason = Some(reason);
                }
                rows.push(row);
            }
        }
        rows
    }

    fn from_ranked_targets(
        targets: impl IntoIterator<Item = ResolvedActionTarget>,
        query: &str,
    ) -> Self {
        let registry = UniversalActionRegistry;
        let mut rows = Vec::new();
        for target in targets {
            let context = action_resolution_context_for_target(
                &target.target,
                ActionSurface::RadialMenu,
                query,
            );
            rows.extend(rows_for_target(&target, &registry, &context));
        }
        Self { rows }
    }

    pub(crate) fn row_for_semantic_action(
        target: &ResolvedActionTarget,
        action_id: &crate::universal_actions::ActionId,
        query: &str,
    ) -> Option<UniversalActionPickerRow> {
        let context =
            action_resolution_context_for_target(&target.target, ActionSurface::RadialMenu, query);
        rows_for_target(target, &UniversalActionRegistry, &context)
            .into_iter()
            .find(|row| &row.action_id == action_id)
    }
}

fn rows_for_target(
    target: &ResolvedActionTarget,
    registry: &UniversalActionRegistry,
    context: &ActionResolutionContext<'_>,
) -> Vec<UniversalActionPickerRow> {
    let persistent = target.target.persistent_ref();
    let search_text = [
        target.selected_action.label.as_str(),
        target.selected_action.desc.as_str(),
        target.selected_action.action.as_str(),
        target.selected_action.args.as_deref().unwrap_or_default(),
    ]
    .join(" ");
    let target_title = target_title(target);
    let target_type = target_type(&target.target).to_owned();
    let target_disambiguator = target_disambiguator(target);
    registry
        .resolve(target, context)
        .into_iter()
        .map(|action| {
            let binding = persistent.clone().map(|target| ActionBinding::Persisted {
                action: PersistedUniversalActionRef {
                    target: Some(target),
                    action_id: action.id.clone(),
                },
            });
            let mut row = picker_row(
                target.selected_action.action.clone(),
                target.custom_action_index,
                search_text.clone(),
                action,
                binding,
                if persistent.is_some() {
                    PickerPersistence::Persisted
                } else {
                    PickerPersistence::Ephemeral
                },
            );
            row.target_title = target_title.clone();
            row.target_type = target_type.clone();
            row.target_disambiguator = target_disambiguator.clone();
            row
        })
        .collect()
}

fn target_title(target: &ResolvedActionTarget) -> String {
    match &target.target {
        ActionTarget::Generic { .. } | ActionTarget::CustomAction { .. } => {
            target.selected_action.label.clone()
        }
        ActionTarget::Folder { .. } | ActionTarget::Tempfile { .. } => {
            target.selected_action.label.clone()
        }
        ActionTarget::Bookmark { .. } => target.selected_action.label.clone(),
        ActionTarget::Timer { .. } | ActionTarget::Stopwatch { .. } => {
            target.selected_action.label.clone()
        }
        ActionTarget::Snippet { alias } => alias.clone(),
        ActionTarget::Note { .. } => target.selected_action.label.clone(),
        ActionTarget::ClipboardEntry { .. } => target.selected_action.label.clone(),
        ActionTarget::Todo { .. } => target.selected_action.label.clone(),
        ActionTarget::Window { .. } => target.selected_action.label.clone(),
        ActionTarget::MkMacro { .. } => target.selected_action.label.clone(),
        ActionTarget::BrowserTab { .. } => target.selected_action.label.clone(),
    }
}

fn target_type(target: &ActionTarget) -> &'static str {
    match target {
        ActionTarget::Generic { .. } => "Action",
        ActionTarget::CustomAction { .. } => "Custom action",
        ActionTarget::Folder { .. } => "Folder",
        ActionTarget::Bookmark { .. } => "Bookmark",
        ActionTarget::Timer { .. } => "Timer",
        ActionTarget::Stopwatch { .. } => "Stopwatch",
        ActionTarget::Snippet { .. } => "Snippet",
        ActionTarget::Tempfile { .. } => "Temporary file",
        ActionTarget::Note { .. } => "Note",
        ActionTarget::ClipboardEntry { .. } => "Clipboard entry",
        ActionTarget::Todo { .. } => "Todo",
        ActionTarget::Window { .. } => "Window",
        ActionTarget::MkMacro { .. } => "Macro",
        ActionTarget::BrowserTab { .. } => "Browser tab",
    }
}

fn target_disambiguator(target: &ResolvedActionTarget) -> String {
    match &target.target {
        ActionTarget::Note { slug } => format!("slug {slug}"),
        ActionTarget::Timer { id } => format!("timer {id}"),
        ActionTarget::Stopwatch { id } => format!("stopwatch {id}"),
        ActionTarget::ClipboardEntry { index } => format!("live slot {}", index + 1),
        ActionTarget::Todo { index } => format!("live item {}", index + 1),
        ActionTarget::Window { .. } => "live window".into(),
        ActionTarget::MkMacro { id } => format!("macro {id}"),
        ActionTarget::BrowserTab { url, .. } => url
            .as_ref()
            .map(|url| format!("URL {url}"))
            .unwrap_or_else(|| "live tab".into()),
        ActionTarget::CustomAction { index, .. } => format!("custom action {}", index + 1),
        ActionTarget::Folder { path } | ActionTarget::Tempfile { path } => path.clone(),
        ActionTarget::Bookmark { url } => url.clone(),
        ActionTarget::Generic { .. } | ActionTarget::Snippet { .. } => String::new(),
    }
}

fn target_action_display_label(
    title: &str,
    target_type: &str,
    disambiguator: &str,
    action_label: &str,
) -> String {
    let mut target = format!("{title} · {target_type}");
    if !disambiguator.is_empty() {
        target.push_str(&format!(" · {disambiguator}"));
    }
    format!("{target} — {action_label}")
}

pub(super) fn resolved_action_display_label(
    target: &ResolvedActionTarget,
    action: &UniversalAction,
    surface: ActionSurface,
) -> String {
    target_action_display_label(
        &target_title(target),
        target_type(&target.target),
        &target_disambiguator(target),
        &action.effective_presentation(surface).label,
    )
}

pub(super) fn action_resolution_context_for_target<'a>(
    target: &ActionTarget,
    surface: ActionSurface,
    query: &'a str,
) -> ActionResolutionContext<'a> {
    let mut context = ActionResolutionContext::new(surface, query);
    match target {
        ActionTarget::Timer { id } => {
            context.timer_paused = crate::plugins::timer::timer_paused(*id);
        }
        ActionTarget::Stopwatch { id } => {
            context.stopwatch_paused = crate::plugins::stopwatch::stopwatch_paused(*id);
        }
        _ => {}
    }
    context
}

fn picker_row(
    target_command: String,
    custom_action_index: Option<usize>,
    search_text: String,
    action: UniversalAction,
    binding: Option<ActionBinding>,
    persistence: PickerPersistence,
) -> UniversalActionPickerRow {
    let presentation = action.effective_presentation(ActionSurface::RadialMenu);
    let unavailable_reason = action
        .availability
        .disabled_reason()
        .map(str::to_owned)
        .or_else(|| {
            (persistence == PickerPersistence::Ephemeral)
                .then(|| "This live target cannot be saved; choose a contextual binding".into())
        });
    let interaction = interaction_requirement(&action);
    UniversalActionPickerRow {
        target_command,
        target_title: String::new(),
        target_type: String::new(),
        target_disambiguator: String::new(),
        custom_action_index,
        search_text,
        action_id: action.id,
        binding,
        persistence,
        presentation,
        availability: action.availability,
        unavailable_reason,
        destructive: action.safety == ActionSafety::Destructive,
        interaction,
        after_action: AfterActionCompatibility::for_requirement(interaction),
    }
}

fn matches_query(row: &UniversalActionPickerRow, query: &str) -> bool {
    let tokens = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return true;
    }

    let searchable = format!(
        "{} {} {} {}",
        row.presentation.label,
        row.presentation.description.as_deref().unwrap_or_default(),
        row.target_command,
        row.search_text,
    )
    .to_lowercase();
    tokens.iter().all(|token| searchable.contains(token))
}

fn selected_window<'a>(
    invocation: &'a InvocationContext,
    selector: &TargetSelector,
) -> Option<&'a WindowIdentity> {
    match selector {
        TargetSelector::CapturedForeground => invocation.foreground.as_ref(),
        TargetSelector::UnderPointer => invocation.under_pointer.as_ref(),
        TargetSelector::LastExternal => invocation.last_external.as_ref(),
    }
}

fn selector_command(selector: &TargetSelector) -> &'static str {
    match selector {
        TargetSelector::CapturedForeground => "context:captured_foreground",
        TargetSelector::UnderPointer => "context:under_pointer",
        TargetSelector::LastExternal => "context:last_external",
    }
}

fn selector_label(selector: &TargetSelector) -> &'static str {
    match selector {
        TargetSelector::CapturedForeground => "Captured foreground window",
        TargetSelector::UnderPointer => "Window under pointer",
        TargetSelector::LastExternal => "Last external window",
    }
}

fn resolved_window(window: &WindowIdentity) -> ResolvedActionTarget {
    ResolvedActionTarget {
        target: ActionTarget::Window {
            hwnd: window.hwnd as isize,
        },
        selected_action: Action {
            label: window.title.clone(),
            desc: "Windows".into(),
            action: format!("window:switch:{}", window.hwnd),
            args: None,
        },
        custom_action_index: None,
    }
}

#[cfg(test)]
pub(crate) fn retained_window_target(window: &WindowIdentity) -> ResolvedActionTarget {
    resolved_window(window)
}

impl LauncherApp {
    pub(super) fn authoring_catalog_for_ranked_actions(
        &self,
        actions: &[Action],
        query: &str,
    ) -> UniversalActionAuthoringCatalog {
        let custom_len = self.custom_len.min(self.actions.len());
        let resolver_context = ActionTargetResolverContext::new(
            &self.folder_aliases,
            &self.bookmark_aliases,
            &self.actions[..custom_len],
        );
        let targets = actions
            .iter()
            .map(|action| ActionTargetResolver.resolve(action, &resolver_context));
        UniversalActionAuthoringCatalog::from_ranked_targets(targets, query)
    }

    pub(super) fn resolve_launcher_result_action(
        &self,
        selected: &Action,
        query: &str,
    ) -> Result<(ResolvedActionTarget, UniversalAction), String> {
        let custom_len = self.custom_len.min(self.actions.len());
        let resolver_context = ActionTargetResolverContext::new(
            &self.folder_aliases,
            &self.bookmark_aliases,
            &self.actions[..custom_len],
        );
        let resolved = ActionTargetResolver.resolve(selected, &resolver_context);
        let resolution_context = action_resolution_context_for_target(
            &resolved.target,
            ActionSurface::RadialMenu,
            query,
        );
        let actions = UniversalActionRegistry.resolve(&resolved, &resolution_context);
        let parsed = crate::commands::parse_action(selected).ok();
        let action = actions
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
                actions.iter().find(|candidate| {
                    candidate.id == crate::universal_actions::action_ids::RESULT_EXECUTE
                })
            })
            .cloned()
            .ok_or_else(|| {
                "The first launcher result has no executable primary action".to_owned()
            })?;
        if let Some(reason) = action.availability.disabled_reason() {
            return Err(reason.to_owned());
        }
        Ok((resolved, action))
    }

    pub(super) fn resolve_exact_command_action(
        &self,
        command: &str,
        args: Option<&str>,
        query: &str,
    ) -> Result<(ResolvedActionTarget, UniversalAction), String> {
        let binding = ActionBinding::ExactCommand {
            command: command.to_owned(),
            args: args.map(str::to_owned),
        };
        let preparation = crate::radial::bindings::prepare_deferred_binding(&binding)
            .ok_or_else(|| "Exact command is not a deferred radial binding".to_owned())?;
        let (parsed, disposition) = match preparation.binding {
            crate::radial::dynamic::FrozenBinding::Deferred {
                kind: crate::radial::dynamic::DeferredBindingKind::ExactCommand { disposition, .. },
                parsed_command: Some(parsed),
                ..
            } => (parsed, disposition),
            _ => {
                return Err(format!(
                    "Saved exact command could not be parsed: {command}"
                ));
            }
        };
        if disposition == crate::radial::dynamic::ExactCommandDisposition::Invalid {
            return Err(format!(
                "Saved exact command could not be parsed: {command}"
            ));
        }
        let action = Action {
            label: command.to_owned(),
            desc: "Saved exact command".into(),
            action: command.to_owned(),
            args: args.map(str::to_owned),
        };
        let custom_len = self.custom_len.min(self.actions.len());
        let resolver_context = ActionTargetResolverContext::new(
            &self.folder_aliases,
            &self.bookmark_aliases,
            &self.actions[..custom_len],
        );
        let resolved = ActionTargetResolver.resolve(&action, &resolver_context);
        let resolution_context = action_resolution_context_for_target(
            &resolved.target,
            ActionSurface::RadialMenu,
            query,
        );
        let actions = UniversalActionRegistry.resolve(&resolved, &resolution_context);
        let selected = actions
            .iter()
            .find(|candidate| {
                matches!(
                    &candidate.operation,
                    crate::universal_actions::UniversalActionOperation::Command {
                        command: candidate_command,
                        original_action,
                    } if candidate_command == &parsed && original_action == &action
                )
            })
            .or_else(|| {
                actions.iter().find(|candidate| {
                    candidate.id == crate::universal_actions::action_ids::RESULT_EXECUTE
                })
            })
            .cloned()
            .ok_or_else(|| "Saved exact command has no safe radial execution route".to_owned())?;
        if let Some(reason) = selected.availability.disabled_reason() {
            return Err(reason.to_owned());
        }
        Ok((resolved, selected))
    }

    fn authoring_catalog_demand(&self) -> AuthoringCatalogDemand {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        crate::actions::actions_version().hash(&mut hasher);
        self.plugins.search_generation().hash(&mut hasher);
        self.custom_len.hash(&mut hasher);
        self.plugins
            .internal_services()
            .window_catalog
            .generation()
            .hash(&mut hasher);
        let dashboard = self.dashboard_data_cache.snapshot();
        (std::sync::Arc::as_ptr(&dashboard) as usize).hash(&mut hasher);
        let radial_document = crate::gui::radial_published_document();
        radial_document.revision.hash(&mut hasher);
        (std::sync::Arc::as_ptr(&radial_document) as usize).hash(&mut hasher);
        let macro_document = self.plugins.internal_services().mkmacro_store.snapshot();
        (std::sync::Arc::as_ptr(&macro_document) as usize).hash(&mut hasher);

        // Launcher results are part of the authoring catalog. Their contents
        // can change with the active query even when provider generations do
        // not, so fingerprint only these already-materialized rows.
        hash_actions(&self.results, &mut hasher);
        hash_aliases(&self.folder_aliases, &mut hasher);
        hash_aliases(&self.bookmark_aliases, &mut hasher);
        crate::history::with_history(|entries| {
            for entry in entries.iter().rev().take(32) {
                hash_action(&entry.action, &mut hasher);
                entry.query.hash(&mut hasher);
            }
        });
        AuthoringCatalogDemand(hasher.finish())
    }

    pub(crate) fn cached_universal_action_catalog_snapshot(
        &self,
    ) -> std::sync::Arc<UniversalActionCatalogSnapshot> {
        let demand = self.authoring_catalog_demand();
        if let Ok(cache) = self.authoring_catalog_cache.lock()
            && let Some(cached) = cache.as_ref()
            && cached.demand == demand
        {
            return std::sync::Arc::clone(&cached.snapshot);
        }

        let (snapshot, provider_deferral) = self.build_universal_action_catalog_snapshot();
        let snapshot = std::sync::Arc::new(snapshot);
        if let Ok(mut cache) = self.authoring_catalog_cache.lock() {
            *cache = Some(CachedAuthoringCatalog {
                demand,
                snapshot: std::sync::Arc::clone(&snapshot),
                provider_deferral,
            });
        }
        #[cfg(test)]
        self.authoring_catalog_build_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        snapshot
    }

    pub(super) fn retire_capacity_deferred_authoring_catalog(&self) {
        if let Ok(mut cache) = self.authoring_catalog_cache.lock()
            && cache.as_ref().is_some_and(|cached| {
                cached.provider_deferral == super::search::ProviderSearchDeferral::Capacity
            })
        {
            // Retire incomplete discovery without acquiring a new catalog.
            // Closed Designer frames must continue to do no provider work.
            *cache = None;
        }
    }

    pub(crate) fn universal_action_catalog_snapshot(&self) -> UniversalActionCatalogSnapshot {
        self.build_universal_action_catalog_snapshot().0
    }

    fn build_universal_action_catalog_snapshot(
        &self,
    ) -> (
        UniversalActionCatalogSnapshot,
        super::search::ProviderSearchDeferral,
    ) {
        let mut provider_deferral = super::search::ProviderSearchDeferral::None;
        let resolver = ActionTargetResolver;
        let custom_len = self.custom_len.min(self.actions.len());
        let resolver_context = ActionTargetResolverContext::new(
            &self.folder_aliases,
            &self.bookmark_aliases,
            &self.actions[..custom_len],
        );
        let dashboard = self.dashboard_data_cache.snapshot();
        let mut entries: Vec<_> = self
            .actions
            .iter()
            .chain(self.command_cache.iter())
            .chain(self.results.iter())
            .map(|action| resolver.resolve(action, &resolver_context))
            .collect();

        // Radial runtime/editor controls are stable generic commands. Keep
        // them authorable even when launcher plugin enablement hides their
        // search provider; persisted identity is the exact typed wire action.
        entries.extend(
            [
                ("Show default radial menu", "radial"),
                ("Close radial menu", "radial close"),
                ("Edit radial menus", "radial edit"),
                ("Edit radial skins", "radial skins"),
            ]
            .into_iter()
            .map(|(label, command)| Action {
                label: label.into(),
                desc: "Radial menu".into(),
                action: command.into(),
                args: None,
            })
            .map(|action| resolver.resolve(&action, &resolver_context)),
        );
        entries.extend(
            crate::gui::radial_published_document()
                .menus
                .iter()
                .map(|menu| Action {
                    label: format!("Show radial menu {}", menu.name),
                    desc: format!("Radial menu · exact ID {}", menu.id),
                    action: format!("radial show {}", menu.id),
                    args: None,
                })
                .map(|action| resolver.resolve(&action, &resolver_context)),
        );

        // Query-only providers expose their stable authoring and management
        // actions through the same read-only search used by launcher results.
        // Screen Draw and dashboard actions are present in `command_cache`;
        // every route still resolves through Universal Actions.
        for query in ["crop", "ss", "fav", "mkmacro", "note", "cs", "cb"] {
            let outcome = self.search_read_only_outcome(query);
            if outcome.provider_deferral == super::search::ProviderSearchDeferral::Capacity {
                provider_deferral = outcome.provider_deferral;
            }
            entries.extend(
                outcome
                    .actions
                    .iter()
                    .map(|action| resolver.resolve(action, &resolver_context)),
            );
        }

        entries.extend(
            dashboard
                .clipboard_history
                .iter()
                .enumerate()
                .map(|(index, text)| ResolvedActionTarget {
                    target: ActionTarget::ClipboardEntry { index },
                    selected_action: Action {
                        label: text.clone(),
                        desc: "Clipboard".into(),
                        action: format!("clipboard:copy:{index}"),
                        args: None,
                    },
                    custom_action_index: None,
                }),
        );
        entries.extend(
            dashboard
                .snippets
                .iter()
                .map(|snippet| ResolvedActionTarget {
                    target: ActionTarget::Snippet {
                        alias: snippet.alias.clone(),
                    },
                    selected_action: Action {
                        label: snippet.alias.clone(),
                        desc: "Snippet".into(),
                        action: crate::plugins::snippets::snippet_run_action(&snippet.alias),
                        args: None,
                    },
                    custom_action_index: None,
                }),
        );
        entries.extend(dashboard.notes.iter().map(|note| ResolvedActionTarget {
            target: ActionTarget::Note {
                slug: note.slug.clone(),
            },
            selected_action: Action {
                label: note.title.clone(),
                desc: "Note".into(),
                action: format!("note:open:{}", note.slug),
                args: None,
            },
            custom_action_index: None,
        }));
        entries.extend(dashboard.favorites.iter().map(|favorite| {
            let action = Action {
                label: favorite.label.clone(),
                desc: "Fav".into(),
                action: favorite.action.clone(),
                args: favorite.args.clone(),
            };
            resolver.resolve(&action, &resolver_context)
        }));
        entries.extend(
            self.plugins
                .internal_services()
                .mkmacro_store
                .snapshot()
                .macros
                .iter()
                .filter(|value| value.id != 0)
                .map(|value| ResolvedActionTarget {
                    target: ActionTarget::MkMacro { id: value.id },
                    selected_action: Action {
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
                .map(|window| ResolvedActionTarget {
                    target: ActionTarget::Window {
                        hwnd: window.hwnd as isize,
                    },
                    selected_action: Action {
                        label: window.title.clone(),
                        desc: "Windows".into(),
                        action: format!("window:switch:{}", window.hwnd),
                        args: None,
                    },
                    custom_action_index: None,
                }),
        );
        let recent_entries = crate::history::with_history(|history| {
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
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
        entries.extend(recent_entries.iter().map(|(entry, _)| entry.clone()));
        (
            UniversalActionCatalogSnapshot {
                entries,
                recent_entries,
                dashboard,
            },
            provider_deferral,
        )
    }

    pub(crate) fn universal_action_authoring_catalog(
        &self,
        invocation: &InvocationContext,
        query: &str,
    ) -> UniversalActionAuthoringCatalog {
        UniversalActionAuthoringCatalog::build(
            &self.universal_action_catalog_snapshot(),
            invocation,
            query,
        )
    }

    pub(crate) fn resolve_authoring_binding_action(
        &self,
        binding: &ActionBinding,
        invocation: &InvocationContext,
        history_query: &str,
        selected_query_action: Option<&Action>,
    ) -> Result<UniversalAction, String> {
        match binding {
            ActionBinding::Persisted { .. } | ActionBinding::Contextual { .. } => {
                let catalog = self.universal_action_catalog_snapshot().persisted_catalog();
                RadialBindingResolver::new(&catalog, &UniversalActionRegistry)
                    .resolve_with_context(
                        binding,
                        invocation,
                        history_query,
                        action_resolution_context_for_target,
                    )
                    .map(|prepared| prepared.action)
                    .map_err(|reason| format!("Action is no longer available: {reason:?}"))
            }
            ActionBinding::LauncherQuery {
                query,
                mode: crate::radial::model::QueryRunMode::OpenLauncher,
            } => {
                let action = Action {
                    label: query.clone(),
                    desc: "Saved launcher query".into(),
                    action: format!("query:{query}"),
                    args: None,
                };
                self.resolve_launcher_result_action(&action, query)
                    .map(|(_, action)| action)
            }
            ActionBinding::LauncherQuery {
                mode: crate::radial::model::QueryRunMode::ExecuteFirst,
                ..
            } => selected_query_action
                .ok_or_else(|| "Auto Submit Test requires a current first search result".to_owned())
                .and_then(|selected| {
                    self.resolve_launcher_result_action(selected, history_query)
                        .map(|(_, action)| action)
                }),
            ActionBinding::ExactCommand { command, args } => self
                .resolve_exact_command_action(command, args.as_deref(), history_query)
                .map(|(_, action)| action),
        }
    }

    pub(crate) fn test_radial_authoring_action(
        &mut self,
        binding: &ActionBinding,
        invocation: &InvocationContext,
        history_query: &str,
    ) -> super::universal_action_executor::UniversalActionExecution {
        self.test_radial_authoring_action_correlated(
            binding,
            invocation,
            history_query,
            None,
            None,
            None,
            None,
        )
    }

    pub(crate) fn test_radial_authoring_action_for_editor(
        &mut self,
        binding: &ActionBinding,
        invocation: &InvocationContext,
        history_query: &str,
        identity: &crate::gui::radial_editor::action_editor::AuthoringBindingEditorIdentity,
        selected_query_action: Option<&Action>,
        provider_revision: Option<u64>,
        result_catalog_versions: Option<crate::radial::dynamic::MutableResultCatalogVersions>,
    ) -> super::universal_action_executor::UniversalActionExecution {
        self.test_radial_authoring_action_correlated(
            binding,
            invocation,
            history_query,
            Some(identity),
            selected_query_action,
            provider_revision,
            result_catalog_versions,
        )
    }

    fn test_radial_authoring_action_correlated(
        &mut self,
        binding: &ActionBinding,
        invocation: &InvocationContext,
        history_query: &str,
        identity: Option<&crate::gui::radial_editor::action_editor::AuthoringBindingEditorIdentity>,
        selected_query_action: Option<&Action>,
        provider_revision: Option<u64>,
        result_catalog_versions: Option<crate::radial::dynamic::MutableResultCatalogVersions>,
    ) -> super::universal_action_executor::UniversalActionExecution {
        if let Some(identity) = identity
            && !self
                .radial_editor
                .lock()
                .ok()
                .is_some_and(|editor| editor.action_test_request_is_current(identity, binding))
        {
            return super::universal_action_executor::UniversalActionExecution::Unavailable;
        }
        let resolved = self.resolve_authoring_binding_action(
            binding,
            invocation,
            history_query,
            selected_query_action,
        );
        let action = match resolved {
            Ok(action) => action,
            Err(reason) => {
                self.fallback_authoring_execute_first_query(binding, &reason);
                self.report_error_message("radial_authoring.test_action", reason.clone());
                if let Some(identity) = identity {
                    self.finish_authoring_test_editor(identity, binding, Err(reason));
                }
                return super::universal_action_executor::UniversalActionExecution::Unavailable;
            }
        };
        let runtime_identity = match super::radial_actions::runtime_identity_for_target(
            &action.target,
            &self.plugins,
            result_catalog_versions,
        ) {
            Ok(identity) => identity,
            Err(reason) => {
                let reason = reason.to_owned();
                self.fallback_authoring_execute_first_query(binding, &reason);
                self.report_error_message("radial_authoring.test_action", &reason);
                if let Some(identity) = identity {
                    self.finish_authoring_test_editor(identity, binding, Err(reason));
                }
                return super::universal_action_executor::UniversalActionExecution::Unavailable;
            }
        };
        let captured_identity = match binding {
            ActionBinding::Contextual { selector, .. } => selected_window(invocation, selector)
                .map(|window| crate::window_catalog::WindowTargetIdentity {
                    hwnd: window.hwnd,
                    pid: window.pid,
                    executable: window.process_name.clone(),
                    process_path: window.process_path.clone(),
                    class_name: window.class_name.clone(),
                }),
            ActionBinding::Persisted { .. }
            | ActionBinding::LauncherQuery { .. }
            | ActionBinding::ExactCommand { .. } => None,
        };
        if let Some(captured) = captured_identity.as_ref()
            && !self
                .plugins
                .internal_services()
                .window_catalog
                .describe_current(captured.hwnd)
                .is_some_and(|window| captured.matches(&window))
        {
            let reason = "Sampled contextual window is no longer available".to_owned();
            self.report_error_message("radial_authoring.test_action", reason.clone());
            if let Some(identity) = identity {
                self.finish_authoring_test_editor(identity, binding, Err(reason));
            }
            return super::universal_action_executor::UniversalActionExecution::Unavailable;
        }
        if let Some(identity) = identity
            && !self
                .radial_editor
                .lock()
                .ok()
                .is_some_and(|editor| editor.action_test_request_is_current(identity, binding))
        {
            return super::universal_action_executor::UniversalActionExecution::Unavailable;
        }
        if !super::radial_actions::runtime_identity_is_current(
            &action.target,
            runtime_identity.as_ref(),
            &self.plugins,
        ) {
            let reason = "The selected runtime target changed before Test could run";
            self.fallback_authoring_execute_first_query(binding, reason);
            self.report_error_message("radial_authoring.test_action", reason);
            if let Some(identity) = identity {
                self.finish_authoring_test_editor(identity, binding, Err(reason.into()));
            }
            return super::universal_action_executor::UniversalActionExecution::Unavailable;
        }
        let window_catalog_generation =
            self.plugins.internal_services().window_catalog.generation();
        let execution = self.execute_radial_authoring_test_action(
            action,
            persisted_request(binding),
            history_query,
            matches!(
                binding,
                ActionBinding::LauncherQuery { .. } | ActionBinding::ExactCommand { .. }
            ),
        );
        if execution
            == super::universal_action_executor::UniversalActionExecution::ConfirmationRequired
            && let Some(pending) = self.pending_universal_confirm.as_mut()
        {
            pending.authoring_revalidation = Some(super::AuthoringActionRevalidation {
                binding: binding.clone(),
                invocation: invocation.clone(),
                captured_identity,
                window_catalog_generation,
                runtime_target: pending.action.target.clone(),
                runtime_identity,
                editor_identity: identity.cloned(),
                selected_query_action: selected_query_action.cloned(),
                provider_revision,
                result_catalog_versions,
            });
        }
        if let Some(identity) = identity {
            if execution
                == super::universal_action_executor::UniversalActionExecution::ConfirmationRequired
            {
                if let Ok(mut editor) = self.radial_editor.lock() {
                    let _ = editor.await_action_editor_confirmation(identity, binding);
                }
            } else {
                let result = match execution {
                    super::universal_action_executor::UniversalActionExecution::Executed => Ok(()),
                    super::universal_action_executor::UniversalActionExecution::Unavailable => {
                        Err("Action was not available for testing".into())
                    }
                    super::universal_action_executor::UniversalActionExecution::ConfirmationRequired => unreachable!(),
                };
                self.finish_authoring_test_editor(identity, binding, result);
            }
        }
        execution
    }

    fn finish_authoring_test_editor(
        &self,
        identity: &crate::gui::radial_editor::action_editor::AuthoringBindingEditorIdentity,
        binding: &ActionBinding,
        result: Result<(), String>,
    ) {
        if let Ok(mut editor) = self.radial_editor.lock() {
            let _ = editor.finish_action_editor_test(identity, binding, result);
        }
    }

    pub(super) fn fallback_authoring_execute_first_query(
        &mut self,
        binding: &ActionBinding,
        reason: &str,
    ) {
        if let ActionBinding::LauncherQuery {
            query,
            mode: crate::radial::model::QueryRunMode::ExecuteFirst,
        } = binding
        {
            self.open_deferred_query_fallback(query, super::ActivationSource::Click, reason);
        }
    }

    fn execute_radial_authoring_test_action(
        &mut self,
        action: UniversalAction,
        stable_request: Option<PersistedUniversalActionRef>,
        history_query: &str,
        primary_invocation: bool,
    ) -> super::universal_action_executor::UniversalActionExecution {
        let root_policy = if matches!(
            interaction_requirement(&action),
            InteractionRequirement::LauncherUi | InteractionRequirement::ExclusiveCapture
        ) {
            RootLauncherPolicy::Legacy
        } else {
            RootLauncherPolicy::PreserveOrdinaryState
        };
        self.execute_universal_action_with_context(
            action,
            UniversalActionInvocationContext {
                surface: ActionSurface::RadialMenu,
                source: ActivationSource::Click,
                stable_request,
                history_query: history_query.to_owned(),
                root_policy,
                primary_invocation,
            },
            None,
        )
    }
}

fn hash_actions(actions: &[Action], hasher: &mut impl Hasher) {
    actions
        .iter()
        .for_each(|action| hash_action(action, hasher));
}

fn hash_action(action: &Action, hasher: &mut impl Hasher) {
    action.label.hash(hasher);
    action.desc.hash(hasher);
    action.action.hash(hasher);
    action.args.hash(hasher);
}

fn hash_aliases(
    aliases: &std::collections::HashMap<String, Option<String>>,
    hasher: &mut impl Hasher,
) {
    let mut entries = aliases.iter().collect::<Vec<_>>();
    entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
    for (alias, value) in entries {
        alias.hash(hasher);
        value.hash(hasher);
    }
}

fn persisted_request(binding: &ActionBinding) -> Option<PersistedUniversalActionRef> {
    match binding {
        ActionBinding::Persisted { action } => Some(action.clone()),
        ActionBinding::Contextual { .. }
        | ActionBinding::LauncherQuery { .. }
        | ActionBinding::ExactCommand { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::model::TargetSelector;
    use crate::universal_actions::{
        PersistableActionTargetRef, UniversalActionOperation, action_ids,
    };

    fn action(label: &str, command: &str) -> Action {
        Action {
            label: label.into(),
            desc: "Test".into(),
            action: command.into(),
            args: None,
        }
    }

    fn window(hwnd: usize) -> WindowIdentity {
        WindowIdentity {
            hwnd,
            pid: 7,
            process_name: Some("editor.exe".into()),
            process_path: Some("C:\\Apps\\editor.exe".into()),
            class_name: Some("EditorWindow".into()),
            title: "Editor".into(),
        }
    }

    fn snapshot(entries: Vec<ResolvedActionTarget>) -> UniversalActionCatalogSnapshot {
        UniversalActionCatalogSnapshot {
            entries,
            recent_entries: Vec::new(),
            dashboard: std::sync::Arc::new(DashboardDataSnapshot::default()),
        }
    }

    #[test]
    fn dashboard_snippet_catalog_uses_alias_route_and_keeps_clipboard_history_literal() {
        let ctx = eframe::egui::Context::default();
        let directory = tempfile::tempdir().unwrap();
        let mut app = crate::gui::LauncherApp::new(
            &ctx,
            std::sync::Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new(),
            directory.path().join("actions.json").display().to_string(),
            directory.path().join("settings.json").display().to_string(),
            crate::settings::Settings::default(),
            None,
            None,
            None,
            None,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        app.test_skip_history_persistence = true;

        let alias = "Sales: λ|%";
        let template = "Private template {{name}}";
        let snippet = crate::plugins::snippets::SnippetEntry {
            alias: alias.into(),
            text: template.into(),
            hide_contents: true,
            prompt_for_fields: true,
            fields: vec![crate::plugins::snippets::SnippetFieldDefinition::new(
                "name",
            )],
        };
        let history_text = "Clipboard history {{name}}";
        let plain_alias = "literal row";
        let plain_snippet = crate::plugins::snippets::SnippetEntry {
            alias: plain_alias.into(),
            text: history_text.into(),
            hide_contents: false,
            prompt_for_fields: false,
            fields: Vec::new(),
        };
        let mut dashboard = DashboardDataSnapshot::default();
        dashboard.clipboard_history = std::sync::Arc::new(vec![history_text.into()]);
        dashboard.snippets = std::sync::Arc::new(vec![snippet, plain_snippet]);
        app.dashboard_data_cache.set_snapshot_for_test(dashboard);

        let usage_before = app.usage.clone();
        let history_before = app.test_recorded_history_queries.clone();
        let activations_before = app.test_activation_trace.clone();
        let version_before = crate::plugins::snippets::snippets_version();
        let catalog_snapshot = app.universal_action_catalog_snapshot();
        let canonical = crate::plugins::snippets::snippet_run_action(alias);
        let live_snippet = catalog_snapshot
            .entries
            .iter()
            .find(|entry| matches!(&entry.target, ActionTarget::Snippet { alias: found } if found == alias))
            .expect("dashboard prompted snippet appears in the catalog");
        assert_eq!(live_snippet.selected_action.label, alias);
        assert_eq!(live_snippet.selected_action.desc, "Snippet");
        assert_eq!(live_snippet.selected_action.action, canonical);
        assert_eq!(live_snippet.selected_action.args, None);
        assert!(!live_snippet.selected_action.action.contains(template));
        assert_eq!(
            crate::plugins::snippets::decode_snippet_run_action(
                &live_snippet.selected_action.action
            )
            .as_deref(),
            Some(alias)
        );
        let empty_aliases = std::collections::HashMap::new();
        let resolver_context =
            ActionTargetResolverContext::new(&empty_aliases, &empty_aliases, &[]);
        assert_eq!(
            ActionTargetResolver
                .resolve(&live_snippet.selected_action, &resolver_context)
                .target,
            ActionTarget::Snippet {
                alias: alias.into()
            }
        );

        let plain_row = catalog_snapshot
            .entries
            .iter()
            .find(|entry| {
                matches!(&entry.target, ActionTarget::Snippet { alias: found } if found == plain_alias)
            })
            .expect("plain dashboard snippet is still a snippet target");
        assert_eq!(
            plain_row.selected_action.action,
            crate::plugins::snippets::snippet_run_action(plain_alias)
        );
        assert_eq!(plain_row.selected_action.label, plain_alias);
        assert_eq!(plain_row.selected_action.desc, "Snippet");
        assert_eq!(plain_row.selected_action.args, None);

        let history_entry = catalog_snapshot
            .entries
            .iter()
            .find(|entry| matches!(entry.target, ActionTarget::ClipboardEntry { index: 0 }))
            .expect("dashboard clipboard history stays a live clipboard entry");
        assert_eq!(history_entry.selected_action.label, history_text);
        assert_eq!(history_entry.selected_action.desc, "Clipboard");
        assert_eq!(history_entry.selected_action.action, "clipboard:copy:0");

        let picker = UniversalActionAuthoringCatalog::build(
            &catalog_snapshot,
            &InvocationContext::empty(81),
            "",
        );
        for action_id in [
            action_ids::RESULT_EXECUTE,
            action_ids::SNIPPET_EDIT,
            action_ids::SNIPPET_REMOVE,
        ] {
            let row = picker
                .rows()
                .iter()
                .find(|row| {
                    row.target_type == "Snippet"
                        && row.target_title == alias
                        && row.action_id == action_id
                })
                .unwrap_or_else(|| panic!("snippet row {action_id} is available"));
            assert_eq!(row.target_command, canonical);
        }
        let history_row = picker
            .rows()
            .iter()
            .find(|row| {
                row.target_type == "Clipboard entry"
                    && row.target_title == history_text
                    && row.action_id == action_ids::RESULT_EXECUTE
            })
            .expect("clipboard history primary action remains available");
        assert_eq!(history_row.target_command, "clipboard:copy:0");

        let plain_execute = picker
            .rows()
            .iter()
            .find(|row| {
                row.target_type == "Snippet"
                    && row.target_title == plain_alias
                    && row.action_id == action_ids::RESULT_EXECUTE
            })
            .expect("plain snippet has the same alias-based primary route");
        assert_eq!(
            plain_execute.target_command,
            crate::plugins::snippets::snippet_run_action(plain_alias)
        );

        assert!(!app.snippet_prompt_dialog.is_open());
        assert_eq!(app.usage, usage_before);
        assert_eq!(app.test_recorded_history_queries, history_before);
        assert_eq!(app.test_activation_trace, activations_before);
        assert_eq!(crate::plugins::snippets::snippets_version(), version_before);
    }

    struct TimerCleanup(u64);

    impl Drop for TimerCleanup {
        fn drop(&mut self) {
            crate::plugins::timer::cancel_timer(self.0);
        }
    }

    struct StopwatchCleanup(u64);

    impl Drop for StopwatchCleanup {
        fn drop(&mut self) {
            let _ = crate::plugins::stopwatch::try_stop_stopwatch(self.0);
        }
    }

    fn live_state_target(target: ActionTarget, selected_action: Action) -> ResolvedActionTarget {
        ResolvedActionTarget {
            target,
            selected_action,
            custom_action_index: None,
        }
    }

    fn picker_action<'a>(
        catalog: &'a UniversalActionAuthoringCatalog,
        action_id: &crate::universal_actions::ActionId,
    ) -> &'a UniversalActionPickerRow {
        catalog
            .rows()
            .iter()
            .find(|row| &row.action_id == action_id)
            .expect("live timer or stopwatch action is in the picker")
    }

    fn frozen_runtime_binding(
        target: &ResolvedActionTarget,
        action_id: crate::universal_actions::ActionId,
    ) -> crate::radial::dynamic::FrozenBinding {
        crate::radial::dynamic::FrozenBinding::Runtime {
            target: target.target.clone(),
            selected_action: target.selected_action.clone(),
            action_id,
            identity: None,
        }
    }

    fn prepared_disabled_reason(
        result: Result<
            crate::radial::bindings::PreparedBinding,
            crate::radial::bindings::BindingUnavailable,
        >,
    ) -> Option<String> {
        match result {
            Ok(prepared) => {
                assert!(prepared.action.availability.is_available());
                None
            }
            Err(crate::radial::bindings::BindingUnavailable::DisabledAction { reason, .. }) => {
                Some(reason)
            }
            Err(other) => panic!("unexpected target resolution failure: {other:?}"),
        }
    }

    #[test]
    fn picker_and_runtime_resolver_follow_live_timer_and_stopwatch_state() {
        let timer_name = format!("radial-action-context-{}", std::process::id());
        crate::plugins::timer::start_timer_named(
            std::time::Duration::from_secs(600),
            Some(timer_name.clone()),
            String::new(),
        );
        let timer_id = crate::plugins::timer::active_timers()
            .into_iter()
            .find_map(|(id, name, _, _)| (name == timer_name).then_some(id))
            .expect("started timer is in the live catalog");
        let _timer_cleanup = TimerCleanup(timer_id);
        let timer = live_state_target(
            ActionTarget::Timer { id: timer_id },
            Action {
                label: timer_name,
                desc: "Timer".into(),
                action: format!("timer:show:{timer_id}"),
                args: None,
            },
        );
        assert_eq!(timer.target.persistent_ref(), None);

        let stopwatch_id = crate::plugins::stopwatch::start_stopwatch_named(Some(format!(
            "radial-action-context-{}",
            std::process::id()
        )));
        let _stopwatch_cleanup = StopwatchCleanup(stopwatch_id);
        let stopwatch = live_state_target(
            ActionTarget::Stopwatch { id: stopwatch_id },
            Action {
                label: "Live Stopwatch".into(),
                desc: "Stopwatch".into(),
                action: format!("timer:stopwatch:show:{stopwatch_id}"),
                args: None,
            },
        );
        assert_eq!(stopwatch.target.persistent_ref(), None);

        let registry = UniversalActionRegistry;
        let invocation = InvocationContext::empty(1);
        let verify_target = |target: &ResolvedActionTarget,
                             pause_id: &crate::universal_actions::ActionId,
                             resume_id: &crate::universal_actions::ActionId,
                             expected_pause: Option<&str>,
                             expected_resume: Option<&str>| {
            let authoring = UniversalActionAuthoringCatalog::build(
                &snapshot(vec![target.clone()]),
                &invocation,
                "",
            );
            assert_eq!(
                picker_action(&authoring, pause_id)
                    .availability
                    .disabled_reason(),
                expected_pause
            );
            assert_eq!(
                picker_action(&authoring, resume_id)
                    .availability
                    .disabled_reason(),
                expected_resume
            );

            let runtime_catalog = PersistedActionCatalog::new(vec![target.clone()]);
            let resolver = RadialBindingResolver::new(&runtime_catalog, &registry);
            let pause = frozen_runtime_binding(target, pause_id.clone());
            let resume = frozen_runtime_binding(target, resume_id.clone());
            assert_eq!(
                prepared_disabled_reason(resolver.resolve_frozen_with_context(
                    &pause,
                    &invocation,
                    "",
                    action_resolution_context_for_target,
                )),
                expected_pause.map(str::to_owned)
            );
            assert_eq!(
                prepared_disabled_reason(resolver.resolve_frozen_with_context(
                    &resume,
                    &invocation,
                    "",
                    action_resolution_context_for_target,
                )),
                expected_resume.map(str::to_owned)
            );
        };

        verify_target(
            &timer,
            &action_ids::TIMER_PAUSE,
            &action_ids::TIMER_RESUME,
            None,
            Some("Timer is already running"),
        );
        assert_eq!(
            crate::plugins::timer::try_pause_timer(timer_id),
            crate::plugins::timer::TimerMutation::Updated
        );
        verify_target(
            &timer,
            &action_ids::TIMER_PAUSE,
            &action_ids::TIMER_RESUME,
            Some("Timer is already paused"),
            None,
        );
        crate::plugins::timer::cancel_timer(timer_id);
        verify_target(
            &timer,
            &action_ids::TIMER_PAUSE,
            &action_ids::TIMER_RESUME,
            Some("Timer is no longer available"),
            Some("Timer is no longer available"),
        );

        verify_target(
            &stopwatch,
            &action_ids::STOPWATCH_PAUSE,
            &action_ids::STOPWATCH_RESUME,
            None,
            Some("Stopwatch is already running"),
        );
        assert_eq!(
            crate::plugins::stopwatch::try_pause_stopwatch(stopwatch_id),
            crate::plugins::stopwatch::StopwatchMutation::Updated
        );
        verify_target(
            &stopwatch,
            &action_ids::STOPWATCH_PAUSE,
            &action_ids::STOPWATCH_RESUME,
            Some("Stopwatch is already paused"),
            None,
        );
        let _ = crate::plugins::stopwatch::try_stop_stopwatch(stopwatch_id);
        verify_target(
            &stopwatch,
            &action_ids::STOPWATCH_PAUSE,
            &action_ids::STOPWATCH_RESUME,
            Some("Stopwatch is no longer available"),
            Some("Stopwatch is no longer available"),
        );
    }

    fn install_live_window_catalog(
        app: &mut LauncherApp,
        descriptor: crate::window_catalog::WindowDescriptor,
    ) -> std::sync::Arc<std::sync::Mutex<Option<crate::window_catalog::WindowDescriptor>>> {
        let live = std::sync::Arc::new(std::sync::Mutex::new(Some(descriptor.clone())));
        let provider = std::sync::Arc::clone(&live);
        app.plugins.set_window_catalog_for_test(
            crate::window_catalog::WindowCatalog::from_snapshot_with_descriptor(
                vec![descriptor],
                move |hwnd| {
                    provider
                        .lock()
                        .ok()
                        .and_then(|window| window.clone())
                        .filter(|window| window.hwnd == hwnd)
                },
            ),
        );
        live
    }

    #[test]
    fn stable_and_contextual_assignments_round_trip_without_runtime_identity() {
        let selected = action("Screen Draw", "screen_draw:start");
        let stable_target = ResolvedActionTarget {
            target: ActionTarget::Generic {
                action: selected.clone(),
            },
            selected_action: selected,
            custom_action_index: None,
        };
        let invocation = InvocationContext {
            foreground: Some(window(41)),
            ..InvocationContext::empty(1)
        };
        let catalog =
            UniversalActionAuthoringCatalog::build(&snapshot(vec![stable_target]), &invocation, "");
        for persistence in [PickerPersistence::Persisted, PickerPersistence::Contextual] {
            let binding = catalog
                .rows()
                .iter()
                .find(|row| row.persistence == persistence)
                .unwrap()
                .assignment()
                .unwrap();
            let json = serde_json::to_string(&binding).unwrap();
            assert_eq!(
                serde_json::from_str::<ActionBinding>(&json).unwrap(),
                binding
            );
            assert!(!json.contains("41"));
        }
    }

    #[test]
    fn ephemeral_runtime_targets_cannot_be_assigned() {
        let target = resolved_window(&window(91));
        let catalog = UniversalActionAuthoringCatalog::build(
            &snapshot(vec![target]),
            &InvocationContext::empty(1),
            "",
        );
        let row = catalog
            .rows()
            .iter()
            .find(|row| row.persistence == PickerPersistence::Ephemeral)
            .unwrap();
        assert!(row.assignment().is_err());
        assert!(row.binding.is_none());
    }

    #[test]
    fn contextual_window_binding_resolves_after_hwnd_churn() {
        let binding = ActionBinding::Contextual {
            selector: TargetSelector::CapturedForeground,
            action_id: action_ids::WINDOW_ACTIVATE,
        };
        let invocation = InvocationContext {
            foreground: Some(window(808)),
            ..InvocationContext::empty(2)
        };
        let persisted = PersistedActionCatalog::new(Vec::new());
        let prepared = RadialBindingResolver::new(&persisted, &UniversalActionRegistry)
            .resolve(&binding, &invocation, "")
            .unwrap();
        assert_eq!(prepared.action.target, ActionTarget::Window { hwnd: 808 });
    }

    #[test]
    fn explicitly_sampled_context_enables_contextual_picker_rows() {
        let snapshot = snapshot(Vec::new());
        let empty =
            UniversalActionAuthoringCatalog::build(&snapshot, &InvocationContext::empty(1), "");
        let binding = ActionBinding::Contextual {
            selector: TargetSelector::CapturedForeground,
            action_id: action_ids::WINDOW_ACTIVATE,
        };
        assert!(empty.rows().iter().any(|row| {
            row.binding.as_ref() == Some(&binding)
                && matches!(row.availability, ActionAvailability::Disabled { .. })
        }));

        let sampled = UniversalActionAuthoringCatalog::build(
            &snapshot,
            &InvocationContext {
                foreground: Some(window(808)),
                ..InvocationContext::empty(2)
            },
            "",
        );
        assert!(sampled.rows().iter().any(|row| {
            row.binding.as_ref() == Some(&binding)
                && row.availability == ActionAvailability::Available
        }));
    }

    #[test]
    fn picker_presentation_and_policy_flags_match_runtime_resolution() {
        let selected = action("Screen Draw", "screen_draw:start");
        let target = ResolvedActionTarget {
            target: ActionTarget::Generic {
                action: selected.clone(),
            },
            selected_action: selected,
            custom_action_index: None,
        };
        let snapshot = snapshot(vec![target.clone()]);
        let runtime = UniversalActionRegistry.resolve(
            &target,
            &ActionResolutionContext::new(ActionSurface::RadialMenu, ""),
        );
        let rows =
            UniversalActionAuthoringCatalog::build(&snapshot, &InvocationContext::empty(1), "");
        for action in runtime {
            let row = rows
                .rows()
                .iter()
                .find(|row| {
                    row.action_id == action.id
                        && row.target_command == target.selected_action.action
                })
                .unwrap();
            assert_eq!(
                row.presentation,
                action.effective_presentation(ActionSurface::RadialMenu)
            );
            assert_eq!(row.availability, action.availability);
            assert_eq!(row.destructive, action.safety == ActionSafety::Destructive);
            assert_eq!(row.interaction, interaction_requirement(&action));
            assert_eq!(
                row.after_action.supports(AfterActionPolicy::KeepOpen),
                interaction_requirement(&action) == InteractionRequirement::None
            );
        }
    }

    #[test]
    fn search_filters_before_popup_limit_and_keeps_custom_source_identity() {
        let entries = (0..64)
            .map(|index| {
                let command = format!("zz_radial_acceptance_{index:03}");
                let selected = action(&format!("Acceptance action {index:03}"), &command);
                ResolvedActionTarget {
                    target: ActionTarget::Generic {
                        action: selected.clone(),
                    },
                    selected_action: selected,
                    custom_action_index: Some(index),
                }
            })
            .collect();
        let snapshot = snapshot(entries);
        let invocation = InvocationContext::empty(1);
        let unfiltered = UniversalActionAuthoringCatalog::build(&snapshot, &invocation, "");
        let target_rank = unfiltered
            .rows()
            .iter()
            .position(|row| {
                row.custom_action_index == Some(63) && row.action_id == action_ids::RESULT_EXECUTE
            })
            .expect("target action should be in the unfiltered catalog");
        assert!(target_rank >= 50, "target rank was {target_rank}");

        let filtered =
            UniversalActionAuthoringCatalog::build(&snapshot, &invocation, "Acceptance action 063");
        let target = filtered
            .rows()
            .iter()
            .find(|row| {
                row.custom_action_index == Some(63) && row.action_id == action_ids::RESULT_EXECUTE
            })
            .expect("search should reveal the custom target beyond the first page");
        assert!(target.assignment().is_ok());
        assert!(!filtered.rows().iter().any(|row| {
            row.custom_action_index != Some(63) && row.target_command == "zz_radial_acceptance_063"
        }));
    }

    #[test]
    fn unfiltered_rank_receipt_uses_the_actual_snapshot_and_visit_before_search_filtering() {
        let actions = (0..64)
            .map(|index| Action {
                label: format!("Radial Acceptance Harmless Action {index:03}"),
                desc: "Deterministic native authoring fixture".into(),
                action: format!("radial_acceptance_harmless_{index:03}"),
                args: None,
            })
            .collect::<Vec<_>>();
        let entries = actions
            .iter()
            .enumerate()
            .map(|(index, action)| ResolvedActionTarget {
                target: ActionTarget::CustomAction {
                    index,
                    action: action.clone(),
                },
                selected_action: action.clone(),
                custom_action_index: Some(index),
            })
            .collect();
        let snapshot = snapshot(entries);
        let invocation = InvocationContext::empty(41);
        let full = UniversalActionAuthoringCatalog::build(&snapshot, &invocation, "");
        let actual_rank = full
            .rows()
            .iter()
            .position(|row| {
                row.custom_action_index == Some(63) && row.action_id == action_ids::RESULT_EXECUTE
            })
            .unwrap();
        assert!(actual_rank >= 50);
        assert_ne!(actual_rank, 63, "source position is not full-catalog rank");
        let mut receipts = Vec::new();
        snapshot.emit_unfiltered_custom_action_ranks_with(
            &invocation,
            77,
            19,
            |source, rank, len, session, generation| {
                receipts.push((source, rank, len, session, generation))
            },
        );
        assert_eq!(receipts.len(), 64);
        assert_eq!(
            receipts
                .iter()
                .filter(|receipt| receipt.0 == 63)
                .collect::<Vec<_>>(),
            [&(63, actual_rank, full.rows().len(), 77, 19)]
        );
        let filtered = UniversalActionAuthoringCatalog::build(
            &snapshot,
            &invocation,
            "Radial Acceptance Harmless Action 063",
        );
        assert!(
            filtered
                .rows()
                .iter()
                .position(|row| row.action_id == action_ids::RESULT_EXECUTE)
                .unwrap()
                < 50
        );
        assert!(filtered.rows().iter().any(|row| {
            row.custom_action_index == Some(63)
                && row.binding
                    == Some(ActionBinding::Persisted {
                        action: PersistedUniversalActionRef {
                            target: Some(PersistableActionTargetRef::CustomAction {
                                action: actions[63].clone(),
                            }),
                            action_id: action_ids::RESULT_EXECUTE,
                        },
                    })
        }));
        assert_eq!(
            snapshot
                .entries
                .iter()
                .map(|entry| entry.selected_action.clone())
                .collect::<Vec<_>>(),
            actions
        );
        let context = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&context);
        app.custom_len = actions.len();
        app.actions = std::sync::Arc::new(actions.clone());
        app.update_action_cache();
        let history_before =
            serde_json::to_vec(&crate::history::with_history(Clone::clone).unwrap()).unwrap();
        let usage_before = app.usage.clone();
        let activations_before = app.test_activation_trace.clone();
        let query = "app Radial Acceptance Harmless Action 063";
        let outcome = app.search_read_only_outcome(query);
        assert_eq!(outcome.actions.first(), Some(&actions[63]));
        let search_catalog = app.authoring_catalog_for_ranked_actions(&outcome.actions, query);
        let execute = &search_catalog.rows()[0];
        assert_eq!(execute.custom_action_index, Some(63));
        assert_eq!(execute.action_id, action_ids::RESULT_EXECUTE);
        assert_eq!(
            execute.display_label(),
            "Radial Acceptance Harmless Action 063 · Custom action · custom action 64 — Execute"
        );
        assert_eq!(
            serde_json::to_vec(&crate::history::with_history(Clone::clone).unwrap()).unwrap(),
            history_before
        );
        assert_eq!(app.usage, usage_before);
        assert_eq!(app.test_activation_trace, activations_before);
        let mut invalid_receipts = 0;
        for (session, generation) in [(0, 19), (77, 0)] {
            snapshot.emit_unfiltered_custom_action_ranks_with(
                &invocation,
                session,
                generation,
                |_, _, _, _, _| invalid_receipts += 1,
            );
        }
        assert_eq!(invalid_receipts, 0);
    }

    #[test]
    fn acceptance_fixture_queries_reach_real_ranked_actions_and_authoring_rows() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        let mut custom_actions = vec![
            action("QMarker Alpha", "qmarker-alpha"),
            action("QMarker Beta", "qmarker-beta"),
        ];
        custom_actions.extend((0..64).map(|index| {
            action(
                &format!("Radial Acceptance Harmless Action {index:03}"),
                &format!("radial_acceptance_harmless_{index:03}"),
            )
        }));
        for (index, custom_action) in custom_actions.iter_mut().enumerate().skip(34) {
            custom_action.label = format!("Radial Acceptance Secondary Action {:03}", index - 2);
        }
        app.custom_len = custom_actions.len();
        app.actions = std::sync::Arc::new(custom_actions);
        app.update_action_cache();
        app.plugins.register(Box::new(
            crate::plugins::note::NotePlugin::fixture_for_authoring_search(&[
                (
                    "Shared Acceptance Note",
                    "radial-acceptance-shared-a",
                    "First duplicate-title target for radial authoring.",
                ),
                (
                    "Shared Acceptance Note",
                    "radial-acceptance-shared-b",
                    "Second duplicate-title target for radial authoring.",
                ),
            ]),
        ));
        app.query_results_layout.enabled = true;
        app.query_results_layout.respect_plugin_capability = true;

        let history_before =
            serde_json::to_vec(&crate::history::with_history(Clone::clone).unwrap())
                .expect("serialize the pre-search history snapshot");
        let usage_before = app.usage.clone();
        let activations_before = app.test_activation_trace.clone();

        let app_query = "app Radial Acceptance Harmless Action";
        app.query = app_query.into();
        app.search();
        assert!(app.resolved_grid_layout);
        let app_outcome = app.search_read_only_outcome(app_query);
        assert_eq!(
            app_outcome.state,
            crate::gui::search::LauncherSearchState::Results
        );
        assert_eq!(app_outcome.actions.len(), 32);
        assert!(
            app_outcome
                .actions
                .iter()
                .all(|candidate| { candidate.action.starts_with("radial_acceptance_harmless_") })
        );
        let app_catalog = app.authoring_catalog_for_ranked_actions(&app_outcome.actions, app_query);
        assert!(app_catalog.rows().len() > 50);
        assert!(app_catalog.rows().len() <= 96);
        let beyond_fifty_rank = app_outcome
            .actions
            .iter()
            .position(|candidate| candidate.action == "radial_acceptance_harmless_030")
            .expect("fixture custom action 030 should survive shared app ranking");
        assert_eq!(beyond_fifty_rank, 30);
        let ranked_custom = app_catalog
            .rows()
            .iter()
            .find(|row| {
                row.custom_action_index == Some(32)
                    && row.target_command == "radial_acceptance_harmless_030"
                    && row.action_id == action_ids::RESULT_EXECUTE
            })
            .expect("ranked Gate C custom target should expose its real primary action");
        let ranked_custom_row = app_catalog
            .rows()
            .iter()
            .position(|row| std::ptr::eq(row, ranked_custom))
            .expect("selected action row should belong to the bounded catalog");
        assert_eq!(
            ranked_custom_row, 60,
            "Execute row rank was {ranked_custom_row}"
        );
        assert_eq!(
            ranked_custom.target_title,
            "Radial Acceptance Harmless Action 030"
        );
        assert_eq!(ranked_custom.target_disambiguator, "custom action 33");
        assert_eq!(ranked_custom.action_id, action_ids::RESULT_EXECUTE);
        assert_eq!(
            ranked_custom.presentation.label, "Execute",
            "Gate C's readable custom-action label must follow the shared presentation"
        );
        assert_eq!(
            ranked_custom.display_label(),
            "Radial Acceptance Harmless Action 030 · Custom action · custom action 33 — Execute"
        );
        assert!(ranked_custom.assignment().is_ok());

        let adjacent_edit_row = &app_catalog.rows()[61];
        assert_eq!(adjacent_edit_row.custom_action_index, Some(32));
        assert_eq!(
            adjacent_edit_row.target_command,
            "radial_acceptance_harmless_030"
        );
        assert_eq!(
            adjacent_edit_row.target_title,
            "Radial Acceptance Harmless Action 030"
        );
        assert_eq!(adjacent_edit_row.target_type, "Custom action");
        assert_eq!(adjacent_edit_row.target_disambiguator, "custom action 33");
        assert_eq!(adjacent_edit_row.action_id, action_ids::CUSTOM_ACTION_EDIT);
        assert_eq!(adjacent_edit_row.presentation.label, "Edit App");
        assert_eq!(
            adjacent_edit_row.display_label(),
            "Radial Acceptance Harmless Action 030 · Custom action · custom action 33 — Edit App"
        );
        assert!(adjacent_edit_row.assignment().is_ok());

        let note_query = "note search Shared Acceptance";
        app.query = note_query.into();
        app.search();
        assert!(!app.resolved_grid_layout);
        let note_outcome = app.search_read_only_outcome(note_query);
        assert_eq!(
            note_outcome.state,
            crate::gui::search::LauncherSearchState::Results
        );
        assert_eq!(
            note_outcome
                .actions
                .iter()
                .filter(|candidate| candidate
                    .action
                    .starts_with("note:open:radial-acceptance-shared-"))
                .count(),
            2
        );
        let note_catalog =
            app.authoring_catalog_for_ranked_actions(&note_outcome.actions, note_query);
        for slug in ["radial-acceptance-shared-a", "radial-acceptance-shared-b"] {
            let row = note_catalog
                .rows()
                .iter()
                .find(|row| {
                    row.action_id == action_ids::NOTE_EDIT
                        && row.target_disambiguator == format!("slug {slug}")
                })
                .expect("each duplicate note should expose the shared secondary Edit action");
            assert_eq!(row.target_title, "Shared Acceptance Note");
            assert_eq!(row.target_type, "Note");
            assert!(row.display_label().contains(&format!("slug {slug}")));
            assert!(row.assignment().is_ok());
        }

        let history_after =
            serde_json::to_vec(&crate::history::with_history(Clone::clone).unwrap())
                .expect("serialize the post-search history snapshot");
        assert_eq!(history_after, history_before);
        assert_eq!(app.usage, usage_before);
        assert_eq!(app.test_activation_trace, activations_before);
    }

    #[test]
    fn preview_catalog_browsing_never_executes_an_action() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        app.actions = std::sync::Arc::new(vec![action("Help", "help:show")]);
        app.update_action_cache();
        let before = app.test_activation_trace.len();
        let catalog = app.universal_action_authoring_catalog(&InvocationContext::empty(1), "");
        assert!(!catalog.rows().is_empty());
        assert_eq!(app.test_activation_trace.len(), before);
    }

    #[test]
    fn unchanged_designer_reuses_action_catalog_until_results_change() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);

        let first = app.cached_universal_action_catalog_snapshot();
        let second = app.cached_universal_action_catalog_snapshot();
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(
            app.authoring_catalog_build_count
                .load(std::sync::atomic::Ordering::SeqCst),
            1,
            "an unchanged open Designer must not rerun its provider catalog searches"
        );

        app.results.push(action("Changed result", "help:show"));
        let refreshed = app.cached_universal_action_catalog_snapshot();
        assert!(!std::sync::Arc::ptr_eq(&second, &refreshed));
        assert_eq!(
            app.authoring_catalog_build_count
                .load(std::sync::atomic::Ordering::SeqCst),
            2,
            "a changed launcher result set invalidates the cached authoring catalog"
        );
    }

    #[test]
    fn radial_control_commands_are_stable_generic_picker_targets() {
        let ctx = eframe::egui::Context::default();
        let app = crate::gui::actions::tests::new_app(&ctx);
        let catalog = app.universal_action_authoring_catalog(&InvocationContext::empty(1), "");
        for command in ["radial", "radial close", "radial edit", "radial skins"] {
            let row = catalog
                .rows()
                .iter()
                .find(|row| row.target_command == command)
                .unwrap_or_else(|| panic!("missing radial picker target {command}"));
            assert_eq!(row.persistence, PickerPersistence::Persisted);
            assert!(matches!(
                row.assignment().unwrap(),
                ActionBinding::Persisted { .. }
            ));
        }
    }

    #[test]
    fn explicit_test_facade_uses_executor_without_a_radial_dispatch_lease() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        let selected = action("Help", "help:show");
        app.actions = std::sync::Arc::new(vec![selected.clone()]);
        app.update_action_cache();
        let binding = ActionBinding::Persisted {
            action: PersistedUniversalActionRef {
                target: Some(PersistableActionTargetRef::LegacyAction {
                    action: selected.clone(),
                }),
                action_id: action_ids::RESULT_EXECUTE,
            },
        };

        assert_eq!(
            app.test_radial_authoring_action(
                &binding,
                &InvocationContext::empty(1),
                "authoring test",
            ),
            super::super::universal_action_executor::UniversalActionExecution::Executed
        );
        assert_eq!(
            app.test_activation_trace,
            [(selected, ActivationSource::Click)]
        );
        assert!(app.radial_preparations.is_empty());
        assert!(app.radial_consumed_dispatches.is_empty());
        assert!(app.radial_current_preparation.is_none());
    }

    #[test]
    fn contextual_test_revalidates_before_immediate_or_confirmation_disabled_execution() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        app.require_confirm_destructive = false;
        let captured = window(44);
        let descriptor = crate::window_catalog::WindowDescriptor {
            title: captured.title.clone(),
            hwnd: captured.hwnd,
            pid: captured.pid,
            executable: captured.process_name.clone(),
            process_path: captured.process_path.clone(),
            class_name: captured.class_name.clone(),
        };
        let live = install_live_window_catalog(&mut app, descriptor.clone());
        *live.lock().unwrap() = Some(crate::window_catalog::WindowDescriptor {
            pid: 99,
            ..descriptor
        });
        let invocation = InvocationContext {
            foreground: Some(captured),
            ..InvocationContext::empty(1)
        };
        for action_id in [action_ids::WINDOW_ACTIVATE, action_ids::WINDOW_CLOSE] {
            let binding = ActionBinding::Contextual {
                selector: TargetSelector::CapturedForeground,
                action_id,
            };
            assert_eq!(
                app.test_radial_authoring_action(&binding, &invocation, "authoring test"),
                super::super::universal_action_executor::UniversalActionExecution::Unavailable
            );
        }
        assert!(app.test_activation_trace.is_empty());
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.pending_universal_confirm.is_none());
    }

    #[test]
    fn contextual_test_confirmation_rechecks_the_same_captured_identity() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        app.require_confirm_destructive = true;
        let captured = window(44);
        let descriptor = crate::window_catalog::WindowDescriptor {
            title: captured.title.clone(),
            hwnd: captured.hwnd,
            pid: captured.pid,
            executable: captured.process_name.clone(),
            process_path: captured.process_path.clone(),
            class_name: captured.class_name.clone(),
        };
        let live = install_live_window_catalog(&mut app, descriptor.clone());
        let invocation = InvocationContext {
            foreground: Some(captured),
            ..InvocationContext::empty(1)
        };
        let binding = ActionBinding::Contextual {
            selector: TargetSelector::CapturedForeground,
            action_id: action_ids::WINDOW_CLOSE,
        };
        assert_eq!(
            app.test_radial_authoring_action(&binding, &invocation, "authoring test"),
            super::super::universal_action_executor::UniversalActionExecution::ConfirmationRequired
        );
        *live.lock().unwrap() = Some(crate::window_catalog::WindowDescriptor {
            pid: 99,
            ..descriptor
        });
        assert!(app.resolve_pending_universal_action_confirmation(true));
        assert!(app.test_activation_trace.is_empty());
        assert!(app.test_recorded_history_queries.is_empty());
    }

    #[test]
    fn destructive_test_action_confirms_once_and_records_history_once() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        app.require_confirm_destructive = true;
        let selected = action("Screen Draw", "screen_draw:start");
        app.actions = std::sync::Arc::new(vec![selected.clone()]);
        app.update_action_cache();
        let stable = PersistedUniversalActionRef {
            target: Some(PersistableActionTargetRef::LegacyAction {
                action: selected.clone(),
            }),
            action_id: action_ids::RESULT_EXECUTE,
        };
        let destructive = UniversalAction {
            id: action_ids::NOTE_REMOVE,
            target: ActionTarget::Generic {
                action: selected.clone(),
            },
            presentation: crate::universal_actions::ActionPresentation::new("Test remove"),
            availability: ActionAvailability::Available,
            safety: ActionSafety::Destructive,
            operation: UniversalActionOperation::InvokePrimary(selected),
        };
        assert_eq!(
            app.execute_radial_authoring_test_action(
                destructive,
                Some(stable),
                "authoring test",
                false,
            ),
            super::super::universal_action_executor::UniversalActionExecution::ConfirmationRequired
        );
        assert!(app.resolve_pending_universal_action_confirmation(true));
        assert!(!app.resolve_pending_universal_action_confirmation(true));
        assert_eq!(app.test_recorded_history_queries, ["authoring test"]);
        assert_eq!(app.test_activation_trace.len(), 1);
    }

    #[test]
    fn destructive_test_confirmation_revalidates_target_and_fails_closed_after_deletion() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        app.require_confirm_destructive = true;
        app.show_inline_errors = true;
        let selected = action("Disposable", "screen_draw:start");
        app.actions = std::sync::Arc::new(vec![selected.clone()]);
        app.update_action_cache();
        let stable = PersistedUniversalActionRef {
            target: Some(PersistableActionTargetRef::LegacyAction {
                action: selected.clone(),
            }),
            action_id: action_ids::RESULT_EXECUTE,
        };
        let destructive = UniversalAction {
            id: action_ids::NOTE_REMOVE,
            target: ActionTarget::Generic {
                action: selected.clone(),
            },
            presentation: crate::universal_actions::ActionPresentation::new("Test remove"),
            availability: ActionAvailability::Available,
            safety: ActionSafety::Destructive,
            operation: UniversalActionOperation::InvokePrimary(selected),
        };
        assert_eq!(
            app.execute_radial_authoring_test_action(
                destructive,
                Some(stable),
                "authoring test",
                false,
            ),
            super::super::universal_action_executor::UniversalActionExecution::ConfirmationRequired
        );

        app.actions = std::sync::Arc::new(Vec::new());
        app.update_action_cache();
        assert!(app.resolve_pending_universal_action_confirmation(true));
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.test_activation_trace.is_empty());
        assert!(app.error.as_deref().is_some_and(|message| {
            message.contains("Action changed or disappeared before confirmation")
        }));
    }

    #[test]
    fn cancelling_destructive_test_action_executes_and_records_nothing() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        app.require_confirm_destructive = true;
        let selected = action("Screen Draw", "screen_draw:start");
        let destructive = UniversalAction {
            id: action_ids::NOTE_REMOVE,
            target: ActionTarget::Generic {
                action: selected.clone(),
            },
            presentation: crate::universal_actions::ActionPresentation::new("Test remove"),
            availability: ActionAvailability::Available,
            safety: ActionSafety::Destructive,
            operation: UniversalActionOperation::InvokePrimary(selected),
        };
        assert_eq!(
            app.execute_radial_authoring_test_action(destructive, None, "authoring test", false),
            super::super::universal_action_executor::UniversalActionExecution::ConfirmationRequired
        );
        assert!(app.resolve_pending_universal_action_confirmation(false));
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.test_activation_trace.is_empty());
    }
}
