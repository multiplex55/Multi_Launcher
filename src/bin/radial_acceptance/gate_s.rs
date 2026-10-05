//! Bounded, payload-free Gate S evidence and strict owner transition contracts.
use super::*;
use multi_launcher::radial::gallery::{
    AppearanceObservation, MAX_GALLERY_BYTES, MAX_GALLERY_ENTRIES, NativeAppearanceTarget,
};

pub(crate) const GATE_S_CASE_IDS: [&str; 11] = [
    "S01", "S02", "S03", "S04", "S05", "S06", "S07", "S08", "S09", "CLEANUP", "R0",
];
pub(crate) const GATE_S_REQUIRED_CASE_IDS: [&str; 9] = [
    "S01", "S02", "S03", "S04", "S05", "S06", "S07", "S08", "S09",
];
pub(crate) const MAX_GATE_S_STEPS: usize = 16;
pub(crate) const MAX_GATE_S_PACKET_BYTES: usize = 64 * 1024;
pub(crate) const MAX_GATE_S_EVIDENCE_BYTES: usize = 384 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GateSOperation {
    Fresh,
    DirtyRename,
    Preview,
    Cancel,
    Apply,
    Opacity,
    Scale,
    Spacing,
    Labels,
    Undo,
    Redo,
    Dense,
    NativeStart,
    NativeClick,
    NativeNavigate,
    NativeStop,
    Save,
    Idle,
    LeaveGallery,
    ReturnGallery,
}

#[cfg(test)]
mod tests {
    use super::super::tests::{
        acceptance_report, gate_d_test_no_action_proof, gate_d_test_observation,
    };
    use super::*;
    use std::sync::Arc;
    fn state(owner: &GateDObservationEvidence) -> AppearanceObservation {
        AppearanceObservation {
            trace_sequence: owner.trace_sequence + 1,
            session_id: owner.session_id,
            generation: owner.generation,
            menu_digest: 76,
            catalog_count: 5,
            current_builtin: 0,
            preview_builtin: -1,
            effective_digest: 50,
            bindings_digest: 51,
            other_menus_digest: 52,
            raw_overrides_digest: 53,
            opacity_milli: 1000,
            scale_milli: 1000,
            spacing_milli: 1060,
            label_size_milli: 14000,
            labels_visible: true,
            gallery_active: true,
            requested: 5,
            completed: 5,
            cached_tiles: 5,
            cpu_bytes: 500_000,
            gpu_bytes: 500_000,
            prepared_generation: owner.generation,
            geometry_digest: 60,
            scene_digest: 61,
            prepared_menu_digest: 76,
            prepared_owner_session: owner.session_id,
            tile_key: 70,
            tile_scene_digest: 71,
            tile_geometry_digest: 72,
            ..Default::default()
        }
    }
    fn packet(id: &str) -> GateSCaseEvidence {
        let mut packet = GateSCaseEvidence {
            schema_version: 2,
            case_id: id.into(),
            fixture_digest: String::new(),
            steps: Vec::new(),
            no_action_proof: Some(gate_d_test_no_action_proof()),
        };
        let operations: Vec<_> = match id {
            "S01" => vec![GateSOperation::Fresh],
            "S02" => vec![
                GateSOperation::DirtyRename,
                GateSOperation::Preview,
                GateSOperation::Cancel,
            ],
            "S03" => vec![
                GateSOperation::Opacity,
                GateSOperation::Scale,
                GateSOperation::Spacing,
            ],
            "S04" => vec![
                GateSOperation::Preview,
                GateSOperation::Apply,
                GateSOperation::Undo,
                GateSOperation::Redo,
            ],
            "S05" => vec![
                GateSOperation::Preview,
                GateSOperation::Preview,
                GateSOperation::Preview,
                GateSOperation::Preview,
                GateSOperation::Cancel,
            ],
            "S06" => vec![GateSOperation::Dense],
            "S07" => vec![
                GateSOperation::NativeStart,
                GateSOperation::NativeClick,
                GateSOperation::NativeNavigate,
                GateSOperation::NativeNavigate,
                GateSOperation::NativeNavigate,
                GateSOperation::NativeNavigate,
                GateSOperation::NativeStop,
            ],
            "S08" => vec![GateSOperation::Save],
            "S09" => vec![GateSOperation::Idle, GateSOperation::LeaveGallery],
            _ => unreachable!(),
        };
        let mut owner = gate_d_test_observation(1, 10, &[], 0, 0, 0, 0, false);
        owner.trace_sequence = 100;
        let mut appearance = state(&owner);
        let mut dispatches = 0;
        let mut native_generation = owner.generation;
        let mut native_menu = 76;
        for (index, operation) in operations.into_iter().enumerate() {
            let before = owner.clone();
            let a = appearance;
            owner.trace_sequence += 10;
            let mut preset = None;
            let mut target = "";
            let mut native_target = None;
            let mut native_result = None;
            let mut native_pointer = None;
            let dispatch_before = dispatches;
            match operation {
                GateSOperation::DirtyRename => {
                    owner.generation += 2;
                    owner.document_digest = 15;
                    owner.undo_depth += 1;
                    owner.draft_dirty = true;
                    target = "MenuName";
                }
                GateSOperation::Preview => {
                    let p = if id == "S05" { index + 1 } else { 1 };
                    preset = Some(p as u8);
                    appearance.preview_active = true;
                    appearance.preview_builtin = p as i32;
                    appearance.preview_key = 80 + p as u64;
                    appearance.preview_candidate_digest =
                        if id == "S05" { 100 + p as u64 } else { 20 };
                    appearance.preview_token_digest = 200 + p as u64;
                    appearance.prepared_token_digest = appearance.preview_token_digest;
                    appearance.prepared_candidate_digest = appearance.preview_candidate_digest;
                    appearance.prepared_content_key = appearance.preview_key;
                    appearance.tile_key = appearance.preview_key;
                    appearance.tile_scene_digest = 90 + p as u64;
                    appearance.tile_selected_emphasis = p == 3;
                    target = "AppearanceTile";
                }
                GateSOperation::Cancel => {
                    appearance.preview_active = false;
                    appearance.preview_builtin = -1;
                    appearance.tile_scene_digest = 71;
                    target = "AppearanceCancel";
                }
                GateSOperation::Apply => {
                    owner.generation += 1;
                    owner.undo_depth += 1;
                    owner.document_digest = 20;
                    owner.draft_dirty = true;
                    appearance.preview_active = false;
                    appearance.current_builtin = 1;
                    target = "AppearanceApply";
                }
                GateSOperation::Undo => {
                    owner.generation += 1;
                    owner.undo_depth -= 1;
                    owner.redo_depth += 1;
                    owner.document_digest = 10;
                    owner.draft_dirty = false;
                    target = "Undo";
                }
                GateSOperation::Redo => {
                    owner.generation += 1;
                    owner.undo_depth += 1;
                    owner.redo_depth -= 1;
                    owner.document_digest = 20;
                    owner.draft_dirty = true;
                    target = "Redo";
                }
                GateSOperation::Opacity | GateSOperation::Scale | GateSOperation::Spacing => {
                    owner.generation += 1;
                    owner.undo_depth += 1;
                    owner.document_digest += 1;
                    owner.draft_dirty = true;
                    appearance.effective_digest += 1;
                    appearance.geometry_digest += 1;
                    match operation {
                        GateSOperation::Opacity => {
                            appearance.opacity_milli = 500;
                            target = "SimpleOpacity";
                        }
                        GateSOperation::Scale => {
                            appearance.scale_milli = 1250;
                            target = "SimpleScale";
                        }
                        _ => {
                            appearance.spacing_milli = 1400;
                            target = "SimpleSpacing";
                        }
                    }
                }
                GateSOperation::Dense => {
                    appearance.authored_cell_count = 50;
                    appearance.density_warnings = 2;
                }
                GateSOperation::Save => {
                    owner.draft_dirty = false;
                    owner.session_id += 1;
                    owner.generation = 1;
                    owner.document_digest += 1;
                    owner.undo_depth = 0;
                    owner.redo_depth = 0;
                    target = "Save";
                }
                GateSOperation::LeaveGallery => appearance.gallery_active = false,
                GateSOperation::NativeStart => target = "OpenDesktopPreview",
                GateSOperation::NativeStop => target = "StopDesktopPreview",
                GateSOperation::NativeClick | GateSOperation::NativeNavigate => {
                    target = "native_cell";
                    let kind = if operation == GateSOperation::NativeClick {
                        0
                    } else {
                        [1, 3, 4, 2][index - 2]
                    };
                    native_target = Some(NativeAppearanceTarget {
                        session_id: owner.session_id,
                        generation: native_generation,
                        menu_digest: native_menu,
                        cell_digest: 88,
                        geometry_digest: native_generation,
                        point_x: 20,
                        point_y: 20,
                        dpi_milli: 1000,
                        work_area: [0, 0, 2560, 1440],
                        kind,
                        clicked: true,
                    });
                    native_pointer = Some(QueryPointerClickEvidence {
                        down_inserted: 1,
                        up_inserted: 1,
                        layout_generation: native_generation,
                        down_event_ordinal: 2,
                        up_event_ordinal: 3,
                        release_settle_ms: 1,
                    });
                    if kind == 0 {
                        dispatches += 1;
                    } else {
                        native_generation += 1;
                        if kind == 1 {
                            native_menu = 77;
                        }
                        if kind == 2 {
                            native_menu = 76;
                        }
                        native_result = Some(NativeAppearanceTarget {
                            session_id: owner.session_id,
                            generation: native_generation,
                            menu_digest: native_menu,
                            geometry_digest: native_generation,
                            ..native_target.unwrap()
                        });
                    }
                }
                _ => {}
            }
            appearance.generation = owner.generation;
            appearance.session_id = owner.session_id;
            appearance.prepared_owner_session = owner.session_id;
            appearance.trace_sequence = owner.trace_sequence + 1;
            appearance.prepared_generation = owner.generation;
            let input = (!target.is_empty()).then(|| GateSInput {
                target: target.into(),
                index: preset.map(usize::from),
                bounds: [10, 10, 30, 30],
                client_size: [800, 600],
                session_id: before.session_id,
                generation: before.generation,
                trace_sequence: before.trace_sequence + 5,
                clicked: true,
                inserted_move: usize::from(target != "native_cell"),
                inserted_down: 1,
                inserted_up: 1,
                accepted_pointer_sequence: before.trace_sequence + 4,
            });
            packet.steps.push(GateSStep {
                operation,
                preset,
                before: before.clone(),
                after: owner.clone(),
                appearance_before: a,
                appearance_after: appearance,
                input,
                native_target,
                native_pointer,
                native_result,
                preview_dispatch_before: dispatch_before,
                preview_dispatch_after: dispatches,
                expected_before_document: before.document_digest,
                expected_after_document: owner.document_digest,
                expected_candidate_document: appearance.preview_candidate_digest,
                expected_undo_before: before.undo_depth,
                expected_undo_after: owner.undo_depth,
                expected_redo_before: before.redo_depth,
                expected_redo_after: owner.redo_depth,
                native_expected: native_target.map(|target| GateSNativeExpectation {
                    menu: target.menu_digest,
                    cell: target.cell_digest,
                    result_menu: native_result
                        .map_or(target.menu_digest, |result| result.menu_digest),
                    generation: target.generation,
                }),
                expected_menu_after: appearance.menu_digest,
                expected_dirty_before: before.draft_dirty,
                expected_dirty_after: owner.draft_dirty,
            });
        }
        packet
    }
    #[test]
    fn all_nine_packets_prove_distinct_owner_contracts_and_round_trip_without_payloads() {
        let mut report = acceptance_report("native_windows");
        report.suite = AcceptanceSuite::GateS;
        for id in GATE_S_REQUIRED_CASE_IDS {
            let mut packet = packet(id);
            assert!(gate_s_case_contract_is_valid(&packet), "{id}");
            packet.fixture_digest = gate_s_fixture_digest(&report.profile);
            let bytes = serde_json::to_vec(&packet).unwrap();
            assert!(bytes.len() < MAX_GATE_S_PACKET_BYTES);
            assert_eq!(
                serde_json::from_slice::<GateSCaseEvidence>(&bytes).unwrap(),
                packet
            );
            report.gate_s_evidence.push(packet);
            report.push_case(AcceptanceCaseResult {
                id: id.into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "typed appearance owner contract".into(),
                observed: "bounded proof".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        validate_gate_s_evidence_report(&report).unwrap();
        let mut wrong = report.clone();
        wrong.gate_s_evidence.swap(0, 1);
        assert!(validate_gate_s_evidence_report(&wrong).is_err());
        let mut wrong = report.clone();
        wrong.gate_s_evidence[0].fixture_digest = "wrong".into();
        assert!(validate_gate_s_evidence_report(&wrong).is_err());
        let mut wrong = report.clone();
        wrong.suite = AcceptanceSuite::GateD;
        assert!(validate_gate_s_evidence_report(&wrong).is_err());
        let mut wrong = report.clone();
        wrong.gate_s_evidence[0].steps =
            vec![wrong.gate_s_evidence[0].steps[0].clone(); MAX_GATE_S_STEPS + 1];
        assert!(validate_gate_s_evidence_report(&wrong).is_err());
        let mut json = serde_json::to_value(&report.gate_s_evidence[0]).unwrap();
        json["command"] = serde_json::json!("private payload");
        assert!(serde_json::from_value::<GateSCaseEvidence>(json).is_err());
    }
    #[test]
    fn contract_rejects_forged_control_history_geometry_dispatch_emphasis_and_idle_proofs() {
        let mut bad = packet("S02");
        bad.steps[1].input.as_mut().unwrap().generation += 1;
        assert!(!gate_s_case_contract_is_valid(&bad));
        let mut bad = packet("S04");
        bad.steps[1].input.as_mut().unwrap().target = "Undo".into();
        assert!(!gate_s_case_contract_is_valid(&bad));
        let mut bad = packet("S04");
        bad.steps[1].after.undo_depth += 1;
        assert!(!gate_s_case_contract_is_valid(&bad));
        let mut bad = packet("S03");
        bad.steps[1].appearance_after.scale_milli = bad.steps[1].appearance_before.scale_milli;
        assert!(!gate_s_case_contract_is_valid(&bad));
        let mut bad = packet("S05");
        bad.steps[2].appearance_after.tile_selected_emphasis = false;
        assert!(!gate_s_case_contract_is_valid(&bad));
        let mut bad = packet("S07");
        bad.steps[1]
            .native_pointer
            .as_mut()
            .unwrap()
            .layout_generation += 1;
        assert!(!gate_s_case_contract_is_valid(&bad));
        let mut bad = packet("S07");
        bad.steps[1].preview_dispatch_after += 1;
        assert!(!gate_s_case_contract_is_valid(&bad));
        let mut bad = packet("S07");
        bad.steps[2].native_result.as_mut().unwrap().menu_digest =
            bad.steps[2].native_target.unwrap().menu_digest;
        assert!(!gate_s_case_contract_is_valid(&bad));
        let mut bad = packet("S09");
        bad.steps[0].appearance_after.requested += 1;
        assert!(!gate_s_case_contract_is_valid(&bad));
        let mut bad = packet("S09");
        bad.steps[0].appearance_after.visible_pending = 1;
        assert!(!gate_s_case_contract_is_valid(&bad));
        let mut bad = packet("S01");
        bad.no_action_proof.as_mut().unwrap().marker_lines_after = 1;
        assert!(!gate_s_case_contract_is_valid(&bad));
    }
    #[test]
    fn proof_rejects_stale_candidate_menu_frame_assets_selection_navigation_and_redo() {
        for field in ["candidate", "menu", "token", "key", "session", "sequence"] {
            let mut bad = packet("S05");
            let step = &mut bad.steps[1];
            match field {
                "candidate" => step.appearance_after.prepared_candidate_digest += 1,
                "menu" => step.appearance_after.prepared_menu_digest += 1,
                "token" => step.appearance_after.prepared_token_digest += 1,
                "key" => step.appearance_after.prepared_content_key += 1,
                "session" => step.appearance_after.prepared_owner_session += 1,
                _ => {
                    step.appearance_after.trace_sequence =
                        step.input.as_ref().unwrap().trace_sequence
                }
            }
            assert!(!gate_s_case_contract_is_valid(&bad), "{field}");
        }
        for field in ["assets", "selection", "navigation", "redo", "expected"] {
            let mut bad = packet("S04");
            let step = &mut bad.steps[1];
            match field {
                "assets" => step.after.pending_assets_digest += 1,
                "selection" => step.after.selection_digest += 1,
                "navigation" => step.after.navigation_path_digest += 1,
                "redo" => {
                    step.after.redo_depth += 1;
                    step.expected_redo_after += 1;
                }
                _ => step.expected_after_document += 1,
            }
            assert!(!gate_s_case_contract_is_valid(&bad), "{field}");
        }
        let mut bad = packet("S07");
        bad.steps[2].native_target.as_mut().unwrap().cell_digest += 1;
        assert!(!gate_s_case_contract_is_valid(&bad));
        let mut bad = packet("S07");
        bad.steps[2].native_expected.as_mut().unwrap().generation += 1;
        assert!(!gate_s_case_contract_is_valid(&bad));
    }
    fn gate_s_test_documents() -> (tempfile::TempDir, RadialDocument, RadialDocument) {
        let directory = tempfile::tempdir().unwrap();
        let gate_d = deterministic_gate_d_fixture(
            &directory.path().join("acceptance.log"),
            MouseGestureMode::Enabled,
            AcceptanceHotkey::ShiftAltWinEnd,
            &directory.path().join("markers.txt"),
            &std::env::current_exe().unwrap(),
        )
        .unwrap();
        let original = serde_json::from_slice(&gate_d.radial_json).unwrap();
        let fixture = gate_s_fixture(gate_d).unwrap();
        let document: RadialDocument = serde_json::from_slice(&fixture.radial_json).unwrap();
        validate_radial_document(&document).unwrap();
        (directory, original, document)
    }

    fn gate_s_cumulative_session(
        document: &RadialDocument,
    ) -> (
        multi_launcher::radial::authoring::RadialAuthoringSession,
        GateSDocumentOracle,
    ) {
        use multi_launcher::radial::{appearance::*, authoring::*};
        let mut session = RadialAuthoringSession::new(AuthoringSnapshot::new(
            Arc::new(document.clone()),
            "gate-s-cumulative-fixture",
        ));
        let root = document.default_menu_id.clone();
        session.select(Some(StableSelection::Menu(root.clone())));
        let selection = session.selection.clone();
        let assets = session.pending_assets.clone();
        let mut oracle = GateSDocumentOracle::new(document.clone());
        menu::rename_menu(
            &mut session,
            root.clone(),
            "Gate S unrelated dirty name".into(),
            EditPhase::Atomic,
        )
        .unwrap();
        let preview =
            preset_candidate(&session.draft, &root, &BuiltinSkin::Compact.definition()).unwrap();
        validate_radial_document(&preview).unwrap();
        oracle.correlate(&mut packet("S02")).unwrap();
        assert_eq!(session.draft.as_ref(), &oracle.document);
        assert_eq!(session.acceptance_history_depths(), (1, 0));
        for (operation, field) in [
            (GateSOperation::Opacity, "opacity"),
            (GateSOperation::Scale, "scale"),
            (GateSOperation::Spacing, "spacing"),
        ] {
            let effective = simple_state(&session.draft, &root).unwrap().values;
            let edit = match operation {
                GateSOperation::Opacity => SimpleEdit::Opacity(
                    (value(&effective.images.icon_opacity).unwrap() - 0.05).clamp(0.0, 1.0),
                ),
                GateSOperation::Scale => SimpleEdit::Scale(
                    (value(&effective.geometry.menu_scale).unwrap() + 0.05).clamp(0.55, 2.0),
                ),
                GateSOperation::Spacing => SimpleEdit::Spacing(
                    (value(&effective.geometry.radius_scale).unwrap() + 0.05).clamp(0.55, 2.0),
                ),
                _ => unreachable!("only the three authored Simple steps are listed"),
            };
            let generation = session.generation;
            let candidate = simple_candidate(&session.draft, &root, &edit).unwrap();
            assert_eq!(session.generation, generation);
            session
                .replace_document_edit(
                    candidate,
                    EditKey {
                        entity: format!("appearance:{root}"),
                        field: field.into(),
                    },
                    EditPhase::Atomic,
                )
                .unwrap();
            assert_eq!(session.generation.0, generation.0 + 1);
        }
        oracle.correlate(&mut packet("S03")).unwrap();
        assert_eq!(session.draft.as_ref(), &oracle.document);
        assert_eq!(session.acceptance_history_depths(), (4, 0));
        let before_apply = Arc::clone(&session.draft);
        let generation = session.generation;
        let compact =
            preset_candidate(&session.draft, &root, &BuiltinSkin::Compact.definition()).unwrap();
        validate_radial_document(&compact).unwrap();
        assert_eq!(compact, oracle.preset(1).unwrap());
        assert_eq!(session.generation, generation);
        session.replace_document_atomic(compact.clone()).unwrap();
        assert_eq!(session.draft.as_ref(), &compact);
        assert_eq!(session.acceptance_history_depths(), (5, 0));
        assert!(session.undo());
        assert_eq!(session.draft, before_apply);
        assert_eq!(session.acceptance_history_depths(), (4, 1));
        assert!(session.redo());
        assert_eq!(session.draft.as_ref(), &compact);
        oracle.correlate(&mut packet("S04")).unwrap();
        assert_eq!(session.draft.as_ref(), &oracle.document);
        assert_eq!(session.generation.0, 8);
        assert_eq!(session.acceptance_history_depths(), (5, 0));
        assert_eq!(session.selection, selection);
        assert_eq!(session.pending_assets, assets);
        assert!(session.is_dirty());
        (session, oracle)
    }

    fn prepare_gate_s_test_menu(
        preparer: &mut multi_launcher::radial::preparation::PreviewFramePreparer,
        document: &RadialDocument,
        menu_id: &multi_launcher::radial::model::MenuId,
        page: usize,
        generation: u64,
    ) -> multi_launcher::radial::preparation::PreparedFrameInput {
        use multi_launcher::radial::{geometry::*, preparation::*};
        let menu = document
            .menus
            .iter()
            .find(|menu| &menu.id == menu_id)
            .unwrap();
        preparer
            .prepare(
                document,
                menu_id,
                PhysicalPoint {
                    x: 1280.0,
                    y: 720.0,
                },
                PhysicalRect {
                    min: PhysicalPoint { x: 0.0, y: 0.0 },
                    max: PhysicalPoint {
                        x: 2560.0,
                        y: 1440.0,
                    },
                },
                ScaleFactor::new(1.0).unwrap(),
                generation,
                None,
                &PreviewProjection {
                    dynamic: synthetic_preview_dynamic(menu),
                    page,
                    ..Default::default()
                },
            )
            .unwrap()
    }

    #[test]
    fn gate_s_fixture_cumulative_overrides_validate_and_prepare_every_s05_preset() {
        use multi_launcher::radial::{appearance::*, model::*, preparation::*};
        let (_directory, gate_d, document) = gate_s_test_documents();
        let (session, oracle) = gate_s_cumulative_session(&document);
        let root_id = document.default_menu_id.clone();
        let root = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == root_id)
            .unwrap();
        let original = gate_d.menus.iter().find(|menu| menu.id == root_id).unwrap();
        assert_eq!(original.rings[0].radius, 92.0);
        assert_eq!(root.rings[0].radius, 96.0);
        assert_eq!(root.rings[0].cells.len(), 9);
        assert_eq!(root.rings[1], original.rings[1]);
        assert_eq!(root.rings[1].radius, 172.0);
        assert_eq!(root.rings[1].cells.len(), 4);
        assert_eq!(root.rings[0].cells[2..], original.rings[0].cells[2..]);
        assert_eq!(root.style.values.geometry.menu_scale, Override::Value(1.05));
        assert_eq!(
            root.style.values.geometry.radius_scale,
            Override::Value(1.17)
        );
        let retained = (
            Arc::clone(&session.draft),
            session.generation,
            session.selection.clone(),
            session.acceptance_history_depths(),
            session.pending_assets.clone(),
        );
        let mut preparer = PreviewFramePreparer::new(Default::default());
        for (index, builtin) in [
            BuiltinSkin::Compact,
            BuiltinSkin::Comfortable,
            BuiltinSkin::HighContrast,
            BuiltinSkin::Classic,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(BUILTIN_SKINS[index + 1], builtin);
            let candidate =
                preset_candidate(&session.draft, &root_id, &builtin.definition()).unwrap();
            validate_radial_document(&candidate).unwrap();
            assert_eq!(candidate, oracle.preset((index + 1) as u8).unwrap());
            let menu = candidate
                .menus
                .iter()
                .find(|menu| menu.id == root_id)
                .unwrap();
            assert_eq!(menu.rings, root.rings);
            assert_eq!(menu.style, root.style);
            let frame = prepare_gate_s_test_menu(
                &mut preparer,
                &candidate,
                &root_id,
                0,
                session.generation.0,
            );
            assert_eq!(frame.page_count, 1);
            assert_eq!(frame.layout.style.menu_scale, 1.05);
            assert_eq!(frame.layout.style.radius_scale, 1.17);
            for cell in root.rings.iter().flat_map(|ring| &ring.cells) {
                assert!(
                    frame
                        .layout
                        .cells
                        .iter()
                        .any(|prepared| prepared.cell_id == cell.id)
                );
            }
            if matches!(
                builtin,
                BuiltinSkin::Comfortable | BuiltinSkin::HighContrast
            ) {
                let mut old_geometry = candidate.clone();
                old_geometry
                    .menus
                    .iter_mut()
                    .find(|menu| menu.id == root_id)
                    .unwrap()
                    .rings[0]
                    .radius = 92.0;
                let errors = validate_radial_document(&old_geometry).unwrap_err();
                assert!(
                    errors
                        .0
                        .iter()
                        .any(|issue| issue.path == "menus[0].rings[0].style"
                            && issue.message
                                == "effective style causes adjacent circular cells to overlap")
                );
            }
        }
        assert_eq!(
            retained,
            (
                Arc::clone(&session.draft),
                session.generation,
                session.selection.clone(),
                session.acceptance_history_depths(),
                session.pending_assets.clone(),
            )
        );
    }

    #[test]
    fn independently_selected_fixture_link_prepares_real_page_and_back_controls() {
        use multi_launcher::radial::{geometry::*, model::*, preparation::*};
        let (_directory, _gate_d, initial) = gate_s_test_documents();
        let (session, _) = gate_s_cumulative_session(&initial);
        let mut preparer = PreviewFramePreparer::new(Default::default());
        for (document, generation) in [
            (&initial, 1),
            (session.draft.as_ref(), session.generation.0),
        ] {
            validate_radial_document(document).unwrap();
            let root = document
                .menus
                .iter()
                .find(|menu| menu.id == document.default_menu_id)
                .unwrap();
            assert_eq!(root.id, MenuId::new("starter"));
            assert_eq!(root.rings.len(), 2);
            assert_eq!(root.rings[0].cells.len(), 9);
            assert_eq!(root.rings[1].cells.len(), 4);
            assert_eq!(root.rings[0].radius, 96.0);
            assert_eq!(root.rings[1].radius, 172.0);
            assert_eq!(root.rings[0].cells[0].id, CellId::new("qa-open"));
            assert_eq!(root.rings[0].cells[1].id, CellId::new("qa-hidden-first"));
            assert!(
                matches!(&root.rings[0].cells[1].content, CellContent::Submenu {menu_id} if menu_id.as_str() == "gate-s-paged")
            );
            let CellContent::Action {
                binding: ActionBinding::Persisted { action },
            } = &root.rings[0].cells[0].content
            else {
                panic!("fixed native action identity");
            };
            let Some(PersistableActionTargetRef::CustomAction { action }) = &action.target else {
                panic!("fixed read-only query action");
            };
            assert!(matches!(
                multi_launcher::commands::parse_action(action).unwrap(),
                multi_launcher::commands::Command::VirtualDesktop(
                    multi_launcher::commands::VirtualDesktopCommand::Current
                )
            ));
            let oracle = GateSDocumentOracle::new(document.clone());
            let root_frame =
                prepare_gate_s_test_menu(&mut preparer, document, &root.id, 0, generation);
            assert_eq!(root_frame.page_count, 1);
            for (kind, id, result) in [
                (0, CellId::new("qa-open"), root.id.clone()),
                (
                    1,
                    CellId::new("qa-hidden-first"),
                    MenuId::new("gate-s-paged"),
                ),
            ] {
                let prepared = root_frame
                    .layout
                    .cells
                    .iter()
                    .find(|cell| cell.cell_id == id)
                    .unwrap();
                assert!(prepared.actionable && prepared.control.is_none());
                assert!(matches!(prepared.shape, HitShape::Circle { .. }));
                let expected = oracle.native_expectation(kind, generation).unwrap();
                assert_eq!(expected.generation, generation);
                assert_eq!(expected.menu, fixture_identity_digest(&root.id));
                assert_eq!(expected.cell, fixture_identity_digest(&id));
                assert_eq!(expected.result_menu, fixture_identity_digest(&result));
            }
            assert_ne!(
                oracle.native_expectation(1, generation).unwrap().cell,
                fixture_identity_digest(&CellId::new("gate-d-parent-link"))
            );
            let dense = document
                .menus
                .iter()
                .find(|menu| menu.id.as_str() == "gate-s-dense")
                .unwrap();
            assert_eq!(dense.layout, LayoutKind::Wedges);
            assert_eq!(dense.rings[0].radius, 96.0);
            assert_eq!(dense.rings[0].cells.len(), 50);
            let dense_frame =
                prepare_gate_s_test_menu(&mut preparer, document, &dense.id, 0, generation);
            assert_eq!(
                dense_frame
                    .layout
                    .cells
                    .iter()
                    .filter(|cell| cell.ring_id == dense.rings[0].id)
                    .count(),
                50
            );
            for (index, cell) in dense.rings[0].cells.iter().enumerate() {
                assert_eq!(cell.id, CellId::new(format!("gate-s-dense-{index}")));
                assert_eq!(cell.content, root.rings[0].cells[0].content);
                let prepared = dense_frame
                    .layout
                    .cells
                    .iter()
                    .find(|prepared| prepared.cell_id == cell.id)
                    .unwrap();
                assert!(prepared.actionable && prepared.control.is_none());
                assert!(matches!(prepared.shape, HitShape::Wedge { .. }));
            }
            assert!(
                !multi_launcher::radial::density::analyze(
                    dense,
                    &dense_frame.layout,
                    &dense_frame.resources,
                    dense_frame.work_area,
                )
                .is_empty()
            );
            let paged = document
                .menus
                .iter()
                .find(|menu| menu.id.as_str() == "gate-s-paged")
                .unwrap();
            assert_eq!(paged.rings[0].radius, 96.0);
            assert_eq!(
                paged.rings[0].cells[0].id,
                CellId::new("gate-s-paged-source")
            );
            assert!(matches!(
                paged.rings[0].cells[0].content,
                CellContent::Dynamic {
                    source: DynamicSource::Applications
                }
            ));
            for (page, kind, control, id, result) in [
                (
                    0,
                    3,
                    Control::NextPage,
                    CellId::new("__radial_page_next:gate-s-paged-ring"),
                    paged.id.clone(),
                ),
                (
                    1,
                    4,
                    Control::PreviousPage,
                    CellId::new("__radial_page_previous:gate-s-paged-ring"),
                    paged.id.clone(),
                ),
                (
                    0,
                    2,
                    Control::Back,
                    CellId::new("__center"),
                    root.id.clone(),
                ),
            ] {
                let frame =
                    prepare_gate_s_test_menu(&mut preparer, document, &paged.id, page, generation);
                assert_eq!(frame.page, page);
                assert!(frame.page_count > 1);
                let cell = frame
                    .layout
                    .cells
                    .iter()
                    .find(|cell| cell.cell_id == id)
                    .unwrap();
                assert!(cell.actionable);
                assert_eq!(cell.control, Some(control));
                let expected = oracle.native_expectation(kind, generation).unwrap();
                assert_eq!(expected.generation, generation);
                assert_eq!(expected.menu, fixture_identity_digest(&paged.id));
                assert_eq!(expected.cell, fixture_identity_digest(&id));
                assert_eq!(expected.result_menu, fixture_identity_digest(&result));
            }
            let candidate = oracle.preset(1).unwrap();
            assert_eq!(
                candidate
                    .skins
                    .iter()
                    .find(|skin| skin.id.as_str() == "builtin-compact")
                    .unwrap(),
                document
                    .skins
                    .iter()
                    .find(|skin| skin.id.as_str() == "builtin-compact")
                    .unwrap()
            );
            assert_eq!(
                candidate
                    .menus
                    .iter()
                    .find(|menu| menu.id == document.default_menu_id)
                    .unwrap()
                    .skin_id
                    .as_str(),
                "builtin-compact-2"
            );
        }
    }
    #[test]
    fn cli_explicitly_accepts_gate_s_exact_chord_and_rejects_profile_copy_and_unknown_suite() {
        let parse = |args: &[&str]| parse_arguments(args.iter().map(std::ffi::OsString::from));
        let ParseResult::Run(args) = parse(&[
            "--output",
            "m6-report",
            "--suite",
            "gate-s",
            "--hotkey",
            "shift-alt-win-end",
        ])
        .unwrap() else {
            panic!("expected runnable arguments")
        };
        assert_eq!(args.suite, AcceptanceSuite::GateS);
        assert_eq!(args.hotkey, AcceptanceHotkey::ShiftAltWinEnd);
        assert!(
            parse(&[
                "--output",
                "m6-report",
                "--suite",
                "gate-s",
                "--profile-copy",
                "profile"
            ])
            .is_err()
        );
        assert!(parse(&["--output", "m6-report", "--suite", "gate-s-v1"]).is_err());
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GateSInput {
    pub target: String,
    pub index: Option<usize>,
    pub bounds: [i32; 4],
    pub client_size: [i32; 2],
    pub session_id: u64,
    pub generation: u64,
    pub trace_sequence: u64,
    pub clicked: bool,
    pub inserted_move: usize,
    pub inserted_down: usize,
    pub inserted_up: usize,
    pub accepted_pointer_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GateSStep {
    pub operation: GateSOperation,
    pub preset: Option<u8>,
    pub before: GateDObservationEvidence,
    pub after: GateDObservationEvidence,
    pub appearance_before: AppearanceObservation,
    pub appearance_after: AppearanceObservation,
    pub input: Option<GateSInput>,
    pub native_target: Option<NativeAppearanceTarget>,
    pub native_pointer: Option<QueryPointerClickEvidence>,
    pub native_result: Option<NativeAppearanceTarget>,
    pub preview_dispatch_before: u64,
    pub preview_dispatch_after: u64,
    pub expected_before_document: u64,
    pub expected_after_document: u64,
    pub expected_candidate_document: u64,
    pub expected_undo_before: usize,
    pub expected_undo_after: usize,
    pub expected_redo_before: usize,
    pub expected_redo_after: usize,
    pub native_expected: Option<GateSNativeExpectation>,
    pub expected_menu_after: u64,
    pub expected_dirty_before: bool,
    pub expected_dirty_after: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GateSNativeExpectation {
    pub menu: u64,
    pub cell: u64,
    pub result_menu: u64,
    pub generation: u64,
}

fn fixture_identity_digest(value: &impl std::fmt::Debug) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::hash::DefaultHasher::new();
    format!("{value:?}").hash(&mut hash);
    hash.finish()
}

/// Independent runner state starts from the installed typed fixture. It never
/// adopts an observed document or a digest returned by the application.
pub(crate) struct GateSDocumentOracle {
    document: RadialDocument,
    clean_document: RadialDocument,
    undo: Vec<(RadialDocument, RadialDocument)>,
    redo: Vec<(RadialDocument, RadialDocument)>,
    preview: Option<u8>,
}
impl GateSDocumentOracle {
    pub(crate) fn new(document: RadialDocument) -> Self {
        Self {
            clean_document: document.clone(),
            document,
            undo: Vec::new(),
            redo: Vec::new(),
            preview: None,
        }
    }
    fn digest(document: &RadialDocument) -> Result<u64, String> {
        serde_json::to_vec(document)
            .map(|bytes| gate_c_private_digest(&bytes))
            .map_err(|error| error.to_string())
    }
    fn preset(&self, index: u8) -> Result<RadialDocument, String> {
        use multi_launcher::radial::{appearance::BUILTIN_SKINS, model::SkinId};
        let mut candidate = self.document.clone();
        let mut skin = BUILTIN_SKINS
            .get(usize::from(index))
            .ok_or("unknown expected preset")?
            .definition();
        let id = if let Some(existing) = candidate
            .skins
            .iter()
            .find(|existing| existing.name == skin.name && existing.style == skin.style)
        {
            existing.id.clone()
        } else {
            let base = skin.id.as_str().to_owned();
            if candidate
                .skins
                .iter()
                .any(|existing| existing.id == skin.id)
            {
                skin.id = (2..=candidate.skins.len() + 2)
                    .map(|suffix| SkinId::new(format!("{base}-{suffix}")))
                    .find(|id| candidate.skins.iter().all(|existing| &existing.id != id))
                    .ok_or("expected skin collision exhausted")?;
            }
            let id = skin.id.clone();
            candidate.skins.push(skin);
            id
        };
        candidate
            .menus
            .iter_mut()
            .find(|menu| menu.id == candidate.default_menu_id)
            .ok_or("expected root missing")?
            .skin_id = id;
        Ok(candidate)
    }
    pub(crate) fn native_expectation(
        &self,
        kind: u8,
        generation: u64,
    ) -> Result<GateSNativeExpectation, String> {
        use multi_launcher::radial::model::{CellId, MenuId};
        let root = self
            .document
            .menus
            .iter()
            .find(|menu| menu.id == self.document.default_menu_id)
            .ok_or("expected root missing")?;
        let paged = MenuId::new("gate-s-paged");
        let (menu, cell, result) = match kind {
            0 => (
                root.id.clone(),
                root.rings[0].cells[0].id.clone(),
                root.id.clone(),
            ),
            1 => (
                root.id.clone(),
                root.rings[0].cells[1].id.clone(),
                paged.clone(),
            ),
            3 => (
                paged.clone(),
                CellId::new("__radial_page_next:gate-s-paged-ring"),
                paged.clone(),
            ),
            4 => (
                paged.clone(),
                CellId::new("__radial_page_previous:gate-s-paged-ring"),
                paged.clone(),
            ),
            2 => (paged, CellId::new("__center"), root.id.clone()),
            _ => return Err("unknown expected native role".into()),
        };
        Ok(GateSNativeExpectation {
            menu: fixture_identity_digest(&menu),
            cell: fixture_identity_digest(&cell),
            result_menu: fixture_identity_digest(&result),
            generation,
        })
    }
    pub(crate) fn correlate(&mut self, packet: &mut GateSCaseEvidence) -> Result<(), String> {
        use multi_launcher::radial::{
            appearance::{simple_state, value},
            model::*,
        };
        for step in &mut packet.steps {
            step.expected_before_document = Self::digest(&self.document)?;
            step.expected_undo_before = self.undo.len();
            step.expected_redo_before = self.redo.len();
            step.expected_dirty_before = self.document != self.clean_document;
            let before = self.document.clone();
            let root = self.document.default_menu_id.clone();
            match step.operation {
                GateSOperation::Preview => {
                    let preset = step.preset.ok_or("expected preview preset missing")?;
                    step.expected_candidate_document = Self::digest(&self.preset(preset)?)?;
                    self.preview = Some(preset);
                }
                GateSOperation::Cancel => self.preview = None,
                GateSOperation::Apply => {
                    let preset = self
                        .preview
                        .take()
                        .ok_or("expected applied preview missing")?;
                    self.document = self.preset(preset)?;
                }
                GateSOperation::DirtyRename => {
                    self.document
                        .menus
                        .iter_mut()
                        .find(|menu| menu.id == root)
                        .ok_or("expected root missing")?
                        .name = "Gate S unrelated dirty name".into()
                }
                GateSOperation::Opacity | GateSOperation::Scale | GateSOperation::Spacing => {
                    let effective = simple_state(&self.document, &root)?.values;
                    let patch = &mut self
                        .document
                        .menus
                        .iter_mut()
                        .find(|menu| menu.id == root)
                        .ok_or("expected root missing")?
                        .style
                        .values;
                    match step.operation {
                        GateSOperation::Opacity => {
                            let opacity =
                                (value(&effective.images.icon_opacity)? - 0.05).clamp(0.0, 1.0);
                            let mut color = value(&effective.text.color)?;
                            color.alpha = (opacity * 255.0).round() as u8;
                            patch.text.color = Override::Value(color);
                            macro_rules! set {($($field:ident),+) => {$(patch.images.$field = Override::Value(opacity);)+};}
                            set!(
                                icon_opacity,
                                item_glow_opacity,
                                item_background_opacity,
                                item_foreground_opacity,
                                item_shadow_opacity,
                                menu_background_opacity,
                                menu_foreground_opacity,
                                menu_outer_rim_opacity,
                                center_background_opacity,
                                center_image_opacity,
                                submenu_indicator_opacity
                            );
                        }
                        GateSOperation::Scale => {
                            patch.geometry.menu_scale = Override::Value(
                                (value(&effective.geometry.menu_scale)? + 0.05).clamp(0.55, 2.0),
                            )
                        }
                        _ => {
                            patch.geometry.radius_scale = Override::Value(
                                (value(&effective.geometry.radius_scale)? + 0.05).clamp(0.55, 2.0),
                            )
                        }
                    }
                }
                GateSOperation::Undo => {
                    let entry = self.undo.pop().ok_or("expected undo history empty")?;
                    self.document = entry.0.clone();
                    self.redo.push(entry);
                }
                GateSOperation::Redo => {
                    let entry = self.redo.pop().ok_or("expected redo history empty")?;
                    self.document = entry.1.clone();
                    self.undo.push(entry);
                }
                GateSOperation::Save => {
                    self.document.revision.0 += 1;
                    self.clean_document = self.document.clone();
                    self.undo.clear();
                    self.redo.clear();
                    self.preview = None;
                }
                _ => {}
            }
            if matches!(
                step.operation,
                GateSOperation::Apply
                    | GateSOperation::DirtyRename
                    | GateSOperation::Opacity
                    | GateSOperation::Scale
                    | GateSOperation::Spacing
            ) {
                self.undo.push((before, self.document.clone()));
                self.redo.clear();
            }
            step.expected_after_document = Self::digest(&self.document)?;
            step.expected_undo_after = self.undo.len();
            step.expected_redo_after = self.redo.len();
            step.expected_dirty_after = self.document != self.clean_document;
            step.expected_menu_after =
                fixture_identity_digest(&if step.operation == GateSOperation::Dense {
                    MenuId::new("gate-s-dense")
                } else {
                    root
                });
        }
        Ok(())
    }
}

fn same_authoring_context(a: &GateDObservationEvidence, b: &GateDObservationEvidence) -> bool {
    gate_d_same_selection(a, b)
        && a.pending_assets_digest == b.pending_assets_digest
        && a.navigation_path_digest == b.navigation_path_digest
        && a.navigation_menu_id_digests == b.navigation_menu_id_digests
        && a.navigation_edge_digests == b.navigation_edge_digests
        && a.designer_filter_digest == b.designer_filter_digest
        && a.designer_search_hit_count == b.designer_search_hit_count
        && a.root_visible == b.root_visible
        && a.properties_popup_open == b.properties_popup_open
        && a.properties_dirty == b.properties_dirty
}

fn exact_prepared_candidate(state: &AppearanceObservation, candidate: u64) -> bool {
    candidate != 0
        && state.preview_candidate_digest == candidate
        && state.preview_token_digest != 0
        && state.prepared_token_digest == state.preview_token_digest
        && state.prepared_candidate_digest == candidate
        && state.prepared_content_key == state.preview_key
        && state.prepared_menu_digest == state.menu_digest
        && state.prepared_owner_session == state.session_id
        && state.prepared_generation == state.generation
        && state.geometry_digest != 0
        && state.scene_digest != 0
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GateSCaseEvidence {
    pub schema_version: u16,
    pub case_id: String,
    pub fixture_digest: String,
    pub steps: Vec<GateSStep>,
    pub no_action_proof: Option<GateDNoActionProof>,
}

pub(crate) fn gate_s_fixture_digest(profile: &ProfileIdentity) -> String {
    sha256_bytes(
        format!(
            "gate-s-fixture-v1\0{}\0{}\0{}",
            profile.settings_sha256, profile.radial_sha256, profile.actions_sha256
        )
        .as_bytes(),
    )
}

fn bounded(state: &AppearanceObservation) -> bool {
    state.session_id != 0
        && state.menu_digest != 0
        && state.catalog_count == 5
        && state.in_flight <= 1
        && state.visible_pending <= MAX_GALLERY_ENTRIES
        && state.cached_tiles <= MAX_GALLERY_ENTRIES
        && state.cpu_bytes <= MAX_GALLERY_BYTES
        && state.gpu_bytes <= MAX_GALLERY_BYTES
        && state.failure_count <= MAX_GALLERY_ENTRIES
        && (-1..=4).contains(&state.current_builtin)
        && (-1..=4).contains(&state.preview_builtin)
}

fn input_valid(step: &GateSStep) -> bool {
    let Some(input) = &step.input else {
        return false;
    };
    let [l, t, r, b] = input.bounds;
    let expected_target = match step.operation {
        GateSOperation::DirtyRename => "MenuName",
        GateSOperation::Preview => "AppearanceTile",
        GateSOperation::Cancel => "AppearanceCancel",
        GateSOperation::Apply => "AppearanceApply",
        GateSOperation::Opacity => "SimpleOpacity",
        GateSOperation::Scale => "SimpleScale",
        GateSOperation::Spacing => "SimpleSpacing",
        GateSOperation::Labels => "SimpleLabels",
        GateSOperation::Undo => "Undo",
        GateSOperation::Redo => "Redo",
        GateSOperation::Dense => "MenuRow",
        GateSOperation::NativeStart => "OpenDesktopPreview",
        GateSOperation::NativeStop => "StopDesktopPreview",
        GateSOperation::NativeClick | GateSOperation::NativeNavigate => "native_cell",
        GateSOperation::Save => "Save",
        _ => return false,
    };
    input.session_id == step.before.session_id
        && input.target == expected_target
        && input.generation == step.before.generation
        && input.clicked
        && input.trace_sequence > step.before.trace_sequence
        && input.trace_sequence < step.after.trace_sequence
        && input.accepted_pointer_sequence > step.before.trace_sequence
        && input.accepted_pointer_sequence <= input.trace_sequence
        && l >= 0
        && t >= 0
        && r > l
        && b > t
        && r <= input.client_size[0]
        && b <= input.client_size[1]
        && (input.inserted_move >= 1 || input.target == "native_cell")
        && input.inserted_move <= 8
        && input.inserted_down == 1
        && input.inserted_up == 1
        && matches!(
            input.target.as_str(),
            "AppearanceTile"
                | "AppearanceApply"
                | "AppearanceCancel"
                | "SimpleOpacity"
                | "SimpleScale"
                | "SimpleSpacing"
                | "SimpleLabels"
                | "Undo"
                | "Redo"
                | "MenuRow"
                | "MenuName"
                | "OpenDesktopPreview"
                | "StopDesktopPreview"
                | "Save"
                | "native_cell"
        )
}

pub(crate) fn gate_s_case_contract_is_valid(packet: &GateSCaseEvidence) -> bool {
    if packet.schema_version != 2
        || packet.steps.is_empty()
        || packet.steps.len() > MAX_GATE_S_STEPS
        || packet
            .no_action_proof
            .as_ref()
            .is_none_or(|proof| !gate_d_no_action_proof_is_valid(proof))
    {
        return false;
    }
    for (index, step) in packet.steps.iter().enumerate() {
        if !gate_d_observation_is_well_formed(&step.before, false)
            || !gate_d_observation_is_well_formed(&step.after, false)
            || (step.operation != GateSOperation::Save
                && step.before.session_id != step.after.session_id)
            || step.after.trace_sequence <= step.before.trace_sequence
            || !gate_d_same_owner_effects(&step.before, &step.after)
            || !bounded(&step.appearance_before)
            || !bounded(&step.appearance_after)
            || step.appearance_before.session_id != step.before.session_id
            || step.appearance_after.session_id != step.after.session_id
            || step.appearance_before.generation != step.before.generation
            || step.appearance_after.generation != step.after.generation
            || step.expected_before_document == 0
            || step.expected_after_document == 0
            || step.before.document_digest != step.expected_before_document
            || step.after.document_digest != step.expected_after_document
            || step.before.undo_depth != step.expected_undo_before
            || step.after.undo_depth != step.expected_undo_after
            || step.before.redo_depth != step.expected_redo_before
            || step.after.redo_depth != step.expected_redo_after
            || step.appearance_after.menu_digest != step.expected_menu_after
            || step.before.draft_dirty != step.expected_dirty_before
            || step.after.draft_dirty != step.expected_dirty_after
        {
            return false;
        }
        if index > 0 && packet.steps[index - 1].after.document_digest != step.before.document_digest
        {
            return false;
        }
        let a = &step.appearance_before;
        let b = &step.appearance_after;
        if b.trace_sequence == 0
            || b.trace_sequence < a.trace_sequence
            || step
                .input
                .as_ref()
                .is_some_and(|input| b.trace_sequence <= input.trace_sequence)
        {
            return false;
        }
        match step.operation {
            GateSOperation::Preview => {
                if !input_valid(step)
                    || step.input.as_ref().is_none_or(|input| {
                        input.target != "AppearanceTile"
                            || input.index != step.preset.map(usize::from)
                    })
                    || !gate_d_no_mutation(&step.before, &step.after)
                    || !b.preview_active
                    || b.preview_builtin != step.preset.map_or(-1, i32::from)
                    || b.preview_key == 0
                    || b.tile_scene_digest == 0
                    || b.tile_key != b.preview_key
                    || !exact_prepared_candidate(b, step.expected_candidate_document)
                {
                    return false;
                }
            }
            GateSOperation::Cancel => {
                if !input_valid(step)
                    || !a.preview_active
                    || b.preview_active
                    || !gate_d_no_mutation(&step.before, &step.after)
                {
                    return false;
                }
            }
            GateSOperation::Apply => {
                if !input_valid(step)
                    || !a.preview_active
                    || b.preview_active
                    || b.current_builtin != a.preview_builtin
                    || step.after.generation != step.before.generation + 1
                    || step.after.undo_depth != step.before.undo_depth + 1
                    || step.after.document_digest == step.before.document_digest
                    || a.bindings_digest != b.bindings_digest
                    || a.other_menus_digest != b.other_menus_digest
                    || a.raw_overrides_digest != b.raw_overrides_digest
                    || !same_authoring_context(&step.before, &step.after)
                    || !exact_prepared_candidate(a, step.expected_after_document)
                    || step.after.redo_depth != 0
                {
                    return false;
                }
            }
            GateSOperation::Opacity
            | GateSOperation::Scale
            | GateSOperation::Spacing
            | GateSOperation::Labels => {
                if !input_valid(step)
                    || step.after.generation != step.before.generation + 1
                    || step.after.undo_depth != step.before.undo_depth + 1
                    || a.bindings_digest != b.bindings_digest
                    || a.other_menus_digest != b.other_menus_digest
                    || a.effective_digest == b.effective_digest
                    || !same_authoring_context(&step.before, &step.after)
                    || step.after.redo_depth != 0
                {
                    return false;
                }
                let mapped_change = match step.operation {
                    GateSOperation::Opacity => {
                        a.opacity_milli != b.opacity_milli
                            && a.scale_milli == b.scale_milli
                            && a.spacing_milli == b.spacing_milli
                    }
                    GateSOperation::Scale => {
                        a.scale_milli != b.scale_milli
                            && a.spacing_milli == b.spacing_milli
                            && a.opacity_milli == b.opacity_milli
                            && a.geometry_digest != b.geometry_digest
                    }
                    GateSOperation::Spacing => {
                        a.spacing_milli != b.spacing_milli
                            && a.scale_milli == b.scale_milli
                            && a.opacity_milli == b.opacity_milli
                            && a.geometry_digest != b.geometry_digest
                    }
                    GateSOperation::Labels => a.labels_visible != b.labels_visible,
                    _ => false,
                };
                if !mapped_change {
                    return false;
                }
            }
            GateSOperation::Undo | GateSOperation::Redo => {
                if !input_valid(step)
                    || step.after.generation != step.before.generation + 1
                    || step.after.document_digest == step.before.document_digest
                    || !same_authoring_context(&step.before, &step.after)
                {
                    return false;
                }
            }
            GateSOperation::DirtyRename => {
                if !input_valid(step)
                    || step.after.generation <= step.before.generation
                    || step.after.undo_depth != step.before.undo_depth + 1
                    || step.after.document_digest == step.before.document_digest
                    || !same_authoring_context(&step.before, &step.after)
                {
                    return false;
                }
            }
            GateSOperation::NativeClick | GateSOperation::NativeNavigate => {
                let Some(target) = step.native_target else {
                    return false;
                };
                let Some(expected) = step.native_expected else {
                    return false;
                };
                if !input_valid(step)
                    || !target.clicked
                    || target.session_id != step.before.session_id
                    || target.geometry_digest == 0
                    || target.dpi_milli == 0
                    || target.menu_digest != expected.menu
                    || target.cell_digest != expected.cell
                    || target.generation != expected.generation
                    || target.work_area[0] >= target.work_area[2]
                    || target.work_area[1] >= target.work_area[3]
                    || step.preview_dispatch_after
                        != step.preview_dispatch_before
                            + u64::from(step.operation == GateSOperation::NativeClick)
                    || !gate_d_no_mutation(&step.before, &step.after)
                    || step.native_pointer.as_ref().is_none_or(|pointer| {
                        pointer.down_inserted != 1
                            || pointer.up_inserted != 1
                            || pointer.layout_generation != target.generation
                            || pointer.up_event_ordinal <= pointer.down_event_ordinal
                    })
                {
                    return false;
                }
                if step.operation == GateSOperation::NativeClick && target.kind != 0 {
                    return false;
                }
                if step.operation == GateSOperation::NativeNavigate {
                    let Some(result) = step.native_result else {
                        return false;
                    };
                    if !(1..=4).contains(&target.kind)
                        || result.session_id != target.session_id
                        || result.generation != target.generation + 1
                        || result.menu_digest != expected.result_menu
                        || result.geometry_digest == target.geometry_digest
                        || ((target.kind == 1 || target.kind == 2)
                            == (result.menu_digest == target.menu_digest))
                    {
                        return false;
                    }
                }
            }
            GateSOperation::NativeStart | GateSOperation::NativeStop => {
                if !input_valid(step) || !gate_d_no_mutation(&step.before, &step.after) {
                    return false;
                }
            }
            GateSOperation::Fresh
            | GateSOperation::Idle
            | GateSOperation::LeaveGallery
            | GateSOperation::ReturnGallery
            | GateSOperation::Dense => {
                if !gate_d_same_document_state(&step.before, &step.after)
                    || (step.operation != GateSOperation::Dense
                        && (!gate_d_same_selection(&step.before, &step.after)
                            || !same_authoring_context(&step.before, &step.after)))
                {
                    return false;
                }
            }
            GateSOperation::Save => {
                if !input_valid(step)
                    || step.before.session_id == step.after.session_id
                    || step.after.draft_dirty
                    || step.after.undo_depth != 0
                    || step.after.redo_depth != 0
                {
                    return false;
                }
            }
        }
    }
    let ops: Vec<_> = packet.steps.iter().map(|step| step.operation).collect();
    match packet.case_id.as_str() {
        "S01" => {
            ops == [GateSOperation::Fresh] && packet.steps[0].appearance_after.current_builtin == 0
        }
        "S02" => {
            ops == [
                GateSOperation::DirtyRename,
                GateSOperation::Preview,
                GateSOperation::Cancel,
            ] && packet.steps[0].after.draft_dirty
        }
        "S03" => {
            ops == [
                GateSOperation::Opacity,
                GateSOperation::Scale,
                GateSOperation::Spacing,
            ] && packet.steps.iter().all(|step| {
                step.appearance_after.prepared_generation == step.after.generation
                    && step.appearance_after.geometry_digest != 0
            })
        }
        "S04" => {
            ops == [
                GateSOperation::Preview,
                GateSOperation::Apply,
                GateSOperation::Undo,
                GateSOperation::Redo,
            ] && packet.steps[2].after.document_digest == packet.steps[0].before.document_digest
                && packet.steps[3].after.document_digest == packet.steps[1].after.document_digest
                && same_authoring_context(&packet.steps[2].after, &packet.steps[0].before)
                && same_authoring_context(&packet.steps[3].after, &packet.steps[1].after)
                && packet.steps[2].after.undo_depth == packet.steps[0].before.undo_depth
                && packet.steps[2].after.redo_depth == packet.steps[0].before.redo_depth + 1
                && packet.steps[3].after.undo_depth == packet.steps[1].after.undo_depth
                && packet.steps[3].after.redo_depth == packet.steps[1].after.redo_depth
        }
        "S05" => {
            ops == [
                GateSOperation::Preview,
                GateSOperation::Preview,
                GateSOperation::Preview,
                GateSOperation::Preview,
                GateSOperation::Cancel,
            ] && packet
                .steps
                .iter()
                .take(4)
                .enumerate()
                .all(|(i, step)| step.preset == Some((i + 1) as u8))
                && packet.steps[2].appearance_after.tile_selected_emphasis
                && packet.steps[..4].windows(2).all(|pair| {
                    pair[0].appearance_after.tile_scene_digest
                        != pair[1].appearance_after.tile_scene_digest
                })
        }
        "S06" => {
            ops == [GateSOperation::Dense]
                && packet.steps[0].appearance_after.authored_cell_count == 50
                && packet.steps[0].appearance_after.density_warnings > 0
                && packet.steps[0].appearance_after.density_warnings <= 4
                && packet.steps[0].before.document_digest == packet.steps[0].after.document_digest
        }
        "S07" => {
            ops == [
                GateSOperation::NativeStart,
                GateSOperation::NativeClick,
                GateSOperation::NativeNavigate,
                GateSOperation::NativeNavigate,
                GateSOperation::NativeNavigate,
                GateSOperation::NativeNavigate,
                GateSOperation::NativeStop,
            ] && packet.steps[2..6]
                .iter()
                .zip([1, 3, 4, 2])
                .all(|(step, kind)| step.native_target.is_some_and(|target| target.kind == kind))
                && {
                    let mut generation = packet.steps[0].before.generation;
                    packet.steps[1..6].iter().all(|step| {
                        let valid = step
                            .native_expected
                            .is_some_and(|expected| expected.generation == generation);
                        if step.operation == GateSOperation::NativeNavigate {
                            generation += 1;
                        }
                        valid
                    })
                }
        }
        "S08" => ops == [GateSOperation::Save] && !packet.steps[0].after.draft_dirty,
        "S09" => {
            ops == [GateSOperation::Idle, GateSOperation::LeaveGallery]
                && packet.steps[0].appearance_before.requested
                    == packet.steps[0].appearance_after.requested
                && packet.steps[0].appearance_before.completed
                    == packet.steps[0].appearance_after.completed
                && packet.steps[0].appearance_after.in_flight == 0
                && packet.steps[0].appearance_after.visible_pending == 0
                && packet.steps[0].appearance_before.visible_pending == 0
                && packet.steps[0].appearance_before.in_flight == 0
                && !packet.steps[1].appearance_after.gallery_active
                && packet.steps[1].appearance_before.requested
                    == packet.steps[1].appearance_after.requested
                && packet.steps[1].appearance_before.completed
                    == packet.steps[1].appearance_after.completed
        }
        _ => false,
    }
}

pub(crate) fn validate_gate_s_evidence_report(report: &AcceptanceReport) -> Result<(), String> {
    if report.suite != AcceptanceSuite::GateS {
        return if report.gate_s_evidence.is_empty() {
            Ok(())
        } else {
            Err("Gate S evidence outside Gate S suite".into())
        };
    }
    if report.gate_s_evidence.len() != 9 {
        return Err("Gate S requires nine ordered typed packets".into());
    }
    let mut total = 0;
    for (index, packet) in report.gate_s_evidence.iter().enumerate() {
        let bytes = serde_json::to_vec(packet).map_err(|e| e.to_string())?.len();
        total += bytes;
        if packet.schema_version != 2
            || packet.case_id != GATE_S_REQUIRED_CASE_IDS[index]
            || packet.fixture_digest != gate_s_fixture_digest(&report.profile)
            || packet.steps.len() > MAX_GATE_S_STEPS
            || bytes > MAX_GATE_S_PACKET_BYTES
        {
            return Err("Gate S packet identity/bounds invalid".into());
        }
        if report
            .cases
            .iter()
            .any(|case| case.id == packet.case_id && matches!(case.status, CaseStatus::Passed))
            && !gate_s_case_contract_is_valid(packet)
        {
            return Err(format!(
                "{} passed without required Gate S evidence",
                packet.case_id
            ));
        }
    }
    if total > MAX_GATE_S_EVIDENCE_BYTES {
        return Err("Gate S aggregate evidence exceeds its bound".into());
    }
    Ok(())
}

pub(crate) fn gate_s_fixture(
    mut fixture: DeterministicFixture,
) -> Result<DeterministicFixture, String> {
    use multi_launcher::radial::model::*;
    let mut document: RadialDocument =
        serde_json::from_slice(&fixture.radial_json).map_err(|error| error.to_string())?;
    let mut action = document
        .menus
        .iter()
        .flat_map(|menu| &menu.rings)
        .flat_map(|ring| &ring.cells)
        .find(|cell| matches!(cell.content, CellContent::Action { .. }))
        .cloned()
        .ok_or("Gate S fixture has no safe action")?;
    // This known read-only query has a valid KeepOpen requirement. The native
    // preview intercepts it; browsing and this fixture never execute it.
    action.content = CellContent::Action {
        binding: ActionBinding::Persisted {
            action: PersistedUniversalActionRef {
                target: Some(PersistableActionTargetRef::CustomAction {
                    action: multi_launcher::actions::Action {
                        label: "Gate S read-only desktop query".into(),
                        desc: "Intercepted native appearance fixture".into(),
                        action: "vd:current".into(),
                        args: None,
                    },
                }),
                action_id: action_ids::RESULT_EXECUTE,
            },
        },
    };
    let root = document
        .menus
        .iter_mut()
        .find(|menu| menu.id == document.default_menu_id)
        .ok_or("root missing")?;
    // Nine authored cells must also fit 72px presets with the retained S03 spacing.
    root.rings[0].radius = 96.0;
    let root_cell = &mut root.rings[0].cells[0];
    root_cell.content = action.content.clone();
    root_cell.label = "Gate S safe target".into();
    root_cell.after_action = AfterActionPolicy::KeepOpen;
    let mut paged = root.clone();
    paged.id = MenuId::new("gate-s-paged");
    paged.name = "Gate S native pages".into();
    paged.submenu_presentation = SubmenuPresentation::SameCenter;
    paged.center_control = Some(Control::Back);
    paged.center_action = None;
    paged.center_secondary_action = None;
    paged.background_action = None;
    paged.background_secondary_action = None;
    paged.rings.truncate(1);
    paged.rings[0].id = RingId::new("gate-s-paged-ring");
    let mut dynamic = action.clone();
    dynamic.id = CellId::new("gate-s-paged-source");
    dynamic.label = "Safe synthetic items".into();
    dynamic.content = CellContent::Dynamic {
        source: DynamicSource::Applications,
    };
    dynamic.alternate_clicks.clear();
    dynamic.after_action = AfterActionPolicy::KeepOpen;
    paged.rings[0].cells = vec![dynamic];
    root.rings[0].cells[1].content = CellContent::Submenu {
        menu_id: paged.id.clone(),
    };
    root.rings[0].cells[1].label = "Gate S pages".into();
    let mut dense = root.clone();
    dense.id = MenuId::new("gate-s-dense");
    dense.name = "Gate S dense fifty".into();
    dense.layout = LayoutKind::Wedges;
    dense.rings.truncate(1);
    dense.rings[0].id = RingId::new("gate-s-dense-ring");
    dense.rings[0].cells = (0..50)
        .map(|index| {
            let mut cell = action.clone();
            cell.id = CellId::new(format!("gate-s-dense-{index}"));
            cell.label = "Dense measured label".into();
            cell
        })
        .collect();
    dense.style.values.text.font_size = Override::Value(22.0);
    document.menus.push(dense);
    document.menus.push(paged);
    // A reserved-looking custom ID must survive builtin application unchanged.
    let mut custom = multi_launcher::radial::appearance::BuiltinSkin::Compact.definition();
    custom.name = "Authored compact collision".into();
    custom.style.values.text.italic = Override::Value(true);
    document.skins.push(custom);
    validate_radial_document(&document)
        .map_err(|error| format!("invalid Gate S fixture: {error:?}"))?;
    fixture.radial_json =
        serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?;
    Ok(fixture)
}
