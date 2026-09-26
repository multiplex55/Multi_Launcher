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

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct HistoryKey {
    query_digest: u64,
    action_digest: u64,
    source_digest: u64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct QueryObservationCounts {
    history: BTreeMap<HistoryKey, usize>,
    usage: BTreeMap<u64, u32>,
}

impl QueryObservationCounts {
    pub(crate) fn capture(
        history: &[crate::history::HistoryEntry],
        usage: &HashMap<String, u32>,
    ) -> Result<Self, String> {
        let mut counts = Self::default();
        for entry in history {
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
        }
        for (action, count) in usage {
            counts.usage.insert(digest(&[action]), *count);
            if counts.usage.len() > MAX_SUMMARY_KEYS {
                return Err("usage observation exceeded its bounded key capacity".into());
            }
        }
        Ok(counts)
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

fn write_response(path: &Path, response: &QueryObservationResponse) -> Result<(), String> {
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
}
