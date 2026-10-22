//! Signature contracts and call-site binding validation; no runtime capability
//! decisions or compilation live here.
use super::{authoring_analysis::*, call_graph::CallGraph, model::*, validation::*, variables::*};
use std::collections::{HashMap, HashSet};

pub(crate) fn signature(owner_id: u64, signature: &MkMacroSignature, out: &mut Vec<MkDiagnostic>) {
    signature_parts(owner_id, &signature.parameters, &signature.outputs, out);
}

pub(crate) fn signature_parts(
    owner_id: u64,
    parameters: &[MkMacroParameter],
    outputs: &[MkMacroOutput],
    out: &mut Vec<MkDiagnostic>,
) {
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for (id, name) in parameters
        .iter()
        .map(|p| (p.id, &p.name))
        .chain(outputs.iter().map(|o| (o.id, &o.name)))
    {
        if id.0 == 0 || !ids.insert(id) {
            push(
                out,
                owner_id,
                None,
                "invalid_signature_id",
                "Parameter and output IDs must be non-zero and unique within the macro",
            );
        }
        if let Err(reason) = validate_variable_name(name) {
            push(
                out,
                owner_id,
                None,
                "invalid_signature_name",
                format!("Signature name '{name}' is invalid: {reason}"),
            );
        }
        if !names.insert(name) {
            push(
                out,
                owner_id,
                None,
                "duplicate_signature_name",
                format!("Parameter and output names must be unique across the signature: '{name}'"),
            );
        }
    }
    for parameter in parameters {
        if parameter.value_type != MkValueType::String
            && !matches!(parameter.input_kind, MkParameterInputKind::Text)
        {
            push(
                out,
                owner_id,
                None,
                "invalid_parameter_input_kind",
                format!(
                    "Input control for parameter '{}' is only valid for String parameters",
                    parameter.name
                ),
            );
        }
        if parameter
            .default_value
            .as_ref()
            .is_some_and(|value| !parameter.value_type.accepts(value))
        {
            push(
                out,
                owner_id,
                None,
                "invalid_parameter_default",
                format!(
                    "Default for parameter '{}' must be {}",
                    parameter.name,
                    parameter.value_type.label()
                ),
            );
        }
        if let MkParameterInputKind::Choice(options) = &parameter.input_kind {
            if let Some(reason) = choice_options_error(options) {
                push(
                    out,
                    owner_id,
                    None,
                    "invalid_parameter_choice_options",
                    format!(
                        "Choice options for parameter '{}' are invalid: {reason}",
                        parameter.name
                    ),
                );
            } else if let Some(MkValue::String(default)) = &parameter.default_value
                && !options.iter().any(|option| option == default)
            {
                push(
                    out,
                    owner_id,
                    None,
                    "invalid_parameter_choice_default",
                    format!(
                        "Default for choice parameter '{}' must exactly match one of its options",
                        parameter.name
                    ),
                );
            }
        }
    }
}

fn choice_options_error(options: &[String]) -> Option<&'static str> {
    if options.is_empty() {
        return Some("at least one option is required");
    }
    if options.len() > MK_PARAMETER_CHOICE_MAX_OPTIONS {
        return Some("the option count exceeds the configured limit");
    }
    let mut total_bytes = 0usize;
    let mut distinct = HashSet::with_capacity(options.len());
    for option in options {
        if option.trim().is_empty() {
            return Some("options cannot be empty or whitespace-only");
        }
        if option.len() > MK_PARAMETER_CHOICE_MAX_OPTION_BYTES {
            return Some("an option exceeds the UTF-8 byte limit");
        }
        total_bytes = total_bytes.saturating_add(option.len());
        if total_bytes > MK_PARAMETER_CHOICE_MAX_TOTAL_BYTES {
            return Some("the combined UTF-8 byte length exceeds the configured limit");
        }
        if !distinct.insert(option.as_str()) {
            return Some("options must be distinct exact strings");
        }
    }
    None
}

/// Returns the parser-decoded string only when a literal interpolation
/// template is independent of runtime variables. Escaped references are
/// literal text (and therefore can be checked statically); real references
/// are validated after `prepare_call` resolves them.
fn static_interpolated_string(template: &str) -> Option<String> {
    let mut value = String::with_capacity(template.len());
    for part in super::interpolation::scan_template(template) {
        match part.ok()? {
            super::TemplatePart::Text(text) => value.push_str(text),
            super::TemplatePart::EscapedReference(name) => {
                value.push_str("${");
                value.push_str(name);
                value.push('}');
            }
            super::TemplatePart::Reference(_) => return None,
        }
    }
    Some(value)
}

/// Returns a metadata-specific input error after the ordinary value type check.
/// Choice comparisons are exact; file/folder paths are not probed on disk.
pub(crate) fn parameter_value_error(
    parameter: &MkMacroParameter,
    value: &MkValue,
) -> Option<&'static str> {
    let MkValue::String(text) = value else {
        return None;
    };
    match &parameter.input_kind {
        MkParameterInputKind::Text => None,
        MkParameterInputKind::File | MkParameterInputKind::Folder
            if parameter.default_value.is_none() && text.trim().is_empty() =>
        {
            Some("a required file or folder path cannot be blank")
        }
        MkParameterInputKind::File | MkParameterInputKind::Folder => None,
        MkParameterInputKind::Choice(options) if !options.iter().any(|option| option == text) => {
            Some("the value is not one of the configured choices")
        }
        MkParameterInputKind::Choice(_) => None,
    }
}

pub(crate) fn signature_is_valid(owner: &MkMacro) -> bool {
    let mut diagnostics = Vec::new();
    signature(owner.id, &owner.signature, &mut diagnostics);
    diagnostics.is_empty()
}

fn source(
    source: &MkValueSource,
    expected: Option<MkValueType>,
    parameter: Option<&MkMacroParameter>,
    catalog: &VariableCatalog,
    owner: u64,
    step: u64,
    out: &mut Vec<MkDiagnostic>,
) {
    match source {
        MkValueSource::Literal(value) => {
            if expected.is_some_and(|kind| !kind.accepts(value)) {
                push(
                    out,
                    owner,
                    Some(step),
                    "binding_type_mismatch",
                    format!("Binding literal must be {}", expected.unwrap().label()),
                );
            }
            if let Some(parameter) = parameter {
                let value_error = match (&parameter.input_kind, value) {
                    (MkParameterInputKind::Choice(_), MkValue::String(template)) => {
                        static_interpolated_string(template).and_then(|value| {
                            parameter_value_error(parameter, &MkValue::String(value))
                        })
                    }
                    _ => parameter_value_error(parameter, value),
                };
                if let Some(reason) = value_error {
                    push(
                        out,
                        owner,
                        Some(step),
                        "invalid_parameter_binding_value",
                        format!(
                            "Binding for parameter '{}' is invalid: {reason}",
                            parameter.name
                        ),
                    );
                }
            }
            if let MkValue::String(template) = value
                && let Err(reason) = super::validation::interpolation_syntax(template)
            {
                push(
                    out,
                    owner,
                    Some(step),
                    "invalid_binding_interpolation",
                    reason,
                );
            }
        }
        MkValueSource::Variable { name } => {
            if let Err(reason) = validate_variable_reference(name) {
                push(out, owner, Some(step), "invalid_binding_reference", reason);
                return;
            }
            let Some(expected) = expected else {
                return;
            };
            let descriptor = catalog
                .effective_variables()
                .iter()
                .find(|d| d.name == *name);
            let known = builtin_type(name).map(|kind| (kind, true)).or_else(|| {
                descriptor.and_then(|d| match d.value_type {
                    VariableValueType::Known(kind) => Some((
                        kind,
                        d.availability == VariableAvailability::DefinitelyAvailable,
                    )),
                    VariableValueType::Unknown => None,
                })
            });
            if let Some((actual, certain)) = known.filter(|(kind, _)| *kind != expected) {
                let message = format!(
                    "Variable '{name}' is known as {}; binding requires {}",
                    actual.label(),
                    expected.label()
                );
                out.push(if certain {
                    MkDiagnostic::fatal(owner, Some(step), "binding_type_mismatch", message)
                } else {
                    MkDiagnostic::warning(owner, Some(step), "uncertain_binding_type", message)
                });
            }
        }
    }
}

pub(crate) fn bindings(
    document: &MkMacroDocument,
    graph: &CallGraph,
    owner: &MkMacro,
    step: &MkStep,
    catalog: &VariableCatalog,
    invalid_signatures: &HashSet<u64>,
    out: &mut Vec<MkDiagnostic>,
) {
    match &step.action {
        MkAction::CallMacro(call) => {
            // Authored payload validity is retained for disabled rows. Only
            // enabled rows participate in executable closure and cycle analysis.
            if let Some(diagnostic) = graph.target_diagnostic(owner.id, step.id, call.macro_id) {
                out.push(diagnostic);
            }
            let mut target = graph
                .macro_index(call.macro_id)
                .map(|index| &document.macros[index]);
            if invalid_signatures.contains(&call.macro_id) {
                let mut diagnostic = MkDiagnostic::fatal(
                    owner.id,
                    Some(step.id),
                    "invalid_call_signature",
                    "Call target signature is invalid; repair its definitions before binding arguments or outputs",
                );
                diagnostic.target_macro_id = Some(call.macro_id);
                out.push(diagnostic);
                target = None;
            }
            let parameters: HashMap<_, _> = target
                .into_iter()
                .flat_map(|m| &m.signature.parameters)
                .map(|p| (p.id, p))
                .collect();
            let outputs: HashMap<_, _> = target
                .into_iter()
                .flat_map(|m| &m.signature.outputs)
                .map(|o| (o.id, o))
                .collect();
            let start = out.len();
            let mut bound = HashSet::new();
            for argument in &call.arguments {
                if !bound.insert(argument.parameter_id) {
                    push(
                        out,
                        owner.id,
                        Some(step.id),
                        "duplicate_call_argument",
                        format!(
                            "Parameter #{} is bound more than once",
                            argument.parameter_id.0
                        ),
                    );
                }
                let parameter = parameters.get(&argument.parameter_id).copied();
                if target.is_some() && parameter.is_none() {
                    push(
                        out,
                        owner.id,
                        Some(step.id),
                        "dangling_call_parameter",
                        format!(
                            "Call refers to removed parameter #{}",
                            argument.parameter_id.0
                        ),
                    );
                }
                source(
                    &argument.source,
                    parameter.map(|p| p.value_type),
                    parameter,
                    catalog,
                    owner.id,
                    step.id,
                    out,
                );
            }
            if let Some(target) = target {
                for parameter in &target.signature.parameters {
                    if parameter.default_value.is_none() && !bound.contains(&parameter.id) {
                        push(
                            out,
                            owner.id,
                            Some(step.id),
                            "missing_call_argument",
                            format!(
                                "Required parameter '{}' (#{}) has no argument or default",
                                parameter.name, parameter.id.0
                            ),
                        );
                    }
                }
            }
            let mut mapped = HashSet::new();
            let mut names = HashSet::new();
            for binding in &call.outputs {
                if !mapped.insert(binding.output_id) {
                    push(
                        out,
                        owner.id,
                        Some(step.id),
                        "duplicate_call_output",
                        format!("Output #{} is mapped more than once", binding.output_id.0),
                    );
                }
                if target.is_some() && !outputs.contains_key(&binding.output_id) {
                    push(
                        out,
                        owner.id,
                        Some(step.id),
                        "dangling_call_output",
                        format!("Call refers to removed output #{}", binding.output_id.0),
                    );
                }
                if let Err(reason) = validate_variable_name(&binding.caller_variable) {
                    push(
                        out,
                        owner.id,
                        Some(step.id),
                        "invalid_call_output_variable",
                        format!(
                            "Output destination '{}' is invalid: {reason}",
                            binding.caller_variable
                        ),
                    );
                }
                if !names.insert(&binding.caller_variable) {
                    push(
                        out,
                        owner.id,
                        Some(step.id),
                        "duplicate_call_output_variable",
                        format!(
                            "Multiple outputs target caller variable '{}'",
                            binding.caller_variable
                        ),
                    );
                }
            }
            for diagnostic in &mut out[start..] {
                diagnostic.target_macro_id = Some(call.macro_id);
            }
        }
        MkAction::Return(ret) => {
            let signature = (!invalid_signatures.contains(&owner.id)).then_some(&owner.signature);
            let outputs: HashMap<_, _> = signature
                .into_iter()
                .flat_map(|s| &s.outputs)
                .map(|o| (o.id, o))
                .collect();
            let mut bound = HashSet::new();
            for binding in &ret.outputs {
                if !bound.insert(binding.output_id) {
                    push(
                        out,
                        owner.id,
                        Some(step.id),
                        "duplicate_return_output",
                        format!(
                            "Return binds output #{} more than once",
                            binding.output_id.0
                        ),
                    );
                }
                let output = outputs.get(&binding.output_id);
                if signature.is_some() && output.is_none() {
                    push(
                        out,
                        owner.id,
                        Some(step.id),
                        "dangling_return_output",
                        format!("Return refers to removed output #{}", binding.output_id.0),
                    );
                }
                source(
                    &binding.source,
                    output.map(|o| o.value_type),
                    None,
                    catalog,
                    owner.id,
                    step.id,
                    out,
                );
            }
            for output in signature.into_iter().flat_map(|s| &s.outputs) {
                if !bound.contains(&output.id) {
                    push(
                        out,
                        owner.id,
                        Some(step.id),
                        "missing_return_output",
                        format!(
                            "Return must provide output '{}' (#{})",
                            output.name, output.id.0
                        ),
                    );
                }
            }
        }
        _ => {}
    }
}
