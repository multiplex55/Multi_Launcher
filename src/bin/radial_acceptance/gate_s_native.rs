//! Gate S uses the real owned Designer controls and shared native preview.
use super::super::super::gate_s::*;
use super::*;
use multi_launcher::radial::gallery::{AppearanceObservation, NativeAppearanceTarget};

fn parse_appearance(line: &str) -> Option<AppearanceObservation> {
    if !line.contains("trace_event=\"appearance_state\"") {
        return None;
    }
    let mut state = AppearanceObservation::default();
    macro_rules! number {($($field:ident),+) => {$(state.$field=trace_field(line,stringify!($field))?.parse().ok()?;)+};}
    macro_rules! boolean {($($field:ident),+) => {$(state.$field=trace_bool_field(line,stringify!($field))?;)+};}
    number!(
        trace_sequence,
        session_id,
        generation,
        menu_digest,
        catalog_count,
        current_builtin,
        preview_builtin,
        preview_key,
        preview_candidate_digest,
        preview_token_digest,
        prepared_menu_digest,
        prepared_candidate_digest,
        prepared_content_key,
        prepared_token_digest,
        prepared_owner_session,
        effective_digest,
        bindings_digest,
        other_menus_digest,
        raw_overrides_digest,
        opacity_milli,
        scale_milli,
        spacing_milli,
        label_size_milli,
        masked_fields,
        requested,
        completed,
        rejected,
        in_flight,
        visible_pending,
        cached_tiles,
        cpu_bytes,
        gpu_bytes,
        failure_count,
        tile_key,
        tile_scene_digest,
        tile_geometry_digest,
        prepared_generation,
        geometry_digest,
        scene_digest,
        authored_cell_count,
        density_warnings,
        diagnostic_digest
    );
    boolean!(
        preview_active,
        labels_visible,
        gallery_active,
        tile_selected_emphasis
    );
    Some(state)
}

fn native_targets(trace: &Path, session: u64) -> Vec<(u64, NativeAppearanceTarget)> {
    native_targets_in_lines(&trace_lines(trace))
        .into_iter()
        .filter(|(_, state)| state.session_id == session)
        .collect()
}

fn native_targets_in_lines(lines: &[String]) -> Vec<(u64, NativeAppearanceTarget)> {
    lines.iter().filter_map(|line| {
        if !line.contains("trace_event=\"native_appearance_target\"") {return None;}
        let mut state=NativeAppearanceTarget::default();
        macro_rules! number {($($field:ident),+) => {$(state.$field=trace_field(line,stringify!($field))?.parse().ok()?;)+};}
        number!(session_id,generation,menu_digest,cell_digest,geometry_digest,point_x,point_y,dpi_milli);
        state.kind=trace_field(line,"cell_kind")?.parse().ok()?;state.clicked=trace_bool_field(line,"clicked")?;
        state.work_area=[trace_field(line,"work_left")?.parse().ok()?,trace_field(line,"work_top")?.parse().ok()?,trace_field(line,"work_right")?.parse().ok()?,trace_field(line,"work_bottom")?.parse().ok()?];
        Some((trace_field(line,"trace_sequence")?.parse().ok()?,state))
    }).collect()
}

fn native_prepared_target_is_current(
    targets: &[(u64, NativeAppearanceTarget)],
    expected: NativeAppearanceTarget,
) -> bool {
    targets.last().is_some_and(|(_, latest)| {
        latest.session_id == expected.session_id
            && latest.generation == expected.generation
            && latest.menu_digest == expected.menu_digest
            && latest.geometry_digest == expected.geometry_digest
            && latest.dpi_milli == expected.dpi_milli
            && latest.work_area == expected.work_area
    }) && targets
        .iter()
        .any(|(_, target)| *target == expected && !target.clicked)
}

fn wait_appearance_after(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace: &Path,
    session: u64,
    after_sequence: u64,
    predicate: impl Fn(&AppearanceObservation) -> bool,
) -> Result<AppearanceObservation, CaseFailure> {
    let deadline = Instant::now() + UIA_TIMEOUT;
    loop {
        child
            .request_designer_repaint(designer)
            .map_err(|error| CaseFailure::new(FailureStage::DesignerReadiness, error))?;
        if let Some(state) = trace_lines(trace)
            .iter()
            .rev()
            .filter(|line| {
                trace_field(line, "trace_sequence")
                    .and_then(|value| value.parse::<u64>().ok())
                    .is_some_and(|sequence| sequence > after_sequence)
            })
            .filter_map(|line| parse_appearance(line))
            .find(|state| state.session_id == session)
        {
            if predicate(&state) {
                return Ok(state);
            }
        }
        if Instant::now() >= deadline {
            return Err(CaseFailure::new(
                FailureStage::DesignerReadiness,
                "appearance owner did not publish the required prepared/correlated state".into(),
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_appearance(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace: &Path,
    session: u64,
    predicate: impl Fn(&AppearanceObservation) -> bool,
) -> Result<AppearanceObservation, CaseFailure> {
    let boundary = trace_lines(trace)
        .iter()
        .rev()
        .find_map(|line| trace_field(line, "trace_sequence")?.parse::<u64>().ok())
        .unwrap_or(0);
    wait_appearance_after(child, designer, trace, session, boundary, predicate)
}

fn snapshot(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace: &Path,
    session: u64,
) -> Result<(GateDObservationEvidence, AppearanceObservation), CaseFailure> {
    let owner = gate_d_live_observation(child, session)?;
    let appearance = wait_appearance(child, designer, trace, session, |state| {
        state.generation == owner.generation
            && state.prepared_generation == owner.generation
            && state.geometry_digest != 0
            && state.prepared_owner_session == session
            && state.prepared_menu_digest == state.menu_digest
            && (!state.preview_active
                || (state.prepared_candidate_digest == state.preview_candidate_digest
                    && state.prepared_content_key == state.preview_key
                    && state.prepared_token_digest == state.preview_token_digest))
    })?;
    Ok((owner, appearance))
}

fn ready(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace: &Path,
    session: u64,
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
) -> Result<AuthoringControlSnapshot, CaseFailure> {
    let owner = gate_d_live_observation(child, session)?;
    let client_size = child
        .request_designer_repaint(designer)
        .map_err(|error| CaseFailure::new(FailureStage::DesignerReadiness, error))?;
    let boundary = GateDPresentationBoundary {
        first_line: trace_lines(trace).len(),
        trace_sequence: owner.trace_sequence,
        session_id: session,
        generation: owner.generation,
        client_size,
    };
    gate_d_ready_control(child, designer, trace, boundary, target, index, role, None)
}

fn click(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace: &Path,
    control: AuthoringControlSnapshot,
) -> Result<GateSInput, CaseFailure> {
    let before = gate_d_live_observation(child, control.session_id)?;
    if before.generation != control.generation {
        return Err(CaseFailure::new(
            FailureStage::DesignerReadiness,
            "appearance click target no longer belongs to the captured draft generation".into(),
        ));
    }
    let (control, pointer, cursor) = click_with_geometry_reacquisition(
        UIA_TIMEOUT.saturating_mul(3),
        |remaining| {
            let boundary = GateDPresentationBoundary {
                first_line: trace_lines(trace).len(),
                trace_sequence: before.trace_sequence,
                session_id: control.session_id,
                generation: control.generation,
                client_size: control.client_size,
            };
            let prepared = gate_d_ready_control_before_deadline(
                child,
                designer,
                trace,
                boundary,
                control.target,
                control.index,
                control.role,
                control.authored_target_digest,
                Instant::now() + remaining,
            )?;
            let after = gate_d_live_observation(child, control.session_id)?;
            if prepared.index != control.index
                || !gate_d_presentation_state_is_unchanged(&before, &after)
            {
                return Err(CaseFailure::new(
                    FailureStage::DesignerReadiness,
                    "appearance click reacquisition changed its exact slot or captured authoring state".into(),
                ));
            }
            Ok(prepared)
        },
        |mut prepared, remaining| {
            let deadline = Instant::now() + remaining;
            let cursor = trace_lines(trace).len();
            let pointer = click_designer_client_bounds_with_pre_down_check(
                child,
                designer,
                prepared.bounds,
                trace,
                |point, pointer_prepare_cursor| {
                    prepared = gate_d_wait_control_before_pointer_down(
                        child,
                        designer,
                        trace,
                        pointer_prepare_cursor,
                        &prepared,
                        &before,
                        [point.0, point.1],
                        deadline.saturating_duration_since(Instant::now()),
                    )?;
                    Ok(())
                },
            )?;
            Ok((prepared, pointer, cursor))
        },
    )?;
    let receipt = wait_for_authoring_control_click_receipt_after(
        trace,
        cursor,
        &control,
        AuthoringControlClickCompletion::CapturedClick,
        TRACE_TIMEOUT,
    )
    .map_err(|error| CaseFailure::new(FailureStage::DesignerFrameworkInput, error))?;
    if !click_receipt_matches_prepared(&receipt, &control) {
        return Err(CaseFailure::new(
            FailureStage::DesignerFrameworkInput,
            "appearance click receipt changed captured owner/geometry".into(),
        ));
    }
    let accepted = owned_designer_pointer_release_after(
        &trace_lines(trace),
        DesignerPointerReleaseBoundary {
            first_line: cursor,
            after_trace_sequence: control.trace_sequence,
            through_trace_sequence: receipt.trace_sequence,
            session_id: control.session_id,
            generation: control.generation,
            target_hwnd: hwnd_id(designer.hwnd),
        },
        &pointer,
    )
    .map_err(|error| CaseFailure::new(FailureStage::DesignerFrameworkInput, error))?;
    Ok(GateSInput {
        target: format!("{:?}", control.target),
        index: control.index,
        bounds: control.bounds,
        client_size: control.client_size,
        session_id: control.session_id,
        generation: control.generation,
        trace_sequence: receipt.trace_sequence,
        clicked: receipt.clicked,
        inserted_move: pointer.movement.inserted
            + pointer.nudge_movement.inserted
            + pointer.pointer_correction_events,
        inserted_down: pointer.down.inserted,
        inserted_up: pointer.up.inserted,
        accepted_pointer_sequence: accepted.trace_sequence,
    })
}

fn click_receipt_matches_prepared(
    receipt: &AuthoringControlSnapshot,
    prepared: &AuthoringControlSnapshot,
) -> bool {
    authoring_control_has_same_identity(receipt, prepared)
        && receipt.enabled
        && receipt.clicked
        && receipt.trace_sequence > prepared.trace_sequence
        && receipt.bounds == prepared.bounds
        && receipt.clip_bounds == prepared.clip_bounds
        && receipt.client_size == prepared.client_size
        && receipt.generation == prepared.generation
        && receipt
            .frame_nr
            .zip(prepared.frame_nr)
            .is_some_and(|(receipt, prepared)| receipt >= prepared)
}

fn click_with_geometry_reacquisition<T>(
    timeout: Duration,
    mut prepare: impl FnMut(Duration) -> Result<AuthoringControlSnapshot, CaseFailure>,
    mut dispatch: impl FnMut(AuthoringControlSnapshot, Duration) -> Result<T, PointerClickPreDownError>,
) -> Result<T, CaseFailure> {
    let deadline = Instant::now() + timeout;
    for attempt in 0..MAX_PRE_DOWN_GEOMETRY_REACQUISITIONS {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let control = prepare(remaining)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match dispatch(control, remaining) {
            Ok(clicked) => return Ok(clicked),
            Err(PointerClickPreDownError::StaleGeometry(reason)) => {
                if attempt + 1 == MAX_PRE_DOWN_GEOMETRY_REACQUISITIONS || Instant::now() >= deadline
                {
                    return Err(CaseFailure::new(
                        FailureStage::DesignerReadiness,
                        format!(
                            "appearance click geometry remained stale after bounded pre-down reacquisition: {reason}"
                        ),
                    ));
                }
            }
            Err(PointerClickPreDownError::Input(error)) => {
                // Input may already have sent a button edge. It is never retried.
                return Err(CaseFailure::new(FailureStage::InputInjection, error));
            }
        }
    }
    Err(CaseFailure::new(
        FailureStage::DesignerReadiness,
        "appearance click geometry did not settle before the bounded deadline".into(),
    ))
}

fn step(
    packet: &mut GateSCaseEvidence,
    operation: GateSOperation,
    preset: Option<u8>,
    before: (GateDObservationEvidence, AppearanceObservation),
    after: (GateDObservationEvidence, AppearanceObservation),
    input: Option<GateSInput>,
) {
    packet.steps.push(GateSStep {
        operation,
        preset,
        before: before.0,
        after: after.0,
        appearance_before: before.1,
        appearance_after: after.1,
        input,
        native_target: None,
        native_pointer: None,
        native_result: None,
        preview_dispatch_before: 0,
        preview_dispatch_after: 0,
        expected_before_document: 0,
        expected_after_document: 0,
        expected_candidate_document: 0,
        expected_undo_before: 0,
        expected_undo_after: 0,
        expected_redo_before: 0,
        expected_redo_after: 0,
        native_expected: None,
        expected_menu_after: 0,
        expected_dirty_before: false,
        expected_dirty_after: false,
    });
}

fn clicked_step(
    packet: &mut GateSCaseEvidence,
    operation: GateSOperation,
    preset: Option<u8>,
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace: &Path,
    session: u64,
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
) -> Result<(), CaseFailure> {
    let control = ready(child, designer, trace, session, target, index, role)?;
    let before = snapshot(child, designer, trace, session)?;
    let input = click(child, designer, trace, control)?;
    let generation = before.0.generation;
    wait_appearance_after(
        child,
        designer,
        trace,
        session,
        input.trace_sequence,
        |state| match operation {
            GateSOperation::Preview => {
                state.preview_active
                    && state.preview_builtin == preset.map_or(-1, i32::from)
                    && state.tile_key == state.preview_key
                    && state.tile_scene_digest != 0
                    && state.prepared_candidate_digest == state.preview_candidate_digest
                    && state.prepared_content_key == state.preview_key
                    && state.prepared_token_digest == state.preview_token_digest
                    && state.prepared_menu_digest == state.menu_digest
                    && state.prepared_owner_session == session
            }
            GateSOperation::Cancel => !state.preview_active,
            GateSOperation::Apply => state.generation > generation && !state.preview_active,
            GateSOperation::Opacity
            | GateSOperation::Scale
            | GateSOperation::Spacing
            | GateSOperation::Labels
            | GateSOperation::Undo
            | GateSOperation::Redo => state.generation > generation,
            _ => true,
        },
    )?;
    let after = snapshot(child, designer, trace, session)?;
    step(packet, operation, preset, before, after, Some(input));
    Ok(())
}

fn skins(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace: &Path,
    session: u64,
) -> Result<(), CaseFailure> {
    ensure_designer_semantic_target_selected(
        child,
        designer,
        trace,
        session,
        DesignerSemanticTarget::Skins,
    )
    .map(|_| ())
}

fn choose_menu(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace: &Path,
    session: u64,
    index: usize,
) -> Result<GateSInput, CaseFailure> {
    ensure_designer_semantic_target_selected(
        child,
        designer,
        trace,
        session,
        DesignerSemanticTarget::Menus,
    )?;
    gate_d_establish_presentation(
        child,
        designer,
        trace,
        session,
        GateDPresentationPhase::TreeSearch,
    )?;
    let control = ready(
        child,
        designer,
        trace,
        session,
        AuthoringControlTarget::MenuRow,
        Some(index),
        AuthoringControlRole::Selectable,
    )?;
    click(child, designer, trace, control)
}

fn run_case(
    packet: &mut GateSCaseEvidence,
    profile: &Path,
    child: &mut NativeChild,
    entry: &mut DesignerEntry,
    anchor: &FocusAnchor,
    oracle: &GateSDocumentOracle,
    trace: &Path,
) -> Result<String, CaseFailure> {
    let designer = &entry.window;
    let session = entry.session_id;
    match packet.case_id.as_str() {
        "S01" => {
            skins(child, designer, trace, session)?;
            wait_appearance(child, designer, trace, session, |state| {
                state.current_builtin == 0 && state.tile_scene_digest != 0
            })?;
            let before = snapshot(child, designer, trace, session)?;
            let after = snapshot(child, designer, trace, session)?;
            step(packet, GateSOperation::Fresh, None, before, after, None);
        }
        "S02" => {
            ensure_designer_semantic_target_selected(
                child,
                designer,
                trace,
                session,
                DesignerSemanticTarget::Menus,
            )?;
            let name = select_starter_menu_name_target(child, designer, trace, session)?;
            let before = snapshot(child, designer, trace, session)?;
            if name.session_id != before.0.session_id || name.generation != before.0.generation {
                return Err(CaseFailure::new(
                    FailureStage::DesignerReadiness,
                    "dirty-name target did not match the captured Designer owner".into(),
                ));
            }
            let cursor = trace_lines(trace).len();
            let pointer = click_designer_client_bounds(child, designer, name.bounds, trace)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            replace_focused_designer_text(child, designer, "Gate S unrelated dirty name")
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            wait_appearance(child, designer, trace, session, |state| {
                state.generation > before.0.generation
            })?;
            let after = snapshot(child, designer, trace, session)?;
            let release = owned_designer_pointer_release_after(
                &trace_lines(trace),
                DesignerPointerReleaseBoundary {
                    first_line: cursor,
                    after_trace_sequence: before.0.trace_sequence,
                    through_trace_sequence: after.0.trace_sequence,
                    session_id: before.0.session_id,
                    generation: before.0.generation,
                    target_hwnd: hwnd_id(designer.hwnd),
                },
                &pointer,
            )
            .map_err(|error| CaseFailure::new(FailureStage::DesignerFrameworkInput, error))?;
            let sequence = release.trace_sequence;
            let input = GateSInput {
                target: "MenuName".into(),
                index: None,
                bounds: name.bounds,
                client_size: child
                    .request_designer_repaint(designer)
                    .map_err(|e| CaseFailure::new(FailureStage::DesignerReadiness, e))?,
                session_id: session,
                generation: before.0.generation,
                trace_sequence: sequence,
                clicked: true,
                inserted_move: pointer.movement.inserted
                    + pointer.nudge_movement.inserted
                    + pointer.pointer_correction_events,
                inserted_down: pointer.down.inserted,
                inserted_up: pointer.up.inserted,
                accepted_pointer_sequence: sequence,
            };
            step(
                packet,
                GateSOperation::DirtyRename,
                None,
                before,
                after,
                Some(input),
            );
            skins(child, designer, trace, session)?;
            clicked_step(
                packet,
                GateSOperation::Preview,
                Some(1),
                child,
                designer,
                trace,
                session,
                AuthoringControlTarget::AppearanceTile,
                Some(1),
                AuthoringControlRole::Selectable,
            )?;
            clicked_step(
                packet,
                GateSOperation::Cancel,
                None,
                child,
                designer,
                trace,
                session,
                AuthoringControlTarget::AppearanceCancel,
                None,
                AuthoringControlRole::Button,
            )?;
        }
        "S03" => {
            skins(child, designer, trace, session)?;
            for (operation, target) in [
                (
                    GateSOperation::Opacity,
                    AuthoringControlTarget::SimpleOpacity,
                ),
                (GateSOperation::Scale, AuthoringControlTarget::SimpleScale),
                (
                    GateSOperation::Spacing,
                    AuthoringControlTarget::SimpleSpacing,
                ),
            ] {
                clicked_step(
                    packet,
                    operation,
                    None,
                    child,
                    designer,
                    trace,
                    session,
                    target,
                    Some(usize::from(operation != GateSOperation::Opacity)),
                    AuthoringControlRole::Button,
                )?;
            }
        }
        "S04" => {
            clicked_step(
                packet,
                GateSOperation::Preview,
                Some(1),
                child,
                designer,
                trace,
                session,
                AuthoringControlTarget::AppearanceTile,
                Some(1),
                AuthoringControlRole::Selectable,
            )?;
            clicked_step(
                packet,
                GateSOperation::Apply,
                None,
                child,
                designer,
                trace,
                session,
                AuthoringControlTarget::AppearanceApply,
                None,
                AuthoringControlRole::Button,
            )?;
            clicked_step(
                packet,
                GateSOperation::Undo,
                None,
                child,
                designer,
                trace,
                session,
                AuthoringControlTarget::Undo,
                None,
                AuthoringControlRole::Button,
            )?;
            clicked_step(
                packet,
                GateSOperation::Redo,
                None,
                child,
                designer,
                trace,
                session,
                AuthoringControlTarget::Redo,
                None,
                AuthoringControlRole::Button,
            )?;
        }
        "S05" => {
            for preset in 1..=4 {
                clicked_step(
                    packet,
                    GateSOperation::Preview,
                    Some(preset),
                    child,
                    designer,
                    trace,
                    session,
                    AuthoringControlTarget::AppearanceTile,
                    Some(preset as usize),
                    AuthoringControlRole::Selectable,
                )?;
            }
            clicked_step(
                packet,
                GateSOperation::Cancel,
                None,
                child,
                designer,
                trace,
                session,
                AuthoringControlTarget::AppearanceCancel,
                None,
                AuthoringControlRole::Button,
            )?;
        }
        "S06" => {
            let document = gate_d_fixture_document(profile)?;
            let index = document
                .menus
                .iter()
                .position(|menu| menu.id.as_str() == "gate-s-dense")
                .ok_or_else(|| {
                    CaseFailure::new(FailureStage::Environment, "dense fixture missing".into())
                })?;
            let before = snapshot(child, designer, trace, session)?;
            let input = choose_menu(child, designer, trace, session, index)?;
            wait_appearance(child, designer, trace, session, |state| {
                state.authored_cell_count == 50 && state.density_warnings > 0
            })?;
            let after = snapshot(child, designer, trace, session)?;
            step(
                packet,
                GateSOperation::Dense,
                None,
                before,
                after,
                Some(input),
            );
        }
        "S07" => {
            let document = gate_d_fixture_document(profile)?;
            let index = document
                .menus
                .iter()
                .position(|menu| menu.id == document.default_menu_id)
                .ok_or_else(|| {
                    CaseFailure::new(FailureStage::Environment, "default menu missing".into())
                })?;
            choose_menu(child, designer, trace, session, index)?;
            let before = snapshot(child, designer, trace, session)?;
            let control = ready(
                child,
                designer,
                trace,
                session,
                AuthoringControlTarget::OpenDesktopPreview,
                None,
                AuthoringControlRole::Button,
            )?;
            let cursor = trace_lines(trace).len();
            let input = click(child, designer, trace, control)?;
            wait_for_terminal_authoring_request(
                trace,
                cursor,
                session,
                "StartNativePreview",
                UIA_TIMEOUT,
            )
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerReadiness,
                    "native preview start lacked correlated reply".into(),
                )
            })?;
            let after = snapshot(child, designer, trace, session)?;
            step(
                packet,
                GateSOperation::NativeStart,
                None,
                before,
                after,
                Some(input),
            );
            let mut native_generation = before_generation_from_start(packet)?;
            for kind in [0, 1, 3, 4, 2] {
                let expected = oracle
                    .native_expectation(kind, native_generation)
                    .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
                native_transition(packet, child, designer, trace, session, kind, expected)?;
                if kind != 0 {
                    native_generation += 1;
                }
            }
            let original = child
                .designer()
                .filter(|current| current.hwnd == designer.hwnd)
                .ok_or_else(|| {
                    CaseFailure::new(
                        FailureStage::DesignerPresentation,
                        "S07 Designer owner changed before Stop".into(),
                    )
                })?;
            let before = snapshot(child, &original, trace, session)?;
            let target = packet
                .steps
                .last()
                .and_then(|step| step.native_result)
                .ok_or_else(|| {
                    CaseFailure::new(
                        FailureStage::DesignerNativeTarget,
                        "S07 Back result missing before Stop".into(),
                    )
                })?;
            let preview = runtime_windows(child);
            validate_radial_surfaces(child, &preview)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerPresentation, error))?;
            radial_input_surface_at(
                child,
                &preview,
                POINT {
                    x: target.point_x,
                    y: target.point_y,
                },
            )
            .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
            let preview_bounds = preview
                .iter()
                .map(|surface| surface.bounds)
                .collect::<Vec<_>>();
            let displays = native_display_bounds()
                .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
            let position = designer_clear_position(original.bounds, &preview_bounds, &displays)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerPresentation, error))?;
            let input = with_relocated_designer_for_preview_stop(
                || {
                    child
                        .move_owned_designer(&original, position)
                        .map_err(|error| {
                            CaseFailure::new(FailureStage::DesignerPresentation, error)
                        })
                },
                |moved| {
                    // Repaint and reacquire the actual client control after the
                    // move; no screen point from the covered position is reused.
                    let control = ready(
                        child,
                        moved,
                        trace,
                        session,
                        AuthoringControlTarget::StopDesktopPreview,
                        None,
                        AuthoringControlRole::Button,
                    )?;
                    let current = gate_d_live_observation(child, session)?;
                    if !gate_d_presentation_state_is_unchanged(&before.0, &current)
                        || !radial_surfaces_are_active(child, &preview)
                        || !native_prepared_target_is_current(
                            &native_targets_in_lines(&trace_lines(trace)),
                            target,
                        )
                    {
                        return Err(CaseFailure::new(FailureStage::DesignerReadiness,
                            "S07 authoring or exact preview owner changed during Designer relocation".into()));
                    }
                    let cursor = trace_lines(trace).len();
                    let input = click(child, moved, trace, control)?;
                    wait_for_terminal_authoring_request(
                        trace,
                        cursor,
                        session,
                        "StopNativePreview",
                        UIA_TIMEOUT,
                    )
                    .ok_or_else(|| {
                        CaseFailure::new(
                            FailureStage::DesignerReadiness,
                            "native preview stop lacked correlated reply".into(),
                        )
                    })?;
                    Ok(input)
                },
                || {
                    let current = child
                        .designer()
                        .filter(|current| {
                            current.hwnd == original.hwnd
                                && current.process_id == original.process_id
                                && current.class_name == original.class_name
                        })
                        .ok_or_else(|| {
                            CaseFailure::new(
                                FailureStage::DesignerPresentation,
                                "original S07 Designer owner missing during restoration".into(),
                            )
                        })?;
                    let restored = child
                        .move_owned_designer(&current, [original.bounds[0], original.bounds[1]])
                        .map_err(|error| {
                            CaseFailure::new(FailureStage::DesignerPresentation, error)
                        })?;
                    if restored.bounds != original.bounds {
                        return Err(CaseFailure::new(FailureStage::DesignerPresentation,
                            "S07 Designer restoration did not preserve its original dimensions and position".into()));
                    }
                    Ok(())
                },
            )?;
            let after = snapshot(child, &original, trace, session)?;
            step(
                packet,
                GateSOperation::NativeStop,
                None,
                before,
                after,
                Some(input),
            );
        }
        "S08" => {
            let before = snapshot(child, designer, trace, session)?;
            let control = ready(
                child,
                designer,
                trace,
                session,
                AuthoringControlTarget::Save,
                None,
                AuthoringControlRole::Button,
            )?;
            let input = click(child, designer, trace, control)?;
            if !wait_until(UIA_TIMEOUT, || child.designer().is_none()) {
                return Err(CaseFailure::new(
                    FailureStage::DesignerReadiness,
                    "appearance Save did not close its Designer owner".into(),
                ));
            }
            let saved = gate_d_fixture_document(profile)?;
            let uia = UiAutomation::new()
                .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
            let reopened = run_designer_entry(child, &uia, anchor, trace)?;
            skins(child, &reopened.window, trace, reopened.session_id)?;
            let after = snapshot(child, &reopened.window, trace, reopened.session_id)?;
            if gate_d_document_digest(&saved)? != after.0.document_digest {
                return Err(CaseFailure::new(
                    FailureStage::DesignerMutation,
                    "saved appearance differs from authoritative reopened draft".into(),
                ));
            }
            *entry = reopened;
            step(
                packet,
                GateSOperation::Save,
                None,
                before,
                after,
                Some(input),
            );
        }
        "S09" => {
            skins(child, designer, trace, session)?;
            wait_appearance(child, designer, trace, session, |state| {
                state.gallery_active
                    && state.in_flight == 0
                    && state.visible_pending == 0
                    && state.completed > 0
            })?;
            let before = snapshot(child, designer, trace, session)?;
            for _ in 0..32 {
                let _ = gate_d_live_observation(child, session)?;
                child
                    .request_designer_repaint(designer)
                    .map_err(|e| CaseFailure::new(FailureStage::DesignerReadiness, e))?;
            }
            let after = snapshot(child, designer, trace, session)?;
            step(packet, GateSOperation::Idle, None, before, after, None);
            let before = snapshot(child, designer, trace, session)?;
            ensure_designer_semantic_target_selected(
                child,
                designer,
                trace,
                session,
                DesignerSemanticTarget::Menus,
            )?;
            wait_appearance(child, designer, trace, session, |state| {
                !state.gallery_active
            })?;
            let after = snapshot(child, designer, trace, session)?;
            step(
                packet,
                GateSOperation::LeaveGallery,
                None,
                before,
                after,
                None,
            );
        }
        _ => {
            return Err(CaseFailure::new(
                FailureStage::Environment,
                "unknown Gate S case".into(),
            ));
        }
    }
    Ok(format!(
        "{} real owned appearance transitions; zero external execution; bounded shared renderer/gallery evidence retained",
        packet.steps.len()
    ))
}

fn before_generation_from_start(packet: &GateSCaseEvidence) -> Result<u64, CaseFailure> {
    packet
        .steps
        .first()
        .filter(|step| step.operation == GateSOperation::NativeStart)
        .map(|step| step.before.generation)
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::Environment,
                "native start generation missing".into(),
            )
        })
}

fn native_transition(
    packet: &mut GateSCaseEvidence,
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace: &Path,
    session: u64,
    kind: u8,
    expected: GateSNativeExpectation,
) -> Result<(), CaseFailure> {
    let operation = if kind == 0 {
        GateSOperation::NativeClick
    } else {
        GateSOperation::NativeNavigate
    };
    let target = native_targets(trace, session)
        .into_iter()
        .rev()
        .find(|(_, target)| {
            target.kind == kind
                && !target.clicked
                && target.generation == expected.generation
                && target.menu_digest == expected.menu
                && target.cell_digest == expected.cell
        })
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "native prepared target missing".into(),
            )
        })?
        .1;
    let presentation_deadline = Instant::now() + TRACE_TIMEOUT;
    let surfaces = wait_runtime_windows(
        child,
        &[],
        presentation_deadline.saturating_duration_since(Instant::now()),
    )
    .map_err(|error| CaseFailure::new(FailureStage::DesignerPresentation, error))?;
    let point = POINT {
        x: target.point_x,
        y: target.point_y,
    };
    let surface = radial_input_surface_at(child, &surfaces, point)
        .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
    let before = snapshot(child, designer, trace, session)?;
    let count_before = preview_dispatch_count(trace, session);
    let cursor = trace_lines(trace).len();
    let pointer = click_owned_radial_point(
        child,
        &surface,
        point,
        trace,
        cursor,
        target.generation,
        presentation_deadline,
        || {
            if native_prepared_target_is_current(
                &native_targets_in_lines(&trace_lines(trace)),
                target,
            ) {
                Ok(())
            } else {
                Err(
                    "native prepared target session, generation or geometry changed before down"
                        .into(),
                )
            }
        },
    )
    .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let deadline = Instant::now() + TRACE_TIMEOUT;
    let (sequence, clicked) = loop {
        if let Some(hit) = native_targets(trace, session)
            .into_iter()
            .rev()
            .find(|(_, hit)| {
                hit.clicked
                    && hit.cell_digest == target.cell_digest
                    && hit.generation == target.generation
            })
        {
            break hit;
        }
        if Instant::now() >= deadline {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                "native pointer did not acknowledge the prepared target".into(),
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    };
    child
        .focus_window(designer)
        .map_err(|error| CaseFailure::new(FailureStage::DesignerReadiness, error))?;
    let after = snapshot(child, designer, trace, session)?;
    let x = target.point_x - surface.bounds[0];
    let y = target.point_y - surface.bounds[1];
    let input = GateSInput {
        target: "native_cell".into(),
        index: None,
        bounds: [x, y, x + 1, y + 1],
        client_size: [
            surface.bounds[2] - surface.bounds[0],
            surface.bounds[3] - surface.bounds[1],
        ],
        session_id: session,
        generation: before.0.generation,
        trace_sequence: sequence,
        clicked: true,
        // This helper uses validated SetCursorPos, not a fabricated
        // SendInput movement count. Button/WM receipts prove input.
        inserted_move: 0,
        inserted_down: pointer.down_inserted,
        inserted_up: pointer.up_inserted,
        accepted_pointer_sequence: sequence,
    };
    step(packet, operation, None, before, after, Some(input));
    let native_step = packet
        .steps
        .last_mut()
        .ok_or_else(|| CaseFailure::new(FailureStage::Environment, "native step missing".into()))?;
    native_step.native_target = Some(clicked);
    native_step.native_expected = Some(expected);
    native_step.native_pointer = Some(pointer);
    native_step.preview_dispatch_before = count_before;
    native_step.preview_dispatch_after = preview_dispatch_count(trace, session);

    if kind != 0 {
        let deadline = Instant::now() + TRACE_TIMEOUT;
        let result = loop {
            if let Some((_, result)) =
                native_targets(trace, session)
                    .into_iter()
                    .rev()
                    .find(|(_, result)| {
                        !result.clicked
                            && result.generation == expected.generation + 1
                            && result.menu_digest == expected.result_menu
                    })
            {
                break result;
            }
            if Instant::now() >= deadline {
                return Err(CaseFailure::new(
                    FailureStage::DesignerReadiness,
                    "native navigation did not present a new prepared generation".into(),
                ));
            }
            std::thread::sleep(WINDOW_POLL);
        };
        packet
            .steps
            .last_mut()
            .ok_or_else(|| {
                CaseFailure::new(FailureStage::Environment, "native step missing".into())
            })?
            .native_result = Some(result);
    }
    Ok(())
}

fn with_relocated_designer_for_preview_stop<T>(
    relocate: impl FnOnce() -> Result<WindowSnapshot, CaseFailure>,
    stop: impl FnOnce(&WindowSnapshot) -> Result<T, CaseFailure>,
    restore: impl FnOnce() -> Result<(), CaseFailure>,
) -> Result<T, CaseFailure> {
    let result = relocate().and_then(|moved| stop(&moved));
    // A move can take effect before its measurement fails. Always restore the
    // captured exact owner after the move attempt, including a failed Stop.
    let restoration = restore();
    match (result, restoration) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(mut error)) => {
            error.message = format!(
                "native preview Stop completed; Designer restoration failed: {}",
                error.message
            );
            Err(error)
        }
        (Err(mut error), Err(restoration)) => {
            error.message = format!(
                "{}; Designer restoration failed: {restoration}",
                error.message
            );
            Err(error)
        }
    }
}

fn preview_dispatch_count(trace: &Path, session: u64) -> u64 {
    trace_lines(trace)
        .iter()
        .rev()
        .find(|line| {
            line.contains("trace_event=\"native_preview_dispatch_count\"")
                && trace_field(line, "editor_session").and_then(|v| v.parse::<u64>().ok())
                    == Some(session)
        })
        .and_then(|line| trace_field(line, "count")?.parse().ok())
        .unwrap_or(0)
}

pub fn run_gate_s_suite(
    executable: &str,
    profile: &Path,
    output: &Path,
    trace: &Path,
    hotkey: AcceptanceHotkey,
    cursor_restore: Option<POINT>,
    _desktop: &InputDesktopAttachment,
    report: &mut AcceptanceReport,
    log: &mut File,
) -> Option<FocusAnchor> {
    let mut child = None;
    let mut anchor = None;
    let mut entry = None;
    let setup = (|| {
        preflight_acceptance_hotkey(hotkey)
            .map_err(|e| CaseFailure::new(FailureStage::Environment, e))?;
        let launched = NativeChild::launch_gate_s(
            Path::new(executable),
            profile,
            trace,
            &profile.join("child.stdout.log"),
            &profile.join("child.stderr.log"),
        )
        .map_err(|error| CaseFailure::new(FailureStage::CandidateStartup, error.to_string()))?;
        report.environment.child_process_id = Some(launched.process_id());
        report.environment.child_started_unix_ms = launched
            .started()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| d.as_millis());
        child = Some(launched);
        let deadline = Instant::now() + HOTKEY_FIXTURE_STARTUP_TIMEOUT;
        while !gate_s_trace_profile_ready(&trace_lines(trace)) {
            if Instant::now() >= deadline {
                return Err(CaseFailure::new(
                    FailureStage::CandidateStartup,
                    "Gate S bounded trace profile missing".into(),
                ));
            }
            std::thread::sleep(WINDOW_POLL);
        }
        let child = child.as_mut().ok_or_else(|| {
            CaseFailure::new(FailureStage::CandidateStartup, "child missing".into())
        })?;
        wait_hotkey_fixture_ready(child, trace, HOTKEY_FIXTURE_STARTUP_TIMEOUT)
            .map_err(|e| CaseFailure::new(FailureStage::DesignerReadiness, e))?;
        anchor = Some(
            FocusAnchor::create().map_err(|e| CaseFailure::new(FailureStage::Environment, e))?,
        );
        let uia =
            UiAutomation::new().map_err(|e| CaseFailure::new(FailureStage::Environment, e))?;
        entry = Some(run_designer_entry(
            child,
            &uia,
            anchor.as_ref().ok_or_else(|| {
                CaseFailure::new(FailureStage::Environment, "anchor missing".into())
            })?,
            trace,
        )?);
        Ok::<(), CaseFailure>(())
    })();
    let mut oracle = gate_d_fixture_document(profile).map(GateSDocumentOracle::new);
    let mut failure = setup.err();
    for id in GATE_S_REQUIRED_CASE_IDS {
        let started = Instant::now();
        let mut packet = GateSCaseEvidence {
            schema_version: 2,
            case_id: id.into(),
            fixture_digest: gate_s_fixture_digest(&report.profile),
            steps: Vec::new(),
            no_action_proof: None,
        };
        let mut result = if let Some(prior) = &failure {
            Err(CaseFailure::new(
                prior.stage,
                format!("Gate S not run after earlier failure: {}", prior.message),
            ))
        } else {
            match (
                child.as_mut(),
                entry.as_mut(),
                anchor.as_ref(),
                oracle.as_ref(),
            ) {
                (Some(child), Some(entry), Some(anchor), Ok(oracle)) => {
                    let before = gate_d_no_action_state(trace, profile);
                    let result =
                        run_case(&mut packet, profile, child, entry, anchor, oracle, trace);
                    if let (Ok(before), Ok(after)) =
                        (before, gate_d_no_action_state(trace, profile))
                    {
                        packet.no_action_proof = Some(GateDNoActionProof {
                            trace_before: before.0,
                            trace_after: after.0,
                            marker_lines_before: before.1,
                            marker_lines_after: after.1,
                            marker_digest_before: before.2,
                            marker_digest_after: after.2,
                        });
                    }
                    result
                }
                _ => Err(CaseFailure::new(
                    FailureStage::DesignerEntry,
                    "Gate S owner missing".into(),
                )),
            }
        };
        if result.is_ok() {
            result = oracle
                .as_mut()
                .map_err(|error| CaseFailure::new(FailureStage::Environment, error.message.clone()))
                .and_then(|oracle| {
                    oracle
                        .correlate(&mut packet)
                        .map_err(|error| CaseFailure::new(FailureStage::GestureDecision, error))
                })
                .and(result);
        }
        if result.is_ok() && !gate_s_case_contract_is_valid(&packet) {
            result = Err(CaseFailure::new(
                FailureStage::GestureDecision,
                "typed Gate S packet did not prove its appearance/owner contract".into(),
            ));
        }
        if let Err(error) = &result {
            failure = Some(CaseFailure::new(error.stage, error.message.clone()));
        }
        report.gate_s_evidence.push(packet);
        append_case(
            report,
            id,
            expected(id),
            started,
            result,
            child.as_ref(),
            output,
            trace,
        );
    }
    restore_cursor_before_shutdown(cursor_restore, log);
    if let Some(mut child) = child {
        stop_child(&mut child, report, log, output, trace);
    } else {
        append_case(
            report,
            "CLEANUP",
            expected("CLEANUP"),
            Instant::now(),
            Err(CaseFailure::new(
                FailureStage::Cleanup,
                "Gate S child unavailable; no successful cleanup receipt".into(),
            )),
            None,
            output,
            trace,
        );
    }
    anchor
}

fn gate_s_trace_profile_ready(lines: &[String]) -> bool {
    lines.iter().any(|line| {
        line.contains("trace_event=\"trace_ready\"")
            && trace_static_enum_field(line, "trace_budget_profile") == Some("gate_s_v1")
            && trace_field(line, "event_budget") == Some("81920")
            && trace_field(line, "reserved_event_budget") == Some("256")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop_designer_fixture() -> WindowSnapshot {
        WindowSnapshot {
            hwnd: HWND(43780596usize as *mut _),
            process_id: 2796,
            role: WindowRole::Designer,
            class_name: "eframe".into(),
            visible: true,
            minimized: false,
            bounds: [26, 26, 942, 715],
        }
    }

    #[test]
    fn gate_s_stop_relocation_reacquires_current_control_once_then_restores() {
        let original = stop_designer_fixture();
        let current = std::cell::RefCell::new(original.clone());
        let calls = std::cell::RefCell::new(Vec::new());
        let position =
            designer_clear_position(original.bounds, &[[0, 0, 515, 515]], &[[0, 0, 2560, 1440]])
                .unwrap();
        let receipt = with_relocated_designer_for_preview_stop(
            || {
                calls.borrow_mut().push("move");
                let mut moved = original.clone();
                moved.bounds = [
                    position[0],
                    position[1],
                    position[0] + 916,
                    position[1] + 689,
                ];
                *current.borrow_mut() = moved.clone();
                Ok(moved)
            },
            |moved| {
                assert_eq!(moved.hwnd, original.hwnd);
                assert_eq!(moved.bounds, current.borrow().bounds);
                assert_ne!(moved.bounds, original.bounds);
                calls
                    .borrow_mut()
                    .extend(["repaint", "measure_client", "fresh_stop_control"]);
                // Retained Stop [159,59,286,77] is reacquired against the
                // moved client origin, not the old covered point (257,125).
                let stop_point = [moved.bounds[0] + 8 + 223, moved.bounds[1] + 31 + 68];
                assert!(stop_point[0] > 515);
                calls
                    .borrow_mut()
                    .extend(["one_down", "one_up", "actual_stop_terminal"]);
                Ok((1, 1))
            },
            || {
                calls.borrow_mut().push("restore");
                assert_eq!(current.borrow().hwnd, original.hwnd);
                *current.borrow_mut() = original.clone();
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(receipt, (1, 1));
        assert_eq!(current.borrow().bounds, original.bounds);
        assert_eq!(
            *calls.borrow(),
            [
                "move",
                "repaint",
                "measure_client",
                "fresh_stop_control",
                "one_down",
                "one_up",
                "actual_stop_terminal",
                "restore"
            ]
        );
    }

    #[test]
    fn gate_s_stop_relocation_restores_after_move_readiness_input_or_terminal_failure() {
        for failure in 0..6 {
            let original = stop_designer_fixture();
            let current = std::cell::RefCell::new(original.clone());
            let stop_calls = std::cell::Cell::new(0);
            let restore_calls = std::cell::Cell::new(0);
            let result = with_relocated_designer_for_preview_stop(
                || {
                    // A successful native move followed by failed measurement
                    // still has to restore the captured owner before returning.
                    current.borrow_mut().bounds = [1644, 0, 2560, 689];
                    if failure == 0 {
                        Err(CaseFailure::new(
                            FailureStage::DesignerPresentation,
                            "move measurement failed".into(),
                        ))
                    } else {
                        Ok(current.borrow().clone())
                    }
                },
                |_| {
                    stop_calls.set(stop_calls.get() + 1);
                    match failure {
                        1 => Err(CaseFailure::new(
                            FailureStage::DesignerReadiness,
                            "fresh control missing".into(),
                        )),
                        2 => Err(CaseFailure::new(
                            FailureStage::InputInjection,
                            "coverage rejected before down".into(),
                        )),
                        3 | 5 => Err(CaseFailure::new(
                            FailureStage::DesignerReadiness,
                            "actual Stop terminal missing".into(),
                        )),
                        _ => Ok(()),
                    }
                },
                || {
                    restore_calls.set(restore_calls.get() + 1);
                    if failure >= 4 {
                        Err(CaseFailure::new(
                            FailureStage::DesignerPresentation,
                            "restoration measurement failed".into(),
                        ))
                    } else {
                        *current.borrow_mut() = original.clone();
                        Ok(())
                    }
                },
            );
            let error = result.unwrap_err();
            assert_eq!(restore_calls.get(), 1);
            assert_eq!(stop_calls.get(), usize::from(failure != 0));
            if failure < 4 {
                assert_eq!(current.borrow().bounds, original.bounds);
            }
            match failure {
                0 => assert!(error.message.contains("move measurement failed")),
                1 => assert!(error.message.contains("fresh control missing")),
                2 => assert_eq!(error.stage, FailureStage::InputInjection),
                3 => assert!(error.message.contains("actual Stop terminal missing")),
                4 => assert!(
                    error
                        .message
                        .contains("Stop completed; Designer restoration failed")
                ),
                _ => assert!(
                    error.message.contains("actual Stop terminal missing")
                        && error.message.contains("restoration measurement failed")
                ),
            }
        }
    }

    fn native_target_line(sequence: u64, target: NativeAppearanceTarget) -> String {
        format!(
            "trace_event=\"native_appearance_target\" trace_sequence={sequence} session_id={} generation={} menu_digest={} cell_digest={} geometry_digest={} point_x={} point_y={} dpi_milli={} cell_kind={} clicked={} work_left={} work_top={} work_right={} work_bottom={}",
            target.session_id,
            target.generation,
            target.menu_digest,
            target.cell_digest,
            target.geometry_digest,
            target.point_x,
            target.point_y,
            target.dpi_milli,
            target.kind,
            target.clicked,
            target.work_area[0],
            target.work_area[1],
            target.work_area[2],
            target.work_area[3],
        )
    }

    #[test]
    fn gate_s_native_target_readiness_keeps_exact_current_prepared_owner() {
        let back = NativeAppearanceTarget {
            session_id: 1,
            generation: 12,
            menu_digest: 4556281056013730015,
            cell_digest: 4644805793077333250,
            geometry_digest: 9221716653986904436,
            point_x: 257,
            point_y: 257,
            dpi_milli: 1000,
            work_area: [0, 0, 2560, 1440],
            kind: 2,
            clicked: false,
        };
        let mut action = back;
        action.cell_digest = 99;
        action.kind = 0;
        action.point_x = 310;
        let lines = vec![
            native_target_line(16296, back),
            native_target_line(16470, action),
        ];
        let parsed = native_targets_in_lines(&lines);
        assert_eq!(parsed, [(16296, back), (16470, action)]);
        assert!(native_prepared_target_is_current(&parsed, back));
        assert!(native_prepared_target_is_current(&parsed, action));
        for invalid in 0..6 {
            let mut newer = action;
            match invalid {
                0 => newer.session_id += 1,
                1 => newer.generation += 1,
                2 => newer.menu_digest += 1,
                3 => newer.geometry_digest += 1,
                4 => newer.dpi_milli += 1,
                _ => newer.work_area[0] -= 1,
            }
            let changed =
                native_targets_in_lines(&[lines[0].clone(), native_target_line(16471, newer)]);
            assert!(
                !native_prepared_target_is_current(&changed, back),
                "{invalid}"
            );
        }
        let mut moved = back;
        moved.point_x += 1;
        assert!(!native_prepared_target_is_current(&parsed, moved));
        let mut clicked = back;
        clicked.clicked = true;
        assert!(!native_prepared_target_is_current(
            &[(16471, clicked)],
            back
        ));
        assert!(!native_prepared_target_is_current(&[], back));
    }

    fn producer_menu_row_lines(
        sequence: u64,
        frame: u64,
        top: i32,
        index: usize,
        clicked: bool,
    ) -> [String; 2] {
        let bottom = top + 18;
        let right = if index == 11 { 137 } else { 128 };
        [
            format!(
                "trace_event=\"designer_authoring_control\" target=MenuRow role=\"Selectable\" viewport=Deferred control_index={index} left_px=8 top_px={top} right_px={right} bottom_px={bottom} clip_left_px=0 clip_top_px=185 clip_right_px=900 clip_bottom_px=638 client_width_px=900 client_height_px=650 trace_sequence={sequence} enabled=true selected={clicked} focused=false clicked={clicked} session_id=1 generation=9 frame_nr={frame} menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1"
            ),
            format!(
                "trace_event=\"designer_authoring_scroll_viewport\" viewport=Deferred scroll_owner=\"MenuTree\" scroll_id=61 frame_nr={frame} input_left_px=3 input_top_px=188 input_right_px=896 input_bottom_px=635 clip_left_px=0 clip_top_px=185 clip_right_px=900 clip_bottom_px=638 client_width_px=900 client_height_px=650 trace_sequence={} session_id=1 generation=9",
                sequence + 1
            ),
        ]
    }

    fn producer_menu_row(
        sequence: u64,
        frame: u64,
        top: i32,
        index: usize,
        clicked: bool,
    ) -> AuthoringControlSnapshot {
        let lines = producer_menu_row_lines(sequence, frame, top, index, clicked);
        latest_authoring_controls_after(&lines, 0, 1).pop().unwrap()
    }

    fn moving_click_owner(sequence: u64) -> GateDObservationEvidence {
        let mut owner = crate::tests::gate_d_test_observation(9, 901, &[11], 11, 11, 5, 0, true);
        owner.session_id = 1;
        owner.trace_sequence = sequence;
        owner
    }

    #[test]
    fn gate_s_click_partial_viewport_waits_for_same_frame_without_geometry_reacquisition() {
        use crate::native::tests::cadence_menu_row_lines;

        // Keep candidate6's actual input owner and cadence; y596 is the retained
        // candidate5 visible row geometry used by the pre-down click fixture.
        let expected = latest_authoring_controls_after(
            &cadence_menu_row_lines(13894, 1274, 596, 126930),
            0,
            1,
        )
        .pop()
        .unwrap();
        let mut lines = cadence_menu_row_lines(13945, 1279, 596, 127464).to_vec();
        let partial = latest_authoring_controls_after(&lines[..1], 0, 1)
            .pop()
            .unwrap();
        lines.push(
            lines[1]
                .replace("frame_nr=1279", "frame_nr=1280")
                .replace("13947", "13952"),
        );
        let complete = latest_authoring_controls_after(&lines, 0, 1).pop().unwrap();
        assert!(partial.scroll_viewport.is_none());
        assert!(complete.scroll_viewport.is_some());
        assert_eq!(partial.trace_sequence, complete.trace_sequence);
        assert_eq!(partial.frame_nr, complete.frame_nr);
        assert_eq!(complete.scroll_viewport.unwrap().trace_sequence, 13947);
        assert_eq!(complete.scroll_viewport.unwrap().measured.frame_nr, 1279);
        let before = moving_click_owner(13890);
        let preparations = Cell::new(0);
        let attempts = Cell::new(0);
        let polls = Cell::new(0);
        let downs = Cell::new(0);
        let ups = Cell::new(0);
        let mut samples = [partial, complete].into_iter();
        let control = click_with_geometry_reacquisition(
            UIA_TIMEOUT,
            |_| {
                preparations.set(preparations.get() + 1);
                Ok(expected)
            },
            |expected, remaining| {
                attempts.set(attempts.get() + 1);
                let prepared = RefCell::new(None);
                dispatch_pointer_down_after_preflight(
                    || {
                        let current = gate_d_wait_control_before_pointer_down_with(
                            &expected,
                            &before,
                            [68, 605],
                            remaining,
                            || {
                                polls.set(polls.get() + 1);
                                Ok((
                                    expected.client_size,
                                    vec![samples.next().unwrap()],
                                    moving_click_owner(13953),
                                ))
                            },
                            |_| {},
                        )?;
                        *prepared.borrow_mut() = Some(current);
                        Ok(())
                    },
                    || {
                        downs.set(downs.get() + 1);
                        Ok(())
                    },
                )?;
                ups.set(ups.get() + 1);
                Ok(prepared.into_inner().unwrap())
            },
        )
        .unwrap();
        assert_eq!(control, complete);
        assert_eq!((preparations.get(), attempts.get(), polls.get()), (1, 1, 2));
        assert_eq!((downs.get(), ups.get()), (1, 1));
    }

    #[test]
    fn gate_s_click_reacquires_actual_moving_row_without_down_then_clicks_one_exact_receipt() {
        let original = producer_menu_row(7057, 717, 607, 10, false);
        let moved = producer_menu_row(7072, 718, 596, 10, false);
        let settled = producer_menu_row(7080, 719, 596, 10, false);
        let fresh = producer_menu_row(7085, 720, 596, 10, false);
        let wrong_row_receipt = producer_menu_row(7099, 723, 617, 11, true);
        let correct_receipt = producer_menu_row(7100, 724, 596, 10, true);
        let before = moving_click_owner(7050);
        let preparations = Cell::new(0);
        let moves = Cell::new(0);
        let downs = Cell::new(0);
        let ups = Cell::new(0);
        let receipt = click_with_geometry_reacquisition(
            UIA_TIMEOUT,
            |_| {
                preparations.set(preparations.get() + 1);
                Ok(if preparations.get() == 1 {
                    original
                } else {
                    settled
                })
            },
            |expected, _| {
                moves.set(moves.get() + 1);
                let current = if moves.get() == 1 { moved } else { fresh };
                let point = [
                    expected.bounds[0] + (expected.bounds[2] - expected.bounds[0]) / 2,
                    expected.bounds[1] + (expected.bounds[3] - expected.bounds[1]) / 2,
                ];
                let prepared = RefCell::new(None);
                dispatch_pointer_down_after_preflight(
                    || {
                        let control = gate_d_control_pre_down_status(
                            &[current],
                            &expected,
                            &before,
                            &moving_click_owner(current.trace_sequence + 2),
                            expected.client_size,
                            point,
                        )?
                        .ok_or_else(|| {
                            PointerClickPreDownError::StaleGeometry("missing fresh frame".into())
                        })?;
                        *prepared.borrow_mut() = Some(control);
                        Ok(())
                    },
                    || {
                        downs.set(downs.get() + 1);
                        Ok(())
                    },
                )?;
                ups.set(ups.get() + 1);
                let prepared = prepared.into_inner().unwrap();
                assert_eq!(prepared, fresh);
                assert!(
                    authoring_control_click_receipt(
                        &[wrong_row_receipt],
                        &prepared,
                        AuthoringControlClickCompletion::CapturedClick,
                    )
                    .unwrap()
                    .is_none()
                );
                let receipt = authoring_control_click_receipt(
                    &[wrong_row_receipt, correct_receipt],
                    &prepared,
                    AuthoringControlClickCompletion::CapturedClick,
                )
                .unwrap()
                .unwrap();
                assert!(click_receipt_matches_prepared(&receipt, &prepared));
                Ok(receipt)
            },
        )
        .unwrap();
        assert_eq!((preparations.get(), moves.get()), (2, 2));
        assert_eq!((downs.get(), ups.get()), (1, 1));
        assert_eq!(receipt, correct_receipt);
        assert_eq!(receipt.index, Some(10));
        assert_eq!(receipt.bounds, [8, 596, 128, 614]);
        let corruptions: &[fn(&mut AuthoringControlSnapshot)] = &[
            |receipt| receipt.index = Some(11),
            |receipt| receipt.role = AuthoringControlRole::Button,
            |receipt| receipt.session_id += 1,
            |receipt| receipt.generation += 1,
            |receipt| receipt.trace_sequence = 7085,
            |receipt| receipt.bounds[1] += 1,
            |receipt| receipt.clip_bounds = None,
            |receipt| receipt.client_size[0] += 1,
            |receipt| receipt.frame_nr = None,
            |receipt| receipt.frame_nr = Some(719),
            |receipt| receipt.enabled = false,
            |receipt| receipt.clicked = false,
        ];
        for corrupt in corruptions {
            let mut bad = receipt;
            corrupt(&mut bad);
            assert!(!click_receipt_matches_prepared(&bad, &fresh), "{bad:?}");
        }
    }

    #[test]
    fn gate_s_click_reacquisition_budget_and_deadline_never_send_down_on_stale_geometry() {
        let control = producer_menu_row(7057, 717, 607, 10, false);
        let preparations = Cell::new(0);
        let attempts = Cell::new(0);
        let downs = Cell::new(0);
        let result = click_with_geometry_reacquisition(
            UIA_TIMEOUT,
            |_| {
                preparations.set(preparations.get() + 1);
                Ok(control)
            },
            |_, _| {
                attempts.set(attempts.get() + 1);
                dispatch_pointer_down_after_preflight(
                    || {
                        Err(PointerClickPreDownError::StaleGeometry(
                            "moving frame".into(),
                        ))
                    },
                    || {
                        downs.set(downs.get() + 1);
                        Ok(())
                    },
                )
            },
        );
        assert!(result.is_err());
        assert_eq!(preparations.get(), MAX_PRE_DOWN_GEOMETRY_REACQUISITIONS);
        assert_eq!(attempts.get(), MAX_PRE_DOWN_GEOMETRY_REACQUISITIONS);
        assert_eq!(downs.get(), 0);
        let result = click_with_geometry_reacquisition(
            Duration::ZERO,
            |_| {
                preparations.set(preparations.get() + 1);
                Ok(control)
            },
            |_, _| {
                downs.set(downs.get() + 1);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(preparations.get(), MAX_PRE_DOWN_GEOMETRY_REACQUISITIONS);
        assert_eq!(downs.get(), 0);
    }

    #[test]
    fn gate_s_click_never_retries_an_input_failure_before_or_after_button_down() {
        let control = producer_menu_row(7057, 717, 607, 10, false);
        for send_down in [false, true] {
            let preparations = Cell::new(0);
            let attempts = Cell::new(0);
            let downs = Cell::new(0);
            let result: Result<(), CaseFailure> = click_with_geometry_reacquisition(
                UIA_TIMEOUT,
                |_| {
                    preparations.set(preparations.get() + 1);
                    Ok(control)
                },
                |_, _| {
                    attempts.set(attempts.get() + 1);
                    if send_down {
                        dispatch_pointer_down_after_preflight(
                            || Ok(()),
                            || {
                                downs.set(downs.get() + 1);
                                Ok(())
                            },
                        )?;
                    }
                    Err(PointerClickPreDownError::Input(
                        "native input/receipt failed".into(),
                    ))
                },
            );
            assert!(result.is_err());
            assert_eq!((preparations.get(), attempts.get()), (1, 1));
            assert_eq!(downs.get(), usize::from(send_down));
        }
    }

    #[test]
    fn parser_and_allowlisted_excerpt_preserve_typed_appearance_facts_and_reject_missing_numeric_fields()
     {
        let state = AppearanceObservation {
            trace_sequence: 20,
            session_id: 4,
            generation: 7,
            catalog_count: 5,
            current_builtin: 0,
            preview_builtin: -1,
            visible_pending: 0,
            geometry_digest: 88,
            ..Default::default()
        };
        let mut line = "trace_event=\"appearance_state\" trace_sequence=20".to_owned();
        for (key, value) in serde_json::to_value(state).unwrap().as_object().unwrap() {
            line.push_str(&format!(" {key}={value}"));
        }
        assert_eq!(parse_appearance(&line), Some(state));
        assert!(parse_appearance(&line.replace(" geometry_digest=88", "")).is_none());
        let sanitized = safe_trace_excerpt(&format!(
            "{line} command=secret path=private label=private geometry_digest=private"
        ));
        assert!(sanitized.contains("geometry_digest=88"));
        assert!(sanitized.contains("visible_pending=0"));
        assert!(!sanitized.contains("private") && !sanitized.contains("secret"));
        let ready = "trace_event=\"trace_ready\" trace_budget_profile=\"gate_s_v1\" event_budget=81920 reserved_event_budget=256";
        assert!(gate_s_trace_profile_ready(&[ready.into()]));
        for invalid in [
            ready.replace("gate_s_v1", "gate-s-v1"),
            ready.replace("81920", "8192"),
            ready.replace("256", "0"),
        ] {
            assert!(!gate_s_trace_profile_ready(&[invalid]));
        }
    }
}
