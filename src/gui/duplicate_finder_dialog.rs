use crate::duplicate_finder::{
    DiscoveryDiagnostic, DiscoverySnapshot, DiscoveryStage, DiscoveryStatus,
    DuplicateFinderSession, DuplicateFinderSessionError, RootScanStatus, VerifiedKnownFile,
};
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
    rect: egui::Rect,
    response_rect: egui::Rect,
    enabled: bool,
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

pub struct DuplicateFinderDialogState {
    pub open: bool,
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
        self.reopen_pending = false;
        self.session.close();
        self.rows_snapshot = None;
        self.rows.clear();
        self.rows_truncated = false;
        self.status = None;
        self.bump_target_revision();
        #[cfg(any(test, feature = "dev-smoke-binaries"))]
        if self.release_fixture_gate_on_close {
            if let Some(checkpoint) = self.first_read_checkpoint.as_ref() {
                checkpoint.release();
            }
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
                        "response_current": true,
                        "interaction_eligible": self.responses_interaction_eligible
                            && response.enabled
                            && response.rect.is_positive()
                            && layer_current,
                        "painted_frame": frame_id,
                        "rect": {
                            "min_x": response.rect.min.x,
                            "min_y": response.rect.min.y,
                            "max_x": response.rect.max.x,
                            "max_y": response.rect.max.y,
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
                    "response_current": true,
                    "interaction_eligible": self.responses_interaction_eligible
                        && action.response.enabled
                        && action.response.rect.is_positive()
                        && layer_current,
                    "painted_frame": frame_id,
                    "layer_current": layer_current,
                    "rect": {
                        "min_x": action.response.rect.min.x,
                        "min_y": action.response.rect.min.y,
                        "max_x": action.response.rect.max.x,
                        "max_y": action.response.rect.max.y,
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

    pub(crate) fn show_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        instance: Option<u64>,
        owner: crate::gui::jump_mode::ui::NavigationOwner,
        application_input_allowed: bool,
        native_owner_current: bool,
        first_ui_pass: bool,
    ) {
        #[cfg(any(test, feature = "dev-smoke-binaries"))]
        {
            self.responses.clear();
            self.result_action_responses.clear();
            self.unavailable_result_rows.clear();
            self.responses_interaction_eligible = false;
        }
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
        if !self.open {
            return;
        }
        let current_instance = instance.filter(|instance| {
            matches!(
                owner,
                crate::gui::jump_mode::ui::NavigationOwner::Dialog {
                    kind: crate::gui::jump_mode::ui::NavigationDialogKind::DuplicateFinder,
                    instance: owner_instance,
                } if owner_instance == *instance
            )
        });
        let jump_scope = scope.zip(current_instance).map(|(scope, instance)| {
            scope.child_namespace("duplicate-finder").with_surface(
                crate::gui::jump_mode::SurfaceId::dialog("duplicate-finder", instance),
                crate::gui::jump_mode::TargetGeneration(self.target_revision),
            )
        });
        let parent_layer =
            crate::gui::jump_mode::ui::NavigationDialogKind::DuplicateFinder.layer_id();
        let parent_is_current = parent_layer.is_some_and(|layer| {
            crate::gui::jump_mode::ui::visible_top_middle_layer(ctx) == Some(layer)
        });
        let owner_current = self.open
            && native_owner_current
            && application_input_allowed
            && parent_is_current
            && !egui::Popup::is_any_open(ctx);
        let session_view = self.session.view();
        #[cfg(test)]
        let snapshot = self
            .test_snapshot_override
            .as_ref()
            .map_or(session_view.snapshot, Arc::clone);
        #[cfg(not(test))]
        let snapshot = session_view.snapshot;
        self.ensure_rows(&snapshot);

        let mut window_open = self.open;
        let mut order = 0_u64;
        let mut visual_close = false;
        egui::Window::new(WINDOW_TITLE)
            .id(egui::Id::new(Some(WINDOW_TITLE)))
            .collapsible(false)
            .resizable(true)
            .default_size((1040.0, 700.0))
            .min_size((620.0, 420.0))
            .max_size(ctx.content_rect().size())
            .open(&mut window_open)
            .show(ctx, |ui| {
                let visible_owner = owner_current && ui.is_visible() && !ui.is_sizing_pass();
                let commit_enabled = visible_owner && first_ui_pass;
                #[cfg(any(test, feature = "dev-smoke-binaries"))]
                {
                    self.responses_interaction_eligible = commit_enabled;
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
                            self.show_roots(
                                ui,
                                visible_owner,
                                commit_enabled,
                                jump_scope.as_ref(),
                                &mut order,
                            );
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
                                jump_scope.as_ref(),
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
                        self.show_scan_controls(
                            ui,
                            visible_owner,
                            commit_enabled,
                            jump_scope.as_ref(),
                            &mut order,
                        );
                        let close = ui.button("Close");
                        self.record_response("close", &close);
                        register_target(
                            jump_scope.as_ref(),
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
            });
        if first_ui_pass
            && native_owner_current
            && application_input_allowed
            && (!window_open || visual_close)
        {
            self.close();
        }
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
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.add_selected_root(path)
                        .unwrap_or_else(|error| self.status = Some(error));
                }
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
                let root = PathBuf::from(self.root_editor.trim());
                self.add_selected_root(root)
                    .unwrap_or_else(|error| self.status = Some(error));
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
                self.selected_roots.remove(index);
                self.bump_target_revision();
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
            match self.session.start_scan(self.selected_roots.clone()) {
                Ok(_) => {
                    self.status = None;
                    self.bump_target_revision();
                }
                Err(error) => self.status = Some(session_error_message(&error)),
            }
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
            match self.session.cancel() {
                Ok(true) => {
                    self.status =
                        Some("Cancelled. No results were published for this scan.".into());
                    self.bump_target_revision();
                }
                Ok(false) => self.status = Some("There is no active scan to cancel.".into()),
                Err(error) => self.status = Some(session_error_message(&error)),
            }
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
                checkpoint.release();
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
            .min_scrolled_height(0.0)
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
                        self.copy_verified_group(snapshot, index);
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
            self.run_known_action(
                snapshot,
                size_group_index,
                verified,
                KnownFileAction::Open,
                namespace,
                group_index,
                member_index,
            );
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
            self.run_known_action(
                snapshot,
                size_group_index,
                verified,
                KnownFileAction::Reveal,
                namespace,
                group_index,
                member_index,
            );
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
        self.record_geometry(key, id, rect.intersect(clip_rect), rect, enabled);
    }

    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    fn record_geometry(
        &mut self,
        key: &'static str,
        id: egui::Id,
        rect: egui::Rect,
        response_rect: egui::Rect,
        enabled: bool,
    ) {
        #[cfg(any(test, feature = "dev-smoke-binaries"))]
        self.responses.insert(
            key,
            TestWidgetResponse {
                id,
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
            panic!("timed out waiting for {message}");
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
