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
            if let MkValue::String(template) = value {
                if let Err(reason) = super::validation::interpolation_syntax(template) {
                    push(
                        out,
                        owner,
                        Some(step),
                        "invalid_binding_interpolation",
                        reason,
                    );
                }
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
                let parameter = parameters.get(&argument.parameter_id);
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
