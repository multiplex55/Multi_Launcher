//! Compact, versioned wire representation for bounded Gate C evidence.
//!
//! The in-memory evidence types remain the validation model. This module only
//! interns repeated editor identities and encodes high-volume records as typed
//! positional tuples before the report is persisted.

use super::*;
use serde::de::{IgnoredAny, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;

const GATE_C_WIRE_VERSION: u8 = 2;
pub(super) const MAX_GATE_C_IDENTITY_DICTIONARY: usize = 512;
const MAX_GATE_C_INSPECTOR_EDITS: usize = 16;
const MAX_GATE_C_AUTHORING_STATES: usize = 4;
const MAX_GATE_C_DESIGNER_CONTROLS: usize = 8;
const MAX_GATE_C_DESIGNER_CLOSES: usize = 4;
const MAX_GATE_C_INITIAL_SNAPSHOTS: usize = 4;
const MAX_GATE_C_BINDING_CONTROLS: usize = 8;

#[derive(Clone, Debug)]
struct BoundedVec<T, const LIMIT: usize>(Vec<T>);

impl<T, const LIMIT: usize> BoundedVec<T, LIMIT> {
    fn try_from_vec(values: Vec<T>) -> Result<Self, String> {
        if values.len() > LIMIT {
            return Err(format!(
                "wire vector has {} entries; maximum is {LIMIT}",
                values.len()
            ));
        }
        Ok(Self(values))
    }

    fn into_vec(self) -> Vec<T> {
        self.0
    }
}

impl<T: Serialize, const LIMIT: usize> Serialize for BoundedVec<T, LIMIT> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if self.0.len() > LIMIT {
            return Err(serde::ser::Error::custom(format!(
                "wire vector has {} entries; maximum is {LIMIT}",
                self.0.len()
            )));
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for value in &self.0 {
            sequence.serialize_element(value)?;
        }
        sequence.end()
    }
}

impl<'de, T: Deserialize<'de>, const LIMIT: usize> Deserialize<'de> for BoundedVec<T, LIMIT> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct BoundedVecVisitor<T, const LIMIT: usize>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>, const LIMIT: usize> Visitor<'de> for BoundedVecVisitor<T, LIMIT> {
            type Value = BoundedVec<T, LIMIT>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "a sequence with at most {LIMIT} entries")
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                if sequence.size_hint().is_some_and(|hint| hint > LIMIT) {
                    return Err(serde::de::Error::custom(format!(
                        "wire sequence exceeds its {LIMIT}-entry bound"
                    )));
                }
                let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(LIMIT));
                loop {
                    if values.len() == LIMIT {
                        if sequence.next_element::<IgnoredAny>()?.is_some() {
                            return Err(serde::de::Error::custom(format!(
                                "wire sequence exceeds its {LIMIT}-entry bound"
                            )));
                        }
                        break;
                    }
                    match sequence.next_element()? {
                        Some(value) => values.push(value),
                        None => break,
                    }
                }
                Ok(BoundedVec(values))
            }
        }

        deserializer.deserialize_seq(BoundedVecVisitor::<T, LIMIT>(PhantomData))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GateCIdentityWire(
    GateCSurface,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
);

impl From<&GateCEditorIdentity> for GateCIdentityWire {
    fn from(identity: &GateCEditorIdentity) -> Self {
        Self(
            identity.surface,
            identity.session_id,
            identity.draft_generation,
            identity.stable_target_digest,
            identity.editor_epoch,
            identity.edit_generation,
            identity.query_generation,
            identity.query_request_generation,
            identity.search_request_generation,
            identity.test_request_generation,
            identity.query_digest,
            identity.binding_digest,
        )
    }
}

impl From<GateCIdentityWire> for GateCEditorIdentity {
    fn from(identity: GateCIdentityWire) -> Self {
        Self {
            surface: identity.0,
            session_id: identity.1,
            draft_generation: identity.2,
            stable_target_digest: identity.3,
            editor_epoch: identity.4,
            edit_generation: identity.5,
            query_generation: identity.6,
            query_request_generation: identity.7,
            search_request_generation: identity.8,
            test_request_generation: identity.9,
            query_digest: identity.10,
            binding_digest: identity.11,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GateCControlWire(
    String,
    u64,
    u16,
    (Option<u64>, Option<u64>, Option<usize>),
    [u64; 9],
    ([i32; 4], [i32; 4], [i32; 2]),
    [bool; 8],
    Option<u64>,
);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GateCResultRowWire(
    GateCSurface,
    u16,
    u64,
    u64,
    usize,
    [u64; 7],
    bool,
    ([i32; 4], [i32; 4], [i32; 2]),
    [bool; 2],
);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GateCProviderWire(
    u64,
    GateCProviderEdge,
    GateCProviderKind,
    u16,
    u64,
    u64,
    Option<u64>,
);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GateCAuthoringStateWire(
    u64,
    Option<u64>,
    (u64, u64, u64, u64),
    (usize, usize, u64),
    (usize, u64),
    bool,
    (u64, u64, u64, u64, u64, u64),
    Option<u64>,
    (bool, bool),
    (usize, usize),
    bool,
    Option<u16>,
    Option<u64>,
    (bool, bool),
);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GateCBindingControlWire(
    GateCBindingControlKind,
    String,
    u64,
    GateCSurface,
    u16,
    Option<u64>,
    Option<u64>,
    u64,
    u64,
);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GateCSearchWire(
    GateCSearchPurpose,
    GateCSurface,
    u16,
    u64,
    u64,
    (u64, u64, u64, u64, u64),
    usize,
    u64,
    u64,
    usize,
    bool,
);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GateCIncompleteSearchWire(
    u16,
    u64,
    u64,
    u64,
    u64,
    BoundedVec<u64, MAX_GATE_C_LIFECYCLE_EDGES>,
    GateCSearchIncompleteReason,
);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GateCQ14OperationWire(
    GateCQ14OperationKind,
    GateCInputMethod,
    u64,
    u64,
    u16,
    u64,
    u64,
);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GateCInsertionControlWire(
    GateCInsertionControlKind,
    GateCInsertionWidgetPart,
    u64,
    u64,
    u64,
    [bool; 3],
    ([i32; 4], [i32; 4], [i32; 2]),
    [bool; 2],
    [u64; 3],
    (u64, u64),
);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GateCInsertionObservationReceiptWire(
    GateCAuthoringStateWire,
    GateCObservationBoundaryEvidence,
);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GateCBindingWire {
    #[serde(rename = "r")]
    route: GateCBindingRoute,
    #[serde(rename = "d")]
    destination_cell_digest: u64,
    #[serde(rename = "t")]
    semantic_target_digest: u64,
    #[serde(rename = "a")]
    semantic_action_digest: u64,
    #[serde(rename = "k")]
    kind: GateCBindingKind,
    #[serde(rename = "ai")]
    action_id_digest: Option<u64>,
    #[serde(rename = "tr")]
    target_reference_digest: Option<u64>,
    #[serde(rename = "q")]
    query_digest: Option<u64>,
    #[serde(rename = "qm")]
    query_mode: Option<GateCQueryMode>,
    #[serde(rename = "c")]
    command_digest: Option<u64>,
    #[serde(rename = "ar")]
    arguments_digest: Option<u64>,
    #[serde(rename = "eb")]
    expected_binding_digest: u64,
    #[serde(rename = "pb")]
    previous_binding_digest: u64,
    #[serde(rename = "db")]
    document_before_digest: u64,
    #[serde(rename = "ds")]
    document_staged_digest: u64,
    #[serde(rename = "hb")]
    history_before_digest: u64,
    #[serde(rename = "hs")]
    history_staged_digest: u64,
    #[serde(rename = "da")]
    document_applied_digest: u64,
    #[serde(rename = "ha")]
    history_applied_digest: u64,
    #[serde(rename = "sd")]
    saved_document_digest: u64,
    #[serde(rename = "rd")]
    reopened_document_digest: u64,
    #[serde(rename = "save")]
    radial_save: Option<GateCRadialSaveEvidence>,
    #[serde(rename = "rb")]
    reopened_binding_digest: u64,
    #[serde(rename = "ib")]
    inspector_binding_digest: u64,
    #[serde(rename = "ud")]
    undo_document_digest: u64,
    #[serde(rename = "dd")]
    redo_document_digest: u64,
    #[serde(rename = "udep")]
    undo_depth_before: usize,
    #[serde(rename = "uafter")]
    undo_depth_after: usize,
    #[serde(rename = "mc")]
    mutation_count: usize,
    #[serde(rename = "cc")]
    controls: BoundedVec<GateCBindingControlWire, MAX_GATE_C_BINDING_CONTROLS>,
    #[serde(rename = "p")]
    after_action_policy: GateCAfterActionPolicy,
    #[serde(rename = "sr")]
    saved_after_reopen: bool,
    #[serde(rename = "im")]
    inspector_matches_saved_binding: bool,
    #[serde(rename = "ur")]
    undo_restores_before: bool,
    #[serde(rename = "dr")]
    redo_restores_after: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GateCInsertionWire {
    #[serde(rename = "l")]
    layout: GateCResultLayout,
    #[serde(rename = "o")]
    session_origin: GateCInsertionSessionOrigin,
    #[serde(rename = "r")]
    request_id: u64,
    #[serde(rename = "st")]
    source_target_digest: u64,
    #[serde(rename = "sa")]
    source_action_digest: u64,
    #[serde(rename = "sb")]
    source_binding_digest: u64,
    #[serde(rename = "sq")]
    source_query_digest: u64,
    #[serde(rename = "srq")]
    source_root_query_digest: u64,
    #[serde(rename = "grid")]
    source_results_grid_layout: bool,
    #[serde(rename = "pb")]
    prior_binding_digest: u64,
    #[serde(rename = "act")]
    source_activation_delta: usize,
    #[serde(rename = "disp")]
    source_dispatch_delta: usize,
    #[serde(rename = "exec")]
    source_action_execution_delta: usize,
    #[serde(rename = "hb")]
    source_history_digest_before: u64,
    #[serde(rename = "ha")]
    source_history_digest_after: u64,
    #[serde(rename = "ub")]
    source_usage_digest_before: u64,
    #[serde(rename = "ua")]
    source_usage_digest_after: u64,
    #[serde(rename = "mb")]
    source_marker_digest_before: u64,
    #[serde(rename = "ma")]
    source_marker_digest_after: u64,
    #[serde(rename = "c")]
    controls: BoundedVec<GateCInsertionControlWire, MAX_GATE_C_INSERTION_CONTROLS>,
    #[serde(rename = "ready")]
    initial_snapshot_ready: bool,
    #[serde(rename = "isr")]
    initial_snapshot_receipt: GateCInitialSnapshotReceipt,
    #[serde(rename = "base")]
    source_baseline_receipt: GateCInsertionObservationReceiptWire,
    #[serde(rename = "owner")]
    source_owner_receipt: GateCInsertionObservationReceiptWire,
    #[serde(rename = "out")]
    outcome_receipt: GateCInsertionObservationReceiptWire,
    #[serde(rename = "bseq")]
    source_baseline_observation_sequence: u64,
    #[serde(rename = "bbseq")]
    source_baseline_boundary_sequence: u64,
    #[serde(rename = "bsid")]
    source_baseline_session_id: u64,
    #[serde(rename = "bgen")]
    source_baseline_generation: u64,
    #[serde(rename = "bdoc")]
    source_baseline_document_digest: u64,
    #[serde(rename = "isid")]
    initial_snapshot_session_id: u64,
    #[serde(rename = "iseq")]
    initial_snapshot_sequence: u64,
    #[serde(rename = "oseq")]
    source_owner_observation_sequence: u64,
    #[serde(rename = "obseq")]
    source_owner_boundary_sequence: u64,
    #[serde(rename = "osid")]
    source_owner_session_id: u64,
    #[serde(rename = "ogen")]
    source_owner_generation: u64,
    #[serde(rename = "odoc")]
    source_owner_document_digest: u64,
    #[serde(rename = "xseq")]
    outcome_observation_sequence: u64,
    #[serde(rename = "xbseq")]
    outcome_boundary_sequence: u64,
    #[serde(rename = "xsid")]
    outcome_session_id: u64,
    #[serde(rename = "xgen")]
    outcome_generation: u64,
    #[serde(rename = "xdoc")]
    outcome_document_digest: u64,
    #[serde(rename = "xud")]
    outcome_undo_depth: usize,
    #[serde(rename = "xrd")]
    outcome_redo_depth: usize,
    #[serde(rename = "xdd")]
    outcome_draft_dirty: bool,
    #[serde(rename = "pc")]
    policy_confirmation_required: bool,
    #[serde(rename = "dm")]
    destination_menu_digest: u64,
    #[serde(rename = "dr")]
    destination_ring_digest: u64,
    #[serde(rename = "dc")]
    destination_cell_digest: u64,
    #[serde(rename = "outcome")]
    outcome: GateCInsertionOutcome,
    #[serde(rename = "doc")]
    documents: [u64; 4],
    #[serde(rename = "edits")]
    unrelated_edit_digests: [u64; 4],
    #[serde(rename = "unrelated")]
    unrelated_cell_target_digest: u64,
    #[serde(rename = "label")]
    unrelated_label_value_digest: u64,
    #[serde(rename = "udepths")]
    undo_depths: [usize; 4],
    #[serde(rename = "rdepths")]
    redo_depths: [usize; 4],
    #[serde(rename = "dirty")]
    dirty_states: [bool; 4],
    #[serde(rename = "history")]
    radial_history_digests: [u64; 4],
    #[serde(rename = "preserved")]
    unrelated_dirty_edit_preserved: bool,
    #[serde(rename = "binding_matches")]
    saved_typed_binding_matches: bool,
    #[serde(rename = "saved")]
    saved_binding: Option<GateCBindingWire>,
    #[serde(rename = "undo")]
    one_undo_restores_before: bool,
    #[serde(rename = "redo")]
    one_redo_restores_after: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GateCQ14Wire {
    #[serde(rename = "b")]
    baseline: GateCAuthoringStateWire,
    #[serde(rename = "t")]
    terminal: GateCAuthoringStateWire,
    #[serde(rename = "mb")]
    marker_entries_before: usize,
    #[serde(rename = "ma")]
    marker_entries_after: usize,
    #[serde(rename = "md")]
    marker_digests: [u64; 2],
    #[serde(rename = "nd")]
    normal_dispatches: usize,
    #[serde(rename = "ud")]
    universal_action_dispatches: usize,
    #[serde(rename = "rd")]
    radial_action_dispatches: usize,
    #[serde(rename = "at")]
    authoring_tests: usize,
    #[serde(rename = "op")]
    operations: BoundedVec<GateCQ14OperationWire, MAX_GATE_C_Q14_OPERATIONS>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GateCQ14PartialWire {
    #[serde(rename = "b")]
    baseline: GateCAuthoringStateWire,
    #[serde(rename = "t")]
    terminal: Option<GateCAuthoringStateWire>,
    #[serde(rename = "op")]
    operations: BoundedVec<GateCQ14OperationWire, MAX_GATE_C_Q14_OPERATIONS>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GateCCaseBodyWire {
    #[serde(rename = "sv")]
    schema_version: u16,
    #[serde(rename = "id")]
    case_id: String,
    #[serde(rename = "fd")]
    fixture_digest: String,
    #[serde(rename = "sid")]
    session_id: u64,
    #[serde(rename = "gen")]
    draft_generation: u64,
    #[serde(rename = "surf")]
    editor_surface: Option<GateCSurface>,
    #[serde(rename = "ei")]
    editor_identity: Option<u16>,
    #[serde(rename = "c")]
    controls: BoundedVec<GateCControlWire, MAX_GATE_C_CONTROLS>,
    #[serde(rename = "rows")]
    ordered_results: BoundedVec<GateCResultRowWire, MAX_GATE_C_RESULTS>,
    #[serde(rename = "edits")]
    inspector_text_edits: BoundedVec<GateCInspectorTextEvidence, MAX_GATE_C_INSPECTOR_EDITS>,
    #[serde(rename = "search")]
    searches: BoundedVec<GateCSearchWire, MAX_GATE_C_SEARCHES>,
    #[serde(rename = "incomplete")]
    incomplete_searches: BoundedVec<GateCIncompleteSearchWire, MAX_GATE_C_SEARCHES>,
    #[serde(rename = "life")]
    provider_lifecycle: BoundedVec<GateCProviderWire, MAX_GATE_C_LIFECYCLE_EDGES>,
    #[serde(rename = "state")]
    authoring_states: BoundedVec<GateCAuthoringStateWire, MAX_GATE_C_AUTHORING_STATES>,
    #[serde(rename = "boundaries")]
    observation_boundaries:
        BoundedVec<GateCObservationBoundaryEvidence, MAX_GATE_C_OBSERVATION_BOUNDARIES>,
    #[serde(rename = "designer_controls")]
    designer_controls: BoundedVec<GateCDesignerControlEvidence, MAX_GATE_C_DESIGNER_CONTROLS>,
    #[serde(rename = "designer_closes")]
    designer_closes: BoundedVec<GateCDesignerCloseEvidence, MAX_GATE_C_DESIGNER_CLOSES>,
    #[serde(rename = "snapshots")]
    initial_snapshots: BoundedVec<GateCInitialSnapshotReceipt, MAX_GATE_C_INITIAL_SNAPSHOTS>,
    #[serde(rename = "d09")]
    d09: Option<GateCD09Evidence>,
    #[serde(rename = "bindings")]
    bindings: BoundedVec<GateCBindingWire, MAX_GATE_C_BINDINGS>,
    #[serde(rename = "insertions")]
    insertions: BoundedVec<GateCInsertionWire, MAX_GATE_C_INSERTIONS>,
    #[serde(rename = "q14")]
    q14: Option<GateCQ14Wire>,
    #[serde(rename = "q14_partial")]
    q14_partial: Option<GateCQ14PartialWire>,
    #[serde(rename = "artifacts")]
    screenshot_artifacts: BoundedVec<String, MAX_GATE_C_SCREENSHOTS>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GateCCaseWireV2 {
    #[serde(rename = "v")]
    version: u8,
    #[serde(rename = "i")]
    identities: BoundedVec<GateCIdentityWire, MAX_GATE_C_IDENTITY_DICTIONARY>,
    #[serde(rename = "p")]
    packet: GateCCaseBodyWire,
}

#[derive(Deserialize)]
struct GateCReportEvidenceReadback {
    gate_c_evidence: BoundedVec<GateCCaseEvidence, GATE_C_REQUIRED_CASES>,
}

pub(super) fn deserialize_report_evidence(bytes: &[u8]) -> Result<Vec<GateCCaseEvidence>, String> {
    let readback: GateCReportEvidenceReadback = serde_json::from_slice(bytes)
        .map_err(|error| format!("typed Gate C v2 report readback failed: {error}"))?;
    Ok(readback.gate_c_evidence.into_vec())
}

#[derive(Default)]
struct IdentityTable {
    values: Vec<GateCEditorIdentity>,
}

impl IdentityTable {
    fn intern(&mut self, identity: &GateCEditorIdentity) -> Result<u16, String> {
        if let Some(index) = self.values.iter().position(|value| value == identity) {
            return u16::try_from(index).map_err(|_| "Gate C identity reference overflowed".into());
        }
        if self.values.len() >= MAX_GATE_C_IDENTITY_DICTIONARY {
            return Err("Gate C identity dictionary exceeds its bound".into());
        }
        let index = self.values.len();
        self.values.push(identity.clone());
        u16::try_from(index).map_err(|_| "Gate C identity reference overflowed".into())
    }
}

fn identity_at(
    identities: &[GateCEditorIdentity],
    index: u16,
) -> Result<GateCEditorIdentity, String> {
    identities
        .get(usize::from(index))
        .cloned()
        .ok_or_else(|| format!("Gate C identity reference {index} is out of bounds"))
}

fn encode_control(
    evidence: &GateCControlEvidence,
    identities: &mut IdentityTable,
) -> Result<GateCControlWire, String> {
    Ok(GateCControlWire(
        evidence.control.clone(),
        evidence.trace_sequence,
        identities.intern(&evidence.identity)?,
        (
            evidence.owner_session_id,
            evidence.owner_generation,
            evidence.control_index,
        ),
        [
            evidence.target_digest,
            evidence.title_digest,
            evidence.type_digest,
            evidence.disambiguator_digest,
            evidence.action_digest,
            evidence.binding_digest,
            evidence.query_digest,
            evidence.value_digest,
            evidence.displayed_text_digest,
        ],
        (evidence.bounds, evidence.full_bounds, evidence.client_size),
        [
            evidence.visible,
            evidence.fully_visible,
            evidence.enabled,
            evidence.selected,
            evidence.focused,
            evidence.clicked,
            evidence.changed,
            evidence.enter_pressed,
        ],
        evidence.readable_text_digest,
    ))
}

fn decode_control(
    wire: GateCControlWire,
    identities: &[GateCEditorIdentity],
) -> Result<GateCControlEvidence, String> {
    let GateCControlWire(
        control,
        trace_sequence,
        identity,
        owners,
        digests,
        geometry,
        flags,
        readable,
    ) = wire;
    Ok(GateCControlEvidence {
        control,
        trace_sequence,
        identity: identity_at(identities, identity)?,
        owner_session_id: owners.0,
        owner_generation: owners.1,
        control_index: owners.2,
        target_digest: digests[0],
        title_digest: digests[1],
        type_digest: digests[2],
        disambiguator_digest: digests[3],
        action_digest: digests[4],
        binding_digest: digests[5],
        query_digest: digests[6],
        value_digest: digests[7],
        displayed_text_digest: digests[8],
        bounds: geometry.0,
        full_bounds: geometry.1,
        client_size: geometry.2,
        visible: flags[0],
        fully_visible: flags[1],
        enabled: flags[2],
        selected: flags[3],
        focused: flags[4],
        clicked: flags[5],
        changed: flags[6],
        enter_pressed: flags[7],
        readable_text_digest: readable,
    })
}

fn encode_result_row(
    evidence: &GateCResultRowEvidence,
    identities: &mut IdentityTable,
) -> Result<GateCResultRowWire, String> {
    Ok(GateCResultRowWire(
        evidence.surface,
        identities.intern(&evidence.identity)?,
        evidence.search_completion_sequence,
        evidence.observed_trace_sequence,
        evidence.order,
        [
            evidence.target_digest,
            evidence.title_digest,
            evidence.type_digest,
            evidence.disambiguator_digest,
            evidence.action_digest,
            evidence.binding_digest,
            evidence.displayed_text_digest,
        ],
        evidence.enabled,
        (evidence.bounds, evidence.full_bounds, evidence.client_size),
        [evidence.fully_visible, evidence.readable],
    ))
}

fn decode_result_row(
    wire: GateCResultRowWire,
    identities: &[GateCEditorIdentity],
) -> Result<GateCResultRowEvidence, String> {
    Ok(GateCResultRowEvidence {
        surface: wire.0,
        identity: identity_at(identities, wire.1)?,
        search_completion_sequence: wire.2,
        observed_trace_sequence: wire.3,
        order: wire.4,
        target_digest: wire.5[0],
        title_digest: wire.5[1],
        type_digest: wire.5[2],
        disambiguator_digest: wire.5[3],
        action_digest: wire.5[4],
        binding_digest: wire.5[5],
        displayed_text_digest: wire.5[6],
        enabled: wire.6,
        bounds: wire.7.0,
        full_bounds: wire.7.1,
        client_size: wire.7.2,
        fully_visible: wire.8[0],
        readable: wire.8[1],
    })
}

fn encode_provider(
    evidence: &GateCProviderLifecycleEvidence,
    identities: &mut IdentityTable,
) -> Result<GateCProviderWire, String> {
    Ok(GateCProviderWire(
        evidence.trace_sequence,
        evidence.edge,
        evidence.kind,
        identities.intern(&evidence.identity)?,
        evidence.query_digest,
        evidence.binding_digest,
        evidence.provider_revision,
    ))
}

fn decode_provider(
    wire: GateCProviderWire,
    identities: &[GateCEditorIdentity],
) -> Result<GateCProviderLifecycleEvidence, String> {
    Ok(GateCProviderLifecycleEvidence {
        trace_sequence: wire.0,
        edge: wire.1,
        kind: wire.2,
        identity: identity_at(identities, wire.3)?,
        query_digest: wire.4,
        binding_digest: wire.5,
        provider_revision: wire.6,
    })
}

fn encode_authoring_state(
    evidence: &GateCAuthoringStateEvidence,
    identities: &mut IdentityTable,
) -> Result<GateCAuthoringStateWire, String> {
    Ok(GateCAuthoringStateWire(
        evidence.request_id,
        evidence.baseline_request_id,
        (
            evidence.frame_ordinal,
            evidence.trace_sequence,
            evidence.trace_boundary_sequence,
            evidence.root_state_digest,
        ),
        (
            evidence.history_entries,
            evidence.history_keys,
            evidence.history_digest,
        ),
        (evidence.usage_entries, evidence.usage_digest),
        evidence.editor_open,
        (
            evidence.session_id,
            evidence.generation,
            evidence.selected_target_digest,
            evidence.selected_cell_digest,
            evidence.document_digest,
            evidence.assigned_binding_digest,
        ),
        evidence.properties_staged_digest,
        (evidence.draft_dirty, evidence.properties_dirty),
        (evidence.undo_depth, evidence.redo_depth),
        evidence.initial_snapshot_pending,
        evidence
            .action_editor
            .as_ref()
            .map(|identity| identities.intern(identity))
            .transpose()?,
        evidence.action_editor_authored_input_digest,
        (evidence.search_pending, evidence.test_pending),
    ))
}

fn decode_authoring_state(
    wire: GateCAuthoringStateWire,
    identities: &[GateCEditorIdentity],
) -> Result<GateCAuthoringStateEvidence, String> {
    Ok(GateCAuthoringStateEvidence {
        request_id: wire.0,
        baseline_request_id: wire.1,
        frame_ordinal: wire.2.0,
        trace_sequence: wire.2.1,
        trace_boundary_sequence: wire.2.2,
        root_state_digest: wire.2.3,
        history_entries: wire.3.0,
        history_keys: wire.3.1,
        history_digest: wire.3.2,
        usage_entries: wire.4.0,
        usage_digest: wire.4.1,
        editor_open: wire.5,
        session_id: wire.6.0,
        generation: wire.6.1,
        selected_target_digest: wire.6.2,
        selected_cell_digest: wire.6.3,
        document_digest: wire.6.4,
        assigned_binding_digest: wire.6.5,
        properties_staged_digest: wire.7,
        draft_dirty: wire.8.0,
        properties_dirty: wire.8.1,
        undo_depth: wire.9.0,
        redo_depth: wire.9.1,
        initial_snapshot_pending: wire.10,
        action_editor: wire
            .11
            .map(|index| identity_at(identities, index))
            .transpose()?,
        action_editor_authored_input_digest: wire.12,
        search_pending: wire.13.0,
        test_pending: wire.13.1,
    })
}

fn encode_binding_control(
    evidence: &GateCBindingControlEvidence,
    identities: &mut IdentityTable,
) -> Result<GateCBindingControlWire, String> {
    Ok(GateCBindingControlWire(
        evidence.kind,
        evidence.control.clone(),
        evidence.trace_sequence,
        evidence.surface,
        identities.intern(&evidence.identity)?,
        evidence.owner_session_id,
        evidence.owner_generation,
        evidence.binding_digest,
        evidence.document_digest_after,
    ))
}

fn decode_binding_control(
    wire: GateCBindingControlWire,
    identities: &[GateCEditorIdentity],
) -> Result<GateCBindingControlEvidence, String> {
    Ok(GateCBindingControlEvidence {
        kind: wire.0,
        control: wire.1,
        trace_sequence: wire.2,
        surface: wire.3,
        identity: identity_at(identities, wire.4)?,
        owner_session_id: wire.5,
        owner_generation: wire.6,
        binding_digest: wire.7,
        document_digest_after: wire.8,
    })
}

fn encode_search(
    evidence: &GateCSearchEvidence,
    identities: &mut IdentityTable,
) -> Result<GateCSearchWire, String> {
    Ok(GateCSearchWire(
        evidence.purpose,
        evidence.surface,
        identities.intern(&evidence.identity)?,
        evidence.query_digest,
        evidence.query_binding_digest,
        (
            evidence.search_control_sequence,
            evidence.queued_sequence,
            evidence.worker_started_sequence,
            evidence.worker_terminal_sequence,
            evidence.observed_trace_sequence,
        ),
        evidence.result_count,
        evidence.target_digest,
        evidence.action_digest,
        evidence.result_order,
        evidence.settled,
    ))
}

fn decode_search(
    wire: GateCSearchWire,
    identities: &[GateCEditorIdentity],
) -> Result<GateCSearchEvidence, String> {
    Ok(GateCSearchEvidence {
        purpose: wire.0,
        surface: wire.1,
        identity: identity_at(identities, wire.2)?,
        query_digest: wire.3,
        query_binding_digest: wire.4,
        search_control_sequence: wire.5.0,
        queued_sequence: wire.5.1,
        worker_started_sequence: wire.5.2,
        worker_terminal_sequence: wire.5.3,
        observed_trace_sequence: wire.5.4,
        result_count: wire.6,
        target_digest: wire.7,
        action_digest: wire.8,
        result_order: wire.9,
        settled: wire.10,
    })
}

fn encode_incomplete_search(
    evidence: &GateCIncompleteSearchEvidence,
    identities: &mut IdentityTable,
) -> Result<GateCIncompleteSearchWire, String> {
    Ok(GateCIncompleteSearchWire(
        identities.intern(&evidence.identity)?,
        evidence.query_digest,
        evidence.query_binding_digest,
        evidence.search_control_sequence,
        evidence.observed_trace_sequence,
        BoundedVec::try_from_vec(evidence.observed_edge_sequences.clone())?,
        evidence.reason,
    ))
}

fn decode_incomplete_search(
    wire: GateCIncompleteSearchWire,
    identities: &[GateCEditorIdentity],
) -> Result<GateCIncompleteSearchEvidence, String> {
    Ok(GateCIncompleteSearchEvidence {
        identity: identity_at(identities, wire.0)?,
        query_digest: wire.1,
        query_binding_digest: wire.2,
        search_control_sequence: wire.3,
        observed_trace_sequence: wire.4,
        observed_edge_sequences: wire.5.into_vec(),
        reason: wire.6,
    })
}

fn encode_q14_operation(
    evidence: &GateCQ14OperationEvidence,
    identities: &mut IdentityTable,
) -> Result<GateCQ14OperationWire, String> {
    Ok(GateCQ14OperationWire(
        evidence.kind,
        evidence.input_method,
        evidence.trace_sequence,
        evidence.control_sequence,
        identities.intern(&evidence.identity)?,
        evidence.query_digest,
        evidence.binding_digest,
    ))
}

fn decode_q14_operation(
    wire: GateCQ14OperationWire,
    identities: &[GateCEditorIdentity],
) -> Result<GateCQ14OperationEvidence, String> {
    Ok(GateCQ14OperationEvidence {
        kind: wire.0,
        input_method: wire.1,
        trace_sequence: wire.2,
        control_sequence: wire.3,
        identity: identity_at(identities, wire.4)?,
        query_digest: wire.5,
        binding_digest: wire.6,
    })
}

fn encode_binding(
    evidence: &GateCBindingEvidence,
    identities: &mut IdentityTable,
) -> Result<GateCBindingWire, String> {
    Ok(GateCBindingWire {
        route: evidence.route,
        destination_cell_digest: evidence.destination_cell_digest,
        semantic_target_digest: evidence.semantic_target_digest,
        semantic_action_digest: evidence.semantic_action_digest,
        kind: evidence.kind,
        action_id_digest: evidence.action_id_digest,
        target_reference_digest: evidence.target_reference_digest,
        query_digest: evidence.query_digest,
        query_mode: evidence.query_mode,
        command_digest: evidence.command_digest,
        arguments_digest: evidence.arguments_digest,
        expected_binding_digest: evidence.expected_binding_digest,
        previous_binding_digest: evidence.previous_binding_digest,
        document_before_digest: evidence.document_before_digest,
        document_staged_digest: evidence.document_staged_digest,
        history_before_digest: evidence.history_before_digest,
        history_staged_digest: evidence.history_staged_digest,
        document_applied_digest: evidence.document_applied_digest,
        history_applied_digest: evidence.history_applied_digest,
        saved_document_digest: evidence.saved_document_digest,
        reopened_document_digest: evidence.reopened_document_digest,
        radial_save: evidence.radial_save.clone(),
        reopened_binding_digest: evidence.reopened_binding_digest,
        inspector_binding_digest: evidence.inspector_binding_digest,
        undo_document_digest: evidence.undo_document_digest,
        redo_document_digest: evidence.redo_document_digest,
        undo_depth_before: evidence.undo_depth_before,
        undo_depth_after: evidence.undo_depth_after,
        mutation_count: evidence.mutation_count,
        controls: BoundedVec::try_from_vec(
            evidence
                .controls
                .iter()
                .map(|control| encode_binding_control(control, identities))
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        after_action_policy: evidence.after_action_policy,
        saved_after_reopen: evidence.saved_after_reopen,
        inspector_matches_saved_binding: evidence.inspector_matches_saved_binding,
        undo_restores_before: evidence.undo_restores_before,
        redo_restores_after: evidence.redo_restores_after,
    })
}

fn decode_binding(
    wire: GateCBindingWire,
    identities: &[GateCEditorIdentity],
) -> Result<GateCBindingEvidence, String> {
    Ok(GateCBindingEvidence {
        route: wire.route,
        destination_cell_digest: wire.destination_cell_digest,
        semantic_target_digest: wire.semantic_target_digest,
        semantic_action_digest: wire.semantic_action_digest,
        kind: wire.kind,
        action_id_digest: wire.action_id_digest,
        target_reference_digest: wire.target_reference_digest,
        query_digest: wire.query_digest,
        query_mode: wire.query_mode,
        command_digest: wire.command_digest,
        arguments_digest: wire.arguments_digest,
        expected_binding_digest: wire.expected_binding_digest,
        previous_binding_digest: wire.previous_binding_digest,
        document_before_digest: wire.document_before_digest,
        document_staged_digest: wire.document_staged_digest,
        history_before_digest: wire.history_before_digest,
        history_staged_digest: wire.history_staged_digest,
        document_applied_digest: wire.document_applied_digest,
        history_applied_digest: wire.history_applied_digest,
        saved_document_digest: wire.saved_document_digest,
        reopened_document_digest: wire.reopened_document_digest,
        radial_save: wire.radial_save,
        reopened_binding_digest: wire.reopened_binding_digest,
        inspector_binding_digest: wire.inspector_binding_digest,
        undo_document_digest: wire.undo_document_digest,
        redo_document_digest: wire.redo_document_digest,
        undo_depth_before: wire.undo_depth_before,
        undo_depth_after: wire.undo_depth_after,
        mutation_count: wire.mutation_count,
        controls: wire
            .controls
            .into_vec()
            .into_iter()
            .map(|control| decode_binding_control(control, identities))
            .collect::<Result<Vec<_>, _>>()?,
        after_action_policy: wire.after_action_policy,
        saved_after_reopen: wire.saved_after_reopen,
        inspector_matches_saved_binding: wire.inspector_matches_saved_binding,
        undo_restores_before: wire.undo_restores_before,
        redo_restores_after: wire.redo_restores_after,
    })
}

fn encode_insertion_control(evidence: &GateCInsertionControlEvidence) -> GateCInsertionControlWire {
    GateCInsertionControlWire(
        evidence.kind,
        evidence.widget_part,
        evidence.trace_sequence,
        evidence.session_id,
        evidence.generation,
        [evidence.enabled, evidence.selected, evidence.clicked],
        (evidence.bounds, evidence.full_bounds, evidence.client_size),
        [evidence.visible, evidence.fully_visible],
        [
            evidence.destination_menu_digest,
            evidence.destination_ring_digest,
            evidence.destination_cell_digest,
        ],
        (
            evidence.document_digest_after,
            evidence.binding_digest_after,
        ),
    )
}

fn decode_insertion_control(
    wire: GateCInsertionControlWire,
    parent: &GateCInsertionEvidence,
) -> GateCInsertionControlEvidence {
    GateCInsertionControlEvidence {
        kind: wire.0,
        widget_part: wire.1,
        trace_sequence: wire.2,
        request_id: parent.request_id,
        session_id: wire.3,
        generation: wire.4,
        source_target_digest: parent.source_target_digest,
        source_action_digest: parent.source_action_digest,
        source_binding_digest: parent.source_binding_digest,
        source_query_digest: parent.source_query_digest,
        enabled: wire.5[0],
        selected: wire.5[1],
        clicked: wire.5[2],
        bounds: wire.6.0,
        full_bounds: wire.6.1,
        client_size: wire.6.2,
        visible: wire.7[0],
        fully_visible: wire.7[1],
        destination_menu_digest: wire.8[0],
        destination_ring_digest: wire.8[1],
        destination_cell_digest: wire.8[2],
        document_digest_after: wire.9.0,
        binding_digest_after: wire.9.1,
    }
}

fn encode_observation_receipt(
    evidence: &GateCInsertionObservationReceipt,
    identities: &mut IdentityTable,
) -> Result<GateCInsertionObservationReceiptWire, String> {
    Ok(GateCInsertionObservationReceiptWire(
        encode_authoring_state(&evidence.state, identities)?,
        evidence.boundary.clone(),
    ))
}

fn decode_observation_receipt(
    wire: GateCInsertionObservationReceiptWire,
    identities: &[GateCEditorIdentity],
) -> Result<GateCInsertionObservationReceipt, String> {
    Ok(GateCInsertionObservationReceipt {
        state: decode_authoring_state(wire.0, identities)?,
        boundary: wire.1,
    })
}

fn encode_insertion(
    evidence: &GateCInsertionEvidence,
    identities: &mut IdentityTable,
) -> Result<GateCInsertionWire, String> {
    Ok(GateCInsertionWire {
        layout: evidence.layout,
        session_origin: evidence.session_origin,
        request_id: evidence.request_id,
        source_target_digest: evidence.source_target_digest,
        source_action_digest: evidence.source_action_digest,
        source_binding_digest: evidence.source_binding_digest,
        source_query_digest: evidence.source_query_digest,
        source_root_query_digest: evidence.source_root_query_digest,
        source_results_grid_layout: evidence.source_results_grid_layout,
        prior_binding_digest: evidence.prior_binding_digest,
        source_activation_delta: evidence.source_activation_delta,
        source_dispatch_delta: evidence.source_dispatch_delta,
        source_action_execution_delta: evidence.source_action_execution_delta,
        source_history_digest_before: evidence.source_history_digest_before,
        source_history_digest_after: evidence.source_history_digest_after,
        source_usage_digest_before: evidence.source_usage_digest_before,
        source_usage_digest_after: evidence.source_usage_digest_after,
        source_marker_digest_before: evidence.source_marker_digest_before,
        source_marker_digest_after: evidence.source_marker_digest_after,
        controls: BoundedVec::try_from_vec(
            evidence
                .controls
                .iter()
                .map(encode_insertion_control)
                .collect(),
        )?,
        initial_snapshot_ready: evidence.initial_snapshot_ready,
        initial_snapshot_receipt: evidence.initial_snapshot_receipt,
        source_baseline_receipt: encode_observation_receipt(
            &evidence.source_baseline_receipt,
            identities,
        )?,
        source_owner_receipt: encode_observation_receipt(
            &evidence.source_owner_receipt,
            identities,
        )?,
        outcome_receipt: encode_observation_receipt(&evidence.outcome_receipt, identities)?,
        source_baseline_observation_sequence: evidence.source_baseline_observation_sequence,
        source_baseline_boundary_sequence: evidence.source_baseline_boundary_sequence,
        source_baseline_session_id: evidence.source_baseline_session_id,
        source_baseline_generation: evidence.source_baseline_generation,
        source_baseline_document_digest: evidence.source_baseline_document_digest,
        initial_snapshot_session_id: evidence.initial_snapshot_session_id,
        initial_snapshot_sequence: evidence.initial_snapshot_sequence,
        source_owner_observation_sequence: evidence.source_owner_observation_sequence,
        source_owner_boundary_sequence: evidence.source_owner_boundary_sequence,
        source_owner_session_id: evidence.source_owner_session_id,
        source_owner_generation: evidence.source_owner_generation,
        source_owner_document_digest: evidence.source_owner_document_digest,
        outcome_observation_sequence: evidence.outcome_observation_sequence,
        outcome_boundary_sequence: evidence.outcome_boundary_sequence,
        outcome_session_id: evidence.outcome_session_id,
        outcome_generation: evidence.outcome_generation,
        outcome_document_digest: evidence.outcome_document_digest,
        outcome_undo_depth: evidence.outcome_undo_depth,
        outcome_redo_depth: evidence.outcome_redo_depth,
        outcome_draft_dirty: evidence.outcome_draft_dirty,
        policy_confirmation_required: evidence.policy_confirmation_required,
        destination_menu_digest: evidence.destination_menu_digest,
        destination_ring_digest: evidence.destination_ring_digest,
        destination_cell_digest: evidence.destination_cell_digest,
        outcome: evidence.outcome,
        documents: [
            evidence.document_before_digest,
            evidence.document_after_digest,
            evidence.document_undo_digest,
            evidence.document_redo_digest,
        ],
        unrelated_edit_digests: [
            evidence.unrelated_edit_digest_before,
            evidence.unrelated_edit_digest_after,
            evidence.unrelated_edit_digest_undo,
            evidence.unrelated_edit_digest_redo,
        ],
        unrelated_cell_target_digest: evidence.unrelated_cell_target_digest,
        unrelated_label_value_digest: evidence.unrelated_label_value_digest,
        undo_depths: [
            evidence.undo_depth_before,
            evidence.undo_depth_after,
            evidence.undo_depth_undo,
            evidence.undo_depth_redo,
        ],
        redo_depths: [
            evidence.redo_depth_before,
            evidence.redo_depth_after,
            evidence.redo_depth_undo,
            evidence.redo_depth_redo,
        ],
        dirty_states: [
            evidence.draft_dirty_before,
            evidence.draft_dirty_after,
            evidence.draft_dirty_undo,
            evidence.draft_dirty_redo,
        ],
        radial_history_digests: [
            evidence.radial_history_digest_before,
            evidence.radial_history_digest_after,
            evidence.radial_history_digest_undo,
            evidence.radial_history_digest_redo,
        ],
        unrelated_dirty_edit_preserved: evidence.unrelated_dirty_edit_preserved,
        saved_typed_binding_matches: evidence.saved_typed_binding_matches,
        saved_binding: evidence
            .saved_binding
            .as_ref()
            .map(|binding| encode_binding(binding, identities))
            .transpose()?,
        one_undo_restores_before: evidence.one_undo_restores_before,
        one_redo_restores_after: evidence.one_redo_restores_after,
    })
}

fn decode_insertion(
    wire: GateCInsertionWire,
    identities: &[GateCEditorIdentity],
) -> Result<GateCInsertionEvidence, String> {
    // The source tuple is present once on the parent and inherited by every
    // compact child record when rebuilding the complete validation model.
    let mut evidence = GateCInsertionEvidence {
        layout: wire.layout,
        session_origin: wire.session_origin,
        request_id: wire.request_id,
        source_target_digest: wire.source_target_digest,
        source_action_digest: wire.source_action_digest,
        source_binding_digest: wire.source_binding_digest,
        source_query_digest: wire.source_query_digest,
        source_root_query_digest: wire.source_root_query_digest,
        source_results_grid_layout: wire.source_results_grid_layout,
        prior_binding_digest: wire.prior_binding_digest,
        source_activation_delta: wire.source_activation_delta,
        source_dispatch_delta: wire.source_dispatch_delta,
        source_action_execution_delta: wire.source_action_execution_delta,
        source_history_digest_before: wire.source_history_digest_before,
        source_history_digest_after: wire.source_history_digest_after,
        source_usage_digest_before: wire.source_usage_digest_before,
        source_usage_digest_after: wire.source_usage_digest_after,
        source_marker_digest_before: wire.source_marker_digest_before,
        source_marker_digest_after: wire.source_marker_digest_after,
        controls: Vec::new(),
        initial_snapshot_ready: wire.initial_snapshot_ready,
        initial_snapshot_receipt: wire.initial_snapshot_receipt,
        source_baseline_receipt: decode_observation_receipt(
            wire.source_baseline_receipt,
            identities,
        )?,
        source_owner_receipt: decode_observation_receipt(wire.source_owner_receipt, identities)?,
        outcome_receipt: decode_observation_receipt(wire.outcome_receipt, identities)?,
        source_baseline_observation_sequence: wire.source_baseline_observation_sequence,
        source_baseline_boundary_sequence: wire.source_baseline_boundary_sequence,
        source_baseline_session_id: wire.source_baseline_session_id,
        source_baseline_generation: wire.source_baseline_generation,
        source_baseline_document_digest: wire.source_baseline_document_digest,
        initial_snapshot_session_id: wire.initial_snapshot_session_id,
        initial_snapshot_sequence: wire.initial_snapshot_sequence,
        source_owner_observation_sequence: wire.source_owner_observation_sequence,
        source_owner_boundary_sequence: wire.source_owner_boundary_sequence,
        source_owner_session_id: wire.source_owner_session_id,
        source_owner_generation: wire.source_owner_generation,
        source_owner_document_digest: wire.source_owner_document_digest,
        outcome_observation_sequence: wire.outcome_observation_sequence,
        outcome_boundary_sequence: wire.outcome_boundary_sequence,
        outcome_session_id: wire.outcome_session_id,
        outcome_generation: wire.outcome_generation,
        outcome_document_digest: wire.outcome_document_digest,
        outcome_undo_depth: wire.outcome_undo_depth,
        outcome_redo_depth: wire.outcome_redo_depth,
        outcome_draft_dirty: wire.outcome_draft_dirty,
        policy_confirmation_required: wire.policy_confirmation_required,
        destination_menu_digest: wire.destination_menu_digest,
        destination_ring_digest: wire.destination_ring_digest,
        destination_cell_digest: wire.destination_cell_digest,
        outcome: wire.outcome,
        document_before_digest: wire.documents[0],
        document_after_digest: wire.documents[1],
        document_undo_digest: wire.documents[2],
        document_redo_digest: wire.documents[3],
        unrelated_edit_digest_before: wire.unrelated_edit_digests[0],
        unrelated_edit_digest_after: wire.unrelated_edit_digests[1],
        unrelated_edit_digest_undo: wire.unrelated_edit_digests[2],
        unrelated_edit_digest_redo: wire.unrelated_edit_digests[3],
        unrelated_cell_target_digest: wire.unrelated_cell_target_digest,
        unrelated_label_value_digest: wire.unrelated_label_value_digest,
        undo_depth_before: wire.undo_depths[0],
        undo_depth_after: wire.undo_depths[1],
        undo_depth_undo: wire.undo_depths[2],
        undo_depth_redo: wire.undo_depths[3],
        redo_depth_before: wire.redo_depths[0],
        redo_depth_after: wire.redo_depths[1],
        redo_depth_undo: wire.redo_depths[2],
        redo_depth_redo: wire.redo_depths[3],
        draft_dirty_before: wire.dirty_states[0],
        draft_dirty_after: wire.dirty_states[1],
        draft_dirty_undo: wire.dirty_states[2],
        draft_dirty_redo: wire.dirty_states[3],
        radial_history_digest_before: wire.radial_history_digests[0],
        radial_history_digest_after: wire.radial_history_digests[1],
        radial_history_digest_undo: wire.radial_history_digests[2],
        radial_history_digest_redo: wire.radial_history_digests[3],
        unrelated_dirty_edit_preserved: wire.unrelated_dirty_edit_preserved,
        saved_typed_binding_matches: wire.saved_typed_binding_matches,
        saved_binding: wire
            .saved_binding
            .map(|binding| decode_binding(binding, identities))
            .transpose()?,
        one_undo_restores_before: wire.one_undo_restores_before,
        one_redo_restores_after: wire.one_redo_restores_after,
    };
    evidence.controls = wire
        .controls
        .into_vec()
        .into_iter()
        .map(|control| decode_insertion_control(control, &evidence))
        .collect();
    Ok(evidence)
}

fn encode_q14(
    evidence: &GateCQ14Evidence,
    identities: &mut IdentityTable,
) -> Result<GateCQ14Wire, String> {
    Ok(GateCQ14Wire {
        baseline: encode_authoring_state(&evidence.baseline, identities)?,
        terminal: encode_authoring_state(&evidence.terminal, identities)?,
        marker_entries_before: evidence.marker_entries_before,
        marker_entries_after: evidence.marker_entries_after,
        marker_digests: [evidence.marker_digest_before, evidence.marker_digest_after],
        normal_dispatches: evidence.normal_dispatches,
        universal_action_dispatches: evidence.universal_action_dispatches,
        radial_action_dispatches: evidence.radial_action_dispatches,
        authoring_tests: evidence.authoring_tests,
        operations: BoundedVec::try_from_vec(
            evidence
                .operations
                .iter()
                .map(|operation| encode_q14_operation(operation, identities))
                .collect::<Result<Vec<_>, _>>()?,
        )?,
    })
}

fn decode_q14(
    wire: GateCQ14Wire,
    identities: &[GateCEditorIdentity],
) -> Result<GateCQ14Evidence, String> {
    Ok(GateCQ14Evidence {
        baseline: decode_authoring_state(wire.baseline, identities)?,
        terminal: decode_authoring_state(wire.terminal, identities)?,
        marker_entries_before: wire.marker_entries_before,
        marker_entries_after: wire.marker_entries_after,
        marker_digest_before: wire.marker_digests[0],
        marker_digest_after: wire.marker_digests[1],
        normal_dispatches: wire.normal_dispatches,
        universal_action_dispatches: wire.universal_action_dispatches,
        radial_action_dispatches: wire.radial_action_dispatches,
        authoring_tests: wire.authoring_tests,
        operations: wire
            .operations
            .into_vec()
            .into_iter()
            .map(|operation| decode_q14_operation(operation, identities))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn encode_q14_partial(
    evidence: &GateCQ14PartialEvidence,
    identities: &mut IdentityTable,
) -> Result<GateCQ14PartialWire, String> {
    Ok(GateCQ14PartialWire {
        baseline: encode_authoring_state(&evidence.baseline, identities)?,
        terminal: evidence
            .terminal
            .as_ref()
            .map(|state| encode_authoring_state(state, identities))
            .transpose()?,
        operations: BoundedVec::try_from_vec(
            evidence
                .operations
                .iter()
                .map(|operation| encode_q14_operation(operation, identities))
                .collect::<Result<Vec<_>, _>>()?,
        )?,
    })
}

fn decode_q14_partial(
    wire: GateCQ14PartialWire,
    identities: &[GateCEditorIdentity],
) -> Result<GateCQ14PartialEvidence, String> {
    Ok(GateCQ14PartialEvidence {
        baseline: decode_authoring_state(wire.baseline, identities)?,
        terminal: wire
            .terminal
            .map(|state| decode_authoring_state(state, identities))
            .transpose()?,
        operations: wire
            .operations
            .into_vec()
            .into_iter()
            .map(|operation| decode_q14_operation(operation, identities))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn encode_body(
    evidence: &GateCCaseEvidence,
    identities: &mut IdentityTable,
) -> Result<GateCCaseBodyWire, String> {
    Ok(GateCCaseBodyWire {
        schema_version: evidence.schema_version,
        case_id: evidence.case_id.clone(),
        fixture_digest: evidence.fixture_digest.clone(),
        session_id: evidence.session_id,
        draft_generation: evidence.draft_generation,
        editor_surface: evidence.editor_surface,
        editor_identity: evidence
            .editor_identity
            .as_ref()
            .map(|identity| identities.intern(identity))
            .transpose()?,
        controls: BoundedVec::try_from_vec(
            evidence
                .controls
                .iter()
                .map(|control| encode_control(control, identities))
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        ordered_results: BoundedVec::try_from_vec(
            evidence
                .ordered_results
                .iter()
                .map(|row| encode_result_row(row, identities))
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        inspector_text_edits: BoundedVec::try_from_vec(evidence.inspector_text_edits.clone())?,
        searches: BoundedVec::try_from_vec(
            evidence
                .searches
                .iter()
                .map(|search| encode_search(search, identities))
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        incomplete_searches: BoundedVec::try_from_vec(
            evidence
                .incomplete_searches
                .iter()
                .map(|search| encode_incomplete_search(search, identities))
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        provider_lifecycle: BoundedVec::try_from_vec(
            evidence
                .provider_lifecycle
                .iter()
                .map(|event| encode_provider(event, identities))
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        authoring_states: BoundedVec::try_from_vec(
            evidence
                .authoring_states
                .iter()
                .map(|state| encode_authoring_state(state, identities))
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        observation_boundaries: BoundedVec::try_from_vec(evidence.observation_boundaries.clone())?,
        designer_controls: BoundedVec::try_from_vec(evidence.designer_controls.clone())?,
        designer_closes: BoundedVec::try_from_vec(evidence.designer_closes.clone())?,
        initial_snapshots: BoundedVec::try_from_vec(evidence.initial_snapshots.clone())?,
        d09: evidence.d09.clone(),
        bindings: BoundedVec::try_from_vec(
            evidence
                .bindings
                .iter()
                .map(|binding| encode_binding(binding, identities))
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        insertions: BoundedVec::try_from_vec(
            evidence
                .insertions
                .iter()
                .map(|insertion| encode_insertion(insertion, identities))
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        q14: evidence
            .q14
            .as_ref()
            .map(|q14| encode_q14(q14, identities))
            .transpose()?,
        q14_partial: evidence
            .q14_partial
            .as_ref()
            .map(|partial| encode_q14_partial(partial, identities))
            .transpose()?,
        screenshot_artifacts: BoundedVec::try_from_vec(evidence.screenshot_artifacts.clone())?,
    })
}

fn decode_body(
    wire: GateCCaseBodyWire,
    identities: &[GateCEditorIdentity],
) -> Result<GateCCaseEvidence, String> {
    Ok(GateCCaseEvidence {
        schema_version: wire.schema_version,
        case_id: wire.case_id,
        fixture_digest: wire.fixture_digest,
        session_id: wire.session_id,
        draft_generation: wire.draft_generation,
        editor_surface: wire.editor_surface,
        editor_identity: wire
            .editor_identity
            .map(|index| identity_at(identities, index))
            .transpose()?,
        controls: wire
            .controls
            .into_vec()
            .into_iter()
            .map(|control| decode_control(control, identities))
            .collect::<Result<Vec<_>, _>>()?,
        ordered_results: wire
            .ordered_results
            .into_vec()
            .into_iter()
            .map(|row| decode_result_row(row, identities))
            .collect::<Result<Vec<_>, _>>()?,
        inspector_text_edits: wire.inspector_text_edits.into_vec(),
        searches: wire
            .searches
            .into_vec()
            .into_iter()
            .map(|search| decode_search(search, identities))
            .collect::<Result<Vec<_>, _>>()?,
        incomplete_searches: wire
            .incomplete_searches
            .into_vec()
            .into_iter()
            .map(|search| decode_incomplete_search(search, identities))
            .collect::<Result<Vec<_>, _>>()?,
        provider_lifecycle: wire
            .provider_lifecycle
            .into_vec()
            .into_iter()
            .map(|event| decode_provider(event, identities))
            .collect::<Result<Vec<_>, _>>()?,
        authoring_states: wire
            .authoring_states
            .into_vec()
            .into_iter()
            .map(|state| decode_authoring_state(state, identities))
            .collect::<Result<Vec<_>, _>>()?,
        observation_boundaries: wire.observation_boundaries.into_vec(),
        designer_controls: wire.designer_controls.into_vec(),
        designer_closes: wire.designer_closes.into_vec(),
        initial_snapshots: wire.initial_snapshots.into_vec(),
        d09: wire.d09,
        bindings: wire
            .bindings
            .into_vec()
            .into_iter()
            .map(|binding| decode_binding(binding, identities))
            .collect::<Result<Vec<_>, _>>()?,
        insertions: wire
            .insertions
            .into_vec()
            .into_iter()
            .map(|insertion| decode_insertion(insertion, identities))
            .collect::<Result<Vec<_>, _>>()?,
        q14: wire
            .q14
            .map(|q14| decode_q14(q14, identities))
            .transpose()?,
        q14_partial: wire
            .q14_partial
            .map(|partial| decode_q14_partial(partial, identities))
            .transpose()?,
        screenshot_artifacts: wire.screenshot_artifacts.into_vec(),
    })
}

impl Serialize for GateCCaseEvidence {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut table = IdentityTable::default();
        let packet = encode_body(self, &mut table).map_err(serde::ser::Error::custom)?;
        let wire = GateCCaseWireV2 {
            version: GATE_C_WIRE_VERSION,
            identities: BoundedVec::try_from_vec(
                table.values.iter().map(GateCIdentityWire::from).collect(),
            )
            .map_err(serde::ser::Error::custom)?,
            packet,
        };
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for GateCCaseEvidence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = GateCCaseWireV2::deserialize(deserializer)?;
        if wire.version != GATE_C_WIRE_VERSION {
            return Err(serde::de::Error::custom(format!(
                "unsupported Gate C evidence wire version {}",
                wire.version
            )));
        }
        let identities = wire
            .identities
            .into_vec()
            .into_iter()
            .map(GateCEditorIdentity::from)
            .collect::<Vec<_>>();
        for index in 0..identities.len() {
            if identities[..index].contains(&identities[index]) {
                return Err(serde::de::Error::custom(
                    "Gate C identity dictionary contains a duplicate entry",
                ));
            }
        }
        decode_body(wire.packet, &identities).map_err(serde::de::Error::custom)
    }
}
