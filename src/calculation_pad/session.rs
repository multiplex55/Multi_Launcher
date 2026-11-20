//! Editable Calculation Pad session with bounded history and dirty handling.

use super::document::{
    CalculationDocument, CalculationEvaluation, CalculationValue, DocumentDiagnostic, EditError, FinalBinding, LineOutcome,
};
use super::limits::{ABSOLUTE_BYTES as MAX_DOCUMENT_BYTES, ABSOLUTE_LINES as MAX_SOURCE_LINES, physical_rows};
use super::inspector::{
    DocumentIncarnation, InspectorInsertRequest, InspectorSourceOrigin, InspectorSourceSnapshot,
};
use super::persistence::{
    DocumentId, LineId, LoadedDocument, PadDocumentStore, PadDocumentSummary, PadStoreError,
    StoredDocument, PadViewMetadata, DocumentOptions,
};
use super::{IntegerMode, IntegerWidth, TypedInteger};
use super::worker::{EvaluationStamp, EvaluationWorker, Request as EvaluationRequest};
use crate::platform::app_data::AppDataRoot;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

pub(crate) const MAX_UNDO_ENTRIES: usize = 64;
pub(crate) const MAX_UNDO_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct SessionLineResult {
    pub id: LineId,
    pub line_index: usize,
    pub source_range: std::ops::Range<usize>,
    pub assignment_name: Option<String>,
    pub dependency_indices: Vec<usize>,
    pub loop_iteration: Option<Vec<usize>>,
    pub outcome: LineOutcome,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionEvaluation {
    pub basis: super::EvaluationBasis,
    pub source: Arc<str>,
    pub lines: Vec<SessionLineResult>,
    /// Physical rows holding the final bindings in this evaluation.
    pub(crate) final_bindings: Arc<HashMap<String, FinalBinding>>,
    pub diagnostic: Option<DocumentDiagnostic>,
    pub usage: Option<super::document::EvaluationUsage>,
}

pub(super) fn project_evaluation(
    evaluation: CalculationEvaluation<'_>,
    line_ids: &[LineId],
    basis: super::EvaluationBasis,
) -> Result<SessionEvaluation, String> {
    let source: Arc<str> = Arc::from(evaluation.source);
    if evaluation.diagnostic.is_none() && evaluation.lines.len() != line_ids.len() {
        return Err("Calculation results do not match the current source lines.".into());
    }
    let lines = evaluation
        .lines
        .into_iter()
        .map(|line| {
            let id = line_ids
                .get(line.line_index)
                .copied()
                .ok_or_else(|| "Calculation result refers to an unknown source line.".to_owned())?;
            Ok(SessionLineResult {
                id,
                line_index: line.line_index,
                source_range: line.source_range,
                assignment_name: line.assignment_name,
                dependency_indices: line.dependency_indices,
                loop_iteration: line.loop_iteration,
                outcome: line.outcome,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(SessionEvaluation {
        basis,
        source,
        lines,
        final_bindings: Arc::new(evaluation.final_bindings),
        diagnostic: evaluation.diagnostic,
        usage: evaluation.usage,
    })
}

#[derive(Debug)]
pub enum InspectorInsertError {
    PendingTransition,
    StaleSource,
    InvalidTarget,
    Edit(SessionError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingTransition {
    New,
    Open(DocumentId),
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionDecision {
    Save,
    Discard,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionOutcome {
    Applied,
    DecisionRequired,
    Cancelled,
}

#[derive(Debug)]
pub enum SessionError {
    Store(PadStoreError),
    Edit(EditError),
    NoActiveDocument,
    SaveAsRequired,
    RecoveryScratchRequired,
    PendingTransitionExists,
    NoPendingTransition,
    DocumentTooLarge { max_bytes: usize },
    TooManyLines { max_lines: usize },
    LineIdNotFound(LineId),
    LineIdStateMismatch,
    LineIdExhausted,
    RevisionExhausted,
    StaleRelatedEdit,
    InvalidRelatedMetadata(&'static str),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => error.fmt(formatter),
            Self::Edit(error) => error.fmt(formatter),
            Self::NoActiveDocument => {
                formatter.write_str("there is no active Calculation Pad document")
            }
            Self::SaveAsRequired => {
                formatter.write_str("save this untitled document with a name first")
            }
            Self::RecoveryScratchRequired => {
                formatter.write_str("recovery close requires an unsaved scratch document")
            }
            Self::PendingTransitionExists => {
                formatter.write_str("resolve the pending document transition first")
            }
            Self::NoPendingTransition => {
                formatter.write_str("there is no pending document transition")
            }
            Self::DocumentTooLarge { max_bytes } => {
                write!(formatter, "document exceeds the {max_bytes}-byte limit")
            }
            Self::TooManyLines { max_lines } => {
                write!(formatter, "document exceeds the {max_lines}-line limit")
            }
            Self::LineIdNotFound(id) => {
                write!(formatter, "line {} is not in the active document", id.get())
            }
            Self::LineIdStateMismatch => {
                formatter.write_str("line identity count does not match source lines")
            }
            Self::LineIdExhausted => formatter.write_str("no more stable line IDs are available"),
            Self::RevisionExhausted => formatter.write_str("document revision limit was reached"),
            Self::StaleRelatedEdit => formatter.write_str("the document changed before the edit was applied"),
            Self::InvalidRelatedMetadata(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for SessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            Self::Edit(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PadStoreError> for SessionError {
    fn from(error: PadStoreError) -> Self {
        Self::Store(error)
    }
}

impl From<EditError> for SessionError {
    fn from(error: EditError) -> Self {
        Self::Edit(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EditableState {
    source: String,
    line_ids: Vec<LineId>,
}

impl EditableState {
    fn byte_size(&self) -> usize {
        self.source.len().saturating_add(
            self.line_ids
                .len()
                .saturating_mul(std::mem::size_of::<LineId>()),
        )
    }
}

#[derive(Clone, Debug)]
struct HistoryEntry {
    before: EditableState,
    after: EditableState,
    related: RelatedReferenceEdits,
    watches: Vec<WatchReplay>,
    sensitivity_input_live: bool,
    sensitivity_outputs: Vec<WatchReplay>,
    chart_outputs: Vec<WatchReplay>,
    chart_x_live: bool,
    chart_distribution_live: bool,
}

#[derive(Clone, Debug)]
struct WatchReplay { id: Option<u64>, chart_was_hidden: bool }

#[derive(Clone, Debug, Default)]
pub(crate) struct RelatedReferenceEdits {
    pub inputs: Vec<RelatedInputRename>,
    pub watches: Vec<RelatedWatchRename>,
    pub sensitivity_input: Option<RelatedInputRename>,
    pub sensitivity_outputs: Vec<RelatedWatchRename>,
    pub chart_outputs: Vec<RelatedWatchRename>,
    pub chart_x: Option<RelatedWatchRename>,
    pub chart_distribution: Option<RelatedWatchRename>,
    pub chart_references: Vec<RelatedChartReferenceRename>,
}

#[derive(Clone, Debug)]
pub(crate) struct RelatedChartReferenceRename { pub id: u64, pub before: String, pub after: String }

#[derive(Clone, Debug)]
pub(crate) struct RelatedInputRename {
    pub id: LineId,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Debug)]
pub(crate) struct RelatedWatchRename {
    pub before: String,
    pub after: String,
}

impl RelatedReferenceEdits {
    fn byte_size(&self) -> usize {
        self.inputs.iter().map(|item| std::mem::size_of::<LineId>() + item.before.len() + item.after.len())
            .chain(self.watches.iter().map(|item| item.before.len() + item.after.len()))
            .chain(self.sensitivity_input.iter().map(|item| item.before.len() + item.after.len()))
            .chain(self.sensitivity_outputs.iter().map(|item| item.before.len() + item.after.len()))
            .chain(self.chart_outputs.iter().map(|item| item.before.len() + item.after.len()))
            .chain(self.chart_x.iter().map(|item| item.before.len() + item.after.len()))
            .chain(self.chart_distribution.iter().map(|item| item.before.len() + item.after.len()))
            .chain(self.chart_references.iter().map(|item| item.before.len() + item.after.len() + std::mem::size_of::<u64>()))
            .fold(0usize, usize::saturating_add)
    }

    fn apply(&self, view: &PadViewMetadata, reverse: bool, strict: bool) -> Result<PadViewMetadata, &'static str> {
        let mut result = view.clone();
        for (index, item) in self.inputs.iter().enumerate() {
            if self.inputs[..index].iter().any(|previous| previous.id == item.id) {
                return Err("Duplicate input reference edit.");
            }
            let (from, to) = if reverse { (&item.after, &item.before) } else { (&item.before, &item.after) };
            if let Some(input) = result.inputs.iter_mut().find(|input| input.id == item.id && input.name.as_str() == from.as_str()) {
                input.name.clone_from(to);
            } else if strict {
                return Err("An input reference changed before the edit was applied.");
            }
        }
        for (index, item) in self.watches.iter().enumerate() {
            let (from, _) = if reverse { (&item.after, &item.before) } else { (&item.before, &item.after) };
            if self.watches[..index].iter().any(|previous| if reverse { &previous.after == from } else { &previous.before == from }) {
                return Err("Duplicate watch reference edit.");
            }
            if strict && !view.watches.contains(from) {
                return Err("A watch reference changed before the edit was applied.");
            }
        }
        for watch in &mut result.watches {
            if let Some(item) = self.watches.iter().find(|item| if reverse { item.after.as_str() == watch.as_str() } else { item.before.as_str() == watch.as_str() }) {
                watch.clone_from(if reverse { &item.before } else { &item.after });
            }
        }
        if let Some(settings) = result.sensitivity.as_mut() {
            if let Some(item) = &self.sensitivity_input {
                let (from, to) = if reverse { (&item.after, &item.before) } else { (&item.before, &item.after) };
                if settings.definition.input == item.id && settings.input_name == *from { settings.input_name.clone_from(to); }
            }
            for item in &self.sensitivity_outputs {
                let (from, to) = if reverse { (&item.after, &item.before) } else { (&item.before, &item.after) };
                if !settings.definition.outputs.contains(to)
                    && let Some(output) = settings.definition.outputs.iter_mut().find(|name| *name == from) { output.clone_from(to); }
            }
        }
        if let Some(super::ChartDefinition { source: super::ChartSource::Sensitivity { outputs }, .. }) = result.chart.as_mut() {
            for item in &self.chart_outputs {
                let (from, to) = if reverse { (&item.after, &item.before) } else { (&item.before, &item.after) };
                if !outputs.contains(to) && let Some(output) = outputs.iter_mut().find(|name| *name == from) { output.clone_from(to); }
            }
        }
        if let Some(super::ChartDefinition { source: super::ChartSource::Vectors { x, y }, .. }) = result.chart.as_mut() {
            if let Some(item) = &self.chart_x {
                let (from, to) = if reverse { (&item.after, &item.before) } else { (&item.before, &item.after) };
                if matches!(x, Some(super::ChartVectorReference::Named { name }) if name == from) {
                    *x = Some(super::ChartVectorReference::Named { name: to.clone() });
                }
            }
            for item in &self.chart_outputs {
                let (from, to) = if reverse { (&item.after, &item.before) } else { (&item.before, &item.after) };
                let old = super::ChartVectorReference::Named { name: from.clone() };
                let new = super::ChartVectorReference::Named { name: to.clone() };
                if !y.contains(&new) && let Some(output) = y.iter_mut().find(|reference| **reference == old) { *output = new; }
            }
        }
        if let Some(chart) = result.chart.as_mut() {
            if let Some(item) = &self.chart_distribution {
                let (from, to) = if reverse { (&item.after, &item.before) } else { (&item.before, &item.after) };
                if let Some(distribution) = chart.distribution.as_mut() && matches!(&distribution.vector, super::ChartVectorReference::Named { name } if name == from) {
                    distribution.vector = super::ChartVectorReference::Named { name: to.clone() };
                    rename_distribution_style(chart, from, to);
                } else if strict { return Err("The distribution source changed before the edit was applied."); }
            }
            for item in &self.chart_outputs {
                let (from, to) = if reverse { (&item.after, &item.before) } else { (&item.before, &item.after) };
                let vector_source = matches!(chart.source, super::ChartSource::Vectors { .. });
                for style in &mut chart.options.styles {
                    match &mut style.key {
                        super::chart::options::SeriesKey::Vector(super::ChartVectorReference::Named { name }) if vector_source && name == from => name.clone_from(to),
                        super::chart::options::SeriesKey::Sensitivity(name) if !vector_source && name == from => name.clone_from(to),
                        _ => {}
                    }
                }
            }
            for item in &self.chart_references {
                let (from, to) = if reverse { (&item.after, &item.before) } else { (&item.before, &item.after) };
                if let Some(reference) = chart.options.references.iter_mut().find(|reference| reference.id == item.id && matches!(&reference.value, super::chart::options::ReferenceValue::Named(name) if name == from)) {
                    reference.value = super::chart::options::ReferenceValue::Named(to.clone());
                } else if strict { return Err("A chart reference changed before the edit was applied."); }
            }
        }
        super::persistence::validate_view_metadata(&result)?;
        Ok(result)
    }
}

impl HistoryEntry {
    fn replay(&mut self, view: &PadViewMetadata, ids: &[u64], hidden: &HashSet<u64>, sensitivity_ids: &[u64], sensitivity_hidden: &HashSet<u64>, chart_ids: &[u64], chart_hidden: &HashSet<u64>, hidden_styles: &HashMap<u64, super::chart::options::SeriesStyle>, reverse: bool) -> Result<(PadViewMetadata, Vec<u64>, HashSet<u64>, Vec<u64>, HashSet<u64>, Vec<u64>, HashSet<u64>, HashMap<u64, super::chart::options::SeriesStyle>), &'static str> {
        let inputs = RelatedReferenceEdits { inputs: self.related.inputs.clone(), watches: Vec::new(), sensitivity_input: self.related.sensitivity_input.clone().filter(|_| self.sensitivity_input_live), sensitivity_outputs: Vec::new(), chart_outputs: Vec::new(), chart_x: self.related.chart_x.clone().filter(|_| self.chart_x_live), chart_distribution: self.related.chart_distribution.clone().filter(|_| self.chart_distribution_live), chart_references: self.related.chart_references.clone() };
        let mut result = inputs.apply(view, reverse, false)?;
        let mut sensitivity_ids = sensitivity_ids.to_vec();
        let mut sensitivity_hidden = sensitivity_hidden.clone();
        if let Some(settings) = result.sensitivity.as_mut() {
            replay_values(&mut settings.definition.outputs, &mut sensitivity_ids, &mut sensitivity_hidden, &self.related.sensitivity_outputs, &mut self.sensitivity_outputs, 8, reverse, str::to_owned);
        } else { self.sensitivity_input_live = false; for state in &mut self.sensitivity_outputs { state.id = None; } }
        let mut ids = ids.to_vec();
        let mut hidden = hidden.clone();
        let mut chart_ids = chart_ids.to_vec();
        let mut chart_hidden = chart_hidden.clone();
        let mut hidden_styles = hidden_styles.clone();
        if let Some(chart) = result.chart.as_mut() {
            let old_keys = chart_source_keys(&chart.source);
            let mut detached = Vec::new();
            let mut styles = std::mem::take(&mut chart.options.styles).into_iter().filter_map(|style| {
                if let Some(id) = old_keys.iter().position(|key| *key == style.key).and_then(|index| chart_ids.get(index)).copied() { Some((id, style)) }
                else { detached.push(style); None }
            }).collect::<Vec<_>>();
            let mut visual_hidden = chart.hidden.iter().filter_map(|index| chart_ids.get(*index).copied()).collect::<HashSet<_>>();
            for state in &mut self.chart_outputs {
                if let Some(id) = state.id && chart_ids.contains(&id) {
                    state.chart_was_hidden = visual_hidden.contains(&id);
                }
            }
            match &mut chart.source {
                super::ChartSource::Sensitivity { outputs } => replay_values(outputs, &mut chart_ids, &mut chart_hidden, &self.related.chart_outputs, &mut self.chart_outputs, 8, reverse, str::to_owned),
                super::ChartSource::Vectors { y, .. } => replay_values(y, &mut chart_ids, &mut chart_hidden, &self.related.chart_outputs, &mut self.chart_outputs, 8, reverse, |name| super::ChartVectorReference::Named { name: name.to_owned() }),
            }
            let new_keys = chart_source_keys(&chart.source);
            for state in &mut self.chart_outputs {
                if let Some(id) = state.id {
                    if !chart_ids.contains(&id) {
                        if let Some(index) = styles.iter().position(|(owner, _)| *owner == id) { hidden_styles.insert(id, styles.remove(index).1); }
                    } else if let Some(style) = hidden_styles.remove(&id) { styles.push((id, style)); }
                }
            }
            chart.options.styles = styles.into_iter().filter_map(|(id, mut style)| {
                let index = chart_ids.iter().position(|owner| *owner == id)?;
                style.key = new_keys.get(index)?.clone();
                Some(style)
            }).collect();
            let distribution_key = chart.distribution.as_ref().map(|distribution| super::chart::options::SeriesKey::Distribution(distribution.vector.clone()));
            detached.retain(|style| Some(&style.key) == distribution_key.as_ref());
            chart.options.styles.extend(detached);
            for state in &self.chart_outputs {
                if let Some(id) = state.id && state.chart_was_hidden { visual_hidden.insert(id); }
            }
            chart.hidden = chart_ids.iter().enumerate().filter_map(|(index, id)| visual_hidden.contains(id).then_some(index)).collect();
        } else {
            for state in &mut self.chart_outputs { state.id = None; }
        }
        replay_values(&mut result.watches, &mut ids, &mut hidden, &self.related.watches, &mut self.watches, super::persistence::MAX_WATCHES, reverse, str::to_owned);
        super::persistence::validate_view_metadata(&result)?;
        Ok((result, ids, hidden, sensitivity_ids, sensitivity_hidden, chart_ids, chart_hidden, hidden_styles))
    }

    fn byte_size(&self) -> usize {
        self.before.byte_size().saturating_add(self.after.byte_size()).saturating_add(self.related.byte_size())
            .saturating_add((self.watches.len() + self.sensitivity_outputs.len() + self.chart_outputs.len()).saturating_mul(std::mem::size_of::<WatchReplay>()))
    }
}

fn replay_values<T: PartialEq>(names: &mut Vec<T>, ids: &mut Vec<u64>, hidden: &mut HashSet<u64>, edits: &[RelatedWatchRename], states: &mut [WatchReplay], cap: usize, reverse: bool, item: impl Fn(&str) -> T) {
        for (edit, state) in edits.iter().zip(states) {
            let Some(id) = state.id else { continue; };
            let to = item(if reverse { &edit.before } else { &edit.after });
            if !hidden.contains(&id) {
                let Some(index) = ids.iter().position(|candidate| *candidate == id) else { state.id = None; continue; };
                if names.contains(&to) {
                    names.remove(index);
                    ids.remove(index);
                    hidden.insert(id);
                } else { names[index] = to; }
            } else if !names.contains(&to) && names.len() < cap {
                names.push(to);
                ids.push(id);
                hidden.remove(&id);
            }
        }
}

#[derive(Default)]
struct EditHistory {
    undo: VecDeque<HistoryEntry>,
    redo: VecDeque<HistoryEntry>,
}

impl EditHistory {
    fn record(&mut self, before: EditableState, after: EditableState, related: RelatedReferenceEdits, watch_ids: Vec<Option<u64>>, sensitivity_ids: Vec<Option<u64>>, chart_ids: Vec<Option<u64>>) {
        if before == after {
            return;
        }
        self.redo.clear();
        let watches = watch_ids.into_iter().map(|id| WatchReplay { id, chart_was_hidden: false }).collect();
        let sensitivity_input_live = related.sensitivity_input.is_some();
        let sensitivity_outputs = sensitivity_ids.into_iter().map(|id| WatchReplay { id, chart_was_hidden: false }).collect();
        let chart_outputs = chart_ids.into_iter().map(|id| WatchReplay { id, chart_was_hidden: false }).collect();
        let chart_x_live = related.chart_x.is_some();
        let chart_distribution_live = related.chart_distribution.is_some();
        self.undo.push_back(HistoryEntry { before, after, related, watches, sensitivity_input_live, sensitivity_outputs, chart_outputs, chart_x_live, chart_distribution_live });
        self.trim();
    }

    fn trim(&mut self) {
        while self.undo.len().saturating_add(self.redo.len()) > MAX_UNDO_ENTRIES
            || self.total_bytes() > MAX_UNDO_BYTES
        {
            if self.undo.pop_front().is_none() {
                self.redo.pop_front();
            }
        }
    }

    fn total_bytes(&self) -> usize {
        self.undo
            .iter()
            .chain(self.redo.iter())
            .map(HistoryEntry::byte_size)
            .fold(0usize, usize::saturating_add)
    }

    fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[derive(Clone, Debug)]
struct DocumentBaseline {
    state: EditableState,
    id: DocumentId,
    name: Option<String>,
    disk_sha256: Option<[u8; 32]>,
    next_line_id: u64,
    view: PadViewMetadata,
    options: DocumentOptions,
    format_version: u32,
}

fn chart_output_names(view: &PadViewMetadata) -> Vec<String> {
    match view.chart.as_ref().map(|chart| &chart.source) {
        Some(super::ChartSource::Sensitivity { outputs }) => outputs.clone(),
        Some(super::ChartSource::Vectors { y, .. }) => y.iter().filter_map(|reference| match reference { super::ChartVectorReference::Named { name } => Some(name.clone()), _ => None }).collect(),
        _ => Vec::new(),
    }
}

fn chart_output_keys(view: &PadViewMetadata) -> Vec<super::chart::options::SeriesKey> {
    view.chart.as_ref().map_or_else(Vec::new, |chart| chart_source_keys(&chart.source))
}

fn chart_source_keys(source: &super::ChartSource) -> Vec<super::chart::options::SeriesKey> {
    match source {
        super::ChartSource::Sensitivity { outputs } => outputs.iter().cloned().map(super::chart::options::SeriesKey::Sensitivity).collect(),
        super::ChartSource::Vectors { y, .. } => y.iter().cloned().map(super::chart::options::SeriesKey::Vector).collect(),
    }
}

fn rename_distribution_style(chart: &mut super::ChartDefinition, from: &str, to: &str) {
    use super::chart::options::SeriesKey;
    let old = SeriesKey::Distribution(super::ChartVectorReference::Named { name: from.into() });
    let new = SeriesKey::Distribution(super::ChartVectorReference::Named { name: to.into() });
    let Some(index) = chart.options.styles.iter().position(|style| style.key == old) else { return; };
    chart.options.styles[index].key = new;
}

fn chart_x(view: &PadViewMetadata) -> Option<&super::ChartVectorReference> {
    match view.chart.as_ref().map(|chart| &chart.source) {
        Some(super::ChartSource::Vectors { x, .. }) => x.as_ref(),
        _ => None,
    }
}

fn chart_distribution(view: &PadViewMetadata) -> Option<&super::ChartVectorReference> {
    view.chart.as_ref()?.distribution.as_ref().map(|distribution| &distribution.vector)
}

struct ActiveDocument {
    id: DocumentId,
    incarnation: DocumentIncarnation,
    name: Option<String>,
    source: String,
    line_ids: Vec<LineId>,
    next_line_id: u64,
    revision: u64,
    generation: u64,
    view: PadViewMetadata,
    watch_ids: Vec<u64>,
    hidden_watch_ids: HashSet<u64>,
    next_watch_id: u64,
    sensitivity_output_ids: Vec<u64>,
    hidden_sensitivity_output_ids: HashSet<u64>,
    next_sensitivity_output_id: u64,
    chart_output_ids: Vec<u64>,
    hidden_chart_output_ids: HashSet<u64>,
    hidden_chart_styles: HashMap<u64, super::chart::options::SeriesStyle>,
    next_chart_output_id: u64,
    chart_reference_highwater: u64,
    options: DocumentOptions,
    baseline: DocumentBaseline,
    cached_evaluation: Option<Arc<SessionEvaluation>>,
    dependency_snapshot: Option<Arc<super::dependencies::DependencySnapshot>>,
    #[cfg(test)]
    dependency_build_count: usize,
    #[cfg(test)]
    evaluation_count: usize,
}

impl ActiveDocument {
    fn new_blank() -> Self {
        let id = DocumentId::fresh();
        let mut document = Self {
            id,
            incarnation: DocumentIncarnation(DocumentId::fresh()),
            name: None,
            source: String::new(),
            line_ids: vec![LineId(1)],
            next_line_id: 2,
            revision: 0,
            generation: 0,
            view: PadViewMetadata::default(),
            watch_ids: Vec::new(),
            hidden_watch_ids: HashSet::new(),
            next_watch_id: 1,
            sensitivity_output_ids: Vec::new(),
            hidden_sensitivity_output_ids: HashSet::new(),
            next_sensitivity_output_id: 1,
            chart_output_ids: Vec::new(),
            hidden_chart_output_ids: HashSet::new(),
            hidden_chart_styles: HashMap::new(),
            next_chart_output_id: 1,
            chart_reference_highwater: 1,
            options: DocumentOptions::default(),
            baseline: DocumentBaseline {
                state: EditableState {
                    source: String::new(),
                    line_ids: vec![LineId(1)],
                },
                id,
                name: None,
                disk_sha256: None,
                next_line_id: 2,
                view: PadViewMetadata::default(),
                options: DocumentOptions::default(),
                format_version: super::persistence::CURRENT_DOCUMENT_FORMAT_VERSION,
            },
            cached_evaluation: None,
            dependency_snapshot: None,
            #[cfg(test)]
            dependency_build_count: 0,
            #[cfg(test)]
            evaluation_count: 0,
        };
        document.baseline.state = document.editable_state();
        document
    }

    fn from_loaded(loaded: LoadedDocument) -> Self {
        let stored = loaded.stored;
        let saved_id = stored.id;
        let saved_name = stored.name.clone();
        let saved_next_line_id = stored.next_line_id;
        let mut document = Self {
            id: stored.id,
            incarnation: DocumentIncarnation(DocumentId::fresh()),
            name: Some(stored.name),
            source: stored.source,
            line_ids: stored.line_ids,
            next_line_id: stored.next_line_id,
            revision: 0,
            generation: 0,
            watch_ids: (1..=stored.view.watches.len() as u64).collect(),
            hidden_watch_ids: HashSet::new(),
            next_watch_id: stored.view.watches.len() as u64 + 1,
            sensitivity_output_ids: (1..=stored.view.sensitivity.as_ref().map_or(0, |settings| settings.definition.outputs.len()) as u64).collect(),
            hidden_sensitivity_output_ids: HashSet::new(),
            next_sensitivity_output_id: stored.view.sensitivity.as_ref().map_or(0, |settings| settings.definition.outputs.len()) as u64 + 1,
            chart_output_ids: (1..=chart_output_keys(&stored.view).len() as u64).collect(),
            hidden_chart_output_ids: HashSet::new(),
            hidden_chart_styles: HashMap::new(),
            next_chart_output_id: chart_output_keys(&stored.view).len() as u64 + 1,
            chart_reference_highwater: stored.view.chart.as_ref().map_or(1, |chart| chart.options.next_reference_id),
            view: stored.view.clone(),
            options: stored.options,
            baseline: DocumentBaseline {
                state: EditableState {
                    source: String::new(),
                    line_ids: Vec::new(),
                },
                id: saved_id,
                name: Some(saved_name),
                disk_sha256: Some(loaded.disk_sha256),
                next_line_id: saved_next_line_id,
                view: stored.view,
                options: stored.options,
                format_version: stored.format_version,
            },
            cached_evaluation: None,
            dependency_snapshot: None,
            #[cfg(test)]
            dependency_build_count: 0,
            #[cfg(test)]
            evaluation_count: 0,
        };
        document.baseline.state = document.editable_state();
        document
    }

    fn editable_state(&self) -> EditableState {
        EditableState {
            source: self.source.clone(),
            line_ids: self.line_ids.clone(),
        }
    }

    fn is_dirty(&self) -> bool {
        self.source != self.baseline.state.source
            || self.line_ids != self.baseline.state.line_ids
            || self.id != self.baseline.id
            || self.name != self.baseline.name
            || self.next_line_id != self.baseline.next_line_id
            || self.view != self.baseline.view
            || self.options != self.baseline.options
    }

    fn stored_document(&self) -> Result<StoredDocument, SessionError> {
        let name = self.name.clone().ok_or(SessionError::SaveAsRequired)?;
        let mut stored = StoredDocument::new(self.id, name, self.source.clone(), self.line_ids.clone(), self.next_line_id);
        stored.view = self.view.clone();
        stored.options = self.options;
        Ok(stored)
    }
}

#[derive(Clone, Copy)]
enum PendingAction {
    New,
    Open(DocumentId),
    Close,
}

impl PendingAction {
    fn public_kind(&self) -> PendingTransition {
        match self {
            Self::New => PendingTransition::New,
            Self::Open(id) => PendingTransition::Open(*id),
            Self::Close => PendingTransition::Close,
        }
    }
}

/// Source editing, evaluation caching, bounded undo/redo, and explicit
/// save/discard/cancel transitions for one active document.
pub struct CalculationPadSession {
    store: PadDocumentStore,
    current: Option<ActiveDocument>,
    pending: Option<PendingAction>,
    history: EditHistory,
    evaluation_wake: Option<Arc<dyn Fn() + Send + Sync>>,
    evaluation_worker: Option<EvaluationWorker>,
    scheduled_evaluation: Option<EvaluationStamp>,
    evaluation_failure: Option<(EvaluationStamp, String)>,
    canceled_evaluation: Option<EvaluationStamp>,
    sensitivity_worker: Option<super::sensitivity::SensitivityWorker>,
    sensitivity_active: Option<super::SensitivityBasis>,
    sensitivity_progress: Option<(Arc<std::sync::atomic::AtomicUsize>, usize)>,
    sensitivity_table: Option<Arc<super::SensitivityTable>>,
    sensitivity_failure: Option<String>,
    sensitivity_canceled: bool,
    sensitivity_generation: u64,
    #[cfg(test)]
    evaluation_attempts: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvaluationStatus {
    Ready,
    Pending,
    NotCalculated,
    Outdated,
    Canceled,
    Failed(String),
    Inactive,
}

#[derive(Clone, Debug)]
pub(crate) struct RenamePreview {
    pub basis: super::EvaluationBasis,
    pub plan: super::RenamePlan,
    caret: usize,
    replacement: String,
    view: PadViewMetadata,
}

impl CalculationPadSession {
    pub(crate) fn rename_target(&self, caret: usize) -> Result<(String, super::RenameScope), String> {
        if self.pending.is_some() { return Err("Resolve the pending document transition first.".into()); }
        let source = self.source().ok_or("No document is open.")?;
        super::document::rename::target(source, caret)
    }
    pub(crate) fn rename_preview_is_current(&self, preview: &RenamePreview) -> bool {
        self.pending.is_none() && self.evaluation_basis() == Some(preview.basis)
            && self.current.as_ref().is_some_and(|current| current.view == preview.view)
    }
    pub(crate) fn preview_rename(&self, caret: usize, replacement: &str) -> Result<RenamePreview, String> {
        let current = self.current.as_ref().ok_or("No document is open.")?;
        if self.pending.is_some() { return Err("Resolve the pending document transition first.".into()); }
        let basis = self.evaluation_basis().ok_or("No document is open.")?;
        let plan = super::document::rename::plan(&current.source, caret, replacement)?;
        validate_source_size(&plan.source).map_err(|error| error.to_string())?;
        if matches!(plan.scope, super::RenameScope::Document)
            && current.view.watches.contains(&plan.old_name) && current.view.watches.contains(&plan.new_name) {
            return Err("The new name is already pinned as a watch.".into());
        }
        if matches!(plan.scope, super::RenameScope::Document)
            && current.view.sensitivity.as_ref().is_some_and(|settings| settings.definition.outputs.contains(&plan.old_name) && settings.definition.outputs.contains(&plan.new_name)) {
            return Err("The new name is already a sensitivity output.".into());
        }
        if matches!(plan.scope, super::RenameScope::Document) && chart_output_names(&current.view).contains(&plan.old_name) && chart_output_names(&current.view).contains(&plan.new_name) {
            return Err("The new name is already a chart output.".into());
        }
        Ok(RenamePreview { basis, plan, caret, replacement: replacement.to_owned(), view: current.view.clone() })
    }

    pub(crate) fn apply_rename(&mut self, preview: &RenamePreview) -> Result<bool, String> {
        self.ensure_editable().map_err(|error| error.to_string())?;
        if self.evaluation_basis() != Some(preview.basis) { return Err("The document changed; preview the rename again.".into()); }
        let current = self.current.as_ref().ok_or("No document is open.")?;
        if current.view != preview.view || super::document::rename::plan(&current.source, preview.caret, &preview.replacement)? != preview.plan {
            return Err("The document or related references changed; preview the rename again.".into());
        }
        validate_source_size(&preview.plan.source).map_err(|error| error.to_string())?;
        let mut related = RelatedReferenceEdits::default();
        if matches!(preview.plan.scope, super::RenameScope::Document) {
            for input in &current.view.inputs {
                if input.name == preview.plan.old_name {
                    related.inputs.push(RelatedInputRename { id: input.id, before: preview.plan.old_name.clone(), after: preview.plan.new_name.clone() });
                }
            }
            if current.view.watches.contains(&preview.plan.old_name) {
                related.watches.push(RelatedWatchRename { before: preview.plan.old_name.clone(), after: preview.plan.new_name.clone() });
            }
            if let Some(settings) = &current.view.sensitivity {
                if settings.input_name == preview.plan.old_name {
                    related.sensitivity_input = Some(RelatedInputRename { id: settings.definition.input, before: preview.plan.old_name.clone(), after: preview.plan.new_name.clone() });
                }
                if settings.definition.outputs.contains(&preview.plan.old_name) {
                    related.sensitivity_outputs.push(RelatedWatchRename { before: preview.plan.old_name.clone(), after: preview.plan.new_name.clone() });
                }
            }
            if chart_output_names(&current.view).contains(&preview.plan.old_name) {
                related.chart_outputs.push(RelatedWatchRename { before: preview.plan.old_name.clone(), after: preview.plan.new_name.clone() });
            }
            if matches!(chart_x(&current.view), Some(super::ChartVectorReference::Named { name }) if name == &preview.plan.old_name) {
                related.chart_x = Some(RelatedWatchRename { before: preview.plan.old_name.clone(), after: preview.plan.new_name.clone() });
            }
            if matches!(chart_distribution(&current.view), Some(super::ChartVectorReference::Named { name }) if name == &preview.plan.old_name) {
                related.chart_distribution = Some(RelatedWatchRename { before: preview.plan.old_name.clone(), after: preview.plan.new_name.clone() });
            }
            if let Some(chart) = &current.view.chart {
                for reference in &chart.options.references {
                    if matches!(&reference.value, super::chart::options::ReferenceValue::Named(name) if name == &preview.plan.old_name) {
                        related.chart_references.push(RelatedChartReferenceRename { id: reference.id, before: preview.plan.old_name.clone(), after: preview.plan.new_name.clone() });
                    }
                }
            }
        }
        let line_ids = current.line_ids.clone();
        let next_id = current.next_line_id;
        self.commit_source_edit(preview.plan.source.clone(), line_ids, next_id, related).map_err(|error| error.to_string())
    }
    /// Creates an immediately usable blank session without touching the filesystem.
    pub fn new(root: &AppDataRoot) -> Self {
        Self::with_store(PadDocumentStore::new(root))
    }

    pub fn with_store(store: PadDocumentStore) -> Self {
        Self {
            store,
            current: Some(ActiveDocument::new_blank()),
            pending: None,
            history: EditHistory::default(),
            evaluation_wake: None,
            evaluation_worker: None,
            scheduled_evaluation: None,
            evaluation_failure: None,
            canceled_evaluation: None,
            sensitivity_worker: None,
            sensitivity_active: None,
            sensitivity_progress: None,
            sensitivity_table: None,
            sensitivity_failure: None,
            sensitivity_canceled: false,
            sensitivity_generation: 0,
            #[cfg(test)]
            evaluation_attempts: 0,
        }
    }

    /// Enable nonblocking script evaluation for the owning GUI session.
    pub(crate) fn enable_background_evaluation(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        if self.evaluation_wake.is_none() {
            self.evaluation_wake = Some(Arc::new(wake));
        }
    }

    fn sensitivity_matches(&self, basis: &super::SensitivityBasis) -> bool {
        self.pending.is_none() && self.current.as_ref().is_some_and(|current| {
            current.id == basis.document && current.incarnation == basis.incarnation
                && current.revision == basis.revision && current.options == basis.options
                && current.line_ids.contains(&basis.definition.input)
        })
    }

    fn retire_sensitivity(&mut self) {
        if let Some(worker) = &self.sensitivity_worker { worker.retire(); }
        self.sensitivity_active = None;
        self.sensitivity_progress = None;
        self.sensitivity_failure = None;
        self.sensitivity_canceled = false;
    }

    fn invalidate_stale_sensitivity(&mut self) {
        if self.sensitivity_active.as_ref().is_some_and(|basis| !self.sensitivity_matches(basis)) { self.retire_sensitivity(); }
    }

    pub(crate) fn request_sensitivity(&mut self, definition: super::SensitivityDefinition) -> Result<super::SensitivityBasis, String> {
        self.ensure_editable().map_err(|error| error.to_string())?;
        let current = self.current.as_ref().ok_or("No document is open.")?;
        let limits = super::limits::profile_limits(current.options.profile);
        if current.source.len() > limits.bytes || physical_rows(&current.source) > limits.lines { return Err("The source exceeds the selected document profile.".into()); }
        let line = current.line_ids.iter().position(|id| *id == definition.input).ok_or("The selected input no longer exists.")?;
        let descriptor = super::document::literal_inputs(&current.source).into_iter().find(|input| input.line == line).ok_or("The selected input is not an eligible literal.")?;
        let generation = self.sensitivity_generation.checked_add(1).ok_or("Sensitivity generation exhausted.")?;
        let basis = super::SensitivityBasis { document: current.id, incarnation: current.incarnation, revision: current.revision, options: current.options,
            input_name: descriptor.name.clone(), definition, generation };
        let request = super::sensitivity::Request::new(basis.clone(), current.source.clone(), descriptor)?;
        let progress = Arc::clone(&request.progress);
        let count = request.count();
        if self.sensitivity_worker.is_none() {
            let wake = self.evaluation_wake.clone().unwrap_or_else(|| Arc::new(|| {}));
            self.sensitivity_worker = Some(super::sensitivity::SensitivityWorker::start(wake)?);
        }
        self.sensitivity_generation = generation;
        self.sensitivity_active = Some(basis.clone());
        self.sensitivity_progress = Some((progress, count));
        self.sensitivity_failure = None;
        self.sensitivity_canceled = false;
        if let Some(worker) = &self.sensitivity_worker { worker.submit(request); }
        Ok(basis)
    }

    pub(crate) fn poll_sensitivity(&mut self) {
        self.invalidate_stale_sensitivity();
        let completion = self.sensitivity_worker.as_ref().and_then(|worker| worker.take());
        if let Some((basis, result)) = completion
            && self.sensitivity_active.as_ref() == Some(&basis) && self.sensitivity_matches(&basis) {
            self.sensitivity_progress = None;
            match result { Ok(table) if table.basis == basis => self.sensitivity_table = Some(Arc::new(table)),
                Ok(_) => self.sensitivity_failure = Some("Sensitivity result identity is invalid.".into()),
                Err(message) => self.sensitivity_failure = Some(message), }
        }
    }

    pub(crate) fn cancel_sensitivity(&mut self, basis: &super::SensitivityBasis) -> bool {
        if self.sensitivity_active.as_ref() != Some(basis) || self.sensitivity_progress.is_none() { return false; }
        if let Some(worker) = &self.sensitivity_worker { worker.retire(); }
        self.sensitivity_active = None;
        self.sensitivity_progress = None;
        self.sensitivity_failure = None;
        self.sensitivity_canceled = true;
        true
    }

    pub(crate) fn sensitivity_definition_changed(&mut self, definition: &super::SensitivityDefinition) {
        if self.sensitivity_active.as_ref().is_some_and(|basis| &basis.definition != definition) {
            self.retire_sensitivity();
        }
    }

    pub(crate) fn sensitivity_status(&self) -> super::SensitivityStatus {
        if self.sensitivity_canceled { return super::SensitivityStatus::Canceled; }
        if let Some(basis) = &self.sensitivity_active {
            if !self.sensitivity_matches(basis) { return super::SensitivityStatus::Outdated; }
            if let Some((progress, total)) = &self.sensitivity_progress { return super::SensitivityStatus::Pending { completed: progress.load(std::sync::atomic::Ordering::Relaxed), total: *total }; }
            if let Some(error) = &self.sensitivity_failure { return super::SensitivityStatus::Failed(error.clone()); }
            if self.sensitivity_table.as_ref().is_some_and(|table| table.basis == *basis) { return super::SensitivityStatus::Ready; }
        }
        if self.sensitivity_table.is_some() { super::SensitivityStatus::Outdated } else { super::SensitivityStatus::Idle }
    }

    pub(crate) fn active_sensitivity_basis(&self) -> Option<super::SensitivityBasis> {
        self.sensitivity_active.clone()
    }

    pub(crate) fn sensitivity_table(&self) -> Option<Arc<super::SensitivityTable>> {
        let basis = self.sensitivity_active.as_ref()?;
        if !self.sensitivity_matches(basis) || self.sensitivity_progress.is_some() || self.sensitivity_failure.is_some() { return None; }
        if self.current.as_ref().and_then(|current| current.view.sensitivity.as_ref()).is_some_and(|settings| settings.definition != basis.definition || settings.input_name != basis.input_name) { return None; }
        self.sensitivity_table.as_ref().filter(|table| table.basis == *basis).cloned()
    }

    fn evaluation_stamp(&self) -> Option<EvaluationStamp> {
        self.current.as_ref().map(|current| EvaluationStamp {
            document: current.id,
            incarnation: current.incarnation,
            revision: current.revision,
            generation: current.generation,
        })
    }

    pub(crate) fn retire_background_evaluation(&mut self) {
        if let Some(worker) = &self.evaluation_worker {
            worker.retire();
        }
        self.scheduled_evaluation = None;
        self.evaluation_failure = None;
        self.canceled_evaluation = None;
        if let Some(current) = self.current.as_mut() {
            if let Some(next) = current.generation.checked_add(1) {
                current.generation = next;
            } else {
                current.incarnation = DocumentIncarnation(DocumentId::fresh());
                current.generation = 0;
            }
        }
    }

    pub fn evaluation_status(&self) -> EvaluationStatus {
        let Some(stamp) = self.evaluation_stamp() else {
            return EvaluationStatus::Inactive;
        };
        if let Some((failed, message)) = &self.evaluation_failure
            && *failed == stamp
        {
            return EvaluationStatus::Failed(message.clone());
        }
        if self.canceled_evaluation == Some(stamp) {
            return EvaluationStatus::Canceled;
        }
        if self.scheduled_evaluation == Some(stamp) {
            return EvaluationStatus::Pending;
        }
        if self
            .current
            .as_ref()
            .and_then(|current| current.cached_evaluation.as_ref())
            .is_some_and(|evaluation| evaluation.basis == stamp)
        {
            EvaluationStatus::Ready
        } else if self.current.as_ref().is_some_and(|current| current.cached_evaluation.is_some()) {
            EvaluationStatus::Outdated
        } else {
            EvaluationStatus::NotCalculated
        }
    }

    pub(crate) fn view_metadata(&self) -> Option<&PadViewMetadata> {
        self.current.as_ref().map(|document| &document.view)
    }

    pub fn document_options(&self) -> Option<DocumentOptions> {
        self.current.as_ref().map(|document| document.options)
    }

    pub fn set_document_options(&mut self, options: DocumentOptions) -> Result<bool, SessionError> {
        self.ensure_editable()?;
        if self.current.as_ref().is_some_and(|document| document.options == options) { return Ok(false); }
        self.retire_sensitivity();
        self.retire_background_evaluation();
        let document = self.current.as_mut().ok_or(SessionError::NoActiveDocument)?;
        document.options = options;
        document.dependency_snapshot = None;
        Ok(true)
    }

    pub fn evaluation_basis(&self) -> Option<super::EvaluationBasis> {
        self.evaluation_stamp()
    }

    pub(crate) fn is_view_dirty(&self) -> bool {
        self.current.as_ref().is_some_and(|document| document.view != document.baseline.view)
    }

    pub(crate) fn set_view_metadata(&mut self, mut view: PadViewMetadata) -> Result<(), &'static str> {
        if self.pending.is_some() { return Err("Resolve the pending document transition first."); }
        let current = self.current.as_ref().ok_or("No document is open.")?;
        if let Some(chart) = view.chart.as_mut() {
            if current.view.chart.as_ref().is_some_and(|old| old.source != chart.source || old.distribution.as_ref().map(|value| &value.vector) != chart.distribution.as_ref().map(|value| &value.vector)) {
                chart.options.retain_selected_styles(&chart.source, chart.distribution.as_ref().map(|distribution| &distribution.vector));
            }
            let old_refs = current.view.chart.as_ref().map(|old| old.options.references.as_slice()).unwrap_or_default();
            if chart.options.references.iter().any(|reference| reference.id < current.chart_reference_highwater && !old_refs.iter().any(|old| old.id == reference.id)) {
                return Err("A retired chart reference identity cannot be reused.");
            }
            chart.options.next_reference_id = chart.options.next_reference_id.max(current.chart_reference_highwater);
        }
        if self.current.as_ref().is_some_and(|document| document.view == view) { return Ok(()); }
        super::persistence::validate_view_metadata(&view)?;
        let old_refs = current.view.chart.as_ref().map(|chart| chart.options.references.as_slice()).unwrap_or_default();
        let new_refs = view.chart.as_ref().map(|chart| chart.options.references.as_slice()).unwrap_or_default();
        let retired = old_refs.iter().filter(|old| !new_refs.iter().any(|new| new.id == old.id && new.value == old.value)).map(|reference| reference.id).collect::<HashSet<_>>();
        let (sensitivity_ids, sensitivity_next) = if let Some(current) = self.current.as_ref() {
            let old = current.view.sensitivity.as_ref();
            let new = view.sensitivity.as_ref();
            let old_names = old.map(|settings| settings.definition.outputs.as_slice()).unwrap_or_default();
            let new_names = new.map(|settings| settings.definition.outputs.as_slice()).unwrap_or_default();
            let mut next = current.next_sensitivity_output_id;
            let mut ids = Vec::with_capacity(new_names.len());
            for name in new_names {
                let id = if let Some(index) = old_names.iter().position(|old| old == name) { current.sensitivity_output_ids[index] }
                    else { let id = next; next = next.checked_add(1).ok_or("Sensitivity output identity space exhausted.")?; id };
                ids.push(id);
            }
            let removed: HashSet<_> = current.sensitivity_output_ids.iter().copied().filter(|id| !ids.contains(id)).collect();
            for entry in self.history.undo.iter_mut().chain(self.history.redo.iter_mut()) {
                for state in &mut entry.sensitivity_outputs {
                    if state.id.is_some_and(|id| removed.contains(&id)) { state.id = None; }
                }
                if let Some(item) = &entry.related.sensitivity_input
                    && old.is_some_and(|settings| settings.definition.input == item.id)
                    && old.map(|settings| (settings.definition.input, &settings.input_name)) != new.map(|settings| (settings.definition.input, &settings.input_name))
                { entry.sensitivity_input_live = false; }
            }
            (ids, next)
        } else { return Err("No document is open."); };
        if self.current.as_ref().is_some_and(|current| current.view.sensitivity != view.sensitivity)
            && self.sensitivity_active.as_ref().is_some_and(|basis| view.sensitivity.as_ref().is_none_or(|settings| settings.definition != basis.definition || settings.input_name != basis.input_name)) {
            self.retire_sensitivity();
        }
        let (chart_ids, chart_next) = if let Some(current) = self.current.as_ref() {
            let old = chart_output_keys(&current.view);
            let new = chart_output_keys(&view);
            let mut next = current.next_chart_output_id;
            let mut ids = Vec::with_capacity(new.len());
            for name in &new {
                let id = if let Some(index) = old.iter().position(|old| old == name) { current.chart_output_ids[index] }
                    else { let id = next; next = next.checked_add(1).ok_or("Chart output identity space exhausted.")?; id };
                ids.push(id);
            }
            let mut removed: HashSet<_> = current.chart_output_ids.iter().copied().filter(|id| !ids.contains(id)).collect();
            let kind = |view: &PadViewMetadata| view.chart.as_ref().map(|chart| matches!(chart.source, super::ChartSource::Vectors { .. }));
            if kind(&current.view) != kind(&view) { removed.extend(current.hidden_chart_output_ids.iter().copied()); }
            for entry in self.history.undo.iter_mut().chain(self.history.redo.iter_mut()) {
                for state in &mut entry.chart_outputs { if state.id.is_some_and(|id| removed.contains(&id)) { state.id = None; } }
                if chart_x(&current.view) != chart_x(&view) { entry.chart_x_live = false; }
                if chart_distribution(&current.view) != chart_distribution(&view) { entry.chart_distribution_live = false; }
            }
            (ids, next)
        } else { return Err("No document is open."); };
        for entry in self.history.undo.iter_mut().chain(self.history.redo.iter_mut()) {
            entry.related.chart_references.retain(|reference| !retired.contains(&reference.id));
        }
        let document = self.current.as_mut().ok_or("No document is open.")?;
        document.chart_reference_highwater = view.chart.as_ref().map_or(document.chart_reference_highwater, |chart| document.chart_reference_highwater.max(chart.options.next_reference_id));
        document.sensitivity_output_ids = sensitivity_ids;
        document.next_sensitivity_output_id = sensitivity_next;
        document.chart_output_ids = chart_ids;
        document.next_chart_output_id = chart_next;
        if document.view.watches == view.watches {
            document.view = view;
            self.prune_hidden_watch_ids();
            return Ok(());
        }
        let mut next_id = document.next_watch_id;
        let mut ids = Vec::with_capacity(view.watches.len());
        for name in &view.watches {
            let id = if let Some(index) = document.view.watches.iter().position(|old| old == name) {
                document.watch_ids[index]
            } else {
                let id = next_id;
                next_id = next_id.checked_add(1).ok_or("Watch identity space exhausted.")?;
                id
            };
            ids.push(id);
        }
        let removed: HashSet<_> = document.watch_ids.iter().copied().filter(|id| !ids.contains(id)).collect();
        for entry in self.history.undo.iter_mut().chain(self.history.redo.iter_mut()) {
            for state in &mut entry.watches {
                if state.id.is_some_and(|id| removed.contains(&id)) { state.id = None; }
            }
        }
        document.view = view;
        document.watch_ids = ids;
        document.next_watch_id = next_id;
        self.prune_hidden_watch_ids();
        Ok(())
    }

    fn prune_hidden_watch_ids(&mut self) {
        let retained = self.history.undo.iter().chain(&self.history.redo)
            .flat_map(|entry| entry.watches.iter().filter_map(|watch| watch.id))
            .collect::<HashSet<_>>();
        if let Some(current) = self.current.as_mut() {
            current.hidden_watch_ids.retain(|id| retained.contains(id));
            let retained = self.history.undo.iter().chain(&self.history.redo).flat_map(|entry| entry.sensitivity_outputs.iter().filter_map(|state| state.id)).collect::<HashSet<_>>();
            current.hidden_sensitivity_output_ids.retain(|id| retained.contains(id));
            let retained = self.history.undo.iter().chain(&self.history.redo).flat_map(|entry| entry.chart_outputs.iter().filter_map(|state| state.id)).collect::<HashSet<_>>();
            current.hidden_chart_output_ids.retain(|id| retained.contains(id));
            current.hidden_chart_styles.retain(|id, _| current.hidden_chart_output_ids.contains(id));
        }
    }

    pub fn current_id(&self) -> Option<DocumentId> {
        self.current.as_ref().map(|document| document.id)
    }

    pub fn current_name(&self) -> Option<&str> {
        self.current
            .as_ref()
            .and_then(|document| document.name.as_deref())
    }

    pub fn source(&self) -> Option<&str> {
        self.current
            .as_ref()
            .map(|document| document.source.as_str())
    }

    pub fn line_ids(&self) -> Option<&[LineId]> {
        self.current
            .as_ref()
            .map(|document| document.line_ids.as_slice())
    }

    pub fn line_id(&self, line_index: usize) -> Option<LineId> {
        self.current
            .as_ref()
            .and_then(|document| document.line_ids.get(line_index).copied())
    }

    pub fn revision(&self) -> Option<u64> {
        self.current.as_ref().map(|document| document.revision)
    }

    pub fn is_dirty(&self) -> bool {
        self.current.as_ref().is_some_and(ActiveDocument::is_dirty)
    }

    pub fn is_saved(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|document| document.baseline.disk_sha256.is_some())
    }

    pub fn can_undo(&self) -> bool {
        self.pending.is_none() && !self.history.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        self.pending.is_none() && !self.history.redo.is_empty()
    }

    pub fn pending_transition(&self) -> Option<PendingTransition> {
        self.pending.as_ref().map(PendingAction::public_kind)
    }

    /// Identity of this installed document, including same-file reopenings.
    pub fn document_incarnation(&self) -> Option<DocumentIncarnation> {
        self.current.as_ref().map(|document| document.incarnation)
    }

    /// Freeze the already-current cached physical rows without reevaluating the Pad.
    pub fn comparison_context(&self) -> Option<super::comparison::ComparisonContext> {
        let document = self.current.as_ref()?;
        let evaluation = document.cached_evaluation.as_ref()?;
        if Some(evaluation.basis) != self.evaluation_stamp() || evaluation.diagnostic.is_some() {
            return None;
        }
        let rows = evaluation
            .lines
            .iter()
            .map(|line| super::comparison::ComparisonRow {
                id: line.id,
                assignment_name: line.assignment_name.clone(),
                outcome: line.outcome.clone(),
            })
            .collect::<Vec<_>>();
        Some(super::comparison::ComparisonContext {
            basis: evaluation.basis,
            incarnation: document.incarnation,
            document_id: document.id,
            revision: document.revision,
            rows: rows.into(),
            final_bindings: Arc::clone(&evaluation.final_bindings),
        })
    }

    /// Read the current dependency graph without triggering Pad evaluation.
    pub fn dependency_snapshot(&mut self) -> Option<Arc<super::DependencySnapshot>> {
        let current = self.current.as_mut()?;
        let evaluation = current.cached_evaluation.as_ref()?;
        if evaluation.basis != (EvaluationStamp { document: current.id, incarnation: current.incarnation, revision: current.revision, generation: current.generation }) {
            return None;
        }
        if let Some(snapshot) = current.dependency_snapshot.as_ref()
            && snapshot.basis == evaluation.basis
        {
            return Some(Arc::clone(snapshot));
        }
        let snapshot = Arc::new(super::DependencySnapshot::from_evaluation(
            current.id,
            current.incarnation,
            &current.source,
            evaluation,
        )?);
        current.dependency_snapshot = Some(Arc::clone(&snapshot));
        #[cfg(test)]
        {
            current.dependency_build_count += 1;
        }
        Some(snapshot)
    }

    /// Resolve a stable row against the currently cached evaluation, never reevaluating.
    pub fn resolve_dependency_line(
        &self,
        document_id: DocumentId,
        incarnation: DocumentIncarnation,
        id: LineId,
    ) -> Option<super::DependencyLocation> {
        let current = self.current.as_ref()?;
        if current.id != document_id || current.incarnation != incarnation {
            return None;
        }
        let evaluation = current.cached_evaluation.as_ref()?;
        if Some(evaluation.basis) != self.evaluation_stamp() {
            return None;
        }
        let row = evaluation.lines.iter().find(|row| row.id == id)?;
        current.source.get(row.source_range.clone())?;
        Some(super::DependencyLocation {
            line_index: row.line_index,
            source_range: row.source_range.clone(),
        })
    }

    /// Resolve only an already-current cached result; never evaluate as an observation.
    pub fn inspector_source(&self, line_id: LineId) -> Option<InspectorSourceSnapshot> {
        let current = self.current.as_ref()?;
        let evaluation = current.cached_evaluation.as_ref()?;
        if Some(evaluation.basis) != self.evaluation_stamp() {
            return None;
        }
        let line = evaluation.lines.iter().find(|line| line.id == line_id)?;
        let (value, origin) = match &line.outcome {
            LineOutcome::Value(CalculationValue::Integer(value)) => {
                (*value, InspectorSourceOrigin::TypedInteger)
            }
            LineOutcome::Value(CalculationValue::ExactBase { value, .. }) => {
                let width = if *value <= u64::MAX as u128 {
                    IntegerWidth::Bits64
                } else {
                    IntegerWidth::Bits128
                };
                (
                    TypedInteger::from_unsigned(*value, IntegerMode::unsigned(width)).ok()?,
                    InspectorSourceOrigin::LegacyExactBase,
                )
            }
            _ => return None,
        };
        Some(InspectorSourceSnapshot {
            incarnation: current.incarnation,
            document_id: current.id,
            revision: current.revision,
            basis: evaluation.basis,
            line_id,
            source_range: line.source_range.clone(),
            source_text: std::sync::Arc::from(current.source.get(line.source_range.clone())?),
            value,
            origin,
        })
    }

    /// Apply one canonical replacement only while its original source binding remains current.
    pub fn apply_inspector_insert(
        &mut self,
        request: InspectorInsertRequest,
    ) -> Result<bool, InspectorInsertError> {
        if self.pending.is_some() {
            return Err(InspectorInsertError::PendingTransition);
        }
        if self.inspector_source(request.source.line_id).as_ref() != Some(&request.source) {
            return Err(InspectorInsertError::StaleSource);
        }
        let current = self
            .current
            .as_ref()
            .ok_or(InspectorInsertError::StaleSource)?;
        if request.target.start > request.target.end
            || current.source.get(request.target.clone()).is_none()
        {
            return Err(InspectorInsertError::InvalidTarget);
        }
        let mut replacement = current.source.clone();
        replacement.replace_range(request.target, &request.expression);
        self.replace_source(replacement)
            .map_err(InspectorInsertError::Edit)
    }

    pub fn list_saved_documents(&self) -> Result<Vec<PadDocumentSummary>, SessionError> {
        self.store.list().map_err(Into::into)
    }

    /// Prepare a literal edit against the exact source snapshot without mutating it.
    pub(crate) fn input_replacement(&self, revision: u64, line_id: LineId, original: &str, value: &str) -> Result<String, String> {
        if self.pending.is_some() || self.revision() != Some(revision) {
            return Err("The source changed; review this input before applying it.".into());
        }
        if value.contains(['\r', '\n']) { return Err("Enter a single literal value.".into()); }
        let source = self.source().ok_or("No document is open.")?;
        let line = self.line_ids().and_then(|ids| ids.iter().position(|id| *id == line_id))
            .ok_or("This input no longer exists.")?;
        let descriptor = super::document::literal_inputs(source).into_iter()
            .find(|input| input.line == line && input.source_line == original)
            .ok_or("This input changed or is no longer a literal.")?;
        let selected = self.current.as_ref().and_then(|current| current.view.inputs.iter().find(|input| input.id == line_id));
        if selected.is_some_and(|input| input.unavailable || input.name != descriptor.name) { return Err("This selected input is unavailable or changed.".into()); }
        let default = super::input::InputConfig::default();
        let config = selected.map_or(&default, |input| &input.config);
        let replacement = super::input::replace_literal(source, &descriptor, config, value)?;
        validate_source_size(&replacement).map_err(|error| error.to_string())?;
        Ok(replacement)
    }

    pub(crate) fn apply_input(&mut self, basis: super::EvaluationBasis, line_id: LineId, original: &str, config: &super::input::InputConfig, value: &str) -> Result<bool, String> {
        if self.evaluation_basis() != Some(basis) { return Err("Source changed; reset this draft.".into()); }
        let current = self.current.as_ref().ok_or("No document is open.")?;
        if !current.view.inputs.iter().any(|input| input.id == line_id && !input.unavailable && &input.config == config) {
            return Err("Input settings changed; reset this draft.".into());
        }
        let source = self.input_replacement(basis.revision, line_id, original, value)?;
        let current = self.current.as_ref().ok_or("No document is open.")?;
        let ids = current.line_ids.clone();
        let next = current.next_line_id;
        self.commit_source_edit(source, ids, next, RelatedReferenceEdits::default()).map_err(|error| error.to_string())
    }

    pub(crate) fn input_projection(&self, line_id: LineId) -> Result<super::input::InputProjection, String> {
        let current = self.current.as_ref().ok_or("No document is open.")?;
        let selection = current.view.inputs.iter().find(|input| input.id == line_id).ok_or("Input is not selected.")?;
        if selection.unavailable { return Err("Input is unavailable.".into()); }
        let line = current.line_ids.iter().position(|id| *id == line_id).ok_or("Input no longer exists.")?;
        let descriptor = super::document::literal_inputs(&current.source).into_iter().find(|input| input.line == line && input.name == selection.name).ok_or("Input changed or is no longer a literal.")?;
        super::input::project(descriptor, selection.config.clone())
    }

    pub(crate) fn configure_input(&mut self, line_id: LineId, mut config: super::input::InputConfig) -> Result<(), String> {
        let current = self.current.as_ref().ok_or("No document is open.")?;
        let selection = current.view.inputs.iter().find(|input| input.id == line_id).ok_or("Input is not selected.")?;
        if selection.unavailable { return Err("Input is unavailable.".into()); }
        let line = current.line_ids.iter().position(|id| *id == line_id).ok_or("Input no longer exists.")?;
        let descriptor = super::document::literal_inputs(&current.source).into_iter().find(|input| input.line == line && input.name == selection.name).ok_or("Input changed or is no longer a literal.")?;
        config.anchor = config.has_numeric_constraints().then(|| descriptor.kind.anchor());
        super::input::validate_config(&descriptor.kind, &config)?;
        let mut view = current.view.clone();
        view.inputs.iter_mut().find(|input| input.id == line_id).ok_or("Input is not selected.")?.config = config;
        self.set_view_metadata(view).map_err(str::to_owned)
    }

    pub fn replace_source(&mut self, source: impl Into<String>) -> Result<bool, SessionError> {
        let basis = self.evaluation_basis().ok_or(SessionError::NoActiveDocument)?;
        self.commit_related_edit(basis, source.into(), RelatedReferenceEdits::default())
    }

    pub(crate) fn commit_related_edit(
        &mut self,
        basis: super::EvaluationBasis,
        source: String,
        related: RelatedReferenceEdits,
    ) -> Result<bool, SessionError> {
        self.ensure_editable()?;
        if self.evaluation_basis() != Some(basis) { return Err(SessionError::StaleRelatedEdit); }
        validate_source_size(&source)?;
        let current = self
            .current
            .as_ref()
            .ok_or(SessionError::NoActiveDocument)?;
        if current.source == source {
            return Ok(false);
        }
        let (line_ids, next_line_id) = remap_line_ids(
            &current.source,
            &current.line_ids,
            &source,
            current.next_line_id,
        )?;
        self.commit_source_edit(source, line_ids, next_line_id, related)
    }

    /// Commits a prepared top-level splice while preserving every retained row.
    pub(crate) fn apply_reference_example(
        &mut self,
        basis: super::EvaluationBasis,
        prepared: super::reference::insertion::PreparedExampleInsertion,
    ) -> Result<bool, SessionError> {
        self.ensure_editable()?;
        if self.evaluation_basis() != Some(basis) {
            return Err(SessionError::StaleRelatedEdit);
        }
        let current = self.current.as_ref().ok_or(SessionError::NoActiveDocument)?;
        if current.source == prepared.source {
            return Ok(false);
        }
        validate_source_size(&prepared.source)?;
        let (prefix, suffix) = prepared.retained_rows();
        let retained = prefix + suffix;
        let old_count = current.line_ids.len();
        let new_count = prepared.source.split('\n').count();
        if retained > old_count || retained > new_count {
            return Err(SessionError::LineIdStateMismatch);
        }
        let mut ids = Vec::with_capacity(new_count);
        ids.extend_from_slice(&current.line_ids[..prefix]);
        let mut next_id = current.next_line_id;
        for _ in retained..new_count {
            ids.push(LineId(next_id));
            next_id = next_id.checked_add(1).ok_or(SessionError::LineIdExhausted)?;
        }
        ids.extend_from_slice(&current.line_ids[old_count - suffix..]);
        self.commit_source_edit(prepared.source, ids, next_id, RelatedReferenceEdits::default())
    }

    /// Restores an unnamed recovery draft over the session's initial blank
    /// document. Recovery starts with fresh line identities, no undo history,
    /// and the blank source as its saved baseline so restored text stays dirty.
    #[cfg(test)]
    pub(crate) fn restore_recovered_source(
        &mut self,
        source: String,
    ) -> Result<bool, SessionError> {
        self.restore_recovered_document(source, DocumentOptions::default())
    }

    pub(crate) fn restore_recovered_document(
        &mut self,
        source: String,
        options: DocumentOptions,
    ) -> Result<bool, SessionError> {
        self.ensure_editable()?;
        validate_source_size(&source)?;
        let current = self
            .current
            .as_ref()
            .ok_or(SessionError::NoActiveDocument)?;
        if current.name.is_some()
            || current.is_dirty()
            || !current.source.is_empty()
            || current.revision != 0
        {
            return Ok(false);
        }
        if source.is_empty() {
            self.retire_background_evaluation();
            if let Some(current) = self.current.as_mut() { current.options = options; }
            self.history.clear();
            return Ok(true);
        }

        let (line_ids, next_line_id) = remap_line_ids(
            &current.source,
            &current.line_ids,
            &source,
            current.next_line_id,
        )?;
        let next_revision = current
            .revision
            .checked_add(1)
            .ok_or(SessionError::RevisionExhausted)?;
        self.retire_background_evaluation();
        let current = self
            .current
            .as_mut()
            .ok_or(SessionError::NoActiveDocument)?;
        current.source = source;
        current.line_ids = line_ids;
        current.next_line_id = next_line_id;
        current.revision = next_revision;
        current.options = options;
        current.cached_evaluation = None;
        self.history.clear();
        Ok(true)
    }

    fn commit_source_edit(
        &mut self,
        source: String,
        line_ids: Vec<LineId>,
        next_line_id: u64,
        related: RelatedReferenceEdits,
    ) -> Result<bool, SessionError> {
        let current = self
            .current
            .as_ref()
            .ok_or(SessionError::NoActiveDocument)?;
        if current.source == source {
            return Ok(false);
        }
        let before = current.editable_state();
        let watch_ids = related.watches.iter().map(|edit| current.view.watches.iter().position(|name| name == &edit.before).map(|index| current.watch_ids[index])).collect();
        let sensitivity_ids = related.sensitivity_outputs.iter().map(|edit| current.view.sensitivity.as_ref().and_then(|settings| settings.definition.outputs.iter().position(|name| name == &edit.before)).map(|index| current.sensitivity_output_ids[index])).collect();
        let keys = chart_output_keys(&current.view);
        let chart_ids = related.chart_outputs.iter().map(|edit| keys.iter().position(|key| match key {
            super::chart::options::SeriesKey::Sensitivity(name) => name == &edit.before,
            super::chart::options::SeriesKey::Vector(super::ChartVectorReference::Named { name }) => name == &edit.before,
            super::chart::options::SeriesKey::Vector(_) => false,
            super::chart::options::SeriesKey::Distribution(_) => false,
        }).map(|index| current.chart_output_ids[index])).collect();
        let view = related.apply(&current.view, false, true).map_err(SessionError::InvalidRelatedMetadata)?;
        let mut after = before.clone();
        after.source = source.clone();
        after.line_ids = line_ids.clone();
        let next_revision = current
            .revision
            .checked_add(1)
            .ok_or(SessionError::RevisionExhausted)?;

        self.retire_sensitivity();
        self.retire_background_evaluation();
        self.history.record(before, after, related, watch_ids, sensitivity_ids, chart_ids);
        let current = self
            .current
            .as_mut()
            .ok_or(SessionError::NoActiveDocument)?;
        current.source = source;
        current.line_ids = line_ids;
        current.next_line_id = next_line_id;
        current.revision = next_revision;
        current.view = view;
        self.prune_hidden_watch_ids();
        Ok(true)
    }

    pub fn replace_line(
        &mut self,
        line_id: LineId,
        replacement: &str,
    ) -> Result<bool, SessionError> {
        self.ensure_editable()?;
        let current = self
            .current
            .as_ref()
            .ok_or(SessionError::NoActiveDocument)?;
        let line_index = current
            .line_ids
            .iter()
            .position(|candidate| *candidate == line_id)
            .ok_or(SessionError::LineIdNotFound(line_id))?;
        let mut document = CalculationDocument::new(current.source.clone());
        document.replace_line(line_index, replacement)?;
        let source = document.source().to_owned();
        validate_source_size(&source)?;
        if current.source == source {
            return Ok(false);
        }
        let line_ids = current.line_ids.clone();
        let next_line_id = current.next_line_id;
        self.commit_source_edit(source, line_ids, next_line_id, RelatedReferenceEdits::default())
    }

    pub fn undo(&mut self) -> bool {
        if self.pending.is_some() || self.history.undo.is_empty() {
            return false;
        }
        let Some(current) = self.current.as_ref() else {
            return false;
        };
        let Some(next_revision) = current.revision.checked_add(1) else {
            return false;
        };
        let Some(mut entry) = self.history.undo.back().cloned() else { return false; };
        let Ok((view, watch_ids, hidden_watch_ids, sensitivity_ids, sensitivity_hidden, chart_ids, chart_hidden, hidden_styles)) = entry.replay(&current.view, &current.watch_ids, &current.hidden_watch_ids, &current.sensitivity_output_ids, &current.hidden_sensitivity_output_ids, &current.chart_output_ids, &current.hidden_chart_output_ids, &current.hidden_chart_styles, true) else {
            return false;
        };
        if self.history.undo.pop_back().is_none() {
            return false;
        }
        self.retire_sensitivity();
        self.retire_background_evaluation();
        if let Some(current) = self.current.as_mut() {
            apply_editable_state(current, &entry.before);
            current.revision = next_revision;
            current.view = view;
            current.watch_ids = watch_ids;
            current.hidden_watch_ids = hidden_watch_ids;
            current.sensitivity_output_ids = sensitivity_ids;
            current.hidden_sensitivity_output_ids = sensitivity_hidden;
            current.chart_output_ids = chart_ids;
            current.hidden_chart_output_ids = chart_hidden;
            current.hidden_chart_styles = hidden_styles;
        }
        self.history.redo.push_back(entry);
        self.prune_hidden_watch_ids();
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.pending.is_some() || self.history.redo.is_empty() {
            return false;
        }
        let Some(current) = self.current.as_ref() else {
            return false;
        };
        let Some(next_revision) = current.revision.checked_add(1) else {
            return false;
        };
        let Some(mut entry) = self.history.redo.back().cloned() else { return false; };
        let Ok((view, watch_ids, hidden_watch_ids, sensitivity_ids, sensitivity_hidden, chart_ids, chart_hidden, hidden_styles)) = entry.replay(&current.view, &current.watch_ids, &current.hidden_watch_ids, &current.sensitivity_output_ids, &current.hidden_sensitivity_output_ids, &current.chart_output_ids, &current.hidden_chart_output_ids, &current.hidden_chart_styles, false) else {
            return false;
        };
        if self.history.redo.pop_back().is_none() {
            return false;
        }
        self.retire_sensitivity();
        self.retire_background_evaluation();
        if let Some(current) = self.current.as_mut() {
            apply_editable_state(current, &entry.after);
            current.revision = next_revision;
            current.view = view;
            current.watch_ids = watch_ids;
            current.hidden_watch_ids = hidden_watch_ids;
            current.sensitivity_output_ids = sensitivity_ids;
            current.hidden_sensitivity_output_ids = sensitivity_hidden;
            current.chart_output_ids = chart_ids;
            current.hidden_chart_output_ids = chart_hidden;
            current.hidden_chart_styles = hidden_styles;
        }
        self.history.undo.push_back(entry);
        self.prune_hidden_watch_ids();
        true
    }

    fn poll_evaluation(&mut self) {
        let Some(stamp) = self.evaluation_stamp() else { return; };
        if let Some(completion) = self
            .evaluation_worker
            .as_ref()
            .and_then(EvaluationWorker::take)
            && completion.stamp == stamp && self.scheduled_evaluation == Some(stamp)
        {
            self.scheduled_evaluation = None;
            match completion.result {
                Ok(result) if result.basis == stamp => {
                    if let Some(current) = self.current.as_mut() {
                        current.cached_evaluation = Some(Arc::new(result));
                        current.dependency_snapshot = None;
                        #[cfg(test)]
                        {
                            current.evaluation_count += 1;
                        }
                    }
                }
                Ok(_) => self.evaluation_failure = Some((stamp, "Calculation results have an invalid evaluation identity.".into())),
                Err(message) => self.evaluation_failure = Some((stamp, message)),
            }
        }
    }

    fn start_evaluation(&mut self) {
        let Some(stamp) = self.evaluation_stamp() else { return; };
        let Some(current) = self.current.as_ref() else { return; };
        #[cfg(test)]
        { self.evaluation_attempts += 1; }
        let limits = super::limits::profile_limits(current.options.profile);
        let within_profile = current.source.len() <= limits.bytes && physical_rows(&current.source) <= limits.lines;
        if self.evaluation_wake.is_some() && (current.options.profile == super::persistence::DocumentProfile::Extended || (within_profile && CalculationDocument::source_requires_repeated_execution(&current.source))) {
            if self.evaluation_worker.is_none() {
                let Some(wake) = self.evaluation_wake.as_ref().cloned() else { return; };
                match EvaluationWorker::start(wake) {
                    Ok(worker) => self.evaluation_worker = Some(worker),
                    Err(error) => { self.evaluation_failure = Some((stamp, error)); return; }
                }
            }
            let Some(worker) = self.evaluation_worker.as_ref() else { return; };
            worker.submit(EvaluationRequest { stamp, source: current.source.clone(), line_ids: current.line_ids.clone(), profile: current.options.profile });
            self.scheduled_evaluation = Some(stamp);
            return;
        }
        let document = CalculationDocument::with_profile(current.source.clone(), current.options.profile);
        let evaluation = document.evaluate();
        match project_evaluation(evaluation, &current.line_ids, stamp) {
            Ok(result) => {
                if let Some(current) = self.current.as_mut() {
                    current.cached_evaluation = Some(Arc::new(result));
                    current.dependency_snapshot = None;
                    #[cfg(test)]
                    { current.evaluation_count += 1; }
                }
            }
            Err(message) => self.evaluation_failure = Some((stamp, message)),
        }
    }

    /// Poll a finished worker and advance automatic calculation once when needed.
    pub(crate) fn advance_automatic(&mut self) {
        self.poll_evaluation();
        let Some(stamp) = self.evaluation_stamp() else { return; };
        if self.pending.is_some()
            || self.document_options().is_none_or(|options| options.calculation != super::CalculationPolicy::Automatic)
            || self.scheduled_evaluation == Some(stamp)
            || self.canceled_evaluation == Some(stamp)
            || self.evaluation_failure.as_ref().is_some_and(|(failed, _)| *failed == stamp)
            || self.current.as_ref().and_then(|current| current.cached_evaluation.as_ref()).is_some_and(|result| result.basis == stamp)
        { return; }
        self.start_evaluation();
    }

    /// Explicitly calculate the current document, coalescing an active request.
    pub fn request_calculate(&mut self) {
        let Some(stamp) = self.evaluation_stamp() else { return; };
        if self.pending.is_some() || self.scheduled_evaluation == Some(stamp) { return; }
        self.poll_evaluation();
        self.retire_background_evaluation();
        self.start_evaluation();
    }

    pub fn cancel_calculation(&mut self, basis: super::EvaluationBasis) -> bool {
        if self.scheduled_evaluation != Some(basis) || self.evaluation_stamp() != Some(basis) { return false; }
        self.retire_background_evaluation();
        self.canceled_evaluation = self.evaluation_stamp();
        true
    }

    /// Drive automatic evaluation explicitly and return current results only.
    pub fn evaluation(&mut self) -> Option<&SessionEvaluation> {
        self.advance_automatic();
        let stamp = self.evaluation_stamp()?;
        self.current.as_ref()?.cached_evaluation.as_deref().filter(|result| result.basis == stamp)
    }

    /// Returns the source and its evaluation from the same current revision.
    pub(crate) fn evaluation_with_source(&mut self) -> Option<(&str, &SessionEvaluation)> {
        let stamp = self.evaluation_stamp()?;
        let current = self.current.as_ref()?;
        let evaluation = current.cached_evaluation.as_deref().filter(|result| result.basis == stamp)?;
        Some((&evaluation.source, evaluation))
    }

    pub(crate) fn shared_evaluation(&mut self) -> Option<Arc<SessionEvaluation>> {
        let stamp = self.evaluation_stamp()?;
        self.current.as_ref()?.cached_evaluation.as_ref().filter(|result| result.basis == stamp).cloned()
    }

    pub(crate) fn displayed_evaluation(&self) -> Option<Arc<SessionEvaluation>> {
        self.current.as_ref()?.cached_evaluation.clone()
    }

    pub fn cached_evaluation_revision(&self) -> Option<u64> {
        self.current
            .as_ref()
            .and_then(|document| document.cached_evaluation.as_ref())
            .map(|evaluation| evaluation.basis.revision)
    }

    pub fn save(&mut self) -> Result<(), SessionError> {
        self.save_with_writer(crate::common::atomic_file::save_atomic)
    }

    fn save_with_writer(
        &mut self,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<(), SessionError> {
        let current = self
            .current
            .as_ref()
            .ok_or(SessionError::NoActiveDocument)?;
        if current.name.is_none() {
            return Err(SessionError::SaveAsRequired);
        }
        if !current.is_dirty() && current.baseline.format_version == super::persistence::CURRENT_DOCUMENT_FORMAT_VERSION {
            return Ok(());
        }
        let stored = current.stored_document()?;
        let expected_disk_sha256 = current
            .baseline
            .disk_sha256
            .ok_or(SessionError::SaveAsRequired)?;
        let disk_sha256 =
            self.store
                .save_existing_with_writer(&stored, expected_disk_sha256, writer)?;
        let current = self
            .current
            .as_mut()
            .ok_or(SessionError::NoActiveDocument)?;
        current.baseline = DocumentBaseline {
            state: current.editable_state(),
            id: current.id,
            name: current.name.clone(),
            disk_sha256: Some(disk_sha256),
            next_line_id: current.next_line_id,
            view: current.view.clone(),
            options: current.options,
            format_version: super::persistence::CURRENT_DOCUMENT_FORMAT_VERSION,
        };
        Ok(())
    }

    pub fn save_as(&mut self, name: &str) -> Result<(), SessionError> {
        let current = self
            .current
            .as_ref()
            .ok_or(SessionError::NoActiveDocument)?;
        let (stored, disk_sha256) = self.store.save_as_with_metadata(
            name,
            current.source.clone(),
            current.line_ids.clone(),
            current.next_line_id,
            current.view.clone(),
            current.options,
        )?;
        self.retire_sensitivity();
        self.retire_background_evaluation();
        let current = self
            .current
            .as_mut()
            .ok_or(SessionError::NoActiveDocument)?;
        current.id = stored.id;
        current.name = Some(stored.name);
        current.baseline = DocumentBaseline {
            state: current.editable_state(),
            id: current.id,
            name: current.name.clone(),
            disk_sha256: Some(disk_sha256),
            next_line_id: current.next_line_id,
            view: current.view.clone(),
            options: current.options,
            format_version: super::persistence::CURRENT_DOCUMENT_FORMAT_VERSION,
        };
        Ok(())
    }

    pub fn request_new(&mut self) -> Result<TransitionOutcome, SessionError> {
        self.ensure_no_pending()?;
        if self.is_dirty() {
            self.pending = Some(PendingAction::New);
            return Ok(TransitionOutcome::DecisionRequired);
        }
        self.install_new();
        Ok(TransitionOutcome::Applied)
    }

    /// The target is read and validated before the active draft is touched.
    pub fn request_open(&mut self, id: DocumentId) -> Result<TransitionOutcome, SessionError> {
        self.ensure_no_pending()?;
        let loaded = self.store.open(id)?;
        if self.is_dirty() {
            self.pending = Some(PendingAction::Open(id));
            return Ok(TransitionOutcome::DecisionRequired);
        }
        self.retire_sensitivity();
        self.retire_background_evaluation();
        self.current = Some(ActiveDocument::from_loaded(loaded));
        self.history.clear();
        Ok(TransitionOutcome::Applied)
    }

    pub fn request_close(&mut self) -> Result<TransitionOutcome, SessionError> {
        self.ensure_no_pending()?;
        if self.current.is_none() {
            return Ok(TransitionOutcome::Applied);
        }
        if self.is_dirty() {
            self.pending = Some(PendingAction::Close);
            return Ok(TransitionOutcome::DecisionRequired);
        }
        self.retire_sensitivity();
        self.retire_background_evaluation();
        self.current = None;
        self.history.clear();
        Ok(TransitionOutcome::Applied)
    }

    /// Retire an unbound scratch only after its recovery snapshot was saved.
    pub(crate) fn close_recovered_scratch(&mut self) -> Result<(), SessionError> {
        self.ensure_no_pending()?;
        if self.current.is_none() {
            return Err(SessionError::NoActiveDocument);
        }
        if self.is_saved() {
            return Err(SessionError::RecoveryScratchRequired);
        }
        self.retire_sensitivity();
        self.retire_background_evaluation();
        self.current = None;
        self.history.clear();
        Ok(())
    }

    pub fn resolve_pending(
        &mut self,
        decision: TransitionDecision,
    ) -> Result<TransitionOutcome, SessionError> {
        if self.pending.is_none() {
            return Err(SessionError::NoPendingTransition);
        }
        match decision {
            TransitionDecision::Cancel => {
                self.pending = None;
                Ok(TransitionOutcome::Cancelled)
            }
            TransitionDecision::Save => {
                if self.is_dirty() {
                    self.save()?;
                }
                self.apply_pending()?;
                Ok(TransitionOutcome::Applied)
            }
            TransitionDecision::Discard => {
                self.apply_pending()?;
                Ok(TransitionOutcome::Applied)
            }
        }
    }

    fn apply_pending(&mut self) -> Result<(), SessionError> {
        let pending = self.pending.ok_or(SessionError::NoPendingTransition)?;
        let current = match pending {
            PendingAction::New => Some(ActiveDocument::new_blank()),
            PendingAction::Open(id) => Some(ActiveDocument::from_loaded(self.store.open(id)?)),
            PendingAction::Close => None,
        };
        self.retire_sensitivity();
        self.retire_background_evaluation();
        self.current = current;
        self.pending = None;
        self.history.clear();
        Ok(())
    }

    fn install_new(&mut self) {
        self.retire_sensitivity();
        self.retire_background_evaluation();
        self.current = Some(ActiveDocument::new_blank());
        self.history.clear();
    }

    fn ensure_editable(&self) -> Result<(), SessionError> {
        self.ensure_no_pending()?;
        if self.current.is_none() {
            return Err(SessionError::NoActiveDocument);
        }
        Ok(())
    }

    fn ensure_no_pending(&self) -> Result<(), SessionError> {
        if self.pending.is_some() {
            Err(SessionError::PendingTransitionExists)
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    pub(crate) fn evaluation_count(&self) -> usize {
        self.current
            .as_ref()
            .map_or(0, |document| document.evaluation_count)
    }
}

fn validate_source_size(source: &str) -> Result<(), SessionError> {
    if source.len() > MAX_DOCUMENT_BYTES {
        return Err(SessionError::DocumentTooLarge {
            max_bytes: MAX_DOCUMENT_BYTES,
        });
    }
    let line_count = physical_rows(source);
    if line_count > MAX_SOURCE_LINES {
        return Err(SessionError::TooManyLines {
            max_lines: MAX_SOURCE_LINES,
        });
    }
    Ok(())
}

fn apply_editable_state(document: &mut ActiveDocument, state: &EditableState) {
    document.source.clone_from(&state.source);
    document.line_ids.clone_from(&state.line_ids);
}

fn remap_line_ids(
    old_source: &str,
    old_ids: &[LineId],
    new_source: &str,
    next_line_id: u64,
) -> Result<(Vec<LineId>, u64), SessionError> {
    // Keep the diff's memory bounded even if this helper gains another caller
    // besides `replace_source` in the future.
    validate_source_size(new_source)?;
    let old_lines = physical_lines(old_source);
    let new_lines = physical_lines(new_source);
    if old_ids.len() != old_lines.len() {
        return Err(SessionError::LineIdStateMismatch);
    }

    let old_count = old_lines.len();
    let new_count = new_lines.len();
    if old_count == new_count {
        // A whole-editor snapshot with at most one changed physical row is a
        // single-line edit. Keep IDs by position so duplicate text cannot
        // make the LCS matcher assign a neighboring row's identity instead.
        let mut changed_lines = 0;
        for (old_line, new_line) in old_lines.iter().zip(&new_lines) {
            if old_line != new_line {
                changed_lines += 1;
                if changed_lines > 1 {
                    break;
                }
            }
        }
        if changed_lines <= 1 {
            return Ok((old_ids.to_vec(), next_line_id));
        }
    }
    let width = new_count + 1;
    let mut lcs = vec![0u16; (old_count + 1) * width];
    for old_index in (0..old_count).rev() {
        for new_index in (0..new_count).rev() {
            let value = if old_lines[old_index] == new_lines[new_index] {
                1 + lcs[(old_index + 1) * width + new_index + 1]
            } else {
                lcs[(old_index + 1) * width + new_index].max(lcs[old_index * width + new_index + 1])
            };
            lcs[old_index * width + new_index] = value;
        }
    }

    let mut anchors = Vec::new();
    let (mut old_index, mut new_index) = (0, 0);
    while old_index < old_count && new_index < new_count {
        if old_lines[old_index] == new_lines[new_index] {
            anchors.push((old_index, new_index));
            old_index += 1;
            new_index += 1;
        } else if lcs[(old_index + 1) * width + new_index] >= lcs[old_index * width + new_index + 1]
        {
            old_index += 1;
        } else {
            new_index += 1;
        }
    }

    let mut mapped = vec![None; new_count];
    let (mut old_start, mut new_start) = (0, 0);
    for (matched_old, matched_new) in anchors {
        map_unmatched_segment(
            old_start..matched_old,
            new_start..matched_new,
            old_ids,
            &mut mapped,
        );
        mapped[matched_new] = Some(old_ids[matched_old]);
        old_start = matched_old + 1;
        new_start = matched_new + 1;
    }
    map_unmatched_segment(
        old_start..old_count,
        new_start..new_count,
        old_ids,
        &mut mapped,
    );

    let mut next_id = next_line_id;
    let line_ids = mapped
        .into_iter()
        .map(|id| match id {
            Some(id) => Ok(id),
            None => {
                let fresh = LineId(next_id);
                next_id = next_id
                    .checked_add(1)
                    .ok_or(SessionError::LineIdExhausted)?;
                Ok(fresh)
            }
        })
        .collect::<Result<Vec<_>, SessionError>>()?;
    Ok((line_ids, next_id))
}

fn map_unmatched_segment(
    old_range: std::ops::Range<usize>,
    new_range: std::ops::Range<usize>,
    old_ids: &[LineId],
    mapped: &mut [Option<LineId>],
) {
    for offset in 0..old_range.len().min(new_range.len()) {
        mapped[new_range.start + offset] = Some(old_ids[old_range.start + offset]);
    }
}

fn physical_lines(source: &str) -> Vec<&str> {
    source
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        CalculationPadSession, MAX_UNDO_ENTRIES, PendingTransition, SessionError,
        TransitionDecision, TransitionOutcome,
    };
    use crate::calculation_pad::{
        CalculationValue, DiagnosticKind, DocumentId, LineId, LineOutcome, MAX_DOCUMENT_BYTES,
        PadStoreError,
    };
    use crate::platform::app_data::AppDataRoot;
    use std::path::PathBuf;

    #[test]
    fn input_literal_detection_rejects_arithmetic_and_accepts_prefixed_literals() {
        let source = "a = u64(1+2)\nb = u64( 0x10 )\nc = 0x10\nd = 0x10 m\ne = 12unknownunit\n  f = 2";
        let inputs = super::super::document::literal_inputs(source);
        assert_eq!(inputs.iter().map(|input| input.line).collect::<Vec<_>>(), vec![1, 2, 3, 5]);
        assert_eq!(inputs[0].value, "0x10");
        assert!(super::super::document::watch_name("{gross total}").is_some());
        assert!(super::super::document::watch_name("{bad").is_none());
        assert!(super::super::document::watch_name("{{bad}}").is_none());
        let looped = "for i in range(2):\n  inner = 1\n  stray = 2\nafter = 3";
        assert_eq!(super::super::document::literal_inputs(looped).iter().map(|input| input.line).collect::<Vec<_>>(), vec![3]);
    }

    #[test]
    fn decimal_literal_input_preserves_constructor_and_exact_source() {
        let (_directory, _root, mut session) = fixture();
        let source = "price = decimal(\"0.10\") # retained\r\nprice * 2";
        session.replace_source(source).unwrap();
        let input = super::super::document::literal_inputs(source).into_iter().next().unwrap();
        assert_eq!(input.value, "0.10");
        assert_eq!(&source[input.value_range.clone()], "0.10");
        let line = session.line_id(0).unwrap();
        let revision = session.revision().unwrap();
        assert!(session.input_replacement(revision, line, &input.source_line, "1/3").is_err());
        let replacement = session.input_replacement(revision, line, &input.source_line, "0.25").unwrap();
        assert_eq!(replacement, "price = decimal(\"0.25\") # retained\r\nprice * 2");
        session.replace_source(replacement).unwrap();
        assert!(session.undo());
        assert_eq!(session.source(), Some(source));
    }

    #[test]
    fn multiline_literal_input_apply_preserves_comments_crlf_and_undo() {
        let (_dir, _root, mut session) = fixture();
        let source = "{α#} = u64( // explanation\r\n  42) # retained\r\nfor i in range(2):\r\n  hidden = 3\r\nresult = {α#}";
        session.replace_source(source).unwrap();
        let input = super::super::document::literal_inputs(source).into_iter().next().unwrap();
        assert_eq!(input.line, 0);
        assert_eq!(&source[input.value_range.clone()], "42");
        let line = session.line_ids().unwrap()[0];
        let replacement = session.input_replacement(session.revision().unwrap(), line, &input.source_line, "99").unwrap();
        assert_eq!(replacement, source.replace("42)", "99)"));
        session.replace_source(replacement).unwrap();
        assert!(session.undo());
        assert_eq!(session.source(), Some(source));
    }

    #[test]
    fn options_round_trip_dirty_state_and_retire_same_revision_results() {
        let (_dir, root, mut session) = fixture();
        session.replace_source("u8(3)").unwrap();
        let line = session.line_id(0).unwrap();
        session.evaluation().unwrap();
        let original = session.inspector_source(line).unwrap();
        let basis = session.evaluation_basis().unwrap();
        let options = super::DocumentOptions { calculation: super::super::CalculationPolicy::Manual, profile: super::super::DocumentProfile::Extended };
        session.set_document_options(options).unwrap();
        assert_eq!(session.revision(), Some(basis.revision));
        assert_ne!(session.evaluation_basis(), Some(basis));
        assert!(session.is_dirty());
        assert!(session.inspector_source(line).is_none());
        session.request_calculate();
        session.evaluation().unwrap();
        assert_ne!(session.inspector_source(line).unwrap().basis, original.basis);
        session.save_as("options").unwrap();
        assert!(session.inspector_source(line).is_none());
        let id = session.current_id().unwrap();
        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(id).unwrap();
        assert_eq!(reopened.document_options(), Some(options));
    }

    #[test]
    fn sensitivity_definition_persists_and_rename_undo_preserves_later_settings() {
        let (_dir, root, mut session) = fixture();
        session.replace_source("x = 2\ny = x + 1").unwrap();
        let settings = super::super::SensitivitySettings { input_name: "x".into(), definition: super::super::SensitivityDefinition { input: session.line_id(0).unwrap(), start: "0".into(), end: "10".into(), sampling: super::super::Sampling::Count(3), outputs: vec!["x".into(), "y".into()] } };
        let mut view = session.view_metadata().unwrap().clone();
        view.sensitivity = Some(settings.clone());
        session.set_view_metadata(view).unwrap();
        session.save_as("sensitivity").unwrap();
        let id = session.current_id().unwrap();
        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(id).unwrap();
        assert_eq!(reopened.view_metadata().unwrap().sensitivity, Some(settings));
        let preview = reopened.preview_rename(0, "z").unwrap();
        assert!(reopened.apply_rename(&preview).unwrap());
        let mut later = reopened.view_metadata().unwrap().clone();
        let sensitivity = later.sensitivity.as_mut().unwrap();
        assert_eq!(sensitivity.input_name, "z");
        assert_eq!(sensitivity.definition.outputs, vec!["z", "y"]);
        sensitivity.definition.end = "20".into();
        reopened.set_view_metadata(later).unwrap();
        assert!(reopened.undo());
        let sensitivity = reopened.view_metadata().unwrap().sensitivity.as_ref().unwrap();
        assert_eq!(sensitivity.input_name, "x");
        assert_eq!(sensitivity.definition.outputs, vec!["x", "y"]);
        assert_eq!(sensitivity.definition.end, "20");
    }

    #[test]
    fn sensitivity_rename_does_not_rewrite_removed_and_readded_output() {
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 2\nx + 1").unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        view.sensitivity = Some(super::super::SensitivitySettings { input_name: "x".into(), definition: super::super::SensitivityDefinition { input: session.line_id(0).unwrap(), start: "0".into(), end: "1".into(), sampling: super::super::Sampling::Count(2), outputs: vec!["x".into()] } });
        session.set_view_metadata(view).unwrap();
        let preview = session.preview_rename(0, "y").unwrap();
        session.apply_rename(&preview).unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        view.sensitivity.as_mut().unwrap().definition.outputs.clear();
        session.set_view_metadata(view.clone()).unwrap();
        view.sensitivity.as_mut().unwrap().definition.outputs.push("x".into());
        session.set_view_metadata(view).unwrap();
        assert!(session.undo());
        assert!(session.redo());
        assert_eq!(session.view_metadata().unwrap().sensitivity.as_ref().unwrap().definition.outputs, vec!["x"]);
    }

    #[test]
    fn chained_sensitivity_renames_preserve_independent_reselection() {
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 2\nx + 1").unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        view.sensitivity = Some(super::super::SensitivitySettings { input_name: "x".into(), definition: super::super::SensitivityDefinition { input: session.line_id(0).unwrap(), start: "0".into(), end: "1".into(), sampling: super::super::Sampling::Count(2), outputs: vec!["x".into()] } });
        session.set_view_metadata(view).unwrap();
        let first = session.preview_rename(0, "y").unwrap(); session.apply_rename(&first).unwrap();
        let second = session.preview_rename(0, "z").unwrap(); session.apply_rename(&second).unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        view.sensitivity.as_mut().unwrap().definition.outputs.clear();
        session.set_view_metadata(view.clone()).unwrap();
        view.sensitivity.as_mut().unwrap().definition.outputs.push("y".into());
        session.set_view_metadata(view).unwrap();
        assert!(session.undo()); assert!(session.undo());
        assert_eq!(session.view_metadata().unwrap().sensitivity.as_ref().unwrap().definition.outputs, vec!["y"]);
        assert!(session.redo()); assert!(session.redo());
        assert_eq!(session.view_metadata().unwrap().sensitivity.as_ref().unwrap().definition.outputs, vec!["y"]);
    }

    #[test]
    fn sensitivity_rename_collision_restores_hidden_selection() {
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 2\nx + 1").unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        view.sensitivity = Some(super::super::SensitivitySettings { input_name: "x".into(), definition: super::super::SensitivityDefinition { input: session.line_id(0).unwrap(), start: "0".into(), end: "1".into(), sampling: super::super::Sampling::Count(2), outputs: vec!["x".into()] } });
        session.set_view_metadata(view).unwrap();
        let preview = session.preview_rename(0, "y").unwrap(); session.apply_rename(&preview).unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        view.sensitivity.as_mut().unwrap().definition.outputs.push("x".into());
        session.set_view_metadata(view).unwrap();
        assert!(session.undo());
        assert_eq!(session.view_metadata().unwrap().sensitivity.as_ref().unwrap().definition.outputs, vec!["x"]);
        assert!(session.redo());
        assert_eq!(session.view_metadata().unwrap().sensitivity.as_ref().unwrap().definition.outputs, vec!["x", "y"]);
    }

    #[test]
    fn chained_sensitivity_input_retarget_survives_undo() {
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 2\nother = 3\nx + 1").unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        view.sensitivity = Some(super::super::SensitivitySettings { input_name: "x".into(), definition: super::super::SensitivityDefinition { input: session.line_id(0).unwrap(), start: "0".into(), end: "1".into(), sampling: super::super::Sampling::Count(2), outputs: vec!["x".into()] } });
        session.set_view_metadata(view).unwrap();
        let first = session.preview_rename(0, "y").unwrap(); session.apply_rename(&first).unwrap();
        let second = session.preview_rename(0, "z").unwrap(); session.apply_rename(&second).unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        let settings = view.sensitivity.as_mut().unwrap();
        settings.definition.input = session.line_id(1).unwrap(); settings.input_name = "other".into();
        session.set_view_metadata(view).unwrap();
        assert!(session.undo()); assert!(session.undo());
        let settings = session.view_metadata().unwrap().sensitivity.as_ref().unwrap();
        assert_eq!(settings.input_name, "other");
        assert_eq!(settings.definition.input, session.line_id(1).unwrap());
    }

    #[test]
    fn chart_definition_roundtrip_and_rename_history_keeps_independent_selection() {
        let (_dir, root, mut session) = fixture();
        session.replace_source("x = 2\nx + 1").unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        view.chart = Some(super::super::ChartDefinition { source: super::super::ChartSource::Sensitivity { outputs: vec!["x".into()] }, kind: super::super::ChartKind::Bar, x_label: "Input".into(), y_label: "Output".into(), hidden: Vec::new(), bar_labels: vec!["first".into()], distribution: None, options: Default::default() });
        session.set_view_metadata(view).unwrap();
        session.save_as("chart").unwrap();
        let id = session.current_id().unwrap();
        let mut reopened = CalculationPadSession::new(&root); reopened.request_open(id).unwrap();
        assert!(matches!(&reopened.view_metadata().unwrap().chart.as_ref().unwrap().source, super::super::ChartSource::Sensitivity { outputs } if outputs == &["x"]));
        let first = reopened.preview_rename(0, "y").unwrap(); reopened.apply_rename(&first).unwrap();
        let second = reopened.preview_rename(0, "z").unwrap(); reopened.apply_rename(&second).unwrap();
        let mut view = reopened.view_metadata().unwrap().clone();
        let Some(super::super::ChartDefinition { source: super::super::ChartSource::Sensitivity { outputs }, .. }) = view.chart.as_mut() else { panic!("sensitivity chart"); };
        outputs.clear(); reopened.set_view_metadata(view.clone()).unwrap();
        let Some(super::super::ChartDefinition { source: super::super::ChartSource::Sensitivity { outputs }, .. }) = view.chart.as_mut() else { panic!("sensitivity chart"); };
        outputs.push("y".into()); reopened.set_view_metadata(view).unwrap();
        assert!(reopened.undo()); assert!(reopened.undo());
        assert!(matches!(&reopened.view_metadata().unwrap().chart.as_ref().unwrap().source, super::super::ChartSource::Sensitivity { outputs } if outputs == &["y"]));
    }

    #[test]
    fn named_chart_references_follow_rename_history_and_independent_edits() {
        use super::super::{ChartDefinition, ChartKind, ChartSource, ChartVectorReference as Ref};
        let (_dir, root, mut session) = fixture();
        session.replace_source("x = [1]\ny = [2]").unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        let named = |name: &str| Ref::Named { name: name.into() };
        let mut view = session.view_metadata().unwrap().clone();
        view.chart = Some(ChartDefinition { source: ChartSource::Vectors { x: Some(named("x")), y: vec![Ref::Physical(ids[0]), named("x"), named("y")] }, kind: ChartKind::Bar, x_label: "custom".into(), y_label: String::new(), hidden: vec![1], bar_labels: Vec::new(), distribution: None, options: Default::default() });
        session.set_view_metadata(view).unwrap();
        let preview = session.preview_rename(0, "z").unwrap();
        session.apply_rename(&preview).unwrap();
        let chart = session.view_metadata().unwrap().chart.as_ref().unwrap();
        assert!(matches!(&chart.source, ChartSource::Vectors { x: Some(Ref::Named { name }), y } if name == "z" && y[1] == named("z")));
        let mut view = session.view_metadata().unwrap().clone();
        let chart = view.chart.as_mut().unwrap();
        chart.kind = ChartKind::Scatter;
        chart.source = ChartSource::Vectors { x: Some(Ref::Physical(ids[1])), y: vec![named("y"), named("z"), Ref::Physical(ids[0])] };
        chart.hidden = vec![1];
        session.set_view_metadata(view).unwrap();
        assert!(session.undo());
        let chart = session.view_metadata().unwrap().chart.as_ref().unwrap();
        assert_eq!(chart.kind, ChartKind::Scatter);
        assert_eq!(chart.hidden, [1]);
        assert!(matches!(&chart.source, ChartSource::Vectors { x: Some(Ref::Physical(id)), y } if *id == ids[1] && y == &[named("y"), named("x"), Ref::Physical(ids[0])]));
        assert!(session.redo());
        let mut view = session.view_metadata().unwrap().clone();
        let chart = view.chart.as_mut().unwrap();
        let ChartSource::Vectors { y, .. } = &mut chart.source else { panic!("vector chart"); };
        y.push(named("x"));
        session.set_view_metadata(view).unwrap();
        assert!(session.undo());
        let chart = session.view_metadata().unwrap().chart.as_ref().unwrap();
        assert!(matches!(&chart.source, ChartSource::Vectors { y, .. } if y == &[named("y"), Ref::Physical(ids[0]), named("x")]));
        assert!(chart.hidden.is_empty());
        assert!(session.redo());
        let chart = session.view_metadata().unwrap().chart.as_ref().unwrap();
        assert!(matches!(&chart.source, ChartSource::Vectors { x: Some(Ref::Physical(id)), y } if *id == ids[1] && y == &[named("y"), Ref::Physical(ids[0]), named("x"), named("z")]));
        assert_eq!(chart.hidden, [3]);
        session.save_as("named chart").unwrap();
        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(session.current_id().unwrap()).unwrap();
        assert_eq!(reopened.view_metadata().unwrap().chart, session.view_metadata().unwrap().chart);
    }

    #[test]
    fn chart_styles_and_reference_ids_follow_owned_rename_replay() {
        use super::super::{ChartDefinition, ChartKind, ChartSource, ChartVectorReference as Ref};
        use super::super::chart::options::{FiniteValue, LineStyle, MarkerStyle, ReferenceAxis, ReferenceLine, ReferenceValue, SeriesKey, SeriesStyle};
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 2\ny = 3").unwrap();
        let named = |name: &str| Ref::Named { name: name.into() };
        let mut view = session.view_metadata().unwrap().clone();
        let mut chart = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![named("x"), named("y")] }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: None, options: Default::default() };
        chart.options.styles.push(SeriesStyle { key: SeriesKey::Vector(named("x")), color: [1, 2, 3, 255], line: LineStyle::Dashed, marker: MarkerStyle::Square, thickness: FiniteValue::try_from(2.0).unwrap() });
        chart.options.references.push(ReferenceLine { id: 1, axis: ReferenceAxis::Y, value: ReferenceValue::Named("x".into()), label: "original".into() });
        chart.options.next_reference_id = 2;
        view.chart = Some(chart);
        session.set_view_metadata(view).unwrap();
        let preview = session.preview_rename(0, "z").unwrap();
        session.apply_rename(&preview).unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        let chart = view.chart.as_mut().unwrap();
        assert_eq!(chart.options.styles[0].key, SeriesKey::Vector(named("z")));
        assert_eq!(chart.options.references[0].value, ReferenceValue::Named("z".into()));
        if let ChartSource::Vectors { y, .. } = &mut chart.source { y.reverse(); }
        chart.options.styles[0].color = [9, 8, 7, 255];
        chart.options.references[0].label = "independent".into();
        session.set_view_metadata(view).unwrap();
        assert!(session.undo());
        let chart = session.view_metadata().unwrap().chart.as_ref().unwrap();
        assert_eq!(chart.options.styles[0].key, SeriesKey::Vector(named("x")));
        assert_eq!(chart.options.styles[0].color, [9, 8, 7, 255]);
        assert_eq!(chart.options.references[0].value, ReferenceValue::Named("x".into()));
        assert_eq!(chart.options.references[0].label, "independent");
        assert!(session.redo());
        let mut view = session.view_metadata().unwrap().clone();
        let chart = view.chart.as_mut().unwrap();
        chart.options.references.clear();
        session.set_view_metadata(view).unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        let chart = view.chart.as_mut().unwrap();
        chart.options.references.push(ReferenceLine { id: 1, axis: ReferenceAxis::Y, value: ReferenceValue::Named("z".into()), label: "new".into() });
        assert!(session.set_view_metadata(view.clone()).is_err());
        let chart = view.chart.as_mut().unwrap();
        let id = chart.options.allocate_reference_id().unwrap();
        chart.options.references[0].id = id;
        session.set_view_metadata(view).unwrap();
        assert!(session.undo());
        let chart = session.view_metadata().unwrap().chart.as_ref().unwrap();
        assert_eq!(chart.options.references[0].value, ReferenceValue::Named("z".into()));
        assert!(chart.options.next_reference_id >= 3);
    }

    #[test]
    fn chart_distribution_source_follows_rename_and_preserves_independent_edits() {
        use super::super::chart::distribution::{DistributionBins, DistributionDefinition, DistributionScale};
        use super::super::{ChartDefinition, ChartKind, ChartSource, ChartVectorReference as Ref};
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = [1, 2]\ny = [3]").unwrap();
        let named = |name: &str| Ref::Named { name: name.into() };
        let mut view = session.view_metadata().unwrap().clone();
        view.chart = Some(ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![named("y")] }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: Some(DistributionDefinition { vector: named("x"), bins: DistributionBins::Count { count: 2 }, scale: DistributionScale::Percentage }), options: Default::default() });
        session.set_view_metadata(view).unwrap();
        let preview = session.preview_rename(0, "z").unwrap(); session.apply_rename(&preview).unwrap();
        assert_eq!(super::chart_distribution(session.view_metadata().unwrap()), Some(&named("z")));
        let mut view = session.view_metadata().unwrap().clone();
        view.chart.as_mut().unwrap().distribution.as_mut().unwrap().bins = DistributionBins::Count { count: 3 };
        session.set_view_metadata(view).unwrap();
        assert!(session.undo());
        let chart = session.view_metadata().unwrap().chart.as_ref().unwrap();
        assert_eq!(chart.distribution.as_ref().unwrap().vector, named("x"));
        assert_eq!(chart.distribution.as_ref().unwrap().bins, DistributionBins::Count { count: 3 });
        assert!(session.redo());
        let mut view = session.view_metadata().unwrap().clone();
        view.chart.as_mut().unwrap().distribution.as_mut().unwrap().vector = named("y");
        session.set_view_metadata(view).unwrap();
        assert!(session.undo());
        assert_eq!(super::chart_distribution(session.view_metadata().unwrap()), Some(&named("y")));
    }

    #[test]
    fn chart_distribution_style_retains_source_identity_through_rename_and_pruning() {
        use super::super::{ChartDefinition, ChartKind, ChartSource, ChartVectorReference as Ref};
        use super::super::chart::distribution::{DistributionBins, DistributionDefinition, DistributionScale};
        use super::super::chart::options::{FiniteValue, LineStyle, MarkerStyle, SeriesKey, SeriesStyle};
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = [1, 2]\ny = [3, 4]").unwrap();
        let named = |name: &str| Ref::Named { name: name.into() };
        let mut view = session.view_metadata().unwrap().clone();
        let mut chart = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![named("y")] }, kind: ChartKind::Histogram, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: Some(DistributionDefinition { vector: named("x"), bins: DistributionBins::Auto, scale: DistributionScale::Count }), options: Default::default() };
        chart.options.styles.push(SeriesStyle { key: SeriesKey::Distribution(named("x")), color: [10, 20, 30, 40], line: LineStyle::Dashed, marker: MarkerStyle::Square, thickness: FiniteValue::try_from(3.0).unwrap() });
        view.chart = Some(chart); session.set_view_metadata(view).unwrap();
        let preview = session.preview_rename(0, "z").unwrap(); session.apply_rename(&preview).unwrap();
        assert_eq!(session.view_metadata().unwrap().chart.as_ref().unwrap().options.styles[0].key, SeriesKey::Distribution(named("z")));
        let mut view = session.view_metadata().unwrap().clone(); view.chart.as_mut().unwrap().options.styles[0].color[3] = 99; session.set_view_metadata(view).unwrap();
        assert!(session.undo());
        let chart = session.view_metadata().unwrap().chart.as_ref().unwrap();
        assert_eq!(chart.options.styles[0].key, SeriesKey::Distribution(named("x"))); assert_eq!(chart.options.styles[0].color[3], 99);
        assert!(session.redo());
        let mut view = session.view_metadata().unwrap().clone(); view.chart.as_mut().unwrap().distribution.as_mut().unwrap().vector = named("y"); session.set_view_metadata(view).unwrap();
        assert!(session.view_metadata().unwrap().chart.as_ref().unwrap().options.styles.is_empty());
        assert!(session.undo());
        assert!(session.view_metadata().unwrap().chart.as_ref().unwrap().options.styles.is_empty());
    }

    #[test]
    fn chart_distribution_style_shared_with_series_survives_rename_replay() {
        use super::super::{ChartDefinition, ChartKind, ChartSource, ChartVectorReference as Ref};
        use super::super::chart::distribution::{DistributionBins, DistributionDefinition, DistributionScale};
        use super::super::chart::options::{FiniteValue, LineStyle, MarkerStyle, SeriesKey, SeriesStyle};
        let (_dir, _root, mut session) = fixture(); session.replace_source("x = [1, 2]").unwrap();
        let named = |name: &str| Ref::Named { name: name.into() };
        let mut view = session.view_metadata().unwrap().clone();
        let mut chart = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![named("x")] }, kind: ChartKind::Histogram, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: Some(DistributionDefinition { vector: named("x"), bins: DistributionBins::Auto, scale: DistributionScale::Count }), options: Default::default() };
        chart.options.styles.push(SeriesStyle { key: SeriesKey::Distribution(named("x")), color: [1, 2, 3, 255], line: LineStyle::Solid, marker: MarkerStyle::Auto, thickness: FiniteValue::try_from(2.0).unwrap() });
        view.chart = Some(chart); session.set_view_metadata(view).unwrap();
        let preview = session.preview_rename(0, "z").unwrap(); session.apply_rename(&preview).unwrap();
        assert!(session.undo());
        let chart = session.view_metadata().unwrap().chart.as_ref().unwrap();
        assert_eq!(chart.options.styles.len(), 1); assert_eq!(chart.options.styles[0].key, SeriesKey::Distribution(named("x")));
        assert!(session.redo());
        let chart = session.view_metadata().unwrap().chart.as_ref().unwrap();
        assert_eq!(chart.options.styles.len(), 1); assert_eq!(chart.options.styles[0].key, SeriesKey::Distribution(named("z")));
    }

    #[test]
    fn chart_distribution_style_is_separate_from_ordinary_rename_collision() {
        use super::super::{ChartDefinition, ChartKind, ChartSource, ChartVectorReference as Ref};
        use super::super::chart::distribution::{DistributionBins, DistributionDefinition, DistributionScale};
        use super::super::chart::options::{FiniteValue, LineStyle, MarkerStyle, SeriesKey, SeriesStyle};
        let named = |name: &str| Ref::Named { name: name.into() };
        for styled in [true, false] {
            let (_dir, _root, mut session) = fixture(); session.replace_source("x = [1, 2]").unwrap();
            let mut view = session.view_metadata().unwrap().clone();
            let mut chart = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![named("z")] }, kind: ChartKind::Histogram, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: Some(DistributionDefinition { vector: named("x"), bins: DistributionBins::Auto, scale: DistributionScale::Count }), options: Default::default() };
            let style = |key, color| SeriesStyle { key, color, line: LineStyle::Solid, marker: MarkerStyle::Auto, thickness: FiniteValue::try_from(2.0).unwrap() };
            chart.options.styles.push(style(SeriesKey::Vector(named("z")), [0, 0, 255, 255]));
            if styled { chart.options.styles.push(style(SeriesKey::Distribution(named("x")), [255, 0, 0, 255])); }
            view.chart = Some(chart); session.set_view_metadata(view).unwrap();
            let preview = session.preview_rename(0, "z").unwrap(); session.apply_rename(&preview).unwrap();
            assert!(session.undo());
            let chart = session.view_metadata().unwrap().chart.as_ref().unwrap();
            assert!(chart.options.styles.iter().any(|style| style.key == SeriesKey::Vector(named("z")) && style.color == [0, 0, 255, 255]));
            assert_eq!(chart.options.styles.iter().any(|style| style.key == SeriesKey::Distribution(named("x")) && style.color == [255, 0, 0, 255]), styled);
            assert!(session.redo());
        }
    }

    #[test]
    fn chart_source_change_prunes_removed_styles() {
        use super::super::{ChartDefinition, ChartKind, ChartSource, ChartVectorReference as Ref};
        use super::super::chart::options::{FiniteValue, LineStyle, MarkerStyle, SeriesKey, SeriesStyle};
        let (_dir, _root, mut session) = fixture();
        let named = |name: &str| Ref::Named { name: name.into() };
        let mut view = session.view_metadata().unwrap().clone();
        let mut chart = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![named("x"), named("y")] }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: None, options: Default::default() };
        chart.options.styles.push(SeriesStyle { key: SeriesKey::Vector(named("x")), color: [1, 2, 3, 255], line: LineStyle::Solid, marker: MarkerStyle::Auto, thickness: FiniteValue::try_from(2.0).unwrap() });
        view.chart = Some(chart); session.set_view_metadata(view).unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        if let ChartSource::Vectors { y, .. } = &mut view.chart.as_mut().unwrap().source { y.remove(0); }
        session.set_view_metadata(view).unwrap();
        assert!(session.view_metadata().unwrap().chart.as_ref().unwrap().options.styles.is_empty());
    }

    #[test]
    fn chart_style_survives_rename_collision_and_restores_with_owner() {
        use super::super::{ChartDefinition, ChartKind, ChartSource, ChartVectorReference as Ref};
        use super::super::chart::options::{FiniteValue, LineStyle, MarkerStyle, SeriesKey, SeriesStyle};
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 2").unwrap();
        let named = |name: &str| Ref::Named { name: name.into() };
        let mut view = session.view_metadata().unwrap().clone();
        let mut chart = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![named("x")] }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: None, options: Default::default() };
        chart.options.styles.push(SeriesStyle { key: SeriesKey::Vector(named("x")), color: [9, 8, 7, 255], line: LineStyle::Dashed, marker: MarkerStyle::Auto, thickness: FiniteValue::try_from(2.0).unwrap() });
        view.chart = Some(chart); session.set_view_metadata(view).unwrap();
        let preview = session.preview_rename(0, "z").unwrap(); session.apply_rename(&preview).unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        if let ChartSource::Vectors { y, .. } = &mut view.chart.as_mut().unwrap().source { y.push(named("x")); }
        session.set_view_metadata(view).unwrap();
        assert!(session.undo());
        assert!(session.view_metadata().unwrap().chart.as_ref().unwrap().options.styles.is_empty());
        assert!(session.redo());
        let chart = session.view_metadata().unwrap().chart.as_ref().unwrap();
        assert_eq!(chart.options.styles[0].key, SeriesKey::Vector(named("z")));
        assert_eq!(chart.options.styles[0].color, [9, 8, 7, 255]);
    }

    #[test]
    fn chart_hidden_style_survives_chained_rename_and_retires_on_source_switch() {
        use super::super::{ChartDefinition, ChartKind, ChartSource, ChartVectorReference as Ref};
        use super::super::chart::options::{FiniteValue, LineStyle, MarkerStyle, SeriesKey, SeriesStyle};
        let named = |name: &str| Ref::Named { name: name.into() };
        for delete_chart in [false, true] {
            let (_dir, _root, mut session) = fixture();
            session.replace_source("x = 2").unwrap();
            let mut view = session.view_metadata().unwrap().clone();
            let mut chart = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![named("x")] }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: None, options: Default::default() };
            chart.options.styles.push(SeriesStyle { key: SeriesKey::Vector(named("x")), color: [9, 8, 7, 255], line: LineStyle::Dashed, marker: MarkerStyle::Auto, thickness: FiniteValue::try_from(2.0).unwrap() });
            view.chart = Some(chart); session.set_view_metadata(view).unwrap();
            let preview = session.preview_rename(0, "z").unwrap(); session.apply_rename(&preview).unwrap();
            let preview = session.preview_rename(0, "w").unwrap(); session.apply_rename(&preview).unwrap();
            let mut view = session.view_metadata().unwrap().clone();
            if let ChartSource::Vectors { y, .. } = &mut view.chart.as_mut().unwrap().source { y.push(named("z")); }
            session.set_view_metadata(view).unwrap();
            assert!(session.undo()); // w -> z collides with independent z
            assert!(session.undo()); // z -> x restores the same hidden owner
            let chart = session.view_metadata().unwrap().chart.as_ref().unwrap();
            assert_eq!(chart.options.styles[0].key, SeriesKey::Vector(named("x")));
            assert_eq!(chart.options.styles[0].color, [9, 8, 7, 255]);
            assert!(session.redo()); // x -> z collides again
            let mut view = session.view_metadata().unwrap().clone();
            if delete_chart { view.chart = None; } else { view.chart.as_mut().unwrap().source = ChartSource::Sensitivity { outputs: vec!["z".into()] }; }
            session.set_view_metadata(view).unwrap();
            assert!(session.redo());
            let chart = session.view_metadata().unwrap().chart.as_ref();
            if let Some(chart) = chart {
                assert!(matches!(&chart.source, ChartSource::Sensitivity { outputs } if outputs == &["z"]));
                assert!(chart.options.styles.is_empty());
            } else { assert!(delete_chart); }
        }
    }

    #[test]
    fn related_reference_transaction_undo_preserves_later_view_changes() {
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 2\nx + 1").unwrap();
        let line = session.line_id(0).unwrap();
        let view = super::PadViewMetadata {
            inputs: vec![super::super::persistence::PadInputReference { id: line, name: "x".into(), unavailable: false, config: Default::default() }],
            watches: vec!["x".into(), "other".into()],
            ..Default::default()
        };
        session.set_view_metadata(view).unwrap();
        let edits = super::RelatedReferenceEdits {
            inputs: vec![super::RelatedInputRename { id: line, before: "x".into(), after: "y".into() }],
            watches: vec![super::RelatedWatchRename { before: "x".into(), after: "y".into() }],
            ..Default::default()
        };
        let basis = session.evaluation_basis().unwrap();
        assert!(session.commit_related_edit(basis, "y = 2\ny + 1".into(), edits.clone()).unwrap());
        let mut later = session.view_metadata().unwrap().clone();
        later.inputs[0].unavailable = true;
        later.watches.push("later".into());
        session.set_view_metadata(later).unwrap();
        session.save_as("related edit").unwrap();
        let saved_id = session.current_id();
        assert!(session.undo());
        assert_eq!(session.source(), Some("x = 2\nx + 1"));
        assert_eq!(session.current_id(), saved_id);
        assert_eq!(session.view_metadata().unwrap().inputs[0].name, "x");
        assert!(session.view_metadata().unwrap().inputs[0].unavailable);
        assert_eq!(session.view_metadata().unwrap().watches, ["x", "other", "later"]);
        assert!(session.redo());
        assert_eq!(session.view_metadata().unwrap().inputs[0].name, "y");
        assert_eq!(session.view_metadata().unwrap().watches, ["y", "other", "later"]);
        let before = session.source().unwrap().to_owned();
        assert!(matches!(session.commit_related_edit(basis, "z = 1".into(), edits), Err(super::SessionError::StaleRelatedEdit)));
        assert_eq!(session.source(), Some(before.as_str()));
    }

    #[test]
    fn rename_transaction_preserves_ids_related_settings_and_independent_watches() {
        let (_dir, root, mut session) = fixture();
        session.replace_source("x = 2\nx + 1").unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        let view = super::PadViewMetadata { inputs: vec![super::super::persistence::PadInputReference { id: ids[0], name: "x".into(), unavailable: false, config: Default::default() }], watches: vec!["x".into()], ..Default::default() };
        session.set_view_metadata(view).unwrap();
        let preview = session.preview_rename(0, "y").unwrap();
        assert!(session.apply_rename(&preview).unwrap());
        assert_eq!(session.line_ids().unwrap(), ids);
        let mut view = session.view_metadata().unwrap().clone();
        view.inputs[0].unavailable = true;
        view.watches.push("x".into());
        session.set_view_metadata(view).unwrap();
        assert!(session.undo());
        assert_eq!(session.view_metadata().unwrap().watches, ["x"]);
        assert!(session.view_metadata().unwrap().inputs[0].unavailable);
        assert!(session.redo());
        assert_eq!(session.view_metadata().unwrap().watches, ["x", "y"]);
        session.save_as("renamed").unwrap();
        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(session.current_id().unwrap()).unwrap();
        assert_eq!(reopened.source(), Some("y = 2\ny + 1"));
        assert_eq!(reopened.line_ids().unwrap(), ids);
        assert!(session.apply_rename(&preview).is_err());
    }

    #[test]
    fn rename_undo_does_not_resurrect_removed_and_repinned_watch() {
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 2\nx").unwrap();
        session.set_view_metadata(super::PadViewMetadata { watches: vec!["x".into()], ..Default::default() }).unwrap();
        let preview = session.preview_rename(0, "y").unwrap();
        session.apply_rename(&preview).unwrap();
        session.set_view_metadata(super::PadViewMetadata::default()).unwrap();
        session.set_view_metadata(super::PadViewMetadata { watches: vec!["y".into()], ..Default::default() }).unwrap();
        assert!(session.undo());
        assert_eq!(session.view_metadata().unwrap().watches, ["y"]);
        assert!(session.redo());
        assert_eq!(session.view_metadata().unwrap().watches, ["y"]);
    }

    #[test]
    fn chained_renames_preserve_independently_repinned_watch_identity() {
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 2\nx").unwrap();
        session.set_view_metadata(super::PadViewMetadata { watches: vec!["x".into()], ..Default::default() }).unwrap();
        let first = session.preview_rename(0, "y").unwrap();
        session.apply_rename(&first).unwrap();
        let second = session.preview_rename(0, "z").unwrap();
        session.apply_rename(&second).unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        view.watches.push("y".into());
        session.set_view_metadata(view).unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        view.watches.retain(|name| name != "z");
        session.set_view_metadata(view).unwrap();
        assert!(session.undo());
        assert!(session.undo());
        assert_eq!(session.view_metadata().unwrap().watches, ["y"]);
        assert!(session.redo());
        assert!(session.redo());
        assert_eq!(session.view_metadata().unwrap().watches, ["y"]);
    }

    #[test]
    fn full_watch_list_never_blocks_rename_history() {
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 2\nx").unwrap();
        session.set_view_metadata(super::PadViewMetadata { watches: vec!["x".into()], ..Default::default() }).unwrap();
        let preview = session.preview_rename(0, "y").unwrap();
        session.apply_rename(&preview).unwrap();
        let mut view = session.view_metadata().unwrap().clone();
        view.watches.push("x".into());
        session.set_view_metadata(view).unwrap();
        assert!(session.undo());
        let mut view = session.view_metadata().unwrap().clone();
        view.watches.extend((0..63).map(|index| format!("watch_{index}")));
        session.set_view_metadata(view).unwrap();
        assert!(session.redo());
        assert_eq!(session.source(), Some("y = 2\ny"));
        assert_eq!(session.view_metadata().unwrap().watches.len(), 64);
        assert!(session.undo());
        let mut view = session.view_metadata().unwrap().clone();
        view.watches.pop();
        session.set_view_metadata(view).unwrap();
        assert!(session.redo());
        assert!(session.view_metadata().unwrap().watches.iter().any(|name| name == "y"));
    }

    #[test]
    fn rename_preview_rejects_stale_and_oversized_result() {
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 1\nx").unwrap();
        let preview = session.preview_rename(0, "longer").unwrap();
        session.replace_source("x = 2\nx").unwrap();
        assert!(session.apply_rename(&preview).is_err());
        assert_eq!(session.source(), Some("x = 2\nx"));
        let max = crate::calculation_pad::MAX_ADMITTED_DOCUMENT_BYTES;
        let source = format!("x = 1\n#{}", "a".repeat(max - 7));
        session.replace_source(source.clone()).unwrap();
        assert!(session.preview_rename(0, "longer").is_err());
        assert_eq!(session.source(), Some(source.as_str()));
    }

    #[test]
    fn recovery_restores_blank_nondefault_options_as_dirty_scratch() {
        let (_dir, _root, mut session) = fixture();
        let options = super::DocumentOptions { calculation: super::super::CalculationPolicy::Manual, profile: super::super::DocumentProfile::Standard };
        assert!(session.restore_recovered_document(String::new(), options).unwrap());
        assert_eq!(session.source(), Some(""));
        assert_eq!(session.revision(), Some(0));
        assert_eq!(session.document_options(), Some(options));
        assert!(session.is_dirty());
        assert!(!session.can_undo());
        assert!(!session.is_saved());
    }

    #[test]
    fn manual_mode_never_calculates_from_edits_or_observers() {
        let (_dir, _root, mut session) = fixture();
        let options = super::DocumentOptions { calculation: super::super::CalculationPolicy::Manual, profile: super::super::DocumentProfile::Standard };
        session.set_document_options(options).unwrap();
        for value in 1..=6 {
            session.replace_source(format!("x = {value}\nx * 2")).unwrap();
            assert!(session.evaluation().is_none());
            assert!(session.shared_evaluation().is_none());
            assert!(session.dependency_snapshot().is_none());
            assert!(session.comparison_context().is_none());
        }
        assert!(session.undo());
        assert!(session.redo());
        assert_eq!(session.evaluation_attempts, 0);
        assert_eq!(session.evaluation_status(), super::EvaluationStatus::NotCalculated);
        session.request_calculate();
        assert_eq!(session.evaluation_attempts, 1);
        assert_eq!(session.evaluation_status(), super::EvaluationStatus::Ready);
        let old = session.displayed_evaluation().unwrap();
        session.replace_source("x = 10\nx * 2").unwrap();
        assert_eq!(session.evaluation_status(), super::EvaluationStatus::Outdated);
        assert_eq!(&*old.source, "x = 6\nx * 2");
        assert!(session.evaluation_with_source().is_none());
        assert!(session.inspector_source(session.line_id(0).unwrap()).is_none());
        assert_eq!(session.evaluation_attempts, 1);
        session.set_document_options(super::DocumentOptions::default()).unwrap();
        session.advance_automatic();
        assert_eq!(session.evaluation_attempts, 2);
        assert_eq!(session.evaluation_status(), super::EvaluationStatus::Ready);
    }

    #[test]
    fn cancel_retires_same_revision_and_explicit_retry_uses_new_generation() {
        use std::sync::{mpsc, Mutex};
        use std::time::Duration;
        let (_dir, _root, mut session) = fixture();
        let (started_tx, started_rx) = mpsc::channel();
        let (continue_tx, continue_rx) = mpsc::channel();
        let continue_rx = Mutex::new(continue_rx);
        let (wake_tx, wake_rx) = mpsc::channel();
        let wake: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || { let _ = wake_tx.send(()); });
        let evaluator = std::sync::Arc::new(move |request: super::EvaluationRequest, cancellation| {
            started_tx.send(request.stamp.generation).unwrap();
            continue_rx.lock().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
            let document = super::CalculationDocument::with_profile(request.source, request.profile);
            super::project_evaluation(document.evaluate_with_cancellation(cancellation), &request.line_ids, request.stamp)
        });
        session.evaluation_worker = Some(super::EvaluationWorker::start_with(wake.clone(), evaluator).unwrap());
        session.evaluation_wake = Some(wake);
        session.set_document_options(super::DocumentOptions { calculation: super::super::CalculationPolicy::Manual, profile: super::super::DocumentProfile::Standard }).unwrap();
        session.replace_source("sum(range(1000))").unwrap();
        session.request_calculate();
        let first = session.evaluation_basis().unwrap();
        assert_eq!(started_rx.recv_timeout(Duration::from_secs(10)).unwrap(), first.generation);
        assert_eq!(session.evaluation_status(), super::EvaluationStatus::Pending);
        session.request_calculate();
        assert_eq!(session.evaluation_attempts, 1);
        assert!(session.cancel_calculation(first));
        assert_eq!(session.evaluation_status(), super::EvaluationStatus::Canceled);
        session.advance_automatic();
        assert_eq!(session.evaluation_attempts, 1);
        session.request_calculate();
        assert_eq!(session.evaluation_attempts, 2);
        assert_ne!(session.evaluation_basis(), Some(first));
        assert!(!session.cancel_calculation(first));
        continue_tx.send(()).unwrap();
        let second = session.evaluation_basis().unwrap();
        assert_eq!(started_rx.recv_timeout(Duration::from_secs(10)).unwrap(), second.generation);
        continue_tx.send(()).unwrap();
        wake_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        session.request_calculate();
        assert_eq!(session.evaluation_attempts, 2);
        session.advance_automatic();
        assert_eq!(session.evaluation_status(), super::EvaluationStatus::Ready);
        session.request_calculate();
        assert_eq!(session.evaluation_attempts, 3);
        let third = session.evaluation_basis().unwrap();
        assert_eq!(started_rx.recv_timeout(Duration::from_secs(10)).unwrap(), third.generation);
        continue_tx.send(()).unwrap();
        wake_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        session.advance_automatic();
        assert_eq!(session.evaluation_status(), super::EvaluationStatus::Ready);
    }

    #[test]
    fn extended_capacity_worker_and_controlled_cancellation_observation() {
        use std::sync::{mpsc, Arc, Mutex};
        use std::time::{Duration, Instant};
        let (_dir, _root, mut session) = fixture();
        let (started_tx, started_rx) = mpsc::channel();
        let (continue_tx, continue_rx) = mpsc::channel();
        let continue_rx = Mutex::new(continue_rx);
        let (wake_tx, wake_rx) = mpsc::channel();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || { let _ = wake_tx.send(()); });
        let evaluator = Arc::new(move |request: super::EvaluationRequest, cancellation| {
            started_tx.send(request.stamp).unwrap();
            continue_rx.lock().unwrap().recv_timeout(Duration::from_secs(30)).unwrap();
            let document = super::CalculationDocument::with_profile(request.source, request.profile);
            super::project_evaluation(document.evaluate_with_cancellation(cancellation), &request.line_ids, request.stamp)
        });
        session.evaluation_worker = Some(super::EvaluationWorker::start_with(wake.clone(), evaluator).unwrap());
        session.evaluation_wake = Some(wake);
        session.set_document_options(super::DocumentOptions { calculation: super::super::CalculationPolicy::Manual, profile: super::super::DocumentProfile::Extended }).unwrap();
        let source = vec![format!("1 #{}", "x".repeat(127)); 1000].join("\n");
        session.replace_source(&source).unwrap();
        let start = Instant::now();
        session.request_calculate();
        let retired = started_rx.recv_timeout(Duration::from_secs(30)).unwrap();
        assert!(session.cancel_calculation(retired));
        session.request_calculate();
        let current = session.evaluation_basis().unwrap();
        assert_ne!(retired, current);
        continue_tx.send(()).unwrap();
        assert_eq!(started_rx.recv_timeout(Duration::from_secs(30)).unwrap(), current);
        assert!(session.displayed_evaluation().is_none());
        continue_tx.send(()).unwrap();
        wake_rx.recv_timeout(Duration::from_secs(30)).unwrap();
        session.advance_automatic();
        let evaluation = session.displayed_evaluation().unwrap();
        assert_eq!(evaluation.basis, current);
        assert!(evaluation.diagnostic.is_none());
        println!("capacity worker profile=Extended bytes={} rows=1000 elapsed={:?} usage={:?} retired_never_published=true", source.len(), start.elapsed(), evaluation.usage);
    }

    #[test]
    fn usage_counters_belong_to_current_evaluation_basis() {
        let (_dir, _root, mut session) = fixture();
        session.set_document_options(super::DocumentOptions { calculation: super::super::CalculationPolicy::Manual, profile: super::super::DocumentProfile::Standard }).unwrap();
        session.replace_source("for i in range(3):\n  x = i + 1").unwrap();
        session.request_calculate();
        let first = session.displayed_evaluation().unwrap();
        let usage = first.usage.unwrap();
        assert_eq!(usage.loop_iterations, 3);
        assert!(usage.work_units > 0);
        session.replace_source("1").unwrap();
        assert_ne!(Some(first.basis), session.evaluation_basis());
        assert!(session.evaluation_with_source().is_none());
    }

    #[test]
    fn profile_shrink_keeps_source_and_reports_current_admission_failure() {
        let (_dir, _root, mut session) = fixture();
        session.set_document_options(super::DocumentOptions { calculation: super::super::CalculationPolicy::Manual, profile: super::super::DocumentProfile::Extended }).unwrap();
        let source = vec!["1"; 257].join("\n");
        session.replace_source(&source).unwrap();
        session.set_document_options(super::DocumentOptions { calculation: super::super::CalculationPolicy::Manual, profile: super::super::DocumentProfile::Standard }).unwrap();
        assert_eq!(session.source(), Some(source.as_str()));
        assert_eq!(session.evaluation_status(), super::EvaluationStatus::NotCalculated);
        session.request_calculate();
        let result = session.displayed_evaluation().unwrap();
        assert!(matches!(&result.diagnostic, Some(super::DocumentDiagnostic::TooManyLines { .. })));
        assert_eq!(result.usage, None);
        session.replace_source("1").unwrap();
        assert_eq!(session.source(), Some("1"));
    }

    #[test]
    fn extended_evaluation_limits_preserve_larger_admitted_saved_source() {
        use super::super::limits::{EXTENDED_BYTES, EXTENDED_LINES};
        let (_dir, root, mut session) = fixture();
        session.set_document_options(super::DocumentOptions { profile: super::super::DocumentProfile::Extended, ..Default::default() }).unwrap();
        let exact_bytes = format!("#{}", "x".repeat(EXTENDED_BYTES - 1));
        session.replace_source(&exact_bytes).unwrap();
        assert!(session.evaluation().unwrap().diagnostic.is_none());
        let larger = format!("{exact_bytes}x");
        session.replace_source(&larger).unwrap();
        assert!(matches!(&session.evaluation().unwrap().diagnostic, Some(super::DocumentDiagnostic::DocumentTooLarge { max_bytes }) if *max_bytes == EXTENDED_BYTES));
        session.save_as("over extended limit").unwrap();
        let id = session.current_id().unwrap();
        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(id).unwrap();
        assert_eq!(reopened.source(), Some(larger.as_str()));
        reopened.replace_source(vec!["1"; EXTENDED_LINES].join("\n")).unwrap();
        assert!(reopened.evaluation().unwrap().diagnostic.is_none());
        reopened.replace_source(vec!["1"; EXTENDED_LINES + 1].join("\n")).unwrap();
        assert!(matches!(&reopened.evaluation().unwrap().diagnostic, Some(super::DocumentDiagnostic::TooManyLines { max_lines }) if *max_lines == EXTENDED_LINES));
    }

    #[test]
    fn changing_profile_retires_pending_extended_worker() {
        use std::sync::{mpsc, Mutex};
        use std::time::Duration;
        let (_dir, _root, mut session) = fixture();
        let (started_tx, started_rx) = mpsc::channel();
        let (continue_tx, continue_rx) = mpsc::channel();
        let continue_rx = Mutex::new(continue_rx);
        let wake: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(|| {});
        let evaluator = std::sync::Arc::new(move |request: super::EvaluationRequest, cancellation| {
            started_tx.send(()).unwrap();
            continue_rx.lock().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
            let document = super::CalculationDocument::with_profile(request.source, request.profile);
            super::project_evaluation(document.evaluate_with_cancellation(cancellation), &request.line_ids, request.stamp)
        });
        session.evaluation_worker = Some(super::EvaluationWorker::start_with(wake.clone(), evaluator).unwrap());
        session.evaluation_wake = Some(wake);
        session.replace_source("1").unwrap();
        session.set_document_options(super::DocumentOptions { profile: super::super::DocumentProfile::Extended, ..Default::default() }).unwrap();
        session.advance_automatic();
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let old = session.evaluation_basis().unwrap();
        assert_eq!(session.evaluation_status(), super::EvaluationStatus::Pending);
        session.set_document_options(super::DocumentOptions::default()).unwrap();
        assert_ne!(session.evaluation_basis(), Some(old));
        session.advance_automatic();
        assert_eq!(session.evaluation_status(), super::EvaluationStatus::Ready);
        continue_tx.send(()).unwrap();
        session.advance_automatic();
        assert_eq!(session.evaluation_status(), super::EvaluationStatus::Ready);
        assert_ne!(session.shared_evaluation().unwrap().basis, old);
    }

    #[test]
    fn watches_use_final_current_binding_including_failed_reassignment() {
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 2\nx = x + 3\nbroken = 7\nbroken = 1 / 0").unwrap();
        session.evaluation().unwrap();
        let evaluation = session.shared_evaluation().unwrap();
        assert!(matches!(&evaluation.final_bindings["x"].outcome, LineOutcome::Value(CalculationValue::Scalar(value)) if *value == 5.0));
        assert!(matches!(&evaluation.final_bindings["broken"].outcome, LineOutcome::Error(_)));
        assert!(!evaluation.final_bindings.contains_key("missing"));
    }

    #[test]
    fn compound_loop_session_preserves_source_history_bindings_and_inspector() {
        let (_directory, root, mut session) = fixture();
        let source = "{顧客 名} = u64(9007199254740993)\r\nfor n in [1, 2]:\r\n    {顧客 名} += 1\r\n{顧客 名}";
        session.replace_source(source).unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        assert_eq!(integer(&mut session, 3).bit_pattern(), 9007199254740995);
        let result = session.shared_evaluation().unwrap();
        assert_eq!(result.final_bindings["顧客 名"].line_index, 2);
        let graph = session.dependency_snapshot().unwrap();
        assert_eq!(graph.rows[2].dependencies, [ids[2]]);
        assert_eq!(session.inspector_source(ids[2]).unwrap().value.bit_pattern(), 9007199254740995);
        let inputs = super::super::document::literal_inputs(source);
        assert_eq!(inputs.iter().map(|input| input.line).collect::<Vec<_>>(), vec![0]);
        assert!(session.input_replacement(session.revision().unwrap(), ids[2], "    {顧客 名} += 1", "9").is_err());

        session.replace_line(ids[0], "{顧客 名} = u64(9007199254740994)").unwrap();
        assert_eq!(integer(&mut session, 3).bit_pattern(), 9007199254740996);
        assert!(session.undo());
        assert_eq!(session.source(), Some(source));
        assert_eq!(integer(&mut session, 3).bit_pattern(), 9007199254740995);
        assert!(session.redo());
        assert_eq!(integer(&mut session, 3).bit_pattern(), 9007199254740996);
        let edited = session.source().unwrap().to_owned();
        assert!(edited.contains("\r\n"));
        session.save_as("compound loop").unwrap();
        let id = session.current_id().unwrap();
        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(id).unwrap();
        assert_eq!(reopened.source(), Some(edited.as_str()));
        assert_eq!(reopened.line_ids().unwrap(), ids);
        assert_eq!(integer(&mut reopened, 3).bit_pattern(), 9007199254740996);
    }

    #[test]
    fn literal_inputs_are_source_bound_and_one_undo_restores_shadowed_assignment() {
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 2\nx = u64(4)\ndistance = 3 m\nformula = x + 1\nfor i in range(2):\n    local = 3").unwrap();
        let inputs = super::super::document::literal_inputs(session.source().unwrap());
        assert_eq!(inputs.iter().map(|input| input.line).collect::<Vec<_>>(), vec![0, 1, 2]);
        let ids = session.line_ids().unwrap().to_vec();
        let revision = session.revision().unwrap();
        assert!(session.input_replacement(revision, ids[1], &inputs[1].source_line, "-").is_err());
        assert!(session.input_replacement(revision, ids[1], &inputs[1].source_line, "1+2").is_err());
        let replacement = session.input_replacement(revision, ids[1], &inputs[1].source_line, "9").unwrap();
        session.replace_source(replacement).unwrap();
        assert!(session.source().unwrap().contains("x = u64(9)"));
        assert!(session.input_replacement(revision, ids[1], &inputs[1].source_line, "5").is_err());
        assert!(session.undo());
        assert!(session.source().unwrap().contains("x = u64(4)"));
        let quantity = session.input_replacement(session.revision().unwrap(), ids[2], &inputs[2].source_line, "7").unwrap();
        assert!(quantity.contains("distance = 7 m"));
    }

    #[test]
    fn configured_input_survives_rename_save_reopen_and_rejects_unit_changes() {
        let (_dir, root, mut session) = fixture();
        session.replace_source("distance = 5 m\ndistance * 2").unwrap();
        let line = session.line_id(0).unwrap();
        let view = super::PadViewMetadata { inputs: vec![super::super::persistence::PadInputReference { id: line, name: "distance".into(), unavailable: false, config: Default::default() }], ..Default::default() };
        session.set_view_metadata(view).unwrap();
        let options = super::DocumentOptions { calculation: super::super::CalculationPolicy::Manual, ..Default::default() };
        session.set_document_options(options).unwrap();
        session.configure_input(line, super::super::InputConfig { label: "Trip".into(), suffix: "display only".into(), minimum: Some("6".into()), maximum: Some("10".into()), step: Some("1".into()), control: super::super::InputControl::Slider, ..Default::default() }).unwrap();
        let projection = session.input_projection(line).unwrap();
        assert!(projection.current_error.is_some());
        assert!(projection.slider_available);
        assert_eq!(projection.config.suffix, "display only");
        let descriptor = projection.descriptor;
        let revision = session.revision().unwrap();
        assert!(session.input_replacement(revision, line, &descriptor.source_line, "11").is_err());
        let replacement = session.input_replacement(revision, line, &descriptor.source_line, "7").unwrap();
        session.replace_source(replacement).unwrap();
        assert_eq!(session.evaluation_attempts, 0);
        let preview = session.preview_rename(0, "length").unwrap();
        session.apply_rename(&preview).unwrap();
        assert_eq!(session.input_projection(line).unwrap().config.label, "Trip");
        assert!(session.undo());
        assert_eq!(session.input_projection(line).unwrap().config.label, "Trip");
        assert!(session.redo());
        session.save_as("configured input").unwrap();
        let id = session.current_id().unwrap();
        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(id).unwrap();
        assert_eq!(reopened.input_projection(line).unwrap().config.label, "Trip");
        reopened.replace_source("length = 7 cm\nlength * 2").unwrap();
        assert!(reopened.input_projection(line).unwrap_err().contains("type or unit changed"));
    }

    #[test]
    fn decorative_input_settings_survive_type_and_unit_changes() {
        let (_dir, _root, mut session) = fixture();
        session.replace_source("size = 2 m").unwrap();
        let id = session.line_id(0).unwrap();
        session.set_view_metadata(super::PadViewMetadata { inputs: vec![super::super::persistence::PadInputReference { id, name: "size".into(), unavailable: false, config: Default::default() }], ..Default::default() }).unwrap();
        session.configure_input(id, super::super::InputConfig { label: "Size".into(), suffix: "ore".into(), ..Default::default() }).unwrap();
        session.replace_source("size = 3 cm").unwrap();
        let projection = session.input_projection(id).unwrap();
        assert_eq!(projection.config.label, "Size");
        assert_eq!(projection.config.suffix, "ore");
        assert!(projection.current_error.is_none());
        assert!(session.input_replacement(session.revision().unwrap(), id, &projection.descriptor.source_line, "4").unwrap().contains("4 cm"));
    }

    #[test]
    fn sensitivity_is_independent_of_manual_main_results_and_supersedes_old_runs() {
        use std::sync::mpsc;
        use std::time::Duration;
        let (_dir, _root, mut session) = fixture();
        session.replace_source("ore = 1000\nxp = ore * 56.2\nremaining = 1000000 - xp").unwrap();
        session.set_document_options(super::DocumentOptions { calculation: super::super::CalculationPolicy::Manual, ..Default::default() }).unwrap();
        session.request_calculate();
        let original_result = session.shared_evaluation().unwrap();
        let source = session.source().unwrap().to_owned();
        let dirty = session.is_dirty();
        let undo = session.history.undo.len();
        let (wake_tx, wake_rx) = mpsc::channel();
        session.enable_background_evaluation(move || { let _ = wake_tx.send(()); });
        let definition = super::super::SensitivityDefinition { input: session.line_id(0).unwrap(), start: "1000".into(), end: "5000".into(), sampling: super::super::Sampling::Step("1000".into()), outputs: vec!["xp".into(), "remaining".into()] };
        let old = session.request_sensitivity(definition.clone()).unwrap();
        let current = session.request_sensitivity(definition).unwrap();
        assert_ne!(old.generation, current.generation);
        for _ in 0..20 {
            wake_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            session.poll_sensitivity();
            if session.sensitivity_status() == super::super::SensitivityStatus::Ready { break; }
        }
        let table = session.sensitivity_table().unwrap();
        assert_eq!(table.basis, current);
        assert_eq!(table.rows.len(), 5);
        assert_eq!(session.source(), Some(source.as_str()));
        assert_eq!(session.is_dirty(), dirty);
        assert_eq!(session.history.undo.len(), undo);
        assert!(std::sync::Arc::ptr_eq(&session.shared_evaluation().unwrap(), &original_result));
        session.request_calculate();
        assert!(session.sensitivity_table().is_some());
        session.replace_source("ore = 2000\nxp = ore * 56.2\nremaining = 1000000 - xp").unwrap();
        assert!(session.sensitivity_table().is_none());
        assert_eq!(session.sensitivity_status(), super::super::SensitivityStatus::Outdated);
    }

    #[test]
    fn sensitivity_cancel_and_retry_retire_the_exact_run() {
        use std::sync::mpsc;
        use std::time::Duration;
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 1\ny = x + 1").unwrap();
        let (wake_tx, wake_rx) = mpsc::channel();
        session.enable_background_evaluation(move || { let _ = wake_tx.send(()); });
        let definition = super::super::SensitivityDefinition { input: session.line_id(0).unwrap(), start: "0".into(), end: "999".into(), sampling: super::super::Sampling::Count(1000), outputs: vec!["y".into()] };
        let old = session.request_sensitivity(definition.clone()).unwrap();
        assert!(session.cancel_sensitivity(&old));
        assert_eq!(session.sensitivity_status(), super::super::SensitivityStatus::Canceled);
        assert!(session.sensitivity_table().is_none());
        let mut retry = definition;
        retry.sampling = super::super::Sampling::Count(2);
        let current = session.request_sensitivity(retry).unwrap();
        assert!(!session.cancel_sensitivity(&old));
        for _ in 0..20 {
            wake_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            session.poll_sensitivity();
            if session.sensitivity_status() == super::super::SensitivityStatus::Ready { break; }
        }
        assert_eq!(session.sensitivity_table().unwrap().basis, current);
    }

    #[test]
    fn sensitivity_does_not_apply_editable_input_bounds() {
        use std::sync::mpsc;
        use std::time::Duration;
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = 1\ny = x").unwrap();
        let id = session.line_id(0).unwrap();
        session.set_view_metadata(super::PadViewMetadata { inputs: vec![super::super::persistence::PadInputReference { id, name: "x".into(), unavailable: false, config: Default::default() }], ..Default::default() }).unwrap();
        session.configure_input(id, super::super::InputConfig { minimum: Some("1".into()), maximum: Some("1".into()), ..Default::default() }).unwrap();
        session.set_document_options(super::DocumentOptions { calculation: super::super::CalculationPolicy::Manual, ..Default::default() }).unwrap();
        session.request_calculate();
        let main = session.shared_evaluation().unwrap();
        let source = session.source().unwrap().to_owned();
        let history = session.history.undo.len();
        let dirty = session.is_dirty();
        let (wake_tx, wake_rx) = mpsc::channel();
        session.enable_background_evaluation(move || { let _ = wake_tx.send(()); });
        let definition = super::super::SensitivityDefinition { input: id, start: "0".into(), end: "2".into(), sampling: super::super::Sampling::Count(3), outputs: vec!["y".into()] };
        session.request_sensitivity(definition).unwrap();
        for _ in 0..20 { wake_rx.recv_timeout(Duration::from_secs(10)).unwrap(); session.poll_sensitivity(); if session.sensitivity_status() == super::super::SensitivityStatus::Ready { break; } }
        let table = session.sensitivity_table().unwrap();
        assert_eq!(table.rows.len(), 3);
        assert!(table.rows.iter().all(|row| row.cells[0].is_ok()));
        let dataset = super::super::ChartDataset::from_sensitivity(&table, &[0]).unwrap();
        assert_eq!(dataset.series[0].points.len(), 3);
        assert_eq!(dataset.series[0].points[0].as_ref().unwrap().x, 0.0);
        assert_eq!(dataset.series[0].points[2].as_ref().unwrap().y, 2.0);
        assert_eq!(session.source(), Some(source.as_str()));
        assert_eq!(session.history.undo.len(), history);
        assert_eq!(session.is_dirty(), dirty);
        assert!(std::sync::Arc::ptr_eq(&session.shared_evaluation().unwrap(), &main));
        session.replace_line(id, "x = 2").unwrap();
        assert!(session.sensitivity_table().is_none());
        assert!(session.shared_evaluation().is_none());
    }

    #[test]
    fn parameter_rename_and_undo_leave_document_binding_unchanged() {
        let (_dir, _root, mut session) = fixture();
        let source = "x = 10\nfn add_one(x) = (\n  x + 1\n)\nadd_one(2) + x";
        session.replace_source(source).unwrap();
        let parameter = source.find("add_one(x)").unwrap() + "add_one(".len();
        let preview = session.preview_rename(parameter, "value").unwrap();
        assert!(session.apply_rename(&preview).unwrap());
        assert_eq!(session.source(), Some("x = 10\nfn add_one(value) = (\n  value + 1\n)\nadd_one(2) + x"));
        assert_eq!(scalar(&mut session, 4), 13.0);
        assert!(session.undo());
        assert_eq!(session.source(), Some(source));
        assert_eq!(scalar(&mut session, 4), 13.0);
    }

    #[test]
    fn callback_function_rename_is_one_undoable_edit() {
        let (_dir, _root, mut session) = fixture();
        let source = "fn scale(value) = value * 2\nmap([1, 2], scale)";
        session.replace_source(source).unwrap();
        let preview = session.preview_rename(source.find("scale").unwrap(), "double_value").unwrap();
        assert!(session.apply_rename(&preview).unwrap());
        assert_eq!(session.source(), Some("fn double_value(value) = value * 2\nmap([1, 2], double_value)"));
        assert!(session.undo());
        assert_eq!(session.source(), Some(source));
    }

    #[test]
    fn inspector_observes_exact_values_and_preserves_dirty_binding() {
        use crate::calculation_pad::{InspectorSourceOrigin, ProgrammerInspector};
        let (_dir, _root, mut session) = fixture();
        session
            .replace_source("123 >> 12\n9007199254740993 dec to hex\n340282366920938463463374607431768211455 dec to hex")
            .unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        assert!(session.inspector_source(ids[0]).is_none());
        session.evaluation();
        let mut inspector = ProgrammerInspector::default();
        inspector.observe(session.inspector_source(ids[0]));
        assert_eq!(inspector.value().formatted_decimal(), "0");
        inspector.toggle_bit(0).unwrap();
        inspector.observe(session.inspector_source(ids[1]));
        assert_eq!(inspector.adopted().unwrap().line_id, ids[0]);
        assert_eq!(
            inspector.candidate().unwrap().origin,
            InspectorSourceOrigin::LegacyExactBase
        );
        inspector.refresh().unwrap();
        assert_eq!(inspector.value().formatted_decimal(), "9007199254740993");
        inspector.observe(session.inspector_source(ids[2]));
        assert_eq!(inspector.value().formatted_decimal(), u128::MAX.to_string());
        assert_eq!(inspector.value().mode().type_name(), "u128");
        assert_eq!(session.evaluation_count(), 1);
    }

    #[test]
    fn inspector_insert_checks_binding_and_is_one_undo_step() {
        use crate::calculation_pad::{InspectorInsertError, ProgrammerInspector};
        let (_dir, _root, mut session) = fixture();
        session.replace_source("i8(-1)\nα").unwrap();
        session.save_as("inspector edit atomicity").unwrap();
        assert!(!session.is_dirty());
        let id = session.line_id(0).unwrap();
        session.evaluation();
        let mut inspector = ProgrammerInspector::default();
        inspector.observe(session.inspector_source(id));
        let fingerprint = |session: &CalculationPadSession| {
            (
                session.source().map(str::to_owned),
                session.line_ids().map(<[LineId]>::to_vec),
                session.revision(),
                session.is_dirty(),
                session.history.undo.len(),
                session.history.redo.len(),
                session.cached_evaluation_revision(),
                session.evaluation_count(),
                session
                    .current
                    .as_ref()
                    .and_then(|document| document.cached_evaluation.clone()),
            )
        };
        let unchanged = fingerprint(&session);
        let no_op = inspector.insert_request(0..6).unwrap();
        assert!(!session.apply_inspector_insert(no_op).unwrap());
        assert_eq!(fingerprint(&session), unchanged);
        inspector.toggle_bit(0).unwrap();
        let bad = inspector.insert_request(8..9).unwrap();
        let before = session.source().unwrap().to_owned();
        assert!(matches!(
            session.apply_inspector_insert(bad),
            Err(InspectorInsertError::InvalidTarget)
        ));
        assert_eq!(session.source(), Some(before.as_str()));
        assert_eq!(fingerprint(&session), unchanged);
        let undo_entries = session.history.undo.len();
        let stale = inspector.insert_request(0..6).unwrap();
        let request = inspector.insert_request(0..6).unwrap();
        assert!(session.apply_inspector_insert(request).unwrap());
        assert_eq!(session.history.undo.len(), undo_entries + 1);
        assert_eq!(session.source(), Some("i8(-2)\nα"));
        assert!(session.undo());
        assert_eq!(session.source(), Some(before.as_str()));
        assert!(session.redo());
        assert_eq!(session.source(), Some("i8(-2)\nα"));
        assert!(matches!(
            session.apply_inspector_insert(stale),
            Err(InspectorInsertError::StaleSource)
        ));
    }

    #[test]
    fn inspector_rejects_pending_and_reopened_document_identity() {
        use crate::calculation_pad::{InspectorInsertError, ProgrammerInspector};
        let (_dir, _root, mut session) = fixture();
        session.replace_source("u64(7)").unwrap();
        session.save_as("inspector reopen").unwrap();
        let id = session.current_id().unwrap();
        assert_eq!(
            session.request_open(id).unwrap(),
            TransitionOutcome::Applied
        );
        let line = session.line_id(0).unwrap();
        session.evaluation();
        let mut inspector = ProgrammerInspector::default();
        inspector.observe(session.inspector_source(line));
        let original = inspector.adopted().unwrap().clone();
        let request = inspector.insert_request(0..6).unwrap();
        session.replace_source("u64(8)").unwrap();
        assert_eq!(
            session.request_open(id).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert!(matches!(
            session.apply_inspector_insert(request.clone()),
            Err(InspectorInsertError::PendingTransition)
        ));
        assert_eq!(session.source(), Some("u64(8)"));
        session
            .resolve_pending(TransitionDecision::Discard)
            .unwrap();
        session.evaluation();
        let reopened = session.inspector_source(line).unwrap();
        assert_eq!(original.document_id, reopened.document_id);
        assert_eq!(original.revision, 0);
        assert_eq!(original.revision, reopened.revision);
        assert_ne!(original.incarnation, reopened.incarnation);
        assert_eq!(original.basis.document, reopened.basis.document);
        assert_eq!(original.basis.document, id);
        assert_eq!(original.basis.revision, reopened.basis.revision);
        assert_ne!(original.basis.incarnation, reopened.basis.incarnation);
        assert_eq!(original.line_id, reopened.line_id);
        assert_eq!(original.source_range, reopened.source_range);
        assert_eq!(original.source_text, reopened.source_text);
        assert_eq!(original.value, reopened.value);
        assert_eq!(original.origin, reopened.origin);
        assert!(matches!(
            session.apply_inspector_insert(request),
            Err(InspectorInsertError::StaleSource)
        ));
    }

    #[test]
    fn inspector_refresh_and_canonical_roundtrip_preserve_pad_until_insert() {
        use crate::calculation_pad::{
            IntegerMode, IntegerWidth, ProgrammerInspector, TypedInteger,
        };
        let (_dir, _root, mut session) = fixture();
        session
            .replace_source("u64(9007199254740993)\nu8(1)")
            .unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        session.evaluation();
        let revision = session.revision();
        let history = session.can_undo();
        let mut inspector = ProgrammerInspector::default();
        inspector.observe(session.inspector_source(ids[0]));
        inspector.toggle_bit(0).unwrap();
        inspector.toggle_bit(0).unwrap();
        assert!(!inspector.numeric_dirty());
        inspector.set_annotation(0..1, "flag".into()).unwrap();
        inspector.observe(session.inspector_source(ids[1]));
        assert_eq!(inspector.adopted().unwrap().line_id, ids[0]);
        inspector.observe(None);
        assert!(inspector.refresh().is_err());
        assert_eq!(inspector.adopted().unwrap().line_id, ids[0]);
        inspector.observe(session.inspector_source(ids[1]));
        inspector.refresh().unwrap();
        assert_eq!(inspector.adopted().unwrap().line_id, ids[1]);
        assert_eq!(inspector.annotations()[0].label, "flag");
        assert_eq!(session.revision(), revision);
        assert_eq!(session.evaluation_count(), 1);
        assert_eq!(session.can_undo(), history);
        for value in [
            TypedInteger::from_signed(-1, IntegerMode::signed(IntegerWidth::Bits8)).unwrap(),
            TypedInteger::from_signed(i128::MIN, IntegerMode::signed(IntegerWidth::Bits128))
                .unwrap(),
            TypedInteger::from_unsigned(
                9_007_199_254_740_993,
                IntegerMode::unsigned(IntegerWidth::Bits64),
            )
            .unwrap(),
            TypedInteger::from_unsigned(u128::MAX, IntegerMode::unsigned(IntegerWidth::Bits128))
                .unwrap(),
        ] {
            inspector.set_value(value);
            let expression = inspector.canonical_expression();
            let document = crate::calculation_pad::CalculationDocument::new(expression);
            assert!(
                matches!(&document.evaluate().lines[0].outcome, LineOutcome::Value(CalculationValue::Integer(actual)) if *actual == value)
            );
        }
    }

    #[test]
    fn inspector_dirty_work_survives_changed_or_missing_candidate() {
        use crate::calculation_pad::ProgrammerInspector;
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = u8(1)\nx + 1").unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        session.evaluation();
        let mut inspector = ProgrammerInspector::default();
        inspector.observe(session.inspector_source(ids[1]));
        let adopted = inspector.adopted().unwrap().clone();
        inspector.toggle_bit(0).unwrap();
        let working = inspector.value();
        session.replace_line(ids[0], "x = u8(2)").unwrap();
        assert!(session.inspector_source(ids[1]).is_none());
        session.evaluation();
        inspector.observe(session.inspector_source(ids[1]));
        assert_eq!(inspector.adopted(), Some(&adopted));
        assert_eq!(inspector.value(), working);
        assert_ne!(inspector.candidate(), inspector.adopted());
        session.replace_line(ids[1], "1.5").unwrap();
        session.evaluation();
        inspector.observe(session.inspector_source(ids[1]));
        assert!(inspector.candidate().is_none());
        assert!(inspector.refresh().is_err());
        assert_eq!(inspector.value(), working);
        assert_eq!(inspector.adopted(), Some(&adopted));
        session.replace_source("u8(0)").unwrap();
        session.evaluation();
        inspector.observe(session.inspector_source(ids[1]));
        assert!(inspector.candidate().is_none());
        assert_eq!(inspector.adopted(), Some(&adopted));
    }

    fn fixture() -> (tempfile::TempDir, AppDataRoot, CalculationPadSession) {
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path().join("app-data"));
        let session = CalculationPadSession::new(&root);
        (directory, root, session)
    }

    #[test]
    fn background_results_follow_save_as_and_document_incarnation() {
        use std::sync::mpsc;
        use std::time::Duration;
        let (_dir, _root, mut session) = fixture();
        let (wake, wakes) = mpsc::channel();
        session.enable_background_evaluation(move || {
            let _ = wake.send(());
        });
        session
            .replace_source("total = 0\nfor x in range(4):\n    total = total + x\ntotal")
            .unwrap();
        let initial = session.current_id().unwrap();
        assert!(session.evaluation().is_none());
        assert_eq!(
            session.evaluation_status(),
            super::EvaluationStatus::Pending
        );
        wakes.recv_timeout(Duration::from_secs(10)).unwrap();
        session.save_as("worker identity").unwrap();
        let saved = session.current_id().unwrap();
        assert_ne!(initial, saved);
        assert!(session.evaluation().is_none());
        wakes.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(scalar(&mut session, 3), 6.0);
        assert_eq!(session.evaluation_status(), super::EvaluationStatus::Ready);
        assert_eq!(session.comparison_context().unwrap().document_id, saved);
        let incarnation = session.document_incarnation().unwrap();
        assert_eq!(
            session.request_close().unwrap(),
            super::TransitionOutcome::Applied
        );
        assert_eq!(
            session.request_open(saved).unwrap(),
            super::TransitionOutcome::Applied
        );
        assert_ne!(session.document_incarnation(), Some(incarnation));
        assert!(session.evaluation().is_none());
        wakes.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(scalar(&mut session, 3), 6.0);
    }

    fn scalar(session: &mut CalculationPadSession, line_index: usize) -> f64 {
        match &session.evaluation().unwrap().lines[line_index].outcome {
            LineOutcome::Value(CalculationValue::Scalar(value)) => *value,
            other => panic!("expected scalar result on line {line_index}, got {other:?}"),
        }
    }

    fn integer(
        session: &mut CalculationPadSession,
        line_index: usize,
    ) -> crate::calculation_pad::TypedInteger {
        match &session.evaluation().unwrap().lines[line_index].outcome {
            LineOutcome::Value(CalculationValue::Integer(value)) => *value,
            other => panic!("expected an integer result on line {line_index}, got {other:?}"),
        }
    }

    fn formatted(session: &mut CalculationPadSession, line_index: usize) -> String {
        match &session.evaluation().unwrap().lines[line_index].outcome {
            LineOutcome::Value(value) => value
                .formatted_result()
                .expect("numeric result should have a display form"),
            other => panic!("expected a formatted result on line {line_index}, got {other:?}"),
        }
    }

    fn saved_path(session: &CalculationPadSession, id: DocumentId) -> PathBuf {
        session
            .store
            .directory()
            .join(format!("{:032x}.json", id.as_u128()))
    }

    #[test]
    fn blank_session_is_usable_without_creating_or_scanning_persistent_state() {
        let (_directory, _root, mut session) = fixture();

        assert_eq!(session.source(), Some(""));
        assert_eq!(session.line_ids().unwrap().len(), 1);
        assert!(!session.is_dirty());
        assert!(session.list_saved_documents().unwrap().is_empty());
        assert!(!session.store.directory().exists());
        assert!(session.evaluation().is_some());
        assert!(!session.store.directory().exists());
    }

    #[test]
    fn edits_keep_line_ids_stable_and_leave_physical_row_references_unchanged() {
        let (_directory, _root, mut session) = fixture();
        let source = "α = 2\nr2 + α\n{顧客 名} = α + 3";
        session.replace_source(source).unwrap();
        let original_ids = session.line_ids().unwrap().to_vec();

        session
            .replace_source("α = 2\ninserted = 9\nr2 + α\n{顧客 名} = α + 3")
            .unwrap();
        let inserted_ids = session.line_ids().unwrap().to_vec();
        assert_eq!(inserted_ids[0], original_ids[0]);
        assert_eq!(inserted_ids[2], original_ids[1]);
        assert_eq!(inserted_ids[3], original_ids[2]);
        assert!(!original_ids.contains(&inserted_ids[1]));
        assert!(session.source().unwrap().contains("r2 + α"));

        session
            .replace_line(original_ids[2], "{顧客 名} = α + 4")
            .unwrap();
        assert_eq!(session.line_id(3), Some(original_ids[2]));
        session
            .replace_source("α = 2\nr2 + α\n{顧客 名} = α + 4")
            .unwrap();
        assert_eq!(
            session.line_ids().unwrap(),
            &[original_ids[0], original_ids[1], original_ids[2]]
        );
        assert!(session.source().unwrap().contains("r2 + α"));
    }

    #[test]
    fn whole_source_single_line_edit_keeps_identity_when_it_duplicates_a_neighbor() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("A\nB").unwrap();
        let original_ids = session.line_ids().unwrap().to_vec();

        session.replace_source("B\nB").unwrap();

        assert_eq!(session.source(), Some("B\nB"));
        assert_eq!(session.line_ids(), Some(original_ids.as_slice()));
    }

    #[test]
    fn targeted_line_edits_keep_identity_when_contents_duplicate() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("A\nB").unwrap();
        let ids = session.line_ids().unwrap().to_vec();

        session.replace_line(ids[0], "B").unwrap();

        assert_eq!(session.source(), Some("B\nB"));
        assert_eq!(session.line_ids().unwrap().to_vec(), ids);
    }

    #[test]
    fn rejects_oversized_and_overlong_edits_before_line_diffing() {
        use crate::calculation_pad::{MAX_ADMITTED_DOCUMENT_BYTES as LIMIT_BYTES, MAX_ADMITTED_SOURCE_LINES as LIMIT_LINES};
        let (_directory, _root, mut session) = fixture();
        session.replace_source("existing = 1").unwrap();
        let original = session.source().unwrap().to_owned();

        assert!(matches!(
            session.replace_source("x".repeat(LIMIT_BYTES + 1)),
            Err(SessionError::DocumentTooLarge { .. })
        ));
        let too_many_lines = vec![""; LIMIT_LINES + 1].join("\n");
        assert!(matches!(
            session.replace_source(too_many_lines),
            Err(SessionError::TooManyLines { .. })
        ));
        assert!(matches!(
            super::remap_line_ids("", &[LineId(1)], &"x".repeat(LIMIT_BYTES + 1), 2),
            Err(SessionError::DocumentTooLarge { .. })
        ));
        let too_many_lines = vec![""; LIMIT_LINES + 1].join("\n");
        assert!(matches!(
            super::remap_line_ids("", &[LineId(1)], &too_many_lines, 2),
            Err(SessionError::TooManyLines { .. })
        ));
        assert_eq!(session.source(), Some(original.as_str()));
        assert_eq!(session.line_ids().unwrap().len(), 1);
    }

    #[test]
    fn extended_source_survives_profile_save_reopen_and_recovery() {
        let (_directory, root, mut session) = fixture();
        let source = format!("{}value = 2\r\nr257", "# α\r\n".repeat(256));
        session.replace_source(source.clone()).unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        assert!(matches!(session.evaluation().unwrap().diagnostic, Some(super::DocumentDiagnostic::TooManyLines { .. })));
        session.save_as("extended source").unwrap();
        let id = session.current_id().unwrap();
        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(id).unwrap();
        assert_eq!(reopened.source(), Some(source.as_str()));
        assert_eq!(reopened.line_ids().unwrap(), ids);
        let options = super::DocumentOptions { profile: super::super::DocumentProfile::Extended, ..Default::default() };
        reopened.set_document_options(options).unwrap();
        let result = reopened.evaluation().unwrap();
        assert!(result.diagnostic.is_none());
        assert!(matches!(result.lines[257].outcome, LineOutcome::Value(CalculationValue::Scalar(2.0))));
        let comparison = reopened.comparison_context().unwrap().compare(super::super::ComparisonRequest {
            left: super::super::ComparisonSlot::Expression("r257".into()),
            right: super::super::ComparisonSlot::Expression("value".into()),
        });
        assert!(matches!(comparison.relation, super::super::ComparisonRelation::Scalar { ordering: std::cmp::Ordering::Equal, .. }));
        let mut recovery = super::super::PadRecoveryWriter::new(super::super::PadRecoveryStore::new(&root));
        let snapshot = super::super::PadRecoverySnapshot { document_id: id, revision: 1, source: source.clone(), options };
        recovery.save_now(snapshot).unwrap();
        let loaded = recovery.load().unwrap().unwrap();
        assert_eq!(loaded.source, source);
        let mut restored = CalculationPadSession::new(&root);
        assert!(restored.restore_recovered_document(loaded.source, loaded.options).unwrap());
        assert_eq!(restored.source(), Some(source.as_str()));
    }

    #[test]
    fn absolute_admission_counts_utf8_bytes_and_trailing_physical_rows() {
        use crate::calculation_pad::{MAX_ADMITTED_DOCUMENT_BYTES as BYTES, MAX_ADMITTED_SOURCE_LINES as LINES};
        let (_directory, _root, mut session) = fixture();
        let exact_bytes = "é".repeat(BYTES / 2);
        session.replace_source(exact_bytes.clone()).unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        assert!(session.replace_source(format!("{exact_bytes}a")).is_err());
        assert_eq!(session.source(), Some(exact_bytes.as_str()));
        assert_eq!(session.line_ids().unwrap(), ids);
        let exact_rows = "\r\n".repeat(LINES - 1);
        session.replace_source(exact_rows.clone()).unwrap();
        assert_eq!(session.line_ids().unwrap().len(), LINES);
        assert!(session.replace_source(format!("{exact_rows}\n")).is_err());
        assert_eq!(session.source(), Some(exact_rows.as_str()));
    }

    #[test]
    fn dependency_snapshot_uses_cached_physical_rows_and_reuses_one_build() {
        use std::sync::Arc;
        let (_directory, _root, mut session) = fixture();
        let source = "# comment\n\nα = 2\n{顧客 名} = α + 1\nr3 + {顧客 名}\nα * 2\n[1, 2]\nlater + 1\nlater = 2\nself = self + 1\na = b\nb = a\nx = 1\nx = 2\nz = x + 1";
        session.replace_source(source).unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        let id = session.current_id().unwrap();
        let incarnation = session.document_incarnation().unwrap();
        assert!(session.dependency_snapshot().is_none());
        assert!(
            session
                .resolve_dependency_line(id, incarnation, ids[2])
                .is_none()
        );
        assert_eq!(session.evaluation_count(), 0);
        let evaluation = session.evaluation().unwrap().clone();
        let before = (
            session.source().unwrap().to_owned(),
            session.revision(),
            session.line_ids().unwrap().to_vec(),
            session.is_dirty(),
            session.can_undo(),
            session.can_redo(),
            session.evaluation_count(),
        );
        let graph = session.dependency_snapshot().unwrap();
        assert!(Arc::ptr_eq(&graph, &session.dependency_snapshot().unwrap()));
        assert_eq!(session.current.as_ref().unwrap().dependency_build_count, 1);
        assert_eq!(graph.rows.len(), ids.len());
        assert_eq!(graph.rows[0].outcome, LineOutcome::Comment);
        assert_eq!(graph.rows[1].outcome, LineOutcome::Blank);
        assert_eq!(graph.rows[3].assignment_name.as_deref(), Some("顧客 名"));
        assert_eq!(graph.rows[3].source, "{顧客 名} = α + 1");
        assert_eq!(graph.rows[3].dependencies, vec![ids[2]]);
        assert_eq!(graph.rows[4].dependencies, vec![ids[2], ids[3]]);
        assert_eq!(graph.upstream(ids[4]), Some(vec![ids[2], ids[3]]));
        assert_eq!(graph.downstream(ids[2]), Some(vec![ids[3], ids[4], ids[5]]));
        assert!(matches!(
            graph.rows[6].outcome,
            LineOutcome::Value(CalculationValue::Vector(_))
        ));
        for (row, original) in graph.rows.iter().zip(&evaluation.lines) {
            assert_eq!(row.outcome, original.outcome);
            let mut expected = original
                .dependency_indices
                .iter()
                .map(|&index| ids[index])
                .collect::<Vec<_>>();
            expected.sort_by_key(|id| ids.iter().position(|candidate| candidate == id));
            expected.dedup();
            assert_eq!(row.dependencies, expected);
        }
        assert!(matches!(graph.rows[7].outcome, LineOutcome::Error(_)));
        assert!(matches!(graph.rows[9].outcome, LineOutcome::Error(_)));
        assert!(matches!(graph.rows[10].outcome, LineOutcome::Error(_)));
        assert_eq!(
            (
                session.source().unwrap().to_owned(),
                session.revision(),
                session.line_ids().unwrap().to_vec(),
                session.is_dirty(),
                session.can_undo(),
                session.can_redo(),
                session.evaluation_count()
            ),
            before
        );
    }

    #[test]
    fn dependency_location_tracks_live_line_and_rejects_stale_incarnations() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("α = 2\nα + 1").unwrap();
        session.save_as("dependencies").unwrap();
        let document = session.current_id().unwrap();
        let incarnation = session.document_incarnation().unwrap();
        let line = session.line_id(1).unwrap();
        session.evaluation();
        let first = session.dependency_snapshot().unwrap();
        let location = session
            .resolve_dependency_line(document, incarnation, line)
            .unwrap();
        assert_eq!(&session.source().unwrap()[location.source_range], "α + 1");
        session.replace_source("# inserted\nα = 2\nα + 1").unwrap();
        assert!(session.dependency_snapshot().is_none());
        assert!(
            session
                .resolve_dependency_line(document, incarnation, line)
                .is_none()
        );
        assert_eq!(session.evaluation_count(), 1);
        session.evaluation();
        let second = session.dependency_snapshot().unwrap();
        assert!(!std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(session.current.as_ref().unwrap().dependency_build_count, 2);
        let moved = session
            .resolve_dependency_line(document, incarnation, line)
            .unwrap();
        assert_eq!(moved.line_index, 2);
        assert_eq!(&session.source().unwrap()[moved.source_range], "α + 1");
        session.replace_source("# inserted\nα = 2").unwrap();
        session.evaluation();
        assert!(
            session
                .resolve_dependency_line(document, incarnation, line)
                .is_none()
        );
        assert_eq!(
            session.request_open(document).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            session
                .resolve_pending(TransitionDecision::Discard)
                .unwrap(),
            TransitionOutcome::Applied
        );
        session.evaluation();
        assert_eq!(session.current_id(), Some(document));
        assert_ne!(session.document_incarnation(), Some(incarnation));
        assert!(
            session
                .resolve_dependency_line(document, incarnation, line)
                .is_none()
        );
    }

    #[test]
    fn evaluation_is_cached_until_the_source_revision_changes() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("x = 2\nx + 1").unwrap();
        let revision = session.revision().unwrap();

        assert_eq!(scalar(&mut session, 1), 3.0);
        assert_eq!(session.evaluation_count(), 1);
        assert_eq!(scalar(&mut session, 1), 3.0);
        assert_eq!(session.evaluation_count(), 1);
        assert_eq!(session.cached_evaluation_revision(), Some(revision));

        let assignment_id = session.line_id(0).unwrap();
        session.replace_line(assignment_id, "x = 4").unwrap();
        assert_eq!(session.revision(), Some(revision + 1));
        assert_eq!(scalar(&mut session, 1), 5.0);
        assert_eq!(session.evaluation_count(), 2);
    }

    #[test]
    fn exact_integer_modes_survive_session_cache_edit_undo_and_redo() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("mask = u8(40)\nmask + 1").unwrap();
        let ids = session.line_ids().unwrap().to_vec();

        let initial = integer(&mut session, 1);
        assert_eq!(initial.formatted_decimal(), "41");
        assert_eq!(initial.mode().type_name(), "u8");
        assert_eq!(session.evaluation_count(), 1);
        assert_eq!(integer(&mut session, 1), initial);
        assert_eq!(session.evaluation_count(), 1);

        session.replace_line(ids[0], "mask = u8(41)").unwrap();
        let edited = integer(&mut session, 1);
        assert_eq!(edited.formatted_decimal(), "42");
        assert_eq!(edited.mode().type_name(), "u8");
        assert_eq!(session.line_ids().unwrap().to_vec(), ids);
        assert_eq!(session.evaluation_count(), 2);

        assert!(session.undo());
        assert_eq!(integer(&mut session, 1), initial);
        assert_eq!(session.line_ids().unwrap().to_vec(), ids);
        assert_eq!(session.evaluation_count(), 3);

        assert!(session.redo());
        assert_eq!(integer(&mut session, 1), edited);
        assert_eq!(session.line_ids().unwrap().to_vec(), ids);
        assert_eq!(session.evaluation_count(), 4);
    }

    #[test]
    fn vector_values_follow_edits_history_save_reopen_and_inspector_rules() {
        use crate::calculation_pad::InspectorSourceOrigin;
        let (_directory, root, mut session) = fixture();
        session
            .replace_source("v = [1, 2, 3]\nr1 * 2\n9007199254740993 dec to hex\nu8(7)")
            .unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        assert_eq!(formatted(&mut session, 1), "[2, 4, 6]");
        assert!(session.inspector_source(ids[0]).is_none());
        assert!(session.inspector_source(ids[1]).is_none());
        assert_eq!(
            session.inspector_source(ids[2]).unwrap().origin,
            InspectorSourceOrigin::LegacyExactBase
        );
        assert_eq!(
            session.inspector_source(ids[3]).unwrap().origin,
            InspectorSourceOrigin::TypedInteger
        );
        session.replace_line(ids[0], "v = [3, 4, 5]").unwrap();
        assert_eq!(formatted(&mut session, 1), "[6, 8, 10]");
        assert!(session.undo());
        assert_eq!(formatted(&mut session, 1), "[2, 4, 6]");
        assert!(session.redo());
        assert_eq!(formatted(&mut session, 1), "[6, 8, 10]");
        session.save_as("vector history").unwrap();
        let id = session.current_id().unwrap();
        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(id).unwrap();
        assert_eq!(reopened.line_ids().unwrap(), ids);
        assert_eq!(formatted(&mut reopened, 1), "[6, 8, 10]");
    }

    #[test]
    fn collection_failure_retained_binding_survives_manual_history_and_reopen() {
        let (_directory, root, mut session) = fixture();
        session.set_document_options(super::DocumentOptions {
            calculation: super::super::CalculationPolicy::Manual,
            profile: super::super::DocumentProfile::Standard,
        }).unwrap();
        session.replace_source("v = [1]\nv = append(v, get([], 0))\nlen(v)").unwrap();
        assert!(session.shared_evaluation().is_none());
        session.request_calculate();
        let old = session.shared_evaluation().unwrap();
        assert_eq!(old.final_bindings["v"].line_index, 0);
        assert!(matches!(&old.lines[2].outcome, LineOutcome::Value(CalculationValue::Scalar(1.0))));
        let preview = session.preview_rename(0, "data").unwrap();
        assert!(session.apply_rename(&preview).unwrap());
        assert_eq!(session.evaluation_status(), super::EvaluationStatus::Outdated);
        assert!(session.shared_evaluation().is_none());
        assert!(session.undo());
        assert_eq!(old.final_bindings["v"].line_index, 0);
        session.request_calculate();
        assert_eq!(formatted(&mut session, 2), "1");
        session.save_as("collection retention").unwrap();
        let id = session.current_id().unwrap();
        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(id).unwrap();
        reopened.request_calculate();
        assert_eq!(formatted(&mut reopened, 2), "1");
        assert_eq!(reopened.shared_evaluation().unwrap().final_bindings["v"].line_index, 0);
    }

    #[test]
    fn dimensional_quantities_survive_session_cache_edit_undo_and_redo() {
        let (_directory, _root, mut session) = fixture();
        session
            .replace_source("area = (2 m)*(3 m)\narea to cm^2")
            .unwrap();
        let ids = session.line_ids().unwrap().to_vec();

        assert_eq!(formatted(&mut session, 0), "6 m²");
        assert_eq!(formatted(&mut session, 1), "60000 cm²");
        assert_eq!(session.evaluation_count(), 1);
        assert_eq!(formatted(&mut session, 1), "60000 cm²");
        assert_eq!(session.evaluation_count(), 1);

        session.replace_line(ids[0], "area = (3 m)*(3 m)").unwrap();
        assert_eq!(formatted(&mut session, 0), "9 m²");
        assert_eq!(formatted(&mut session, 1), "90000 cm²");
        assert_eq!(session.line_ids().unwrap(), ids);
        assert_eq!(session.evaluation_count(), 2);

        assert!(session.undo());
        assert_eq!(formatted(&mut session, 0), "6 m²");
        assert_eq!(formatted(&mut session, 1), "60000 cm²");
        assert_eq!(session.line_ids().unwrap(), ids);
        assert_eq!(session.evaluation_count(), 3);

        assert!(session.redo());
        assert_eq!(formatted(&mut session, 0), "9 m²");
        assert_eq!(formatted(&mut session, 1), "90000 cm²");
        assert_eq!(session.line_ids().unwrap(), ids);
        assert_eq!(session.evaluation_count(), 4);
    }

    #[test]
    fn recovered_source_is_an_exact_dirty_draft_with_fresh_line_ids() {
        let (_directory, _root, mut session) = fixture();
        let source = "α = 2\r\n{顧客 名} = α + 3\r\n";

        assert!(session.restore_recovered_source(source.to_owned()).unwrap());

        assert_eq!(session.source(), Some(source));
        assert!(session.is_dirty());
        assert!(!session.is_saved());
        assert!(!session.can_undo());
        let line_ids = session.line_ids().unwrap();
        assert_eq!(line_ids.len(), 3);
        let unique_ids = line_ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique_ids.len(), line_ids.len());
        assert_eq!(session.evaluation_count(), 0);
        session.evaluation().unwrap();
        assert_eq!(session.evaluation_count(), 1);
    }

    #[test]
    fn recovery_cannot_replace_a_live_draft_undone_to_blank() {
        let (_directory, _root, mut session) = fixture();
        let original_id = session.current_id();
        session.replace_source("temporary = 1").unwrap();
        assert!(session.undo());
        assert_eq!(session.source(), Some(""));
        assert!(session.can_redo());

        assert!(
            !session
                .restore_recovered_source("older draft = 9".to_owned())
                .unwrap()
        );

        assert_eq!(session.source(), Some(""));
        assert_eq!(session.current_id(), original_id);
        assert!(session.can_redo());
    }

    #[test]
    fn undo_and_redo_recompute_dependent_lines_and_restore_stable_ids() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("x = 2\nx + 1").unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        assert_eq!(scalar(&mut session, 1), 3.0);

        session.replace_line(ids[0], "x = 4").unwrap();
        assert_eq!(scalar(&mut session, 1), 5.0);
        assert_eq!(session.line_ids().unwrap().to_vec(), ids);
        assert!(session.undo());
        assert_eq!(session.source(), Some("x = 2\nx + 1"));
        assert_eq!(session.line_ids().unwrap().to_vec(), ids);
        assert_eq!(scalar(&mut session, 1), 3.0);
        assert!(session.redo());
        assert_eq!(session.source(), Some("x = 4\nx + 1"));
        assert_eq!(session.line_ids().unwrap().to_vec(), ids);
        assert_eq!(scalar(&mut session, 1), 5.0);
    }

    #[test]
    fn undo_history_is_bounded() {
        let (_directory, _root, mut session) = fixture();
        for value in 0..(MAX_UNDO_ENTRIES + 12) {
            session.replace_source(format!("value = {value}")).unwrap();
        }

        let mut undo_entries = 0;
        while session.undo() {
            undo_entries += 1;
        }
        assert_eq!(undo_entries, MAX_UNDO_ENTRIES);
        assert!(!session.can_undo());

        let (_directory, _root, mut large_edits) = fixture();
        for value in 0..24 {
            large_edits
                .replace_source(format!("{value} {}", "x".repeat(MAX_DOCUMENT_BYTES - 16)))
                .unwrap();
        }
        assert!(large_edits.history.total_bytes() <= super::MAX_UNDO_BYTES);
        assert!(large_edits.history.undo.len() < MAX_UNDO_ENTRIES);
    }

    #[test]
    fn view_metadata_is_dirty_without_source_edits_and_survives_save_as() {
        let (_directory, root, mut session) = fixture();
        session.replace_source("x = 2\nx = 3").unwrap();
        session.save_as("view original").unwrap();
        let id = session.current_id().unwrap();
        let line = session.line_id(0).unwrap();
        let revision = session.revision();
        let view = super::super::persistence::PadViewMetadata {
            inputs: vec![super::super::persistence::PadInputReference { id: line, name: "x".into(), unavailable: false, config: Default::default() }],
            watches: vec!["x".into(), "missing".into()],
            ..Default::default()
        };
        session.set_view_metadata(view.clone()).unwrap();
        assert!(session.is_dirty());
        assert_eq!(session.revision(), revision);
        assert_eq!(session.source(), Some("x = 2\nx = 3"));
        session.save().unwrap();
        assert!(!session.is_dirty());
        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(id).unwrap();
        assert_eq!(reopened.view_metadata(), Some(&view));
        reopened.save_as("view copy").unwrap();
        let copy = reopened.current_id().unwrap();
        let mut copy_session = CalculationPadSession::new(&root);
        copy_session.request_open(copy).unwrap();
        assert_eq!(copy_session.view_metadata(), Some(&view));
        copy_session.request_new().unwrap();
        assert_eq!(copy_session.view_metadata(), Some(&Default::default()));
    }

    #[test]
    fn save_and_reopen_round_trip_unicode_source_and_line_ids() {
        let (_directory, root, mut session) = fixture();
        let source = "π = 2\n{顧客 名} = π + 1\n";
        session.replace_source(source).unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        session.save_as("計算 draft").unwrap();
        let id = session.current_id().unwrap();
        assert!(!session.is_dirty());

        let mut reopened = CalculationPadSession::new(&root);
        assert_eq!(
            reopened.request_open(id).unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(reopened.source(), Some(source));
        assert_eq!(reopened.line_ids().unwrap().to_vec(), ids);
        assert_eq!(reopened.current_name(), Some("計算 draft"));
    }

    #[test]
    fn saving_an_untitled_blank_document_requires_save_as() {
        let (_directory, _root, mut session) = fixture();
        assert_eq!(session.source(), Some(""));
        assert_eq!(session.current_name(), None);

        assert!(matches!(session.save(), Err(SessionError::SaveAsRequired)));
        assert!(session.list_saved_documents().unwrap().is_empty());
    }

    #[test]
    fn save_as_identity_is_not_reverted_by_undo_and_redo() {
        let (_directory, root, mut session) = fixture();
        session.replace_source("x = 1").unwrap();
        session.save_as("undo identity").unwrap();
        let id = session.current_id().unwrap();
        let line_id = session.line_id(0).unwrap();

        session.replace_line(line_id, "x = 2").unwrap();
        assert!(session.undo());
        assert_eq!(session.source(), Some("x = 1"));
        assert_eq!(session.current_id(), Some(id));
        assert_eq!(session.current_name(), Some("undo identity"));
        assert!(!session.is_dirty());
        assert!(session.redo());
        assert_eq!(session.current_id(), Some(id));
        assert_eq!(session.current_name(), Some("undo identity"));
        session.save().unwrap();

        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(id).unwrap();
        assert_eq!(reopened.source(), Some("x = 2"));
    }

    #[test]
    fn persisted_line_id_allocator_does_not_reuse_removed_identities() {
        let (_directory, root, mut session) = fixture();
        session.save_as("line identity counter").unwrap();
        let saved_id = session.current_id().unwrap();

        session.replace_source("first\nsecond").unwrap();
        let removed_id = session.line_id(1).unwrap();
        session.replace_source("").unwrap();
        assert!(session.is_dirty());
        session.replace_source("first\nsecond").unwrap();
        let replacement_id = session.line_id(1).unwrap();
        assert_ne!(replacement_id, removed_id);
        assert!(replacement_id.get() > removed_id.get());
        session.save().unwrap();

        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(saved_id).unwrap();
        reopened.replace_source("first\ninserted\nsecond").unwrap();
        assert_eq!(reopened.line_id(1).unwrap().get(), replacement_id.get() + 1);
    }

    #[test]
    fn failed_open_preserves_the_draft_and_open_transition_can_cancel_or_discard() {
        let (_directory, root, mut owner) = fixture();
        owner.replace_source("target = 7\ntarget * 2").unwrap();
        owner.save_as("target").unwrap();
        let target_id = owner.current_id().unwrap();
        let target_path = saved_path(&owner, target_id);
        let target_bytes = std::fs::read(&target_path).unwrap();

        let mut draft = CalculationPadSession::new(&root);
        draft.replace_source("local draft = 9").unwrap();
        let draft_source = draft.source().unwrap().to_owned();
        std::fs::write(&target_path, b"not valid JSON").unwrap();
        assert!(matches!(
            draft.request_open(target_id),
            Err(SessionError::Store(PadStoreError::MalformedJson { .. }))
        ));
        assert_eq!(draft.source(), Some(draft_source.as_str()));
        assert_eq!(draft.pending_transition(), None);

        std::fs::write(&target_path, target_bytes).unwrap();
        assert_eq!(
            draft.request_open(target_id).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            draft.pending_transition(),
            Some(PendingTransition::Open(target_id))
        );
        assert_eq!(
            draft.resolve_pending(TransitionDecision::Cancel).unwrap(),
            TransitionOutcome::Cancelled
        );
        assert_eq!(draft.source(), Some(draft_source.as_str()));

        assert_eq!(
            draft.request_open(target_id).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert!(matches!(
            draft.resolve_pending(TransitionDecision::Save),
            Err(SessionError::SaveAsRequired)
        ));
        assert_eq!(
            draft.pending_transition(),
            Some(PendingTransition::Open(target_id))
        );
        draft.save_as("saved draft").unwrap();
        assert_eq!(
            draft.resolve_pending(TransitionDecision::Save).unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(draft.source(), Some("target = 7\ntarget * 2"));
        assert_eq!(draft.current_id(), Some(target_id));

        draft.replace_source("local target edit").unwrap();
        assert_eq!(
            draft.request_open(target_id).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            draft.resolve_pending(TransitionDecision::Discard).unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(draft.source(), Some("target = 7\ntarget * 2"));
    }

    #[test]
    fn opening_the_same_saved_document_after_save_reloads_its_latest_source() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("answer = 1").unwrap();
        session.save_as("same document").unwrap();
        let id = session.current_id().unwrap();

        let answer_line = session.line_id(0).unwrap();
        session.replace_line(answer_line, "answer = 2").unwrap();
        assert_eq!(
            session.request_open(id).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            session.pending_transition(),
            Some(PendingTransition::Open(id))
        );
        assert_eq!(
            session.resolve_pending(TransitionDecision::Save).unwrap(),
            TransitionOutcome::Applied
        );

        assert_eq!(session.current_id(), Some(id));
        assert_eq!(session.source(), Some("answer = 2"));
        assert!(!session.is_dirty());
        let loaded = session.store.open(id).unwrap();
        assert_eq!(loaded.stored.source, "answer = 2");
        assert_eq!(
            session.current.as_ref().unwrap().baseline.disk_sha256,
            Some(loaded.disk_sha256)
        );
    }

    #[test]
    fn pending_open_reloads_after_save_and_preserves_session_on_target_error() {
        let (_directory, root, mut current) = fixture();
        current.replace_source("current = 1").unwrap();
        current.save_as("current").unwrap();
        let current_id = current.current_id().unwrap();

        let mut target = CalculationPadSession::new(&root);
        target.replace_source("target = 2").unwrap();
        target.save_as("target").unwrap();
        let target_id = target.current_id().unwrap();
        let target_path = saved_path(&current, target_id);
        let valid_target_bytes = std::fs::read(&target_path).unwrap();

        let current_line = current.line_id(0).unwrap();
        current.replace_line(current_line, "current = 3").unwrap();
        assert_eq!(
            current.request_open(target_id).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        std::fs::write(&target_path, b"invalid after the prompt").unwrap();

        assert!(matches!(
            current.resolve_pending(TransitionDecision::Save),
            Err(SessionError::Store(PadStoreError::MalformedJson { .. }))
        ));
        assert_eq!(current.current_id(), Some(current_id));
        assert_eq!(current.current_name(), Some("current"));
        assert_eq!(current.source(), Some("current = 3"));
        assert!(!current.is_dirty());
        assert_eq!(
            current.pending_transition(),
            Some(PendingTransition::Open(target_id))
        );
        assert_eq!(
            current.store.open(current_id).unwrap().stored.source,
            "current = 3"
        );

        std::fs::write(&target_path, valid_target_bytes).unwrap();
        assert_eq!(
            current
                .resolve_pending(TransitionDecision::Discard)
                .unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(current.current_id(), Some(target_id));
        assert_eq!(current.source(), Some("target = 2"));
    }

    #[test]
    fn recovery_scratch_close_rejects_saved_or_pending_documents() {
        let (_dir, _root, mut session) = fixture();
        session.replace_source("scratch").unwrap();
        session.request_new().unwrap();
        assert!(matches!(session.close_recovered_scratch(), Err(SessionError::PendingTransitionExists)));
        session.resolve_pending(TransitionDecision::Cancel).unwrap();
        session.save_as("bound").unwrap();
        assert!(matches!(session.close_recovered_scratch(), Err(SessionError::RecoveryScratchRequired)));
        assert_eq!(session.source(), Some("scratch"));
        session.request_new().unwrap();
        session.replace_source("recovered").unwrap();
        session.close_recovered_scratch().unwrap();
        assert_eq!(session.source(), None);
    }

    #[test]
    fn new_and_close_transitions_handle_save_discard_and_cancel() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("unsaved = 5").unwrap();
        assert_eq!(
            session.request_new().unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert!(matches!(
            session.resolve_pending(TransitionDecision::Save),
            Err(SessionError::SaveAsRequired)
        ));
        assert_eq!(session.pending_transition(), Some(PendingTransition::New));
        session.save_as("kept on Save").unwrap();
        assert_eq!(
            session.resolve_pending(TransitionDecision::Save).unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(session.source(), Some(""));
        assert_eq!(
            session.list_saved_documents().unwrap()[0].name,
            "kept on Save"
        );

        session.replace_source("discard me").unwrap();
        assert_eq!(
            session.request_close().unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            session.resolve_pending(TransitionDecision::Cancel).unwrap(),
            TransitionOutcome::Cancelled
        );
        assert_eq!(session.source(), Some("discard me"));
        assert_eq!(
            session.request_close().unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            session
                .resolve_pending(TransitionDecision::Discard)
                .unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(session.source(), None);

        session.request_new().unwrap();
        session.replace_source("save on close").unwrap();
        session.save_as("close-save").unwrap();
        session.replace_source("saved close").unwrap();
        let close_id = session.current_id().unwrap();
        assert_eq!(
            session.request_close().unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            session.resolve_pending(TransitionDecision::Save).unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(session.source(), None);
        assert_eq!(
            session.store.open(close_id).unwrap().stored.source,
            "saved close"
        );
        assert_eq!(
            session
                .list_saved_documents()
                .unwrap()
                .iter()
                .find(|document| document.id == close_id)
                .unwrap()
                .source_bytes,
            "saved close".len()
        );
    }

    #[test]
    fn external_changes_and_deletion_keep_the_local_draft_dirty() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("baseline").unwrap();
        session.save_as("external-check").unwrap();
        session.replace_source("local draft").unwrap();
        let id = session.current_id().unwrap();
        let path = saved_path(&session, id);
        let baseline = session.current.as_ref().unwrap().baseline.disk_sha256;
        let name = session.current_name().unwrap().to_owned();

        let mut external = std::fs::read(&path).unwrap();
        external.push(b' ');
        std::fs::write(&path, external).unwrap();
        assert_eq!(
            session.request_close().unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert!(matches!(
            session.resolve_pending(TransitionDecision::Save),
            Err(SessionError::Store(PadStoreError::DiskConflict(found))) if found == id
        ));
        assert_eq!(session.pending_transition(), Some(PendingTransition::Close));
        assert_eq!(session.source(), Some("local draft"));
        assert_eq!(session.current_name(), Some(name.as_str()));
        assert_eq!(
            session.current.as_ref().unwrap().baseline.disk_sha256,
            baseline
        );
        assert!(session.is_dirty());

        std::fs::remove_file(path).unwrap();
        assert!(matches!(
            session.resolve_pending(TransitionDecision::Save),
            Err(SessionError::Store(PadStoreError::DiskConflict(found))) if found == id
        ));
        assert_eq!(session.pending_transition(), Some(PendingTransition::Close));
        assert_eq!(session.source(), Some("local draft"));
        assert!(session.is_dirty());
    }

    #[test]
    fn failed_atomic_write_keeps_file_baseline_and_dirty_draft_unchanged() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("before").unwrap();
        session.save_as("write-failure").unwrap();
        session.replace_source("after").unwrap();
        let id = session.current_id().unwrap();
        let path = saved_path(&session, id);
        let original_bytes = std::fs::read(&path).unwrap();
        let baseline = session.current.as_ref().unwrap().baseline.disk_sha256;
        let name = session.current_name().unwrap().to_owned();

        assert!(matches!(
            session.save_with_writer(|_, _| anyhow::bail!("injected write failure")),
            Err(SessionError::Store(PadStoreError::AtomicWrite { .. }))
        ));
        assert_eq!(std::fs::read(path).unwrap(), original_bytes);
        assert_eq!(session.current_id(), Some(id));
        assert_eq!(session.current_name(), Some(name.as_str()));
        assert_eq!(session.source(), Some("after"));
        assert_eq!(
            session.current.as_ref().unwrap().baseline.disk_sha256,
            baseline
        );
        assert!(session.is_dirty());
    }

    #[test]
    fn failed_or_invalid_rows_remain_owned_by_the_source_order() {
        let (_directory, _root, mut session) = fixture();
        session
            .replace_source("good = 2\nbad = missing\ngood + 1")
            .unwrap();
        let first_line_id = session.line_id(0).unwrap();
        let evaluation = session.evaluation().unwrap();
        assert_eq!(evaluation.lines.len(), 3);
        assert_eq!(
            evaluation
                .lines
                .iter()
                .map(|line| line.line_index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(evaluation.lines[0].id, first_line_id);
        assert!(matches!(
            &evaluation.lines[1].outcome,
            LineOutcome::Error(diagnostic) if matches!(&diagnostic.kind, DiagnosticKind::UnknownName(_))
        ));
    }
}
