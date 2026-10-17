use super::model::{ConfigurationFingerprint, HotFolderRule, RuleAction};
use crate::actions::Action;
use crate::commands::{Command, ExternalCommand, parse_action};
use crate::mkmacro::{MkMacro, MkMacroDocument};
use crate::universal_actions::{
    ActionResolutionContext, ActionTarget, PersistableActionTargetRef, PersistedUniversalActionRef,
    ResolvedActionTarget, UniversalActionOperation, UniversalActionRegistry, action_ids,
};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootAvailability {
    ExistingDirectory,
    Missing,
    NotDirectory,
    Unreadable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionEnablement {
    Standard,
    StrongConfirmationRequired,
}

/// Read-only inputs built by a later lifecycle boundary from current in-memory
/// catalogs and root metadata. No path is opened and no action is dispatched.
pub struct HotFolderCapabilityContext<'a> {
    pub launcher_targets: &'a [ResolvedActionTarget],
    pub action_resolution: ActionResolutionContext<'a>,
    pub mkmacro_document: &'a MkMacroDocument,
    pub root_availability: &'a HashMap<PathBuf, RootAvailability>,
}

impl<'a> HotFolderCapabilityContext<'a> {
    pub fn new(
        launcher_targets: &'a [ResolvedActionTarget],
        action_resolution: ActionResolutionContext<'a>,
        mkmacro_document: &'a MkMacroDocument,
        root_availability: &'a HashMap<PathBuf, RootAvailability>,
    ) -> Self {
        Self {
            launcher_targets,
            action_resolution,
            mkmacro_document,
            root_availability,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionCapability {
    Supported { enablement: ActionEnablement },
    Unsupported { reason: &'static str },
    Unavailable { reason: &'static str },
    Invalid { reason: &'static str },
}

/// Build a persisted reference to the exact registered primary launcher action
/// represented by the supplied current target snapshot. Capability is still
/// recomputed when the reference is later evaluated.
pub fn bind_registered_launcher_action(
    target: &ResolvedActionTarget,
) -> Result<RuleAction, &'static str> {
    let target_ref = target
        .target
        .persistent_ref()
        .ok_or("launcher target does not have a stable persistent identity")?;
    let action_id = action_ids::RESULT_EXECUTE;
    let reference = PersistedUniversalActionRef {
        target: Some(target_ref.clone()),
        action_id: action_id.clone(),
    };
    let fingerprint =
        action_configuration_fingerprint(&target_ref, &action_id, &target.selected_action)
            .map_err(|_| "launcher action configuration could not be fingerprinted")?;
    Ok(RuleAction::RegisteredLauncherAction {
        reference,
        configuration_fingerprint: fingerprint,
    })
}

/// Bind a macro by its current non-zero document ID and exact saved macro
/// configuration. The document is only inspected; no compilation or execution
/// is attempted at this boundary.
pub fn bind_mkmacro_action(
    document: &MkMacroDocument,
    macro_id: u64,
) -> Result<RuleAction, &'static str> {
    if macro_id == 0 {
        return Err("MkMacro ID must be non-zero");
    }
    let mut matches = document
        .macros
        .iter()
        .filter(|macro_| macro_.id == macro_id);
    let macro_ = matches.next().ok_or("MkMacro no longer exists")?;
    if matches.next().is_some() {
        return Err("MkMacro ID is ambiguous in the current document");
    }
    let configuration_fingerprint = macro_configuration_fingerprint(macro_)
        .map_err(|_| "MkMacro configuration could not be fingerprinted")?;
    Ok(RuleAction::MkMacro {
        macro_id,
        configuration_fingerprint,
    })
}

pub fn evaluate_action_capability(
    rule: &HotFolderRule,
    context: &HotFolderCapabilityContext<'_>,
) -> ActionCapability {
    if let Err(reason) = rule.validate() {
        return ActionCapability::Invalid { reason };
    }
    match context.root_availability.get(&rule.watched_root) {
        Some(RootAvailability::ExistingDirectory) => {}
        Some(RootAvailability::Missing) => {
            return ActionCapability::Unavailable {
                reason: "watched root is missing",
            };
        }
        Some(RootAvailability::NotDirectory) => {
            return ActionCapability::Invalid {
                reason: "watched root is not a directory",
            };
        }
        Some(RootAvailability::Unreadable) => {
            return ActionCapability::Unavailable {
                reason: "watched root could not be inspected",
            };
        }
        None => {
            return ActionCapability::Unavailable {
                reason: "watched root availability has not been checked",
            };
        }
    }

    match &rule.action {
        RuleAction::Copy { .. } => ActionCapability::Supported {
            enablement: ActionEnablement::Standard,
        },
        RuleAction::Move { .. } | RuleAction::Rename { .. } => ActionCapability::Supported {
            enablement: ActionEnablement::StrongConfirmationRequired,
        },
        RuleAction::RegisteredLauncherAction {
            reference,
            configuration_fingerprint,
        } => evaluate_registered_action(reference, configuration_fingerprint, context),
        RuleAction::MkMacro {
            macro_id,
            configuration_fingerprint,
        } => evaluate_mkmacro(
            *macro_id,
            configuration_fingerprint,
            context.mkmacro_document,
        ),
    }
}

fn evaluate_registered_action(
    reference: &PersistedUniversalActionRef,
    saved_fingerprint: &ConfigurationFingerprint,
    context: &HotFolderCapabilityContext<'_>,
) -> ActionCapability {
    if reference.action_id != action_ids::RESULT_EXECUTE {
        return ActionCapability::Unsupported {
            reason: "only a registered primary launcher action is supported for unattended use",
        };
    }
    let Some(saved_target) = reference.target.as_ref() else {
        return ActionCapability::Invalid {
            reason: "launcher action reference has no stable target",
        };
    };

    let same_identity = context
        .launcher_targets
        .iter()
        .filter(|entry| entry.target.persistent_ref().as_ref() == Some(saved_target))
        .collect::<Vec<_>>();
    if same_identity.is_empty() {
        return ActionCapability::Unavailable {
            reason: "registered launcher target is missing",
        };
    }

    let mut exact = Vec::new();
    for entry in &same_identity {
        let Ok(current_fingerprint) = action_configuration_fingerprint(
            saved_target,
            &reference.action_id,
            &entry.selected_action,
        ) else {
            return ActionCapability::Invalid {
                reason: "registered launcher configuration could not be fingerprinted",
            };
        };
        if current_fingerprint == *saved_fingerprint {
            exact.push(*entry);
        }
    }

    if exact.is_empty() {
        return ActionCapability::Unavailable {
            reason: "registered launcher configuration changed",
        };
    }
    if exact.len() != 1 {
        return ActionCapability::Invalid {
            reason: "registered launcher target is ambiguous",
        };
    }

    let entry = exact[0];
    let actions = UniversalActionRegistry.resolve(entry, &context.action_resolution);
    let selected = actions
        .iter()
        .find(|action| action.id == reference.action_id);
    let Some(selected) = selected else {
        return ActionCapability::Unavailable {
            reason: "registered launcher action is no longer available",
        };
    };
    if !selected.is_available() {
        return ActionCapability::Unavailable {
            reason: "registered launcher action is currently disabled",
        };
    }

    let primary = match &selected.operation {
        UniversalActionOperation::InvokePrimary(action) => action,
        UniversalActionOperation::Command { .. } | UniversalActionOperation::UiIntent(_) => {
            return ActionCapability::Unsupported {
                reason: "selected launcher action requires a UI or non-primary command path",
            };
        }
    };
    let command = match parse_action(primary) {
        Ok(command) => command,
        Err(_) => {
            return ActionCapability::Unsupported {
                reason: "selected launcher action has invalid command semantics",
            };
        }
    };
    let Command::External(external) = command else {
        return ActionCapability::Unsupported {
            reason: "selected launcher action is not declared safe for unattended execution",
        };
    };
    if !is_registered_process_target(&external) {
        return ActionCapability::Unsupported {
            reason: "unknown commands, URLs, and UI-dependent targets are not supported",
        };
    }

    ActionCapability::Supported {
        enablement: ActionEnablement::StrongConfirmationRequired,
    }
}

fn evaluate_mkmacro(
    macro_id: u64,
    saved_fingerprint: &ConfigurationFingerprint,
    document: &MkMacroDocument,
) -> ActionCapability {
    let mut matches = document
        .macros
        .iter()
        .filter(|macro_| macro_.id == macro_id);
    let Some(current) = matches.next() else {
        return ActionCapability::Unavailable {
            reason: "MkMacro no longer exists",
        };
    };
    if matches.next().is_some() {
        return ActionCapability::Invalid {
            reason: "MkMacro ID is ambiguous in the current document",
        };
    }
    if !current.enabled {
        return ActionCapability::Unsupported {
            reason: "MkMacro is disabled in the current document",
        };
    }
    let Ok(current_fingerprint) = macro_configuration_fingerprint(current) else {
        return ActionCapability::Invalid {
            reason: "MkMacro configuration could not be fingerprinted",
        };
    };
    if current_fingerprint != *saved_fingerprint {
        return ActionCapability::Unavailable {
            reason: "MkMacro configuration changed",
        };
    }

    // Transitive macro validation and correlated completion belong to the
    // later runtime bridge. A resolved ID alone is not an unattended-safety
    // claim.
    ActionCapability::Unavailable {
        reason: "MkMacro unattended capability requires runtime validation",
    }
}

fn action_configuration_fingerprint(
    target: &PersistableActionTargetRef,
    action_id: &crate::universal_actions::ActionId,
    selected_action: &Action,
) -> Result<ConfigurationFingerprint, serde_json::Error> {
    #[derive(Serialize)]
    struct FingerprintSource<'a> {
        target: &'a PersistableActionTargetRef,
        action_id: &'a crate::universal_actions::ActionId,
        selected_action: &'a Action,
    }
    ConfigurationFingerprint::of_serializable(&FingerprintSource {
        target,
        action_id,
        selected_action,
    })
}

fn macro_configuration_fingerprint(
    macro_: &MkMacro,
) -> Result<ConfigurationFingerprint, serde_json::Error> {
    ConfigurationFingerprint::of_serializable(macro_)
}

fn is_registered_process_target(command: &ExternalCommand) -> bool {
    if command.namespace.is_some() {
        return false;
    }
    let target = command.target.trim();
    if target.is_empty()
        || target.chars().count() > super::model::MAX_EXTERNAL_ACTION_TEXT_CHARS
        || target.chars().any(char::is_control)
    {
        return false;
    }
    if has_unknown_protocol_prefix(target) {
        return false;
    }

    let extension = Path::new(target)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    let process_extension = matches!(
        extension.as_deref(),
        Some("exe" | "com" | "bat" | "cmd" | "ps1" | "py" | "sh")
    );
    if !process_extension {
        return false;
    }

    // The existing launcher invokes an explicit target and tokenized args;
    // it does not interpolate watched file names or route through a shell.
    command.args.as_ref().is_none_or(|args| {
        args.chars().count() <= super::model::MAX_EXTERNAL_ACTION_TEXT_CHARS
            && !args.chars().any(char::is_control)
    })
}

fn has_unknown_protocol_prefix(target: &str) -> bool {
    let Some((scheme, _)) = target.split_once(':') else {
        return false;
    };
    if scheme.len() == 1 && scheme.as_bytes()[0].is_ascii_alphabetic() {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universal_actions::{ActionResolutionContext, ActionSurface, ActionTarget};
    use std::collections::HashMap;

    fn root() -> PathBuf {
        std::env::temp_dir().join("target/productivity_expansion/watch/incoming")
    }

    fn target(action: Action) -> ResolvedActionTarget {
        ResolvedActionTarget {
            target: ActionTarget::Folder {
                path: "C:\\Fixture\\source".into(),
            },
            selected_action: action,
            custom_action_index: None,
        }
    }

    fn external_action(label: &str, args: Option<&str>) -> Action {
        Action {
            label: label.into(),
            desc: "Application".into(),
            action: "C:\\Tools\\editor.exe".into(),
            args: args.map(str::to_owned),
        }
    }

    fn capability_context<'a>(
        launcher_targets: &'a [ResolvedActionTarget],
        document: &'a MkMacroDocument,
        roots: &'a HashMap<PathBuf, RootAvailability>,
    ) -> HotFolderCapabilityContext<'a> {
        HotFolderCapabilityContext::new(
            launcher_targets,
            ActionResolutionContext::new(ActionSurface::LauncherList, ""),
            document,
            roots,
        )
    }

    fn rule(action: RuleAction) -> HotFolderRule {
        HotFolderRule::new("Rule", root(), action)
    }

    fn roots() -> HashMap<PathBuf, RootAvailability> {
        HashMap::from([(root(), RootAvailability::ExistingDirectory)])
    }

    #[test]
    fn capability_resolves_only_exact_unattended_registered_process_actions() {
        let document = MkMacroDocument::default();
        let roots = roots();
        let selected = target(external_action("Editor", Some("--open")));
        let bound = bind_registered_launcher_action(&selected).unwrap();
        let rules = [selected.clone()];
        let context = capability_context(&rules, &document, &roots);
        assert_eq!(
            evaluate_action_capability(&rule(bound.clone()), &context),
            ActionCapability::Supported {
                enablement: ActionEnablement::StrongConfirmationRequired,
            }
        );

        let missing = capability_context(&[], &document, &roots);
        assert!(matches!(
            evaluate_action_capability(&rule(bound.clone()), &missing),
            ActionCapability::Unavailable { .. }
        ));

        let mut changed = selected.clone();
        changed.selected_action.args = Some("--different".into());
        let changed_targets = [changed];
        let changed_context = capability_context(&changed_targets, &document, &roots);
        assert!(matches!(
            evaluate_action_capability(&rule(bound.clone()), &changed_context),
            ActionCapability::Unavailable { .. }
        ));

        let ambiguous_targets = [selected.clone(), selected.clone()];
        let ambiguous_context = capability_context(&ambiguous_targets, &document, &roots);
        assert!(matches!(
            evaluate_action_capability(&rule(bound), &ambiguous_context),
            ActionCapability::Invalid { .. }
        ));

        let unknown = target(Action {
            label: "Unknown".into(),
            desc: "Application".into(),
            action: "unknown:future-command".into(),
            args: None,
        });
        let unknown_ref = bind_registered_launcher_action(&unknown).unwrap();
        let unknown_targets = [unknown];
        let unknown_context = capability_context(&unknown_targets, &document, &roots);
        assert!(matches!(
            evaluate_action_capability(&rule(unknown_ref), &unknown_context),
            ActionCapability::Unsupported { .. }
        ));
    }

    #[test]
    fn capability_marks_filesystem_risk_and_macro_staleness_without_dispatch() {
        let mut macro_document = MkMacroDocument::default();
        macro_document.macros.push(MkMacro {
            id: 7,
            name: "Fixture macro".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            signature: Default::default(),
            steps: Vec::new(),
        });
        let roots = roots();
        let empty_targets = [];
        let context = capability_context(&empty_targets, &macro_document, &roots);

        let copy = rule(RuleAction::Copy {
            destination: std::env::temp_dir().join("target/productivity_expansion/app_data/out"),
        });
        assert_eq!(
            evaluate_action_capability(&copy, &context),
            ActionCapability::Supported {
                enablement: ActionEnablement::Standard,
            }
        );
        let move_rule = rule(RuleAction::Move {
            destination: std::env::temp_dir().join("target/productivity_expansion/app_data/out"),
        });
        assert_eq!(
            evaluate_action_capability(&move_rule, &context),
            ActionCapability::Supported {
                enablement: ActionEnablement::StrongConfirmationRequired,
            }
        );

        let macro_action = bind_mkmacro_action(&macro_document, 7).unwrap();
        assert!(matches!(
            evaluate_action_capability(&rule(macro_action.clone()), &context),
            ActionCapability::Unavailable { .. }
        ));
        let changed_document = MkMacroDocument::default();
        let changed_context = capability_context(&empty_targets, &changed_document, &roots);
        assert!(matches!(
            evaluate_action_capability(&rule(macro_action), &changed_context),
            ActionCapability::Unavailable { .. }
        ));
    }
}
