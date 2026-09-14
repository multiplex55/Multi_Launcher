use super::{
    ActionId, ActionResolutionContext, ActionTarget, PersistableActionTargetRef,
    PersistedUniversalActionRef, ResolvedActionTarget, UniversalAction, UniversalActionRegistry,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistedActionUnavailable {
    GlobalTargetUnsupported,
    InvalidStableIdentity { kind: &'static str },
    TargetMissing { kind: &'static str },
    ActionMissing { action_id: ActionId },
}

impl std::fmt::Display for PersistedActionUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GlobalTargetUnsupported => {
                formatter.write_str("global Universal Actions are not registered")
            }
            Self::InvalidStableIdentity { kind } => {
                write!(formatter, "saved {kind} identity is invalid")
            }
            Self::TargetMissing { kind } => write!(formatter, "saved {kind} no longer exists"),
            Self::ActionMissing { action_id } => {
                write!(
                    formatter,
                    "action {action_id} is unavailable for the saved target"
                )
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedPersistedAction {
    pub reference: PersistedUniversalActionRef,
    pub target: ResolvedActionTarget,
    pub action: UniversalAction,
}

/// Immutable current catalog used to reverse-resolve durable radial bindings.
/// Callers rebuild it from their already-loaded snapshots; no index is retained
/// beyond this resolution operation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PersistedActionCatalog {
    entries: Vec<ResolvedActionTarget>,
}

impl PersistedActionCatalog {
    pub fn new(entries: Vec<ResolvedActionTarget>) -> Self {
        Self { entries }
    }

    pub fn entries(&self) -> &[ResolvedActionTarget] {
        &self.entries
    }

    pub fn resolve(
        &self,
        reference: &PersistedUniversalActionRef,
        registry: &UniversalActionRegistry,
        context: &ActionResolutionContext<'_>,
    ) -> Result<ResolvedPersistedAction, PersistedActionUnavailable> {
        let saved = reference
            .target
            .as_ref()
            .ok_or(PersistedActionUnavailable::GlobalTargetUnsupported)?;
        validate_saved_target(saved)?;
        let target = self
            .entries
            .iter()
            .find(|entry| target_matches(&entry.target, saved))
            .cloned()
            .ok_or(PersistedActionUnavailable::TargetMissing {
                kind: target_kind(saved),
            })?;
        let action = registry
            .resolve(&target, context)
            .into_iter()
            .find(|action| action.id == reference.action_id)
            .ok_or_else(|| PersistedActionUnavailable::ActionMissing {
                action_id: reference.action_id.clone(),
            })?;
        Ok(ResolvedPersistedAction {
            reference: reference.clone(),
            target,
            action,
        })
    }
}

fn validate_saved_target(
    saved: &PersistableActionTargetRef,
) -> Result<(), PersistedActionUnavailable> {
    let invalid = match saved {
        PersistableActionTargetRef::LegacyAction { action }
        | PersistableActionTargetRef::CustomAction { action } => {
            action.label.is_empty() || action.action.is_empty()
        }
        PersistableActionTargetRef::Folder { path }
        | PersistableActionTargetRef::Bookmark { url: path }
        | PersistableActionTargetRef::Snippet { alias: path }
        | PersistableActionTargetRef::Tempfile { path }
        | PersistableActionTargetRef::Note { slug: path } => path.is_empty(),
        PersistableActionTargetRef::MkMacro { id } => *id == 0,
    };
    if invalid {
        Err(PersistedActionUnavailable::InvalidStableIdentity {
            kind: target_kind(saved),
        })
    } else {
        Ok(())
    }
}

fn target_kind(saved: &PersistableActionTargetRef) -> &'static str {
    match saved {
        PersistableActionTargetRef::LegacyAction { .. } => "legacy action",
        PersistableActionTargetRef::CustomAction { .. } => "custom action",
        PersistableActionTargetRef::Folder { .. } => "folder",
        PersistableActionTargetRef::Bookmark { .. } => "bookmark",
        PersistableActionTargetRef::Snippet { .. } => "snippet",
        PersistableActionTargetRef::Tempfile { .. } => "tempfile",
        PersistableActionTargetRef::Note { .. } => "note",
        PersistableActionTargetRef::MkMacro { .. } => "MkMacro",
    }
}

fn target_matches(target: &ActionTarget, saved: &PersistableActionTargetRef) -> bool {
    match (target, saved) {
        (
            ActionTarget::Generic { action: current },
            PersistableActionTargetRef::LegacyAction { action: saved },
        ) => current == saved,
        (
            ActionTarget::CustomAction {
                action: current, ..
            },
            PersistableActionTargetRef::CustomAction { action: saved },
        ) => current == saved,
        (
            ActionTarget::Folder { path: current },
            PersistableActionTargetRef::Folder { path: saved },
        )
        | (
            ActionTarget::Tempfile { path: current },
            PersistableActionTargetRef::Tempfile { path: saved },
        ) => current == saved,
        (
            ActionTarget::Bookmark { url: current },
            PersistableActionTargetRef::Bookmark { url: saved },
        ) => current == saved,
        (
            ActionTarget::Snippet { alias: current },
            PersistableActionTargetRef::Snippet { alias: saved },
        ) => current == saved,
        (
            ActionTarget::Note { slug: current },
            PersistableActionTargetRef::Note { slug: saved },
        ) => current == saved,
        (
            ActionTarget::MkMacro { id: current },
            PersistableActionTargetRef::MkMacro { id: saved },
        ) => current == saved,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::universal_actions::{ActionSurface, action_ids};

    fn action(label: &str, command: &str) -> Action {
        Action {
            label: label.into(),
            desc: "Custom".into(),
            action: command.into(),
            args: None,
        }
    }

    fn resolved_custom(index: usize, value: Action) -> ResolvedActionTarget {
        ResolvedActionTarget {
            target: ActionTarget::CustomAction {
                index,
                action: value.clone(),
            },
            selected_action: value,
            custom_action_index: Some(index),
        }
    }

    fn reference(value: Action, action_id: ActionId) -> PersistedUniversalActionRef {
        PersistedUniversalActionRef {
            target: Some(PersistableActionTargetRef::CustomAction { action: value }),
            action_id,
        }
    }

    #[test]
    fn custom_identity_survives_reorder_but_never_retargets_a_deleted_slot() {
        let saved = action("Build", "build.exe");
        let other = action("Other", "other.exe");
        let catalog = PersistedActionCatalog::new(vec![
            resolved_custom(0, other),
            resolved_custom(1, saved.clone()),
        ]);
        let context = ActionResolutionContext::new(ActionSurface::RadialMenu, "");
        let resolved = catalog
            .resolve(
                &reference(saved.clone(), action_ids::RESULT_EXECUTE),
                &UniversalActionRegistry,
                &context,
            )
            .unwrap();
        assert!(matches!(
            resolved.target.target,
            ActionTarget::CustomAction { index: 1, .. }
        ));

        let deleted = PersistedActionCatalog::new(Vec::new())
            .resolve(
                &reference(saved, action_ids::RESULT_EXECUTE),
                &UniversalActionRegistry,
                &context,
            )
            .unwrap_err();
        assert_eq!(
            deleted,
            PersistedActionUnavailable::TargetMissing {
                kind: "custom action"
            }
        );
    }

    #[test]
    fn custom_identity_includes_arguments() {
        let mut saved = action("Build", "build.exe");
        saved.args = Some("--release".into());
        let mut wrong = saved.clone();
        wrong.args = Some("--debug".into());
        let catalog = PersistedActionCatalog::new(vec![
            resolved_custom(0, wrong),
            resolved_custom(1, saved.clone()),
        ]);
        let context = ActionResolutionContext::new(ActionSurface::RadialMenu, "");
        let resolved = catalog
            .resolve(
                &reference(saved, action_ids::RESULT_EXECUTE),
                &UniversalActionRegistry,
                &context,
            )
            .unwrap();
        assert!(matches!(
            resolved.target.target,
            ActionTarget::CustomAction { index: 1, .. }
        ));
    }

    #[test]
    fn unknown_action_and_zero_macro_have_typed_failures() {
        let saved = action("Build", "build.exe");
        let catalog = PersistedActionCatalog::new(vec![resolved_custom(0, saved.clone())]);
        let context = ActionResolutionContext::new(ActionSurface::RadialMenu, "");
        assert!(matches!(
            catalog.resolve(
                &reference(saved, ActionId::new("missing.action")),
                &UniversalActionRegistry,
                &context
            ),
            Err(PersistedActionUnavailable::ActionMissing { .. })
        ));
        let macro_ref = PersistedUniversalActionRef {
            target: Some(PersistableActionTargetRef::MkMacro { id: 0 }),
            action_id: action_ids::MKMACRO_RUN,
        };
        assert!(matches!(
            catalog.resolve(&macro_ref, &UniversalActionRegistry, &context),
            Err(PersistedActionUnavailable::InvalidStableIdentity { kind: "MkMacro" })
        ));
    }
}
