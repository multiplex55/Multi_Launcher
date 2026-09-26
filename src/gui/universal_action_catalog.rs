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

pub(super) type AuthoringCatalogCache = Option<(
    AuthoringCatalogDemand,
    std::sync::Arc<UniversalActionCatalogSnapshot>,
)>;

impl UniversalActionCatalogSnapshot {
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
            let persistent = target.target.persistent_ref();
            let search_text = [
                target.selected_action.label.as_str(),
                target.selected_action.desc.as_str(),
                target.selected_action.action.as_str(),
                target.selected_action.args.as_deref().unwrap_or_default(),
            ]
            .join(" ");
            let context = ActionResolutionContext::new(ActionSurface::RadialMenu, query);
            for action in registry.resolve(target, &context) {
                let binding = persistent.clone().map(|target| ActionBinding::Persisted {
                    action: PersistedUniversalActionRef {
                        target: Some(target),
                        action_id: action.id.clone(),
                    },
                });
                rows.push(picker_row(
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
                ));
            }
        }

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
            let context = ActionResolutionContext::new(ActionSurface::RadialMenu, query);
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

impl LauncherApp {
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
            && let Some((cached_demand, snapshot)) = cache.as_ref()
            && *cached_demand == demand
        {
            return std::sync::Arc::clone(snapshot);
        }

        let snapshot = std::sync::Arc::new(self.universal_action_catalog_snapshot());
        if let Ok(mut cache) = self.authoring_catalog_cache.lock() {
            *cache = Some((demand, std::sync::Arc::clone(&snapshot)));
        }
        #[cfg(test)]
        self.authoring_catalog_build_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        snapshot
    }

    pub(crate) fn universal_action_catalog_snapshot(&self) -> UniversalActionCatalogSnapshot {
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
            entries.extend(
                self.search_read_only(query)
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
                        action: format!("clipboard:{}", snippet.text),
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
        UniversalActionCatalogSnapshot {
            entries,
            recent_entries,
            dashboard,
        }
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

    pub(crate) fn test_radial_authoring_action(
        &mut self,
        binding: &ActionBinding,
        invocation: &InvocationContext,
        history_query: &str,
    ) -> super::universal_action_executor::UniversalActionExecution {
        let snapshot = self.universal_action_catalog_snapshot();
        let catalog = snapshot.persisted_catalog();
        let registry = UniversalActionRegistry;
        let prepared = RadialBindingResolver {
            catalog: &catalog,
            registry: &registry,
        }
        .resolve(binding, invocation, history_query);
        match prepared {
            Ok(prepared) => {
                let captured_identity = match binding {
                    ActionBinding::Contextual { selector, .. } => {
                        selected_window(invocation, selector).map(|window| {
                            crate::window_catalog::WindowTargetIdentity {
                                hwnd: window.hwnd,
                                pid: window.pid,
                                executable: window.process_name.clone(),
                                process_path: window.process_path.clone(),
                                class_name: window.class_name.clone(),
                            }
                        })
                    }
                    ActionBinding::Persisted { .. } => None,
                    ActionBinding::LauncherQuery { .. } | ActionBinding::ExactCommand { .. } => {
                        None
                    }
                };
                if let Some(identity) = captured_identity.as_ref()
                    && !self
                        .plugins
                        .internal_services()
                        .window_catalog
                        .describe_current(identity.hwnd)
                        .is_some_and(|window| identity.matches(&window))
                {
                    self.report_error_message(
                        "radial_authoring.test_action",
                        "Sampled contextual window is no longer available",
                    );
                    return super::universal_action_executor::UniversalActionExecution::Unavailable;
                }
                let window_catalog_generation =
                    self.plugins.internal_services().window_catalog.generation();
                let result = self.execute_radial_authoring_test_action(
                    prepared.action,
                    persisted_request(binding),
                    history_query,
                );
                if result
                    == super::universal_action_executor::UniversalActionExecution::ConfirmationRequired
                    && let Some(pending) = self.pending_universal_confirm.as_mut()
                {
                    pending.authoring_revalidation = Some(super::AuthoringActionRevalidation {
                        binding: binding.clone(),
                        invocation: invocation.clone(),
                        captured_identity,
                        window_catalog_generation,
                    });
                }
                result
            }
            Err(reason) => {
                self.report_error_message(
                    "radial_authoring.test_action",
                    format!("Action is no longer available: {reason:?}"),
                );
                super::universal_action_executor::UniversalActionExecution::Unavailable
            }
        }
    }

    fn execute_radial_authoring_test_action(
        &mut self,
        action: UniversalAction,
        stable_request: Option<PersistedUniversalActionRef>,
        history_query: &str,
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
        let prepared = RadialBindingResolver {
            catalog: &persisted,
            registry: &UniversalActionRegistry,
        }
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
            app.execute_radial_authoring_test_action(destructive, Some(stable), "authoring test"),
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
            app.execute_radial_authoring_test_action(destructive, Some(stable), "authoring test"),
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
            app.execute_radial_authoring_test_action(destructive, None, "authoring test"),
            super::super::universal_action_executor::UniversalActionExecution::ConfirmationRequired
        );
        assert!(app.resolve_pending_universal_action_confirmation(false));
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.test_activation_trace.is_empty());
    }
}
