//! Authoring-time inventory of variables produced by earlier macro steps.
//!
//! This deliberately contains descriptors, not runtime values.  It is safe to
//! rebuild whenever a picker opens and is never serialized with a macro.

use crate::mkmacro::{
    MkAction, MkBlockKind, MkErrorPolicy, MkImageNotFoundPolicy, MkImageOutputs, MkMacro,
    MkMacroDocument, MkMacroSignature, MkSignatureId, MkStep, MkValue, MkValueType,
    StructureAnalysis, structure::analyze_structure,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableValueType {
    Known(MkValueType),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableAvailability {
    DefinitelyAvailable,
    PossiblyUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableUncertaintyReason {
    ProducedInside(MkBlockKind),
    MayBeNullIfNotFound,
    MayBeUnset,
    ErrorMayContinue,
}

/// A non-fatal authoring diagnostic for a variable consumer.
///
/// The kind, variable name, and producer metadata deliberately remain
/// structured; callers should use
/// [`VariableConsumerWarning::message_for_consumer`] rather than assembling
/// UI-specific wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableWarningKind {
    NoKnownPriorProducer,
    KnownWrongType {
        actual: VariableValueType,
        expected: VariableValueType,
    },
    PossiblyUnavailable {
        reason: VariableUncertaintyReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableStepSource {
    pub step_id: u64,
    pub step_index: usize,
    pub step_number: usize,
    pub action_label: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariableSource {
    Parameter { id: MkSignatureId },
    Step(VariableStepSource),
}
impl VariableSource {
    pub fn step(&self) -> Option<&VariableStepSource> {
        match self {
            Self::Step(step) => Some(step),
            Self::Parameter { .. } => None,
        }
    }
    pub fn caption(&self) -> String {
        match self {
            Self::Parameter { id } => format!("Parameter (stable ID {})", id.0),
            Self::Step(step) => format!(
                "{} at step {} (stable ID {})",
                step.action_label, step.step_number, step.step_id
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableConsumerWarning {
    pub variable_name: String,
    pub expected_type: VariableValueType,
    pub source: Option<VariableSource>,
    pub kind: VariableWarningKind,
}

impl VariableConsumerWarning {
    pub fn message_for_consumer(&self, consumer_label: &str) -> String {
        match self.kind {
            VariableWarningKind::NoKnownPriorProducer => format!(
                "No earlier action is known to produce {} variable \"{}\". Manual/dynamic variables are still allowed.",
                variable_type_label(self.expected_type),
                self.variable_name
            ),
            VariableWarningKind::KnownWrongType { actual, expected } => format!(
                "\"{}\" is currently known as {}; {} requires {}.",
                self.variable_name,
                variable_type_label(actual),
                consumer_label,
                variable_type_label(expected)
            ),
            VariableWarningKind::PossiblyUnavailable { .. } => format!(
                "\"{}\" is produced conditionally and may be Null/unavailable here.",
                self.variable_name
            ),
        }
    }
}

pub fn variable_type_label(value_type: VariableValueType) -> &'static str {
    match value_type {
        VariableValueType::Known(kind) => kind.label(),
        VariableValueType::Unknown => "Unknown",
    }
}
impl From<MkValueType> for VariableValueType {
    fn from(value: MkValueType) -> Self {
        Self::Known(value)
    }
}

impl VariableUncertaintyReason {
    pub fn help_text(self) -> &'static str {
        match self {
            Self::ProducedInside(MkBlockKind::If) => "Produced inside If",
            Self::ProducedInside(MkBlockKind::While) => "Produced inside While",
            Self::ProducedInside(MkBlockKind::Repeat) => "Produced inside Repeat",
            Self::MayBeNullIfNotFound => "May be Null if not found",
            Self::MayBeUnset => "May have been unset conditionally",
            Self::ErrorMayContinue => "An error may continue without producing a value",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableDescriptor {
    pub name: String,
    pub value_type: VariableValueType,
    pub source: VariableSource,
    pub availability: VariableAvailability,
    /// Structured causes used by picker warning icons and tooltips.
    pub uncertainty_reasons: Vec<VariableUncertaintyReason>,
    /// Extra picker guidance, including when a value can be Null at runtime.
    pub help_text: Option<&'static str>,
}

impl VariableDescriptor {
    /// Marker rendered beside entries whose producer may not execute or may
    /// yield Null.
    pub fn warning_marker(&self) -> Option<&'static str> {
        (self.availability == VariableAvailability::PossiblyUnavailable).then_some("⚠")
    }
}

/// Both views of the variable definitions visible at a consumer location.
///
/// Names have the same semantics as [`crate::mkmacro::RuntimeVariables`] keys:
/// surrounding editor whitespace is removed when a descriptor is made, but
/// comparison is otherwise exact and case-sensitive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VariableCatalog {
    history: Vec<VariableDescriptor>,
    effective: Vec<VariableDescriptor>,
}

impl VariableCatalog {
    /// Builds the catalog immediately before `consumer_index`.
    ///
    /// Out-of-range indices are consistently clamped to `steps.len()`, making
    /// `usize::MAX` useful for asking for the catalog at the end of a macro.
    pub fn before_step(steps: &[MkStep], consumer_index: usize) -> Self {
        Self::build(
            steps,
            &MkMacroSignature::default(),
            consumer_index,
            &std::collections::HashMap::new(),
        )
    }

    pub fn before_macro(document: &MkMacroDocument, macro_id: u64, consumer_index: usize) -> Self {
        let Some(owner) = document.macros.iter().find(|m| m.id == macro_id) else {
            return Self::default();
        };
        let macros = document
            .macros
            .iter()
            .filter(|m| super::reusable_validation::signature_is_valid(m))
            .map(|m| (m.id, m))
            .collect();
        let empty = MkMacroSignature::default();
        let signature = if super::reusable_validation::signature_is_valid(owner) {
            &owner.signature
        } else {
            &empty
        };
        Self::build(&owner.steps, signature, consumer_index, &macros)
    }

    fn build(
        steps: &[MkStep],
        signature: &MkMacroSignature,
        consumer_index: usize,
        macros: &std::collections::HashMap<u64, &MkMacro>,
    ) -> Self {
        let structure = analyze_structure(steps);
        let flow = ControlFlowFacts::new(steps, &structure);
        let mut catalog = Self::from_signature(signature);
        for (index, step) in steps
            .iter()
            .enumerate()
            .take(consumer_index.min(steps.len()))
        {
            if flow.reachable[index] {
                let callee = match &step.action {
                    MkAction::CallMacro(call) => macros.get(&call.macro_id).map(|m| &m.signature),
                    _ => None,
                };
                catalog.advance(step, index, &structure, callee);
            }
        }
        catalog
    }

    pub(crate) fn from_signature(signature: &MkMacroSignature) -> Self {
        let mut catalog = Self::default();
        for parameter in &signature.parameters {
            catalog.record(VariableDescriptor {
                name: parameter.name.clone(),
                value_type: parameter.value_type.into(),
                source: VariableSource::Parameter { id: parameter.id },
                availability: VariableAvailability::DefinitelyAvailable,
                uncertainty_reasons: Vec::new(),
                help_text: None,
            });
        }
        catalog
    }

    fn record(&mut self, descriptor: VariableDescriptor) {
        self.effective.retain(|old| old.name != descriptor.name);
        self.history.push(descriptor.clone());
        self.effective.push(descriptor);
    }

    pub(crate) fn advance(
        &mut self,
        step: &MkStep,
        index: usize,
        structure: &StructureAnalysis,
        callee: Option<&MkMacroSignature>,
    ) {
        if !step.enabled {
            return;
        }
        let enclosing = structure
            .containing_block(step.id)
            .map(|block| block.kind)
            .or_else(|| structure.editor_enclosing_kind(step.id));
        if let MkAction::UnsetVariable { name } = &step.action {
            if enclosing.is_some() {
                if let Some(old) = self.effective.iter_mut().find(|d| d.name == *name) {
                    old.availability = VariableAvailability::PossiblyUnavailable;
                    old.uncertainty_reasons
                        .push(VariableUncertaintyReason::MayBeUnset);
                }
            } else {
                self.effective.retain(|old| old.name != *name);
            }
        }
        for mut descriptor in descriptors_for_action_with_signature(step, index, callee) {
            if let Some(kind) = enclosing {
                descriptor.availability = VariableAvailability::PossiblyUnavailable;
                descriptor
                    .uncertainty_reasons
                    .insert(0, VariableUncertaintyReason::ProducedInside(kind));
                descriptor
                    .help_text
                    .get_or_insert(VariableUncertaintyReason::ProducedInside(kind).help_text());
            }
            if matches!(step.on_error, MkErrorPolicy::Continue) {
                descriptor.availability = VariableAvailability::PossiblyUnavailable;
                descriptor
                    .uncertainty_reasons
                    .push(VariableUncertaintyReason::ErrorMayContinue);
            }
            self.record(descriptor);
        }
    }

    /// Ordered producer history, intended for diagnostics and hover details.
    pub fn history(&self) -> &[VariableDescriptor] {
        &self.history
    }

    /// Effective variables at the consumer, with one entry per exact name.
    ///
    /// Entries are ordered by the source position of their latest definition,
    /// then by that action's output order.
    pub fn effective_variables(&self) -> &[VariableDescriptor] {
        &self.effective
    }

    /// Checks an exact variable name against its effective preceding
    /// definition. Lookup intentionally happens before type comparison, so a
    /// wrong-typed shadowing definition cannot reveal an older compatible one.
    pub fn warning_for_expected_type(
        &self,
        name: &str,
        expected: VariableValueType,
    ) -> Option<VariableConsumerWarning> {
        if name.is_empty() {
            return None;
        }
        let descriptor = self.effective.iter().find(|item| item.name == name);
        let source = descriptor.map(|item| item.source.clone());
        let kind = match descriptor {
            None => VariableWarningKind::NoKnownPriorProducer,
            Some(item) if item.value_type != expected => VariableWarningKind::KnownWrongType {
                actual: item.value_type,
                expected,
            },
            Some(item) if item.availability == VariableAvailability::PossiblyUnavailable => {
                VariableWarningKind::PossiblyUnavailable {
                    reason: item
                        .uncertainty_reasons
                        .first()
                        .copied()
                        .unwrap_or(VariableUncertaintyReason::MayBeNullIfNotFound),
                }
            }
            Some(_) => return None,
        };
        Some(VariableConsumerWarning {
            variable_name: name.to_owned(),
            expected_type: expected,
            source,
            kind,
        })
    }

    /// Effective picker entries of `value_type`. Shadowing is deliberately
    /// resolved before this filter is applied.
    pub fn effective_variables_of_type(
        &self,
        value_type: VariableValueType,
    ) -> impl Iterator<Item = &VariableDescriptor> {
        self.effective
            .iter()
            .filter(move |descriptor| descriptor.value_type == value_type)
    }

    /// Compatibility alias for the ordered producer history.
    pub fn descriptors(&self) -> &[VariableDescriptor] {
        self.history()
    }

    /// Returns one descriptor per name, choosing its latest definition while
    /// retaining the deterministic source order of those winning definitions.
    pub fn latest_definitions(&self) -> Vec<&VariableDescriptor> {
        self.effective.iter().collect()
    }
}

/// The shared action-to-produced-output mapping for analysis and authoring.
pub fn descriptors_for_action(step: &MkStep, step_index: usize) -> Vec<VariableDescriptor> {
    descriptors_for_action_with_signature(step, step_index, None)
}

fn descriptors_for_action_with_signature(
    step: &MkStep,
    step_index: usize,
    callee: Option<&MkMacroSignature>,
) -> Vec<VariableDescriptor> {
    let mut result = Vec::new();
    let label = match &step.action {
        MkAction::SetVariable { .. } => "Set Variable",
        MkAction::PromptInput(_) => "Prompt for Input",
        MkAction::ImageFind(_) => "Find Image",
        MkAction::ImageClick(_) => "Click Image",
        MkAction::OcrFindText(_) => "OCR Find Text",
        MkAction::OcrReadText(_) => "OCR Read Text",
        MkAction::FindPixel(_) => "Find Pixel",
        MkAction::CaptureScreenshot(_) => "Capture Screenshot",
        MkAction::UiReadValue { .. } => "UI Read Value",
        MkAction::CallMacro(_) => "Call Macro",
        _ => return result,
    };
    let mut add = |name: &str, value_type, availability, help_text| {
        let name = name.trim();
        if !name.is_empty() {
            result.push(VariableDescriptor {
                name: name.to_owned(),
                value_type,
                source: VariableSource::Step(VariableStepSource {
                    step_id: step.id,
                    step_index,
                    step_number: step_index + 1,
                    action_label: label,
                }),
                availability,
                uncertainty_reasons: if availability == VariableAvailability::PossiblyUnavailable {
                    vec![VariableUncertaintyReason::MayBeNullIfNotFound]
                } else {
                    Vec::new()
                },
                help_text,
            });
        }
    };

    match &step.action {
        MkAction::SetVariable { name, value } => add(
            name,
            value
                .value_type()
                .map(VariableValueType::Known)
                .unwrap_or(VariableValueType::Unknown),
            VariableAvailability::DefinitelyAvailable,
            None,
        ),
        MkAction::UiReadValue { variable, .. } => add(
            variable,
            MkValueType::String.into(),
            VariableAvailability::DefinitelyAvailable,
            None,
        ),
        MkAction::CallMacro(call) => {
            if let Some(signature) = callee {
                let outputs: std::collections::HashMap<_, _> =
                    signature.outputs.iter().map(|o| (o.id, o)).collect();
                for binding in &call.outputs {
                    if let Some(output) = outputs.get(&binding.output_id) {
                        add(
                            &binding.caller_variable,
                            output.value_type.into(),
                            VariableAvailability::DefinitelyAvailable,
                            None,
                        );
                    }
                }
            }
        }
        MkAction::PromptInput(payload) => add(
            &payload.variable,
            VariableValueType::Known(MkValueType::String),
            VariableAvailability::DefinitelyAvailable,
            None,
        ),
        MkAction::OcrReadText(payload) => add(
            &payload.output_variable,
            VariableValueType::Known(MkValueType::String),
            VariableAvailability::DefinitelyAvailable,
            None,
        ),
        MkAction::OcrFindText(payload) => add_ocr_outputs(
            &payload.outputs,
            &mut add,
            payload.not_found_policy == MkImageNotFoundPolicy::Continue,
        ),
        MkAction::ImageFind(payload) | MkAction::ImageClick(payload) => add_visual_outputs(
            &payload.outputs,
            &mut add,
            payload.not_found_policy == MkImageNotFoundPolicy::Continue,
            "May be Null if the image is not found",
        ),
        MkAction::FindPixel(payload) => add_visual_outputs(
            &payload.outputs,
            &mut add,
            payload.not_found_policy == MkImageNotFoundPolicy::Continue,
            "May be Null if the pixel is not found",
        ),
        MkAction::CaptureScreenshot(payload) if payload.destination.produces_file() => {
            if let Some(name) = &payload.path_output {
                add(
                    name,
                    VariableValueType::Known(MkValueType::String),
                    VariableAvailability::DefinitelyAvailable,
                    None,
                );
            }
        }
        _ => {}
    }
    result
}

fn add_ocr_outputs(
    outputs: &super::MkOcrOutputs,
    add: &mut impl FnMut(&str, VariableValueType, VariableAvailability, Option<&'static str>),
    can_continue_missing: bool,
) {
    let selected_availability = if can_continue_missing {
        VariableAvailability::PossiblyUnavailable
    } else {
        VariableAvailability::DefinitelyAvailable
    };
    if let Some(name) = &outputs.found {
        add(
            name,
            VariableValueType::Known(MkValueType::Boolean),
            VariableAvailability::DefinitelyAvailable,
            None,
        );
    }
    for (name, value_type) in [
        (&outputs.matched_text, MkValueType::String),
        (&outputs.point, MkValueType::Point),
        (&outputs.x, MkValueType::Number),
        (&outputs.y, MkValueType::Number),
    ] {
        if let Some(name) = name {
            add(
                name,
                VariableValueType::Known(value_type),
                selected_availability,
                can_continue_missing.then_some("May be Null if OCR text is not found"),
            );
        }
    }
    if let Some(name) = &outputs.match_count {
        add(
            name,
            VariableValueType::Known(MkValueType::Number),
            VariableAvailability::DefinitelyAvailable,
            None,
        );
    }
}

fn add_visual_outputs(
    outputs: &MkImageOutputs,
    add: &mut impl FnMut(&str, VariableValueType, VariableAvailability, Option<&'static str>),
    can_continue_missing: bool,
    nullable_help: &'static str,
) {
    let optional = if can_continue_missing {
        VariableAvailability::PossiblyUnavailable
    } else {
        VariableAvailability::DefinitelyAvailable
    };
    if let Some(name) = &outputs.found {
        add(
            name,
            VariableValueType::Known(MkValueType::Boolean),
            VariableAvailability::DefinitelyAvailable,
            None,
        );
    }
    for (name, value_type) in [
        (&outputs.point, VariableValueType::Known(MkValueType::Point)),
        (&outputs.x, VariableValueType::Known(MkValueType::Number)),
        (&outputs.y, VariableValueType::Known(MkValueType::Number)),
    ] {
        if let Some(name) = name {
            add(
                name,
                value_type,
                optional,
                can_continue_missing.then_some(nullable_help),
            );
        }
    }
}

fn return_sources_cannot_fail(ret: &super::MkReturnPayload) -> bool {
    ret.outputs.iter().all(|binding| match &binding.source {
        super::MkValueSource::Literal(MkValue::String(template)) => {
            super::interpolation::scan_template(template).all(|part| {
                matches!(
                    part,
                    Ok(super::TemplatePart::Text(_) | super::TemplatePart::EscapedReference(_))
                )
            })
        }
        super::MkValueSource::Literal(value) => value.value_type().is_some(),
        super::MkValueSource::Variable { .. } => false,
    })
}

/// Conservative instruction reachability, independent of compilation. The
/// structure owner supplies matching boundaries; disabled instructions fall
/// through, including disabled openers (their bodies still execute).
pub(crate) struct ControlFlowFacts {
    pub reachable: Vec<bool>,
    pub falls_through: bool,
}

impl ControlFlowFacts {
    pub fn new(steps: &[MkStep], structure: &StructureAnalysis) -> Self {
        if !structure.diagnostics.is_empty() {
            return Self {
                reachable: vec![true; steps.len()],
                falls_through: false,
            };
        }
        let mut successors = vec![Vec::new(); steps.len()];
        let mut loops = Vec::<&super::structure::StructuralBlock>::new();
        for (index, step) in steps.iter().enumerate() {
            while loops.last().is_some_and(|b| b.closer_index <= index) {
                loops.pop();
            }
            let block = structure.block_for_marker(step.id);
            let next = &mut successors[index];
            next.push(index + 1);
            if step.enabled {
                match (&step.action, block) {
                    (MkAction::Return(ret), _)
                        if !matches!(step.on_error, MkErrorPolicy::Continue)
                            || return_sources_cannot_fail(ret) =>
                    {
                        next.clear()
                    }
                    (MkAction::If(_), Some(block)) => next.push(
                        block
                            .else_marker
                            .map_or(block.closer_index + 1, |(_, index)| index + 1),
                    ),
                    (MkAction::Else, Some(block)) => {
                        next.clear();
                        next.push(block.closer_index + 1);
                    }
                    (MkAction::WhileStart { .. }, Some(block)) => next.push(block.closer_index + 1),
                    (MkAction::WhileEnd, Some(block)) => {
                        next.clear();
                        next.push(block.opener_index);
                    }
                    (MkAction::RepeatStart { count: 0 }, Some(block)) => {
                        next.clear();
                        next.push(block.closer_index + 1);
                    }
                    (MkAction::RepeatEnd, Some(block))
                        if steps[block.opener_index].enabled
                            && matches!(steps[block.opener_index].action, MkAction::RepeatStart { count } if count > 1) =>
                    {
                        next.push(block.opener_index + 1)
                    }
                    (MkAction::Break | MkAction::Continue, _) => {
                        if let Some(block) = loops.last() {
                            next.clear();
                            next.push(if matches!(step.action, MkAction::Break) {
                                block.closer_index + 1
                            } else if block.kind == MkBlockKind::While {
                                block.opener_index
                            } else {
                                block.closer_index
                            });
                        }
                    }
                    _ => {}
                }
            }
            if let Some(block) =
                block.filter(|block| block.opener_index == index && block.kind != MkBlockKind::If)
            {
                loops.push(block);
            }
        }
        let mut seen = vec![false; steps.len() + 1];
        let mut pending = vec![0];
        while let Some(index) = pending.pop() {
            if seen[index] {
                continue;
            }
            seen[index] = true;
            if let Some(next) = successors.get(index) {
                pending.extend(next.iter().copied());
            }
        }
        let falls_through = seen.pop().unwrap_or(true);
        Self {
            reachable: seen,
            falls_through,
        }
    }
}

pub(crate) fn analyze_macro(
    document: &MkMacroDocument,
    graph: &super::call_graph::CallGraph,
    owner: &MkMacro,
    invalid_signatures: &std::collections::HashSet<u64>,
    out: &mut Vec<super::MkDiagnostic>,
) {
    use super::{
        MkDiagnostic,
        authoring_fields::{FieldKind, step_fields},
        interpolation::{TemplatePart, scan_template},
        variables::*,
    };
    use std::collections::HashSet;
    let structure = analyze_structure(&owner.steps);
    let flow = ControlFlowFacts::new(&owner.steps, &structure);
    let mut catalog = if invalid_signatures.contains(&owner.id) {
        VariableCatalog::default()
    } else {
        VariableCatalog::from_signature(&owner.signature)
    };
    let mut labels = HashSet::new();
    let mut reads = HashSet::new();
    for (index, step) in owner.steps.iter().enumerate() {
        if !step.metadata.label.is_empty() && !labels.insert(&step.metadata.label) {
            out.push(MkDiagnostic::warning(
                owner.id,
                Some(step.id),
                "duplicate_label",
                format!("Duplicate annotation label '{}'", step.metadata.label),
            ));
        }
        super::reusable_validation::bindings(
            document,
            graph,
            owner,
            step,
            &catalog,
            invalid_signatures,
            out,
        );
        let executing = step.enabled && flow.reachable[index];
        if step.enabled && !flow.reachable[index] && !step.action.is_block_marker() {
            out.push(MkDiagnostic::warning(
                owner.id,
                Some(step.id),
                "unreachable_step",
                "Step is unreachable after control flow leaves this path",
            ));
        }
        let mut step_reads = HashSet::new();
        // The same exhaustive roles power Replace and static reads. Literal
        // strings, labels and comments never accidentally become references.
        for field in step_fields(step) {
            let mut names = Vec::new();
            match field.kind {
                FieldKind::VariableRead => {
                    if let Err(reason) = validate_variable_reference(&field.value) {
                        if !matches!(step.action, MkAction::CallMacro(_) | MkAction::Return(_)) {
                            out.push(MkDiagnostic::fatal(
                                owner.id,
                                Some(step.id),
                                "invalid_variable_reference",
                                format!("{}: {reason}", field.path),
                            ));
                        }
                    } else {
                        names.push(field.value.as_str());
                    }
                }
                FieldKind::Template => {
                    for part in scan_template(&field.value) {
                        match part {
                            Ok(TemplatePart::Reference(name)) => names.push(name),
                            Err(reason) => {
                                if !out.iter().any(|d| {
                                    d.macro_id == owner.id
                                        && d.step_id == Some(step.id)
                                        && d.code.contains("interpolation")
                                }) {
                                    out.push(MkDiagnostic::fatal(
                                        owner.id,
                                        Some(step.id),
                                        "invalid_interpolation",
                                        format!("{}: {reason}", field.path),
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            for name in names {
                if !executing || !step_reads.insert(name.to_owned()) {
                    continue;
                }
                reads.insert(name.to_owned());
                if is_builtin(name) {
                    continue;
                }
                match catalog.effective.iter().find(|d| d.name == name) {
                    None => out.push(MkDiagnostic::warning(
                        owner.id,
                        Some(step.id),
                        "read_before_definition",
                        format!("No reachable prior definition is known for variable '{name}'"),
                    )),
                    Some(d)
                        if d.availability == VariableAvailability::PossiblyUnavailable
                            || d.value_type == VariableValueType::Unknown =>
                    {
                        out.push(MkDiagnostic::warning(
                            owner.id,
                            Some(step.id),
                            "possibly_unavailable_variable",
                            format!("Variable '{name}' may be unavailable or Null on this path"),
                        ))
                    }
                    _ => {}
                }
            }
        }
        if executing {
            let callee = match &step.action {
                MkAction::CallMacro(call) if !invalid_signatures.contains(&call.macro_id) => graph
                    .macro_index(call.macro_id)
                    .map(|i| &document.macros[i].signature),
                _ => None,
            };
            catalog.advance(step, index, &structure, callee);
        }
    }
    // Suppress a warning whenever any reachable read may use this name. This
    // deliberately avoids claiming an overwritten/loop-carried value is unused.
    for descriptor in catalog.history() {
        if !reads.contains(&descriptor.name) {
            if let VariableSource::Step(source) = &descriptor.source {
                out.push(MkDiagnostic::warning(
                    owner.id,
                    Some(source.step_id),
                    "unused_local",
                    format!(
                        "Local variable '{}' has no known reachable reads",
                        descriptor.name
                    ),
                ));
            }
        }
    }
    if !owner.signature.outputs.is_empty() && flow.falls_through {
        out.push(MkDiagnostic::fatal(owner.id, None, "output_return_fallthrough", "A macro with declared outputs has a path that reaches the end without returning its outputs"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{
        AlphaPolicy, MkBlockKind, MkCondition, MkFileCollisionPolicy, MkImagePayload, MkImageRef,
        MkPixelSearchPayload, MkPoint, MkPromptInputPayload, MkScreenshotDestination,
        MkScreenshotFormat, MkScreenshotPayload, MkWaitOptions, ReturnPoint, SearchRegion,
    };

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            metadata: Default::default(),
            id,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action,
        }
    }
    fn outputs() -> MkImageOutputs {
        MkImageOutputs {
            found: Some("found".into()),
            point: Some("point".into()),
            x: Some("x".into()),
            y: Some("y".into()),
        }
    }
    fn image(outputs: MkImageOutputs) -> MkAction {
        MkAction::ImageFind(MkImagePayload {
            image: MkImageRef::from_filename("1.png"),
            wait: MkWaitOptions::default(),
            region: SearchRegion::Desktop,
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
            not_found_policy: MkImageNotFoundPolicy::Continue,
            outputs,
        })
    }
    fn pixel(outputs: MkImageOutputs) -> MkAction {
        MkAction::FindPixel(MkPixelSearchPayload {
            search_id: 1,
            color: "#000000".into(),
            tolerance: 0,
            region: SearchRegion::Desktop,
            wait: MkWaitOptions::default(),
            not_found_policy: MkImageNotFoundPolicy::Continue,
            outputs,
        })
    }
    fn set(id: u64, name: &str) -> MkStep {
        step(
            id,
            MkAction::SetVariable {
                name: name.into(),
                value: MkValue::Number(id as f64),
            },
        )
    }
    fn set_value(id: u64, name: &str, value: MkValue) -> MkStep {
        step(
            id,
            MkAction::SetVariable {
                name: name.into(),
                value,
            },
        )
    }
    fn empty_condition() -> MkCondition {
        MkCondition::All { conditions: vec![] }
    }

    #[test]
    fn set_variable_maps_every_value_type() {
        let values = [
            (
                MkValue::String("s".into()),
                VariableValueType::Known(MkValueType::String),
            ),
            (
                MkValue::Number(1.0),
                VariableValueType::Known(MkValueType::Number),
            ),
            (
                MkValue::Boolean(true),
                VariableValueType::Known(MkValueType::Boolean),
            ),
            (
                MkValue::Point(MkPoint { x: 1, y: 2 }),
                VariableValueType::Known(MkValueType::Point),
            ),
            (MkValue::Null, VariableValueType::Unknown),
        ];
        for (index, (value, expected)) in values.into_iter().enumerate() {
            let descriptors = descriptors_for_action(
                &step(
                    index as u64,
                    MkAction::SetVariable {
                        name: format!("v{index}"),
                        value,
                    },
                ),
                index,
            );
            assert_eq!(descriptors[0].value_type, expected);
        }
    }

    #[test]
    fn prompt_and_visual_actions_produce_expected_types() {
        let prompt = descriptors_for_action(
            &step(
                1,
                MkAction::PromptInput(MkPromptInputPayload {
                    variable: " answer ".into(),
                    ..Default::default()
                }),
            ),
            0,
        );
        assert_eq!(
            (&prompt[0].name, prompt[0].value_type),
            (
                &"answer".to_owned(),
                VariableValueType::Known(MkValueType::String)
            )
        );
        for action in [image(outputs()), pixel(outputs())] {
            let descriptors = descriptors_for_action(&step(2, action), 1);
            assert_eq!(
                descriptors
                    .iter()
                    .map(|d| (&*d.name, d.value_type))
                    .collect::<Vec<_>>(),
                vec![
                    ("found", VariableValueType::Known(MkValueType::Boolean)),
                    ("point", VariableValueType::Known(MkValueType::Point)),
                    ("x", VariableValueType::Known(MkValueType::Number)),
                    ("y", VariableValueType::Known(MkValueType::Number))
                ]
            );
            assert_eq!(
                descriptors[0].availability,
                VariableAvailability::DefinitelyAvailable
            );
            for descriptor in &descriptors[1..] {
                assert_eq!(
                    descriptor.availability,
                    VariableAvailability::PossiblyUnavailable
                );
                assert!(descriptor.help_text.unwrap().contains("Null"));
            }
        }
    }

    #[test]
    fn screenshot_only_exposes_paths_for_file_destinations() {
        let screenshot = |destination| {
            MkAction::CaptureScreenshot(MkScreenshotPayload {
                region: SearchRegion::Desktop,
                destination,
                path: None,
                format: MkScreenshotFormat::Png,
                collision: MkFileCollisionPolicy::Unique,
                path_output: Some(" saved ".into()),
            })
        };
        assert_eq!(
            descriptors_for_action(&step(1, screenshot(MkScreenshotDestination::File)), 0)[0].name,
            "saved"
        );
        assert!(
            descriptors_for_action(&step(2, screenshot(MkScreenshotDestination::Clipboard)), 0)
                .is_empty()
        );
        assert_eq!(
            descriptors_for_action(&step(3, screenshot(MkScreenshotDestination::Both)), 0).len(),
            1
        );
    }

    #[test]
    fn blank_names_are_ignored() {
        let blank = MkImageOutputs {
            found: None,
            point: Some(String::new()),
            x: Some("  ".into()),
            y: None,
        };
        assert!(descriptors_for_action(&step(1, image(blank)), 0).is_empty());
        assert!(
            descriptors_for_action(
                &step(
                    2,
                    MkAction::SetVariable {
                        name: " \t".into(),
                        value: MkValue::Null
                    }
                ),
                0
            )
            .is_empty()
        );
    }

    #[test]
    fn before_step_is_half_open_clamped_and_deterministic() {
        let set = |id, name: &str| {
            step(
                id,
                MkAction::SetVariable {
                    name: name.into(),
                    value: MkValue::Number(id as f64),
                },
            )
        };
        let steps = vec![set(10, "a"), set(11, "b"), set(12, "a"), set(13, "later")];
        let catalog = VariableCatalog::before_step(&steps, 2);
        assert_eq!(
            catalog
                .descriptors()
                .iter()
                .map(|d| (&*d.name, d.source.step().unwrap().step_index))
                .collect::<Vec<_>>(),
            vec![("a", 0), ("b", 1)]
        );
        assert!(
            !catalog
                .descriptors()
                .iter()
                .any(|d| d.source.step().unwrap().step_id == 12
                    || d.source.step().unwrap().step_id == 13)
        );
        let all = VariableCatalog::before_step(&steps, usize::MAX);
        assert_eq!(
            all.descriptors()
                .iter()
                .map(|d| &*d.name)
                .collect::<Vec<_>>(),
            vec!["a", "b", "a", "later"]
        );
        assert_eq!(
            all.latest_definitions()
                .iter()
                .map(|d| &*d.name)
                .collect::<Vec<_>>(),
            vec!["b", "a", "later"]
        );
    }

    #[test]
    fn effective_view_replaces_same_name_and_keeps_latest_metadata() {
        let steps = vec![
            set_value(10, " value ", MkValue::Number(1.0)),
            step(
                20,
                MkAction::PromptInput(MkPromptInputPayload {
                    variable: "value".into(),
                    ..Default::default()
                }),
            ),
        ];
        let catalog = VariableCatalog::before_step(&steps, usize::MAX);

        assert_eq!(catalog.history().len(), 2, "history retains both producers");
        assert_eq!(catalog.effective_variables().len(), 1);
        let effective = &catalog.effective_variables()[0];
        assert_eq!(
            effective.value_type,
            VariableValueType::Known(MkValueType::String)
        );
        assert_eq!(effective.source.step().unwrap().step_id, 20);
        assert_eq!(effective.source.step().unwrap().step_index, 1);
        assert_eq!(effective.source.step().unwrap().step_number, 2);
        assert_eq!(
            effective.source.step().unwrap().action_label,
            "Prompt for Input"
        );
    }

    #[test]
    fn effective_type_filter_runs_after_point_string_shadow_resolution() {
        let point = |id, name: &str| set_value(id, name, MkValue::Point(MkPoint { x: 1, y: 2 }));
        let string = |id, name: &str| set_value(id, name, MkValue::String("text".into()));

        let point_then_string =
            VariableCatalog::before_step(&[point(1, "target"), string(4, "target")], 2);
        assert_eq!(
            point_then_string
                .effective_variables_of_type(VariableValueType::Known(MkValueType::Point))
                .count(),
            0
        );

        let string_then_point =
            VariableCatalog::before_step(&[string(1, "target"), point(4, "target")], 2);
        let points: Vec<_> = string_then_point
            .effective_variables_of_type(VariableValueType::Known(MkValueType::Point))
            .collect();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].source.step().unwrap().step_id, 4);
    }

    #[test]
    fn conditional_latest_definition_supplies_type_and_availability() {
        let conditional_string = vec![
            set_value(1, "target", MkValue::Point(MkPoint { x: 1, y: 2 })),
            step(2, MkAction::If(empty_condition())),
            set_value(3, "target", MkValue::String("conditional".into())),
            step(4, MkAction::EndIf),
        ];
        let catalog = VariableCatalog::before_step(&conditional_string, usize::MAX);
        let effective = &catalog.effective_variables()[0];

        assert_eq!(
            effective.value_type,
            VariableValueType::Known(MkValueType::String)
        );
        assert_eq!(
            effective.availability,
            VariableAvailability::PossiblyUnavailable
        );
        assert_eq!(effective.source.step().unwrap().step_id, 3);
        assert_eq!(
            catalog
                .effective_variables_of_type(VariableValueType::Known(MkValueType::Point))
                .count(),
            0
        );
    }

    fn mouse_move(id: u64) -> MkStep {
        step(
            id,
            MkAction::MouseMove(crate::mkmacro::MkMouseMovePayload {
                target: crate::mkmacro::MkCoordinateTarget::Variable { name: "p".into() },
                duration_ms: 0,
            }),
        )
    }

    #[test]
    fn mouse_move_catalog_uses_real_ordered_action_outputs() {
        let steps = vec![
            set_value(101, "p", MkValue::Point(MkPoint { x: 10, y: 20 })),
            step(
                102,
                MkAction::PromptInput(MkPromptInputPayload {
                    variable: "text".into(),
                    ..Default::default()
                }),
            ),
            step(
                103,
                image(MkImageOutputs {
                    found: Some("was_found".into()),
                    point: Some("found_point".into()),
                    x: None,
                    y: None,
                }),
            ),
            mouse_move(104),
        ];

        let catalog = VariableCatalog::before_step(&steps, 3);
        let effective = catalog.effective_variables();
        assert_eq!(
            effective
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            vec!["p", "text", "was_found", "found_point"]
        );
        let descriptor = |name| effective.iter().find(|item| item.name == name).unwrap();
        for (name, value_type, producer) in [
            ("p", VariableValueType::Known(MkValueType::Point), 101),
            ("text", VariableValueType::Known(MkValueType::String), 102),
            (
                "was_found",
                VariableValueType::Known(MkValueType::Boolean),
                103,
            ),
            (
                "found_point",
                VariableValueType::Known(MkValueType::Point),
                103,
            ),
        ] {
            assert_eq!(descriptor(name).value_type, value_type);
            assert_eq!(descriptor(name).source.step().unwrap().step_id, producer);
        }
        assert_eq!(descriptor("p").source.step().unwrap().step_number, 1);
        assert_eq!(descriptor("text").source.step().unwrap().step_number, 2);
        assert_eq!(
            descriptor("found_point").source.step().unwrap().step_number,
            3
        );
        assert_eq!(
            descriptor("p").availability,
            VariableAvailability::DefinitelyAvailable
        );
        assert!(descriptor("p").uncertainty_reasons.is_empty());
        assert_eq!(
            descriptor("found_point").availability,
            VariableAvailability::PossiblyUnavailable
        );
        assert_eq!(
            descriptor("found_point").uncertainty_reasons,
            vec![VariableUncertaintyReason::MayBeNullIfNotFound]
        );
        assert!(
            descriptor("found_point")
                .help_text
                .unwrap()
                .contains("Null")
        );

        // This is the filtering operation used by a Point consumer. Nullable
        // values remain useful suggestions and retain their warning metadata.
        let points: Vec<_> = catalog
            .effective_variables_of_type(VariableValueType::Known(MkValueType::Point))
            .collect();
        assert_eq!(
            points
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            vec!["p", "found_point"]
        );
        assert_eq!(points[1].warning_marker(), Some("⚠"));
    }

    #[test]
    fn conditional_nullable_output_is_single_effective_descriptor() {
        let steps = vec![
            step(201, MkAction::If(empty_condition())),
            step(
                202,
                image(MkImageOutputs {
                    found: None,
                    point: Some("maybe_point".into()),
                    x: None,
                    y: None,
                }),
            ),
            step(203, MkAction::EndIf),
            mouse_move(204),
        ];
        let catalog = VariableCatalog::before_step(&steps, 3);
        assert_eq!(catalog.effective_variables().len(), 1);
        let descriptor = &catalog.effective_variables()[0];
        assert_eq!(
            (descriptor.name.as_str(), descriptor.value_type),
            ("maybe_point", VariableValueType::Known(MkValueType::Point))
        );
        assert_eq!(descriptor.source.step().unwrap().step_id, 202);
        assert_eq!(
            descriptor.availability,
            VariableAvailability::PossiblyUnavailable
        );
        assert_eq!(
            descriptor.uncertainty_reasons,
            vec![
                VariableUncertaintyReason::ProducedInside(MkBlockKind::If),
                VariableUncertaintyReason::MayBeNullIfNotFound,
            ]
        );
        assert_eq!(
            format!(
                "{} · {} · conditional",
                descriptor.name,
                variable_type_label(descriptor.value_type)
            ),
            "maybe_point · Point · conditional"
        );
    }

    #[test]
    fn latest_same_name_output_owns_all_effective_metadata() {
        let steps = vec![
            step(
                301,
                image(MkImageOutputs {
                    found: None,
                    point: Some("result".into()),
                    x: None,
                    y: None,
                }),
            ),
            step(302, MkAction::If(empty_condition())),
            step(
                303,
                image(MkImageOutputs {
                    found: Some("result".into()),
                    point: None,
                    x: None,
                    y: None,
                }),
            ),
            step(304, MkAction::EndIf),
        ];
        let catalog = VariableCatalog::before_step(&steps, usize::MAX);
        assert_eq!(
            catalog
                .history()
                .iter()
                .filter(|item| item.name == "result")
                .count(),
            2
        );
        assert_eq!(catalog.effective_variables().len(), 1);
        let result = &catalog.effective_variables()[0];
        assert_eq!(
            result.value_type,
            VariableValueType::Known(MkValueType::Boolean)
        );
        assert_eq!(result.source.step().unwrap().step_id, 303);
        assert_eq!(result.source.step().unwrap().step_number, 3);
        assert_eq!(result.source.step().unwrap().action_label, "Find Image");
        assert_eq!(
            result.availability,
            VariableAvailability::PossiblyUnavailable
        );
        assert_eq!(
            result.uncertainty_reasons,
            vec![VariableUncertaintyReason::ProducedInside(MkBlockKind::If)]
        );
        assert_eq!(result.help_text, Some("Produced inside If"));
    }

    #[test]
    fn effective_names_are_case_sensitive_ignore_future_steps_and_have_stable_order() {
        let steps = vec![
            set(1, "target"),
            set(2, "middle"),
            set(3, "Target"),
            set(4, "target"),
            set(5, "future"),
        ];
        let catalog = VariableCatalog::before_step(&steps, 4);
        let effective: Vec<_> = catalog
            .effective_variables()
            .iter()
            .map(|descriptor| {
                (
                    &*descriptor.name,
                    descriptor.source.step().unwrap().step_index,
                )
            })
            .collect();

        // Winners are ordered by their latest source position. Exact casing
        // keeps the two runtime keys distinct.
        assert_eq!(effective, vec![("middle", 1), ("Target", 2), ("target", 3)]);
        assert!(!catalog.history().iter().any(|item| item.name == "future"));
    }

    #[test]
    fn expected_type_warnings_obey_lookup_precedence() {
        let point = |id, name: &str| set_value(id, name, MkValue::Point(MkPoint { x: 1, y: 2 }));
        let string = |id, name: &str| set_value(id, name, MkValue::String("text".into()));

        let unknown = VariableCatalog::before_step(&[point(9, "future")], 0)
            .warning_for_expected_type("point", VariableValueType::Known(MkValueType::Point))
            .unwrap();
        assert_eq!(unknown.kind, VariableWarningKind::NoKnownPriorProducer);
        assert_eq!(unknown.source, None);
        assert_eq!(
            unknown.message_for_consumer("Mouse Move"),
            "No earlier action is known to produce Point variable \"point\". Manual/dynamic variables are still allowed."
        );

        let wrong =
            VariableCatalog::before_step(&[point(1, "point"), string(2, "point")], usize::MAX)
                .warning_for_expected_type("point", VariableValueType::Known(MkValueType::Point))
                .unwrap();
        assert!(matches!(
            wrong.kind,
            VariableWarningKind::KnownWrongType {
                actual: VariableValueType::Known(MkValueType::String),
                expected: VariableValueType::Known(MkValueType::Point)
            }
        ));
        assert_eq!(wrong.source.as_ref().unwrap().step().unwrap().step_id, 2);
        assert_eq!(
            wrong.message_for_consumer("Mouse Move"),
            "\"point\" is currently known as String; Mouse Move requires Point."
        );

        let latest_point =
            VariableCatalog::before_step(&[string(1, "point"), point(2, "point")], usize::MAX);
        assert_eq!(
            latest_point
                .warning_for_expected_type("point", VariableValueType::Known(MkValueType::Point)),
            None
        );
    }

    #[test]
    fn conditional_and_nullable_points_each_produce_one_warning() {
        let conditional = VariableCatalog::before_step(
            &[
                step(1, MkAction::If(empty_condition())),
                set_value(2, "point", MkValue::Point(MkPoint { x: 1, y: 2 })),
                step(3, MkAction::EndIf),
            ],
            usize::MAX,
        );
        let warning = conditional
            .warning_for_expected_type("point", VariableValueType::Known(MkValueType::Point))
            .unwrap();
        assert!(matches!(
            warning.kind,
            VariableWarningKind::PossiblyUnavailable {
                reason: VariableUncertaintyReason::ProducedInside(MkBlockKind::If)
            }
        ));

        let nullable = VariableCatalog::before_step(&[step(4, image(outputs()))], usize::MAX);
        let warnings: Vec<_> = std::iter::once(
            nullable
                .warning_for_expected_type("point", VariableValueType::Known(MkValueType::Point)),
        )
        .flatten()
        .collect();
        assert_eq!(warnings.len(), 1, "one consumer/name/reason diagnostic");
        assert!(matches!(
            warnings[0].kind,
            VariableWarningKind::PossiblyUnavailable {
                reason: VariableUncertaintyReason::MayBeNullIfNotFound
            }
        ));
        assert_eq!(
            warnings[0].message_for_consumer("Mouse Move"),
            "\"point\" is produced conditionally and may be Null/unavailable here."
        );
    }

    #[test]
    fn structural_enclosures_make_producers_possibly_unavailable() {
        let cases = [
            (
                vec![
                    step(1, MkAction::If(empty_condition())),
                    set(2, "if_body"),
                    step(3, MkAction::EndIf),
                ],
                MkBlockKind::If,
            ),
            (
                vec![
                    step(1, MkAction::If(empty_condition())),
                    step(2, MkAction::Else),
                    set(3, "else_body"),
                    step(4, MkAction::EndIf),
                ],
                MkBlockKind::If,
            ),
            (
                vec![
                    step(
                        1,
                        MkAction::WhileStart {
                            condition: empty_condition(),
                        },
                    ),
                    set(2, "while_body"),
                    step(3, MkAction::WhileEnd),
                ],
                MkBlockKind::While,
            ),
            (
                vec![
                    step(1, MkAction::RepeatStart { count: 2 }),
                    set(2, "repeat_body"),
                    step(3, MkAction::RepeatEnd),
                ],
                MkBlockKind::Repeat,
            ),
        ];
        for (steps, kind) in cases {
            let catalog = VariableCatalog::before_step(&steps, usize::MAX);
            let descriptor = &catalog.descriptors()[0];
            assert_eq!(
                descriptor.availability,
                VariableAvailability::PossiblyUnavailable
            );
            assert_eq!(
                descriptor.uncertainty_reasons,
                vec![VariableUncertaintyReason::ProducedInside(kind)]
            );
            assert_eq!(descriptor.warning_marker(), Some("⚠"));
        }
    }

    #[test]
    fn nested_production_stays_possible_but_completed_block_restores_top_level() {
        let steps = vec![
            set(10, "top"),
            step(11, MkAction::If(empty_condition())),
            step(12, MkAction::RepeatStart { count: 2 }),
            set(13, "nested"),
            step(14, MkAction::RepeatEnd),
            step(15, MkAction::EndIf),
            set(16, "after"),
        ];
        let catalog = VariableCatalog::before_step(&steps, usize::MAX);
        let descriptors = catalog.descriptors();
        assert_eq!(
            descriptors[0].availability,
            VariableAvailability::DefinitelyAvailable
        );
        assert_eq!(
            descriptors[1].availability,
            VariableAvailability::PossiblyUnavailable
        );
        assert_eq!(
            descriptors[2].availability,
            VariableAvailability::DefinitelyAvailable
        );
    }

    #[test]
    fn unclosed_draft_is_conservative_and_preserves_source_metadata() {
        let steps = vec![step(40, MkAction::If(empty_condition())), set(987, "draft")];
        let catalog = VariableCatalog::before_step(&steps, usize::MAX);
        let descriptor = &catalog.descriptors()[0];
        assert_eq!(
            descriptor.availability,
            VariableAvailability::PossiblyUnavailable
        );
        assert_eq!(descriptor.source.step().unwrap().step_id, 987);
        assert_eq!(descriptor.source.step().unwrap().step_index, 1);
        assert_eq!(descriptor.source.step().unwrap().step_number, 2);
        assert_eq!(
            descriptor.source.step().unwrap().action_label,
            "Set Variable"
        );
    }

    #[test]
    fn top_level_nullable_point_remains_possibly_unavailable() {
        let steps = vec![step(
            55,
            image(MkImageOutputs {
                found: None,
                point: Some("location".into()),
                x: None,
                y: None,
            }),
        )];
        let catalog = VariableCatalog::before_step(&steps, usize::MAX);
        let descriptor = &catalog.descriptors()[0];
        assert_eq!(
            descriptor.availability,
            VariableAvailability::PossiblyUnavailable
        );
        assert_eq!(
            descriptor.uncertainty_reasons,
            vec![VariableUncertaintyReason::MayBeNullIfNotFound]
        );
    }
    #[test]
    fn document_catalog_has_real_parameters_calls_ui_reads_unset_and_disabled_writes() {
        use crate::mkmacro::*;
        let mut root: MkMacro =
            serde_json::from_value(serde_json::json!({"id":1,"name":"root"})).unwrap();
        root.signature.parameters.push(MkMacroParameter {
            id: MkSignatureId(7),
            name: "input".into(),
            value_type: MkValueType::Number,
            description: String::new(),
            default_value: None,
        });
        root.steps = vec![
            set_value(1, "input", MkValue::String("disabled".into())),
            step(
                2,
                MkAction::CallMacro(MkCallMacroPayload {
                    macro_id: 2,
                    outputs: vec![MkCallOutputBinding {
                        output_id: MkSignatureId(20),
                        caller_variable: "answer".into(),
                    }],
                    ..Default::default()
                }),
            ),
            step(
                3,
                MkAction::UiReadValue {
                    target: MkUiPayload {
                        window: Default::default(),
                        selector: MkUiSelector {
                            automation_id: None,
                            name: None,
                            class_name: None,
                            framework_id: None,
                            control_type: None,
                            ancestor_path: Vec::new(),
                        },
                        wait: None,
                    },
                    variable: "ui_value".into(),
                },
            ),
            step(
                4,
                MkAction::UnsetVariable {
                    name: "answer".into(),
                },
            ),
        ];
        root.steps[0].enabled = false;
        let mut child: MkMacro =
            serde_json::from_value(serde_json::json!({"id":2,"name":"child"})).unwrap();
        child.signature.outputs.push(MkMacroOutput {
            id: MkSignatureId(20),
            name: "output".into(),
            value_type: MkValueType::Point,
            description: String::new(),
        });
        let doc = MkMacroDocument {
            macros: vec![root, child],
            ..Default::default()
        };
        let before = VariableCatalog::before_macro(&doc, 1, 0);
        assert_eq!(
            before.effective_variables()[0].source,
            VariableSource::Parameter {
                id: MkSignatureId(7)
            }
        );
        let catalog = VariableCatalog::before_macro(&doc, 1, 3);
        assert_eq!(
            catalog
                .effective_variables()
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>(),
            ["input", "answer", "ui_value"]
        );
        assert_eq!(
            catalog.effective_variables()[0].value_type,
            MkValueType::Number.into()
        );
        assert_eq!(
            catalog.effective_variables()[1].value_type,
            MkValueType::Point.into()
        );
        assert_eq!(
            catalog.effective_variables()[1]
                .source
                .step()
                .unwrap()
                .step_id,
            2
        );
        assert_eq!(
            catalog.effective_variables()[2].value_type,
            MkValueType::String.into()
        );
        let after = VariableCatalog::before_macro(&doc, 1, usize::MAX);
        assert_eq!(
            after
                .effective_variables()
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>(),
            ["input", "ui_value"]
        );
        assert!(
            after
                .warning_for_expected_type("answer", MkValueType::Point.into())
                .is_some()
        );
    }

    #[test]
    fn unreachable_definitions_do_not_shadow_and_conditional_unset_stays_uncertain() {
        let steps = vec![
            set(1, "value"),
            step(2, MkAction::Return(Default::default())),
            set_value(3, "value", MkValue::String("unreachable".into())),
        ];
        let catalog = VariableCatalog::before_step(&steps, usize::MAX);
        assert_eq!(
            catalog.effective_variables()[0].value_type,
            MkValueType::Number.into()
        );
        assert_eq!(catalog.history().len(), 1);
        let steps = vec![
            set(1, "value"),
            step(2, MkAction::If(empty_condition())),
            step(
                3,
                MkAction::UnsetVariable {
                    name: "value".into(),
                },
            ),
            step(4, MkAction::EndIf),
        ];
        let catalog = VariableCatalog::before_step(&steps, usize::MAX);
        let value = &catalog.effective_variables()[0];
        assert_eq!(
            value.availability,
            VariableAvailability::PossiblyUnavailable
        );
        assert!(
            value
                .uncertainty_reasons
                .contains(&VariableUncertaintyReason::MayBeUnset)
        );
    }

    #[test]
    fn ocr_outputs_have_exact_types_and_missing_match_availability() {
        let steps = vec![
            step(
                1,
                MkAction::OcrFindText(crate::mkmacro::MkOcrFindPayload {
                    outputs: crate::mkmacro::MkOcrOutputs {
                        found: Some("found".into()),
                        matched_text: Some("text".into()),
                        point: Some("point".into()),
                        x: Some("x".into()),
                        y: Some("y".into()),
                        match_count: Some("count".into()),
                    },
                    not_found_policy: MkImageNotFoundPolicy::Continue,
                    ..Default::default()
                }),
            ),
            step(
                2,
                MkAction::OcrReadText(crate::mkmacro::MkOcrReadPayload {
                    output_variable: "read".into(),
                    ..Default::default()
                }),
            ),
        ];
        let catalog = VariableCatalog::before_step(&steps, usize::MAX);
        let get = |name: &str| {
            catalog
                .effective_variables()
                .iter()
                .find(|descriptor| descriptor.name == name)
                .unwrap()
        };
        for (name, value_type, availability) in [
            (
                "found",
                MkValueType::Boolean,
                VariableAvailability::DefinitelyAvailable,
            ),
            (
                "text",
                MkValueType::String,
                VariableAvailability::PossiblyUnavailable,
            ),
            (
                "point",
                MkValueType::Point,
                VariableAvailability::PossiblyUnavailable,
            ),
            (
                "x",
                MkValueType::Number,
                VariableAvailability::PossiblyUnavailable,
            ),
            (
                "y",
                MkValueType::Number,
                VariableAvailability::PossiblyUnavailable,
            ),
            (
                "count",
                MkValueType::Number,
                VariableAvailability::DefinitelyAvailable,
            ),
            (
                "read",
                MkValueType::String,
                VariableAvailability::DefinitelyAvailable,
            ),
        ] {
            let descriptor = get(name);
            assert_eq!(
                descriptor.value_type,
                VariableValueType::Known(value_type),
                "{name}"
            );
            assert_eq!(descriptor.availability, availability, "{name}");
        }
    }
}
