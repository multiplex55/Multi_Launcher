//! Transient typed invocation and pure binding preparation, shared by runtime
//! admission and the direct-invocation UI. Defaults are values, never templates.
use super::{
    compiler::{MkCompiledProgram, MkCompiledSignature},
    executor::{DiagnosticKind, ExecResult, ExecutionDiagnostic, ExecutionMode},
    model::*,
    variables::*,
};
use std::collections::{BTreeMap, HashSet};

pub type MkInvocationValues = BTreeMap<MkSignatureId, MkValue>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MkInvocationSubset {
    #[default]
    Whole,
    From(u64),
    Selected(Vec<u64>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MkInvocation {
    pub macro_id: u64,
    pub arguments: MkInvocationValues,
    pub mode: ExecutionMode,
    pub subset: MkInvocationSubset,
}
impl MkInvocation {
    pub fn new(macro_id: u64) -> Self {
        Self {
            macro_id,
            arguments: BTreeMap::new(),
            mode: ExecutionMode::Normal,
            subset: MkInvocationSubset::Whole,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MkParameterPreparation {
    pub values: MkInvocationValues,
    pub missing: Vec<MkMacroParameter>,
}
impl MkParameterPreparation {
    pub fn into_variables(self, parameters: &[MkMacroParameter]) -> ExecResult<RuntimeVariables> {
        if !self.missing.is_empty() {
            return Err(binding_error(format!(
                "Required macro parameters are missing: {}",
                self.missing
                    .iter()
                    .map(|p| format!("{} (#{})", p.name, p.id.0))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        parameters
            .iter()
            .map(|p| {
                self.values
                    .get(&p.id)
                    .cloned()
                    .map(|v| (p.name.clone(), v))
                    .ok_or_else(|| {
                        binding_error(format!("Parameter #{} has no prepared value", p.id.0))
                    })
            })
            .collect()
    }
}

/// Applies defaults and exact types without effects or access to caller locals.
/// Missing required definitions are data so a UI can request them later.
pub fn prepare_parameters(
    parameters: &[MkMacroParameter],
    outputs: &[MkMacroOutput],
    supplied: &MkInvocationValues,
) -> ExecResult<MkParameterPreparation> {
    let mut diagnostics = Vec::new();
    super::reusable_validation::signature_parts(0, parameters, outputs, &mut diagnostics);
    if let Some(diagnostic) = diagnostics.first() {
        return Err(binding_error(&diagnostic.message));
    }
    let ids: HashSet<_> = parameters.iter().map(|p| p.id).collect();
    if let Some(id) = supplied.keys().find(|id| !ids.contains(id)) {
        return Err(binding_error(format!("Unknown parameter #{}", id.0)));
    }
    let mut values = BTreeMap::new();
    let mut missing = Vec::new();
    for parameter in parameters {
        if let Some(value) = supplied
            .get(&parameter.id)
            .or(parameter.default_value.as_ref())
        {
            require_type(parameter.value_type, value, &parameter.name)?;
            if let Some(reason) =
                super::reusable_validation::parameter_value_error(parameter, value)
            {
                return Err(binding_error(format!(
                    "Parameter '{}' is invalid: {reason}",
                    parameter.name
                )));
            }
            values.insert(parameter.id, value.clone());
        } else {
            missing.push(parameter.clone());
        }
    }
    Ok(MkParameterPreparation { values, missing })
}

pub(crate) fn resolve_source(
    source: &MkValueSource,
    variables: &RuntimeVariables,
) -> ExecResult<MkValue> {
    match source {
        MkValueSource::Literal(MkValue::String(template)) => {
            super::interpolate(template, variables).map(MkValue::String)
        }
        MkValueSource::Literal(value) => Ok(value.clone()),
        MkValueSource::Variable { name } => {
            validate_variable_reference(name).map_err(binding_error)?;
            variables
                .get(name)
                .cloned()
                .ok_or_else(|| binding_error(format!("Variable '{name}' is not defined")))
        }
    }
}

pub(crate) fn prepare_call(
    signature: &MkCompiledSignature,
    call: &MkCallMacroPayload,
    variables: &RuntimeVariables,
) -> ExecResult<RuntimeVariables> {
    validate_output_mappings(signature, &call.outputs)?;
    let mut supplied = BTreeMap::new();
    for binding in &call.arguments {
        if supplied.contains_key(&binding.parameter_id) {
            return Err(binding_error(format!(
                "Duplicate argument #{}",
                binding.parameter_id.0
            )));
        }
        if signature.parameter(binding.parameter_id).is_none() {
            return Err(binding_error(format!(
                "Unknown parameter #{}",
                binding.parameter_id.0
            )));
        }
        supplied.insert(
            binding.parameter_id,
            resolve_source(&binding.source, variables)?,
        );
    }
    prepare_parameters(signature.parameters(), signature.outputs(), &supplied)?
        .into_variables(signature.parameters())
}

pub(crate) fn prepare_return(
    signature: &MkCompiledSignature,
    payload: &MkReturnPayload,
    variables: &RuntimeVariables,
) -> ExecResult<MkInvocationValues> {
    let mut values = BTreeMap::new();
    for binding in &payload.outputs {
        let output = signature.output(binding.output_id).ok_or_else(|| {
            binding_error(format!("Unknown Return output #{}", binding.output_id.0))
        })?;
        if values.contains_key(&output.id) {
            return Err(binding_error(format!(
                "Duplicate Return output #{}",
                output.id.0
            )));
        }
        let value = resolve_source(&binding.source, variables)?;
        require_type(output.value_type, &value, &output.name)?;
        values.insert(output.id, value);
    }
    for output in signature.outputs().iter() {
        if !values.contains_key(&output.id) {
            return Err(binding_error(format!(
                "Return is missing output '{}' (#{})",
                output.name, output.id.0
            )));
        }
    }
    Ok(values)
}

fn validate_output_mappings(
    signature: &MkCompiledSignature,
    bindings: &[MkCallOutputBinding],
) -> ExecResult {
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for binding in bindings {
        if signature.output(binding.output_id).is_none() {
            return Err(binding_error(format!(
                "Unknown mapped output #{}",
                binding.output_id.0
            )));
        }
        validate_variable_name(&binding.caller_variable).map_err(binding_error)?;
        if !ids.insert(binding.output_id) || !names.insert(&binding.caller_variable) {
            return Err(binding_error("Duplicate output mapping or destination"));
        }
    }
    Ok(())
}

/// Prepares every write before the caller mutates any variable.
pub(crate) fn prepare_output_writes(
    signature: &MkCompiledSignature,
    bindings: &[MkCallOutputBinding],
    returned: &MkInvocationValues,
) -> ExecResult<RuntimeVariables> {
    validate_output_mappings(signature, bindings)?;
    let mut writes = BTreeMap::new();
    for output in signature.outputs().iter() {
        let value = returned.get(&output.id).ok_or_else(|| {
            binding_error(format!("Callee did not return output '{}'", output.name))
        })?;
        require_type(output.value_type, value, &output.name)?;
    }
    for binding in bindings {
        let value = returned
            .get(&binding.output_id)
            .ok_or_else(|| binding_error("Mapped output has no returned value"))?;
        writes.insert(binding.caller_variable.clone(), value.clone());
    }
    Ok(writes)
}

fn require_type(kind: MkValueType, value: &MkValue, name: &str) -> ExecResult {
    if kind.accepts(value) {
        Ok(())
    } else {
        Err(ExecutionDiagnostic::new(
            DiagnosticKind::TypeMismatch,
            format!("'{name}' requires {}", kind.label()),
        ))
    }
}
fn binding_error(message: impl Into<String>) -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(DiagnosticKind::InvalidPlan, message)
}

/// Captures one document version for admission. Execution uses this same
/// immutable program even if authoring publishes a newer document afterward.
pub(crate) fn compile_invocation_program(
    document: &MkMacroDocument,
    invocation: &MkInvocation,
) -> ExecResult<MkCompiledProgram> {
    let target = document
        .macros
        .iter()
        .find(|m| m.id == invocation.macro_id)
        .ok_or_else(|| {
            ExecutionDiagnostic::new(
                DiagnosticKind::TargetNotFound,
                format!("macro {} was not found", invocation.macro_id),
            )
        })?;
    if !target.enabled {
        return Err(ExecutionDiagnostic::new(
            DiagnosticKind::InvalidTarget,
            "macro is disabled",
        ));
    }
    let mut program =
        super::compile_program(document, invocation.macro_id).map_err(|diagnostics| {
            binding_error(format!(
                "macro validation failed: {}",
                diagnostics
                    .iter()
                    .find(|d| d.severity == super::DiagnosticSeverity::Fatal)
                    .map(|d| d.message.as_str())
                    .unwrap_or("invalid plan")
            ))
        })?;
    apply_root_subset(&mut program, &invocation.subset)?;
    Ok(program)
}

/// Names, descriptions and ordering are presentation. IDs, types, defaults,
/// and input metadata are the contract the user saw when input was requested.
pub(crate) fn validate_parameter_assumptions(
    previous: &[MkMacroParameter],
    current: &[MkMacroParameter],
) -> ExecResult {
    let current: BTreeMap<_, _> = current.iter().map(|p| (p.id, p)).collect();
    if previous.len() != current.len()
        || previous.iter().any(|old| {
            current.get(&old.id).is_none_or(|new| {
                new.value_type != old.value_type
                    || new.default_value != old.default_value
                    || new.input_kind != old.input_kind
            })
        })
    {
        return Err(binding_error(
            "Macro parameters changed while input was pending; run the macro again",
        ));
    }
    Ok(())
}

/// Restricts only the root, preserving the historical rejection of structured
/// subsets and all complete callee plans captured by compilation.
pub fn apply_root_subset(
    program: &mut MkCompiledProgram,
    subset: &MkInvocationSubset,
) -> ExecResult {
    let plan = program
        .root_plan_mut()
        .ok_or_else(|| binding_error("Program root is missing"))?;
    let invalid = |message| ExecutionDiagnostic::new(DiagnosticKind::InvalidSelection, message);
    match subset {
        MkInvocationSubset::Whole => return Ok(()),
        MkInvocationSubset::From(id) => {
            if plan
                .instructions
                .iter()
                .any(|x| x.step.action.is_structural())
            {
                return Err(invalid(
                    "run-from cannot enter a structured control-flow plan",
                ));
            }
            let start = *plan.step_to_instruction.get(id).ok_or_else(|| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::TargetNotFound,
                    format!("step {id} was not found"),
                )
            })?;
            plan.instructions = plan.instructions[start..].to_vec().into();
        }
        MkInvocationSubset::Selected(ids) => {
            if ids.is_empty() {
                return Err(invalid("selection is empty"));
            }
            let wanted: HashSet<_> = ids.iter().copied().collect();
            if wanted
                .iter()
                .any(|id| !plan.step_to_instruction.contains_key(id))
            {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::TargetNotFound,
                    "selection contains an unknown step",
                ));
            }
            if plan
                .instructions
                .iter()
                .any(|x| x.step.action.is_structural())
            {
                return Err(invalid(
                    "structural selections must include a complete executable plan",
                ));
            }
            plan.instructions = plan
                .instructions
                .iter()
                .filter(|x| wanted.contains(&x.step.id))
                .cloned()
                .collect::<Vec<_>>()
                .into();
        }
    }
    plan.step_to_instruction = plan
        .instructions
        .iter()
        .enumerate()
        .map(|(i, x)| (x.step.id, i))
        .collect();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parameter(id: u64, name: &str, default_value: Option<MkValue>) -> MkMacroParameter {
        MkMacroParameter {
            id: MkSignatureId(id),
            name: name.into(),
            value_type: MkValueType::String,
            input_kind: Default::default(),
            description: String::new(),
            default_value,
        }
    }

    #[test]
    fn preparation_reports_missing_ids_applies_literal_defaults_and_rejects_stale_types() {
        let parameters = vec![
            parameter(3, "required", None),
            parameter(8, "defaulted", Some(MkValue::String("${caller}".into()))),
        ];
        let prepared = prepare_parameters(&parameters, &[], &BTreeMap::new()).unwrap();
        assert_eq!(prepared.missing, [parameters[0].clone()]);
        assert_eq!(
            prepared.values[&MkSignatureId(8)],
            MkValue::String("${caller}".into())
        );
        assert!(prepared.into_variables(&parameters).is_err());
        let values = [(MkSignatureId(3), MkValue::String("provided".into()))]
            .into_iter()
            .collect();
        let locals = prepare_parameters(&parameters, &[], &values)
            .unwrap()
            .into_variables(&parameters)
            .unwrap();
        assert_eq!(locals["required"], MkValue::String("provided".into()));
        assert_eq!(locals["defaulted"], MkValue::String("${caller}".into()));
        let wrong_type = [(MkSignatureId(3), MkValue::Number(1.0))]
            .into_iter()
            .collect();
        assert_eq!(
            prepare_parameters(&parameters, &[], &wrong_type)
                .unwrap_err()
                .kind,
            DiagnosticKind::TypeMismatch
        );
        let removed = [(MkSignatureId(4), MkValue::String("provided".into()))]
            .into_iter()
            .collect();
        assert!(prepare_parameters(&parameters, &[], &removed).is_err());
        let mut duplicate = parameters.clone();
        duplicate[1].id = duplicate[0].id;
        assert!(prepare_parameters(&duplicate, &[], &values).is_err());
    }

    #[test]
    fn parameter_input_metadata_prepares_exact_choices_and_portable_paths() {
        let mut choice = parameter(21, "mode", None);
        choice.input_kind = MkParameterInputKind::Choice(vec!["First".into(), "選択肢二".into()]);
        let choice_value = [(choice.id, MkValue::String("選択肢二".into()))]
            .into_iter()
            .collect();
        assert!(prepare_parameters(&[choice.clone()], &[], &choice_value).is_ok());
        let wrong_choice = [(choice.id, MkValue::String("選択肢二 ".into()))]
            .into_iter()
            .collect();
        assert!(
            prepare_parameters(&[choice], &[], &wrong_choice)
                .unwrap_err()
                .message
                .contains("not one of the configured choices")
        );

        for (id, input_kind) in [
            (22, MkParameterInputKind::File),
            (23, MkParameterInputKind::Folder),
        ] {
            let mut path = parameter(id, "path", None);
            path.input_kind = input_kind;
            let portable = [(
                path.id,
                MkValue::String(r"Z:\portable\future folder\missing.txt".into()),
            )]
            .into_iter()
            .collect();
            assert!(prepare_parameters(&[path.clone()], &[], &portable).is_ok());
            let blank = [(path.id, MkValue::String(" \t".into()))]
                .into_iter()
                .collect();
            assert!(
                prepare_parameters(&[path], &[], &blank)
                    .unwrap_err()
                    .message
                    .contains("required file or folder path cannot be blank")
            );
        }

        let legacy_text = parameter(24, "legacy", None);
        let blank = [(legacy_text.id, MkValue::String(String::new()))]
            .into_iter()
            .collect();
        assert!(prepare_parameters(&[legacy_text], &[], &blank).is_ok());
    }

    #[test]
    fn parameter_input_metadata_validates_interpolated_choices_after_resolution() {
        let mut mode = parameter(9, "mode", None);
        mode.input_kind = MkParameterInputKind::Choice(vec!["safe".into(), "other".into()]);
        let callee = MkMacro {
            id: 2,
            name: "Choice target".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            signature: MkMacroSignature {
                parameters: vec![mode],
                outputs: Vec::new(),
            },
            steps: Vec::new(),
        };
        let signature = super::super::compile(&callee).unwrap().signature;
        let call = MkCallMacroPayload {
            macro_id: callee.id,
            arguments: vec![MkCallArgumentBinding {
                parameter_id: MkSignatureId(9),
                source: MkValueSource::Literal(MkValue::String("${selected}".into())),
            }],
            ..Default::default()
        };
        let variables = [("selected".into(), MkValue::String("safe".into()))]
            .into_iter()
            .collect();
        assert_eq!(
            prepare_call(&signature, &call, &variables).unwrap()["mode"],
            MkValue::String("safe".into())
        );

        let invalid_variables = [("selected".into(), MkValue::String("unsafe".into()))]
            .into_iter()
            .collect();
        assert!(
            prepare_call(&signature, &call, &invalid_variables)
                .unwrap_err()
                .message
                .contains("not one of the configured choices"),
            "the resolved value, not the template, must satisfy exact choice membership"
        );
    }

    #[test]
    fn parameter_input_metadata_changes_invalidate_pending_assumptions_but_reorder_does_not() {
        let mut choice = parameter(31, "mode", None);
        choice.input_kind = MkParameterInputKind::Choice(vec!["A".into(), "B".into()]);
        let text = parameter(32, "note", None);
        let previous = vec![choice.clone(), text.clone()];

        let mut renamed_text = text;
        renamed_text.name = "renamed_note".into();
        let mut renamed_choice = choice.clone();
        renamed_choice.name = "renamed_mode".into();
        assert!(
            validate_parameter_assumptions(
                &previous,
                &[renamed_text.clone(), renamed_choice.clone()]
            )
            .is_ok()
        );

        renamed_choice.input_kind = MkParameterInputKind::Choice(vec!["A".into(), "C".into()]);
        assert!(
            validate_parameter_assumptions(&previous, &[renamed_text.clone(), renamed_choice])
                .is_err()
        );

        let mut changed_control = choice;
        changed_control.input_kind = MkParameterInputKind::File;
        assert!(
            validate_parameter_assumptions(
                &previous,
                &[changed_control, parameter(32, "note", None)]
            )
            .is_err()
        );
    }

    #[test]
    fn sources_preserve_exact_keys_values_and_single_pass_interpolation() {
        let locals = [
            ("键.值".into(), MkValue::String("${never_rescan}".into())),
            ("macro.name".into(), MkValue::String("Actual name".into())),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            resolve_source(
                &MkValueSource::Variable {
                    name: "键.值".into()
                },
                &locals
            )
            .unwrap(),
            MkValue::String("${never_rescan}".into())
        );
        assert_eq!(
            resolve_source(
                &MkValueSource::Literal(MkValue::String(
                    "${键.值} / $${escaped} / ${macro.name}".into()
                )),
                &locals
            )
            .unwrap(),
            MkValue::String("${never_rescan} / ${escaped} / Actual name".into())
        );
        assert!(resolve_source(&MkValueSource::Variable { name: "".into() }, &locals).is_err());
    }

    #[test]
    fn preparation_accepts_each_persisted_value_type_without_coercion() {
        let parameters = [
            (MkValueType::String, MkValue::String("value".into())),
            (MkValueType::Number, MkValue::Number(42.5)),
            (MkValueType::Boolean, MkValue::Boolean(true)),
            (MkValueType::Point, MkValue::Point(MkPoint { x: -4, y: 9 })),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (value_type, value))| {
            let id = MkSignatureId(index as u64 + 1);
            (
                MkMacroParameter {
                    id,
                    name: format!("value_{index}"),
                    value_type,
                    input_kind: Default::default(),
                    description: String::new(),
                    default_value: None,
                },
                (id, value),
            )
        })
        .collect::<Vec<_>>();
        let definitions = parameters
            .iter()
            .map(|(parameter, _)| parameter.clone())
            .collect::<Vec<_>>();
        let supplied = parameters
            .iter()
            .map(|(_, supplied)| supplied.clone())
            .collect::<MkInvocationValues>();

        let locals = prepare_parameters(&definitions, &[], &supplied)
            .unwrap()
            .into_variables(&definitions)
            .unwrap();

        for (index, (_, (_, value))) in parameters.iter().enumerate() {
            assert_eq!(locals[&format!("value_{index}")], *value);
        }
        for parameter in definitions {
            let wrong = [(parameter.id, MkValue::Null)].into_iter().collect();
            assert_eq!(
                prepare_parameters(&[parameter], &[], &wrong)
                    .unwrap_err()
                    .kind,
                DiagnosticKind::TypeMismatch
            );
        }
    }
}
