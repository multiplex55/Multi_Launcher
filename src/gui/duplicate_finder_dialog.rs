use crate::duplicate_finder::{
    DiscoveryDiagnostic, DiscoverySnapshot, DiscoveryStage, DiscoveryStatus,
    DuplicateFinderSession, DuplicateFinderSessionError, RootScanStatus, VerifiedKnownFile,
};
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const WINDOW_TITLE: &str = "Duplicate Finder";
const FOOTER_SEPARATOR_SPACING: f32 = 6.0;
const RESULT_ROW_HEIGHT: f32 = 28.0;
const COPY_PATHS_BUTTON_WIDTH: f32 = 88.0;
const OPEN_PATH_BUTTON_WIDTH: f32 = 64.0;
const REVEAL_PATH_BUTTON_WIDTH: f32 = 76.0;
const RESULT_SIZE_LABEL_WIDTH: f32 = 88.0;
const MAX_RESULT_ROWS: usize = 350_000;
const MAX_COPY_PATHS: usize = 256;
const MAX_COPY_TEXT_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
pub(crate) struct CandidateProof {
    root: crate::file_operations::DuplicateDiscoveryRootIdentity,
    generation: u64,
    path: PathBuf,
    identity: crate::file_operations::NativeFileIdentity,
    length: u64,
    modified: std::time::SystemTime,
}

impl CandidateProof {
    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    pub(super) fn fixture_generation(&self) -> u64 {
        self.generation
    }

    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    pub(super) fn fixture_path(&self) -> &Path {
        &self.path
    }

    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    pub(super) fn fixture_identity(&self) -> &crate::file_operations::NativeFileIdentity {
        &self.identity
    }
}

pub(crate) trait DuplicateFinderActions: Send + Sync {
    fn revalidate(&self, proof: &CandidateProof) -> Result<(), String>;
    fn open(&self, path: &Path) -> Result<(), String>;
    fn reveal(&self, path: &Path) -> Result<(), String>;
    fn copy_paths(&self, text: &str) -> Result<(), String>;
}

pub(crate) struct SystemDuplicateFinderActions;

impl DuplicateFinderActions for SystemDuplicateFinderActions {
    fn revalidate(&self, proof: &CandidateProof) -> Result<(), String> {
        use crate::file_operations::{
            DuplicateDiscoveryContentRequest, RenameScanGeneration, ScanCancellation,
            open_duplicate_discovery_content_file,
        };

        let cancellation = ScanCancellation::new();
        let request = DuplicateDiscoveryContentRequest {
            root: proof.root.clone(),
            generation: RenameScanGeneration(proof.generation),
            path: proof.path.clone(),
            identity: proof.identity,
            length: proof.length,
            modified: proof.modified,
        };
        let file = open_duplicate_discovery_content_file(request, &cancellation)
            .map_err(|error| format!("The scanned file is no longer current: {error}"))?;
        file.revalidate(&cancellation)
            .map_err(|error| format!("The scanned file changed before the action: {error}"))?;
        drop(file);
        Ok(())
    }

    fn open(&self, path: &Path) -> Result<(), String> {
        crate::file_search::actions::open_path(path)
            .map_err(|error| format!("Could not open file: {error:#}"))
    }

    fn reveal(&self, path: &Path) -> Result<(), String> {
        crate::file_search::actions::reveal_path(path)
            .map_err(|error| format!("Could not reveal file: {error:#}"))
    }

    fn copy_paths(&self, text: &str) -> Result<(), String> {
        crate::actions::clipboard::set_text(text)
            .map_err(|error| format!("Could not copy paths to the clipboard: {error}"))
    }
}

#[derive(Clone, Copy)]
enum ResultRow {
    VerifiedHeader(usize),
    VerifiedMember { group: usize, member: usize },
    UnverifiedHeader(usize),
    UnverifiedKnownMember { group: usize, member: usize },
    UnverifiedPath { group: usize, path: usize },
}

#[derive(Clone, Copy, Debug)]
struct TestWidgetResponse {
    id: egui::Id,
    layer_id: Option<egui::LayerId>,
    rect: egui::Rect,
    response_rect: egui::Rect,
    enabled: bool,
}

#[cfg(any(test, feature = "dev-smoke-binaries"))]
#[derive(Clone, Debug)]
pub(crate) enum SmokeResponseKind {
    Control(&'static str),
    ResultAction {
        action: &'static str,
        group_index: usize,
        member_index: Option<usize>,
    },
}

#[cfg(any(test, feature = "dev-smoke-binaries"))]
#[derive(Clone, Debug)]
pub(crate) struct SmokeResponseCandidate {
    pub(crate) kind: SmokeResponseKind,
    pub(crate) id: egui::Id,
    pub(crate) layer_id: egui::LayerId,
    pub(crate) rect: egui::Rect,
    pub(crate) response_rect: egui::Rect,
    pub(crate) enabled: bool,
    pub(crate) interaction_eligible: bool,
    pub(crate) generation: Option<u64>,
}

#[cfg(any(test, feature = "dev-smoke-binaries"))]
#[derive(Clone, Debug)]
struct TestResultActionResponse {
    action: &'static str,
    group_kind: &'static str,
    group_index: usize,
    member_index: Option<usize>,
    generation: u64,
    response: TestWidgetResponse,
}

enum FinderIntent {
    PickRoot,
    AddRoot(PathBuf),
    RemoveRoot(PathBuf),
    Start,
    Cancel,
    Known {
        snapshot: Arc<DiscoverySnapshot>,
        size_group: usize,
        verified: VerifiedKnownFile,
        action: KnownFileAction,
        namespace: &'static str,
        group: usize,
        member: usize,
    },
    Copy {
        snapshot: Arc<DiscoverySnapshot>,
        group: usize,
    },
    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    ReleaseGate,
}
struct FinderHostFrame {
    root: super::utility_host::ViewportPassProvenance,
    owner: super::utility_host::ViewportPassProvenance,
    host: super::utility_host::HostIdentity,
    epoch: u64,
    target_revision: u64,
    previous_editor: String,
    intents: Vec<FinderIntent>,
    copies: Vec<String>,
    close: bool,
    request: Option<super::utility_host::UtilityPresentation>,
    info: Option<egui::ViewportInfo>,
    native: bool,
}
#[derive(Default)]
pub(super) struct DuplicateFinderHostOutcome {
    pub(super) closed: bool,
    pub(super) show_root: bool,
    pub(super) copies: Vec<String>,
}

pub struct DuplicateFinderDialogState {
    pub open: bool,
    host: super::utility_host::UtilityHostState,
    host_epoch: u64,
    host_pending: Option<FinderHostFrame>,
    host_os_close_pending: bool,
    host_focus: bool,
    host_jump: crate::gui::jump_mode::ui::JumpModeUi,
    host_jump_frame: Option<u64>,
    staged_intents: Vec<FinderIntent>,
    #[cfg(test)]
    host_button: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_folder_picks: std::collections::VecDeque<Option<PathBuf>>,
    reopen_pending: bool,
    session: DuplicateFinderSession,
    selected_roots: Vec<PathBuf>,
    root_editor: String,
    status: Option<String>,
    target_revision: u64,
    rows_snapshot: Option<Arc<DiscoverySnapshot>>,
    rows: Vec<ResultRow>,
    rows_truncated: bool,
    actions: Arc<dyn DuplicateFinderActions>,
    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    first_read_checkpoint: Option<Arc<crate::duplicate_finder::FirstReadCheckpointGate>>,
    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    release_fixture_gate_on_close: bool,
    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    fixture_action_receipts: Vec<serde_json::Value>,
    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    responses: std::collections::BTreeMap<&'static str, TestWidgetResponse>,
    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    responses_interaction_eligible: bool,
    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    result_action_responses: Vec<TestResultActionResponse>,
    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    unavailable_result_rows: Vec<serde_json::Value>,
    #[cfg(test)]
    test_snapshot_override: Option<Arc<DiscoverySnapshot>>,
}

impl DuplicateFinderDialogState {
    pub(crate) fn attach_activity_center(&mut self, center: crate::activity::ActivityCenter) {
        self.session.attach_activity_center(center);
    }

    pub fn new(ctx: &egui::Context) -> Self {
        let wake_context = ctx.clone();
        Self::from_session(DuplicateFinderSession::with_wake(Arc::new(move || {
            wake_context.request_repaint();
        })))
    }

    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    pub(crate) fn new_with_first_read_checkpoint(
        ctx: &egui::Context,
        checkpoint: Arc<crate::duplicate_finder::FirstReadCheckpointGate>,
    ) -> Self {
        let wake_context = ctx.clone();
        let mut state =
            Self::from_session(DuplicateFinderSession::with_wake_and_first_read_checkpoint(
                Arc::new(move || wake_context.request_repaint()),
                Arc::clone(&checkpoint),
            ));
        state.first_read_checkpoint = Some(checkpoint);
        state
    }

    fn from_session(session: DuplicateFinderSession) -> Self {
        Self {
            open: false,
            host: super::utility_host::UtilityHostState::docked(),
            host_epoch: 0,
            host_pending: None,
            host_os_close_pending: false,
            host_focus: false,
            host_jump: crate::gui::jump_mode::ui::JumpModeUi::default(),
            host_jump_frame: None,
            staged_intents: Vec::new(),
            #[cfg(test)]
            host_button: None,
            #[cfg(test)]
            test_folder_picks: std::collections::VecDeque::new(),
            reopen_pending: false,
            session,
            selected_roots: Vec::new(),
            root_editor: String::new(),
            status: None,
            target_revision: 0,
            rows_snapshot: None,
            rows: Vec::new(),
            rows_truncated: false,
            actions: Arc::new(SystemDuplicateFinderActions),
            #[cfg(any(test, feature = "dev-smoke-binaries"))]
            first_read_checkpoint: None,
            #[cfg(any(test, feature = "dev-smoke-binaries"))]
            release_fixture_gate_on_close: true,
            #[cfg(any(test, feature = "dev-smoke-binaries"))]
            fixture_action_receipts: Vec::new(),
            #[cfg(any(test, feature = "dev-smoke-binaries"))]
            responses: std::collections::BTreeMap::new(),
            #[cfg(any(test, feature = "dev-smoke-binaries"))]
            responses_interaction_eligible: false,
            #[cfg(any(test, feature = "dev-smoke-binaries"))]
            result_action_responses: Vec::new(),
            #[cfg(any(test, feature = "dev-smoke-binaries"))]
            unavailable_result_rows: Vec::new(),
            #[cfg(test)]
            test_snapshot_override: None,
        }
    }

    pub fn open(&mut self) {
        if self.open {
            return;
        }
        self.host_epoch = self.host_epoch.wrapping_add(1).max(1);
        self.reopen_pending = false;
        if self.session.view().closed {
            match self.session.reopen() {
                Ok(()) => self.status = None,
                Err(DuplicateFinderSessionError::WorkerRetiring) => {
                    self.reopen_pending = true;
                    self.status = Some(session_error_message(
                        &DuplicateFinderSessionError::WorkerRetiring,
                    ));
                }
                Err(error) => self.status = Some(session_error_message(&error)),
            }
        }
        self.open = true;
        self.bump_target_revision();
    }

    pub fn close(&mut self) {
        if !self.open && self.session.view().closed {
            return;
        }
        self.open = false;
        self.host_pending = None;
        self.host_os_close_pending = false;
        self.reopen_pending = false;
        self.session.close();
        self.rows_snapshot = None;
        self.rows.clear();
        self.rows_truncated = false;
        self.status = None;
        self.bump_target_revision();
        #[cfg(any(test, feature = "dev-smoke-binaries"))]
        if self.release_fixture_gate_on_close
            && let Some(checkpoint) = self.first_read_checkpoint.as_ref()
        {
            checkpoint.release();
        }
    }

    pub fn shutdown(&mut self) {
        self.close();
        self.session.close();
    }

    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    pub(crate) fn set_action_sink_for_fixture(&mut self, actions: Arc<dyn DuplicateFinderActions>) {
        self.actions = actions;
    }

    #[cfg(test)]
    fn set_release_fixture_gate_on_close(&mut self, release: bool) {
        self.release_fixture_gate_on_close = release;
    }

    #[cfg(test)]
    fn set_snapshot_for_test(&mut self, snapshot: DiscoverySnapshot) {
        self.test_snapshot_override = Some(Arc::new(snapshot));
    }

    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    pub(crate) fn add_fixture_root(&mut self, root: PathBuf) -> Result<(), String> {
        self.add_selected_root(root)
    }

    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    pub(crate) fn smoke_observation(
        &self,
        fixture_identity: &str,
        ctx: &egui::Context,
        dialog_instance: u64,
    ) -> serde_json::Value {
        let view = self.session.view();
        let frame_id = ctx.cumulative_frame_nr();
        let layer_current = crate::gui::jump_mode::ui::visible_top_middle_layer(ctx)
            == crate::gui::jump_mode::ui::NavigationDialogKind::DuplicateFinder.layer_id();
        let response = |key: &'static str| {
            self.responses
                .get(key)
                .map_or(serde_json::Value::Null, |response| {
                    serde_json::json!({
                        "id": response.id.value(),
                        "enabled": response.enabled,
                        "visible": response.rect.is_positive(),
                        "response_current": false,
                        "interaction_eligible": false,
                        "input_hit_eligible": false,
                        "layer": response.layer_id.map(|layer| {
                            serde_json::json!({
                                "order": format!("{:?}", layer.order),
                                "id": layer.id.value(),
                            })
                        }),
                        "painted_frame": frame_id,
                        "rect": {
                            "min_x": response.rect.min.x,
                            "min_y": response.rect.min.y,
                            "max_x": response.rect.max.x,
                            "max_y": response.rect.max.y,
                        },
                        "response_rect": {
                            "min_x": response.response_rect.min.x,
                            "min_y": response.response_rect.min.y,
                            "max_x": response.response_rect.max.x,
                            "max_y": response.response_rect.max.y,
                        },
                        "layer_current": layer_current,
                    })
                })
        };
        let progress = view.progress.as_ref();
        let distinct_known_files = view
            .snapshot
            .size_groups
            .iter()
            .map(|group| group.known_files.len())
            .sum::<usize>();
        let unverified_path_count = view
            .snapshot
            .size_groups
            .iter()
            .map(|group| group.unverified_paths.len())
            .sum::<usize>();
        let skipped_overlap_roots = view
            .snapshot
            .roots
            .iter()
            .filter(|root| root.status == RootScanStatus::SkippedOverlap)
            .count();
        let verified_group_records = view
            .snapshot
            .verified_duplicate_groups
            .iter()
            .take(64)
            .enumerate()
            .map(|(group_index, group)| {
                let size_group = view.snapshot.size_groups.get(group.size_group_index);
                let members = group
                    .known_files
                    .iter()
                    .take(16)
                    .map(|member| {
                        let known = size_group
                            .and_then(|size_group| {
                                size_group.known_files.get(member.known_file_index)
                            });
                        let alias = known.and_then(|known| known.aliases.get(member.alias_index));
                        serde_json::json!({
                            "known_file_index": member.known_file_index,
                            "alias_index": member.alias_index,
                            "root_index": alias.map(|alias| alias.root_index),
                            "path": alias.and_then(|alias| alias.path.to_str()),
                            "size_bytes": known.map(|known| known.size_bytes),
                            "identity_volume": known.map(|known| known.identity.volume_serial_number),
                            "identity_file_id": known.map(|known| hex::encode(known.identity.file_id)),
                        })
                    })
                    .collect::<Vec<_>>();
                serde_json::json!({
                    "group_index": group_index,
                    "size_group_index": group.size_group_index,
                    "size_bytes": size_group.map(|size_group| size_group.size_bytes),
                    "logical_duplicate_bytes": group.logical_duplicate_bytes,
                    "member_count": group.known_files.len(),
                    "members_truncated": group.known_files.len() > 16,
                    "members": members,
                })
            })
            .collect::<Vec<_>>();
        let root_statuses = view
            .snapshot
            .roots
            .iter()
            .enumerate()
            .map(|(index, root)| {
                serde_json::json!({
                    "root_index": index,
                    "status": root_status_label(root.status),
                    "directories_scanned": root.counts.directories_scanned,
                    "files_seen": root.counts.files_seen,
                    "inaccessible_entries": root.counts.inaccessible_entries,
                    "reparse_entries_excluded": root.counts.reparse_entries_excluded,
                    "cloud_placeholders_excluded": root.counts.cloud_placeholders_excluded,
                })
            })
            .collect::<Vec<_>>();
        let result_actions = self
            .result_action_responses
            .iter()
            .map(|action| {
                serde_json::json!({
                    "id": action.response.id.value(),
                    "action": action.action,
                    "group_kind": action.group_kind,
                    "group_index": action.group_index,
                    "member_index": action.member_index,
                    "generation": action.generation,
                    "enabled": action.response.enabled,
                    "visible": action.response.rect.is_positive(),
                    "response_current": false,
                    "interaction_eligible": false,
                    "input_hit_eligible": false,
                    "layer": action.response.layer_id.map(|layer| {
                        serde_json::json!({
                            "order": format!("{:?}", layer.order),
                            "id": layer.id.value(),
                        })
                    }),
                    "painted_frame": frame_id,
                    "layer_current": layer_current,
                    "rect": {
                        "min_x": action.response.rect.min.x,
                        "min_y": action.response.rect.min.y,
                        "max_x": action.response.rect.max.x,
                        "max_y": action.response.rect.max.y,
                    },
                    "response_rect": {
                        "min_x": action.response.response_rect.min.x,
                        "min_y": action.response.response_rect.min.y,
                        "max_x": action.response.response_rect.max.x,
                        "max_y": action.response.response_rect.max.y,
                    },
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "fixture_identity": fixture_identity,
            "process_id": std::process::id(),
            "frame_id": frame_id,
            "pixels_per_point": ctx.pixels_per_point(),
            "dialog_instance": dialog_instance,
            "session_generation": view.snapshot.generation,
            "status": status_name(view.snapshot.status),
            "busy": view.busy,
            "closed": view.closed,
            "selected_root_count": self.selected_roots.len(),
            "root_statuses": root_statuses,
            "stage": progress.map(|progress| stage_name(progress.stage)),
            "files_seen": progress.map(|progress| progress.counts.files_seen),
            "directories_scanned": progress.map(|progress| progress.counts.directories_scanned),
            "scan_counts": {
                "directories_scanned": view.snapshot.counts.directories_scanned,
                "entries_seen": view.snapshot.counts.entries_seen,
                "files_seen": view.snapshot.counts.files_seen,
                "directories_seen": view.snapshot.counts.directories_seen,
                "other_entries_seen": view.snapshot.counts.other_entries_seen,
                "reparse_entries_excluded": view.snapshot.counts.reparse_entries_excluded,
                "cloud_placeholders_excluded": view.snapshot.counts.cloud_placeholders_excluded,
                "inaccessible_entries": view.snapshot.counts.inaccessible_entries,
            },
            "distinct_known_files": distinct_known_files,
            "unverified_path_count": unverified_path_count,
            "skipped_overlap_roots": skipped_overlap_roots,
            "verified_groups": view.snapshot.duplicate_totals.verified_groups,
            "verified_files": view.snapshot.duplicate_totals.verified_files,
            "logical_duplicate_bytes": view.snapshot.duplicate_totals.logical_duplicate_bytes,
            "unverified_matches": view.snapshot.duplicate_totals.identity_unverified_matches,
            "diagnostic_count": view.snapshot.diagnostics.len(),
            "verified_group_records_truncated": view.snapshot.verified_duplicate_groups.len() > 64,
            "verified_group_records": verified_group_records,
            "active_drained": !view.busy,
            "read_gate_reached": self
                .first_read_checkpoint
                .as_ref()
                .is_some_and(|gate| gate.reached()),
            "read_gate_released": self
                .first_read_checkpoint
                .as_ref()
                .is_some_and(|gate| gate.released()),
            "rows_truncated": self.rows_truncated,
            "responses": {
                "start": response("start"),
                "cancel": response("cancel"),
                "add_root": response("add-root"),
                "pick_root": response("pick-root"),
                "release_gate": response("fixture-release"),
                "root_status_scroll": response("root-status-scroll"),
                "diagnostics_toggle": response("diagnostics-toggle"),
                "diagnostics_scroll": response("diagnostics-scroll"),
                "results_scroll": response("results-scroll"),
                "close": response("close"),
            },
            "result_actions": result_actions,
            "unavailable_result_rows": self.unavailable_result_rows.clone(),
            "identity_unverified_path_actions_available": false,
            "action_receipts": self.fixture_action_receipts.clone(),
        })
    }

    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    pub(crate) fn smoke_response_candidates(&self) -> Vec<SmokeResponseCandidate> {
        let mut candidates = Vec::new();
        for key in ["start", "cancel", "close", "fixture-release"] {
            if let Some(response) = self.responses.get(key)
                && let Some(layer_id) = response.layer_id
            {
                candidates.push(SmokeResponseCandidate {
                    kind: SmokeResponseKind::Control(key),
                    id: response.id,
                    layer_id,
                    rect: response.rect,
                    response_rect: response.response_rect,
                    enabled: response.enabled,
                    interaction_eligible: self.responses_interaction_eligible,
                    generation: None,
                });
            }
        }
        candidates.extend(self.result_action_responses.iter().filter_map(|action| {
            let layer_id = action.response.layer_id?;
            Some(SmokeResponseCandidate {
                kind: SmokeResponseKind::ResultAction {
                    action: action.action,
                    group_index: action.group_index,
                    member_index: action.member_index,
                },
                id: action.response.id,
                layer_id,
                rect: action.response.rect,
                response_rect: action.response.response_rect,
                enabled: action.response.enabled,
                interaction_eligible: self.responses_interaction_eligible,
                generation: Some(action.generation),
            })
        }));
        candidates
    }

    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    pub(crate) fn smoke_generation(&self) -> u64 {
        self.session.view().snapshot.generation
    }

    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    pub(crate) fn finish_fixture_close(&mut self) -> bool {
        if self.open || !self.session.view().closed {
            return false;
        }
        match self.session.reopen() {
            Ok(()) => {
                self.session.close();
                true
            }
            Err(DuplicateFinderSessionError::WorkerRetiring) => false,
            Err(error) => {
                self.status = Some(session_error_message(&error));
                false
            }
        }
    }

    fn root_window_layer() -> egui::LayerId {
        crate::gui::jump_mode::ui::NavigationDialogKind::DuplicateFinder
            .layer_id()
            .expect("duplicate finder layer")
    }
    pub(crate) fn root_layer(&self) -> Option<egui::LayerId> {
        (self.open
            && (self.host.presentation() == super::utility_host::UtilityPresentation::Docked
                || self.host.embedded_root_layer().is_some()))
        .then_some(Self::root_window_layer())
    }
    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        self.open && self.host.presentation() == super::utility_host::UtilityPresentation::Popout
    }
    pub(crate) fn needs_viewport_bootstrap(&self) -> bool {
        self.open && self.host.bootstrap_pending()
    }
    pub(crate) fn request_host_focus(&mut self) {
        self.host_focus = true;
        self.host.request_focus();
    }
    pub(super) fn presentation(&self) -> super::utility_host::UtilityPresentation {
        self.host.presentation()
    }
    pub(crate) fn viewport() -> egui::ViewportId {
        egui::ViewportId::from_hash_of("duplicate-finder-viewport")
    }
    fn native_identity(
        &self,
        host: super::utility_host::HostIdentity,
    ) -> super::utility_window_geometry::NativeViewportIdentity {
        super::utility_window_geometry::NativeViewportIdentity {
            key: crate::settings::UtilityToolKey::parse("duplicate-finder")
                .expect("stable duplicate finder key"),
            viewport_id: Self::viewport(),
            owner_generation: self.host_epoch,
            host_generation: host.generation,
        }
    }
    pub(crate) fn end_session_with_host(&mut self, ctx: &egui::Context) {
        if self.host.presentation() == super::utility_host::UtilityPresentation::Popout {
            let identity = self.native_identity(self.host.identity());
            super::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
            ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
        }
        self.host.retire_to_docked();
        self.host_jump.cancel_all();
        self.host_jump_frame = None;
        self.host_focus = false;
        self.close();
    }
    pub(crate) fn poll_host_background(&mut self, _ctx: &egui::Context) {
        if self.open && self.reopen_pending {
            match self.session.reopen() {
                Ok(()) => {
                    self.reopen_pending = false;
                    self.status = None;
                    self.bump_target_revision();
                }
                Err(DuplicateFinderSessionError::WorkerRetiring) => {}
                Err(error) => {
                    self.reopen_pending = false;
                    self.status = Some(session_error_message(&error));
                }
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn test_host_button(&self) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.host_button
    }
    #[cfg(test)]
    pub(crate) fn test_response(
        &self,
        key: &'static str,
    ) -> Option<(egui::Id, egui::Rect, egui::Rect, bool)> {
        self.responses
            .get(key)
            .map(|r| (r.id, r.rect, r.response_rect, r.enabled))
    }
    #[cfg(test)]
    pub(crate) fn test_result_response(&self, action: &'static str) -> Option<(egui::Rect, bool)> {
        self.result_action_responses
            .iter()
            .find(|r| r.action == action && r.group_kind == "verified")
            .map(|r| (r.response.rect, r.response.enabled))
    }
    #[cfg(test)]
    pub(crate) fn test_snapshot(&self) -> Arc<DiscoverySnapshot> {
        self.current_snapshot()
    }
    #[cfg(test)]
    pub(crate) fn test_status(&self) -> Option<&str> {
        self.status.as_deref()
    }
    #[cfg(test)]
    pub(crate) fn queue_folder_pick_for_test(&mut self, path: Option<PathBuf>) {
        self.test_folder_picks.push_back(path);
    }
    #[cfg(test)]
    pub(crate) fn test_host_snapshot(
        &self,
    ) -> (usize, u64, u64, bool, bool, Vec<PathBuf>, String, usize) {
        let view = self.session.view();
        (
            &self.session as *const _ as usize,
            self.target_revision,
            view.snapshot.generation,
            view.busy,
            view.closed,
            self.selected_roots.clone(),
            self.root_editor.clone(),
            self.test_folder_picks.len(),
        )
    }
    fn jump_generation(&self, snapshot: &Arc<DiscoverySnapshot>) -> u64 {
        self.target_revision.rotate_left(7)
            ^ self.host_epoch.rotate_left(19)
            ^ self.host.identity().generation.rotate_left(31)
            ^ snapshot.generation.rotate_left(43)
            ^ (Arc::as_ptr(snapshot) as usize as u64)
    }
    fn current_snapshot(&self) -> Arc<DiscoverySnapshot> {
        #[cfg(test)]
        if let Some(snapshot) = &self.test_snapshot_override {
            return Arc::clone(snapshot);
        }
        self.session.view().snapshot
    }
    fn render_host_body(
        &mut self,
        ctx: &egui::Context,
        snapshot: &Arc<DiscoverySnapshot>,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        owner_current: bool,
        first: bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: Option<super::utility_host::UtilityPresentation>,
    ) -> (
        Vec<FinderIntent>,
        Vec<String>,
        Option<super::utility_host::UtilityPresentation>,
        bool,
    ) {
        self.staged_intents.clear();
        self.ensure_rows(snapshot);
        let mut window_open = self.open;
        let mut order = 0_u64;
        let mut visual_close = false;
        let mut request = None;
        let start = ctx.output(|output| output.commands.len());
        let mut draw = |ui: &mut egui::Ui| {
            let visible_owner = owner_current && ui.is_visible() && !ui.is_sizing_pass();
            let commit_enabled = visible_owner && first && !ctx.will_discard();
            #[cfg(any(test, feature = "dev-smoke-binaries"))]
            {
                self.responses_interaction_eligible = commit_enabled;
            }
            if let Some(presentation) = presentation {
                let (button, target) =
                    super::utility_host::presentation_button(ui, presentation, visible_owner);
                self.record_response("host-presentation", &button);
                #[cfg(test)]
                {
                    self.host_button = Some((button.rect, ui.clip_rect(), button.enabled()));
                }
                register_target(
                    scope,
                    ui,
                    &button,
                    "dialog/host",
                    if presentation == super::utility_host::UtilityPresentation::Docked {
                        "Pop Out"
                    } else {
                        "Dock"
                    },
                    crate::gui::jump_mode::TargetKind::Button,
                    &mut order,
                );
                if button.clicked() && commit_enabled {
                    request = Some(target);
                }
            }
            ui.add_enabled_ui(visible_owner, |ui| {
                let footer_reserve = ui.spacing().interact_size.y
                    + 2.0 * ui.spacing().item_spacing.y
                    + FOOTER_SEPARATOR_SPACING;
                let body_max_height = (ui.available_height() - footer_reserve).max(0.0);
                let _content = egui::ScrollArea::vertical()
                    .id_salt("duplicate-finder-content")
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .max_height(body_max_height)
                    .show(ui, |ui| {
                        self.show_roots(ui, visible_owner, commit_enabled, scope, &mut order);
                        ui.separator();
                        self.show_status(ui, &snapshot);
                        if let Some(message) = self.status.as_deref() {
                            ui.separator();
                            ui.label(message);
                        }
                        ui.separator();
                        // The inner scroll area's remaining height may be
                        // exhausted by the roots and status sections. Give
                        // results a finite viewport derived from the
                        // shared body budget; the outer body scrolls those
                        // sections together when the window is crowded.
                        let results_max_height = body_max_height * 0.4;
                        self.show_results(
                            ui,
                            &snapshot,
                            visible_owner,
                            commit_enabled,
                            results_max_height,
                            scope,
                            &mut order,
                        );
                    });
                #[cfg(any(test, feature = "dev-smoke-binaries"))]
                self.record_scroll_area(
                    "content-scroll",
                    _content.id,
                    _content.inner_rect,
                    ui.clip_rect(),
                    ui.is_enabled(),
                );
                ui.add(egui::Separator::default().spacing(FOOTER_SEPARATOR_SPACING));
                ui.horizontal(|ui| {
                    self.show_scan_controls(ui, visible_owner, commit_enabled, scope, &mut order);
                    let close = ui.button("Close");
                    self.record_response("close", &close);
                    register_target(
                        scope,
                        ui,
                        &close,
                        "dialog/close",
                        "Close Duplicate Finder",
                        crate::gui::jump_mode::TargetKind::Button,
                        &mut order,
                    );
                    visual_close = close.clicked() && commit_enabled;
                });
            });
        };
        if let Some(ui) = child_ui {
            draw(ui);
        } else {
            egui::Window::new(WINDOW_TITLE)
                .id(egui::Id::new(Some(WINDOW_TITLE)))
                .collapsible(false)
                .resizable(true)
                .default_size((1040.0, 700.0))
                .min_size((620.0, 420.0))
                .max_size(ctx.content_rect().size())
                .show_utility(
                    ctx,
                    "duplicate-finder",
                    egui::Id::new(WINDOW_TITLE),
                    Some(&mut window_open),
                    crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(
                            egui::Vec2::from((1040.0, 700.0)),
                        ),
                        egui::Vec2::ZERO,
                        [None, None],
                    )
                    .with_minimum(egui::Vec2::from((620.0, 420.0)))
                    .with_maximum(ctx.content_rect().size()),
                    |ui| draw(ui),
                );
        }
        let copies = ctx.output_mut(|output| {
            let mut copies = Vec::new();
            for command in output.commands.split_off(start) {
                match command {
                    egui::OutputCommand::CopyText(text) => copies.push(text),
                    other => output.commands.push(other),
                }
            }
            copies
        });
        let authorized = first && owner_current && !ctx.will_discard();
        (
            std::mem::take(&mut self.staged_intents),
            if authorized { copies } else { Vec::new() },
            request,
            authorized && (!window_open || visual_close),
        )
    }
    fn clear_test_responses(&mut self) {
        #[cfg(any(test, feature = "dev-smoke-binaries"))]
        {
            self.responses.clear();
            self.result_action_responses.clear();
            self.unavailable_result_rows.clear();
            self.responses_interaction_eligible = false;
        }
        #[cfg(test)]
        {
            self.host_button = None;
        }
    }
    pub(crate) fn show_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        instance: Option<u64>,
        owner: crate::gui::jump_mode::ui::NavigationOwner,
        input_allowed: bool,
        root_owner: bool,
        first: bool,
    ) {
        use super::utility_host::ViewportPassProvenance;
        if let Some(old) = self.host_pending.take() {
            if old.epoch == self.host_epoch {
                self.root_editor = old.previous_editor;
            }
        }
        self.poll_host_background(ctx);
        self.clear_test_responses();
        if !self.open {
            return;
        }
        let snapshot = self.current_snapshot();
        let scope = scope.zip(instance.filter(|i| matches!(owner, crate::gui::jump_mode::ui::NavigationOwner::Dialog { kind: crate::gui::jump_mode::ui::NavigationDialogKind::DuplicateFinder, instance } if instance == *i)))
            .map(|(scope, instance)| scope.child_namespace("duplicate-finder").with_surface(crate::gui::jump_mode::SurfaceId::dialog("duplicate-finder", instance), crate::gui::jump_mode::TargetGeneration(self.jump_generation(&snapshot)))).map(|scope| if first && !ctx.will_discard() { scope } else { scope.registration_only() });
        let allowed = root_owner
            && input_allowed
            && crate::gui::jump_mode::ui::visible_top_middle_layer(ctx)
                == Some(Self::root_window_layer())
            && !egui::Popup::is_any_open(ctx);
        let previous_editor = self.root_editor.clone();
        let (intents, copies, request, close) =
            self.render_host_body(ctx, &snapshot, scope.as_ref(), allowed, first, None, None);
        if first && !ctx.will_discard() {
            let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
            self.host_pending = Some(FinderHostFrame {
                root,
                owner: root,
                host: self.host.identity(),
                epoch: self.host_epoch,
                target_revision: self.target_revision,
                previous_editor,
                intents,
                copies,
                close,
                request,
                info: None,
                native: false,
            });
        } else {
            self.root_editor = previous_editor;
        }
    }
    pub(crate) fn show_hosted(
        &mut self,
        ctx: &egui::Context,
        root_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        instance: Option<u64>,
        root_owner: crate::gui::jump_mode::ui::NavigationOwner,
        input_allowed: bool,
        root_native_owner: bool,
        launcher_jump: &crate::gui::jump_mode::ui::JumpModeUi,
        jump_enabled: bool,
        root_visible: bool,
        first: bool,
        pinned: bool,
        root_shortcut_allowed: bool,
    ) {
        use super::utility_host::{UtilityPresentation, ViewportPassProvenance};
        use crate::gui::jump_mode::ui::{
            NavigationDialogKind, NavigationOwner, NavigationViewportBinding,
        };
        if let Some(old) = self.host_pending.take() {
            if old.epoch == self.host_epoch {
                self.root_editor = old.previous_editor;
            }
        }
        self.clear_test_responses();
        if !self.open {
            return;
        }
        if pinned {
            self.host_os_close_pending = false;
        }
        let snapshot = self.current_snapshot();
        let host = self.host.identity();
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let previous_editor = self.root_editor.clone();
        let revision = self.target_revision;
        let jump_generation = self.jump_generation(&snapshot);
        let mut staged = None;
        let mut owner = None;
        let mut info = None;
        let mut native_owner = false;
        if host.host == UtilityPresentation::Docked {
            if root_visible {
                let allowed = input_allowed && root_native_owner && !egui::Popup::is_any_open(ctx);
                let scope = root_scope.zip(instance.filter(|i| matches!(root_owner, NavigationOwner::Dialog { kind: NavigationDialogKind::DuplicateFinder, instance } if instance == *i)))
                    .map(|(scope, instance)| scope.child_namespace("duplicate-finder").with_surface(crate::gui::jump_mode::SurfaceId::dialog("duplicate-finder", instance), crate::gui::jump_mode::TargetGeneration(jump_generation)))
                    .map(|scope| if first && !ctx.will_discard() { scope } else { scope.registration_only() });
                let shortcut = first
                    && allowed
                    && root_shortcut_allowed
                    && !pinned
                    && ctx.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                    });
                let mut result = self.render_host_body(
                    ctx,
                    &snapshot,
                    scope.as_ref(),
                    allowed,
                    first,
                    None,
                    Some(host.host),
                );
                result.3 |= shortcut;
                staged = Some(result);
                owner = Some(root);
            }
        } else {
            let viewport = Self::viewport();
            let builder = egui::ViewportBuilder::default()
                .with_title(WINDOW_TITLE)
                .with_inner_size([1040.0, 700.0])
                .with_min_inner_size([620.0, 420.0])
                .with_resizable(true);
            ctx.show_viewport_immediate(viewport, builder, |child, class| {
                let c = child.ctx().clone(); let native = class == egui::ViewportClass::Immediate && c.viewport_id() == viewport;
                let child_first = first && child.current_pass_index() == 0;
                if child_first { owner = Some(ViewportPassProvenance::current(&c, c.viewport_id())); native_owner = native; if native { info = Some(child.input(|i| i.viewport().clone())); } }
                else { owner = None; info = None; staged = None; self.root_editor = previous_editor.clone(); }
                let allowed = if native { input_allowed && c.input(|i| i.focused) && c.memory(|m| m.top_modal_layer().is_none()) && super::jump_mode::ui::visible_top_middle_layer(&c).is_none() && !egui::Popup::is_any_open(&c) && super::jump_mode::ui::focused_response_layer(&c).is_none_or(|l| l == egui::LayerId::background()) } else { input_allowed && root_native_owner && !egui::Popup::is_any_open(&c) };
                if native && child_first && child.input(|i| i.viewport().close_requested()) { child.send_viewport_cmd(egui::ViewportCommand::CancelClose); if !pinned { self.host_os_close_pending = true; } }
                if native {
                    let (chord, alphabet) = launcher_jump.configuration(); self.host_jump.set_configuration(chord, alphabet); self.host_jump.set_enabled(jump_enabled);
                    let frame = c.cumulative_frame_nr(); if self.host_jump_frame != Some(frame) { self.host_jump.begin_frame(); self.host_jump_frame = Some(frame); }
                    let binding = instance.map(|instance| NavigationViewportBinding { viewport, owner: NavigationOwner::Dialog { kind: NavigationDialogKind::DuplicateFinder, instance }, base_layer: egui::LayerId::background() });
                    let nav = binding.map_or(NavigationOwner::Blocked, |b| self.host_jump.update_navigation_owner_for_viewport(&c, b, input_allowed && jump_enabled));
                    let jump_was_active = self.host_jump.active_session().is_some();
                    self.host_jump.handle_viewport_input(&c, binding.is_some_and(|b| b.owner == nav), child_first, viewport);
                    let shortcut = child_first && allowed && !pinned && !jump_was_active && self.host_jump.active_session().is_none() && c.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W));
                    self.host_jump.begin_ui_pass();
                    let scope = binding.filter(|b| b.owner == nav).and_then(|b| self.host_jump.scope_for_viewport_binding(crate::gui::jump_mode::SurfaceId::dialog("duplicate-finder", instance.unwrap_or_default()), crate::gui::jump_mode::TargetGeneration(jump_generation), "duplicate-finder/view", b)).map(|scope| { let scope = scope.child_namespace("duplicate-finder"); if child_first && !c.will_discard() { scope } else { scope.registration_only() } });
                    egui::CentralPanel::default().show(child, |ui| { staged = Some(self.render_host_body(&c, &snapshot, scope.as_ref(), allowed, child_first, Some(ui), Some(host.host))); });
                    if let Some(result) = &mut staged { result.3 |= shortcut || (!pinned && self.host_os_close_pending); }
                    self.host_jump.finish_ui_pass();
                    if let Some(binding) = binding {
                        let current = self.host_jump.update_navigation_owner_for_viewport(&c, binding, input_allowed && jump_enabled);
                        if self.host_jump.prepare_pass_for_viewport(&c, binding, current, input_allowed && jump_enabled) { super::jump_mode::paint::paint(&c, &self.host_jump); }
                        if !c.will_discard() { self.host_jump.complete_frame_for_viewport(&c, binding, current, input_allowed && jump_enabled); }
                    }
                } else {
                    let scope = root_scope.zip(instance.filter(|i| matches!(root_owner, NavigationOwner::Dialog { kind: NavigationDialogKind::DuplicateFinder, instance } if instance == *i)))
                        .map(|(scope, instance)| scope.child_namespace("duplicate-finder").with_surface(crate::gui::jump_mode::SurfaceId::dialog("duplicate-finder", instance), crate::gui::jump_mode::TargetGeneration(jump_generation)))
                        .map(|scope| if child_first && !c.will_discard() { scope } else { scope.registration_only() });
                    let shortcut = child_first && allowed && root_shortcut_allowed && !pinned && c.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W));
                    let mut result = self.render_host_body(&c, &snapshot, scope.as_ref(), allowed, child_first, None, Some(host.host)); result.3 |= shortcut; staged = Some(result);
                }
            });
        }
        if let (Some(owner), Some((intents, copies, request, mut close))) = (owner, staged) {
            if pinned {
                close = false;
            }
            if first && !ctx.will_discard() {
                self.host_pending = Some(FinderHostFrame {
                    root,
                    owner,
                    host,
                    epoch: self.host_epoch,
                    target_revision: revision,
                    previous_editor,
                    intents,
                    copies,
                    close,
                    request,
                    info,
                    native: native_owner,
                });
                return;
            }
        }
        self.root_editor = previous_editor;
    }
    pub(super) fn on_hosted_frame_complete(
        &mut self,
        ctx: &egui::Context,
    ) -> DuplicateFinderHostOutcome {
        use super::utility_host::UtilityPresentation;
        let Some(frame) = self.host_pending.take() else {
            return DuplicateFinderHostOutcome::default();
        };
        if !self.open
            || frame.epoch != self.host_epoch
            || frame.host != self.host.identity()
            || frame.target_revision != self.target_revision
            || !frame.root.was_retained(ctx)
            || !frame.owner.was_retained(ctx)
        {
            if self.open && frame.epoch == self.host_epoch {
                self.root_editor = frame.previous_editor;
            }
            return DuplicateFinderHostOutcome::default();
        }
        let identity = self.native_identity(frame.host);
        if let Some(info) = frame.info.as_ref() {
            super::utility_dialog_size::apply_native_utility_geometry_restore(
                ctx,
                egui::ViewportClass::Immediate,
                true,
                WINDOW_TITLE,
                identity,
                info,
                egui::vec2(1040.0, 700.0),
                Some(egui::vec2(620.0, 420.0)),
                None,
            );
            if self.host_focus {
                ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Focus);
            }
        } else if self.host_focus {
            if let Some(layer) = self.root_layer() {
                ctx.move_to_top(layer);
            }
        }
        self.host_focus = false;
        if frame.close {
            self.end_session_with_host(ctx);
            return DuplicateFinderHostOutcome {
                closed: true,
                ..Default::default()
            };
        }
        let show_root = frame.request == Some(UtilityPresentation::Docked);
        if let Some(next) = frame.request {
            if self.host.transition(next) {
                self.host_jump.cancel_all();
                self.host_jump_frame = None;
                if next == UtilityPresentation::Docked {
                    super::utility_dialog_size::close_native_utility_geometry_viewport(
                        ctx, identity,
                    );
                    ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
                }
                self.host_focus = true;
                ctx.request_repaint();
            }
        } else {
            for intent in frame.intents {
                self.apply_host_intent(
                    ctx,
                    intent,
                    frame.host,
                    frame.epoch,
                    self.target_revision,
                    frame.native,
                );
            }
        }
        if frame.host.host == UtilityPresentation::Popout {
            self.host.complete_bootstrap(
                frame.host,
                (!frame.native).then_some(Self::root_window_layer()),
                None,
            );
        }
        DuplicateFinderHostOutcome {
            closed: false,
            show_root,
            copies: if frame.request.is_none() {
                frame.copies
            } else {
                Vec::new()
            },
        }
    }
    fn apply_host_intent(
        &mut self,
        ctx: &egui::Context,
        intent: FinderIntent,
        host: super::utility_host::HostIdentity,
        epoch: u64,
        revision: u64,
        native: bool,
    ) {
        match intent {
            FinderIntent::PickRoot => {
                #[cfg(windows)]
                let captured_hwnd = super::utility_dialog_size::utility_picker_hwnd(
                    ctx,
                    native.then(|| self.native_identity(host)),
                );
                #[cfg(test)]
                let queued = self.test_folder_picks.pop_front();
                #[cfg(test)]
                let selected = if let Some(selected) = queued {
                    selected
                } else {
                    self.host_folder_picker(ctx, host, native)
                };
                #[cfg(not(test))]
                let selected = self.host_folder_picker(ctx, host, native);
                if !self.open
                    || self.host_epoch != epoch
                    || self.host.identity() != host
                    || self.target_revision != revision
                {
                    return;
                }
                #[cfg(all(windows, not(test)))]
                if captured_hwnd.is_none()
                    || captured_hwnd
                        != super::utility_dialog_size::utility_picker_hwnd(
                            ctx,
                            native.then(|| self.native_identity(host)),
                        )
                {
                    return;
                }
                #[cfg(all(windows, test))]
                let _ = captured_hwnd;
                if let Some(path) = selected {
                    if let Err(error) = self.add_selected_root(path) {
                        self.status = Some(error);
                    }
                }
            }
            FinderIntent::AddRoot(path) => {
                if let Err(error) = self.add_selected_root(path) {
                    self.status = Some(error);
                }
            }
            FinderIntent::RemoveRoot(path) => {
                if let Some(index) = self.selected_roots.iter().position(|root| *root == path) {
                    self.selected_roots.remove(index);
                    self.bump_target_revision();
                }
            }
            FinderIntent::Start => match self.session.start_scan(self.selected_roots.clone()) {
                Ok(_) => {
                    self.status = None;
                    self.bump_target_revision();
                }
                Err(error) => self.status = Some(session_error_message(&error)),
            },
            FinderIntent::Cancel => match self.session.cancel() {
                Ok(true) => {
                    self.status =
                        Some("Cancelled. No results were published for this scan.".into());
                    self.bump_target_revision();
                }
                Ok(false) => self.status = Some("There is no active scan to cancel.".into()),
                Err(error) => self.status = Some(session_error_message(&error)),
            },
            FinderIntent::Known {
                snapshot,
                size_group,
                verified,
                action,
                namespace,
                group,
                member,
            } => self.run_known_action(
                &snapshot, size_group, &verified, action, namespace, group, member,
            ),
            FinderIntent::Copy { snapshot, group } => self.copy_verified_group(&snapshot, group),
            #[cfg(any(test, feature = "dev-smoke-binaries"))]
            FinderIntent::ReleaseGate => {
                if let Some(gate) = &self.first_read_checkpoint {
                    gate.release();
                }
            }
        }
    }
    fn host_folder_picker(
        &mut self,
        ctx: &egui::Context,
        host: super::utility_host::HostIdentity,
        native: bool,
    ) -> Option<PathBuf> {
        let picker = rfd::FileDialog::new();
        #[cfg(windows)]
        let picker = {
            let Some(hwnd) = super::utility_dialog_size::utility_picker_hwnd(
                ctx,
                native.then(|| self.native_identity(host)),
            )
            .and_then(|h| std::num::NonZeroIsize::new(h as isize)) else {
                self.status = Some(
                    "Cannot verify the folder picker window. Focus Duplicate Finder and try again."
                        .into(),
                );
                return None;
            };
            picker.set_parent(&super::add_action_dialog::NativeDialogParent(hwnd))
        };
        #[cfg(not(windows))]
        let _ = (ctx, host, native);
        picker.pick_folder()
    }

    fn show_roots(
        &mut self,
        ui: &mut egui::Ui,
        enabled: bool,
        commit_enabled: bool,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        order: &mut u64,
    ) {
        ui.label("Choose one or more folders. Duplicate discovery runs only after Start.");
        let root_editor_width = (ui.available_width() - 260.0).clamp(120.0, 360.0);
        ui.horizontal_wrapped(|ui| {
            let root = ui.add_enabled(
                enabled,
                egui::TextEdit::singleline(&mut self.root_editor)
                    .desired_width(root_editor_width)
                    .hint_text("folder path"),
            );
            self.record_response("root-editor", &root);
            register_target(
                scope,
                ui,
                &root,
                "roots/path",
                "Enter a folder path",
                crate::gui::jump_mode::TargetKind::TextEntry,
                order,
            );
            let pick = ui.add_enabled(enabled, egui::Button::new("Choose Folder…"));
            self.record_response("pick-root", &pick);
            register_target(
                scope,
                ui,
                &pick,
                "roots/pick",
                "Choose a folder",
                crate::gui::jump_mode::TargetKind::Button,
                order,
            );
            if pick.clicked() && commit_enabled {
                self.staged_intents.push(FinderIntent::PickRoot);
            }
            let add = ui.add_enabled(enabled, egui::Button::new("Add Root"));
            self.record_response("add-root", &add);
            register_target(
                scope,
                ui,
                &add,
                "roots/add",
                "Add this folder root",
                crate::gui::jump_mode::TargetKind::Button,
                order,
            );
            if add.clicked() && commit_enabled {
                self.staged_intents
                    .push(FinderIntent::AddRoot(PathBuf::from(
                        self.root_editor.trim(),
                    )));
            }
        });

        if self.selected_roots.is_empty() {
            ui.weak("No roots selected.");
        } else {
            let mut remove_index = None;
            egui::ScrollArea::both()
                .id_salt("duplicate-finder-roots")
                .min_scrolled_height(0.0)
                .max_height(34.0)
                .show(ui, |ui| {
                    for (index, root) in self.selected_roots.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.monospace(
                                root.to_str()
                                    .unwrap_or("<path is not representable as text>"),
                            );
                            let remove = ui.add_enabled(enabled, egui::Button::new("Remove"));
                            let target = format!("roots/remove/{index}");
                            register_target(
                                scope,
                                ui,
                                &remove,
                                target,
                                format!("Remove root {}", index + 1),
                                crate::gui::jump_mode::TargetKind::Button,
                                order,
                            );
                            if remove.clicked() && commit_enabled {
                                remove_index = Some(index);
                            }
                        });
                    }
                });
            if let Some(index) = remove_index {
                self.staged_intents
                    .push(FinderIntent::RemoveRoot(self.selected_roots[index].clone()));
            }
        }
    }

    fn show_scan_controls(
        &mut self,
        ui: &mut egui::Ui,
        enabled: bool,
        commit_enabled: bool,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        order: &mut u64,
    ) {
        let session = self.session.view();
        let start = ui.add_enabled(
            enabled && !session.busy && !session.closed && !self.selected_roots.is_empty(),
            egui::Button::new("Start Scan"),
        );
        self.record_response("start", &start);
        register_target(
            scope,
            ui,
            &start,
            "scan/start",
            "Start duplicate scan",
            crate::gui::jump_mode::TargetKind::Button,
            order,
        );
        if start.clicked() && commit_enabled {
            self.staged_intents.push(FinderIntent::Start);
        }
        let cancel = ui.add_enabled(
            enabled && session.busy && !session.closed,
            egui::Button::new("Cancel Scan"),
        );
        self.record_response("cancel", &cancel);
        register_target(
            scope,
            ui,
            &cancel,
            "scan/cancel",
            "Cancel duplicate scan",
            crate::gui::jump_mode::TargetKind::Button,
            order,
        );
        if cancel.clicked() && commit_enabled {
            self.staged_intents.push(FinderIntent::Cancel);
        }
        #[cfg(any(test, feature = "dev-smoke-binaries"))]
        if let Some(checkpoint) = self.first_read_checkpoint.as_ref() {
            let checkpoint = Arc::clone(checkpoint);
            let release = ui.add_enabled(
                enabled && checkpoint.reached() && !checkpoint.released(),
                egui::Button::new("Release fixture read gate"),
            );
            self.record_response("fixture-release", &release);
            if release.clicked() && commit_enabled {
                self.staged_intents.push(FinderIntent::ReleaseGate);
            }
        }
    }

    fn show_status(&mut self, ui: &mut egui::Ui, snapshot: &DiscoverySnapshot) {
        let view = self.session.view();
        // Keep optional status lists proportional to the space left in the
        // bounded body; the fixed scan/close footer is outside this area.
        let status_scroll_budget = ui.available_height();
        let root_status_max_height = (status_scroll_budget * 0.16).clamp(0.0, 72.0);
        let diagnostics_max_height = (status_scroll_budget * 0.22).clamp(0.0, 96.0);
        match snapshot.status {
            DiscoveryStatus::Idle => {
                ui.label("Ready. Select folders, then start a scan.");
            }
            DiscoveryStatus::Scanning => {
                if let Some(progress) = view.progress {
                    ui.label(format!(
                        "Scanning {:?} · roots {}/{} · {} directories · {} files",
                        progress.stage,
                        progress.roots_completed,
                        progress.roots_total,
                        progress.counts.directories_scanned,
                        progress.counts.files_seen,
                    ));
                    if let Some(content) = progress.content {
                        ui.label(format!(
                            "Content verification · {}/{} files · {} bytes read",
                            content.files_completed, content.files_total, content.bytes_processed
                        ));
                    }
                } else {
                    ui.label("Preparing the selected roots…");
                }
            }
            DiscoveryStatus::Complete => {
                if snapshot.verified_duplicate_groups.is_empty()
                    && snapshot.identity_unverified_matches.is_empty()
                {
                    ui.label("Scan complete. No byte-identical groups were found.");
                } else {
                    ui.label(format!(
                        "Scan complete · {} verified groups · {} distinct files · {} logical duplicate bytes (not recoverable disk space)",
                        snapshot.duplicate_totals.verified_groups,
                        snapshot.duplicate_totals.verified_files,
                        snapshot.duplicate_totals.logical_duplicate_bytes,
                    ));
                }
            }
            DiscoveryStatus::Partial => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!(
                        "Partial results · {} verified groups · {} logical duplicate bytes · some entries or content could not be confirmed",
                        snapshot.duplicate_totals.verified_groups,
                        snapshot.duplicate_totals.logical_duplicate_bytes,
                    ),
                );
            }
            DiscoveryStatus::Cancelled => {
                let suffix = if view.busy {
                    " The worker is still draining; Start remains unavailable until it retires."
                } else {
                    " Start a new scan when ready."
                };
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("Cancelled. No results were published for this scan.{suffix}"),
                );
            }
            DiscoveryStatus::Failed => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "The scan could not be started.",
                );
            }
            DiscoveryStatus::Closed => {
                ui.weak("The prior session is retiring before this dialog can reopen.");
            }
        }

        if snapshot.status != DiscoveryStatus::Scanning {
            let _roots = egui::ScrollArea::vertical()
                .id_salt("duplicate-finder-root-statuses")
                .auto_shrink([false, false])
                .min_scrolled_height(0.0)
                .max_height(root_status_max_height)
                .show(ui, |ui| {
                    for root in &snapshot.roots {
                        ui.horizontal_wrapped(|ui| {
                            ui.monospace(root.selected_path.to_str().unwrap_or("<unrepresentable selected root>"));
                            ui.weak(root_status_label(root.status));
                            if root.counts.inaccessible_entries > 0
                                || root.counts.reparse_entries_excluded > 0
                                || root.counts.cloud_placeholders_excluded > 0
                            {
                                ui.weak(format!(
                                    "{} inaccessible · {} reparse excluded · {} cloud placeholders excluded",
                                    root.counts.inaccessible_entries,
                                    root.counts.reparse_entries_excluded,
                                    root.counts.cloud_placeholders_excluded,
                                ));
                            }
                        });
                    }
                });
            #[cfg(any(test, feature = "dev-smoke-binaries"))]
            self.record_scroll_area(
                "root-status-scroll",
                _roots.id,
                _roots.inner_rect,
                ui.clip_rect(),
                ui.is_enabled(),
            );
        }
        if !snapshot.diagnostics.is_empty() {
            let mut diagnostics_scroll_geometry = None;
            let diagnostics = egui::CollapsingHeader::new(format!(
                "Diagnostics ({})",
                snapshot.diagnostics.len() + usize::from(snapshot.diagnostics_truncated)
            ))
            .id_salt("duplicate-finder-diagnostics")
            .show(ui, |ui| {
                let scroll = egui::ScrollArea::vertical()
                    .id_salt("duplicate-finder-diagnostics-body")
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .max_height(diagnostics_max_height)
                    .show(ui, |ui| {
                        for diagnostic in snapshot.diagnostics.iter().take(64) {
                            show_diagnostic(ui, diagnostic);
                        }
                        if snapshot.diagnostics_truncated {
                            ui.weak("Additional diagnostics were omitted by the scan limit.");
                        }
                    });
                diagnostics_scroll_geometry = Some((scroll.id, scroll.inner_rect));
            });
            #[cfg(any(test, feature = "dev-smoke-binaries"))]
            {
                self.record_response("diagnostics-toggle", &diagnostics.header_response);
                if let Some((id, rect)) = diagnostics_scroll_geometry {
                    self.record_geometry(
                        "diagnostics-scroll",
                        id,
                        None,
                        rect.intersect(ui.clip_rect()),
                        rect,
                        ui.is_enabled(),
                    );
                }
            }
            #[cfg(not(any(test, feature = "dev-smoke-binaries")))]
            let _ = (diagnostics, diagnostics_scroll_geometry);
        }
    }

    fn show_results(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &Arc<DiscoverySnapshot>,
        enabled: bool,
        commit_enabled: bool,
        max_height: f32,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        order: &mut u64,
    ) {
        if self.rows.is_empty() {
            return;
        }
        if self.rows_truncated {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "The visible result list reached its display limit; scan totals remain unchanged.",
            );
        }
        let _row_range = egui::ScrollArea::vertical()
            .id_salt("duplicate-finder-results")
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show_rows(ui, RESULT_ROW_HEIGHT, self.rows.len(), |ui, range| {
                let row_viewport = ui.clip_rect().intersect(ui.available_rect_before_wrap());
                for row_index in range {
                    let row = self.rows[row_index];
                    self.show_result_row(
                        ui,
                        row_viewport,
                        snapshot,
                        row,
                        enabled,
                        commit_enabled,
                        scope,
                        order,
                    );
                }
            });
        #[cfg(any(test, feature = "dev-smoke-binaries"))]
        {
            self.record_scroll_area(
                "results-scroll",
                _row_range.id,
                _row_range.inner_rect,
                ui.clip_rect(),
                ui.is_enabled(),
            );
        }
    }

    fn show_result_row(
        &mut self,
        ui: &mut egui::Ui,
        row_viewport: egui::Rect,
        snapshot: &Arc<DiscoverySnapshot>,
        row: ResultRow,
        enabled: bool,
        commit_enabled: bool,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        order: &mut u64,
    ) {
        let row_width = row_viewport.width().max(0.0);
        ui.set_max_width(row_width);
        match row {
            ResultRow::VerifiedHeader(index) => {
                let group = &snapshot.verified_duplicate_groups[index];
                let row_height = ui.spacing().interact_size.y;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let copy = ui.add_enabled(
                        enabled,
                        egui::Button::new("Copy Paths")
                            .min_size(egui::vec2(COPY_PATHS_BUTTON_WIDTH, row_height)),
                    );
                    let target = format!("results/verified/{index}/copy");
                    self.record_result_action(
                        "copy_paths",
                        "verified",
                        index,
                        None,
                        snapshot.generation,
                        &copy,
                    );
                    register_target(
                        scope,
                        ui,
                        &copy,
                        target,
                        "Copy paths for this verified group",
                        crate::gui::jump_mode::TargetKind::Button,
                        order,
                    );
                    if copy.clicked() && commit_enabled {
                        self.staged_intents.push(FinderIntent::Copy {
                            snapshot: Arc::clone(snapshot),
                            group: index,
                        });
                    }
                    let details = format!(
                        "{} logical duplicate bytes (not recoverable disk space)",
                        group.logical_duplicate_bytes
                    );
                    let details_width = ui.available_width().min(190.0).max(0.0);
                    ui.add_sized(
                        egui::vec2(details_width, row_height),
                        egui::Label::new(egui::RichText::new(details).weak())
                            .truncate()
                            .show_tooltip_when_elided(true),
                    );
                    let title = format!(
                        "Verified duplicate group · {} bytes each · {} distinct files",
                        snapshot.size_groups[group.size_group_index].size_bytes,
                        group.known_files.len()
                    );
                    ui.add_sized(
                        egui::vec2(ui.available_width().max(0.0), row_height),
                        egui::Label::new(egui::RichText::new(title).strong())
                            .truncate()
                            .halign(egui::Align::Min)
                            .show_tooltip_when_elided(true),
                    );
                });
            }
            ResultRow::VerifiedMember { group, member } => {
                let verified = &snapshot.verified_duplicate_groups[group].known_files[member];
                self.show_known_member(
                    ui,
                    row_viewport,
                    snapshot,
                    snapshot.verified_duplicate_groups[group].size_group_index,
                    verified,
                    "verified",
                    group,
                    member,
                    snapshot.generation,
                    enabled,
                    commit_enabled,
                    scope,
                    order,
                );
            }
            ResultRow::UnverifiedHeader(index) => {
                let group = &snapshot.identity_unverified_matches[index];
                let label = format!(
                    "Content match with unverified identity · {} bytes each · not included in verified totals",
                    snapshot.size_groups[group.size_group_index].size_bytes
                );
                ui.add_sized(
                    egui::vec2(row_width, ui.spacing().interact_size.y),
                    egui::Label::new(egui::RichText::new(label).strong())
                        .truncate()
                        .show_tooltip_when_elided(true),
                );
            }
            ResultRow::UnverifiedKnownMember { group, member } => {
                let match_group = &snapshot.identity_unverified_matches[group];
                let verified = &match_group.known_files[member];
                self.show_known_member(
                    ui,
                    row_viewport,
                    snapshot,
                    match_group.size_group_index,
                    verified,
                    "identity-match",
                    group,
                    member,
                    snapshot.generation,
                    enabled,
                    commit_enabled,
                    scope,
                    order,
                );
            }
            ResultRow::UnverifiedPath { group, path } => {
                let match_group = &snapshot.identity_unverified_matches[group];
                let size_group = &snapshot.size_groups[match_group.size_group_index];
                let Some(path_index) = match_group.unverified_path_indices.get(path).copied()
                else {
                    return;
                };
                let Some(candidate) = size_group.unverified_paths.get(path_index) else {
                    return;
                };
                #[cfg(any(test, feature = "dev-smoke-binaries"))]
                self.unavailable_result_rows.push(serde_json::json!({
                    "generation": snapshot.generation,
                    "group_kind": "identity-match",
                    "group_index": group,
                    "path_index": path_index,
                    "actions_available": false,
                    "reason": "identity_unavailable",
                }));
                let row_height = ui.spacing().interact_size.y;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let details = format!(
                        "{} bytes · identity unavailable; actions disabled",
                        candidate.size_bytes
                    );
                    ui.add_sized(
                        egui::vec2(220.0_f32.min(ui.available_width()).max(0.0), row_height),
                        egui::Label::new(egui::RichText::new(details).weak())
                            .truncate()
                            .show_tooltip_when_elided(true),
                    );
                    let path_width = ui.available_width().max(0.0);
                    ui.add_sized(
                        egui::vec2(path_width, row_height),
                        egui::Label::new(
                            egui::RichText::new(
                                candidate
                                    .path
                                    .to_str()
                                    .unwrap_or("<path is not representable as text>"),
                            )
                            .monospace(),
                        )
                        .truncate()
                        .halign(egui::Align::Min)
                        .show_tooltip_when_elided(true),
                    );
                });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn show_known_member(
        &mut self,
        ui: &mut egui::Ui,
        row_viewport: egui::Rect,
        snapshot: &Arc<DiscoverySnapshot>,
        size_group_index: usize,
        verified: &VerifiedKnownFile,
        namespace: &'static str,
        group_index: usize,
        member_index: usize,
        generation: u64,
        enabled: bool,
        commit_enabled: bool,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        order: &mut u64,
    ) {
        let Some(known) = snapshot
            .size_groups
            .get(size_group_index)
            .and_then(|group| group.known_files.get(verified.known_file_index))
        else {
            return;
        };
        let Some(alias) = known.aliases.get(verified.alias_index) else {
            return;
        };
        let path_text = alias.path.to_str();
        let action_enabled = enabled && path_text.is_some() && known.modified.is_some();
        let row_width = row_viewport.width().max(0.0);
        let row_height = ui.spacing().interact_size.y;
        ui.set_max_width(row_width);
        let (open, reveal) = ui
            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Reserve fixed action/size columns before using the remaining
                // viewport width for the full-path tooltip label.
                let reveal = ui.add_enabled(
                    action_enabled,
                    egui::Button::new("Reveal")
                        .min_size(egui::vec2(REVEAL_PATH_BUTTON_WIDTH, row_height)),
                );
                let open = ui.add_enabled(
                    action_enabled,
                    egui::Button::new("Open")
                        .min_size(egui::vec2(OPEN_PATH_BUTTON_WIDTH, row_height)),
                );
                ui.add_sized(
                    egui::vec2(RESULT_SIZE_LABEL_WIDTH, row_height),
                    egui::Label::new(
                        egui::RichText::new(format!("{} bytes", known.size_bytes)).weak(),
                    )
                    .truncate()
                    .show_tooltip_when_elided(true),
                );
                ui.add_sized(
                    egui::vec2(ui.available_width().max(0.0), row_height),
                    egui::Label::new(
                        egui::RichText::new(
                            path_text.unwrap_or("<path is not representable as text>"),
                        )
                        .monospace(),
                    )
                    .truncate()
                    .halign(egui::Align::Min)
                    .show_tooltip_when_elided(true),
                );
                (open, reveal)
            })
            .inner;

        let target = format!("results/{namespace}/{group_index}/{member_index}/open");
        self.record_result_action(
            "open",
            namespace,
            group_index,
            Some(member_index),
            generation,
            &open,
        );
        register_target(
            scope,
            ui,
            &open,
            target,
            "Open this verified path",
            crate::gui::jump_mode::TargetKind::Button,
            order,
        );
        if open.clicked() && commit_enabled {
            self.staged_intents.push(FinderIntent::Known {
                snapshot: Arc::clone(snapshot),
                size_group: size_group_index,
                verified: *verified,
                action: KnownFileAction::Open,
                namespace,
                group: group_index,
                member: member_index,
            });
        }

        let target = format!("results/{namespace}/{group_index}/{member_index}/reveal");
        self.record_result_action(
            "reveal",
            namespace,
            group_index,
            Some(member_index),
            generation,
            &reveal,
        );
        register_target(
            scope,
            ui,
            &reveal,
            target,
            "Reveal this verified path",
            crate::gui::jump_mode::TargetKind::Button,
            order,
        );
        if reveal.clicked() && commit_enabled {
            self.staged_intents.push(FinderIntent::Known {
                snapshot: Arc::clone(snapshot),
                size_group: size_group_index,
                verified: *verified,
                action: KnownFileAction::Reveal,
                namespace,
                group: group_index,
                member: member_index,
            });
        }
    }

    fn run_known_action(
        &mut self,
        snapshot: &Arc<DiscoverySnapshot>,
        size_group_index: usize,
        verified: &VerifiedKnownFile,
        action: KnownFileAction,
        _namespace: &'static str,
        _group_index: usize,
        _member_index: usize,
    ) {
        let result = (|| {
            let proof = proof_for_known_alias(snapshot, size_group_index, verified)?;
            self.actions.revalidate(&proof)?;
            if !Arc::ptr_eq(snapshot, &self.session.view().snapshot) {
                return Err("The scan results changed before the file action.".to_owned());
            }
            if proof.path.to_str().is_none() {
                return Err("This path cannot be represented safely for a file action.".into());
            }
            match action {
                KnownFileAction::Open => self.actions.open(&proof.path),
                KnownFileAction::Reveal => self.actions.reveal(&proof.path),
            }
        })();
        #[cfg(any(test, feature = "dev-smoke-binaries"))]
        if self.fixture_action_receipts.len() < 64 {
            self.fixture_action_receipts.push(serde_json::json!({
                "action": match action {
                    KnownFileAction::Open => "open",
                    KnownFileAction::Reveal => "reveal",
                },
                "result": if result.is_ok() { "accepted" } else { "refused" },
                "generation": snapshot.generation,
                "group_kind": _namespace,
                "group_index": _group_index,
                "member_index": _member_index,
            }));
        }
        self.status = Some(match result {
            Ok(()) => match action {
                KnownFileAction::Open => "Opened the revalidated file.".into(),
                KnownFileAction::Reveal => "Revealed the revalidated file.".into(),
            },
            Err(error) => error,
        });
    }

    fn copy_verified_group(&mut self, snapshot: &Arc<DiscoverySnapshot>, group_index: usize) {
        let result = (|| {
            let group = snapshot
                .verified_duplicate_groups
                .get(group_index)
                .ok_or_else(|| "The duplicate group is no longer available.".to_owned())?;
            if group.known_files.len() > MAX_COPY_PATHS {
                return Err(format!(
                    "This group has more than {MAX_COPY_PATHS} paths; copy paths one group at a time after narrowing the scan."
                ));
            }
            let mut proofs = Vec::with_capacity(group.known_files.len());
            let mut total_bytes = 0usize;
            for member in &group.known_files {
                let proof = proof_for_known_alias(snapshot, group.size_group_index, member)?;
                let text = proof
                    .path
                    .to_str()
                    .ok_or_else(|| "A group path cannot be represented safely as text.".to_owned())?
                    .to_owned();
                total_bytes = total_bytes
                    .checked_add(text.len())
                    .and_then(|bytes| bytes.checked_add(1))
                    .ok_or_else(|| "The path list is too large to copy safely.".to_owned())?;
                if total_bytes > MAX_COPY_TEXT_BYTES {
                    return Err("The path list exceeds the 1 MiB clipboard limit.".into());
                }
                proofs.push((proof, text));
            }
            let mut copied = String::with_capacity(total_bytes);
            for (proof, text) in &proofs {
                self.actions.revalidate(proof)?;
                if !Arc::ptr_eq(snapshot, &self.session.view().snapshot) {
                    return Err("The scan results changed before the path list was copied.".into());
                }
                copied.push_str(text);
                copied.push('\n');
            }
            if !Arc::ptr_eq(snapshot, &self.session.view().snapshot) {
                return Err("The scan results changed before the path list was copied.".into());
            }
            self.actions.copy_paths(&copied)
        })();
        #[cfg(any(test, feature = "dev-smoke-binaries"))]
        if self.fixture_action_receipts.len() < 64 {
            self.fixture_action_receipts.push(serde_json::json!({
                "action": "copy_paths",
                "result": if result.is_ok() { "accepted" } else { "refused" },
                "generation": snapshot.generation,
                "group_index": group_index,
            }));
        }
        self.status =
            Some(result.map_or_else(|error| error, |_| "Copied the revalidated paths.".into()));
    }

    fn add_selected_root(&mut self, path: PathBuf) -> Result<(), String> {
        if path.as_os_str().is_empty() {
            return Err("Enter or choose a folder before adding it.".into());
        }
        if self.selected_roots.len() >= 32 {
            return Err("Select no more than 32 folder roots per scan.".into());
        }
        let bytes = encoded_path_bytes(&path);
        let existing = self
            .selected_roots
            .iter()
            .map(|path| encoded_path_bytes(path.as_path()))
            .sum::<usize>();
        if existing.saturating_add(bytes) > 2 * 1024 * 1024 {
            return Err("Selected root paths exceed the 2 MiB selection limit.".into());
        }
        if self.selected_roots.iter().any(|selected| selected == &path) {
            return Err("That exact folder path is already selected.".into());
        }
        self.selected_roots.push(path);
        self.root_editor.clear();
        self.status = None;
        self.bump_target_revision();
        Ok(())
    }

    fn ensure_rows(&mut self, snapshot: &Arc<DiscoverySnapshot>) {
        if self
            .rows_snapshot
            .as_ref()
            .is_some_and(|previous| Arc::ptr_eq(previous, snapshot))
        {
            return;
        }
        self.rows.clear();
        self.rows_truncated = false;
        let mut append = |row| {
            if self.rows.len() < MAX_RESULT_ROWS {
                self.rows.push(row);
            } else {
                self.rows_truncated = true;
            }
        };
        for (group_index, group) in snapshot.verified_duplicate_groups.iter().enumerate() {
            append(ResultRow::VerifiedHeader(group_index));
            for member in 0..group.known_files.len() {
                append(ResultRow::VerifiedMember {
                    group: group_index,
                    member,
                });
            }
        }
        for (group_index, group) in snapshot.identity_unverified_matches.iter().enumerate() {
            append(ResultRow::UnverifiedHeader(group_index));
            for member in 0..group.known_files.len() {
                append(ResultRow::UnverifiedKnownMember {
                    group: group_index,
                    member,
                });
            }
            for path in 0..group.unverified_path_indices.len() {
                append(ResultRow::UnverifiedPath {
                    group: group_index,
                    path,
                });
            }
        }
        self.rows_snapshot = Some(Arc::clone(snapshot));
    }

    fn bump_target_revision(&mut self) {
        self.target_revision = self.target_revision.saturating_add(1);
    }

    fn record_response(&mut self, key: &'static str, response: &egui::Response) {
        #[cfg(any(test, feature = "dev-smoke-binaries"))]
        {
            self.record_geometry(
                key,
                response.id,
                Some(response.layer_id),
                response.interact_rect,
                response.rect,
                response.enabled(),
            );
        }
        #[cfg(not(any(test, feature = "dev-smoke-binaries")))]
        let _ = (key, response);
    }

    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    fn record_scroll_area(
        &mut self,
        key: &'static str,
        id: egui::Id,
        rect: egui::Rect,
        clip_rect: egui::Rect,
        enabled: bool,
    ) {
        self.record_geometry(key, id, None, rect.intersect(clip_rect), rect, enabled);
    }

    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    fn record_geometry(
        &mut self,
        key: &'static str,
        id: egui::Id,
        layer_id: Option<egui::LayerId>,
        rect: egui::Rect,
        response_rect: egui::Rect,
        enabled: bool,
    ) {
        #[cfg(any(test, feature = "dev-smoke-binaries"))]
        self.responses.insert(
            key,
            TestWidgetResponse {
                id,
                layer_id,
                rect,
                response_rect,
                enabled,
            },
        );
        #[cfg(not(any(test, feature = "dev-smoke-binaries")))]
        let _ = (key, id, rect, response_rect, enabled);
    }

    fn record_result_action(
        &mut self,
        action: &'static str,
        group_kind: &'static str,
        group_index: usize,
        member_index: Option<usize>,
        generation: u64,
        response: &egui::Response,
    ) {
        #[cfg(any(test, feature = "dev-smoke-binaries"))]
        {
            if self.result_action_responses.len() >= 512 {
                return;
            }
            self.result_action_responses.push(TestResultActionResponse {
                action,
                group_kind,
                group_index,
                member_index,
                generation,
                response: TestWidgetResponse {
                    id: response.id,
                    layer_id: Some(response.layer_id),
                    rect: response.interact_rect,
                    response_rect: response.rect,
                    enabled: response.enabled(),
                },
            });
        }
        #[cfg(not(any(test, feature = "dev-smoke-binaries")))]
        let _ = (
            action,
            group_kind,
            group_index,
            member_index,
            generation,
            response,
        );
    }
}

#[derive(Clone, Copy)]
enum KnownFileAction {
    Open,
    Reveal,
}

fn proof_for_known_alias(
    snapshot: &DiscoverySnapshot,
    size_group_index: usize,
    member: &VerifiedKnownFile,
) -> Result<CandidateProof, String> {
    let size_group = snapshot
        .size_groups
        .get(size_group_index)
        .ok_or_else(|| "The result's size group is no longer available.".to_owned())?;
    let known = size_group
        .known_files
        .get(member.known_file_index)
        .ok_or_else(|| "The verified file is no longer available.".to_owned())?;
    let alias = known
        .aliases
        .get(member.alias_index)
        .ok_or_else(|| "The verified alias is no longer available.".to_owned())?;
    let root = snapshot
        .roots
        .get(alias.root_index)
        .ok_or_else(|| "The scanned root is no longer available.".to_owned())?;
    let canonical_root = root
        .canonical_root
        .clone()
        .ok_or_else(|| "The selected root was not admitted for scanning.".to_owned())?;
    let root_identity = root
        .identity
        .ok_or_else(|| "The selected root identity is no longer available.".to_owned())?;
    let modified = known.modified.ok_or_else(|| {
        "The file's modification time was not recorded; the action is disabled.".to_owned()
    })?;
    Ok(CandidateProof {
        root: crate::file_operations::DuplicateDiscoveryRootIdentity {
            canonical_root,
            identity: crate::file_operations::NativeFileIdentity::from_parts(
                root_identity.volume_serial_number,
                root_identity.file_id,
            ),
        },
        generation: snapshot.generation,
        path: alias.path.clone(),
        identity: crate::file_operations::NativeFileIdentity::from_parts(
            known.identity.volume_serial_number,
            known.identity.file_id,
        ),
        length: known.size_bytes,
        modified,
    })
}

fn show_diagnostic(ui: &mut egui::Ui, diagnostic: &DiscoveryDiagnostic) {
    ui.horizontal_wrapped(|ui| {
        if let Some(path) = diagnostic.path.as_ref() {
            ui.monospace(path.to_str().unwrap_or("<unrepresentable path>"));
            ui.label("·");
        }
        ui.label(&diagnostic.message);
    });
}

fn register_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    id: impl AsRef<str>,
    label: impl Into<String>,
    kind: crate::gui::jump_mode::TargetKind,
    order: &mut u64,
) {
    crate::gui::jump_mode::ui::register_and_apply_target(
        scope, ui, response, id, label, kind, order,
    );
}

fn root_status_label(status: RootScanStatus) -> &'static str {
    match status {
        RootScanStatus::Pending => "pending",
        RootScanStatus::Scanning => "scanning",
        RootScanStatus::Complete => "complete",
        RootScanStatus::Partial => "partial",
        RootScanStatus::InvalidRoot => "invalid root",
        RootScanStatus::Inaccessible => "inaccessible",
        RootScanStatus::UnsupportedIdentity => "unsupported identity",
        RootScanStatus::ChangedDuringScan => "changed during scan",
        RootScanStatus::ResourceLimit => "resource limit",
        RootScanStatus::Cancelled => "cancelled",
        RootScanStatus::SkippedOverlap => "skipped overlapping root",
        RootScanStatus::SkippedEquivalent => "skipped equivalent root",
    }
}

fn session_error_message(error: &DuplicateFinderSessionError) -> String {
    match error {
        DuplicateFinderSessionError::WorkerRetiring => {
            "The previous scan is still draining. Try again after it retires.".into()
        }
        other => other.to_string(),
    }
}

fn status_name(status: DiscoveryStatus) -> &'static str {
    match status {
        DiscoveryStatus::Idle => "idle",
        DiscoveryStatus::Scanning => "scanning",
        DiscoveryStatus::Complete => "complete",
        DiscoveryStatus::Partial => "partial",
        DiscoveryStatus::Cancelled => "cancelled",
        DiscoveryStatus::Failed => "failed",
        DiscoveryStatus::Closed => "closed",
    }
}

fn stage_name(stage: DiscoveryStage) -> &'static str {
    match stage {
        DiscoveryStage::Traversing => "traversing",
        DiscoveryStage::Hashing => "hashing",
        DiscoveryStage::Comparing => "comparing",
        DiscoveryStage::Complete => "complete",
    }
}

fn encoded_path_bytes(path: &Path) -> usize {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().count().saturating_mul(2)
    }
    #[cfg(not(windows))]
    {
        path.as_os_str().as_encoded_bytes().len()
    }
}

fn build_result_rows(snapshot: &DiscoverySnapshot) -> (Vec<ResultRow>, bool) {
    let mut rows = Vec::new();
    let mut truncated = false;
    let mut append = |row| {
        if rows.len() < MAX_RESULT_ROWS {
            rows.push(row);
        } else {
            truncated = true;
        }
    };
    for (group_index, group) in snapshot.verified_duplicate_groups.iter().enumerate() {
        append(ResultRow::VerifiedHeader(group_index));
        for member in 0..group.known_files.len() {
            append(ResultRow::VerifiedMember {
                group: group_index,
                member,
            });
        }
    }
    for (group_index, group) in snapshot.identity_unverified_matches.iter().enumerate() {
        append(ResultRow::UnverifiedHeader(group_index));
        for member in 0..group.known_files.len() {
            append(ResultRow::UnverifiedKnownMember {
                group: group_index,
                member,
            });
        }
        for path in 0..group.unverified_path_indices.len() {
            append(ResultRow::UnverifiedPath {
                group: group_index,
                path,
            });
        }
    }
    (rows, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::duplicate_finder::{
        DuplicateTotals, FileAlias, IdentityUnverifiedMatch, IdentityUnverifiedPath, KnownFile,
        KnownFileIdentity, SizeGroup, VerifiedDuplicateGroup,
    };

    #[test]
    fn dff_dialog_rows_keep_verified_and_identity_unverified_results_separate() {
        let snapshot = DiscoverySnapshot {
            generation: 7,
            status: DiscoveryStatus::Partial,
            progress: None,
            roots: Vec::new(),
            size_groups: vec![SizeGroup {
                size_bytes: 4,
                known_files: vec![KnownFile {
                    identity: KnownFileIdentity {
                        volume_serial_number: 1,
                        file_id: [2; 16],
                    },
                    size_bytes: 4,
                    modified: None,
                    aliases: vec![FileAlias {
                        root_index: 0,
                        path: PathBuf::from("C:\\fixture\\same.bin"),
                    }],
                }],
                unverified_paths: vec![IdentityUnverifiedPath {
                    root_index: 0,
                    size_bytes: 4,
                    observed_identity: crate::file_operations::NativeFileIdentity::default(),
                    modified: None,
                    path: PathBuf::from("C:\\fixture\\unknown.bin"),
                }],
            }],
            verified_duplicate_groups: vec![VerifiedDuplicateGroup {
                size_group_index: 0,
                sha256: [0; 32],
                known_files: vec![VerifiedKnownFile {
                    known_file_index: 0,
                    alias_index: 0,
                }],
                logical_duplicate_bytes: 4,
            }],
            identity_unverified_matches: vec![IdentityUnverifiedMatch {
                size_group_index: 0,
                sha256: [0; 32],
                known_files: vec![VerifiedKnownFile {
                    known_file_index: 0,
                    alias_index: 0,
                }],
                unverified_path_indices: vec![0],
            }],
            duplicate_totals: DuplicateTotals {
                verified_groups: 1,
                verified_files: 2,
                logical_duplicate_bytes: 4,
                identity_unverified_matches: 1,
            },
            diagnostics: Vec::new(),
            diagnostics_truncated: false,
            counts: Default::default(),
        };

        let (rows, truncated) = build_result_rows(&snapshot);
        assert!(!truncated);
        assert!(matches!(rows[0], ResultRow::VerifiedHeader(0)));
        assert!(matches!(
            rows[1],
            ResultRow::VerifiedMember {
                group: 0,
                member: 0
            }
        ));
        assert!(matches!(rows[2], ResultRow::UnverifiedHeader(0)));
        assert!(matches!(
            rows[3],
            ResultRow::UnverifiedKnownMember {
                group: 0,
                member: 0
            }
        ));
        assert!(matches!(
            rows[4],
            ResultRow::UnverifiedPath { group: 0, path: 0 }
        ));
        assert_eq!(snapshot.duplicate_totals.verified_groups, 1);
        assert_eq!(snapshot.duplicate_totals.logical_duplicate_bytes, 4);
        assert_eq!(snapshot.identity_unverified_matches.len(), 1);
        assert_eq!(snapshot.size_groups[0].unverified_paths[0].size_bytes, 4);
    }

    #[test]
    fn dff_dialog_action_proof_uses_the_exact_verified_alias_index() {
        let mut snapshot = DiscoverySnapshot::idle(4);
        snapshot.roots.push(crate::duplicate_finder::RootScan {
            selected_path: PathBuf::from("C:\\selected"),
            canonical_root: Some(PathBuf::from("C:\\selected")),
            identity: Some(KnownFileIdentity {
                volume_serial_number: 11,
                file_id: [12; 16],
            }),
            status: RootScanStatus::Complete,
            counts: Default::default(),
        });
        snapshot.size_groups.push(SizeGroup {
            size_bytes: 4,
            known_files: vec![KnownFile {
                identity: KnownFileIdentity {
                    volume_serial_number: 13,
                    file_id: [14; 16],
                },
                size_bytes: 4,
                modified: Some(std::time::SystemTime::UNIX_EPOCH),
                aliases: vec![
                    crate::duplicate_finder::FileAlias {
                        root_index: 0,
                        path: PathBuf::from("C:\\selected\\stale.bin"),
                    },
                    crate::duplicate_finder::FileAlias {
                        root_index: 0,
                        path: PathBuf::from("C:\\selected\\current.bin"),
                    },
                ],
            }],
            unverified_paths: Vec::new(),
        });
        let member = VerifiedKnownFile {
            known_file_index: 0,
            alias_index: 1,
        };
        let proof = proof_for_known_alias(&snapshot, 0, &member).unwrap();
        assert_eq!(proof.path, PathBuf::from("C:\\selected\\current.bin"));
        assert_eq!(proof.generation, 4);
        assert_eq!(proof.identity.file_id(), [14; 16]);
        assert_eq!(proof.root.identity.file_id(), [12; 16]);
        struct AcceptProof(std::sync::atomic::AtomicUsize);
        impl DuplicateFinderActions for AcceptProof {
            fn revalidate(&self, _: &CandidateProof) -> Result<(), String> {
                Ok(())
            }
            fn open(&self, _: &Path) -> Result<(), String> {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }
            fn reveal(&self, _: &Path) -> Result<(), String> {
                Ok(())
            }
            fn copy_paths(&self, _: &str) -> Result<(), String> {
                Ok(())
            }
        }
        let ctx = egui::Context::default();
        let mut dialog = DuplicateFinderDialogState::new(&ctx);
        dialog.open();
        let sink = Arc::new(AcceptProof(std::sync::atomic::AtomicUsize::new(0)));
        dialog.set_action_sink_for_fixture(sink.clone());
        dialog.run_known_action(
            &Arc::new(snapshot),
            0,
            &member,
            KnownFileAction::Open,
            "verified",
            0,
            0,
        );
        assert_eq!(
            sink.0.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "stale originating snapshot must not dispatch even when proof revalidation succeeds"
        );
        assert!(
            dialog
                .status
                .as_deref()
                .is_some_and(|status| status.contains("results changed"))
        );
    }

    #[test]
    fn dff_retained_order_applies_root_edits_before_start_and_picker() {
        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().unwrap();
        let a = directory.path().join("a");
        let b = directory.path().join("b");
        let c = directory.path().join("c");
        for path in [&a, &b, &c] {
            std::fs::create_dir(path).unwrap();
        }
        let mut dialog = DuplicateFinderDialogState::new(&ctx);
        dialog.open();
        dialog.add_fixture_root(a.clone()).unwrap();
        dialog.queue_folder_pick_for_test(Some(c.clone()));
        let output = ctx.run_ui(egui::RawInput::default(), |ctx| {
            let root = super::super::utility_host::ViewportPassProvenance::current(
                ctx,
                egui::ViewportId::ROOT,
            );
            dialog.host_pending = Some(FinderHostFrame {
                root,
                owner: root,
                host: dialog.host.identity(),
                epoch: dialog.host_epoch,
                target_revision: dialog.target_revision,
                previous_editor: dialog.root_editor.clone(),
                intents: vec![
                    FinderIntent::RemoveRoot(a.clone()),
                    FinderIntent::AddRoot(b.clone()),
                    FinderIntent::PickRoot,
                    FinderIntent::Start,
                ],
                copies: Vec::new(),
                close: false,
                request: None,
                info: None,
                native: false,
            });
        });
        let _ = dialog.on_hosted_frame_complete(&ctx);
        output.drop_without_applying_deltas();
        assert_eq!(dialog.selected_roots, vec![b.clone(), c.clone()]);
        let view = dialog.session.view();
        assert_eq!(
            view.snapshot
                .roots
                .iter()
                .map(|r| &r.selected_path)
                .collect::<Vec<_>>(),
            vec![&b, &c]
        );
        assert_eq!(dialog.test_folder_picks.len(), 0);
        dialog.shutdown();
    }

    #[test]
    fn dff_dialog_window_geometry_stays_stable_before_and_after_resizing() {
        const INSTANCE: u64 = 0xDFF_6001;

        fn actual_window_rect(ctx: &egui::Context) -> egui::Rect {
            ctx.memory(|memory| memory.area_rect(egui::Id::new(Some(WINDOW_TITLE))))
                .expect("the actual Duplicate Finder window area is retained")
        }

        fn frame_at_size(
            ctx: &egui::Context,
            dialog: &mut DuplicateFinderDialogState,
            screen_size: egui::Vec2,
        ) -> egui::Rect {
            frame_with_events(ctx, dialog, screen_size, Vec::new())
        }

        fn frame_with_events(
            ctx: &egui::Context,
            dialog: &mut DuplicateFinderDialogState,
            screen_size: egui::Vec2,
            events: Vec<egui::Event>,
        ) -> egui::Rect {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size)),
                focused: true,
                events,
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ctx| {
                let first_ui_pass = ctx.current_pass_index() == 0;
                dialog.show_with_jump_mode(
                    ctx,
                    None,
                    Some(INSTANCE),
                    crate::gui::jump_mode::ui::NavigationOwner::Dialog {
                        kind: crate::gui::jump_mode::ui::NavigationDialogKind::DuplicateFinder,
                        instance: INSTANCE,
                    },
                    true,
                    true,
                    first_ui_pass,
                );
            });
            let _ = dialog.on_hosted_frame_complete(ctx);
            output.drop_without_applying_deltas();
            actual_window_rect(ctx)
        }

        fn resize_by(
            ctx: &egui::Context,
            dialog: &mut DuplicateFinderDialogState,
            screen_size: egui::Vec2,
            delta: egui::Vec2,
        ) -> egui::Rect {
            let rect = actual_window_rect(ctx);
            let start = rect.right_bottom() - egui::vec2(4.0, 4.0);
            let end = start + delta;
            frame_with_events(
                ctx,
                dialog,
                screen_size,
                vec![
                    egui::Event::PointerMoved(start),
                    egui::Event::PointerButton {
                        pos: start,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            frame_with_events(
                ctx,
                dialog,
                screen_size,
                vec![egui::Event::PointerMoved(end)],
            );
            frame_with_events(
                ctx,
                dialog,
                screen_size,
                vec![egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            )
        }

        fn assert_rects_stay_stable(rects: &[egui::Rect], context: &str) -> egui::Rect {
            let settled = rects[8];
            for (frame, rect) in rects.iter().enumerate().skip(8) {
                assert!(
                    (rect.height() - settled.height()).abs() <= 0.5
                        && (rect.top() - settled.top()).abs() <= 0.5
                        && (rect.bottom() - settled.bottom()).abs() <= 0.5,
                    "actual window geometry stays stable after convergence: context={context}, frame={frame}, settled={settled:?}, actual={rect:?}",
                );
            }
            settled
        }

        fn assert_footer_visible(
            dialog: &DuplicateFinderDialogState,
            screen_size: egui::Vec2,
            context: &str,
        ) {
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size);
            for key in ["start", "cancel", "close"] {
                let response = dialog
                    .responses
                    .get(key)
                    .unwrap_or_else(|| panic!("the actual {key} footer response is rendered"));
                assert!(
                    screen.contains_rect(response.response_rect),
                    "the complete {key} footer response remains reachable: context={context}, response={response:?}, screen={screen:?}",
                );
            }
        }

        fn scroll_until_results_visible(
            ctx: &egui::Context,
            dialog: &mut DuplicateFinderDialogState,
            screen_size: egui::Vec2,
        ) -> egui::Rect {
            for _ in 0..12 {
                let results = dialog.responses["results-scroll"];
                if results.rect.is_positive() {
                    return actual_window_rect(ctx);
                }
                let content = dialog.responses["content-scroll"];
                assert!(
                    content.rect.is_positive(),
                    "the outer content viewport remains available while revealing results: content={content:?}, results={results:?}",
                );
                let delta_y = if results.response_rect.bottom() > content.rect.bottom() {
                    (results.response_rect.bottom() - content.rect.bottom() + 8.0).min(160.0)
                } else if results.response_rect.top() < content.rect.top() {
                    -(content.rect.top() - results.response_rect.top() + 8.0).min(160.0)
                } else {
                    panic!(
                        "the results viewport should be visible or scrollable within the outer content area: content={content:?}, results={results:?}",
                    );
                };
                let wheel_position =
                    egui::pos2(content.rect.left() + 8.0, content.rect.top() + 4.0);
                frame_with_events(
                    ctx,
                    dialog,
                    screen_size,
                    vec![
                        egui::Event::PointerMoved(wheel_position),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, -delta_y),
                            phase: egui::TouchPhase::Move,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            panic!(
                "bounded outer-content scrolling reveals the results viewport; content={:?}, results={:?}",
                dialog.responses.get("content-scroll"),
                dialog.responses.get("results-scroll"),
            );
        }

        fn crowded_snapshot() -> DiscoverySnapshot {
            let roots = (0..32)
                .map(|index| PathBuf::from(format!("C:\\fixture\\root-{index:02}")))
                .collect::<Vec<_>>();
            let content_size = b"identical generated content".len() as u64;
            DiscoverySnapshot {
                generation: 40,
                status: DiscoveryStatus::Partial,
                progress: None,
                roots: roots
                    .iter()
                    .map(|path| crate::duplicate_finder::RootScan {
                        selected_path: path.clone(),
                        canonical_root: None,
                        identity: None,
                        status: RootScanStatus::Complete,
                        counts: Default::default(),
                    })
                    .collect(),
                size_groups: vec![SizeGroup {
                    size_bytes: content_size,
                    known_files: vec![
                        KnownFile {
                            identity: KnownFileIdentity {
                                volume_serial_number: 1,
                                file_id: [2; 16],
                            },
                            size_bytes: content_size,
                            modified: Some(std::time::SystemTime::UNIX_EPOCH),
                            aliases: vec![FileAlias {
                                root_index: 0,
                                path: roots[0].join("first.bin"),
                            }],
                        },
                        KnownFile {
                            identity: KnownFileIdentity {
                                volume_serial_number: 1,
                                file_id: [3; 16],
                            },
                            size_bytes: content_size,
                            modified: Some(std::time::SystemTime::UNIX_EPOCH),
                            aliases: vec![FileAlias {
                                root_index: 0,
                                path: roots[0].join("second.bin"),
                            }],
                        },
                    ],
                    unverified_paths: Vec::new(),
                }],
                verified_duplicate_groups: vec![VerifiedDuplicateGroup {
                    size_group_index: 0,
                    sha256: [7; 32],
                    known_files: vec![
                        VerifiedKnownFile {
                            known_file_index: 0,
                            alias_index: 0,
                        },
                        VerifiedKnownFile {
                            known_file_index: 1,
                            alias_index: 0,
                        },
                    ],
                    logical_duplicate_bytes: content_size,
                }],
                identity_unverified_matches: Vec::new(),
                duplicate_totals: DuplicateTotals {
                    verified_groups: 1,
                    verified_files: 2,
                    logical_duplicate_bytes: content_size,
                    identity_unverified_matches: 0,
                },
                diagnostics: (0..64)
                    .map(|index| DiscoveryDiagnostic {
                        root_index: Some(index % roots.len()),
                        path: None,
                        message: format!("generated diagnostic {index}"),
                    })
                    .collect(),
                diagnostics_truncated: false,
                counts: Default::default(),
            }
        }

        for (snapshot_name, is_crowded) in [("idle", false), ("crowded", true)] {
            for viewport in [egui::vec2(1180.0, 860.0), egui::vec2(620.0, 420.0)] {
                let ctx = egui::Context::default();
                let mut theme_settings = crate::settings::ThemeSettings::default();
                if is_crowded {
                    theme_settings.preferences.density = crate::settings::ThemeDensity::Comfortable;
                    theme_settings.preferences.typography_scale = 1.5;
                }
                if is_crowded && viewport.x == 1180.0 {
                    ctx.set_pixels_per_point(1.25);
                }
                crate::gui::theme::initialize_runtime(&ctx, &theme_settings);
                ctx.all_styles_mut(|style| style.animation_time = 0.0);
                let mut dialog = DuplicateFinderDialogState::new(&ctx);
                dialog.open();
                if is_crowded {
                    let snapshot = crowded_snapshot();
                    dialog.selected_roots = snapshot
                        .roots
                        .iter()
                        .map(|root| root.selected_path.clone())
                        .collect();
                    dialog.set_snapshot_for_test(snapshot);
                }
                frame_at_size(&ctx, &mut dialog, viewport);
                frame_at_size(&ctx, &mut dialog, viewport);
                if is_crowded && viewport.x == 1180.0 {
                    let position = dialog.responses["diagnostics-toggle"].rect.center();
                    frame_with_events(
                        &ctx,
                        &mut dialog,
                        viewport,
                        vec![
                            egui::Event::PointerMoved(position),
                            egui::Event::PointerButton {
                                pos: position,
                                button: egui::PointerButton::Primary,
                                pressed: true,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                    );
                    frame_with_events(
                        &ctx,
                        &mut dialog,
                        viewport,
                        vec![egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                    );
                    assert!(
                        dialog.responses.contains_key("diagnostics-scroll"),
                        "the actual diagnostics body expanded before its geometry is sampled"
                    );
                }
                let mut rects = Vec::new();
                for _ in 0..64 {
                    rects.push(frame_at_size(&ctx, &mut dialog, viewport));
                }
                let context = format!(
                    "{snapshot_name}, viewport={viewport:?}, ppp={}, theme={:?}",
                    ctx.pixels_per_point(),
                    theme_settings.preferences,
                );
                let settled = assert_rects_stay_stable(&rects, &context);
                assert_footer_visible(&dialog, viewport, &context);
                if is_crowded {
                    assert!(dialog.responses["root-status-scroll"].rect.is_positive());
                    if viewport.x > 1000.0 {
                        assert!(dialog.responses["results-scroll"].rect.is_positive());
                    } else {
                        let after_scroll =
                            scroll_until_results_visible(&ctx, &mut dialog, viewport);
                        assert!(dialog.responses["results-scroll"].rect.is_positive());
                        assert!(
                            (after_scroll.height() - settled.height()).abs() <= 0.5
                                && (after_scroll.top() - settled.top()).abs() <= 0.5,
                            "scrolling the crowded body does not change the user-visible window geometry: settled={settled:?}, after_scroll={after_scroll:?}",
                        );
                        assert_footer_visible(&dialog, viewport, &context);
                    }
                    assert!(
                        dialog.responses["diagnostics-toggle"]
                            .response_rect
                            .is_positive()
                    );
                }
                if is_crowded && viewport.x == 1180.0 {
                    let enlarged = resize_by(&ctx, &mut dialog, viewport, egui::vec2(40.0, 80.0));
                    assert!(
                        enlarged.height() >= settled.height() + 70.0,
                        "a pointer drag on the window's resize grip applies the requested height increase: before={settled:?}, after={enlarged:?}"
                    );
                    let after_resize = (0..48)
                        .map(|_| frame_at_size(&ctx, &mut dialog, viewport))
                        .collect::<Vec<_>>();
                    let resized_stable =
                        assert_rects_stay_stable(&after_resize, "after user resize up");
                    assert!(
                        (resized_stable.height() - enlarged.height()).abs() <= 0.5,
                        "the user-selected size persists after idle frames: resized={enlarged:?}, settled={resized_stable:?}"
                    );
                    assert_footer_visible(&dialog, viewport, "after user resize up");
                    let reduced = resize_by(&ctx, &mut dialog, viewport, egui::vec2(-40.0, -60.0));
                    assert!(
                        reduced.height() <= resized_stable.height() - 50.0,
                        "a pointer drag on the window's resize grip applies the requested height decrease: before={resized_stable:?}, after={reduced:?}"
                    );
                    let after_resize_down = (0..48)
                        .map(|_| frame_at_size(&ctx, &mut dialog, viewport))
                        .collect::<Vec<_>>();
                    let resized_down_stable =
                        assert_rects_stay_stable(&after_resize_down, "after user resize down");
                    assert!(
                        (resized_down_stable.height() - reduced.height()).abs() <= 0.5,
                        "the reduced user-selected size persists after idle frames: resized={reduced:?}, settled={resized_down_stable:?}"
                    );
                    assert_footer_visible(&dialog, viewport, "after user resize down");
                }
                dialog.shutdown();
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn dff_dialog_response_flow_cancels_drains_restarts_and_uses_current_verified_actions() {
        use std::sync::Mutex;
        use std::time::{Duration, Instant};

        #[derive(Default)]
        struct RecordingActions {
            revalidated: Mutex<Vec<PathBuf>>,
            opened: Mutex<Vec<PathBuf>>,
            revealed: Mutex<Vec<PathBuf>>,
            copied: Mutex<Vec<String>>,
        }

        impl DuplicateFinderActions for RecordingActions {
            fn revalidate(&self, proof: &CandidateProof) -> Result<(), String> {
                SystemDuplicateFinderActions.revalidate(proof)?;
                self.revalidated.lock().unwrap().push(proof.path.clone());
                Ok(())
            }

            fn open(&self, path: &Path) -> Result<(), String> {
                self.opened.lock().unwrap().push(path.to_owned());
                Ok(())
            }

            fn reveal(&self, path: &Path) -> Result<(), String> {
                self.revealed.lock().unwrap().push(path.to_owned());
                Ok(())
            }

            fn copy_paths(&self, text: &str) -> Result<(), String> {
                self.copied.lock().unwrap().push(text.to_owned());
                Ok(())
            }
        }

        const INSTANCE: u64 = 0xDFF_C011;

        fn input(events: Vec<egui::Event>, screen_size: egui::Vec2) -> egui::RawInput {
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size)),
                focused: true,
                events,
                ..Default::default()
            }
        }

        fn frame(
            ctx: &egui::Context,
            dialog: &mut DuplicateFinderDialogState,
            events: Vec<egui::Event>,
        ) {
            frame_at_size(ctx, dialog, events, egui::vec2(1180.0, 860.0));
        }

        fn frame_at_size(
            ctx: &egui::Context,
            dialog: &mut DuplicateFinderDialogState,
            events: Vec<egui::Event>,
            screen_size: egui::Vec2,
        ) {
            let output = ctx.run_ui(input(events, screen_size), |ctx| {
                let first_ui_pass = ctx.current_pass_index() == 0;
                dialog.show_with_jump_mode(
                    ctx,
                    None,
                    Some(INSTANCE),
                    crate::gui::jump_mode::ui::NavigationOwner::Dialog {
                        kind: crate::gui::jump_mode::ui::NavigationDialogKind::DuplicateFinder,
                        instance: INSTANCE,
                    },
                    true,
                    true,
                    first_ui_pass,
                );
            });
            let _ = dialog.on_hosted_frame_complete(ctx);
            output.drop_without_applying_deltas();
        }

        fn click_response(
            ctx: &egui::Context,
            dialog: &mut DuplicateFinderDialogState,
            response: TestWidgetResponse,
        ) {
            click_response_at_size(ctx, dialog, response, egui::vec2(1180.0, 860.0));
        }

        fn click_response_at_size(
            ctx: &egui::Context,
            dialog: &mut DuplicateFinderDialogState,
            response: TestWidgetResponse,
            screen_size: egui::Vec2,
        ) {
            assert!(response.enabled, "the actual current response is enabled");
            assert!(
                response.rect.is_positive(),
                "the actual response is positively clipped"
            );
            assert_eq!(
                crate::gui::jump_mode::ui::visible_top_middle_layer(ctx),
                crate::gui::jump_mode::ui::NavigationDialogKind::DuplicateFinder.layer_id(),
                "Duplicate Finder owns the current top ordinary window",
            );
            let position = response.rect.center();
            frame_at_size(
                ctx,
                dialog,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                screen_size,
            );
            frame_at_size(
                ctx,
                dialog,
                vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                screen_size,
            );
        }

        fn click_control(
            ctx: &egui::Context,
            dialog: &mut DuplicateFinderDialogState,
            key: &'static str,
        ) {
            frame(ctx, dialog, Vec::new());
            let response = *dialog
                .responses
                .get(key)
                .unwrap_or_else(|| panic!("actual {key} response is rendered"));
            click_response(ctx, dialog, response);
        }

        fn click_control_at_size(
            ctx: &egui::Context,
            dialog: &mut DuplicateFinderDialogState,
            key: &'static str,
            screen_size: egui::Vec2,
        ) {
            frame_at_size(ctx, dialog, Vec::new(), screen_size);
            let response = *dialog
                .responses
                .get(key)
                .unwrap_or_else(|| panic!("actual {key} response is rendered"));
            click_response_at_size(ctx, dialog, response, screen_size);
        }

        fn assert_fully_visible(
            dialog: &DuplicateFinderDialogState,
            key: &'static str,
            screen_size: egui::Vec2,
        ) {
            let response = dialog
                .responses
                .get(key)
                .unwrap_or_else(|| panic!("actual {key} response is rendered"));
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size);
            assert!(
                response.rect.is_positive(),
                "{key} has a visible response; clipped_rect={:?}, response_rect={:?}, viewport={viewport:?}",
                response.rect,
                response.response_rect,
            );
            assert!(
                viewport.contains_rect(response.response_rect),
                "the full {key} response remains inside the minimum viewport; clipped_rect={:?}, response_rect={:?}, viewport={viewport:?}",
                response.rect,
                response.response_rect,
            );
            assert_eq!(
                response.rect, response.response_rect,
                "the full {key} response is not clipped; viewport={viewport:?}"
            );
        }

        fn wait_for(
            ctx: &egui::Context,
            dialog: &mut DuplicateFinderDialogState,
            predicate: impl Fn(&DuplicateFinderDialogState) -> bool,
            message: &str,
        ) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                frame(ctx, dialog, Vec::new());
                if predicate(dialog) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let view = dialog.session.view();
            panic!(
                "timed out waiting for {message}; busy={}, closed={}, status={:?}, generation={}, progress={:?}, diagnostics={:?}, snapshot_roots={}, selected_roots={}, feedback={:?}, start_response={:?}, interaction_eligible={}",
                view.busy,
                view.closed,
                view.snapshot.status,
                view.snapshot.generation,
                view.progress,
                view.snapshot.diagnostics,
                view.snapshot.roots.len(),
                dialog.selected_roots.len(),
                dialog.status,
                dialog.responses.get("start"),
                dialog.responses_interaction_eligible
            );
        }

        fn click_result_action(
            ctx: &egui::Context,
            dialog: &mut DuplicateFinderDialogState,
            action: &'static str,
            group_index: usize,
            member_index: Option<usize>,
        ) {
            click_result_action_at_size(
                ctx,
                dialog,
                action,
                group_index,
                member_index,
                egui::vec2(1180.0, 860.0),
                false,
            );
        }

        fn click_result_action_at_size(
            ctx: &egui::Context,
            dialog: &mut DuplicateFinderDialogState,
            action: &'static str,
            group_index: usize,
            member_index: Option<usize>,
            screen_size: egui::Vec2,
            require_full_visibility: bool,
        ) {
            if require_full_visibility {
                scroll_body_to_result_action(
                    ctx,
                    dialog,
                    action,
                    group_index,
                    member_index,
                    screen_size,
                );
            } else {
                frame_at_size(ctx, dialog, Vec::new(), screen_size);
            }
            let response =
                current_result_action_response(dialog, action, group_index, member_index);
            if require_full_visibility {
                let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size);
                assert!(
                    viewport.contains_rect(response.response_rect),
                    "the full {action} response for a long path is inside the viewport; clipped_rect={:?}, response_rect={:?}, viewport={viewport:?}",
                    response.rect,
                    response.response_rect,
                );
                assert_eq!(
                    response.rect, response.response_rect,
                    "the {action} response is not clipped by the long path; viewport={viewport:?}"
                );
            }
            click_response_at_size(ctx, dialog, response, screen_size);
        }

        fn current_result_action_response(
            dialog: &DuplicateFinderDialogState,
            action: &'static str,
            group_index: usize,
            member_index: Option<usize>,
        ) -> TestWidgetResponse {
            let generation = dialog.session.view().snapshot.generation;
            dialog
                .result_action_responses
                .iter()
                .find(|candidate| {
                    candidate.action == action
                        && candidate.group_kind == "verified"
                        && candidate.group_index == group_index
                        && candidate.member_index == member_index
                        && candidate.generation == generation
                })
                .unwrap_or_else(|| {
                    panic!(
                        "actual {action} response for verified group {group_index}, member {member_index:?} is rendered"
                    )
                })
                .response
        }

        fn scroll_body_to_result_action(
            ctx: &egui::Context,
            dialog: &mut DuplicateFinderDialogState,
            action: &'static str,
            group_index: usize,
            member_index: Option<usize>,
            screen_size: egui::Vec2,
        ) {
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size);
            for _ in 0..12 {
                frame_at_size(ctx, dialog, Vec::new(), screen_size);
                let action_response =
                    current_result_action_response(dialog, action, group_index, member_index);
                let content = *dialog
                    .responses
                    .get("content-scroll")
                    .expect("the bounded content scroll response is recorded");
                if content.rect.contains_rect(action_response.response_rect)
                    && action_response.rect == action_response.response_rect
                    && screen.contains_rect(action_response.response_rect)
                {
                    return;
                }
                assert!(
                    content.rect.is_positive(),
                    "the current content scroll viewport is visible; content_rect={:?}, action_rect={:?}, screen={screen:?}",
                    content.rect,
                    action_response.response_rect,
                );
                let delta_y = if action_response.response_rect.bottom() > content.rect.bottom() {
                    (action_response.response_rect.bottom() - content.rect.bottom() + 8.0)
                        .min(160.0)
                } else if action_response.response_rect.top() < content.rect.top() {
                    -(content.rect.top() - action_response.response_rect.top() + 8.0).min(160.0)
                } else {
                    panic!(
                        "the current result response is clipped outside its outer content area; content_rect={:?}, action_rect={:?}, clipped_rect={:?}, screen={screen:?}",
                        content.rect, action_response.response_rect, action_response.rect,
                    );
                };
                let wheel_position =
                    egui::pos2(content.rect.left() + 8.0, content.rect.top() + 4.0);
                frame_at_size(
                    ctx,
                    dialog,
                    vec![
                        egui::Event::PointerMoved(wheel_position),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, -delta_y),
                            phase: egui::TouchPhase::Move,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    screen_size,
                );
            }
            let response =
                current_result_action_response(dialog, action, group_index, member_index);
            let content = dialog.responses.get("content-scroll");
            panic!(
                "bounded content scrolling did not reveal {action}; content_rect={:?}, clipped_rect={:?}, action_rect={:?}, screen={screen:?}",
                content.map(|content| content.rect),
                response.rect,
                response.response_rect,
            );
        }

        fn crowded_snapshot(roots: &[PathBuf]) -> DiscoverySnapshot {
            let content_size = b"identical generated content".len() as u64;
            DiscoverySnapshot {
                generation: 40,
                status: DiscoveryStatus::Partial,
                progress: None,
                roots: roots
                    .iter()
                    .map(|path| crate::duplicate_finder::RootScan {
                        selected_path: path.clone(),
                        canonical_root: None,
                        identity: None,
                        status: RootScanStatus::Complete,
                        counts: Default::default(),
                    })
                    .collect(),
                size_groups: vec![crate::duplicate_finder::SizeGroup {
                    size_bytes: content_size,
                    known_files: vec![
                        crate::duplicate_finder::KnownFile {
                            identity: crate::duplicate_finder::KnownFileIdentity {
                                volume_serial_number: 1,
                                file_id: [2; 16],
                            },
                            size_bytes: content_size,
                            modified: Some(std::time::SystemTime::UNIX_EPOCH),
                            aliases: vec![crate::duplicate_finder::FileAlias {
                                root_index: 0,
                                path: roots[0].join("first.bin"),
                            }],
                        },
                        crate::duplicate_finder::KnownFile {
                            identity: crate::duplicate_finder::KnownFileIdentity {
                                volume_serial_number: 1,
                                file_id: [3; 16],
                            },
                            size_bytes: content_size,
                            modified: Some(std::time::SystemTime::UNIX_EPOCH),
                            aliases: vec![crate::duplicate_finder::FileAlias {
                                root_index: 0,
                                path: roots[0].join("second.bin"),
                            }],
                        },
                    ],
                    unverified_paths: Vec::new(),
                }],
                verified_duplicate_groups: vec![crate::duplicate_finder::VerifiedDuplicateGroup {
                    size_group_index: 0,
                    sha256: [7; 32],
                    known_files: vec![
                        VerifiedKnownFile {
                            known_file_index: 0,
                            alias_index: 0,
                        },
                        VerifiedKnownFile {
                            known_file_index: 1,
                            alias_index: 0,
                        },
                    ],
                    logical_duplicate_bytes: content_size,
                }],
                identity_unverified_matches: Vec::new(),
                duplicate_totals: crate::duplicate_finder::DuplicateTotals {
                    verified_groups: 1,
                    verified_files: 2,
                    logical_duplicate_bytes: content_size,
                    identity_unverified_matches: 0,
                },
                diagnostics: (0..64)
                    .map(|index| DiscoveryDiagnostic {
                        root_index: Some(index % roots.len()),
                        path: None,
                        message: format!("generated diagnostic {index}"),
                    })
                    .collect(),
                diagnostics_truncated: false,
                counts: Default::default(),
            }
        }

        let directory = tempfile::tempdir().expect("owned temporary scan tree");
        let roots = (0..32)
            .map(|index| directory.path().join(format!("root-{index:02}")))
            .collect::<Vec<_>>();
        for root in &roots {
            std::fs::create_dir(root).expect("create owned scan root");
        }
        let root = &roots[0];
        let long_leaf = "x".repeat(128);
        let first_path = root.join(format!("first-{long_leaf}.bin"));
        let second_path = root.join(format!("second-{long_leaf}.bin"));
        std::fs::write(&first_path, b"identical generated content")
            .expect("write first generated file");
        std::fs::write(&second_path, b"identical generated content")
            .expect("write second generated file");
        std::fs::write(root.join("unique.bin"), b"different length")
            .expect("write unique generated file");

        let context = egui::Context::default();
        context.all_styles_mut(|style| style.animation_time = 0.0);
        let checkpoint = Arc::new(crate::duplicate_finder::FirstReadCheckpointGate::new());
        let mut dialog = DuplicateFinderDialogState::new_with_first_read_checkpoint(
            &context,
            Arc::clone(&checkpoint),
        );
        dialog.set_release_fixture_gate_on_close(false);
        let actions = Arc::new(RecordingActions::default());
        dialog.set_action_sink_for_fixture(actions.clone());
        for root in &roots {
            dialog
                .add_fixture_root(root.clone())
                .expect("select generated root");
        }
        dialog.open();
        frame(&context, &mut dialog, Vec::new());
        frame(&context, &mut dialog, Vec::new());

        click_control(&context, &mut dialog, "start");
        let after_start = dialog.session.view();
        assert!(
            after_start.busy || checkpoint.reached(),
            "real Start did not start a live worker: closed={}, status={:?}, generation={}, roots={}, feedback={:?}, response_enabled={:?}, interaction_eligible={}",
            after_start.closed,
            after_start.snapshot.status,
            after_start.snapshot.generation,
            dialog.selected_roots.len(),
            dialog.status,
            dialog.responses.get("start").map(|r| r.enabled),
            dialog.responses_interaction_eligible
        );
        wait_for(
            &context,
            &mut dialog,
            |_| checkpoint.reached(),
            "the actual first content read checkpoint",
        );
        assert!(dialog.session.view().busy);
        let started_generation = dialog.session.view().snapshot.generation;

        click_control(&context, &mut dialog, "cancel");
        let cancelled = dialog.session.view();
        assert_eq!(cancelled.snapshot.status, DiscoveryStatus::Cancelled);
        assert!(
            cancelled.busy,
            "the gated read still owns the one worker slot"
        );
        assert!(cancelled.snapshot.verified_duplicate_groups.is_empty());
        assert!(cancelled.snapshot.identity_unverified_matches.is_empty());
        let cancelled_generation = cancelled.snapshot.generation;
        assert!(cancelled_generation > started_generation);
        assert_eq!(
            dialog.session.start_scan(roots.clone()),
            Err(DuplicateFinderSessionError::WorkerRetiring),
            "the domain refuses a second scan while cancelled I/O drains",
        );
        let still_cancelled = dialog.session.view();
        assert_eq!(still_cancelled.snapshot.generation, cancelled_generation);
        assert_eq!(still_cancelled.snapshot.status, DiscoveryStatus::Cancelled);
        assert!(
            still_cancelled
                .snapshot
                .verified_duplicate_groups
                .is_empty()
        );

        dialog.set_snapshot_for_test(crowded_snapshot(&roots));
        let footer_raw_rects = |dialog: &DuplicateFinderDialogState| {
            [
                dialog.responses["start"].response_rect,
                dialog.responses["cancel"].response_rect,
                dialog.responses["close"].response_rect,
            ]
        };
        let normal_viewport = egui::vec2(1180.0, 860.0);
        frame_at_size(&context, &mut dialog, Vec::new(), normal_viewport);
        let normal_footer = footer_raw_rects(&dialog);
        for key in ["start", "cancel", "close"] {
            assert_fully_visible(&dialog, key, normal_viewport);
        }
        for pass in 0..8 {
            frame_at_size(&context, &mut dialog, Vec::new(), normal_viewport);
            assert_eq!(
                footer_raw_rects(&dialog),
                normal_footer,
                "the footer does not drift at normal size on held-read frame {pass}",
            );
            for key in ["start", "cancel", "close"] {
                assert_fully_visible(&dialog, key, normal_viewport);
            }
        }

        let minimum_viewport = egui::vec2(620.0, 420.0);
        frame_at_size(&context, &mut dialog, Vec::new(), minimum_viewport);
        frame_at_size(&context, &mut dialog, Vec::new(), minimum_viewport);
        assert!(!dialog.responses["start"].enabled);
        assert!(dialog.responses["cancel"].enabled);
        assert!(dialog.responses["fixture-release"].enabled);
        assert!(dialog.responses["root-status-scroll"].rect.is_positive());
        assert!(dialog.responses["diagnostics-toggle"].rect.is_positive());
        assert!(dialog.responses["results-scroll"].rect.is_positive());
        assert!(
            !dialog.responses.contains_key("diagnostics-scroll"),
            "the body starts with diagnostics collapsed",
        );
        assert_fully_visible(&dialog, "start", minimum_viewport);
        assert_fully_visible(&dialog, "cancel", minimum_viewport);
        assert_fully_visible(&dialog, "close", minimum_viewport);
        let settled_footer = footer_raw_rects(&dialog);
        frame_at_size(&context, &mut dialog, Vec::new(), minimum_viewport);
        assert_eq!(
            footer_raw_rects(&dialog),
            settled_footer,
            "the fixed Start/Cancel/Close footer stays put on consecutive held-read frames",
        );
        click_control_at_size(
            &context,
            &mut dialog,
            "diagnostics-toggle",
            minimum_viewport,
        );
        let expanded_footer = footer_raw_rects(&dialog);
        assert!(dialog.responses["diagnostics-scroll"].rect.is_positive());
        assert!(dialog.responses["start"].rect.is_positive());
        assert!(dialog.responses["cancel"].rect.is_positive());
        assert!(dialog.responses["results-scroll"].rect.is_positive());
        assert_fully_visible(&dialog, "start", minimum_viewport);
        assert_fully_visible(&dialog, "cancel", minimum_viewport);
        assert_fully_visible(&dialog, "close", minimum_viewport);
        assert_eq!(
            expanded_footer, settled_footer,
            "expanding the status body does not move the reserved footer",
        );
        frame_at_size(&context, &mut dialog, Vec::new(), minimum_viewport);
        assert_eq!(
            footer_raw_rects(&dialog),
            expanded_footer,
            "the footer remains stable on the following held-read frame",
        );
        assert_fully_visible(&dialog, "start", minimum_viewport);
        assert_fully_visible(&dialog, "cancel", minimum_viewport);
        assert_fully_visible(&dialog, "close", minimum_viewport);
        dialog.test_snapshot_override = None;

        click_control(&context, &mut dialog, "close");
        assert!(!dialog.open);
        assert!(dialog.session.view().closed);
        assert!(dialog.session.view().busy);
        dialog.open();
        assert!(dialog.open);
        assert!(dialog.reopen_pending);
        frame(&context, &mut dialog, Vec::new());
        assert!(!dialog.responses["start"].enabled);
        assert!(dialog.responses["close"].rect.is_positive());
        checkpoint.release();
        let retirement_deadline = Instant::now() + Duration::from_secs(10);
        while !dialog.session.worker_reaped_for_test() {
            assert!(
                Instant::now() < retirement_deadline,
                "the reaper did not publish actual worker retirement"
            );
            std::thread::yield_now();
        }
        // The only frame after the reaper's actual-join notification must be
        // enough to reopen the same owner and make Start usable; no frame loop
        // is allowed to hide an early worker-exit wakeup race.
        frame(&context, &mut dialog, Vec::new());
        assert!(!dialog.reopen_pending);
        assert!(!dialog.session.view().closed);
        assert!(!dialog.session.view().busy);
        assert_eq!(dialog.session.view().snapshot.status, DiscoveryStatus::Idle);
        assert!(dialog.responses["start"].enabled);
        assert!(
            dialog
                .session
                .view()
                .snapshot
                .verified_duplicate_groups
                .is_empty()
        );
        assert!(
            dialog
                .session
                .view()
                .snapshot
                .identity_unverified_matches
                .is_empty()
        );

        click_control(&context, &mut dialog, "start");
        wait_for(
            &context,
            &mut dialog,
            |dialog| {
                let view = dialog.session.view();
                !view.busy
                    && matches!(
                        view.snapshot.status,
                        DiscoveryStatus::Complete | DiscoveryStatus::Partial
                    )
            },
            "the restarted scan to publish its terminal snapshot",
        );
        let completed = dialog.session.view();
        assert!(completed.snapshot.generation > cancelled_generation);
        assert_eq!(completed.snapshot.verified_duplicate_groups.len(), 1);
        assert_eq!(
            completed.snapshot.verified_duplicate_groups[0]
                .known_files
                .len(),
            2
        );
        assert_eq!(completed.snapshot.identity_unverified_matches.len(), 0);

        let verified_group = &completed.snapshot.verified_duplicate_groups[0];
        let first_member = verified_group.known_files[0];
        let second_member = verified_group.known_files[1];
        let expected_open = proof_for_known_alias(
            &completed.snapshot,
            verified_group.size_group_index,
            &first_member,
        )
        .expect("first exact verified alias has action proof")
        .path;
        let expected_reveal = proof_for_known_alias(
            &completed.snapshot,
            verified_group.size_group_index,
            &second_member,
        )
        .expect("second exact verified alias has action proof")
        .path;
        let expected_copy = verified_group
            .known_files
            .iter()
            .map(|member| {
                proof_for_known_alias(&completed.snapshot, verified_group.size_group_index, member)
                    .expect("each copied alias has its own exact proof")
                    .path
            })
            .collect::<Vec<_>>();
        let completed_generation = completed.snapshot.generation;
        drop(completed);

        assert!(
            expected_open
                .to_str()
                .expect("generated path is representable")
                .chars()
                .count()
                > 145,
            "the action-flow fixture exercises a long real path",
        );
        let minimum_width_viewport = egui::vec2(620.0, 860.0);
        click_result_action_at_size(
            &context,
            &mut dialog,
            "open",
            0,
            Some(0),
            minimum_width_viewport,
            true,
        );
        click_result_action_at_size(
            &context,
            &mut dialog,
            "reveal",
            0,
            Some(1),
            minimum_width_viewport,
            true,
        );
        click_result_action(&context, &mut dialog, "copy_paths", 0, None);

        assert_eq!(*actions.opened.lock().unwrap(), vec![expected_open.clone()]);
        assert_eq!(
            *actions.revealed.lock().unwrap(),
            vec![expected_reveal.clone()]
        );
        let copied = actions.copied.lock().unwrap();
        assert_eq!(copied.len(), 1);
        let mut copied_paths = copied[0].lines().map(PathBuf::from).collect::<Vec<_>>();
        copied_paths.sort();
        let mut expected_copy = expected_copy;
        expected_copy.sort();
        assert_eq!(copied_paths, expected_copy);
        drop(copied);

        let revalidated = actions.revalidated.lock().unwrap();
        assert_eq!(
            revalidated.len(),
            4,
            "open, reveal, and two copied aliases are revalidated"
        );
        assert!(revalidated.contains(&expected_open));
        assert!(revalidated.contains(&expected_reveal));
        assert!(revalidated.contains(&first_path));
        assert!(revalidated.contains(&second_path));
        drop(revalidated);
        assert_eq!(
            std::fs::read(&first_path).expect("first generated file remains readable"),
            b"identical generated content"
        );
        assert_eq!(
            std::fs::read(&second_path).expect("second generated file remains readable"),
            b"identical generated content"
        );

        let stale_path = expected_open.with_extension("temporarily-stale");
        assert!(
            !stale_path.exists(),
            "the stale-path fixture name is unused"
        );
        std::fs::rename(&expected_open, &stale_path)
            .expect("temporarily remove the exact scanned alias");
        click_result_action(&context, &mut dialog, "open", 0, Some(0));
        let stale_receipt = dialog
            .fixture_action_receipts
            .last()
            .expect("the stale action has an observable refusal receipt");
        assert_eq!(stale_receipt["action"], "open");
        assert_eq!(stale_receipt["result"], "refused");
        assert_eq!(
            *actions.opened.lock().unwrap(),
            vec![expected_open.clone()],
            "a renamed scan alias never reaches the file-action sink",
        );
        assert_eq!(
            actions.revalidated.lock().unwrap().len(),
            4,
            "failed revalidation does not produce an accepted proof",
        );
        std::fs::rename(&stale_path, &expected_open)
            .expect("restore the generated source before fixture cleanup");
        assert_eq!(
            std::fs::read(&expected_open).expect("restored source remains readable"),
            b"identical generated content"
        );
        assert_eq!(
            dialog.session.view().snapshot.generation,
            completed_generation
        );

        click_control(&context, &mut dialog, "close");
        assert!(!dialog.open);
        assert!(dialog.session.view().closed);
        let mut joined = false;
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if dialog.finish_fixture_close() {
                joined = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            joined,
            "the closed session joins its retired worker without blocking the UI owner"
        );
    }
}
