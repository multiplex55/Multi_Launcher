//! Bounded, observation-only mailbox used by the isolated radial acceptance runner.
//!
//! The request can only ask the GUI owner to capture ordinary ROOT state. It
//! cannot execute commands, resolve cells, or change presentation state.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

pub(crate) const OBSERVATION_ENV: &str = "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_OBSERVATION_FILE";
const MAX_REQUEST_BYTES: usize = 8 * 1024;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_SUMMARY_KEYS: usize = 4096;
const AUTHORING_SCHEMA_VERSION: u16 = 1;
const AUTHORING_REQUEST_SUFFIX: &str = ".authoring.request.json";
const AUTHORING_RESPONSE_SUFFIX: &str = ".authoring.response.json";
const NOTE_CLOSE_REQUEST_SUFFIX: &str = ".note-close.request.json";
const NOTE_CLOSE_RESPONSE_SUFFIX: &str = ".note-close.response.json";
pub(super) const Q11_NOTE_SLUG: &str = "radial-acceptance-q11";
pub(super) const Q11_NOTE_MARKER: &str = "# radial acceptance q11";

// Nullable protocol fields are present explicitly, including the first
// read-only lifetime discovery. Missing fields are malformed wire evidence.
fn note_close_required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum NoteCloseFixture {
    Q11,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NoteCloseObservationRequest {
    pub schema_version: u16,
    pub fixture: NoteCloseFixture,
    pub request_id: u64,
    pub run_nonce: [u64; 2],
    pub expected_hwnd: u64,
    pub expected_pid: u32,
    #[serde(deserialize_with = "note_close_required_nullable")]
    pub expected_generation: Option<u64>,
    pub after_frame_ordinal: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NoteCloseRootIdentity {
    pub hwnd: u64,
    pub process_id: u32,
    pub generation: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NoteCloseRenderedDiscard {
    pub owner_slug_digest: u64,
    pub widget_id: u64,
    pub role: NoteCloseWidgetRole,
    pub enabled: bool,
    pub visible: bool,
    pub fully_visible: bool,
    pub bounds: [i32; 4],
    pub clip: [i32; 4],
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum NoteCloseWidgetRole {
    DiscardChanges,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NoteCloseSoleNote {
    pub slug_digest: u64,
    pub fixture_slug: bool,
    pub fixture_marker: bool,
    pub pending_discard: bool,
    #[serde(deserialize_with = "note_close_required_nullable")]
    pub rendered_discard: Option<NoteCloseRenderedDiscard>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NoteCloseSnapshot {
    pub client_size: [i32; 2],
    pub open_note_count: usize,
    #[serde(deserialize_with = "note_close_required_nullable")]
    pub sole_note: Option<NoteCloseSoleNote>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum NoteCloseObservationStatus {
    Captured,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum NoteCloseObservationError {
    MalformedRequest,
    InvalidIdentity,
    StaleRequest,
    WrongRoot,
    StaleFrame,
    UnavailableClient,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NoteCloseObservationResponse {
    pub schema_version: u16,
    pub fixture: NoteCloseFixture,
    pub request_id: u64,
    pub run_nonce: [u64; 2],
    pub status: NoteCloseObservationStatus,
    #[serde(deserialize_with = "note_close_required_nullable")]
    pub error: Option<NoteCloseObservationError>,
    pub observed_frame_ordinal: u64,
    #[serde(deserialize_with = "note_close_required_nullable")]
    pub root: Option<NoteCloseRootIdentity>,
    #[serde(deserialize_with = "note_close_required_nullable")]
    pub snapshot: Option<NoteCloseSnapshot>,
}

/// Local to one ROOT render; no last-frame note/widget state is retained.
pub(super) struct NoteCloseRenderFrame {
    client_size: Option<[i32; 2]>,
    rendered_open_count: usize,
    sole_rendered_note: Option<NoteCloseSoleNote>,
}

impl NoteCloseRenderFrame {
    pub(super) fn new(ctx: &eframe::egui::Context) -> Self {
        let size = ctx.input(|input| input.screen_rect().size());
        let scale = ctx.pixels_per_point();
        let dimension = |logical: f32| {
            let pixels = (f64::from(logical) * f64::from(scale)).round();
            (logical.is_finite()
                && logical > 0.0
                && scale.is_finite()
                && scale > 0.0
                && pixels >= 1.0
                && pixels <= f64::from(i32::MAX))
            .then_some(pixels as i32)
        };
        Self {
            client_size: dimension(size.x)
                .zip(dimension(size.y))
                .map(|(x, y)| [x, y]),
            rendered_open_count: 0,
            sole_rendered_note: None,
        }
    }

    pub(super) fn observe_panel(
        &mut self,
        panel: &super::note_panel::NotePanel,
        discard: Option<NoteCloseRenderedDiscard>,
    ) {
        if !panel.open {
            return;
        }
        self.rendered_open_count = self.rendered_open_count.saturating_add(1);
        self.sole_rendered_note = (self.rendered_open_count == 1).then(|| {
            let mut note = panel.note_close_observation();
            note.rendered_discard = discard;
            note
        });
    }

    pub(super) fn snapshot(
        self,
        panels: &[super::note_panel::NotePanel],
    ) -> Option<NoteCloseSnapshot> {
        let mut open = panels.iter().filter(|panel| panel.open);
        let first = open.next();
        let count = usize::from(first.is_some()) + open.count();
        let sole_note = if count == 1 {
            let mut current = first?.note_close_observation();
            if self.rendered_open_count == 1
                && let Some(rendered) = self.sole_rendered_note
                && current
                    == (NoteCloseSoleNote {
                        rendered_discard: None,
                        ..rendered.clone()
                    })
            {
                current.rendered_discard = rendered.rendered_discard;
            }
            Some(current)
        } else {
            None
        };
        Some(NoteCloseSnapshot {
            client_size: self.client_size?,
            open_note_count: count,
            sole_note,
        })
    }
}

pub(super) fn observe_note_discard_response(
    ui: &eframe::egui::Ui,
    response: &eframe::egui::Response,
    slug: &str,
) -> Option<NoteCloseRenderedDiscard> {
    let scale = ui.ctx().pixels_per_point();
    let rect = |rect: eframe::egui::Rect| {
        let rect = crate::screen_draw::window_layers::desktop_rect_from_logical_edges(
            rect.left(),
            rect.top(),
            rect.right(),
            rect.bottom(),
            scale,
        )?;
        Some([
            rect.x,
            rect.y,
            rect.x.checked_add(i32::try_from(rect.width).ok()?)?,
            rect.y.checked_add(i32::try_from(rect.height).ok()?)?,
        ])
    };
    let bounds = rect(response.rect)?;
    let client = NoteCloseRenderFrame::new(ui.ctx()).client_size?;
    let outward_clip = rect(
        ui.clip_rect()
            .intersect(ui.ctx().input(|input| input.screen_rect())),
    )?;
    let clip = [
        outward_clip[0].max(0),
        outward_clip[1].max(0),
        outward_clip[2].min(client[0]),
        outward_clip[3].min(client[1]),
    ];
    Some(NoteCloseRenderedDiscard {
        owner_slug_digest: id_digest(slug),
        widget_id: response.id.value(),
        role: NoteCloseWidgetRole::DiscardChanges,
        enabled: response.enabled(),
        visible: ui.is_rect_visible(response.rect),
        fully_visible: ui.clip_rect().contains_rect(response.rect)
            && ui
                .ctx()
                .input(|input| input.screen_rect())
                .contains_rect(response.rect),
        bounds,
        clip,
    })
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuthoringObservationPhase {
    Baseline,
    Snapshot,
    Terminal,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthoringObservationRequest {
    pub schema_version: u16,
    pub request_id: u64,
    pub phase: AuthoringObservationPhase,
    pub baseline_request_id: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthoringEditorObservation {
    pub open: bool,
    pub session_id: u64,
    pub generation: u64,
    pub selected_target_digest: u64,
    pub selected_cell_digest: u64,
    pub selection_kind: AuthoringSelectionKind,
    pub selected_member_count: usize,
    pub selected_members_digest: u64,
    pub selected_member_target_digests: Vec<u64>,
    pub primary_cell_digest: u64,
    pub range_anchor_digest: u64,
    pub primary_target_digest: u64,
    pub range_anchor_target_digest: u64,
    pub navigation_path_digest: u64,
    pub navigation_menu_id_digests: Vec<u64>,
    pub navigation_edge_digests: Vec<u64>,
    pub pending_assets_digest: u64,
    pub designer_filter_digest: u64,
    pub designer_search_hit_count: usize,
    pub document_digest: u64,
    pub assigned_binding_digest: u64,
    pub properties_staged_digest: Option<u64>,
    pub draft_dirty: bool,
    pub properties_popup_open: bool,
    pub properties_dirty: bool,
    pub undo_depth: usize,
    pub redo_depth: usize,
    pub initial_snapshot_pending: bool,
    pub action_editor: Option<ActionEditorObservation>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuthoringSelectionKind {
    None,
    Menu,
    Ring,
    Cell,
    CellSet,
    Skin,
    Asset,
}

impl From<Option<&crate::radial::authoring::StableSelection>> for AuthoringSelectionKind {
    fn from(selection: Option<&crate::radial::authoring::StableSelection>) -> Self {
        use crate::radial::authoring::StableSelection;
        match selection {
            None => Self::None,
            Some(StableSelection::Menu(_)) => Self::Menu,
            Some(StableSelection::Ring { .. }) => Self::Ring,
            Some(StableSelection::Cell { .. }) => Self::Cell,
            Some(StableSelection::CellSet(_)) => Self::CellSet,
            Some(StableSelection::Skin(_)) => Self::Skin,
            Some(StableSelection::Asset(_)) => Self::Asset,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActionEditorObservation {
    pub surface: String,
    pub editor_session_id: u64,
    pub draft_generation: u64,
    pub stable_target_digest: u64,
    pub editor_epoch: u64,
    pub edit_generation: u64,
    pub query_generation: u64,
    pub query_request_generation: u64,
    pub search_request_generation: u64,
    pub test_request_generation: u64,
    pub query_digest: u64,
    pub authored_input_digest: u64,
    pub assigned_binding_digest: u64,
    pub selected_binding_digest: u64,
    pub search_pending: bool,
    pub test_pending: bool,
    pub result_count: usize,
    pub results_digest: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthoringObservationEvidence {
    pub frame_ordinal: u64,
    pub trace_sequence: u64,
    pub trace_boundary_sequence: u64,
    pub root: QueryOrdinaryRootCapture,
    pub effects: QueryObservationCountSummary,
    pub editor: AuthoringEditorObservation,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthoringObservationResponse {
    pub schema_version: u16,
    pub request_id: u64,
    pub phase: AuthoringObservationPhase,
    pub status: String,
    pub error: Option<String>,
    pub baseline_request_id: Option<u64>,
    pub observed_frame_ordinal: u64,
    pub before: Option<AuthoringObservationEvidence>,
    pub after: Option<AuthoringObservationEvidence>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum QueryObservationPhase {
    Baseline,
    Terminal,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueryObservationRequest {
    pub schema_version: u16,
    pub request_id: u64,
    pub phase: QueryObservationPhase,
    pub baseline_request_id: Option<u64>,
    pub session_digest: u64,
    pub cell_digest: u64,
    pub invocation_id: Option<u64>,
    pub query_digest: Option<u64>,
    pub action_digest: Option<u64>,
    pub source: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueryObservationResponse {
    pub schema_version: u16,
    pub request_id: u64,
    pub phase: QueryObservationPhase,
    pub status: String,
    pub error: Option<String>,
    pub session_digest: u64,
    pub cell_digest: u64,
    pub invocation_id: Option<u64>,
    pub baseline_request_id: Option<u64>,
    pub baseline_frame_ordinal: Option<u64>,
    pub observed_frame_ordinal: u64,
    pub baseline_state_digest: Option<u64>,
    pub query_digest: Option<u64>,
    pub action_digest: Option<u64>,
    pub source: Option<String>,
    pub before: Option<QueryRootStateEvidence>,
    pub after: Option<QueryRootStateEvidence>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueryRootStateEvidence {
    pub state_digest: u64,
    pub query_digest: u64,
    pub action_digest: u64,
    pub results_digest: u64,
    pub results_count: usize,
    pub selected_index: Option<usize>,
    pub grid_layout: bool,
    pub visible: bool,
    pub restore: bool,
    pub visibility_revision: u64,
    pub focus_query: bool,
    pub move_cursor_end: bool,
    pub last_results_valid: bool,
    pub last_search_query_digest: u64,
    pub suggestions_digest: u64,
    pub autocomplete_index: usize,
    pub query_history_digest: u64,
    pub matching_history_count: usize,
    pub source_history_count: usize,
    pub usage_count: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueryOrdinaryRootCapture {
    pub state_digest: u64,
    pub query_digest: u64,
    pub results_digest: u64,
    pub results_count: usize,
    pub selected_index: Option<usize>,
    pub grid_layout: bool,
    pub visible: bool,
    pub restore: bool,
    pub visibility_revision: u64,
    pub focus_query: bool,
    pub move_cursor_end: bool,
    pub last_results_valid: bool,
    pub last_search_query_digest: u64,
    pub suggestions_digest: u64,
    pub autocomplete_index: usize,
    pub query_history_digest: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueryObservationCountSummary {
    pub history_entries: usize,
    pub history_keys: usize,
    pub history_digest: u64,
    pub usage_entries: usize,
    pub usage_digest: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct HistoryKey {
    query_digest: u64,
    action_digest: u64,
    source_digest: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct QueryObservationCounts {
    history: BTreeMap<HistoryKey, usize>,
    usage: BTreeMap<u64, u32>,
    full_history_entries: usize,
    full_history_digest: u64,
    full_usage_entries: usize,
    full_usage_digest: u64,
}

impl Default for QueryObservationCounts {
    fn default() -> Self {
        Self {
            history: BTreeMap::new(),
            usage: BTreeMap::new(),
            full_history_entries: 0,
            full_history_digest: 0xcbf29ce484222325,
            full_usage_entries: 0,
            full_usage_digest: 0xcbf29ce484222325,
        }
    }
}

#[derive(Clone, Debug)]
struct AuthoringCapturedBaseline {
    request_id: u64,
    frame_ordinal: u64,
    trace_sequence: u64,
    trace_boundary_sequence: u64,
    root: QueryOrdinaryRootCapture,
    effects: QueryObservationCountSummary,
    editor: AuthoringEditorObservation,
}

impl QueryObservationCounts {
    pub(crate) fn capture_available_history(
        history: Option<Vec<crate::history::HistoryEntry>>,
        usage: &HashMap<String, u32>,
    ) -> Result<Self, String> {
        let history = history
            .ok_or_else(|| "history is unavailable for acceptance observation".to_string())?;
        Self::capture(&history, usage)
    }

    pub(crate) fn capture(
        history: &[crate::history::HistoryEntry],
        usage: &HashMap<String, u32>,
    ) -> Result<Self, String> {
        let mut counts = Self {
            full_history_entries: history.len(),
            full_history_digest: 0xcbf29ce484222325,
            full_usage_entries: usage.len(),
            full_usage_digest: 0xcbf29ce484222325,
            ..Self::default()
        };
        for (index, entry) in history.iter().enumerate() {
            let key = HistoryKey {
                query_digest: digest(&[entry.query.as_str()]),
                action_digest: history_action_digest(&entry.action),
                source_digest: entry
                    .source
                    .as_deref()
                    .map_or(0, |source| digest(&[source])),
            };
            *counts.history.entry(key).or_default() += 1;
            if counts.history.len() > MAX_SUMMARY_KEYS {
                return Err("history observation exceeded its bounded key capacity".into());
            }
            counts.full_history_digest =
                digest_feed(counts.full_history_digest, &(index as u64).to_le_bytes());
            counts.full_history_digest = digest_text(counts.full_history_digest, &entry.query);
            counts.full_history_digest = digest_text(counts.full_history_digest, &entry.query_lc);
            counts.full_history_digest =
                digest_text(counts.full_history_digest, &entry.action.label);
            counts.full_history_digest =
                digest_text(counts.full_history_digest, &entry.action.desc);
            counts.full_history_digest =
                digest_text(counts.full_history_digest, &entry.action.action);
            counts.full_history_digest =
                digest_optional_text(counts.full_history_digest, entry.action.args.as_deref());
            counts.full_history_digest =
                digest_optional_text(counts.full_history_digest, entry.source.as_deref());
            counts.full_history_digest =
                digest_feed(counts.full_history_digest, &entry.timestamp.to_le_bytes());
        }
        let mut usage_entries = usage.iter().collect::<Vec<_>>();
        usage_entries.sort_by(|(left, _), (right, _)| left.cmp(right));
        for (action, count) in usage_entries {
            counts.usage.insert(digest(&[action.as_str()]), *count);
            if counts.usage.len() > MAX_SUMMARY_KEYS {
                return Err("usage observation exceeded its bounded key capacity".into());
            }
            counts.full_usage_digest = digest_text(counts.full_usage_digest, action);
            counts.full_usage_digest = digest_feed(counts.full_usage_digest, &count.to_le_bytes());
        }
        Ok(counts)
    }

    fn summary(&self) -> QueryObservationCountSummary {
        QueryObservationCountSummary {
            history_entries: self.full_history_entries,
            history_keys: self.history.len(),
            history_digest: self.full_history_digest,
            usage_entries: self.full_usage_entries,
            usage_digest: self.full_usage_digest,
        }
    }

    fn matching_history(&self, query_digest: u64, action_digest: u64) -> usize {
        self.history
            .iter()
            .filter(|(key, _)| {
                key.query_digest == query_digest && key.action_digest == action_digest
            })
            .map(|(_, count)| *count)
            .sum()
    }

    fn source_history(&self, query_digest: u64, action_digest: u64, source_digest: u64) -> usize {
        self.history
            .get(&HistoryKey {
                query_digest,
                action_digest,
                source_digest,
            })
            .copied()
            .unwrap_or_default()
    }

    fn usage(&self, action_id_digest: u64) -> u32 {
        self.usage
            .get(&action_id_digest)
            .copied()
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug)]
struct CapturedBaseline {
    request_id: u64,
    session_digest: u64,
    cell_digest: u64,
    frame_ordinal: u64,
    root: QueryOrdinaryRootCapture,
    counts: QueryObservationCounts,
}

#[derive(Clone, Debug)]
struct BoundSelection {
    invocation_id: u64,
    session_digest: u64,
    cell_digest: u64,
    query_digest: u64,
    history_query_digest: u64,
    action_digest: u64,
    history_action_digest: u64,
    action_id_digest: u64,
    source: String,
}

#[derive(Debug, Default)]
pub(crate) struct QueryObservationMailbox {
    base_path: Option<PathBuf>,
    frame_ordinal: u64,
    last_request_id: u64,
    baseline: Option<CapturedBaseline>,
    selection: Option<BoundSelection>,
    binding_error: Option<String>,
    authoring_baseline: Option<AuthoringCapturedBaseline>,
    last_authoring_request_id: u64,
    last_note_close_request_id: u64,
}

impl QueryObservationMailbox {
    pub(crate) fn from_environment(trace_enabled: bool) -> Self {
        let base_path = trace_enabled
            .then(|| std::env::var_os(OBSERVATION_ENV).map(PathBuf::from))
            .flatten()
            .filter(|path| path.as_os_str().len() <= 4096);
        Self {
            base_path,
            ..Self::default()
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.base_path.is_some()
    }

    pub(crate) fn advance_frame(&mut self) {
        if self.enabled() {
            self.frame_ordinal = self.frame_ordinal.saturating_add(1);
        }
    }

    pub(crate) fn has_request(&self) -> bool {
        self.base_path
            .as_ref()
            .is_some_and(|base| std::fs::metadata(path_with_suffix(base, ".request.json")).is_ok())
    }

    pub(crate) fn has_authoring_request(&self) -> bool {
        self.base_path.as_ref().is_some_and(|base| {
            std::fs::metadata(path_with_suffix(base, AUTHORING_REQUEST_SUFFIX)).is_ok()
        })
    }

    pub(super) fn has_note_close_request(&self) -> bool {
        self.base_path.as_ref().is_some_and(|base| {
            std::fs::metadata(path_with_suffix(base, NOTE_CLOSE_REQUEST_SUFFIX)).is_ok()
        })
    }

    #[cfg(test)]
    pub(super) fn isolated_note_close_test(base_path: PathBuf) -> Self {
        Self {
            base_path: Some(base_path),
            ..Self::default()
        }
    }

    pub(super) fn poll_note_close(
        &mut self,
        root: Option<NoteCloseRootIdentity>,
        snapshot: Option<NoteCloseSnapshot>,
    ) -> bool {
        let Some(base) = self.base_path.as_ref() else {
            return false;
        };
        let request_path = path_with_suffix(base, NOTE_CLOSE_REQUEST_SUFFIX);
        let response_path = path_with_suffix(base, NOTE_CLOSE_RESPONSE_SUFFIX);
        let Ok(metadata) = std::fs::metadata(&request_path) else {
            return true;
        };
        let request = (metadata.len() <= MAX_REQUEST_BYTES as u64)
            .then(|| std::fs::read(&request_path).ok())
            .flatten()
            .filter(|bytes| bytes.len() <= MAX_REQUEST_BYTES)
            .and_then(|bytes| serde_json::from_slice::<NoteCloseObservationRequest>(&bytes).ok());
        let _ = std::fs::remove_file(&request_path);
        let response = if let Some(request) = request {
            self.apply_note_close_request(request, root, snapshot)
        } else {
            NoteCloseObservationResponse {
                schema_version: 1,
                fixture: NoteCloseFixture::Q11,
                request_id: 0,
                run_nonce: [0, 0],
                status: NoteCloseObservationStatus::Failed,
                error: Some(NoteCloseObservationError::MalformedRequest),
                observed_frame_ordinal: self.frame_ordinal,
                root,
                snapshot: None,
            }
        };
        let _ = write_response(&response_path, &response);
        true
    }

    fn apply_note_close_request(
        &mut self,
        request: NoteCloseObservationRequest,
        root: Option<NoteCloseRootIdentity>,
        snapshot: Option<NoteCloseSnapshot>,
    ) -> NoteCloseObservationResponse {
        let mut response = NoteCloseObservationResponse {
            schema_version: 1,
            fixture: request.fixture,
            request_id: request.request_id,
            run_nonce: request.run_nonce,
            status: NoteCloseObservationStatus::Failed,
            error: None,
            observed_frame_ordinal: self.frame_ordinal,
            root,
            snapshot: None,
        };
        let error = if request.schema_version != 1
            || request.request_id == 0
            || request.run_nonce == [0, 0]
            || request.expected_hwnd == 0
            || request.expected_pid == 0
            || request.expected_generation == Some(0)
        {
            Some(NoteCloseObservationError::InvalidIdentity)
        } else if request.request_id <= self.last_note_close_request_id {
            Some(NoteCloseObservationError::StaleRequest)
        } else if !response.root.as_ref().is_some_and(|root| {
            root.hwnd == request.expected_hwnd
                && root.process_id == request.expected_pid
                && root.generation > 0
                && request
                    .expected_generation
                    .is_none_or(|generation| generation == root.generation)
        }) {
            Some(NoteCloseObservationError::WrongRoot)
        } else if self.frame_ordinal == 0 || self.frame_ordinal <= request.after_frame_ordinal {
            Some(NoteCloseObservationError::StaleFrame)
        } else if snapshot.is_none() {
            Some(NoteCloseObservationError::UnavailableClient)
        } else {
            None
        };
        self.last_note_close_request_id = self.last_note_close_request_id.max(request.request_id);
        response.error = error;
        if error.is_none() {
            response.status = NoteCloseObservationStatus::Captured;
            response.snapshot = snapshot;
        }
        response
    }

    pub(crate) fn bind_selection(
        &mut self,
        identity: &crate::radial::handoff::RadialDispatchIdentity,
        query_digest: u64,
        history_query: &str,
        action: &crate::actions::Action,
        source: crate::commands::ActivationSource,
    ) {
        let Some(baseline) = self.baseline.as_ref() else {
            return;
        };
        let session_digest = id_digest(identity.session_id.as_str());
        let cell_digest = id_digest(&identity.selected_cell_id);
        if session_digest != baseline.session_digest || cell_digest != baseline.cell_digest {
            self.binding_error =
                Some("accepted selection did not match its captured baseline".into());
            self.selection = None;
            return;
        }
        if query_digest == 0 || action_digest(action) == 0 {
            self.binding_error =
                Some("accepted selection has an empty query or action identity".into());
            self.selection = None;
            return;
        }
        self.selection = Some(BoundSelection {
            invocation_id: identity.invocation_id.0,
            session_digest,
            cell_digest,
            query_digest,
            history_query_digest: digest(&[history_query]),
            action_digest: action_digest(action),
            history_action_digest: history_action_digest(action),
            action_id_digest: digest(&[action.action.as_str()]),
            source: source.label().to_owned(),
        });
        self.binding_error = None;
    }

    pub(crate) fn poll(
        &mut self,
        root: QueryOrdinaryRootCapture,
        counts: Result<QueryObservationCounts, String>,
    ) -> bool {
        let Some(base) = self.base_path.as_ref() else {
            return false;
        };
        let request_path = path_with_suffix(base, ".request.json");
        let response_path = path_with_suffix(base, ".response.json");
        let Ok(metadata) = std::fs::metadata(&request_path) else {
            return true;
        };
        let mut response = None;
        if metadata.len() as usize <= MAX_REQUEST_BYTES {
            if let Ok(bytes) = std::fs::read(&request_path) {
                if let Ok(request) = serde_json::from_slice::<QueryObservationRequest>(&bytes) {
                    response = Some(self.apply_request(request, root, counts));
                }
            }
        }
        let _ = std::fs::remove_file(&request_path);
        let response = response.unwrap_or_else(|| QueryObservationResponse {
            schema_version: 1,
            request_id: 0,
            phase: QueryObservationPhase::Baseline,
            status: "failed".into(),
            error: Some("observation request was malformed or exceeded its bound".into()),
            session_digest: 0,
            cell_digest: 0,
            invocation_id: None,
            baseline_request_id: None,
            baseline_frame_ordinal: None,
            observed_frame_ordinal: self.frame_ordinal,
            baseline_state_digest: None,
            query_digest: None,
            action_digest: None,
            source: None,
            before: None,
            after: None,
        });
        let _ = write_response(&response_path, &response);
        true
    }

    pub(crate) fn poll_authoring(
        &mut self,
        root: QueryOrdinaryRootCapture,
        counts: Result<QueryObservationCounts, String>,
        editor: Result<AuthoringEditorObservation, String>,
    ) -> bool {
        self.poll_authoring_with_boundary(root, counts, editor, |phase, request_id, baseline_id| {
            crate::radial::acceptance_trace::emit_authoring_observation_boundary(
                phase,
                request_id,
                baseline_id,
            )
        })
    }

    fn poll_authoring_with_boundary(
        &mut self,
        root: QueryOrdinaryRootCapture,
        counts: Result<QueryObservationCounts, String>,
        editor: Result<AuthoringEditorObservation, String>,
        mut publish_boundary: impl FnMut(&'static str, u64, Option<u64>) -> Option<(u64, u64)>,
    ) -> bool {
        let Some(base) = self.base_path.as_ref() else {
            return false;
        };
        let request_path = path_with_suffix(base, AUTHORING_REQUEST_SUFFIX);
        let response_path = path_with_suffix(base, AUTHORING_RESPONSE_SUFFIX);
        let Ok(metadata) = std::fs::metadata(&request_path) else {
            return true;
        };
        let mut response = None;
        if metadata.len() as usize <= MAX_REQUEST_BYTES {
            if let Ok(bytes) = std::fs::read(&request_path) {
                if let Ok(request) = serde_json::from_slice::<AuthoringObservationRequest>(&bytes) {
                    response = Some(self.apply_authoring_request(request, root, counts, editor));
                }
            }
        }
        let _ = std::fs::remove_file(&request_path);
        let mut response = response.unwrap_or_else(|| AuthoringObservationResponse {
            schema_version: AUTHORING_SCHEMA_VERSION,
            request_id: 0,
            phase: AuthoringObservationPhase::Baseline,
            status: "failed".into(),
            error: Some("authoring observation request was malformed or exceeded its bound".into()),
            baseline_request_id: None,
            observed_frame_ordinal: self.frame_ordinal,
            before: None,
            after: None,
        });
        if response.status == "captured" {
            let phase = match response.phase {
                AuthoringObservationPhase::Baseline => "baseline",
                AuthoringObservationPhase::Snapshot => "snapshot",
                AuthoringObservationPhase::Terminal => "terminal",
            };
            let boundary =
                publish_boundary(phase, response.request_id, response.baseline_request_id);
            if let Some((captured_trace_sequence, trace_boundary_sequence)) = boundary {
                match response.phase {
                    AuthoringObservationPhase::Baseline => {
                        if let Some(evidence) = response.before.as_mut() {
                            evidence.trace_sequence = captured_trace_sequence;
                            evidence.trace_boundary_sequence = trace_boundary_sequence;
                        }
                        if let Some(baseline) = self.authoring_baseline.as_mut() {
                            baseline.trace_sequence = captured_trace_sequence;
                            baseline.trace_boundary_sequence = trace_boundary_sequence;
                        }
                    }
                    AuthoringObservationPhase::Snapshot => {
                        if let Some(evidence) = response.before.as_mut() {
                            evidence.trace_sequence = captured_trace_sequence;
                            evidence.trace_boundary_sequence = trace_boundary_sequence;
                        }
                    }
                    AuthoringObservationPhase::Terminal => {
                        if let Some(evidence) = response.after.as_mut() {
                            evidence.trace_sequence = captured_trace_sequence;
                            evidence.trace_boundary_sequence = trace_boundary_sequence;
                        }
                    }
                }
            } else {
                if response.phase != AuthoringObservationPhase::Snapshot {
                    self.authoring_baseline = None;
                }
                response.status = "failed".into();
                response.error = Some(
                    "authoring trace boundary could not be published; observation was not acknowledged"
                        .into(),
                );
                response.before = None;
                response.after = None;
            }
        }
        let _ = write_response(&response_path, &response);
        true
    }

    fn apply_authoring_request(
        &mut self,
        request: AuthoringObservationRequest,
        root: QueryOrdinaryRootCapture,
        counts: Result<QueryObservationCounts, String>,
        editor: Result<AuthoringEditorObservation, String>,
    ) -> AuthoringObservationResponse {
        let failed = |error: String| AuthoringObservationResponse {
            schema_version: AUTHORING_SCHEMA_VERSION,
            request_id: request.request_id,
            phase: request.phase,
            status: "failed".into(),
            error: Some(error),
            baseline_request_id: request.baseline_request_id,
            observed_frame_ordinal: self.frame_ordinal,
            before: None,
            after: None,
        };
        if request.schema_version != AUTHORING_SCHEMA_VERSION || request.request_id == 0 {
            return failed("authoring observation request identity is invalid".into());
        }
        if request.request_id <= self.last_authoring_request_id {
            return failed("authoring observation request ID is stale or duplicated".into());
        }
        self.last_authoring_request_id = request.request_id;

        match request.phase {
            AuthoringObservationPhase::Baseline => {
                if request.baseline_request_id.is_some() {
                    return failed("authoring baseline contains terminal-only fields".into());
                }
                let effects = match counts {
                    Ok(counts) => counts.summary(),
                    Err(error) => return failed(error),
                };
                let editor = match editor {
                    Ok(editor) => editor,
                    Err(error) => return failed(error),
                };
                let trace_sequence = crate::radial::acceptance_trace::trace_sequence();
                let evidence = AuthoringObservationEvidence {
                    frame_ordinal: self.frame_ordinal,
                    trace_sequence,
                    trace_boundary_sequence: 0,
                    root,
                    effects: effects.clone(),
                    editor: editor.clone(),
                };
                self.authoring_baseline = Some(AuthoringCapturedBaseline {
                    request_id: request.request_id,
                    frame_ordinal: self.frame_ordinal,
                    trace_sequence,
                    trace_boundary_sequence: 0,
                    root: evidence.root.clone(),
                    effects,
                    editor,
                });
                AuthoringObservationResponse {
                    schema_version: AUTHORING_SCHEMA_VERSION,
                    request_id: request.request_id,
                    phase: request.phase,
                    status: "captured".into(),
                    error: None,
                    baseline_request_id: None,
                    observed_frame_ordinal: self.frame_ordinal,
                    before: Some(evidence),
                    after: None,
                }
            }
            AuthoringObservationPhase::Snapshot => {
                if request.baseline_request_id.is_some() {
                    return failed("authoring snapshot contains baseline-only fields".into());
                }
                let effects = match counts {
                    Ok(counts) => counts.summary(),
                    Err(error) => return failed(error),
                };
                let editor = match editor {
                    Ok(editor) => editor,
                    Err(error) => return failed(error),
                };
                let evidence = AuthoringObservationEvidence {
                    frame_ordinal: self.frame_ordinal,
                    trace_sequence: crate::radial::acceptance_trace::trace_sequence(),
                    trace_boundary_sequence: 0,
                    root,
                    effects,
                    editor,
                };
                AuthoringObservationResponse {
                    schema_version: AUTHORING_SCHEMA_VERSION,
                    request_id: request.request_id,
                    phase: request.phase,
                    status: "captured".into(),
                    error: None,
                    baseline_request_id: None,
                    observed_frame_ordinal: self.frame_ordinal,
                    before: Some(evidence),
                    after: None,
                }
            }
            AuthoringObservationPhase::Terminal => {
                let Some(baseline) = self.authoring_baseline.as_ref() else {
                    return failed("authoring terminal request has no captured baseline".into());
                };
                if request.baseline_request_id != Some(baseline.request_id)
                    || request.request_id <= baseline.request_id
                    || self.frame_ordinal <= baseline.frame_ordinal
                {
                    return failed(
                        "authoring terminal request did not match a later baseline".into(),
                    );
                }
                let effects = match counts {
                    Ok(counts) => counts.summary(),
                    Err(error) => return failed(error),
                };
                let editor = match editor {
                    Ok(editor) => editor,
                    Err(error) => return failed(error),
                };
                let before = AuthoringObservationEvidence {
                    frame_ordinal: baseline.frame_ordinal,
                    trace_sequence: baseline.trace_sequence,
                    trace_boundary_sequence: baseline.trace_boundary_sequence,
                    root: baseline.root.clone(),
                    effects: baseline.effects.clone(),
                    editor: baseline.editor.clone(),
                };
                let after = AuthoringObservationEvidence {
                    frame_ordinal: self.frame_ordinal,
                    trace_sequence: crate::radial::acceptance_trace::trace_sequence(),
                    trace_boundary_sequence: 0,
                    root,
                    effects,
                    editor,
                };
                self.authoring_baseline = None;
                AuthoringObservationResponse {
                    schema_version: AUTHORING_SCHEMA_VERSION,
                    request_id: request.request_id,
                    phase: request.phase,
                    status: "captured".into(),
                    error: None,
                    baseline_request_id: request.baseline_request_id,
                    observed_frame_ordinal: self.frame_ordinal,
                    before: Some(before),
                    after: Some(after),
                }
            }
        }
    }

    fn apply_request(
        &mut self,
        request: QueryObservationRequest,
        root: QueryOrdinaryRootCapture,
        counts: Result<QueryObservationCounts, String>,
    ) -> QueryObservationResponse {
        let failed = |error: String| QueryObservationResponse {
            schema_version: 1,
            request_id: request.request_id,
            phase: request.phase,
            status: "failed".into(),
            error: Some(error),
            session_digest: request.session_digest,
            cell_digest: request.cell_digest,
            invocation_id: request.invocation_id,
            baseline_request_id: request.baseline_request_id,
            baseline_frame_ordinal: None,
            observed_frame_ordinal: self.frame_ordinal,
            baseline_state_digest: None,
            query_digest: request.query_digest,
            action_digest: request.action_digest,
            source: request.source.clone(),
            before: None,
            after: None,
        };
        if request.schema_version != 1
            || request.request_id == 0
            || request.session_digest == 0
            || request.cell_digest == 0
        {
            return failed("observation request identity is invalid".into());
        }
        if request.request_id <= self.last_request_id {
            return failed("observation request ID is stale or duplicated".into());
        }
        self.last_request_id = request.request_id;
        match request.phase {
            QueryObservationPhase::Baseline => {
                if request.baseline_request_id.is_some()
                    || request.invocation_id.is_some()
                    || request.query_digest.is_some()
                    || request.action_digest.is_some()
                    || request.source.is_some()
                {
                    return failed("baseline request contains terminal-only fields".into());
                }
                let counts = match counts {
                    Ok(counts) => counts,
                    Err(error) => return failed(error),
                };
                self.baseline = Some(CapturedBaseline {
                    request_id: request.request_id,
                    session_digest: request.session_digest,
                    cell_digest: request.cell_digest,
                    frame_ordinal: self.frame_ordinal,
                    root: root.clone(),
                    counts,
                });
                self.selection = None;
                self.binding_error = None;
                QueryObservationResponse {
                    schema_version: 1,
                    request_id: request.request_id,
                    phase: request.phase,
                    status: "captured".into(),
                    error: None,
                    session_digest: request.session_digest,
                    cell_digest: request.cell_digest,
                    invocation_id: None,
                    baseline_request_id: Some(request.request_id),
                    baseline_frame_ordinal: Some(self.frame_ordinal),
                    observed_frame_ordinal: self.frame_ordinal,
                    baseline_state_digest: Some(root.state_digest),
                    query_digest: None,
                    action_digest: None,
                    source: None,
                    before: None,
                    after: None,
                }
            }
            QueryObservationPhase::Terminal => {
                let (Some(baseline), Some(selection)) =
                    (self.baseline.take(), self.selection.take())
                else {
                    self.binding_error = None;
                    return failed("terminal request has no bound baseline selection".into());
                };
                if let Some(error) = self.binding_error.take() {
                    return failed(error);
                }
                let is_current = request.baseline_request_id == Some(baseline.request_id)
                    && request.request_id > baseline.request_id
                    && request.session_digest == baseline.session_digest
                    && request.cell_digest == baseline.cell_digest
                    && request.invocation_id == Some(selection.invocation_id)
                    && selection.session_digest == baseline.session_digest
                    && selection.cell_digest == baseline.cell_digest
                    && request.query_digest == Some(selection.query_digest)
                    && request.action_digest == Some(selection.action_digest)
                    && request.source.as_deref() == Some(selection.source.as_str());
                if !is_current {
                    return failed("terminal request did not match the accepted selection".into());
                }
                let counts = match counts {
                    Ok(counts) => counts,
                    Err(error) => return failed(error),
                };
                let source_digest = digest(&[selection.source.as_str()]);
                let before = root_evidence(
                    &baseline.root,
                    selection.action_digest,
                    baseline.counts.matching_history(
                        selection.history_query_digest,
                        selection.history_action_digest,
                    ),
                    baseline.counts.source_history(
                        selection.history_query_digest,
                        selection.history_action_digest,
                        source_digest,
                    ),
                    baseline.counts.usage(selection.action_id_digest),
                );
                let after = root_evidence(
                    &root,
                    selection.action_digest,
                    counts.matching_history(
                        selection.history_query_digest,
                        selection.history_action_digest,
                    ),
                    counts.source_history(
                        selection.history_query_digest,
                        selection.history_action_digest,
                        source_digest,
                    ),
                    counts.usage(selection.action_id_digest),
                );
                QueryObservationResponse {
                    schema_version: 1,
                    request_id: request.request_id,
                    phase: request.phase,
                    status: "captured".into(),
                    error: None,
                    session_digest: baseline.session_digest,
                    cell_digest: baseline.cell_digest,
                    invocation_id: Some(selection.invocation_id),
                    baseline_request_id: Some(baseline.request_id),
                    baseline_frame_ordinal: Some(baseline.frame_ordinal),
                    observed_frame_ordinal: self.frame_ordinal,
                    baseline_state_digest: Some(baseline.root.state_digest),
                    query_digest: Some(selection.query_digest),
                    action_digest: Some(selection.action_digest),
                    source: Some(selection.source),
                    before: Some(before),
                    after: Some(after),
                }
            }
        }
    }
}

fn root_evidence(
    root: &QueryOrdinaryRootCapture,
    action_digest: u64,
    matching_history_count: usize,
    source_history_count: usize,
    usage_count: u32,
) -> QueryRootStateEvidence {
    QueryRootStateEvidence {
        state_digest: root.state_digest,
        query_digest: root.query_digest,
        action_digest,
        results_digest: root.results_digest,
        results_count: root.results_count,
        selected_index: root.selected_index,
        grid_layout: root.grid_layout,
        visible: root.visible,
        restore: root.restore,
        visibility_revision: root.visibility_revision,
        focus_query: root.focus_query,
        move_cursor_end: root.move_cursor_end,
        last_results_valid: root.last_results_valid,
        last_search_query_digest: root.last_search_query_digest,
        suggestions_digest: root.suggestions_digest,
        autocomplete_index: root.autocomplete_index,
        query_history_digest: root.query_history_digest,
        matching_history_count,
        source_history_count,
        usage_count,
    }
}

fn write_response<T: Serialize>(path: &Path, response: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(response).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err("observation response exceeded its byte bound".into());
    }
    if path.exists() {
        std::fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    let temp = path_with_suffix(path, ".tmp");
    std::fs::write(&temp, bytes).map_err(|error| error.to_string())?;
    std::fs::rename(&temp, path).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        error.to_string()
    })
}

fn digest_feed(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
    hash
}

fn digest_text(hash: u64, value: &str) -> u64 {
    let hash = digest_feed(hash, &(value.len() as u64).to_le_bytes());
    digest_feed(hash, value.as_bytes())
}

fn digest_optional_text(hash: u64, value: Option<&str>) -> u64 {
    match value {
        Some(value) => digest_text(digest_feed(hash, &[1]), value),
        None => digest_feed(hash, &[0]),
    }
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn action_digest(action: &crate::actions::Action) -> u64 {
    digest(&[
        action.label.as_str(),
        action.desc.as_str(),
        action.action.as_str(),
        action.args.as_deref().unwrap_or_default(),
    ])
}

fn history_action_digest(action: &crate::actions::Action) -> u64 {
    digest(&[
        action.action.as_str(),
        action.args.as_deref().unwrap_or_default(),
    ])
}

fn digest(parts: &[&str]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for part in parts {
        for byte in part.as_bytes().iter().copied().chain([0]) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn id_digest(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note_close_request(request_id: u64, after: u64) -> NoteCloseObservationRequest {
        NoteCloseObservationRequest {
            schema_version: 1,
            fixture: NoteCloseFixture::Q11,
            request_id,
            run_nonce: [71, 73],
            expected_hwnd: 42,
            expected_pid: 202,
            expected_generation: Some(3),
            after_frame_ordinal: after,
        }
    }

    #[test]
    fn note_close_mailbox_publishes_actual_frame_and_rejects_stale_or_wrong_root_without_query_mutation()
     {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("observation");
        let mut mailbox = QueryObservationMailbox::isolated_note_close_test(base.clone());
        mailbox.frame_ordinal = 1;
        let baseline = mailbox.apply_request(
            request(QueryObservationPhase::Baseline, 10, &identity()),
            root(10),
            Ok(QueryObservationCounts::default()),
        );
        assert_eq!(baseline.status, "captured");
        let original_baseline = mailbox.baseline.clone();
        let original_authoring_id = mailbox.last_authoring_request_id;
        let request_path = path_with_suffix(&base, NOTE_CLOSE_REQUEST_SUFFIX);
        let response_path = path_with_suffix(&base, NOTE_CLOSE_RESPONSE_SUFFIX);
        let owner = NoteCloseRootIdentity {
            hwnd: 42,
            process_id: 202,
            generation: 3,
        };
        let snapshot = NoteCloseSnapshot {
            client_size: [900, 650],
            open_note_count: 0,
            sole_note: None,
        };
        for (request, supplied, expected_error) in [
            (note_close_request(1, 0), Some(owner.clone()), None),
            (
                note_close_request(1, 0),
                Some(owner.clone()),
                Some(NoteCloseObservationError::StaleRequest),
            ),
            (
                note_close_request(2, 9),
                Some(owner.clone()),
                Some(NoteCloseObservationError::StaleFrame),
            ),
            (
                note_close_request(3, 0),
                Some(NoteCloseRootIdentity {
                    hwnd: 43,
                    ..owner.clone()
                }),
                Some(NoteCloseObservationError::WrongRoot),
            ),
            (
                note_close_request(4, 0),
                Some(NoteCloseRootIdentity {
                    process_id: 999,
                    ..owner.clone()
                }),
                Some(NoteCloseObservationError::WrongRoot),
            ),
            (
                note_close_request(5, 0),
                Some(NoteCloseRootIdentity {
                    generation: 4,
                    ..owner.clone()
                }),
                Some(NoteCloseObservationError::WrongRoot),
            ),
            (
                note_close_request(6, 0),
                None,
                Some(NoteCloseObservationError::WrongRoot),
            ),
        ] {
            mailbox.advance_frame();
            std::fs::write(&request_path, serde_json::to_vec(&request).unwrap()).unwrap();
            assert!(mailbox.has_note_close_request());
            assert!(mailbox.poll_note_close(supplied.clone(), Some(snapshot.clone())));
            assert!(!request_path.exists());
            let response: NoteCloseObservationResponse =
                serde_json::from_slice(&std::fs::read(&response_path).unwrap()).unwrap();
            assert_eq!(response.request_id, request.request_id);
            assert_eq!(response.run_nonce, request.run_nonce);
            assert_eq!(response.root, supplied);
            assert_eq!(response.observed_frame_ordinal, mailbox.frame_ordinal);
            assert_eq!(response.error, expected_error);
            assert_eq!(
                response.status == NoteCloseObservationStatus::Captured,
                expected_error.is_none()
            );
            assert_eq!(response.snapshot.is_some(), expected_error.is_none());
        }
        assert_eq!(mailbox.last_request_id, 10);
        assert_eq!(mailbox.last_authoring_request_id, original_authoring_id);
        assert_eq!(
            mailbox.baseline.as_ref().unwrap().request_id,
            original_baseline.as_ref().unwrap().request_id
        );
        assert_eq!(
            mailbox.baseline.as_ref().unwrap().root,
            original_baseline.as_ref().unwrap().root
        );
        assert!(mailbox.authoring_baseline.is_none());
        assert!(mailbox.selection.is_none());
    }

    #[test]
    fn note_close_mailbox_bounds_wire_and_never_invents_unavailable_client_or_root() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("observation");
        let mut mailbox = QueryObservationMailbox::isolated_note_close_test(base.clone());
        mailbox.advance_frame();
        let owner = NoteCloseRootIdentity {
            hwnd: 42,
            process_id: 202,
            generation: 3,
        };
        for bytes in [
            b"{bad}".to_vec(),
            vec![b'x'; MAX_REQUEST_BYTES + 1],
            serde_json::to_vec(&serde_json::json!({"schema_version":1,"fixture":"other"})).unwrap(),
        ] {
            std::fs::write(path_with_suffix(&base, NOTE_CLOSE_REQUEST_SUFFIX), bytes).unwrap();
            mailbox.poll_note_close(Some(owner.clone()), None);
            let bytes = std::fs::read(path_with_suffix(&base, NOTE_CLOSE_RESPONSE_SUFFIX)).unwrap();
            assert!(bytes.len() <= MAX_RESPONSE_BYTES);
            let response: NoteCloseObservationResponse = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(response.status, NoteCloseObservationStatus::Failed);
            assert_eq!(
                response.error,
                Some(NoteCloseObservationError::MalformedRequest)
            );
            assert!(response.snapshot.is_none());
            assert!(!mailbox.has_note_close_request());
        }
        let response =
            mailbox.apply_note_close_request(note_close_request(1, 0), Some(owner.clone()), None);
        assert_eq!(
            response.error,
            Some(NoteCloseObservationError::UnavailableClient)
        );
        let mut discovery = note_close_request(2, 0);
        discovery.expected_generation = None;
        let response = mailbox.apply_note_close_request(
            discovery,
            Some(owner.clone()),
            Some(NoteCloseSnapshot {
                client_size: [317, 840],
                open_note_count: 0,
                sole_note: None,
            }),
        );
        assert_eq!(response.status, NoteCloseObservationStatus::Captured);
        assert_eq!(response.root, Some(owner));
        let mut invalid = note_close_request(3, 0);
        invalid.run_nonce = [0, 0];
        assert_eq!(
            mailbox.apply_note_close_request(invalid, None, None).error,
            Some(NoteCloseObservationError::InvalidIdentity)
        );
    }

    fn action() -> crate::actions::Action {
        crate::actions::Action {
            label: "Marker".into(),
            desc: "Acceptance marker".into(),
            action: "acceptance:marker".into(),
            args: Some("nonce-17".into()),
        }
    }

    fn identity() -> crate::radial::handoff::RadialDispatchIdentity {
        crate::radial::handoff::RadialDispatchIdentity {
            session_id: crate::radial::model::SessionId::new("query-session-1"),
            selected_cell_id: "qa-execute-first".into(),
            invocation_id: crate::radial::model::InvocationId(31),
            session_generation: 4,
            token: crate::radial::session::DispatchToken {
                session_generation: 4,
                ordinal: 2,
            },
            config_revision: crate::radial::model::ConfigRevision(8),
            preparation_generation: crate::radial::bindings::PreparationGeneration(9),
        }
    }

    fn root(query_digest: u64) -> QueryOrdinaryRootCapture {
        QueryOrdinaryRootCapture {
            state_digest: query_digest.wrapping_add(100),
            query_digest,
            results_digest: 201,
            results_count: 2,
            selected_index: Some(1),
            grid_layout: true,
            visible: false,
            restore: false,
            visibility_revision: 7,
            focus_query: false,
            move_cursor_end: false,
            last_results_valid: true,
            last_search_query_digest: 202,
            suggestions_digest: 203,
            autocomplete_index: 0,
            query_history_digest: 204,
        }
    }

    fn request(
        phase: QueryObservationPhase,
        request_id: u64,
        identity: &crate::radial::handoff::RadialDispatchIdentity,
    ) -> QueryObservationRequest {
        // Match the raw-byte ID digest used by production traces and the
        // native hover acknowledgement, not the NUL-delimited multipart hash.
        let runner_raw_id_digest = |value: &str| {
            value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
            })
        };
        let session_digest = runner_raw_id_digest(identity.session_id.as_str());
        let cell_digest = runner_raw_id_digest(&identity.selected_cell_id);
        match phase {
            QueryObservationPhase::Baseline => QueryObservationRequest {
                schema_version: 1,
                request_id,
                phase,
                baseline_request_id: None,
                session_digest,
                cell_digest,
                invocation_id: None,
                query_digest: None,
                action_digest: None,
                source: None,
            },
            QueryObservationPhase::Terminal => {
                let action = action();
                QueryObservationRequest {
                    schema_version: 1,
                    request_id,
                    phase,
                    baseline_request_id: Some(request_id - 1),
                    session_digest,
                    cell_digest,
                    invocation_id: Some(identity.invocation_id.0),
                    query_digest: Some(digest(&["selected query"])),
                    action_digest: Some(action_digest(&action)),
                    source: Some(crate::commands::ActivationSource::Click.label().into()),
                }
            }
        }
    }

    #[test]
    fn radial_identity_digest_matches_production_raw_id_format() {
        let value = "query-session-1";
        let runner_digest = value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
        assert_eq!(id_digest(value), runner_digest);
        assert_ne!(id_digest(value), digest(&[value]));
    }

    #[test]
    fn observation_ordinal_tracks_gui_frames_not_request_count() {
        let identity = identity();
        let mut mailbox = QueryObservationMailbox {
            base_path: Some(PathBuf::from("isolated-test-mailbox")),
            ..QueryObservationMailbox::default()
        };
        mailbox.advance_frame();
        mailbox.advance_frame();
        mailbox.advance_frame();
        let response = mailbox.apply_request(
            request(QueryObservationPhase::Baseline, 1, &identity),
            root(10),
            Ok(QueryObservationCounts::default()),
        );
        assert_eq!(response.baseline_frame_ordinal, Some(3));
        assert_eq!(response.observed_frame_ordinal, 3);
    }

    fn matching_history_entry(action: &crate::actions::Action) -> crate::history::HistoryEntry {
        crate::history::HistoryEntry {
            query: "history query".into(),
            query_lc: "history query".into(),
            action: action.clone(),
            source: Some("click".into()),
            timestamp: 1,
        }
    }

    fn authoring_editor() -> AuthoringEditorObservation {
        AuthoringEditorObservation {
            open: true,
            session_id: 7,
            generation: 11,
            selected_target_digest: 13,
            selected_cell_digest: 17,
            selection_kind: AuthoringSelectionKind::Cell,
            selected_member_count: 1,
            selected_members_digest: 17,
            selected_member_target_digests: vec![117],
            primary_cell_digest: 17,
            range_anchor_digest: 17,
            primary_target_digest: 117,
            range_anchor_target_digest: 117,
            navigation_path_digest: 0,
            navigation_menu_id_digests: Vec::new(),
            navigation_edge_digests: Vec::new(),
            pending_assets_digest: 0,
            designer_filter_digest: 0,
            designer_search_hit_count: 0,
            document_digest: 19,
            assigned_binding_digest: 23,
            properties_staged_digest: None,
            draft_dirty: true,
            properties_popup_open: false,
            properties_dirty: false,
            undo_depth: 2,
            redo_depth: 0,
            initial_snapshot_pending: false,
            action_editor: Some(ActionEditorObservation {
                surface: "properties".into(),
                editor_session_id: 7,
                draft_generation: 11,
                stable_target_digest: 13,
                editor_epoch: 29,
                edit_generation: 31,
                query_generation: 37,
                query_request_generation: 41,
                search_request_generation: 43,
                test_request_generation: 47,
                query_digest: digest(&["private authoring query"]),
                authored_input_digest: 53,
                assigned_binding_digest: 23,
                selected_binding_digest: 0,
                search_pending: false,
                test_pending: false,
                result_count: 0,
                results_digest: 53,
            }),
        }
    }

    fn authoring_request(
        phase: AuthoringObservationPhase,
        request_id: u64,
        baseline_request_id: Option<u64>,
    ) -> AuthoringObservationRequest {
        AuthoringObservationRequest {
            schema_version: AUTHORING_SCHEMA_VERSION,
            request_id,
            phase,
            baseline_request_id,
        }
    }

    #[test]
    fn baseline_is_frozen_and_terminal_captures_late_history_usage_and_root_state() {
        let identity = identity();
        let action = action();
        let mut mailbox = QueryObservationMailbox::default();
        let mut baseline_root = root(10);
        mailbox.frame_ordinal = 1;
        let baseline = mailbox.apply_request(
            request(QueryObservationPhase::Baseline, 10, &identity),
            baseline_root.clone(),
            QueryObservationCounts::capture(
                &[matching_history_entry(&action)],
                &HashMap::from([(action.action.clone(), 4)]),
            ),
        );
        assert_eq!(baseline.status, "captured");
        mailbox.bind_selection(
            &identity,
            digest(&["selected query"]),
            "history query",
            &action,
            crate::commands::ActivationSource::Click,
        );

        // The GUI's ordinary query/results state is still preserved while the
        // command records one history and usage effect after the baseline.
        mailbox.frame_ordinal = 2;
        let mut terminal_root = baseline_root.clone();
        terminal_root.state_digest += 0;
        let terminal = mailbox.apply_request(
            request(QueryObservationPhase::Terminal, 11, &identity),
            terminal_root,
            QueryObservationCounts::capture(
                &[
                    matching_history_entry(&action),
                    matching_history_entry(&action),
                ],
                &HashMap::from([(action.action.clone(), 5)]),
            ),
        );
        let before = terminal.before.expect("frozen baseline evidence");
        let after = terminal.after.expect("live terminal evidence");
        assert_eq!(before.matching_history_count, 1);
        assert_eq!(before.source_history_count, 1);
        assert_eq!(before.usage_count, 4);
        assert_eq!(after.matching_history_count, 2);
        assert_eq!(after.source_history_count, 2);
        assert_eq!(after.usage_count, 5);
        assert_eq!(before.state_digest, after.state_digest);
        assert_eq!(before.query_digest, after.query_digest);

        // A late ordinary-state mutation is visible in the terminal record;
        // the runner compares it with the synchronous action snapshot.
        baseline_root.query_digest = 99;
        assert_ne!(baseline_root.query_digest, after.query_digest);
    }

    #[test]
    fn terminal_rejects_stale_selection_and_replayed_request_ids() {
        let identity = identity();
        let action = action();
        let mut mailbox = QueryObservationMailbox::default();
        mailbox.frame_ordinal = 1;
        let _ = mailbox.apply_request(
            request(QueryObservationPhase::Baseline, 20, &identity),
            root(10),
            Ok(QueryObservationCounts::default()),
        );
        mailbox.bind_selection(
            &identity,
            digest(&["selected query"]),
            "history query",
            &action,
            crate::commands::ActivationSource::Click,
        );
        mailbox.frame_ordinal = 2;
        let mut stale = request(QueryObservationPhase::Terminal, 21, &identity);
        stale.invocation_id = Some(identity.invocation_id.0 + 1);
        let response =
            mailbox.apply_request(stale, root(10), Ok(QueryObservationCounts::default()));
        assert_eq!(response.status, "failed");
        assert!(
            response
                .error
                .as_deref()
                .is_some_and(|error| error.contains("did not match"))
        );

        let replay = mailbox.apply_request(
            request(QueryObservationPhase::Baseline, 20, &identity),
            root(10),
            Ok(QueryObservationCounts::default()),
        );
        assert_eq!(replay.status, "failed");
        assert!(
            replay
                .error
                .as_deref()
                .is_some_and(|error| error.contains("stale or duplicated"))
        );
    }

    #[test]
    fn observation_summary_overflow_fails_closed() {
        let entries = (0..=MAX_SUMMARY_KEYS)
            .map(|index| crate::history::HistoryEntry {
                query: format!("query-{index}"),
                query_lc: format!("query-{index}"),
                action: crate::actions::Action {
                    label: format!("Action {index}"),
                    desc: "Overflow fixture".into(),
                    action: format!("action:{index}"),
                    args: None,
                },
                source: Some("click".into()),
                timestamp: index as i64,
            })
            .collect::<Vec<_>>();
        assert!(QueryObservationCounts::capture(&entries, &HashMap::new()).is_err());
    }

    #[test]
    fn unavailable_history_does_not_become_an_empty_successful_snapshot() {
        let error = QueryObservationCounts::capture_available_history(None, &HashMap::new())
            .expect_err("an unavailable history lock must fail the effect snapshot");
        assert!(error.contains("history is unavailable"));

        let empty =
            QueryObservationCounts::capture_available_history(Some(Vec::new()), &HashMap::new())
                .expect("a genuinely empty available history is a valid snapshot");
        assert_eq!(empty.summary().history_entries, 0);
    }

    #[test]
    fn disabled_mailbox_does_not_poll_or_schedule_observation_work() {
        let mut mailbox = QueryObservationMailbox::from_environment(false);
        assert!(!mailbox.enabled());
        assert!(!mailbox.has_request());
        assert!(!mailbox.poll(root(10), Ok(QueryObservationCounts::default())));
    }

    #[test]
    fn oversized_request_is_removed_and_acknowledged_as_failed() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("observation");
        std::fs::write(
            path_with_suffix(&base, ".request.json"),
            vec![b'x'; MAX_REQUEST_BYTES + 1],
        )
        .unwrap();
        let mut mailbox = QueryObservationMailbox {
            base_path: Some(base.clone()),
            ..QueryObservationMailbox::default()
        };
        assert!(mailbox.poll(root(10), Ok(QueryObservationCounts::default())));
        assert!(!path_with_suffix(&base, ".request.json").exists());
        let response: QueryObservationResponse = serde_json::from_slice(
            &std::fs::read(path_with_suffix(&base, ".response.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(response.status, "failed");
        assert!(
            response
                .error
                .as_deref()
                .is_some_and(|error| error.contains("exceeded its bound"))
        );
    }

    #[test]
    fn authoring_file_mailbox_captures_full_baseline_and_terminal_without_runtime_selection() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("observation");
        let mut mailbox = QueryObservationMailbox {
            base_path: Some(base.clone()),
            ..QueryObservationMailbox::default()
        };
        let action = action();
        let history = vec![matching_history_entry(&action)];
        let usage = HashMap::from([(action.action.clone(), 3), ("other:usage".into(), 5)]);

        let request_path = path_with_suffix(&base, AUTHORING_REQUEST_SUFFIX);
        std::fs::write(
            &request_path,
            serde_json::to_vec(&authoring_request(
                AuthoringObservationPhase::Baseline,
                68,
                None,
            ))
            .unwrap(),
        )
        .unwrap();
        assert!(!mailbox.has_request());
        assert!(mailbox.has_authoring_request());
        mailbox.advance_frame();
        assert!(mailbox.poll_authoring(
            root(10),
            QueryObservationCounts::capture(&history, &usage),
            Ok(authoring_editor()),
        ));
        let baseline: AuthoringObservationResponse = serde_json::from_slice(
            &std::fs::read(path_with_suffix(&base, AUTHORING_RESPONSE_SUFFIX)).unwrap(),
        )
        .unwrap();
        assert_eq!(baseline.status, "captured");
        assert_eq!(baseline.phase, AuthoringObservationPhase::Baseline);
        assert_eq!(baseline.baseline_request_id, None);
        assert_eq!(baseline.after, None);
        let frozen = baseline.before.expect("full GUI-owner baseline");
        assert!(frozen.trace_boundary_sequence > frozen.trace_sequence);
        assert_eq!(frozen.effects.history_entries, 1);
        assert_eq!(frozen.effects.history_keys, 1);
        assert_eq!(frozen.effects.usage_entries, 2);
        assert_eq!(frozen.editor, authoring_editor());
        let serialized = serde_json::to_string(&frozen).unwrap();
        assert!(!serialized.contains("private authoring query"));

        let mut snapshot_history = history.clone();
        snapshot_history[0].timestamp += 1;
        let mut snapshot_editor = authoring_editor();
        snapshot_editor.generation += 1;
        snapshot_editor.draft_dirty = false;
        snapshot_editor.assigned_binding_digest += 1;
        snapshot_editor.properties_dirty = false;
        snapshot_editor.properties_staged_digest = None;
        let snapshot_generation = snapshot_editor.generation;
        let snapshot_binding_digest = snapshot_editor.assigned_binding_digest;
        let action_editor = snapshot_editor.action_editor.as_mut().unwrap();
        action_editor.surface = "inspector".into();
        action_editor.draft_generation = snapshot_generation;
        action_editor.assigned_binding_digest = snapshot_binding_digest;

        std::fs::write(
            &request_path,
            serde_json::to_vec(&authoring_request(
                AuthoringObservationPhase::Snapshot,
                69,
                None,
            ))
            .unwrap(),
        )
        .unwrap();
        mailbox.advance_frame();
        assert!(mailbox.poll_authoring(
            root(20),
            QueryObservationCounts::capture(
                &snapshot_history,
                &HashMap::from([(action.action.clone(), 4)]),
            ),
            Ok(snapshot_editor.clone()),
        ));
        let snapshot: AuthoringObservationResponse = serde_json::from_slice(
            &std::fs::read(path_with_suffix(&base, AUTHORING_RESPONSE_SUFFIX)).unwrap(),
        )
        .unwrap();
        assert_eq!(snapshot.status, "captured");
        assert_eq!(snapshot.phase, AuthoringObservationPhase::Snapshot);
        assert_eq!(snapshot.request_id, 69);
        assert_eq!(snapshot.baseline_request_id, None);
        assert_eq!(snapshot.after, None);
        let live_after_apply = snapshot.before.expect("live post-Apply owner snapshot");
        assert!(live_after_apply.trace_boundary_sequence > live_after_apply.trace_sequence);
        assert_ne!(live_after_apply.root, frozen.root);
        assert_ne!(live_after_apply.effects, frozen.effects);
        assert_eq!(live_after_apply.editor, snapshot_editor);
        let retained = mailbox
            .authoring_baseline
            .as_ref()
            .expect("snapshot must leave the browsing baseline pending");
        assert_eq!(retained.request_id, 68);
        assert_eq!(retained.frame_ordinal, frozen.frame_ordinal);
        assert_eq!(retained.trace_sequence, frozen.trace_sequence);
        assert_eq!(
            retained.trace_boundary_sequence,
            frozen.trace_boundary_sequence
        );
        assert_eq!(retained.root, frozen.root);
        assert_eq!(retained.effects, frozen.effects);
        assert_eq!(retained.editor, frozen.editor);

        std::fs::write(
            &request_path,
            serde_json::to_vec(&authoring_request(
                AuthoringObservationPhase::Terminal,
                70,
                Some(68),
            ))
            .unwrap(),
        )
        .unwrap();
        mailbox.advance_frame();
        assert!(mailbox.poll_authoring(
            root(10),
            QueryObservationCounts::capture(&history, &usage),
            Ok(authoring_editor()),
        ));
        let terminal: AuthoringObservationResponse = serde_json::from_slice(
            &std::fs::read(path_with_suffix(&base, AUTHORING_RESPONSE_SUFFIX)).unwrap(),
        )
        .unwrap();
        assert_eq!(terminal.status, "captured");
        assert_eq!(terminal.phase, AuthoringObservationPhase::Terminal);
        assert_eq!(terminal.request_id, 70);
        assert_eq!(terminal.baseline_request_id, Some(68));
        assert!(terminal.observed_frame_ordinal > frozen.frame_ordinal);
        assert_eq!(terminal.before, Some(frozen.clone()));
        let after = terminal.after.expect("later GUI-owner terminal evidence");
        assert!(after.trace_boundary_sequence > after.trace_sequence);
        assert!(after.trace_sequence >= frozen.trace_boundary_sequence);
        assert!(after.trace_boundary_sequence > frozen.trace_boundary_sequence);
        assert_eq!(after.frame_ordinal, terminal.observed_frame_ordinal);
        assert!(after.frame_ordinal > frozen.frame_ordinal);
        assert_eq!(after.root, frozen.root);
        assert_eq!(after.effects, frozen.effects);
        assert_eq!(after.editor, frozen.editor);
        assert!(!path_with_suffix(&base, ".response.json").exists());
    }

    #[test]
    fn authoring_terminal_rejects_wrong_baseline_without_losing_frozen_evidence() {
        let mut mailbox = QueryObservationMailbox {
            base_path: Some(PathBuf::from("enabled-authoring-observation-test")),
            ..QueryObservationMailbox::default()
        };
        mailbox.advance_frame();
        let baseline = mailbox.apply_authoring_request(
            authoring_request(AuthoringObservationPhase::Baseline, 10, None),
            root(10),
            Ok(QueryObservationCounts::default()),
            Ok(authoring_editor()),
        );
        assert_eq!(baseline.status, "captured");
        mailbox.advance_frame();
        let snapshot = mailbox.apply_authoring_request(
            authoring_request(AuthoringObservationPhase::Snapshot, 11, None),
            root(20),
            Ok(QueryObservationCounts::default()),
            Ok(authoring_editor()),
        );
        assert_eq!(snapshot.status, "captured");
        assert_eq!(snapshot.phase, AuthoringObservationPhase::Snapshot);
        assert_eq!(mailbox.authoring_baseline.as_ref().unwrap().request_id, 10);
        mailbox.advance_frame();
        let stale = mailbox.apply_authoring_request(
            authoring_request(AuthoringObservationPhase::Terminal, 12, Some(9)),
            root(20),
            Ok(QueryObservationCounts::default()),
            Ok(authoring_editor()),
        );
        assert_eq!(stale.status, "failed");
        assert!(mailbox.authoring_baseline.is_some());
        assert!(
            stale
                .error
                .as_deref()
                .is_some_and(|error| error.contains("baseline"))
        );

        let snapshot_as_baseline = mailbox.apply_authoring_request(
            authoring_request(AuthoringObservationPhase::Terminal, 13, Some(11)),
            root(20),
            Ok(QueryObservationCounts::default()),
            Ok(authoring_editor()),
        );
        assert_eq!(snapshot_as_baseline.status, "failed");
        assert!(mailbox.authoring_baseline.is_some());

        let replay = mailbox.apply_authoring_request(
            authoring_request(AuthoringObservationPhase::Baseline, 10, None),
            root(10),
            Ok(QueryObservationCounts::default()),
            Ok(authoring_editor()),
        );
        assert_eq!(replay.status, "failed");
        assert!(
            replay
                .error
                .as_deref()
                .is_some_and(|error| error.contains("stale"))
        );

        mailbox.advance_frame();
        let terminal = mailbox.apply_authoring_request(
            authoring_request(AuthoringObservationPhase::Terminal, 14, Some(10)),
            root(10),
            Ok(QueryObservationCounts::default()),
            Ok(authoring_editor()),
        );
        assert_eq!(terminal.status, "captured");
        assert_eq!(terminal.baseline_request_id, Some(10));
        let before = terminal.before.expect("baseline evidence retained");
        let after = terminal.after.expect("terminal evidence captured");
        assert!(after.frame_ordinal > before.frame_ordinal);
    }

    #[test]
    fn authoring_protocol_fails_closed_for_schema_and_live_state_overflow() {
        let mut mailbox = QueryObservationMailbox {
            base_path: Some(PathBuf::from("enabled-authoring-observation-test")),
            ..QueryObservationMailbox::default()
        };
        mailbox.advance_frame();
        let mut wrong_schema = authoring_request(AuthoringObservationPhase::Baseline, 1, None);
        wrong_schema.schema_version = AUTHORING_SCHEMA_VERSION + 1;
        let malformed = mailbox.apply_authoring_request(
            wrong_schema,
            root(10),
            Ok(QueryObservationCounts::default()),
            Ok(authoring_editor()),
        );
        assert_eq!(malformed.status, "failed");
        assert!(mailbox.authoring_baseline.is_none());

        let overflow = mailbox.apply_authoring_request(
            authoring_request(AuthoringObservationPhase::Baseline, 2, None),
            root(10),
            Err("history observation exceeded its bounded key capacity".into()),
            Ok(authoring_editor()),
        );
        assert_eq!(overflow.status, "failed");
        assert!(
            overflow
                .error
                .as_deref()
                .is_some_and(|error| error.contains("bounded"))
        );
        assert!(mailbox.authoring_baseline.is_none());

        let unavailable_editor = mailbox.apply_authoring_request(
            authoring_request(AuthoringObservationPhase::Baseline, 3, None),
            root(10),
            Ok(QueryObservationCounts::default()),
            Err("Designer owner is unavailable".into()),
        );
        assert_eq!(unavailable_editor.status, "failed");
        assert!(mailbox.authoring_baseline.is_none());

        let baseline = mailbox.apply_authoring_request(
            authoring_request(AuthoringObservationPhase::Baseline, 4, None),
            root(10),
            Ok(QueryObservationCounts::default()),
            Ok(authoring_editor()),
        );
        assert_eq!(baseline.status, "captured");
        mailbox.advance_frame();
        let terminal_overflow = mailbox.apply_authoring_request(
            authoring_request(AuthoringObservationPhase::Terminal, 5, Some(4)),
            root(10),
            Err("usage observation exceeded its bounded key capacity".into()),
            Ok(authoring_editor()),
        );
        assert_eq!(terminal_overflow.status, "failed");
        assert!(mailbox.authoring_baseline.is_some());

        let terminal = mailbox.apply_authoring_request(
            authoring_request(AuthoringObservationPhase::Terminal, 6, Some(4)),
            root(10),
            Ok(QueryObservationCounts::default()),
            Ok(authoring_editor()),
        );
        assert_eq!(terminal.status, "captured");
        assert!(mailbox.authoring_baseline.is_none());
    }

    #[test]
    fn authoring_effect_summary_covers_each_history_key_and_usage_entry() {
        let action = action();
        let history = vec![matching_history_entry(&action)];
        let usage = HashMap::from([(action.action.clone(), 3), ("other:usage".into(), 5)]);
        let initial = QueryObservationCounts::capture(&history, &usage)
            .unwrap()
            .summary();

        let mut changed_history = history.clone();
        changed_history[0].source = Some("keyboard".into());
        changed_history[0].timestamp += 1;
        changed_history[0].action.label.push_str(" changed");
        let changed = QueryObservationCounts::capture(&changed_history, &usage)
            .unwrap()
            .summary();
        assert_ne!(initial.history_digest, changed.history_digest);
        assert_eq!(initial.usage_digest, changed.usage_digest);

        let mut changed_usage = usage;
        changed_usage.insert("other:usage".into(), 6);
        let changed = QueryObservationCounts::capture(&history, &changed_usage)
            .unwrap()
            .summary();
        assert_eq!(initial.history_digest, changed.history_digest);
        assert_ne!(initial.usage_digest, changed.usage_digest);
    }

    #[test]
    fn oversized_authoring_request_is_removed_and_acknowledged_as_failed() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("authoring-observation");
        let request_path = path_with_suffix(&base, AUTHORING_REQUEST_SUFFIX);
        std::fs::write(&request_path, vec![b'x'; MAX_REQUEST_BYTES + 1]).unwrap();
        let mut mailbox = QueryObservationMailbox {
            base_path: Some(base.clone()),
            ..QueryObservationMailbox::default()
        };
        mailbox.advance_frame();
        assert!(mailbox.has_authoring_request());
        assert!(mailbox.poll_authoring(
            root(10),
            Ok(QueryObservationCounts::default()),
            Ok(authoring_editor()),
        ));
        assert!(!request_path.exists());
        let response: AuthoringObservationResponse = serde_json::from_slice(
            &std::fs::read(path_with_suffix(&base, AUTHORING_RESPONSE_SUFFIX)).unwrap(),
        )
        .unwrap();
        assert_eq!(response.status, "failed");
        assert!(
            response
                .error
                .as_deref()
                .is_some_and(|error| error.contains("bound"))
        );
    }

    #[test]
    fn authoring_history_digest_preserves_order_and_full_entry_identity() {
        let first = matching_history_entry(&action());
        let mut second = first.clone();
        second.query = "second history query".into();
        second.query_lc = second.query.to_lowercase();
        second.action.label = "Second action label".into();
        second.timestamp = 2;

        let forward =
            QueryObservationCounts::capture(&[first.clone(), second.clone()], &HashMap::new())
                .unwrap()
                .summary();
        let reversed =
            QueryObservationCounts::capture(&[second.clone(), first.clone()], &HashMap::new())
                .unwrap()
                .summary();
        assert_eq!(forward.history_entries, 2);
        assert_ne!(forward.history_digest, reversed.history_digest);

        let mut timestamp_changed = first.clone();
        timestamp_changed.timestamp += 1;
        let timestamp = QueryObservationCounts::capture(&[timestamp_changed], &HashMap::new())
            .unwrap()
            .summary();
        let original = QueryObservationCounts::capture(&[first.clone()], &HashMap::new())
            .unwrap()
            .summary();
        assert_ne!(original.history_digest, timestamp.history_digest);

        let mut label_changed = first.clone();
        label_changed.action.label.push_str(" different");
        let label = QueryObservationCounts::capture(&[label_changed], &HashMap::new())
            .unwrap()
            .summary();
        assert_ne!(original.history_digest, label.history_digest);

        let mut args_presence_changed = first;
        args_presence_changed.action.args = Some(String::new());
        let args = QueryObservationCounts::capture(&[args_presence_changed], &HashMap::new())
            .unwrap()
            .summary();
        assert_ne!(original.history_digest, args.history_digest);
    }
}
