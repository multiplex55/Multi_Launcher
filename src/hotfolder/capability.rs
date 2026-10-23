use super::execution::HotFolderExternalActionAdapter;
use super::model::{ConfigurationFingerprint, HotFolderRule, RuleAction};
use crate::actions::Action;
use crate::commands::{Command, ExternalCommand, parse_action};
use crate::mkmacro::{MkAction, MkCondition, MkInvocation, MkMacro, MkMacroDocument};
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
    pub(crate) external_adapter: Option<&'a dyn HotFolderExternalActionAdapter>,
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
            external_adapter: None,
        }
    }

    pub fn with_external_action_adapter(
        mut self,
        adapter: &'a dyn HotFolderExternalActionAdapter,
    ) -> Self {
        self.external_adapter = Some(adapter);
        self
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

/// Bind a macro by its current ID and the exact validated transitive definition
/// closure that an unattended invocation would execute.
pub fn bind_mkmacro_action(
    document: &MkMacroDocument,
    macro_id: u64,
) -> Result<RuleAction, &'static str> {
    bind_mkmacro_action_with_dependencies(document, macro_id).map(|(action, _)| action)
}

pub(crate) fn bind_mkmacro_action_with_dependencies(
    document: &MkMacroDocument,
    macro_id: u64,
) -> Result<(RuleAction, Vec<MkMacro>), &'static str> {
    if macro_id == 0 {
        return Err("MkMacro ID must be non-zero");
    }
    let mut matches = document
        .macros
        .iter()
        .filter(|macro_| macro_.id == macro_id);
    let _macro = matches.next().ok_or("MkMacro no longer exists")?;
    if matches.next().is_some() {
        return Err("MkMacro ID is ambiguous in the current document");
    }
    let invocation = MkInvocation::new(macro_id);
    let program = crate::mkmacro::invocation::compile_invocation_program(document, &invocation)
        .map_err(|_| "MkMacro dependency or parameter validation failed")?;
    let dependency_definitions = macro_dependency_definitions(document, &program)?;
    let configuration_fingerprint =
        macro_configuration_fingerprint(macro_id, &dependency_definitions)
            .map_err(|_| "MkMacro dependency configuration could not be fingerprinted")?;
    Ok((
        RuleAction::MkMacro {
            macro_id,
            configuration_fingerprint,
        },
        dependency_definitions,
    ))
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
        RuleAction::Copy { destination } => {
            if let Some(unavailable) = directory_capability(destination, context) {
                return unavailable;
            }
            ActionCapability::Supported {
                enablement: ActionEnablement::Standard,
            }
        }
        RuleAction::Move { destination } => {
            if let Some(unavailable) = directory_capability(destination, context) {
                return unavailable;
            }
            ActionCapability::Supported {
                enablement: ActionEnablement::StrongConfirmationRequired,
            }
        }
        RuleAction::Rename { .. } => ActionCapability::Supported {
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
            context,
        ),
    }
}

fn directory_capability(
    path: &Path,
    context: &HotFolderCapabilityContext<'_>,
) -> Option<ActionCapability> {
    match context.root_availability.get(path) {
        Some(RootAvailability::ExistingDirectory) => None,
        Some(RootAvailability::Missing) => Some(ActionCapability::Unavailable {
            reason: "action destination is missing",
        }),
        Some(RootAvailability::NotDirectory) => Some(ActionCapability::Invalid {
            reason: "action destination is not a directory",
        }),
        Some(RootAvailability::Unreadable) | None => Some(ActionCapability::Unavailable {
            reason: "action destination could not be inspected",
        }),
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

    let selected_action = RuleAction::RegisteredLauncherAction {
        reference: reference.clone(),
        configuration_fingerprint: saved_fingerprint.clone(),
    };
    if !context
        .external_adapter
        .is_some_and(|adapter| adapter.supports(&selected_action))
    {
        return ActionCapability::Unavailable {
            reason: "registered launcher completion bridge is unavailable",
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
    context: &HotFolderCapabilityContext<'_>,
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
    let invocation = MkInvocation::new(macro_id);
    let program =
        match crate::mkmacro::invocation::compile_invocation_program(document, &invocation) {
            Ok(program) => program,
            Err(_) => {
                return ActionCapability::Unsupported {
                    reason: "MkMacro dependency or parameter validation failed",
                };
            }
        };
    let Ok(dependency_definitions) = macro_dependency_definitions(document, &program) else {
        return ActionCapability::Invalid {
            reason: "MkMacro dependency closure is invalid",
        };
    };
    let Ok(current_fingerprint) =
        macro_configuration_fingerprint(macro_id, &dependency_definitions)
    else {
        return ActionCapability::Invalid {
            reason: "MkMacro dependency configuration could not be fingerprinted",
        };
    };
    if current_fingerprint != *saved_fingerprint {
        return ActionCapability::Unavailable {
            reason: "MkMacro configuration changed",
        };
    }

    let action = RuleAction::MkMacro {
        macro_id,
        configuration_fingerprint: saved_fingerprint.clone(),
    };
    if !context
        .external_adapter
        .is_some_and(|adapter| adapter.supports(&action))
    {
        return ActionCapability::Unavailable {
            reason: "MkMacro completion bridge is unavailable",
        };
    }

    let Some(root_plan) = program.plan(macro_id) else {
        return ActionCapability::Invalid {
            reason: "MkMacro compiled root is missing",
        };
    };
    let prepared = match crate::mkmacro::prepare_parameters(
        root_plan.signature.parameters(),
        root_plan.signature.outputs(),
        &invocation.arguments,
    ) {
        Ok(prepared) if prepared.missing.is_empty() => prepared,
        _ => {
            return ActionCapability::Unsupported {
                reason: "MkMacro requires invocation input with no unattended default",
            };
        }
    };
    if prepared
        .clone()
        .into_variables(root_plan.signature.parameters())
        .is_err()
    {
        return ActionCapability::Unsupported {
            reason: "MkMacro parameter defaults are invalid for unattended execution",
        };
    }

    let mut strong_confirmation = false;
    for macro_id in program.macro_ids() {
        let Some(plan) = program.plan(*macro_id) else {
            return ActionCapability::Invalid {
                reason: "MkMacro dependency plan is missing",
            };
        };
        for instruction in plan
            .instructions
            .iter()
            .filter(|instruction| instruction.step.enabled)
        {
            match &instruction.step.action {
                MkAction::CallMacro(_)
                | MkAction::Return(_)
                | MkAction::Delay(_)
                | MkAction::Notify(_)
                | MkAction::PlaySound(_)
                | MkAction::SetVariable { .. }
                | MkAction::UnsetVariable { .. }
                | MkAction::Else
                | MkAction::EndIf
                | MkAction::RepeatStart { .. }
                | MkAction::RepeatEnd
                | MkAction::WhileEnd
                | MkAction::Break
                | MkAction::Continue => {}
                MkAction::If(condition) | MkAction::WhileStart { condition }
                    if unattended_condition(condition) => {}
                MkAction::ReadClipboardText { .. } | MkAction::WriteClipboardText(_) => {
                    return ActionCapability::Unsupported {
                        reason: "MkMacro clipboard text actions are not allowed in unattended execution",
                    };
                }
                MkAction::Process(process) if !process.wait => {
                    strong_confirmation = true;
                }
                _ => {
                    return ActionCapability::Unsupported {
                        reason: "MkMacro dependency requires an interactive or unsupported action",
                    };
                }
            }
        }
    }

    ActionCapability::Supported {
        enablement: if strong_confirmation {
            ActionEnablement::StrongConfirmationRequired
        } else {
            ActionEnablement::Standard
        },
    }
}

fn unattended_condition(condition: &MkCondition) -> bool {
    match condition {
        MkCondition::Variable { .. } => true,
        MkCondition::All { conditions } | MkCondition::Any { conditions } => {
            conditions.iter().all(unattended_condition)
        }
        MkCondition::Not { condition } => unattended_condition(condition),
        _ => false,
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

fn macro_dependency_definitions(
    document: &MkMacroDocument,
    program: &crate::mkmacro::MkCompiledProgram,
) -> Result<Vec<MkMacro>, &'static str> {
    let mut dependency_definitions = Vec::with_capacity(program.macro_ids().len());
    for macro_id in program.macro_ids() {
        let mut matching = document
            .macros
            .iter()
            .filter(|macro_| macro_.id == *macro_id);
        let Some(macro_) = matching.next() else {
            return Err("macro dependency is missing");
        };
        if matching.next().is_some() {
            return Err("macro dependency is ambiguous");
        }
        dependency_definitions.push(macro_.clone());
    }
    Ok(dependency_definitions)
}

fn macro_configuration_fingerprint(
    root_macro_id: u64,
    dependency_definitions: &[MkMacro],
) -> Result<ConfigurationFingerprint, &'static str> {
    #[derive(Serialize)]
    struct FingerprintSource<'a> {
        root_macro_id: u64,
        dependency_definitions: Vec<&'a MkMacro>,
    }

    ConfigurationFingerprint::of_serializable(&FingerprintSource {
        root_macro_id,
        dependency_definitions: dependency_definitions.iter().collect(),
    })
    .map_err(|_| "macro dependency configuration could not be serialized")
}

pub(crate) fn is_registered_process_target(command: &ExternalCommand) -> bool {
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
    use crate::hotfolder::{
        HotFolderExecutionPhase, HotFolderExternalActionOutcome, HotFolderExternalActionRequest,
        HotFolderTerminalStatus,
    };
    use crate::universal_actions::{ActionResolutionContext, ActionSurface, ActionTarget};
    use std::collections::HashMap;

    struct AvailableAdapter;

    impl HotFolderExternalActionAdapter for AvailableAdapter {
        fn supports(&self, action: &RuleAction) -> bool {
            matches!(
                action,
                RuleAction::RegisteredLauncherAction { .. } | RuleAction::MkMacro { .. }
            )
        }

        fn dispatch(
            &self,
            request: HotFolderExternalActionRequest,
            _cancellation: &crate::file_operations::ScanCancellation,
        ) -> HotFolderExternalActionOutcome {
            HotFolderExternalActionOutcome {
                operation_id: request.operation_id,
                macro_run_id: None,
                status: HotFolderTerminalStatus::Success,
                phase: HotFolderExecutionPhase::Complete,
                reason: None,
            }
        }
    }

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
        let adapter = AvailableAdapter;
        let document = MkMacroDocument::default();
        let roots = roots();
        let selected = target(external_action("Editor", Some("--open")));
        let bound = bind_registered_launcher_action(&selected).unwrap();
        let rules = [selected.clone()];
        let context =
            capability_context(&rules, &document, &roots).with_external_action_adapter(&adapter);
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
        let changed_context = capability_context(&changed_targets, &document, &roots)
            .with_external_action_adapter(&adapter);
        assert!(matches!(
            evaluate_action_capability(&rule(bound.clone()), &changed_context),
            ActionCapability::Unavailable { .. }
        ));

        let ambiguous_targets = [selected.clone(), selected.clone()];
        let ambiguous_context = capability_context(&ambiguous_targets, &document, &roots)
            .with_external_action_adapter(&adapter);
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
        let unknown_context = capability_context(&unknown_targets, &document, &roots)
            .with_external_action_adapter(&adapter);
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
        let destination = std::env::temp_dir().join("target/productivity_expansion/app_data/out");
        let roots = HashMap::from([
            (root(), RootAvailability::ExistingDirectory),
            (destination.clone(), RootAvailability::ExistingDirectory),
        ]);
        let empty_targets = [];
        let context = capability_context(&empty_targets, &macro_document, &roots);

        let copy = rule(RuleAction::Copy {
            destination: destination.clone(),
        });
        assert_eq!(
            evaluate_action_capability(&copy, &context),
            ActionCapability::Supported {
                enablement: ActionEnablement::Standard,
            }
        );
        let move_rule = rule(RuleAction::Move { destination });
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

    #[test]
    fn macro_transitive_change_invalidates_fingerprint_and_recomputes_strong_enablement() {
        use crate::mkmacro::{
            MkAction, MkCallMacroPayload, MkDelayPayload, MkMacro, MkMacroDocument,
            MkProcessPayload, MkStep,
        };

        let step = |id, action| MkStep {
            metadata: Default::default(),
            id,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action,
        };
        let parent = MkMacro {
            signature: Default::default(),
            id: 7,
            name: "parent".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            steps: vec![step(
                1,
                MkAction::CallMacro(MkCallMacroPayload {
                    macro_id: 8,
                    ..Default::default()
                }),
            )],
        };
        let child = MkMacro {
            signature: Default::default(),
            id: 8,
            name: "child".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            steps: vec![step(1, MkAction::Delay(MkDelayPayload::default()))],
        };
        let original = MkMacroDocument {
            macros: vec![parent.clone(), child.clone()],
            ..Default::default()
        };
        let old_action = bind_mkmacro_action(&original, 7).unwrap();
        let adapter = AvailableAdapter;
        let empty_targets = [];
        let roots = roots();
        let original_context = capability_context(&empty_targets, &original, &roots)
            .with_external_action_adapter(&adapter);
        assert_eq!(
            evaluate_action_capability(&rule(old_action.clone()), &original_context),
            ActionCapability::Supported {
                enablement: ActionEnablement::Standard,
            }
        );

        let mut changed_child = child;
        changed_child.steps[0].action = MkAction::Process(MkProcessPayload {
            program: "C:\\Tools\\worker.exe".into(),
            arguments: Vec::new(),
            working_directory: None,
            wait: false,
        });
        let changed = MkMacroDocument {
            macros: vec![parent, changed_child],
            ..Default::default()
        };
        let changed_action = bind_mkmacro_action(&changed, 7).unwrap();
        assert_ne!(old_action, changed_action);

        let changed_context = capability_context(&empty_targets, &changed, &roots)
            .with_external_action_adapter(&adapter);
        assert!(matches!(
            evaluate_action_capability(&rule(old_action), &changed_context),
            ActionCapability::Unavailable { .. }
        ));
        assert_eq!(
            evaluate_action_capability(&rule(changed_action), &changed_context),
            ActionCapability::Supported {
                enablement: ActionEnablement::StrongConfirmationRequired,
            }
        );
    }

    #[test]
    fn clipboard_action_policy_rejects_enabled_direct_and_nested_macros_only() {
        use crate::mkmacro::{
            MkAction, MkCallMacroPayload, MkMacro, MkMacroDocument, MkStep,
            MkWriteClipboardTextPayload,
        };

        let step = |id, action, enabled| MkStep {
            metadata: Default::default(),
            id,
            enabled,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action,
        };
        let macro_ = |id, name: &str, steps| MkMacro {
            signature: Default::default(),
            id,
            name: name.into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            steps,
        };
        let read = MkAction::ReadClipboardText {
            variable: "clipboard_text".into(),
        };
        let write = MkAction::WriteClipboardText(MkWriteClipboardTextPayload {
            source: crate::mkmacro::MkValueSource::Literal(crate::mkmacro::MkValue::String(
                "plain text".into(),
            )),
        });
        let adapter = AvailableAdapter;
        let empty_targets = [];
        let roots = roots();

        for action in [read.clone(), write.clone()] {
            let document = MkMacroDocument {
                macros: vec![macro_(7, "direct", vec![step(1, action, true)])],
                ..Default::default()
            };
            let bound = bind_mkmacro_action(&document, 7).unwrap();
            let context = capability_context(&empty_targets, &document, &roots)
                .with_external_action_adapter(&adapter);
            assert!(matches!(
                evaluate_action_capability(&rule(bound), &context),
                ActionCapability::Unsupported { reason }
                    if reason.contains("clipboard text")
            ));
        }

        let nested_document = MkMacroDocument {
            macros: vec![
                macro_(
                    7,
                    "parent",
                    vec![step(
                        1,
                        MkAction::CallMacro(MkCallMacroPayload {
                            macro_id: 8,
                            ..Default::default()
                        }),
                        true,
                    )],
                ),
                macro_(8, "child", vec![step(1, write.clone(), true)]),
            ],
            ..Default::default()
        };
        let bound = bind_mkmacro_action(&nested_document, 7).unwrap();
        let context = capability_context(&empty_targets, &nested_document, &roots)
            .with_external_action_adapter(&adapter);
        assert!(matches!(
            evaluate_action_capability(&rule(bound), &context),
            ActionCapability::Unsupported { reason }
                if reason.contains("clipboard text")
        ));

        let disabled_step_document = MkMacroDocument {
            macros: vec![macro_(
                7,
                "disabled step",
                vec![step(1, write.clone(), false)],
            )],
            ..Default::default()
        };
        let bound = bind_mkmacro_action(&disabled_step_document, 7).unwrap();
        let context = capability_context(&empty_targets, &disabled_step_document, &roots)
            .with_external_action_adapter(&adapter);
        assert_eq!(
            evaluate_action_capability(&rule(bound), &context),
            ActionCapability::Supported {
                enablement: ActionEnablement::Standard,
            }
        );

        let mut disabled_macro_document = MkMacroDocument {
            macros: vec![macro_(7, "disabled macro", vec![step(1, write, true)])],
            ..Default::default()
        };
        let bound = bind_mkmacro_action(&disabled_macro_document, 7).unwrap();
        disabled_macro_document.macros[0].enabled = false;
        assert!(bind_mkmacro_action(&disabled_macro_document, 7).is_err());
        let context = capability_context(&empty_targets, &disabled_macro_document, &roots)
            .with_external_action_adapter(&adapter);
        assert!(matches!(
            evaluate_action_capability(&rule(bound), &context),
            ActionCapability::Unsupported {
                reason: "MkMacro is disabled in the current document"
            }
        ));
    }
}
