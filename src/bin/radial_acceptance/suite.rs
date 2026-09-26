use super::super::{
    ACCEPTANCE_TARGET_ACTION_INDEX, AcceptanceCaseResult, AcceptanceHotkey, AcceptanceReport,
    AcceptanceSuite, CASE_IDS, CaseStatus, FailureStage, H04CompletedBurstEvidence,
    H04InputContaminationArtifact, H6RepeatMode, HOTKEY_CASE_IDS, HotkeyActivationEdge,
    HotkeyCandidateEventEvidence, HotkeyCandidateStream, HotkeyCaseEvidence, HotkeyDecisionProof,
    HotkeyEdgeTransition, HotkeyEvidenceNotApplicable, HotkeyFollowOnRestoreEvidence,
    HotkeyGestureEvidence, HotkeyInputProvenance, HotkeyNativeActivationSpan,
    HotkeyObservedPresentation, HotkeyRadialActionStage, HotkeyRootCommand, HotkeyRootCommandSpan,
    HotkeyRootFocusIntent, HotkeyRootIdentityEvidence, HotkeyRunnerEdgeEvidence,
    HotkeyRunnerInputPurpose, HotkeyStandaloneDecisionEvidence, HotkeyTraceEventKind,
    HotkeyVisibilitySource, MAX_HOTKEY_CASE_EVIDENCE_BYTES, MAX_HOTKEY_EVIDENCE_EDGES,
    MAX_HOTKEY_EVIDENCE_EVENTS, MAX_HOTKEY_EVIDENCE_GESTURES, MAX_PATH_BYTES, MAX_RESULT_BYTES,
    format_h04_matrix_evidence, hotkey_expected_state, sha256_bytes,
    validate_h04_contamination_artifact, validate_hotkey_burst_trace_with_baseline,
};
use super::*;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TAP_TIME: Duration = Duration::from_millis(135);
const ROOT_TIMEOUT: Duration = Duration::from_secs(3);
const UIA_TIMEOUT: Duration = Duration::from_secs(5);
const TRACE_TIMEOUT: Duration = Duration::from_secs(3);
const HOTKEY_FIXTURE_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const HOTKEY_FIXTURE_STABLE_SAMPLES: u8 = 3;
const MAX_TRACE_EXCERPT: usize = 512;
const MAX_TRACE_BYTES: usize = 128 * 1024;
const STARTUP_TRACE_EVENTS: usize = 32;
const STARTUP_TRACE_BYTES: usize = 16 * 1024;
const MAX_PRIVATE_LOG_BYTES: usize = 64 * 1024;
const MAX_KEYBOARD_FOCUS_STEPS: usize = 32;
const POST_G2_ROOT_STABILITY_WINDOW: Duration = Duration::from_millis(750);
const POST_G2_ROOT_STABLE_SAMPLES: u8 = 3;
const HOTKEY_TRACE_QUIET_WINDOW: Duration = Duration::from_millis(100);
const HOTKEY_TRACE_DRAIN_TIMEOUT: Duration = Duration::from_millis(1_500);
const DESIGNER_TEXT_PROBE: &str = "Native Edit Probe";
const DESIGNER_STARTER_NAME: &str = "Starter";
static NEXT_HOOK_PUMP_PROBE_ID: AtomicU64 = AtomicU64::new(1);
const MAX_HOTKEY_CAPTURE_SEGMENTS: usize = 32;
const DEFERRED_REPORT_CASE_IDS: [&str; 3] = ["R0", "R1", "R2"];
const BLOCKED_DESIGNER_CASE_IDS: [&str; 20] = [
    "H3", "D1", "D2", "D4", "D5", "A0", "A1", "G0", "A2", "G1", "G2", "A3", "A4", "A5", "A6", "A7",
    "A8", "D3", "D6", "D7",
];

struct HotkeyCaptureSegment {
    stream: HotkeyCandidateStream,
    path: PathBuf,
    cursor: usize,
    end: Option<usize>,
    materialized_events: Option<Vec<HotkeyCandidateEventEvidence>>,
    input_group_id: u32,
    purpose: HotkeyRunnerInputPurpose,
    root_hwnd: u64,
    root_process_id: u32,
}

#[derive(Clone)]
struct HotkeySnapshotWaitContext {
    cursor: usize,
    stream: HotkeyCandidateStream,
    input_group_id: u32,
    purpose: HotkeyRunnerInputPurpose,
    root_hwnd: u64,
    root_process_id: u32,
    physical_displays: Vec<[i32; 4]>,
}

struct ActiveHotkeyEvidenceCapture {
    case_id: String,
    segments: Vec<HotkeyCaptureSegment>,
    stream_overrides: Vec<(PathBuf, HotkeyCandidateStream)>,
    runner_edges: Vec<HotkeyRunnerEdgeEvidence>,
    first_runner_edge: Option<Instant>,
    runner_edge_overflow: bool,
    capture_segment_overflow: bool,
    candidate_trace_overflow: bool,
    physical_displays: Vec<[i32; 4]>,
    next_input_group_id: u32,
    current_purpose: Option<HotkeyRunnerInputPurpose>,
}

thread_local! {
    static ACTIVE_HOTKEY_EVIDENCE_CAPTURE: RefCell<Option<ActiveHotkeyEvidenceCapture>> = const { RefCell::new(None) };
}

fn begin_hotkey_evidence_capture(case_id: &str, _main_trace: &Path) {
    let capture = ActiveHotkeyEvidenceCapture {
        case_id: case_id.to_owned(),
        segments: Vec::new(),
        stream_overrides: Vec::new(),
        runner_edges: Vec::new(),
        first_runner_edge: None,
        runner_edge_overflow: false,
        capture_segment_overflow: false,
        candidate_trace_overflow: false,
        physical_displays: native_display_bounds().unwrap_or_default(),
        next_input_group_id: 1,
        current_purpose: None,
    };
    ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| {
        *slot.borrow_mut() = Some(capture);
    });
}

fn register_hotkey_candidate_stream(path: &Path, stream: HotkeyCandidateStream) {
    ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| {
        let mut capture = slot.borrow_mut();
        let Some(capture) = capture.as_mut() else {
            return;
        };
        let path = path.to_path_buf();
        if !capture
            .stream_overrides
            .iter()
            .any(|(existing, _)| existing == &path)
        {
            capture.stream_overrides.push((path, stream));
        }
    });
}

fn set_hotkey_capture_purpose(purpose: HotkeyRunnerInputPurpose) {
    ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| {
        if let Some(capture) = slot.borrow_mut().as_mut() {
            capture.current_purpose = Some(purpose);
        }
    });
}

fn h04_matrix_capture_active() -> bool {
    ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| {
        slot.borrow().as_ref().is_some_and(|capture| {
            capture.case_id == "H04"
                && capture.current_purpose == Some(HotkeyRunnerInputPurpose::MatrixBurst)
        })
    })
}

fn capture_hotkey_runner_edges(
    observation: &RunnerChordObservation,
    stream: HotkeyCandidateStream,
    input_group_id: u32,
    purpose: HotkeyRunnerInputPurpose,
) {
    ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| {
        let mut capture = slot.borrow_mut();
        let Some(capture) = capture.as_mut() else {
            return;
        };
        let mut observed_edges = observation
            .ordered_edges
            .iter()
            .chain(observation.foreign_edges.iter())
            .collect::<Vec<_>>();
        observed_edges.sort_by_key(|edge| edge.at);
        for edge in observed_edges {
            if capture.runner_edges.len() >= MAX_HOTKEY_EVIDENCE_EDGES {
                capture.runner_edge_overflow = true;
                break;
            }
            let first = *capture.first_runner_edge.get_or_insert(edge.at);
            let elapsed = edge
                .at
                .checked_duration_since(first)
                .unwrap_or_default()
                .as_micros();
            capture.runner_edges.push(HotkeyRunnerEdgeEvidence {
                runner_relative_us: u64::try_from(elapsed).unwrap_or(u64::MAX),
                input_group_id,
                stream,
                purpose,
                virtual_key: edge.vk,
                transition: if edge.down {
                    HotkeyEdgeTransition::Press
                } else {
                    HotkeyEdgeTransition::Release
                },
                injected: edge.injected,
                runner_cookie_matched: edge.extra_info == ACCEPTANCE_RUNNER_INPUT_COOKIE,
            });
        }
    });
}

fn capture_hotkey_attempt_evidence(
    child: &NativeChild,
    default_stream: HotkeyCandidateStream,
    trace_path: &Path,
    trace_cursor: usize,
    observation: &RunnerChordObservation,
    purpose: HotkeyRunnerInputPurpose,
) -> u32 {
    let root = child.root();
    let root_hwnd = hwnd_id(root.hwnd);
    let root_process_id = child.process_id();
    let (stream, group_id, purpose) = ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(capture) = slot.as_mut() else {
            return (default_stream, 0, purpose);
        };
        open_hotkey_capture_segment(
            capture,
            default_stream,
            trace_path,
            trace_cursor,
            purpose,
            root_hwnd,
            root_process_id,
        )
    });
    capture_hotkey_runner_edges(observation, stream, group_id, purpose);
    group_id
}

fn open_hotkey_capture_segment(
    capture: &mut ActiveHotkeyEvidenceCapture,
    default_stream: HotkeyCandidateStream,
    trace_path: &Path,
    trace_cursor: usize,
    default_purpose: HotkeyRunnerInputPurpose,
    root_hwnd: u64,
    root_process_id: u32,
) -> (HotkeyCandidateStream, u32, HotkeyRunnerInputPurpose) {
    let stream = capture
        .stream_overrides
        .iter()
        .find(|(path, _)| path == trace_path)
        .map(|(_, stream)| *stream)
        .unwrap_or(default_stream);
    let purpose = capture.current_purpose.unwrap_or(default_purpose);
    let group_id = capture.next_input_group_id;
    capture.next_input_group_id = capture.next_input_group_id.saturating_add(1);

    if let Some(previous) = capture
        .segments
        .iter_mut()
        .rev()
        .find(|segment| segment.path == trace_path && segment.end.is_none())
    {
        // A new injected group must never extend the prior group through setup
        // work. If its caller omitted terminal fencing, preserve only the
        // known-safe empty interval and fail the packet closed.
        previous.end = Some(previous.cursor);
        capture.capture_segment_overflow = true;
    }
    if capture.segments.len() >= MAX_HOTKEY_CAPTURE_SEGMENTS {
        capture.capture_segment_overflow = true;
    } else {
        capture.segments.push(HotkeyCaptureSegment {
            stream,
            path: trace_path.to_path_buf(),
            cursor: trace_cursor,
            end: None,
            materialized_events: None,
            input_group_id: group_id,
            purpose,
            root_hwnd,
            root_process_id,
        });
    }
    (stream, group_id, purpose)
}

fn complete_hotkey_capture_segment(
    trace_path: &Path,
    terminal_cursor: usize,
) -> Result<(), String> {
    ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(capture) = slot.as_mut() else {
            return Ok(());
        };
        complete_hotkey_capture_segment_in(capture, trace_path, terminal_cursor)
    })
}

fn complete_hotkey_capture_segment_in(
    capture: &mut ActiveHotkeyEvidenceCapture,
    trace_path: &Path,
    terminal_cursor: usize,
) -> Result<(), String> {
    let Some(segment_index) = capture
        .segments
        .iter()
        .rev()
        .position(|segment| segment.path == trace_path && segment.end.is_none())
    else {
        return Err(format!(
            "no open hotkey evidence segment for {}",
            trace_path.display()
        ));
    };
    let segment_index = capture.segments.len() - 1 - segment_index;
    let (cursor, stream, input_group_id, purpose) = {
        let segment = &capture.segments[segment_index];
        (
            segment.cursor,
            segment.stream,
            segment.input_group_id,
            segment.purpose,
        )
    };
    if terminal_cursor < cursor {
        capture.capture_segment_overflow = true;
        return Err(format!(
            "hotkey evidence terminal cursor {terminal_cursor} precedes segment cursor {cursor}"
        ));
    }

    let lines = trace_lines(trace_path);
    if terminal_cursor > lines.len() || cursor > lines.len() {
        capture.candidate_trace_overflow = true;
    }
    let bounded_end = terminal_cursor.min(lines.len());
    let bounded_start = cursor.min(bounded_end);
    let already_materialized = capture
        .segments
        .iter()
        .filter_map(|segment| segment.materialized_events.as_ref())
        .map(Vec::len)
        .sum::<usize>();
    let mut remaining = MAX_HOTKEY_EVIDENCE_EVENTS.saturating_sub(already_materialized);
    let mut events = Vec::new();
    for ordinal in bounded_start..bounded_end {
        if let Some(event) = parse_hotkey_candidate_event(
            &lines[ordinal],
            stream,
            input_group_id,
            purpose,
            ordinal.saturating_add(1),
        ) {
            if remaining == 0 {
                capture.candidate_trace_overflow = true;
                break;
            }
            remaining -= 1;
            events.push(event);
        }
    }
    if bounded_start != cursor || bounded_end != terminal_cursor {
        capture.candidate_trace_overflow = true;
    }
    let segment = &mut capture.segments[segment_index];
    segment.end = Some(terminal_cursor);
    segment.materialized_events = Some(events);
    Ok(())
}

fn complete_hotkey_capture_at_current_trace(trace_path: &Path) -> Result<(), CaseFailure> {
    wait_for_hotkey_capture_snapshots(trace_path).map_err(|error| {
        CaseFailure::new(
            FailureStage::RootCommand,
            format!("could not settle measured hotkey ROOT presentation evidence: {error}"),
        )
    })?;
    let terminal_cursor = trace_lines(trace_path).len();
    complete_hotkey_capture_segment(trace_path, terminal_cursor).map_err(|error| {
        CaseFailure::new(
            FailureStage::GestureDecision,
            format!("could not fence measured hotkey trace segment: {error}"),
        )
    })
}

fn wait_for_hotkey_capture_snapshots(trace_path: &Path) -> Result<(), String> {
    let context = ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| {
        let capture = slot.borrow();
        let capture = capture.as_ref()?;
        let segment = capture
            .segments
            .iter()
            .rev()
            .find(|segment| segment.path == trace_path && segment.end.is_none())?;
        Some(HotkeySnapshotWaitContext {
            cursor: segment.cursor,
            stream: segment.stream,
            input_group_id: segment.input_group_id,
            purpose: segment.purpose,
            root_hwnd: segment.root_hwnd,
            root_process_id: segment.root_process_id,
            physical_displays: capture.physical_displays.clone(),
        })
    });
    let Some(context) = context else {
        return Ok(());
    };

    let mut last_events = Vec::new();
    let settled =
        wait_for_hotkey_snapshot_proof(&context, HOTKEY_TRACE_DRAIN_TIMEOUT, WINDOW_POLL, || {
            last_events = hotkey_candidate_events_for_open_segment(trace_path, &context);
            last_events.clone()
        });
    if settled {
        Ok(())
    } else {
        let taps = last_events
            .iter()
            .filter(|event| {
                event.stream == context.stream
                    && event.input_group_id == context.input_group_id
                    && event.kind == HotkeyTraceEventKind::ShortTap
            })
            .count();
        Err(format!(
            "timed out waiting for a same-stream ROOT command and physical snapshot correlated to all applied short taps (group={}, taps={}, root_hwnd={}, root_pid={})",
            context.input_group_id, taps, context.root_hwnd, context.root_process_id
        ))
    }
}

fn hotkey_candidate_events_for_open_segment(
    trace_path: &Path,
    context: &HotkeySnapshotWaitContext,
) -> Vec<HotkeyCandidateEventEvidence> {
    trace_lines(trace_path)
        .into_iter()
        .skip(context.cursor)
        .enumerate()
        .filter_map(|(offset, line)| {
            parse_hotkey_candidate_event(
                &line,
                context.stream,
                context.input_group_id,
                context.purpose,
                context.cursor.saturating_add(offset).saturating_add(1),
            )
        })
        .collect()
}

fn wait_for_hotkey_snapshot_proof(
    context: &HotkeySnapshotWaitContext,
    timeout: Duration,
    poll_interval: Duration,
    mut read_events: impl FnMut() -> Vec<HotkeyCandidateEventEvidence>,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if hotkey_segment_has_physical_snapshot_proof(context, &read_events()) {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        std::thread::sleep(poll_interval.min(deadline.saturating_duration_since(now)));
    }
}

fn hotkey_segment_has_physical_snapshot_proof(
    context: &HotkeySnapshotWaitContext,
    events: &[HotkeyCandidateEventEvidence],
) -> bool {
    let taps = events
        .iter()
        .filter(|event| {
            event.stream == context.stream
                && event.input_group_id == context.input_group_id
                && event.kind == HotkeyTraceEventKind::ShortTap
        })
        .collect::<Vec<_>>();
    if !taps.is_empty()
        && (context.root_hwnd == 0
            || context.root_process_id == 0
            || context.physical_displays.is_empty())
    {
        return false;
    }
    let invocations_with_visibility_work = events
        .iter()
        .filter(|event| {
            event.stream == context.stream
                && event.input_group_id == context.input_group_id
                && event.kind == HotkeyTraceEventKind::VisibilityIntent
                && event.visibility_source == Some(HotkeyVisibilitySource::ToggleBatch)
                && event.invocation_id.is_some()
        })
        .filter_map(|event| event.invocation_id)
        .collect::<BTreeSet<_>>();
    if invocations_with_visibility_work
        .iter()
        .any(|invocation_id| {
            !taps
                .iter()
                .any(|tap| tap.invocation_id == Some(*invocation_id))
        })
    {
        return false;
    }

    taps.iter().all(|tap| {
        if tap.terminal != Some(true) {
            return false;
        }
        let Some(invocation_id) = tap.invocation_id else {
            return false;
        };
        let intents = events
            .iter()
            .filter(|event| {
                event.stream == context.stream
                    && event.input_group_id == context.input_group_id
                    && event.kind == HotkeyTraceEventKind::VisibilityIntent
                    && event.visibility_source == Some(HotkeyVisibilitySource::ToggleBatch)
                    && event.invocation_id == Some(invocation_id)
                    && event.event_ordinal > tap.event_ordinal
                    && event.elapsed_ms >= tap.elapsed_ms
            })
            .collect::<Vec<_>>();
        if intents.len() != 1 {
            return false;
        }
        let intent = intents[0];
        let (Some(revision), Some(visible)) = (intent.visibility_revision, intent.visible) else {
            return false;
        };
        let commands = events
            .iter()
            .filter(|event| {
                event.stream == context.stream
                    && event.input_group_id == context.input_group_id
                    && event.kind == HotkeyTraceEventKind::RootCommand
                    && event.visibility_revision == Some(revision)
                    && event.invocation_id == Some(invocation_id)
                    && event.elapsed_ms >= intent.elapsed_ms
                    && event.event_ordinal > intent.event_ordinal
                    && event.request_id.is_some()
            })
            .collect::<Vec<_>>();
        if commands.is_empty() {
            return hotkey_has_terminal_successor_snapshot(
                context,
                events,
                intent,
                revision,
                invocation_id,
            );
        }
        commands.iter().any(|command| {
            hotkey_command_has_target_snapshot(
                context,
                events,
                command,
                revision,
                invocation_id,
                visible,
            )
        })
    })
}

fn hotkey_has_terminal_successor_snapshot(
    context: &HotkeySnapshotWaitContext,
    events: &[HotkeyCandidateEventEvidence],
    source_intent: &HotkeyCandidateEventEvidence,
    source_revision: u64,
    source_invocation_id: u64,
) -> bool {
    events.iter().any(|successor| {
        if successor.stream != context.stream
            || successor.input_group_id != context.input_group_id
            || successor.kind != HotkeyTraceEventKind::VisibilityIntent
            || successor.visibility_source != Some(HotkeyVisibilitySource::ToggleBatch)
            || successor.elapsed_ms < source_intent.elapsed_ms
            || !successor
                .visibility_revision
                .is_some_and(|revision| revision > source_revision)
        {
            return false;
        }
        let Some(successor_invocation_id) = successor.invocation_id else {
            return false;
        };
        if successor_invocation_id == source_invocation_id
            || !events.iter().any(|event| {
                event.stream == context.stream
                    && event.input_group_id == context.input_group_id
                    && event.kind == HotkeyTraceEventKind::ShortTap
                    && event.invocation_id == Some(successor_invocation_id)
                    && event.terminal == Some(true)
                    && event.elapsed_ms <= successor.elapsed_ms
            })
        {
            return false;
        }
        let (Some(revision), Some(visible)) = (successor.visibility_revision, successor.visible)
        else {
            return false;
        };
        events.iter().any(|command| {
            command.stream == context.stream
                && command.input_group_id == context.input_group_id
                && command.kind == HotkeyTraceEventKind::RootCommand
                && command.visibility_revision == Some(revision)
                && command.invocation_id == Some(successor_invocation_id)
                && command.elapsed_ms >= successor.elapsed_ms
                && command.event_ordinal > successor.event_ordinal
                && command.request_id.is_some()
                && hotkey_command_has_target_snapshot(
                    context,
                    events,
                    command,
                    revision,
                    successor_invocation_id,
                    visible,
                )
        })
    })
}

fn hotkey_command_has_target_snapshot(
    context: &HotkeySnapshotWaitContext,
    events: &[HotkeyCandidateEventEvidence],
    command: &HotkeyCandidateEventEvidence,
    revision: u64,
    invocation_id: u64,
    visible: bool,
) -> bool {
    let Some(request_id) = command.request_id else {
        return false;
    };
    events.iter().any(|snapshot| {
        if snapshot.stream != context.stream
            || snapshot.input_group_id != context.input_group_id
            || snapshot.kind != HotkeyTraceEventKind::NativeWindowSnapshot
            || snapshot.request_id != Some(request_id)
            || snapshot.visibility_revision != Some(revision)
            || snapshot.invocation_id != Some(invocation_id)
            || snapshot.elapsed_ms < command.elapsed_ms
            || snapshot.event_ordinal <= command.event_ordinal
            || snapshot.hwnd != Some(context.root_hwnd)
            || snapshot.process_id != Some(context.root_process_id)
        {
            return false;
        }
        let (Some(native_visible), Some(minimized), Some(bounds)) =
            (snapshot.visible, snapshot.minimized, snapshot.bounds)
        else {
            return false;
        };
        let physically_visible = native_visible
            && !minimized
            && intersects_display_bounds(bounds, &context.physical_displays);
        physically_visible == visible
    })
}

fn complete_open_hotkey_capture_at_current_trace(trace_path: &Path) -> Result<(), CaseFailure> {
    let has_open_segment = ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| {
        slot.borrow().as_ref().is_some_and(|capture| {
            capture
                .segments
                .iter()
                .any(|segment| segment.path == trace_path && segment.end.is_none())
        })
    });
    if has_open_segment {
        complete_hotkey_capture_at_current_trace(trace_path)
    } else {
        Ok(())
    }
}

fn finish_hotkey_evidence_capture(case_id: &str) -> Option<HotkeyCaseEvidence> {
    let mut capture = ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| slot.borrow_mut().take())?;
    if capture.case_id != case_id {
        return None;
    }
    let mut events = Vec::new();
    let mut candidate_trace_overflow = capture.candidate_trace_overflow;
    for segment in &mut capture.segments {
        if let Some(materialized_events) = segment.materialized_events.take() {
            for event in materialized_events {
                if events.len() >= MAX_HOTKEY_EVIDENCE_EVENTS {
                    candidate_trace_overflow = true;
                    break;
                }
                events.push(event);
            }
            continue;
        }
        let lines = trace_lines(&segment.path);
        let end = segment.end.unwrap_or(lines.len());
        if end > lines.len() {
            candidate_trace_overflow = true;
        }
        let bounded_end = end.min(lines.len());
        if segment.cursor > bounded_end {
            candidate_trace_overflow = true;
            continue;
        }
        for ordinal in segment.cursor..bounded_end {
            if let Some(event) = parse_hotkey_candidate_event(
                &lines[ordinal],
                segment.stream,
                segment.input_group_id,
                segment.purpose,
                ordinal.saturating_add(1),
            ) {
                if events.len() >= MAX_HOTKEY_EVIDENCE_EVENTS {
                    candidate_trace_overflow = true;
                    break;
                }
                events.push(event);
            }
        }
    }
    events.sort_by_key(|event| (event.stream, event.event_ordinal));
    let mut root_identities = Vec::new();
    for segment in &capture.segments {
        let identity = HotkeyRootIdentityEvidence {
            stream: segment.stream,
            hwnd: segment.root_hwnd,
            process_id: segment.root_process_id,
        };
        if let Some(existing) = root_identities
            .iter()
            .find(|existing: &&HotkeyRootIdentityEvidence| existing.stream == segment.stream)
        {
            if *existing != identity {
                candidate_trace_overflow = true;
            }
        } else {
            root_identities.push(identity);
        }
    }
    let (gestures, standalone_decisions, gesture_overflow) = build_hotkey_decision_proofs(&events);
    let follow_on_restorations = build_hotkey_follow_on_restorations(&events);
    Some(HotkeyCaseEvidence {
        schema_version: 4,
        case_id: case_id.to_owned(),
        expected_state: hotkey_expected_state(case_id)?,
        runner_clock: "runner_monotonic_relative_us".to_owned(),
        runner_edges: capture.runner_edges,
        candidate_event_count: events.len(),
        candidate_events: events,
        gestures,
        standalone_decisions,
        follow_on_restorations,
        root_identities,
        physical_displays: capture.physical_displays,
        candidate_trace_overflow,
        capture_segment_overflow: capture.capture_segment_overflow,
        runner_edge_overflow: capture.runner_edge_overflow,
        gesture_overflow,
    })
}

fn parse_hotkey_candidate_event(
    line: &str,
    stream: HotkeyCandidateStream,
    input_group_id: u32,
    input_purpose: HotkeyRunnerInputPurpose,
    event_ordinal: usize,
) -> Option<HotkeyCandidateEventEvidence> {
    let event = trace_field_value(line, "trace_event")?;
    let elapsed_ms = trace_field_value(line, "elapsed_ms")?.parse().ok()?;
    let mut parsed = HotkeyCandidateEventEvidence {
        stream,
        input_group_id,
        input_purpose,
        event_ordinal: u32::try_from(event_ordinal).ok()?,
        elapsed_ms,
        kind: match event {
            "configured_primary" => match trace_field_value(line, "transition")? {
                "Press" => HotkeyTraceEventKind::PrimaryPress,
                "Release" => HotkeyTraceEventKind::PrimaryRelease,
                _ => return None,
            },
            "short_tap" => HotkeyTraceEventKind::ShortTap,
            "screen_draw_restore_focus_intent" => {
                HotkeyTraceEventKind::ScreenDrawRestoreFocusIntent
            }
            "desired_visibility" => HotkeyTraceEventKind::VisibilityIntent,
            "root_command" => HotkeyTraceEventKind::RootCommand,
            "native_window_snapshot" => HotkeyTraceEventKind::NativeWindowSnapshot,
            "native_activation" => HotkeyTraceEventKind::NativeActivation,
            "radial_action" => HotkeyTraceEventKind::RadialAction,
            _ => return None,
        },
        invocation_id: None,
        visibility_revision: None,
        request_id: None,
        visible: None,
        minimized: None,
        bounds: None,
        hwnd: None,
        process_id: None,
        command: None,
        visibility_source: None,
        modifiers_match: None,
        provenance: None,
        terminal: None,
        activation_edge: None,
        focus_intent: None,
        radial_action_stage: None,
    };
    parsed.invocation_id = trace_field_value(line, "invocation_id")
        .filter(|value| *value != "none" && *value != "0")
        .and_then(|value| value.parse().ok());
    parsed.visibility_revision = trace_field_value(line, "revision")
        .or_else(|| trace_field_value(line, "visibility_revision"))
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value != 0);
    parsed.request_id = trace_field_value(line, "request_id")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value != 0);
    parsed.visible = trace_field_value(line, "visible").and_then(super::super::parse_trace_bool);
    parsed.minimized =
        trace_field_value(line, "minimized").and_then(super::super::parse_trace_bool);
    if parsed.kind == HotkeyTraceEventKind::NativeWindowSnapshot {
        parsed.hwnd = Some(trace_field_value(line, "hwnd")?.parse().ok()?);
        parsed.process_id = Some(trace_field_value(line, "process_id")?.parse().ok()?);
        parsed.bounds = Some([
            trace_field_value(line, "left")?.parse().ok()?,
            trace_field_value(line, "top")?.parse().ok()?,
            trace_field_value(line, "right")?.parse().ok()?,
            trace_field_value(line, "bottom")?.parse().ok()?,
        ]);
    } else if parsed.kind == HotkeyTraceEventKind::NativeActivation {
        parsed.hwnd = Some(trace_field_value(line, "hwnd")?.parse().ok()?);
    }
    parsed.command = trace_field_value(line, "command").and_then(|command| match command {
        "position" | "Position" => Some(HotkeyRootCommand::Position),
        "size" | "Size" => Some(HotkeyRootCommand::Size),
        "Show" => Some(HotkeyRootCommand::Show),
        "Minimize" => Some(HotkeyRootCommand::Minimize),
        "Focus" => Some(HotkeyRootCommand::Focus),
        "ParkingBoundary" => Some(HotkeyRootCommand::ParkingBoundary),
        _ => Some(HotkeyRootCommand::Other),
    });
    parsed.visibility_source = trace_field_value(line, "source").and_then(|source| match source {
        "ToggleBatch" => Some(HotkeyVisibilitySource::ToggleBatch),
        "LegacyTrigger" => Some(HotkeyVisibilitySource::LegacyTrigger),
        "Queued" => Some(HotkeyVisibilitySource::Queued),
        "ScreenDrawRestore" => Some(HotkeyVisibilitySource::ScreenDrawRestore),
        _ => None,
    });
    parsed.modifiers_match =
        trace_field_value(line, "modifiers_match").and_then(super::super::parse_trace_bool);
    parsed.provenance = trace_field_value(line, "provenance").map(|provenance| match provenance {
        "Owned" => HotkeyInputProvenance::Owned,
        "ExternalInjected" => HotkeyInputProvenance::ExternalInjected,
        "Physical" => HotkeyInputProvenance::Physical,
        _ => HotkeyInputProvenance::Other,
    });
    parsed.terminal = trace_field_value(line, "terminal").and_then(super::super::parse_trace_bool);
    parsed.activation_edge = trace_field_value(line, "edge").map(|edge| match edge {
        "RestoreRequested" => HotkeyActivationEdge::RestoreRequested,
        "RestoreCompleted" => HotkeyActivationEdge::RestoreCompleted,
        "RestoreFailed" => HotkeyActivationEdge::RestoreFailed,
        "Superseded" => HotkeyActivationEdge::Superseded,
        _ => HotkeyActivationEdge::Other,
    });
    parsed.focus_intent = trace_field_value(line, "focus_intent").and_then(|intent| match intent {
        "ActivateRoot" => Some(HotkeyRootFocusIntent::ActivateRoot),
        "PreserveForeground" => Some(HotkeyRootFocusIntent::PreserveForeground),
        _ => None,
    });
    if parsed.kind == HotkeyTraceEventKind::RadialAction {
        parsed.radial_action_stage = Some(match trace_field_value(line, "stage") {
            Some("Activated") => HotkeyRadialActionStage::Activated,
            Some("Parsed") => HotkeyRadialActionStage::Parsed,
            Some("ParseRejected") => HotkeyRadialActionStage::ParseRejected,
            Some("Dispatched") => HotkeyRadialActionStage::Dispatched,
            Some("HostEntered") => HotkeyRadialActionStage::HostEntered,
            Some("EditorModeApplied") => HotkeyRadialActionStage::EditorModeApplied,
            Some("HostCompleted") => HotkeyRadialActionStage::HostCompleted,
            Some(_) => HotkeyRadialActionStage::Other,
            None => HotkeyRadialActionStage::Other,
        });
    }
    Some(parsed)
}

fn build_hotkey_decision_proofs(
    events: &[HotkeyCandidateEventEvidence],
) -> (
    Vec<HotkeyGestureEvidence>,
    Vec<HotkeyStandaloneDecisionEvidence>,
    bool,
) {
    let mut proof_error = false;
    let mut intents = events
        .iter()
        .filter(|event| {
            event.kind == HotkeyTraceEventKind::VisibilityIntent
                && event.visibility_source.is_some_and(|source| {
                    matches!(
                        source,
                        HotkeyVisibilitySource::ToggleBatch | HotkeyVisibilitySource::LegacyTrigger
                    )
                })
        })
        .collect::<Vec<_>>();
    intents.sort_by_key(|event| (event.stream, event.elapsed_ms, event.event_ordinal));
    let mut gestures = Vec::new();
    let mut standalone = Vec::new();
    let releases = events
        .iter()
        .filter(|event| event.kind == HotkeyTraceEventKind::PrimaryRelease)
        .collect::<Vec<_>>();
    let mut paired_releases = BTreeSet::new();
    for release in releases {
        let (Some(invocation_id), Some(release_modifiers_match)) =
            (release.invocation_id, release.modifiers_match)
        else {
            proof_error = true;
            continue;
        };
        let matching = |event: &&HotkeyCandidateEventEvidence| {
            event.stream == release.stream && event.invocation_id == Some(invocation_id)
        };
        let presses = events
            .iter()
            .filter(|event| event.kind == HotkeyTraceEventKind::PrimaryPress)
            .filter(matching)
            .collect::<Vec<_>>();
        if presses.len() != 1 || presses[0].modifiers_match != Some(true) {
            proof_error = true;
        }
        let taps = events
            .iter()
            .filter(|event| event.kind == HotkeyTraceEventKind::ShortTap)
            .filter(matching)
            .collect::<Vec<_>>();
        if taps.len() > 1 {
            proof_error = true;
        }
        let group_intents = intents
            .iter()
            .copied()
            .filter(|event| {
                event.stream == release.stream && event.invocation_id == Some(invocation_id)
            })
            .collect::<Vec<_>>();
        if taps.is_empty() {
            if !group_intents.is_empty() {
                proof_error = true;
            }
            let legacy_group_intents = intents
                .iter()
                .copied()
                .filter(|event| {
                    event.stream == release.stream
                        && event.input_group_id == release.input_group_id
                        && event.input_purpose == release.input_purpose
                        && event.invocation_id.is_none()
                        && event.visibility_source == Some(HotkeyVisibilitySource::LegacyTrigger)
                })
                .collect::<Vec<_>>();
            let paired_legacy_trigger = legacy_group_intents.len() == 1;
            gestures.push(HotkeyGestureEvidence {
                stream: release.stream,
                input_group_id: release.input_group_id,
                input_purpose: release.input_purpose,
                invocation_id,
                release_elapsed_ms: release.elapsed_ms,
                release_modifiers_match,
                short_tap_elapsed_ms: None,
                decision: HotkeyDecisionProof::NotApplicable {
                    reason: if release.input_purpose == HotkeyRunnerInputPurpose::DirectTrigger
                        || paired_legacy_trigger
                    {
                        HotkeyEvidenceNotApplicable::LegacyTriggerHasNoInvocationReducerId
                    } else {
                        HotkeyEvidenceNotApplicable::HoldGestureHasNoShortTap
                    },
                },
            });
            continue;
        }
        let tap = taps[0];
        if tap.elapsed_ms < release.elapsed_ms
            || tap.terminal != Some(true)
            || group_intents.len() != 1
        {
            proof_error = true;
        }
        let Some(intent) = group_intents.first().copied() else {
            continue;
        };
        if intent.elapsed_ms < release.elapsed_ms || intent.elapsed_ms < tap.elapsed_ms {
            proof_error = true;
        }
        paired_releases.insert((release.stream, invocation_id));
        let revision = intent.visibility_revision.unwrap_or(0);
        let commands = commands_for_intent(events, intent, Some(release.elapsed_ms));
        let decision = if revision == 0 {
            proof_error = true;
            HotkeyDecisionProof::NotApplicable {
                reason: HotkeyEvidenceNotApplicable::SupersededIntermediateVisibility,
            }
        } else if !commands.is_empty() {
            HotkeyDecisionProof::Applied {
                visibility_revision: revision,
                intent_elapsed_ms: intent.elapsed_ms,
                visible: intent.visible.unwrap_or(false),
                release_to_intent_ms: intent.elapsed_ms.saturating_sub(release.elapsed_ms),
                root_commands: commands,
            }
        } else if let Some(next_revision) = intents
            .iter()
            .filter(|next| {
                next.stream == intent.stream
                    && next
                        .visibility_revision
                        .is_some_and(|next_revision| next_revision > revision)
                    && next.elapsed_ms >= intent.elapsed_ms
                    && !commands_for_intent(events, next, None).is_empty()
            })
            .filter_map(|next| next.visibility_revision)
            .min()
        {
            HotkeyDecisionProof::Superseded {
                visibility_revision: revision,
                by_revision: next_revision,
                intent_elapsed_ms: intent.elapsed_ms,
                visible: intent.visible.unwrap_or(false),
                release_to_intent_ms: intent.elapsed_ms.saturating_sub(release.elapsed_ms),
            }
        } else {
            proof_error = true;
            HotkeyDecisionProof::NotApplicable {
                reason: HotkeyEvidenceNotApplicable::SupersededIntermediateVisibility,
            }
        };
        gestures.push(HotkeyGestureEvidence {
            stream: release.stream,
            input_group_id: release.input_group_id,
            input_purpose: release.input_purpose,
            invocation_id,
            release_elapsed_ms: release.elapsed_ms,
            release_modifiers_match,
            short_tap_elapsed_ms: Some(tap.elapsed_ms),
            decision,
        });
    }

    for intent in intents {
        if intent.invocation_id.is_some() {
            if !paired_releases.contains(&(intent.stream, intent.invocation_id.unwrap_or_default()))
            {
                proof_error = true;
            }
            continue;
        }
        if intent.visibility_source != Some(HotkeyVisibilitySource::LegacyTrigger) {
            proof_error = true;
            continue;
        }
        let revision = intent.visibility_revision.unwrap_or(0);
        let root_commands = commands_for_intent(events, intent, None);
        if revision == 0 || root_commands.is_empty() {
            proof_error = true;
        }
        standalone.push(HotkeyStandaloneDecisionEvidence {
            stream: intent.stream,
            input_group_id: intent.input_group_id,
            input_purpose: intent.input_purpose,
            visibility_revision: revision,
            invocation_id: None,
            intent_elapsed_ms: intent.elapsed_ms,
            visible: intent.visible.unwrap_or(false),
            source: HotkeyVisibilitySource::LegacyTrigger,
            root_commands,
            decision: HotkeyDecisionProof::NotApplicable {
                reason: HotkeyEvidenceNotApplicable::LegacyTriggerHasNoInvocationReducerId,
            },
        });
    }
    if gestures.len() > MAX_HOTKEY_EVIDENCE_GESTURES
        || standalone.len() > MAX_HOTKEY_EVIDENCE_GESTURES
    {
        proof_error = true;
        gestures.truncate(MAX_HOTKEY_EVIDENCE_GESTURES);
        standalone.truncate(MAX_HOTKEY_EVIDENCE_GESTURES);
    }
    (gestures, standalone, proof_error)
}

fn build_hotkey_follow_on_restorations(
    events: &[HotkeyCandidateEventEvidence],
) -> Vec<HotkeyFollowOnRestoreEvidence> {
    let mut restorations = events
        .iter()
        .filter(|event| {
            event.kind == HotkeyTraceEventKind::VisibilityIntent
                && event.visibility_source == Some(HotkeyVisibilitySource::ScreenDrawRestore)
        })
        .collect::<Vec<_>>();
    restorations.sort_by_key(|event| (event.stream, event.elapsed_ms, event.event_ordinal));
    restorations
        .into_iter()
        .filter_map(|event| {
            let visibility_revision = event.visibility_revision?;
            let focus_intent_event = events.iter().find(|candidate| {
                candidate.stream == event.stream
                    && candidate.input_group_id == event.input_group_id
                    && candidate.kind == HotkeyTraceEventKind::ScreenDrawRestoreFocusIntent
                    && candidate.visibility_revision == Some(visibility_revision)
                    && candidate.invocation_id == event.invocation_id
                    && candidate.event_ordinal > event.event_ordinal
                    && candidate.elapsed_ms >= event.elapsed_ms
            })?;
            let focus_intent = focus_intent_event.focus_intent?;
            let parent_visibility_revision = event.invocation_id.and_then(|invocation_id| {
                events
                    .iter()
                    .filter(|prior| {
                        prior.stream == event.stream
                            && prior.kind == HotkeyTraceEventKind::VisibilityIntent
                            && prior.visibility_source == Some(HotkeyVisibilitySource::ToggleBatch)
                            && prior.invocation_id == Some(invocation_id)
                            && prior
                                .visibility_revision
                                .is_some_and(|revision| revision < visibility_revision)
                            && prior.elapsed_ms <= event.elapsed_ms
                    })
                    .max_by_key(|prior| prior.elapsed_ms)
                    .and_then(|prior| prior.visibility_revision)
            });
            Some(HotkeyFollowOnRestoreEvidence {
                stream: event.stream,
                input_group_id: event.input_group_id,
                parent_visibility_revision,
                visibility_revision,
                invocation_id: event.invocation_id,
                intent_elapsed_ms: event.elapsed_ms,
                visible: event.visible.unwrap_or(false),
                focus_intent,
                root_commands: commands_for_intent(events, event, None),
                native_activation: native_activation_span_for_intent(events, event),
            })
        })
        .collect()
}

fn native_activation_span_for_intent(
    events: &[HotkeyCandidateEventEvidence],
    intent: &HotkeyCandidateEventEvidence,
) -> Option<HotkeyNativeActivationSpan> {
    let requested = events
        .iter()
        .filter(|event| {
            event.stream == intent.stream
                && event.kind == HotkeyTraceEventKind::NativeActivation
                && event.visibility_revision == intent.visibility_revision
                && event.invocation_id == intent.invocation_id
                && event.elapsed_ms >= intent.elapsed_ms
                && event.activation_edge == Some(HotkeyActivationEdge::RestoreRequested)
        })
        .min_by_key(|event| (event.elapsed_ms, event.event_ordinal))?;
    let request_id = requested.request_id?;
    let hwnd = requested.hwnd?;
    let terminal = events
        .iter()
        .filter(|event| {
            event.stream == intent.stream
                && event.kind == HotkeyTraceEventKind::NativeActivation
                && event.request_id == Some(request_id)
                && event.visibility_revision == intent.visibility_revision
                && event.invocation_id == intent.invocation_id
                && event.hwnd == Some(hwnd)
                && event.elapsed_ms >= requested.elapsed_ms
                && event.terminal == Some(true)
                && matches!(
                    event.activation_edge,
                    Some(
                        HotkeyActivationEdge::RestoreCompleted
                            | HotkeyActivationEdge::RestoreFailed
                            | HotkeyActivationEdge::Superseded
                    )
                )
        })
        .min_by_key(|event| (event.elapsed_ms, event.event_ordinal));
    Some(HotkeyNativeActivationSpan {
        request_id,
        hwnd,
        requested_elapsed_ms: requested.elapsed_ms,
        terminal_elapsed_ms: terminal.map(|event| event.elapsed_ms),
        terminal_edge: terminal.and_then(|event| event.activation_edge),
    })
}

fn commands_for_intent(
    events: &[HotkeyCandidateEventEvidence],
    intent: &HotkeyCandidateEventEvidence,
    release_elapsed_ms: Option<u64>,
) -> Vec<HotkeyRootCommandSpan> {
    let Some(revision) = intent.visibility_revision else {
        return Vec::new();
    };
    let mut commands = events
        .iter()
        .filter(|event| {
            event.stream == intent.stream
                && event.kind == HotkeyTraceEventKind::RootCommand
                && event.visibility_revision == Some(revision)
                && event.invocation_id == intent.invocation_id
                && event.elapsed_ms >= intent.elapsed_ms
        })
        .filter_map(|command| {
            let request_id = command.request_id?;
            let observed = events
                .iter()
                .filter(|event| {
                    event.stream == intent.stream
                        && event.kind == HotkeyTraceEventKind::NativeWindowSnapshot
                        && event.request_id == Some(request_id)
                        && event.visibility_revision == Some(revision)
                        && event.invocation_id == intent.invocation_id
                        && event.elapsed_ms >= command.elapsed_ms
                })
                .min_by_key(|event| event.elapsed_ms);
            let observed_presentation = observed.and_then(|event| {
                Some(HotkeyObservedPresentation {
                    elapsed_ms: event.elapsed_ms,
                    command_to_observed_ms: event.elapsed_ms.checked_sub(command.elapsed_ms)?,
                    visible: event.visible?,
                    minimized: event.minimized?,
                    bounds: event.bounds?,
                })
            });
            Some(HotkeyRootCommandSpan {
                visibility_revision: revision,
                invocation_id: intent.invocation_id,
                request_id,
                command_elapsed_ms: command.elapsed_ms,
                command: command.command.unwrap_or(HotkeyRootCommand::Other),
                release_to_root_command_ms: release_elapsed_ms
                    .and_then(|release| command.elapsed_ms.checked_sub(release)),
                command_event_ordinal: command.event_ordinal,
                observed_snapshot_event_ordinal: observed.map(|event| event.event_ordinal),
                command_to_observed_ms: observed
                    .and_then(|event| event.elapsed_ms.checked_sub(command.elapsed_ms)),
                observed_presentation,
            })
        })
        .collect::<Vec<_>>();
    commands.sort_by_key(|command| (command.command_elapsed_ms, command.request_id));
    commands
}

fn missing_case_ids(existing_ids: &[&str]) -> Vec<&'static str> {
    CASE_IDS
        .iter()
        .copied()
        .filter(|id| !existing_ids.contains(id))
        .collect()
}

struct HoldReleaseHandoff {
    release_at_unix_ms: Option<u128>,
    sentinel_at_unix_ms: Option<u128>,
    quiescent_acknowledged: bool,
    observer: Option<RunnerHookObserver>,
}

struct DesignerEntry {
    window: WindowSnapshot,
    session_id: u64,
    root_recovery: Option<String>,
    root_menu_resolution: Option<String>,
}

#[derive(Clone, Debug)]
struct PersistedMenuGraphExpectation {
    menu_index: usize,
    ring_slots: Vec<usize>,
    populated_cells: usize,
    cell_ids_digest: u64,
}

#[derive(Clone, Debug)]
pub struct CopiedAuthoringOptions {
    pub target_action_index: usize,
    pub skin_index: usize,
    pub original_menu_sha256: Vec<String>,
    pub restore_menu_name: String,
}

#[derive(Clone, Debug)]
struct ActionSideEffectBaseline {
    history: Option<Vec<u8>>,
    trace_event_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AuthoringRequestIdentity {
    request_id: u64,
    generation: u64,
    session_id: u64,
}

#[derive(Clone, Debug)]
struct AuthoringReplyEvidence {
    identity: AuthoringRequestIdentity,
}

struct AcceptancePrepareHold {
    path: PathBuf,
    released: bool,
}

impl AcceptancePrepareHold {
    fn create(profile: &Path, purpose: &str) -> Result<Self, String> {
        let path = profile.join(super::PREPARE_HOLD_FILE_NAME);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("create {purpose} hold marker: {error}"))?;
        Ok(Self {
            path,
            released: false,
        })
    }

    fn release(&mut self) -> Result<String, String> {
        fs::remove_file(&self.path)
            .map_err(|error| format!("release acceptance preparation hold marker: {error}"))?;
        self.released = true;
        if self.path.exists() {
            return Err("D7 preview preparation hold marker remained after release".into());
        }
        Ok("removed the bounded temp-profile hold marker".into())
    }
}

impl Drop for AcceptancePrepareHold {
    fn drop(&mut self) {
        if !self.released {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[derive(Serialize)]
struct PostG2RootSnapshot {
    captured_unix_ms: u128,
    child_process_id: u32,
    root_hwnd: u64,
    visible: bool,
    minimized: bool,
    bounds: [i32; 4],
    physical_displays: Vec<[i32; 4]>,
    intersects_physical_display: bool,
    sample_count: u8,
    drawable_on_display_samples: u8,
    consecutive_stable_samples: u8,
    stable_on_display: bool,
    root_trace_tail: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RootMenuState {
    file_open: bool,
    apps_open: bool,
}

impl RootMenuState {
    fn any_open(self) -> bool {
        self.file_open || self.apps_open
    }
}

fn root_file_menu_ready_for_apps(state: RootMenuState) -> bool {
    state.file_open && !state.apps_open
}

fn should_retry_root_file_menu_open(state: RootMenuState) -> bool {
    !root_file_menu_ready_for_apps(state) && !state.any_open()
}

#[derive(Clone, Copy, Debug)]
enum DesignerSemanticTarget {
    Menus,
    Skins,
    Tree,
    Inspector,
    DefaultMenu,
    MenuName,
    MenuDefaultSkin,
}

struct F11HoldGuard<'a> {
    child: &'a NativeChild,
    anchor: &'a FocusAnchor,
    armed: bool,
}

impl<'a> F11HoldGuard<'a> {
    fn new(child: &'a NativeChild, anchor: &'a FocusAnchor) -> Self {
        Self {
            child,
            anchor,
            armed: true,
        }
    }

    fn release(&mut self) -> Result<NativeInputEdgeEvidence, String> {
        if !self.armed {
            return Err("F11 hold was already released".into());
        }

        let (foreground, process_id) = capture_foreground();
        let target =
            if process_id == self.child.process_id() || process_id == self.anchor.process_id() {
                foreground
            } else {
                self.anchor.focus()?;
                self.anchor.hwnd()
            };
        let evidence = self.child.release_f11(target)?;
        self.armed = false;
        Ok(evidence)
    }
}

impl Drop for F11HoldGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.release();
        }
    }
}

pub fn run_suite(
    executable: &str,
    profile: &Path,
    output: &Path,
    trace_path: &Path,
    hold_threshold_ms: u64,
    h6_repeat_mode: H6RepeatMode,
    cursor_restore: Option<POINT>,
    _desktop: &InputDesktopAttachment,
    report: &mut AcceptanceReport,
    runner_log: &mut File,
) -> Option<FocusAnchor> {
    if let Err(error) = preflight_acceptance_hotkey(AcceptanceHotkey::F11) {
        record_environment_failure(
            format!("acceptance hotkey preflight failed: {error}"),
            report,
            output,
            trace_path,
            runner_log,
        );
        return None;
    }
    let _ = writeln!(
        runner_log,
        "acceptance hotkey F11 registered and unregistered successfully before child launch"
    );

    let stdout_path = profile.join("child.stdout.log");
    let stderr_path = profile.join("child.stderr.log");
    let mut child = match NativeChild::launch(
        Path::new(executable),
        profile,
        trace_path,
        &stdout_path,
        &stderr_path,
    ) {
        Ok(child) => child,
        Err(error) => {
            report.environment.child_process_id = error.process_id;
            report.environment.child_started_unix_ms = error.started.and_then(|started| {
                started
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|duration| duration.as_millis())
            });
            if let Some(inventory) = save_launch_failure_inventory(&error, output) {
                report.push_artifact(inventory.to_string_lossy());
            }
            let failure = CaseFailure::new(FailureStage::CandidateStartup, error.to_string());
            for (index, id) in CASE_IDS
                .into_iter()
                .filter(|id| !DEFERRED_REPORT_CASE_IDS.contains(id))
                .enumerate()
            {
                if index == 0 {
                    append_case(
                        report,
                        id,
                        expected(id),
                        started_now(),
                        Err(failure.clone()),
                        None,
                        output,
                        trace_path,
                    );
                } else {
                    append_case_without_artifacts(
                        report,
                        id,
                        Err(CaseFailure::new(
                            failure.stage,
                            "not run because candidate startup failed; see the first case result"
                                .into(),
                        )),
                    );
                }
            }
            let _ = writeln!(
                runner_log,
                "candidate startup failed pid={:?} windows={} : {failure}",
                error.process_id,
                error.windows.len()
            );
            return None;
        }
    };

    report.environment.child_process_id = Some(child.process_id());
    report.environment.child_started_unix_ms = child
        .started()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis());
    let _ = writeln!(
        runner_log,
        "launched source-matched candidate pid={} root_hwnd={} desktop={} started={:?}",
        child.process_id(),
        hwnd_id(child.root().hwnd),
        child.desktop_name(),
        child.started()
    );

    let anchor = match FocusAnchor::create() {
        Ok(anchor) => anchor,
        Err(error) => {
            let failure = CaseFailure::new(FailureStage::Environment, error);
            for (index, id) in CASE_IDS
                .into_iter()
                .filter(|id| !DEFERRED_REPORT_CASE_IDS.contains(id))
                .enumerate()
            {
                if index == 0 {
                    append_case(
                        report,
                        id,
                        expected(id),
                        started_now(),
                        Err(failure.clone()),
                        Some(&child),
                        output,
                        trace_path,
                    );
                } else {
                    append_case_without_artifacts(
                        report,
                        id,
                        Err(CaseFailure::new(
                            failure.stage,
                            "not run because runner setup failed; see the first case result".into(),
                        )),
                    );
                }
            }
            restore_cursor_before_shutdown(cursor_restore, runner_log);
            stop_child(&mut child, report, runner_log, output, trace_path);
            return None;
        }
    };
    let mut uia: Option<UiAutomation> = None;
    let mut designer_window: Option<WindowSnapshot> = None;
    let mut hold_windows: Option<Vec<WindowSnapshot>> = None;

    run_tap_case(
        report,
        "H0",
        &mut child,
        &anchor,
        trace_path,
        output,
        "focused ROOT tap parks ROOT offscreen with one short-tap path",
        true,
        false,
        1,
    );
    run_tap_case(
        report,
        "H1",
        &mut child,
        &anchor,
        trace_path,
        output,
        "runner-owned focus tap shows and focuses ROOT",
        false,
        true,
        1,
    );
    run_other_focus_case(report, &mut child, &anchor, trace_path, output);
    let (hold_open, hold_guard, hold_observer, hold_observer_error) = run_hold_open_case(
        &child,
        &anchor,
        trace_path,
        hold_threshold_ms,
        &mut hold_windows,
    );
    append_case(
        report,
        "H4",
        expected("H4"),
        started_now(),
        hold_open,
        Some(&child),
        output,
        trace_path,
    );
    let h5_handoff = run_hold_release_case(
        report,
        &child,
        &anchor,
        trace_path,
        output,
        hold_windows.as_deref(),
        hold_guard,
        hold_observer,
        hold_observer_error,
        h6_repeat_mode,
    );
    run_second_hold_case(
        report,
        &child,
        &anchor,
        trace_path,
        output,
        hold_threshold_ms,
        hold_windows.as_deref(),
        h6_repeat_mode,
        h5_handoff,
    );
    run_hotkey_burst_case(
        report,
        "H7",
        &child,
        &anchor,
        trace_path,
        output,
        AcceptanceHotkey::F11,
        3,
    );
    run_hotkey_burst_case(
        report,
        "H8",
        &child,
        &anchor,
        trace_path,
        output,
        AcceptanceHotkey::F11,
        4,
    );

    let ui_result = UiAutomation::new();
    match ui_result {
        Ok(automation) => uia = Some(automation),
        Err(error) => {
            let started = Instant::now();
            append_case(
                report,
                "D0",
                expected("D0"),
                started,
                Err(CaseFailure::new(FailureStage::Environment, error.clone())),
                Some(&child),
                output,
                trace_path,
            );
        }
    }

    if let Some(automation) = uia.as_ref() {
        match run_designer_entry(&mut child, automation, &anchor, trace_path) {
            Ok(entry) => {
                designer_window = Some(entry.window);
                append_case(
                    report,
                    "D0",
                    expected("D0"),
                    started_now(),
                    Ok("one child-owned Designer HWND appeared; UIA root belongs to the child and its InitialSnapshot reply was accepted".into()),
                    None,
                    output,
                    trace_path,
                );
            }
            Err(failure) => {
                append_case(
                    report,
                    "D0",
                    expected("D0"),
                    started_now(),
                    Err(failure.clone()),
                    Some(&child),
                    output,
                    trace_path,
                );
                append_blocked_designer_cases(report, &failure, Some(&child), output, trace_path);
            }
        }
    } else {
        let failure = CaseFailure::new(
            FailureStage::Environment,
            "UI Automation initialization failed".into(),
        );
        append_blocked_designer_cases(report, &failure, Some(&child), output, trace_path);
    }

    run_failure_artifact_case(report, &child, output, trace_path);

    if let (Some(automation), Some(designer)) = (uia.as_ref(), designer_window.as_ref()) {
        run_designer_focus_case(report, &mut child, automation, designer, trace_path, output);
        run_designer_pointer_case(
            report,
            &mut child,
            automation,
            designer,
            trace_path,
            output,
            expected("D1"),
            false,
        );
        run_tab_case(
            report,
            &mut child,
            automation,
            designer,
            output,
            trace_path,
            DESIGNER_STARTER_NAME,
            false,
        );
        run_skins_command_case(report, &mut child, automation, designer, trace_path, output);
        run_designer_close_case(report, &mut child, designer, output, trace_path);
        match run_designer_entry(&mut child, automation, &anchor, trace_path) {
            Ok(entry) => run_authoring_geometry_cases(
                report,
                &mut child,
                automation,
                &anchor,
                profile,
                &entry.window,
                entry.session_id,
                output,
                trace_path,
                None,
            ),
            Err(failure) => {
                append_blocked_authoring_cases(report, &failure, Some(&child), output, trace_path)
            }
        }
    }

    restore_cursor_before_shutdown(cursor_restore, runner_log);
    stop_child(&mut child, report, runner_log, output, trace_path);
    let existing_case_ids = report
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<Vec<_>>();
    for id in missing_case_ids(&existing_case_ids)
        .into_iter()
        .filter(|id| !DEFERRED_REPORT_CASE_IDS.contains(id))
    {
        append_case_without_artifacts(
            report,
            id,
            Err(CaseFailure::new(
                FailureStage::Cleanup,
                "runner omitted a required case result".into(),
            )),
        );
    }
    let _ = writeln!(
        runner_log,
        "native suite completed with {} case records",
        report.cases.len()
    );
    Some(anchor)
}

pub fn run_hotkey_suite(
    executable: &str,
    profile: &Path,
    output: &Path,
    trace_path: &Path,
    hotkey: AcceptanceHotkey,
    cursor_restore: Option<POINT>,
    _desktop: &InputDesktopAttachment,
    report: &mut AcceptanceReport,
    runner_log: &mut File,
) -> Option<FocusAnchor> {
    if let Err(error) = preflight_acceptance_hotkey(hotkey) {
        record_environment_failure(
            format!("acceptance hotkey preflight failed: {error}"),
            report,
            output,
            trace_path,
            runner_log,
        );
        return None;
    }
    let _ = writeln!(
        runner_log,
        "acceptance hotkey {} registered and unregistered successfully before child launch",
        hotkey.as_str()
    );

    let stdout_path = profile.join("child.stdout.log");
    let stderr_path = profile.join("child.stderr.log");
    let mut child = match NativeChild::launch(
        Path::new(executable),
        profile,
        trace_path,
        &stdout_path,
        &stderr_path,
    ) {
        Ok(child) => child,
        Err(error) => {
            report.environment.child_process_id = error.process_id;
            report.environment.child_started_unix_ms = error.started.and_then(|started| {
                started
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|duration| duration.as_millis())
            });
            if let Some(inventory) = save_launch_failure_inventory(&error, output) {
                report.push_artifact(inventory.to_string_lossy());
            }
            record_environment_failure(
                format!("candidate startup failed: {error}"),
                report,
                output,
                trace_path,
                runner_log,
            );
            return None;
        }
    };

    report.environment.child_process_id = Some(child.process_id());
    report.environment.child_started_unix_ms = child
        .started()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis());
    let _ = writeln!(
        runner_log,
        "launched source-matched candidate pid={} root_hwnd={} desktop={} hotkey={}",
        child.process_id(),
        hwnd_id(child.root().hwnd),
        child.desktop_name(),
        hotkey.as_str()
    );

    if let Err(error) =
        wait_hotkey_fixture_ready(&child, trace_path, HOTKEY_FIXTURE_STARTUP_TIMEOUT)
    {
        append_hotkey_setup_failures(report, error, Some(&child), output, trace_path, hotkey);
        restore_cursor_before_shutdown(cursor_restore, runner_log);
        stop_child(&mut child, report, runner_log, output, trace_path);
        return None;
    }

    let anchor = match FocusAnchor::create() {
        Ok(anchor) => anchor,
        Err(error) => {
            append_hotkey_setup_failures(report, error, Some(&child), output, trace_path, hotkey);
            restore_cursor_before_shutdown(cursor_restore, runner_log);
            stop_child(&mut child, report, runner_log, output, trace_path);
            return None;
        }
    };

    let hold_threshold_ms = report.profile.hold_threshold_ms;
    run_hotkey_matrix_case(
        report,
        &child,
        &anchor,
        trace_path,
        output,
        hotkey,
        hold_threshold_ms,
    );
    run_hotkey_behavior_cases(
        executable,
        profile,
        report,
        &mut child,
        &anchor,
        trace_path,
        output,
        hotkey,
        hold_threshold_ms,
    );

    restore_cursor_before_shutdown(cursor_restore, runner_log);
    stop_child(&mut child, report, runner_log, output, trace_path);
    for id in HOTKEY_CASE_IDS.into_iter().filter(|id| *id != "R0") {
        if !report.cases.iter().any(|case| case.id == id) {
            append_case_without_artifacts(
                report,
                id,
                Err(CaseFailure::new(
                    FailureStage::Cleanup,
                    "runner omitted a required hotkey case result".into(),
                )),
            );
        }
    }
    let _ = writeln!(
        runner_log,
        "hotkey suite completed with {} case records",
        report.cases.len()
    );
    Some(anchor)
}

fn append_hotkey_setup_failures(
    report: &mut AcceptanceReport,
    message: String,
    child: Option<&NativeChild>,
    output: &Path,
    trace_path: &Path,
    hotkey: AcceptanceHotkey,
) {
    let mut case_ids = hotkey_setup_failure_case_ids();
    let Some(first_id) = case_ids.next() else {
        return;
    };
    let failure = CaseFailure::new(
        FailureStage::Environment,
        format!("hotkey={}; runner setup failed: {message}", hotkey.as_str()),
    );
    append_case(
        report,
        first_id,
        expected(first_id),
        started_now(),
        Err(failure.clone()),
        child,
        output,
        trace_path,
    );
    for id in case_ids {
        append_case_without_artifacts(
            report,
            id,
            Err(CaseFailure::new(
                failure.stage,
                format!(
                    "hotkey={}; not run because hotkey runner setup failed; see {first_id}",
                    hotkey.as_str()
                ),
            )),
        );
    }
}

fn hotkey_setup_failure_case_ids() -> impl Iterator<Item = &'static str> {
    HOTKEY_CASE_IDS
        .into_iter()
        .filter(|id| !matches!(*id, "CLEANUP" | "R0"))
}

fn run_hotkey_burst_case(
    report: &mut AcceptanceReport,
    id: &str,
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hotkey: AcceptanceHotkey,
    taps: usize,
) {
    let started = Instant::now();
    let hold_threshold_ms = report.profile.hold_threshold_ms;
    let result = run_hotkey_burst_attempt(
        child,
        anchor,
        trace_path,
        hotkey,
        taps,
        true,
        hold_threshold_ms,
    )
    .map(|evidence| {
        format!(
            "evidence:v1; hotkey={}; burst={taps}; initial_visible=true; final_visible={}; unique_invocation_ids={}; hold_ms={}..{}; released_gap_ms={}..{}; trace_fence={}; uninterrupted=true; observer=exact_injected_chord_edges; per_gesture_correlation=release_short_tap_visibility; key_cleanup=verified",
            hotkey.as_str(),
            evidence.final_visible,
            evidence.invocation_ids.len(),
            evidence.hold_min_ms,
            evidence.hold_max_ms,
            evidence.gap_min_ms,
            evidence.gap_max_ms,
            evidence.trace_fence.report_token(),
        )
    });
    append_case(
        report,
        id,
        expected(id),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_hotkey_matrix_case(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) {
    let started = Instant::now();
    let mut retained_artifacts = Vec::new();
    let attempt_progress = RefCell::new(Vec::new());
    let retry = run_h04_matrix_with_retry(
        |attempt| {
            let mut progress = attempt_progress.borrow_mut();
            progress.clear();
            begin_hotkey_evidence_capture("H04", trace_path);
            set_hotkey_capture_purpose(HotkeyRunnerInputPurpose::MatrixBurst);
            run_hotkey_matrix_attempt(
                child,
                anchor,
                trace_path,
                hotkey,
                hold_threshold_ms,
                &mut progress,
            )
            .map(|observed| (attempt, observed))
        },
        |attempt, failure| {
            let path = persist_h04_contaminated_attempt(
                attempt,
                hotkey,
                failure,
                trace_path,
                output,
                &attempt_progress.borrow(),
            )?;
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    CaseFailure::new(
                        FailureStage::Environment,
                        "H04 contamination artifact has no bounded filename".into(),
                    )
                })?
                .to_owned();
            retained_artifacts.push(path);
            Ok(name)
        },
    );
    let artifact_names = retry.contamination_artifact_names.join("|");
    let result = match retry.result {
        Ok((_attempt, observed)) => Ok(format!(
            "{observed}; matrix_attempts={}; contamination_attempts={}; contamination_artifacts={}; attempt_restart_state=hidden; full_clean_matrix=true",
            retry.attempts,
            retry.contamination_attempts,
            if artifact_names.is_empty() {
                "none"
            } else {
                &artifact_names
            },
        )),
        Err(mut failure) => {
            failure.message = format!(
                "evidence:v1; hotkey={}; failure={}",
                hotkey.as_str(),
                failure.message
            );
            failure.message.push_str(&format!(
                "; matrix_attempts={}; contamination_attempts={}; contamination_artifacts={}; attempt_restart_state=hidden; full_clean_matrix=false",
                retry.attempts,
                retry.contamination_attempts,
                if artifact_names.is_empty() {
                    "none"
                } else {
                    &artifact_names
                },
            ));
            Err(failure)
        }
    };

    append_case(
        report,
        "H04",
        expected("H04"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
    for path in retained_artifacts {
        attach_h04_contamination_artifact(report, path);
    }
}

fn run_hotkey_matrix_attempt(
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
    completed_bursts: &mut Vec<H04CompletedBurstEvidence>,
) -> Result<String, CaseFailure> {
    let mut all_ids = std::collections::BTreeSet::new();
    let mut quiet_windows = Vec::with_capacity(10);
    let mut preflight_matching_edges = 0usize;
    let mut foreign_matching_edges = 0usize;
    for initial_visible in [false, true] {
        for taps in [1usize, 2, 5, 10, 25] {
            let evidence = run_hotkey_burst_attempt(
                child,
                anchor,
                trace_path,
                hotkey,
                taps,
                initial_visible,
                hold_threshold_ms,
            )?;
            let matrix_burst_index = u8::try_from(completed_bursts.len() + 1).unwrap_or(u8::MAX);
            completed_bursts.push(H04CompletedBurstEvidence {
                matrix_burst_index,
                input_group_id: evidence.input_group_id,
                initial_visible,
                requested_taps: u8::try_from(taps).unwrap_or(u8::MAX),
                final_visible: evidence.final_visible,
                hold_min_ms: evidence.hold_min_ms,
                hold_max_ms: evidence.hold_max_ms,
                gap_min_ms: evidence.gap_min_ms,
                gap_max_ms: evidence.gap_max_ms,
                invocation_ids: evidence.invocation_ids.clone(),
                trace_probe_id: evidence.trace_fence.probe_id,
                trace_cursor: evidence.trace_fence.cursor,
                baseline_invocation_id: evidence.trace_fence.baseline_invocation_id,
                baseline_visibility_revision: evidence.trace_fence.baseline_visibility_revision,
            });
            for invocation_id in &evidence.invocation_ids {
                if !all_ids.insert(*invocation_id) {
                    return Err(CaseFailure::new(
                        FailureStage::GestureDecision,
                        format!("invocation ID {invocation_id} repeated across H04 bursts"),
                    ));
                }
            }
            let Some(quiet_ms) = evidence.preflight_quiet_ms else {
                return Err(CaseFailure::new(
                    FailureStage::InputInjection,
                    "H04 measured burst omitted its matching-key quiet preflight".into(),
                ));
            };
            quiet_windows.push(quiet_ms);
            preflight_matching_edges =
                preflight_matching_edges.saturating_add(evidence.preflight_matching_edges);
            foreign_matching_edges =
                foreign_matching_edges.saturating_add(evidence.foreign_matching_edges);
        }
    }
    set_hotkey_capture_purpose(HotkeyRunnerInputPurpose::ReadableCadence);
    run_readable_hotkey_transitions(child, anchor, trace_path, hotkey, hold_threshold_ms)?;
    let ids_digest = sha256_bytes(
        all_ids
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
            .as_bytes(),
    );
    let quiet_min = *quiet_windows.iter().min().unwrap_or(&0);
    let quiet_max = *quiet_windows.iter().max().unwrap_or(&0);
    if quiet_min < 75 || quiet_max > 100 {
        return Err(CaseFailure::new(
            FailureStage::InputInjection,
            format!("H04 quiet preflight fell outside 75–100ms: {quiet_min}..{quiet_max}"),
        ));
    }
    Ok(format_h04_matrix_evidence(
        hotkey,
        &ids_digest,
        quiet_min,
        quiet_max,
        preflight_matching_edges,
        foreign_matching_edges,
    ))
}

struct H04RetryOutcome<T> {
    attempts: u8,
    contamination_attempts: u8,
    contamination_artifact_names: Vec<String>,
    result: Result<T, CaseFailure>,
}

fn run_h04_matrix_with_retry<T, RunAttempt, PreserveAttempt>(
    mut run_attempt: RunAttempt,
    mut preserve_attempt: PreserveAttempt,
) -> H04RetryOutcome<T>
where
    RunAttempt: FnMut(u8) -> Result<T, CaseFailure>,
    PreserveAttempt: FnMut(u8, &CaseFailure) -> Result<String, CaseFailure>,
{
    let mut contamination_attempts = 0u8;
    let mut contamination_artifact_names = Vec::new();
    let mut contamination_causes = Vec::new();
    for attempt in 1..=2 {
        match run_attempt(attempt) {
            Ok(value) => {
                return H04RetryOutcome {
                    attempts: attempt,
                    contamination_attempts,
                    contamination_artifact_names,
                    result: Ok(value),
                };
            }
            Err(failure) if failure.input_contamination_group.is_some() => {
                contamination_attempts = contamination_attempts.saturating_add(1);
                contamination_causes.push(format!("attempt {attempt}: {}", failure.message));
                match preserve_attempt(attempt, &failure) {
                    Ok(name) => contamination_artifact_names.push(name),
                    Err(preserve_error) => {
                        return H04RetryOutcome {
                            attempts: attempt,
                            contamination_attempts,
                            contamination_artifact_names,
                            result: Err(CaseFailure::new(
                                failure.stage,
                                format!(
                                    "{}; could not preserve contaminated attempt, so no retry was started: {}",
                                    failure.message, preserve_error.message
                                ),
                            )),
                        };
                    }
                }
                if attempt == 2 {
                    return H04RetryOutcome {
                        attempts: attempt,
                        contamination_attempts,
                        contamination_artifact_names,
                        result: Err(CaseFailure::new(
                            failure.stage,
                            format!(
                                "both whole-matrix attempts were contaminated: {}",
                                contamination_causes.join(" || ")
                            ),
                        )),
                    };
                }
            }
            Err(mut failure) => {
                if !contamination_causes.is_empty() {
                    failure.message = format!(
                        "{}; previous whole-matrix attempt contamination retained: {}",
                        failure.message,
                        contamination_causes.join(" || ")
                    );
                }
                return H04RetryOutcome {
                    attempts: attempt,
                    contamination_attempts,
                    contamination_artifact_names,
                    result: Err(failure),
                };
            }
        }
    }
    unreachable!("bounded H04 retry loop always returns within two attempts")
}

fn persist_h04_contaminated_attempt(
    attempt: u8,
    hotkey: AcceptanceHotkey,
    failure: &CaseFailure,
    trace_path: &Path,
    output: &Path,
    completed_bursts: &[H04CompletedBurstEvidence],
) -> Result<PathBuf, CaseFailure> {
    let group_id = failure.input_contamination_group.ok_or_else(|| {
        CaseFailure::new(
            FailureStage::Environment,
            "refused to preserve an H04 attempt without typed contamination identity".into(),
        )
    })?;
    let terminal_cursor = trace_lines(trace_path).len();
    complete_hotkey_capture_segment(trace_path, terminal_cursor).map_err(|error| {
        CaseFailure::new(
            FailureStage::Environment,
            format!("could not close contaminated H04 trace segment: {error}"),
        )
    })?;
    let packet = finish_hotkey_evidence_capture("H04").ok_or_else(|| {
        CaseFailure::new(
            FailureStage::Environment,
            "contaminated H04 attempt had no partial typed evidence packet".into(),
        )
    })?;
    if packet.candidate_trace_overflow
        || packet.capture_segment_overflow
        || packet.runner_edge_overflow
        || packet.gesture_overflow
    {
        return Err(CaseFailure::new(
            FailureStage::Environment,
            "contaminated H04 attempt evidence overflowed and cannot be safely retried".into(),
        ));
    }
    let stream = packet
        .runner_edges
        .iter()
        .find(|edge| edge.input_group_id == group_id)
        .map(|edge| edge.stream)
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::Environment,
                "contaminated H04 attempt has no runner edges for its typed group".into(),
            )
        })?;
    let root_identity = packet
        .root_identities
        .iter()
        .find(|identity| identity.stream == stream)
        .cloned()
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::Environment,
                "contaminated H04 attempt has no matching ROOT identity".into(),
            )
        })?;
    let mut prior_group_ids = packet
        .runner_edges
        .iter()
        .map(|edge| edge.input_group_id)
        .collect::<Vec<_>>();
    prior_group_ids.sort_unstable();
    prior_group_ids.dedup();
    let artifact = H04InputContaminationArtifact {
        schema_version: 1,
        case_id: "H04".into(),
        attempt,
        hotkey,
        failure_stage: format!("{:?}", failure.stage),
        failure: bounded_text(&failure.message, MAX_RESULT_BYTES),
        declared_initial_state: false,
        next_matrix_burst_index: u8::try_from(completed_bursts.len() + 1).unwrap_or(u8::MAX),
        completed_bursts: completed_bursts.to_vec(),
        input_group_id: group_id,
        stream,
        prior_group_ids,
        owned_edges: packet
            .runner_edges
            .iter()
            .filter(|edge| edge.input_group_id == group_id && edge.runner_cookie_matched)
            .cloned()
            .collect(),
        foreign_edges: packet
            .runner_edges
            .iter()
            .filter(|edge| edge.input_group_id == group_id && !edge.runner_cookie_matched)
            .cloned()
            .collect(),
        candidate_events: packet
            .candidate_events
            .iter()
            .filter(|event| event.input_group_id == group_id && event.stream == stream)
            .cloned()
            .collect(),
        root_identity,
    };
    validate_h04_contamination_artifact(&artifact, hotkey).map_err(|error| {
        CaseFailure::new(
            FailureStage::Environment,
            format!("refused incomplete contaminated H04 attempt evidence: {error}"),
        )
    })?;
    let bytes = serde_json::to_vec(&artifact).map_err(|error| {
        CaseFailure::new(
            FailureStage::Environment,
            format!("encode contaminated H04 attempt evidence: {error}"),
        )
    })?;
    if bytes.is_empty() || bytes.len() > MAX_HOTKEY_CASE_EVIDENCE_BYTES {
        return Err(CaseFailure::new(
            FailureStage::Environment,
            format!(
                "contaminated H04 attempt artifact exceeds {} byte bound ({} bytes)",
                MAX_HOTKEY_CASE_EVIDENCE_BYTES,
                bytes.len()
            ),
        ));
    }
    let path = output.join(format!("case-H04-attempt-{attempt}-contamination.json"));
    super::super::write_new(&path, &bytes).map_err(|error| {
        CaseFailure::new(
            FailureStage::Environment,
            format!("persist contaminated H04 attempt evidence: {error}"),
        )
    })?;
    Ok(path)
}

fn attach_h04_contamination_artifact(report: &mut AcceptanceReport, path: PathBuf) {
    let path_text = bounded_text(&path.to_string_lossy(), MAX_PATH_BYTES);
    if !report
        .artifacts
        .iter()
        .any(|artifact| artifact == &path_text)
    {
        report.push_artifact(&path_text);
    }
    if let Some(case) = report.cases.iter_mut().find(|case| case.id == "H04")
        && !case.artifacts.iter().any(|artifact| artifact == &path_text)
    {
        case.artifacts.push(path_text);
    }
}

fn run_hotkey_behavior_cases(
    executable: &str,
    profile: &Path,
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) {
    run_hotkey_root_focus_case(
        report,
        "H01",
        child,
        anchor,
        trace_path,
        output,
        hotkey,
        hold_threshold_ms,
    );
    run_hotkey_root_focus_case(
        report,
        "H02",
        child,
        anchor,
        trace_path,
        output,
        hotkey,
        hold_threshold_ms,
    );
    run_hotkey_tap_dismiss_case(
        report,
        "H06",
        child,
        anchor,
        trace_path,
        output,
        hotkey,
        false,
        hold_threshold_ms,
        true,
    );
    run_hotkey_tap_dismiss_case(
        report,
        "H07",
        child,
        anchor,
        trace_path,
        output,
        hotkey,
        true,
        hold_threshold_ms,
        false,
    );
    run_hotkey_pending_open_case(
        profile,
        report,
        child,
        anchor,
        trace_path,
        output,
        hotkey,
        hold_threshold_ms,
    );
    run_hotkey_hold_matrix_cases(
        report,
        child,
        anchor,
        trace_path,
        output,
        hotkey,
        hold_threshold_ms,
    );
    run_hotkey_hold_close_case(
        report,
        child,
        anchor,
        trace_path,
        output,
        hotkey,
        hold_threshold_ms,
    );
    run_hotkey_designer_preservation_case(
        report,
        child,
        anchor,
        trace_path,
        output,
        hotkey,
        hold_threshold_ms,
    );
    run_hotkey_designer_preview_preservation_case(
        report,
        child,
        anchor,
        trace_path,
        output,
        hotkey,
        hold_threshold_ms,
    );
    run_hotkey_direct_trigger_preservation_case(
        executable,
        profile,
        report,
        child,
        anchor,
        trace_path,
        output,
        hotkey,
        hold_threshold_ms,
    );
    run_hotkey_hidden_root_wake_case(
        report,
        child,
        anchor,
        trace_path,
        output,
        hotkey,
        hold_threshold_ms,
    );
    run_hotkey_dual_profile_case(
        executable,
        profile,
        report,
        child,
        anchor,
        trace_path,
        output,
        hotkey,
        hold_threshold_ms,
    );
}

fn run_hotkey_root_focus_case(
    report: &mut AcceptanceReport,
    id: &str,
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) {
    let started = Instant::now();
    begin_hotkey_evidence_capture(id, trace_path);
    let result = (|| {
        let initially_visible = id == "H02";
        ensure_hotkey_root_visibility(child, anchor, hotkey, initially_visible)?;
        let root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        let (target_hwnd, target_pid) = if initially_visible {
            child
                .focus_window(&root)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            (root.hwnd, child.process_id())
        } else {
            anchor
                .focus()
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            (anchor.hwnd(), anchor.process_id())
        };
        focus_is_validated(target_hwnd, target_pid)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let cursor = trace_lines(trace_path).len();
        let tap = run_hotkey_burst_attempt_on_target(
            child,
            target_hwnd,
            target_pid,
            None,
            trace_path,
            hotkey,
            1,
            initially_visible,
            hold_threshold_ms,
        )?;
        if tap.final_visible == initially_visible {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "focused short tap did not toggle ROOT".into(),
            ));
        }
        if !runtime_windows(child).is_empty() {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "focused ROOT tap unexpectedly opened a runtime radial".into(),
            ));
        }
        if id == "H01" {
            let root = child
                .refresh_root()
                .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
            if !wait_until(ROOT_TIMEOUT, || {
                capture_foreground() == (root.hwnd, child.process_id())
            }) {
                return Err(CaseFailure::new(
                    FailureStage::NativeRootState,
                    "hidden ROOT became visible but was not focused by its normal show path".into(),
                ));
            }
            if !wait_root_visibility(child, true, ROOT_TIMEOUT) {
                return Err(CaseFailure::new(
                    FailureStage::NativeRootState,
                    "H01 ROOT was not visible on a physical display".into(),
                ));
            }
            Ok(format!(
                "evidence:v1; hotkey={}; initial_hidden=true; target=runner_owned; grid_visible=true; focused_root=true; radial=closed; invocation_ids={}",
                hotkey.as_str(),
                tap.invocation_ids.len()
            ))
        } else {
            std::thread::sleep(Duration::from_millis(750));
            if !wait_root_visibility(child, false, Duration::ZERO) {
                return Err(CaseFailure::new(
                    FailureStage::NativeRootState,
                    "focused ROOT hid but was restored during the stability interval".into(),
                ));
            }
            let later = trace_lines(trace_path)
                .into_iter()
                .skip(cursor)
                .filter(|line| {
                    line.contains("trace_event=\"native_activation\"")
                        && line.contains("edge=RestoreRequested")
                })
                .collect::<Vec<_>>();
            if !later.is_empty() {
                return Err(CaseFailure::new(
                    FailureStage::NativeRootState,
                    format!("newer hide was followed by restore activation: {later:?}"),
                ));
            }
            Ok(format!(
                "evidence:v1; hotkey={}; initial_visible=true; target=root_focused; grid_visible=false; stays_hidden=true; restore=none; invocation_ids={}",
                hotkey.as_str(),
                tap.invocation_ids.len()
            ))
        }
    })();
    append_case(
        report,
        id,
        expected(id),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn current_hotkey_target(
    child: &NativeChild,
    anchor: &FocusAnchor,
) -> Result<(HWND, u32), CaseFailure> {
    let current = capture_foreground();
    let owned = current == (anchor.hwnd(), anchor.process_id())
        || child
            .windows()
            .iter()
            .any(|window| (window.hwnd, window.process_id) == current);
    if !owned {
        return Err(CaseFailure::new(
            FailureStage::InputInjection,
            format!(
                "foreground HWND={} PID={} is not owned by the candidate or runner anchor",
                hwnd_id(current.0),
                current.1
            ),
        ));
    }
    focus_is_validated(current.0, current.1)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    Ok(current)
}

fn hover_executable_radial_cell(
    child: &NativeChild,
    surfaces: &[WindowSnapshot],
    trace_path: &Path,
) -> Result<u64, CaseFailure> {
    if !radial_surfaces_are_active(child, surfaces) {
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            "radial surfaces were inactive before hover setup".into(),
        ));
    }
    let surface = surfaces.first().ok_or_else(|| {
        CaseFailure::new(
            FailureStage::NativeRootState,
            "radial hold did not return a visible surface for hover setup".into(),
        )
    })?;
    let point = POINT {
        x: surface.bounds[0] + (surface.bounds[2] - surface.bounds[0]) / 2,
        y: surface.bounds[1] + (surface.bounds[3] - surface.bounds[1]) * 3 / 20,
    };
    let hover_cursor = trace_lines(trace_path).len();
    set_cursor_position(point)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let hover_events = wait_trace(trace_path, hover_cursor, TRACE_TIMEOUT, |events| {
        events
            .iter()
            .any(|line| trace_line_is_executable_hover(line))
    });
    let digest = hover_events
        .iter()
        .find(|line| trace_line_is_executable_hover(line))
        .and_then(|line| trace_field_value(line, "cell_digest"))
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|digest| *digest != 0)
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::GestureDecision,
                format!(
                    "hover did not receive production acknowledgment for an executable radial cell at ({},{}); events={hover_events:?}",
                    point.x, point.y
                ),
            )
        })?;
    if !radial_surfaces_are_active(child, surfaces) {
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            "radial surface closed before executable-cell hover acknowledgment".into(),
        ));
    }
    Ok(digest)
}

fn run_hotkey_tap_dismiss_case(
    report: &mut AcceptanceReport,
    id: &str,
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hotkey: AcceptanceHotkey,
    initial_visible: bool,
    hold_threshold_ms: u64,
    hover_radial: bool,
) {
    let started = Instant::now();
    begin_hotkey_evidence_capture(id, trace_path);
    let result = (|| {
        ensure_hotkey_root_visibility(child, anchor, hotkey, initial_visible)?;
        let surfaces = run_hotkey_hold_attempt(
            child,
            anchor,
            trace_path,
            hotkey,
            initial_visible,
            hold_threshold_ms,
            true,
            None,
        )?;
        let hover_digest = if hover_radial {
            Some(hover_executable_radial_cell(child, &surfaces, trace_path)?)
        } else {
            None
        };

        let before_trace = trace_lines(trace_path);
        let dispatch_count = before_trace
            .iter()
            .filter(|line| line.contains("trace_event=\"radial_dispatch_requested\""))
            .count();
        if !radial_surfaces_are_active(child, &surfaces) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "radial surfaces were not active immediately before tap dismissal".into(),
            ));
        }
        let (target_hwnd, target_pid) = current_hotkey_target(child, anchor)?;
        let tap = run_hotkey_burst_attempt_on_target_checked(
            child,
            target_hwnd,
            target_pid,
            None,
            trace_path,
            hotkey,
            1,
            initial_visible,
            hold_threshold_ms,
            || {
                if !radial_surfaces_are_active(child, &surfaces) {
                    return Err(CaseFailure::new(
                        FailureStage::NativeRootState,
                        "radial surfaces closed before the dismissal key-down".into(),
                    ));
                }
                if capture_foreground() != (target_hwnd, target_pid) {
                    return Err(CaseFailure::new(
                        FailureStage::InputInjection,
                        "foreground changed before the dismissal key-down".into(),
                    ));
                }
                Ok(())
            },
        )?;
        if tap.final_visible != !initial_visible {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!("short tap did not toggle ROOT from {initial_visible}"),
            ));
        }
        if !wait_until(ROOT_TIMEOUT, || {
            radial_surfaces_are_inactive(child, &surfaces)
        }) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "short tap did not dismiss active radial surfaces [{}]",
                    describe_radial_surfaces(&surfaces)
                ),
            ));
        }
        let after_trace = trace_lines(trace_path);
        let after_dispatch_count = after_trace
            .iter()
            .filter(|line| line.contains("trace_event=\"radial_dispatch_requested\""))
            .count();
        let after_tap = after_trace
            .iter()
            .skip(before_trace.len())
            .cloned()
            .collect::<Vec<_>>();
        if after_dispatch_count != dispatch_count
            || has_trace(&after_tap, "radial_dispatch_requested", &[])
        {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                "launcher tap dispatched a radial action instead of dismissing the runtime surface"
                    .into(),
            ));
        }
        let mut next_gesture = "not_applicable";
        if id == "H07" {
            let show_again = run_hotkey_burst_attempt(
                child,
                anchor,
                trace_path,
                hotkey,
                1,
                false,
                hold_threshold_ms,
            )?;
            let hide_again = run_hotkey_burst_attempt(
                child,
                anchor,
                trace_path,
                hotkey,
                1,
                true,
                hold_threshold_ms,
            )?;
            if !show_again.final_visible || hide_again.final_visible {
                return Err(CaseFailure::new(
                    FailureStage::NativeRootState,
                    "the next short gestures did not alternately show and hide ROOT".into(),
                ));
            }
            next_gesture = "usable_show_then_hide";
        }
        Ok(format!(
            "evidence:v1; hotkey={}; runtime_radial=dismissed; grid_visible={}; selection=none; dispatch=none; child_surface=closed; next_gesture={next_gesture}; hover_setup={}; hover_ack={}; hover_cell_digest={}",
            hotkey.as_str(),
            if id == "H06" { "true" } else { "false" },
            if hover_radial {
                "top_radial_cell_region"
            } else {
                "none"
            },
            if hover_digest.is_some() {
                "executable_cell"
            } else {
                "not_required"
            },
            hover_digest.map_or_else(|| "none".into(), |digest| digest.to_string()),
        ))
    })();
    append_case(
        report,
        id,
        expected(id),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_hotkey_pending_open_case(
    profile: &Path,
    report: &mut AcceptanceReport,
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) {
    let started = Instant::now();
    begin_hotkey_evidence_capture("H08", trace_path);
    let result = (|| {
        ensure_hotkey_root_visibility(child, anchor, hotkey, true)?;
        let root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        child
            .focus_window(&root)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        focus_is_validated(root.hwnd, child.process_id())
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;

        let mut hold = AcceptancePrepareHold::create(profile, "H08 runtime radial preparation")
            .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
        let hold_cursor = trace_lines(trace_path).len();
        let mut observer = RunnerHookObserver::start()
            .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
        let probe_id = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
        observer
            .pump_roundtrip(probe_id, Duration::from_millis(500))
            .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
        let keys = hotkey_observer_keys(hotkey);
        let hold_input = child
            .send_acceptance_hotkey(
                root.hwnd,
                child.process_id(),
                hotkey,
                Duration::from_millis(hold_threshold_ms.saturating_add(250).min(5_000)),
            )
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let hold_observation = observer.wait_for_chord_burst(&keys, 1, Duration::from_secs(5));
        capture_hotkey_attempt_evidence(
            child,
            HotkeyCandidateStream::MainCandidate,
            trace_path,
            hold_cursor,
            &hold_observation,
            HotkeyRunnerInputPurpose::LauncherChord,
        );
        observer
            .stop_and_report()
            .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
        if !hold_observation.exact_injected_pairs(1)
            || !hold_observation.exact_injected_sequence(&expected_hotkey_edges(hotkey, 1))
            || hold_input.observed_vks != keys
        {
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "H08 hold did not produce exact injected chord edges: {}",
                    hold_observation.describe()
                ),
            ));
        }
        let hold_timing = hold_observation
            .timing(hotkey, 1)
            .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
        if hold_timing.primary_hold_ms[0] < u128::from(hold_threshold_ms) {
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "H08 observed hold {}ms below threshold {hold_threshold_ms}ms",
                    hold_timing.primary_hold_ms[0]
                ),
            ));
        }
        input_modifiers_clear().map_err(|error| {
            CaseFailure::new(
                FailureStage::InputInjection,
                format!("H08 hold left modifier state uncleared: {error}"),
            )
        })?;

        let held_events = wait_trace(trace_path, hold_cursor, TRACE_TIMEOUT, |events| {
            events.iter().any(|line| {
                line.contains("trace_event=\"runtime_preparation\"")
                    && trace_field_value(line, "edge") == Some("GateHeld")
            })
        });
        let held = held_events
            .iter()
            .find(|line| {
                line.contains("trace_event=\"runtime_preparation\"")
                    && trace_field_value(line, "edge") == Some("GateHeld")
            })
            .cloned()
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::NativeRootState,
                    "real runtime preparation did not enter the bounded H08 service gate".into(),
                )
            })?;
        let invocation_id = trace_field_value(&held, "invocation_id")
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::GestureDecision,
                    "runtime preparation gate omitted its invocation identity".into(),
                )
            })?;
        let generation = trace_field_value(&held, "generation")
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::GestureDecision,
                    "runtime preparation gate omitted its generation identity".into(),
                )
            })?;
        if held_events.iter().any(|line| {
            line.contains("trace_event=\"short_tap\"")
                || line.contains("trace_event=\"desired_visibility\"")
        }) || !runtime_windows(child).is_empty()
            || capture_foreground() != (root.hwnd, child.process_id())
        {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "H08 hold changed ROOT or opened/focused a radial before tap cancellation".into(),
            ));
        }

        // GateHeld is the bounded terminal point for this deliberately pending
        // hold. Fence it before the separate cancellation tap begins.
        complete_hotkey_capture_at_current_trace(trace_path)?;
        let tap_cursor = trace_lines(trace_path).len();
        let (target_hwnd, target_pid) = current_hotkey_target(child, anchor)?;
        if target_hwnd != root.hwnd || target_pid != child.process_id() {
            return Err(CaseFailure::new(
                FailureStage::InputInjection,
                "H08 pre-Ready tap target was not the focused candidate ROOT".into(),
            ));
        }
        let mut tap_observer = RunnerHookObserver::start()
            .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
        let tap_probe = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
        tap_observer
            .pump_roundtrip(tap_probe, Duration::from_millis(500))
            .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
        let tap_input = child
            .send_acceptance_hotkey(target_hwnd, target_pid, hotkey, Duration::from_millis(25))
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let tap_observation = tap_observer.wait_for_chord_burst(&keys, 1, Duration::from_secs(2));
        capture_hotkey_attempt_evidence(
            child,
            HotkeyCandidateStream::MainCandidate,
            trace_path,
            tap_cursor,
            &tap_observation,
            HotkeyRunnerInputPurpose::LauncherChord,
        );
        tap_observer
            .stop_and_report()
            .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
        if !tap_observation.exact_injected_pairs(1)
            || !tap_observation.exact_injected_sequence(&expected_hotkey_edges(hotkey, 1))
            || tap_input.observed_vks.len() != keys.len()
        {
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "H08 cancellation tap did not produce exact injected chord edges: {}",
                    tap_observation.describe()
                ),
            ));
        }
        let tap_events = wait_trace(trace_path, tap_cursor, ROOT_TIMEOUT, |events| {
            let tap_id = events.iter().find_map(|line| {
                line.contains("trace_event=\"short_tap\"")
                    .then(|| trace_field_value(line, "invocation_id"))
                    .flatten()
            });
            tap_id.is_some_and(|tap_id| {
                events.iter().any(|line| {
                    line.contains("trace_event=\"desired_visibility\"")
                        && trace_field_value(line, "visible") == Some("false")
                        && trace_field_value(line, "invocation_id") == Some(tap_id)
                })
            })
        });
        let tap_id = tap_events
            .iter()
            .find(|line| line.contains("trace_event=\"short_tap\""))
            .and_then(|line| trace_field_value(line, "invocation_id"))
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::GestureDecision,
                    "H08 short tap did not emit its invocation identity".into(),
                )
            })?;
        let visibility = tap_events.iter().find(|line| {
            line.contains("trace_event=\"desired_visibility\"")
                && trace_field_value(line, "visible") == Some("false")
                && trace_field_value(line, "invocation_id") == Some(tap_id)
        });
        if visibility.is_none() {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "H08 short tap did not toggle the grid hidden before runtime preparation completed"
                    .into(),
            ));
        }
        let cancellation = wait_trace(trace_path, hold_cursor, TRACE_TIMEOUT, |events| {
            events.iter().any(|line| {
                runtime_preparation_matches(
                    line,
                    invocation_id,
                    generation,
                    "CancelledByLauncherTap",
                )
            })
        })
        .into_iter()
        .find(|line| {
            runtime_preparation_matches(line, invocation_id, generation, "CancelledByLauncherTap")
        })
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::GestureDecision,
                "H08 tap did not cancel the exact pending runtime invocation".into(),
            )
        })?;
        let tap_and_cancel_events = wait_trace(trace_path, hold_cursor, TRACE_TIMEOUT, |events| {
            let tap_index = events
                .iter()
                .position(|line| line.contains("trace_event=\"short_tap\""));
            let visibility_index = events.iter().position(|line| {
                line.contains("trace_event=\"desired_visibility\"")
                    && trace_field_value(line, "visible") == Some("false")
                    && trace_field_value(line, "invocation_id") == Some(tap_id)
            });
            let cancellation_index = events.iter().position(|line| {
                runtime_preparation_matches(
                    line,
                    invocation_id,
                    generation,
                    "CancelledByLauncherTap",
                )
            });
            matches!((tap_index, visibility_index, cancellation_index), (Some(tap), Some(visibility), Some(cancelled)) if tap < visibility && tap < cancelled)
        });

        let release_cursor = trace_lines(trace_path).len();
        let hold_release = hold
            .release()
            .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?;
        let late_events = wait_trace(trace_path, release_cursor, TRACE_TIMEOUT, |events| {
            ["GateReleased", "ReplyQueued", "ReplyRejected"]
                .into_iter()
                .all(|edge| {
                    events.iter().any(|line| {
                        runtime_preparation_matches(line, invocation_id, generation, edge)
                    })
                })
        });
        for edge in ["GateReleased", "ReplyQueued", "ReplyRejected"] {
            if !late_events
                .iter()
                .any(|line| runtime_preparation_matches(line, invocation_id, generation, edge))
            {
                return Err(CaseFailure::new(
                    FailureStage::GestureDecision,
                    format!("H08 late runtime reply omitted correlated {edge} evidence"),
                ));
            }
        }
        let canceled_index = tap_and_cancel_events
            .iter()
            .position(|line| line == &cancellation)
            .unwrap_or(usize::MAX);
        let release_index = late_events
            .iter()
            .position(|line| {
                runtime_preparation_matches(line, invocation_id, generation, "GateReleased")
            })
            .unwrap_or(usize::MAX);
        let queued_index = late_events
            .iter()
            .position(|line| {
                runtime_preparation_matches(line, invocation_id, generation, "ReplyQueued")
            })
            .unwrap_or(usize::MAX);
        let rejected_index = late_events
            .iter()
            .position(|line| {
                runtime_preparation_matches(line, invocation_id, generation, "ReplyRejected")
            })
            .unwrap_or(usize::MAX);
        if canceled_index == usize::MAX
            || release_index >= queued_index
            || queued_index >= rejected_index
        {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                "H08 cancellation/late-reply trace edges were out of order".into(),
            ));
        }
        if !wait_until(ROOT_TIMEOUT, || {
            wait_root_visibility(child, false, Duration::ZERO) && runtime_windows(child).is_empty()
        }) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "H08 late preparation reopened radial or failed to complete the tap's grid hide"
                    .into(),
            ));
        }
        complete_hotkey_capture_at_current_trace(trace_path)?;
        Ok(format!(
            "evidence:v1; hotkey={}; hold_opened=true; gate=real_runtime_radial_prepare; tap_before_ready=true; cancelled_by_tap=true; late_reply=rejected; native_ready=none; radial_reopened=none; grid_hidden=true; prepared_invocation={invocation_id}; tap_invocation={tap_id}; generation={generation}; key_cleanup=verified; hold_release={hold_release}",
            hotkey.as_str()
        ))
    })();
    append_case(
        report,
        "H08",
        expected("H08"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_hotkey_hold_matrix_cases(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) {
    let started = Instant::now();
    begin_hotkey_evidence_capture("H09", trace_path);
    let result = (|| {
        for initial_visible in [false, true] {
            ensure_hotkey_root_visibility(child, anchor, hotkey, initial_visible)?;
            let surfaces = run_hotkey_hold_attempt(
                child,
                anchor,
                trace_path,
                hotkey,
                initial_visible,
                hold_threshold_ms,
                true,
                None,
            )?;
            let closed = run_hotkey_hold_attempt(
                child,
                anchor,
                trace_path,
                hotkey,
                initial_visible,
                hold_threshold_ms,
                false,
                Some(&surfaces),
            )?;
            if !closed.is_empty() || !wait_root_visibility(child, initial_visible, ROOT_TIMEOUT) {
                return Err(CaseFailure::new(
                    FailureStage::NativeRootState,
                    format!(
                        "hold/release changed ROOT or failed to close radial from visible={initial_visible}"
                    ),
                ));
            }
        }
        Ok(format!(
            "evidence:v1; hotkey={}; initial_visible=hidden+visible; radial_opened=true; grid_visibility_unchanged=true; release_no_toggle=true; observed_hold_ms>={hold_threshold_ms}; release_keys=clear",
            hotkey.as_str()
        ))
    })();
    append_case(
        report,
        "H09",
        expected("H09"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_hotkey_hold_close_case(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) {
    let started = Instant::now();
    begin_hotkey_evidence_capture("H10", trace_path);
    let result = (|| {
        let root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        let initial_visible = root_is_physically_visible(child, &root)?;
        ensure_hotkey_root_visibility(child, anchor, hotkey, initial_visible)?;
        let trace_cursor = trace_lines(trace_path).len();
        let surfaces = run_hotkey_hold_attempt(
            child,
            anchor,
            trace_path,
            hotkey,
            initial_visible,
            hold_threshold_ms,
            true,
            None,
        )?;
        let hover_digest = hover_executable_radial_cell(child, &surfaces, trace_path)?;
        let closed = run_hotkey_hold_attempt(
            child,
            anchor,
            trace_path,
            hotkey,
            initial_visible,
            hold_threshold_ms,
            false,
            Some(&surfaces),
        )?;
        if !closed.is_empty() || !wait_root_visibility(child, initial_visible, ROOT_TIMEOUT) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "second hold did not close radial while preserving ROOT visibility".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(400));
        if !runtime_windows(child).is_empty() {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "radial surface reopened after hold close and key release".into(),
            ));
        }
        let case_events = trace_lines(trace_path)
            .into_iter()
            .skip(trace_cursor)
            .collect::<Vec<_>>();
        if has_trace(&case_events, "radial_dispatch_requested", &[]) {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                "hovered radial action dispatched during the H10 hold-close sequence".into(),
            ));
        }
        input_modifiers_clear()
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        Ok(format!(
            "evidence:v1; hotkey={}; radial_close=hold; hover_ack=executable_cell; hover_cell_digest={hover_digest}; release_keys=clear; grid_toggle=none; selection=none; dispatch=none; late_reopen=none",
            hotkey.as_str(),
        ))
    })();
    append_case(
        report,
        "H10",
        expected("H10"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_hotkey_hold_attempt(
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    hotkey: AcceptanceHotkey,
    initial_visible: bool,
    hold_threshold_ms: u64,
    expect_active: bool,
    known_surfaces: Option<&[WindowSnapshot]>,
) -> Result<Vec<WindowSnapshot>, CaseFailure> {
    if !wait_root_visibility(child, initial_visible, ROOT_TIMEOUT) {
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            format!("hold precondition ROOT visibility was not {initial_visible}"),
        ));
    }
    let root = child
        .refresh_root()
        .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
    let target = if known_surfaces.is_some() {
        let surfaces = known_surfaces.unwrap_or_default();
        if !radial_surfaces_are_active(child, surfaces) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "hold-close preflight found that radial surfaces were already inactive".into(),
            ));
        }
        current_hotkey_target(child, anchor)?
    } else if initial_visible {
        child
            .focus_window(&root)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        (root.hwnd, child.process_id())
    } else {
        anchor
            .focus()
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        (anchor.hwnd(), anchor.process_id())
    };
    focus_is_validated(target.0, target.1)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let cursor_before =
        cursor_position().map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let before_surfaces = runtime_windows(child);
    let preserved_surfaces = known_surfaces.unwrap_or_default();
    if expect_active
        && before_surfaces.iter().any(|surface| {
            !preserved_surfaces
                .iter()
                .any(|known| known.process_id == surface.process_id && known.hwnd == surface.hwnd)
        })
    {
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            "hold-open precondition contains an unexpected active radial surface".into(),
        ));
    }
    if expect_active
        && !preserved_surfaces.is_empty()
        && !radial_surfaces_are_active(child, preserved_surfaces)
    {
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            "preserved authoring preview surfaces were inactive before runtime hold".into(),
        ));
    }
    let mut observer = RunnerHookObserver::start()
        .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
    if !observer.desktop.eq_ignore_ascii_case("Default") {
        return Err(CaseFailure::new(
            FailureStage::HookAdmission,
            format!(
                "runner observer is attached to desktop {:?}",
                observer.desktop
            ),
        ));
    }
    let probe = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
    observer
        .pump_roundtrip(probe, Duration::from_millis(500))
        .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
    if let Some(known) = known_surfaces {
        if !radial_surfaces_are_active(child, known) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "radial surfaces closed during hold-close preflight".into(),
            ));
        }
    }
    focus_is_validated(target.0, target.1)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let trace_before_hold = trace_lines(trace_path);
    let prior_visibility_revision = latest_desired_visibility_revision(&trace_before_hold);
    let trace_cursor = trace_before_hold.len();
    let down_time = Duration::from_millis(hold_threshold_ms.saturating_add(250).min(5_000));
    let input = child
        .send_acceptance_hotkey(target.0, target.1, hotkey, down_time)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let post_release_pump = observer.pump_roundtrip(
        NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed),
        Duration::from_millis(500),
    );
    let post_release_foreground = capture_foreground();
    let post_release_desktop = input_desktop_evidence();
    let post_release_key_state = child
        .verify_acceptance_hotkey_released(hotkey)
        .and_then(|release_state| input_modifiers_clear().map(|_| release_state));
    let keys = match hotkey {
        AcceptanceHotkey::F11 => vec![0x7A],
        AcceptanceHotkey::ShiftAltWinEnd => vec![0xA0, 0xA4, 0x5B, 0x23],
    };
    let observation = observer.wait_for_chord_burst(&keys, 1, Duration::from_secs(5));
    capture_hotkey_attempt_evidence(
        child,
        HotkeyCandidateStream::MainCandidate,
        trace_path,
        trace_cursor,
        &observation,
        HotkeyRunnerInputPurpose::LauncherChord,
    );
    let observation_text = observation.describe();
    let exact_edges = observation.exact_injected_pairs(1)
        && observation.exact_injected_sequence(&expected_hotkey_edges(hotkey, 1));
    observer
        .stop_and_report()
        .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
    if let Err(error) = &post_release_key_state {
        return Err(CaseFailure::new(
            FailureStage::InputInjection,
            format!(
                "owned hotkey key state was not clear after release cleanup: {error}; release=[{}]; foreground_after={}:{}; desktop_after={post_release_desktop:?}; hook_pump_after={post_release_pump:?}; observer={observation_text}",
                input.describe(),
                hwnd_id(post_release_foreground.0),
                post_release_foreground.1,
            ),
        ));
    }
    if !exact_edges {
        return Err(CaseFailure::new(
            FailureStage::HookAdmission,
            format!(
                "hold did not produce exact owned injected chord edges: {observation_text}; release=[{}]; foreground_after={}:{}; desktop_after={post_release_desktop:?}; hook_pump_after={post_release_pump:?}; key_state_after={post_release_key_state:?}",
                input.describe(),
                hwnd_id(post_release_foreground.0),
                post_release_foreground.1,
            ),
        ));
    }
    let timing = observation
        .timing(hotkey, 1)
        .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
    let observed_hold = timing.primary_hold_ms.first().copied().unwrap_or_default();
    if observed_hold < u128::from(hold_threshold_ms) {
        return Err(CaseFailure::new(
            FailureStage::HookAdmission,
            format!("observed primary key hold {observed_hold}ms is below {hold_threshold_ms}ms"),
        ));
    }
    let events = wait_trace(trace_path, trace_cursor, TRACE_TIMEOUT, |events| {
        has_trace(
            events,
            "hook_primary",
            &["transition=Press", "provenance=ExternalInjected"],
        ) && has_trace(
            events,
            "hook_primary",
            &["transition=Release", "provenance=ExternalInjected"],
        ) && has_trace(
            events,
            "configured_primary",
            &[
                "transition=Press",
                "provenance=ExternalInjected",
                "modifiers_match=true",
            ],
        ) && has_trace(
            events,
            "configured_primary",
            &[
                "transition=Release",
                "provenance=ExternalInjected",
                "modifiers_match=true",
            ],
        )
    });
    if !has_trace(
        &events,
        "hook_primary",
        &["transition=Press", "provenance=ExternalInjected"],
    ) || !has_trace(
        &events,
        "hook_primary",
        &["transition=Release", "provenance=ExternalInjected"],
    ) || !has_trace(
        &events,
        "configured_primary",
        &[
            "transition=Press",
            "provenance=ExternalInjected",
            "modifiers_match=true",
        ],
    ) || !has_trace(
        &events,
        "configured_primary",
        &[
            "transition=Release",
            "provenance=ExternalInjected",
            "modifiers_match=true",
        ],
    ) {
        return Err(CaseFailure::new(
            FailureStage::HookAdmission,
            format!(
                "production hook did not acknowledge both hold edges; observed={observation_text}; trace={events:?}"
            ),
        ));
    }
    let hold_invocation_id = events
        .iter()
        .find(|line| {
            line.contains("trace_event=\"hook_primary\"") && line.contains("transition=Press")
        })
        .and_then(|line| trace_field_value(line, "invocation_id"))
        .and_then(|value| value.parse::<u64>().ok());
    if has_trace(&events, "short_tap", &[])
        || trace_contains_hold_visibility_work(
            &events,
            prior_visibility_revision,
            hold_invocation_id,
        )
    {
        return Err(CaseFailure::new(
            FailureStage::GestureDecision,
            format!(
                "hold/release emitted short-tap or correlated ROOT visibility work; prior_revision={prior_visibility_revision:?}; hold_invocation={hold_invocation_id:?}; events={events:?}"
            ),
        ));
    }
    if has_trace(&events, "radial_dispatch_requested", &[]) {
        return Err(CaseFailure::new(
            FailureStage::GestureDecision,
            "hold gesture dispatched an action while opening or closing the radial".into(),
        ));
    }
    let surfaces = if expect_active {
        let surfaces = wait_runtime_windows(child, &before_surfaces, ROOT_TIMEOUT)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        validate_radial_surfaces(child, &surfaces)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        if !preserved_surfaces.is_empty() && !radial_surfaces_are_active(child, preserved_surfaces)
        {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "runtime hold changed the preserved authoring preview surfaces".into(),
            ));
        }
        surfaces
    } else {
        let known = known_surfaces.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::NativeRootState,
                "hold-close requires the active surface set".into(),
            )
        })?;
        if !wait_until(ROOT_TIMEOUT, || radial_surfaces_are_inactive(child, known)) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "hold did not close radial surfaces [{}]",
                    describe_radial_surfaces(known)
                ),
            ));
        }
        Vec::new()
    };
    if !wait_root_visibility(child, initial_visible, ROOT_TIMEOUT) {
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            "hold changed ROOT visibility".into(),
        ));
    }
    let cursor_after =
        cursor_position().map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    if cursor_before.x != cursor_after.x || cursor_before.y != cursor_after.y {
        return Err(CaseFailure::new(
            FailureStage::InputInjection,
            format!(
                "cursor moved during hold: before=({},{}), after=({}, {})",
                cursor_before.x, cursor_before.y, cursor_after.x, cursor_after.y
            ),
        ));
    }
    complete_hotkey_capture_at_current_trace(trace_path)?;
    Ok(surfaces)
}

fn root_is_physically_visible(
    child: &NativeChild,
    root: &WindowSnapshot,
) -> Result<bool, CaseFailure> {
    let displays = native_display_bounds()
        .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
    if root.hwnd != child.root().hwnd || root.process_id != child.process_id() {
        return Err(CaseFailure::new(
            FailureStage::WindowDiscovery,
            "ROOT HWND/PID changed during hotkey behavior case".into(),
        ));
    }
    Ok(root.visible && !root.minimized && intersects_display_bounds(root.bounds, &displays))
}

fn ensure_hotkey_root_visibility(
    child: &NativeChild,
    anchor: &FocusAnchor,
    hotkey: AcceptanceHotkey,
    desired_visible: bool,
) -> Result<(), CaseFailure> {
    let root = child
        .refresh_root()
        .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
    if root_is_physically_visible(child, &root)? == desired_visible {
        return Ok(());
    }
    anchor
        .focus()
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    child
        .send_acceptance_hotkey(
            anchor.hwnd(),
            anchor.process_id(),
            hotkey,
            Duration::from_millis(25),
        )
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    if wait_root_visibility(child, desired_visible, ROOT_TIMEOUT) {
        Ok(())
    } else {
        Err(CaseFailure::new(
            FailureStage::NativeRootState,
            format!("setup short tap did not establish ROOT visibility={desired_visible}"),
        ))
    }
}

fn ensure_designer_semantic_target_selected(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
    target: DesignerSemanticTarget,
) -> Result<DesignerSemanticTargetState, CaseFailure> {
    let baseline = wait_for_designer_semantic_target_in_session(
        trace_path,
        target,
        session_id,
        UIA_TIMEOUT,
        |_| true,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerNativeTarget,
            format!("Designer did not publish its {target:?} target in session {session_id}"),
        )
    })?;
    if baseline.selected {
        return Ok(baseline);
    }
    let cursor = trace_lines(trace_path).len();
    let click = click_designer_client_bounds(child, designer, baseline.bounds, trace_path)
        .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
    if matches!(target, DesignerSemanticTarget::DefaultMenu) {
        return wait_for_designer_semantic_target_in_session(
            trace_path,
            target,
            session_id,
            TRACE_TIMEOUT,
            |state| state.selected,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerFrameworkInput,
                format!(
                    "checked native click did not select {target:?} in Designer session {session_id}; click=[{}]",
                    click.describe()
                ),
            )
        });
    }
    let events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
        checked_designer_toggle_transition(events, target, baseline, true).is_some()
    });
    checked_designer_toggle_transition(&events, target, baseline, true).ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerFrameworkInput,
            format!(
                "checked native click did not select {target:?} in Designer session {session_id}; click=[{}]",
                click.describe()
            ),
        )
    })
}

fn select_starter_menu_name_target(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
) -> Result<DesignerSemanticTargetState, CaseFailure> {
    for target in [
        DesignerSemanticTarget::Menus,
        DesignerSemanticTarget::Tree,
        DesignerSemanticTarget::Inspector,
        DesignerSemanticTarget::DefaultMenu,
    ] {
        ensure_designer_semantic_target_selected(child, designer, trace_path, session_id, target)?;
    }
    wait_for_designer_semantic_target_in_session(
        trace_path,
        DesignerSemanticTarget::MenuName,
        session_id,
        UIA_TIMEOUT,
        |_| true,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerNativeTarget,
            format!(
                "selected starter menu did not publish its Menu Name target in session {session_id}"
            ),
        )
    })
}

fn cleanup_hotkey_designer(
    child: &NativeChild,
    trace_path: &Path,
    tracked: Option<&(WindowSnapshot, u64)>,
    name_bounds: Option<[i32; 4]>,
) -> Result<(), CaseFailure> {
    let Some(current) = child.designer() else {
        return Ok(());
    };
    let (window, session_id) = tracked
        .map(|(window, session_id)| (window, *session_id))
        .unwrap_or((&current, 0));
    if current.hwnd != window.hwnd || current.process_id != child.process_id() {
        return Err(CaseFailure::new(
            FailureStage::Cleanup,
            format!(
                "refusing to close an unexpected Designer HWND: tracked={} current={} PID={}",
                hwnd_id(window.hwnd),
                hwnd_id(current.hwnd),
                current.process_id
            ),
        ));
    }

    let restore_result = if let Some(bounds) = name_bounds {
        if session_id == 0 {
            Err(CaseFailure::new(
                FailureStage::Cleanup,
                "cannot restore the H11 Menu Name without its accepted Designer session".into(),
            ))
        } else {
            (|| {
                click_designer_client_bounds(child, window, bounds, trace_path)
                    .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?;
                wait_for_designer_semantic_target_in_session(
                    trace_path,
                    DesignerSemanticTarget::MenuName,
                    session_id,
                    TRACE_TIMEOUT,
                    |state| state.focused,
                )
                .ok_or_else(|| {
                    CaseFailure::new(
                        FailureStage::Cleanup,
                        "H11 cleanup could not focus Menu Name to restore the fixture draft".into(),
                    )
                })?;
                let cursor = trace_lines(trace_path).len();
                replace_focused_designer_text(child, window, DESIGNER_STARTER_NAME)
                    .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?;
                let restored = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
                    has_trace(
                        events,
                        "designer_edit_state",
                        &["input_matches_model=true", "draft_dirty=false"],
                    )
                });
                if !has_trace(
                    &restored,
                    "designer_edit_state",
                    &["input_matches_model=true", "draft_dirty=false"],
                ) {
                    return Err(CaseFailure::new(
                        FailureStage::Cleanup,
                        "H11 cleanup did not restore the clean starter Menu Name".into(),
                    ));
                }
                Ok(())
            })()
        }
    } else {
        Ok(())
    };

    let close_result = request_window_close(child, &current)
        .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))
        .and_then(|()| {
            if wait_until(ROOT_TIMEOUT, || child.designer().is_none()) {
                Ok(())
            } else {
                Err(CaseFailure::new(
                    FailureStage::Cleanup,
                    "H11 Designer HWND remained after bounded clean close".into(),
                ))
            }
        });
    match (restore_result, close_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Err(restore), Err(close)) => Err(CaseFailure::new(
            FailureStage::Cleanup,
            format!(
                "{}; Designer close also failed: {}",
                restore.message, close.message
            ),
        )),
    }
}

fn run_hotkey_designer_preservation_case(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) {
    let started = Instant::now();
    begin_hotkey_evidence_capture("H11", trace_path);
    let mut tracked_designer = child.designer().map(|window| (window, 0));
    let mut name_bounds = None;
    let proof = (|| {
        ensure_hotkey_root_visibility(child, anchor, hotkey, true)?;
        let uia = UiAutomation::new()
            .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
        let entry = run_designer_entry(child, &uia, anchor, trace_path)?;
        tracked_designer = Some((entry.window.clone(), entry.session_id));
        let name =
            select_starter_menu_name_target(child, &entry.window, trace_path, entry.session_id)?;
        name_bounds = Some(name.bounds);
        let name_click =
            click_designer_client_bounds(child, &entry.window, name.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
        wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::MenuName,
            entry.session_id,
            TRACE_TIMEOUT,
            |state| state.focused,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerFrameworkInput,
                "Menu Name did not receive production keyboard focus".into(),
            )
        })?;
        let dirty_cursor = trace_lines(trace_path).len();
        replace_focused_designer_text(child, &entry.window, "M1 hotkey draft")
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let dirty = wait_trace(trace_path, dirty_cursor, TRACE_TIMEOUT, |events| {
            has_trace(
                events,
                "designer_edit_state",
                &["input_matches_model=true", "draft_dirty=true"],
            )
        });
        if !has_trace(
            &dirty,
            "designer_edit_state",
            &["input_matches_model=true", "draft_dirty=true"],
        ) {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "native Menu Name edit did not create a dirty Designer draft".into(),
            ));
        }
        let dirty_state =
            wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |state| {
                state.session_id == entry.session_id
            })
            .map_err(|error| CaseFailure::new(FailureStage::DesignerMutation, error))?;
        let baseline_digest = dirty_state.draft_cell_ids_digest;
        let trace_cursor = trace_lines(trace_path).len();
        let mut invocation_ids = std::collections::BTreeSet::new();
        for (initial_visible, taps) in [(true, 2usize), (true, 1usize), (false, 2usize)] {
            focus_is_validated(entry.window.hwnd, child.process_id()).map_err(|error| {
                CaseFailure::new(
                    FailureStage::InputInjection,
                    format!("Designer lost foreground before repeated taps: {error}"),
                )
            })?;
            let burst = run_hotkey_burst_attempt_on_target_with_policy(
                child,
                entry.window.hwnd,
                child.process_id(),
                None,
                trace_path,
                hotkey,
                taps,
                initial_visible,
                hold_threshold_ms,
                VisibleBurstSettlePolicy::PreserveForeground {
                    target_hwnd: hwnd_id(entry.window.hwnd),
                    target_process_id: child.process_id(),
                },
            )?;
            let expected_final_visible = initial_visible ^ (taps % 2 == 1);
            if burst.final_visible != expected_final_visible {
                return Err(CaseFailure::new(
                    FailureStage::NativeRootState,
                    format!(
                        "Designer burst parity expected ROOT visible={expected_final_visible}, observed {}",
                        burst.final_visible
                    ),
                ));
            }
            for invocation_id in burst.invocation_ids {
                if !invocation_ids.insert(invocation_id) {
                    return Err(CaseFailure::new(
                        FailureStage::GestureDecision,
                        format!("Designer burst repeated invocation ID {invocation_id}"),
                    ));
                }
            }
            let (foreground, foreground_pid) = capture_foreground();
            if foreground != entry.window.hwnd || foreground_pid != child.process_id() {
                return Err(CaseFailure::new(
                    FailureStage::InputInjection,
                    format!(
                        "hotkey forced ordinary focus away from Designer: HWND={} PID={foreground_pid}",
                        hwnd_id(foreground)
                    ),
                ));
            }
        }
        if invocation_ids.len() != 5 {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                format!(
                    "expected five distinct tap decisions across visible/hidden Designer bursts, observed {}",
                    invocation_ids.len()
                ),
            ));
        }
        let after_lines = trace_lines(trace_path);
        let designer_lines = after_lines
            .iter()
            .skip(trace_cursor)
            .filter(|line| {
                line.contains("trace_event=\"designer_close\"")
                    || line.contains("trace_event=\"designer_submitted\"")
            })
            .cloned()
            .collect::<Vec<_>>();
        if !designer_lines.is_empty() {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!("hotkey taps submitted or closed the Designer: {designer_lines:?}"),
            ));
        }
        if after_lines
            .iter()
            .skip(trace_cursor)
            .any(|line| line.contains("trace_event=\"radial_dispatch_requested\""))
        {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                "Designer hotkey tap dispatched an action instead of only toggling ROOT".into(),
            ));
        }
        let current_designer = child.designer().ok_or_else(|| {
            CaseFailure::new(
                FailureStage::WindowDiscovery,
                "Designer HWND closed during repeated hotkey taps".into(),
            )
        })?;
        if current_designer.hwnd != entry.window.hwnd {
            return Err(CaseFailure::new(
                FailureStage::WindowDiscovery,
                "Designer session HWND changed during repeated hotkey taps".into(),
            ));
        }
        let after_state =
            wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |state| {
                state.session_id == entry.session_id
                    && state.draft_cell_ids_digest == baseline_digest
            })
            .map_err(|error| CaseFailure::new(FailureStage::DesignerMutation, error))?;
        if after_state.draft_cell_ids_digest != baseline_digest {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Designer draft cell identity digest changed during hotkey taps".into(),
            ));
        }
        let latest_edit = after_lines
            .iter()
            .rev()
            .find(|line| line.contains("trace_event=\"designer_edit_state\""));
        if !latest_edit.is_some_and(|line| {
            line.contains("draft_dirty=true") && line.contains("input_matches_model=true")
        }) {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "dirty Designer draft was lost during repeated hotkey taps".into(),
            ));
        }

        replace_focused_designer_text(child, &entry.window, DESIGNER_STARTER_NAME)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let restored = wait_trace(trace_path, trace_cursor, TRACE_TIMEOUT, |events| {
            has_trace(
                events,
                "designer_edit_state",
                &["input_matches_model=true", "draft_dirty=false"],
            )
        });
        if !has_trace(
            &restored,
            "designer_edit_state",
            &["input_matches_model=true", "draft_dirty=false"],
        ) {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "temporary dirty draft value was not restored to the deterministic clean state"
                    .into(),
            ));
        }

        let current_designer = child.designer().ok_or_else(|| {
            CaseFailure::new(
                FailureStage::WindowDiscovery,
                "Designer disappeared before clean post-proof close".into(),
            )
        })?;
        if current_designer.hwnd != entry.window.hwnd {
            return Err(CaseFailure::new(
                FailureStage::WindowDiscovery,
                "Designer HWND changed before clean post-proof close".into(),
            ));
        }
        request_window_close(child, &current_designer)
            .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?;
        if !wait_until(ROOT_TIMEOUT, || child.designer().is_none()) {
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                "clean Designer window did not close after preserving the dirty-draft test evidence".into(),
            ));
        }
        Ok(format!(
            "evidence:v1; hotkey={}; designer_session_preserved=true; draft_digest_preserved=true; save_discard_close=none; ordinary_focus_forced=false; repeated_taps=5; designer_start_states=visible+hidden; hidden_start_burst=true; designer_cleanup=closed_cleanly; distinct_invocation_ids={}; setup_click=[{}]; temporary_draft_restored=true",
            hotkey.as_str(),
            invocation_ids.len(),
            name_click.describe()
        ))
    })();
    let cleanup =
        cleanup_hotkey_designer(child, trace_path, tracked_designer.as_ref(), name_bounds);
    let result = match (proof, cleanup) {
        (Ok(evidence), Ok(())) => Ok(evidence),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
        (Err(error), Err(cleanup_error)) => Err(CaseFailure::new(
            error.stage,
            format!(
                "{}; H11 finally cleanup failed: {}",
                error.message, cleanup_error.message
            ),
        )),
    };
    append_case(
        report,
        "H11",
        expected("H11"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_hotkey_designer_preview_preservation_case(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) {
    let started = Instant::now();
    begin_hotkey_evidence_capture("H12", trace_path);
    let mut designer: Option<(WindowSnapshot, u64)> = None;
    let mut name_bounds = None;
    let mut draft_generation = None;
    let mut draft_digest = None;
    let mut preview_attempted = false;
    let mut preview_generation = None;
    let mut preview_baseline = std::collections::BTreeSet::new();
    let mut preview_surfaces = Vec::new();
    let mut runtime_surfaces = Vec::new();
    let mut runtime_hold_attempted = false;

    let proof = (|| {
        ensure_hotkey_root_visibility(child, anchor, hotkey, true)?;
        // Keep an owned Designer HWND available to the finally-style cleanup even
        // when fresh Designer entry is rejected before returning a session.
        designer = child.designer().map(|window| (window, 0));
        let uia = UiAutomation::new()
            .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
        let entry = run_designer_entry(child, &uia, anchor, trace_path)?;
        designer = Some((entry.window.clone(), entry.session_id));
        let name =
            select_starter_menu_name_target(child, &entry.window, trace_path, entry.session_id)?;
        name_bounds = Some(name.bounds);
        click_designer_client_bounds(child, &entry.window, name.bounds, trace_path)
            .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
        wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::MenuName,
            entry.session_id,
            TRACE_TIMEOUT,
            |state| state.focused,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerFrameworkInput,
                "H12 Menu Name did not receive native keyboard focus".into(),
            )
        })?;
        let dirty_cursor = trace_lines(trace_path).len();
        replace_focused_designer_text(child, &entry.window, "M1 H12 preview draft")
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let dirty = wait_trace(trace_path, dirty_cursor, TRACE_TIMEOUT, |events| {
            has_trace(
                events,
                "designer_edit_state",
                &["input_matches_model=true", "draft_dirty=true"],
            )
        });
        if !has_trace(
            &dirty,
            "designer_edit_state",
            &["input_matches_model=true", "draft_dirty=true"],
        ) {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "H12 native Menu Name edit did not produce a dirty draft".into(),
            ));
        }
        let dirty_state =
            wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |state| {
                state.session_id == entry.session_id
            })
            .map_err(|error| CaseFailure::new(FailureStage::DesignerMutation, error))?;
        draft_generation = Some(dirty_state.generation);
        draft_digest = Some(dirty_state.draft_cell_ids_digest);

        preview_baseline = visible_radial_host_windows(child);
        if !preview_baseline.is_empty() || !runtime_windows(child).is_empty() {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "H12 requires an empty radial surface baseline before starting native preview"
                    .into(),
            ));
        }
        let preview_cursor = trace_lines(trace_path).len();
        preview_attempted = true;
        click_authoring_target(
            child,
            &entry.window,
            trace_path,
            entry.session_id,
            AuthoringControlTarget::OpenDesktopPreview,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
        let preview = wait_for_terminal_authoring_request(
            trace_path,
            preview_cursor,
            entry.session_id,
            "StartNativePreview",
            UIA_TIMEOUT,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "H12 native authoring preview did not receive its correlated start reply".into(),
            )
        })?;
        preview_generation = Some(preview.identity.generation);
        preview_surfaces = wait_runtime_windows(child, &[], TRACE_TIMEOUT)
            .map_err(|error| CaseFailure::new(FailureStage::DesignerPresentation, error))?;
        if preview_surfaces.len() < 2 || visible_radial_host_windows(child) == preview_baseline {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "H12 authoring preview did not create a visible native radial surface set".into(),
            ));
        }

        // Only classify newly opened radial HWNDs as runtime-owned after the
        // preview set has been captured and the runtime hold phase begins.
        runtime_hold_attempted = true;
        let hold_attempt = run_hotkey_hold_attempt(
            child,
            anchor,
            trace_path,
            hotkey,
            true,
            hold_threshold_ms,
            true,
            Some(&preview_surfaces),
        );
        let mut known_preview_surfaces = preview_baseline.clone();
        known_preview_surfaces.extend(preview_surfaces.iter().map(|surface| hwnd_id(surface.hwnd)));
        runtime_surfaces =
            visible_radial_host_snapshots(child, &known_preview_surfaces, runtime_hold_attempted);
        let runtime = hold_attempt?;
        runtime_surfaces = runtime.clone();
        validate_radial_surfaces(child, &runtime)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        if !radial_surfaces_are_active(child, &runtime)
            || !radial_surfaces_are_active(child, &preview_surfaces)
            || visible_radial_host_windows(child).len() < preview_baseline.len() + 4
        {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "H12 runtime hold did not coexist with the original authoring preview HWND set"
                    .into(),
            ));
        }

        let target = current_hotkey_target(child, anchor)?;
        let tap = run_hotkey_burst_attempt_on_target_checked(
            child,
            target.0,
            target.1,
            None,
            trace_path,
            hotkey,
            1,
            true,
            hold_threshold_ms,
            || {
                if !radial_surfaces_are_active(child, &runtime) || capture_foreground() != target {
                    return Err(CaseFailure::new(
                        FailureStage::NativeRootState,
                        "H12 runtime radial or its owned foreground target changed before tap dismissal".into(),
                    ));
                }
                Ok(())
            },
        )?;
        if tap.final_visible
            || !wait_until(ROOT_TIMEOUT, || {
                radial_surface_set_matches_active_state(
                    &runtime,
                    &child.windows(),
                    child.process_id(),
                    false,
                ) && radial_surfaces_are_active(child, &preview_surfaces)
            })
            || visible_radial_host_windows(child)
                != preview_surfaces
                    .iter()
                    .map(|window| hwnd_id(window.hwnd))
                    .collect()
        {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "H12 tap did not close runtime radial while preserving the authoring preview HWNDs"
                    .into(),
            ));
        }

        let current_designer = child.designer().ok_or_else(|| {
            CaseFailure::new(
                FailureStage::WindowDiscovery,
                "H12 Designer closed during runtime tap dismissal".into(),
            )
        })?;
        if current_designer.hwnd != entry.window.hwnd
            || current_designer.process_id != child.process_id()
            || !uia.root_is_queryable(entry.window.hwnd, child.process_id())
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "H12 runtime toggle changed or invalidated the same-session Designer window".into(),
            ));
        }
        let state_after =
            wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |state| {
                state.session_id == entry.session_id
                    && Some(state.draft_cell_ids_digest) == draft_digest
                    && Some(state.generation) == draft_generation
            })
            .map_err(|error| CaseFailure::new(FailureStage::DesignerMutation, error))?;
        if Some(state_after.draft_cell_ids_digest) != draft_digest
            || Some(state_after.generation) != draft_generation
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "H12 runtime tap changed the dirty draft revision or cell identity digest".into(),
            ));
        }
        let latest_edit = trace_lines(trace_path)
            .into_iter()
            .rev()
            .find(|line| line.contains("trace_event=\"designer_edit_state\""));
        if !latest_edit.is_some_and(|line| {
            line.contains("draft_dirty=true") && line.contains("input_matches_model=true")
        }) {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "H12 runtime tap lost the dirty Designer input/model state".into(),
            ));
        }
        let case_events = trace_lines(trace_path)
            .into_iter()
            .skip(preview_cursor)
            .collect::<Vec<_>>();
        if case_events.iter().any(|line| {
            line.contains("trace_event=\"designer_close\"")
                || line.contains("trace_event=\"designer_submitted\"")
                || line.contains("trace_event=\"radial_dispatch_requested\"")
        }) {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                "H12 runtime hold/tap submitted Designer state, closed the editor, or dispatched a radial action".into(),
            ));
        }
        Ok(format!(
            "evidence:v1; hotkey={}; designer_dirty=true; native_preview=active; runtime_hold_opened=true; runtime_tap_dismissed=true; preview_survived=true; preview_baseline_preserved=true; runtime_surfaces_new=true; designer_session_preserved=true; draft_digest_preserved=true; draft_generation={}; draft_cell_ids_digest={}; preview_hwnds={}; runtime_hwnds={}; invocation_ids={}",
            hotkey.as_str(),
            dirty_state.generation,
            dirty_state.draft_cell_ids_digest,
            preview_surfaces.len(),
            runtime.len(),
            tap.invocation_ids.len()
        ))
    })();

    let mut cleanup_errors = Vec::new();
    if let Some((designer_window, session_id)) = designer.as_ref() {
        if runtime_hold_attempted && runtime_surfaces.is_empty() {
            let mut known_preview_surfaces = preview_baseline.clone();
            known_preview_surfaces
                .extend(preview_surfaces.iter().map(|surface| hwnd_id(surface.hwnd)));
            runtime_surfaces = visible_radial_host_snapshots(
                child,
                &known_preview_surfaces,
                runtime_hold_attempted,
            );
        }
        if !runtime_surfaces.is_empty() && radial_surfaces_are_active(child, &runtime_surfaces) {
            attempt_cleanup_step(&mut cleanup_errors, "runtime radial", || {
                let root = child
                    .refresh_root()
                    .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
                let initial_visible = root_is_physically_visible(child, &root)?;
                let (target_hwnd, target_pid) = current_hotkey_target(child, anchor)?;
                run_hotkey_burst_attempt_on_target_checked(
                    child,
                    target_hwnd,
                    target_pid,
                    None,
                    trace_path,
                    hotkey,
                    1,
                    initial_visible,
                    hold_threshold_ms,
                    || {
                        if !radial_surfaces_are_active(child, &runtime_surfaces)
                            || capture_foreground() != (target_hwnd, target_pid)
                        {
                            return Err(CaseFailure::new(
                                FailureStage::Cleanup,
                                "H12 cleanup could not validate the active runtime radial before dismissal".into(),
                            ));
                        }
                        Ok(())
                    },
                )?;
                if !wait_until(ROOT_TIMEOUT, || {
                    radial_surface_set_matches_active_state(
                        &runtime_surfaces,
                        &child.windows(),
                        child.process_id(),
                        false,
                    )
                }) {
                    return Err(CaseFailure::new(
                        FailureStage::Cleanup,
                        "H12 cleanup tap did not close the runtime radial".into(),
                    ));
                }
                Ok(())
            });
        }

        if preview_attempted {
            attempt_cleanup_step(&mut cleanup_errors, "native preview stop", || {
                child
                    .focus_window(designer_window)
                    .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?;
                let generation = preview_generation.or(draft_generation).ok_or_else(|| {
                    CaseFailure::new(
                        FailureStage::Cleanup,
                        "H12 preview cleanup has no captured Designer generation".into(),
                    )
                })?;
                let stop_cursor = trace_lines(trace_path).len();
                tab_and_activate_authoring_control(
                    child,
                    designer_window,
                    trace_path,
                    *session_id,
                    generation,
                    AuthoringControlTarget::StopDesktopPreview,
                )
                .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?;
                wait_for_terminal_authoring_request(
                    trace_path,
                    stop_cursor,
                    *session_id,
                    "StopNativePreview",
                    UIA_TIMEOUT,
                )
                .ok_or_else(|| {
                    CaseFailure::new(
                        FailureStage::Cleanup,
                        "H12 preview stop did not receive its same-session terminal reply".into(),
                    )
                })?;
                Ok(())
            });
            if !wait_until(Duration::from_secs(2), || {
                visible_radial_host_windows(child) == preview_baseline
            }) {
                cleanup_errors.push("native preview windows remained visible after stop".into());
            }
        }

        if let Some(bounds) = name_bounds {
            attempt_cleanup_step(&mut cleanup_errors, "Designer draft restore", || {
                click_designer_client_bounds(child, designer_window, bounds, trace_path)
                    .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?;
                wait_for_designer_semantic_target_in_session(
                    trace_path,
                    DesignerSemanticTarget::MenuName,
                    *session_id,
                    TRACE_TIMEOUT,
                    |state| state.focused,
                )
                .ok_or_else(|| {
                    CaseFailure::new(
                        FailureStage::Cleanup,
                        "H12 cleanup could not focus Menu Name to restore the deterministic draft"
                            .into(),
                    )
                })?;
                let restored_cursor = trace_lines(trace_path).len();
                replace_focused_designer_text(child, designer_window, DESIGNER_STARTER_NAME)
                    .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?;
                let restored = wait_trace(trace_path, restored_cursor, TRACE_TIMEOUT, |events| {
                    has_trace(
                        events,
                        "designer_edit_state",
                        &["input_matches_model=true", "draft_dirty=false"],
                    )
                });
                if !has_trace(
                    &restored,
                    "designer_edit_state",
                    &["input_matches_model=true", "draft_dirty=false"],
                ) {
                    return Err(CaseFailure::new(
                        FailureStage::Cleanup,
                        "H12 cleanup did not restore the deterministic clean Designer draft".into(),
                    ));
                }
                Ok(())
            });
        }

        attempt_cleanup_step(&mut cleanup_errors, "request Designer close", || {
            request_window_close(child, designer_window)
                .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))
        });
        attempt_cleanup_step(&mut cleanup_errors, "verify Designer close", || {
            if wait_until(ROOT_TIMEOUT, || child.designer().is_none()) {
                Ok(())
            } else {
                Err(CaseFailure::new(
                    FailureStage::Cleanup,
                    "clean Designer close did not retire its HWND".into(),
                ))
            }
        });
    }
    let cleanup = if cleanup_errors.is_empty() {
        Ok(())
    } else {
        Err(CaseFailure::new(
            FailureStage::Cleanup,
            cleanup_errors.join("; "),
        ))
    };

    let result = match (proof, cleanup) {
        (Ok(evidence), Ok(())) => Ok(format!(
            "{evidence}; preview_cleanup=stopped; designer_cleanup=closed_cleanly"
        )),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
        (Err(error), Err(cleanup_error)) => Err(CaseFailure::new(
            error.stage,
            format!(
                "{}; H12 cleanup also failed: {}",
                error.message, cleanup_error.message
            ),
        )),
    };
    append_case(
        report,
        "H12",
        expected("H12"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_hotkey_direct_trigger_preservation_case(
    executable: &str,
    profile: &Path,
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) {
    let started = Instant::now();
    begin_hotkey_evidence_capture("H16", trace_path);
    let mut fallback_artifacts = Vec::new();
    let mut result = (|| {
        ensure_hotkey_root_visibility(child, anchor, hotkey, true)?;
        if child.designer().is_some() || !runtime_windows(child).is_empty() {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "H16 requires a clean Designer and runtime radial baseline".into(),
            ));
        }

        let direct = run_acceptance_direct_trigger(
            child,
            anchor,
            trace_path,
            &runtime_windows(child),
            0x54,
        )?;
        validate_radial_surfaces(child, &direct)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        if !radial_surfaces_are_active(child, &direct)
            || !wait_root_visibility(child, true, Duration::ZERO)
        {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "H16 direct trigger did not open runtime radial while preserving visible ROOT"
                    .into(),
            ));
        }

        let first_target = current_hotkey_target(child, anchor)?;
        let first_cursor = trace_lines(trace_path).len();
        let first_tap = run_hotkey_burst_attempt_on_target_checked(
            child,
            first_target.0,
            first_target.1,
            None,
            trace_path,
            hotkey,
            1,
            true,
            hold_threshold_ms,
            || {
                if !radial_surfaces_are_active(child, &direct)
                    || capture_foreground() != first_target
                {
                    return Err(CaseFailure::new(
                        FailureStage::NativeRootState,
                        "H16 runtime radial or owned foreground changed before launcher tap".into(),
                    ));
                }
                Ok(())
            },
        )?;
        let first_events = trace_lines(trace_path)
            .into_iter()
            .skip(first_cursor)
            .collect::<Vec<_>>();
        if first_tap.final_visible
            || !wait_until(ROOT_TIMEOUT, || {
                radial_surfaces_are_inactive(child, &direct)
            })
            || !first_events.iter().any(|line| {
                line.contains("trace_event=\"desired_visibility\"")
                    && trace_field_value(line, "visible") == Some("false")
                    && first_tap.invocation_ids.iter().any(|id| {
                        trace_field_value(line, "invocation_id")
                            .and_then(|value| value.parse::<u64>().ok())
                            == Some(*id)
                    })
            })
            || first_events
                .iter()
                .any(|line| line.contains("trace_event=\"radial_dispatch_requested\""))
        {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                "H16 normal launcher tap did not dismiss runtime radial, toggle grid, and avoid selection".into(),
            ));
        }

        let root_after_hide = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        if root_is_physically_visible(child, &root_after_hide)? {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "H16 first launcher tap did not leave ROOT physically hidden".into(),
            ));
        }
        let second = run_acceptance_direct_trigger(
            child,
            anchor,
            trace_path,
            &runtime_windows(child),
            0x54,
        )?;
        validate_radial_surfaces(child, &second)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        let root_after_direct = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        if !radial_surfaces_are_active(child, &second)
            || root_is_physically_visible(child, &root_after_direct)?
        {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "H16 direct trigger was not preserved after the launcher tap hid ROOT".into(),
            ));
        }

        let second_target = current_hotkey_target(child, anchor)?;
        let second_tap = run_hotkey_burst_attempt_on_target_checked(
            child,
            second_target.0,
            second_target.1,
            None,
            trace_path,
            hotkey,
            1,
            false,
            hold_threshold_ms,
            || {
                if !radial_surfaces_are_active(child, &second)
                    || capture_foreground() != second_target
                {
                    return Err(CaseFailure::new(
                        FailureStage::Cleanup,
                        "H16 runtime radial or owned foreground changed before cleanup tap".into(),
                    ));
                }
                Ok(())
            },
        )?;
        if !second_tap.final_visible
            || !wait_until(ROOT_TIMEOUT, || {
                radial_surfaces_are_inactive(child, &second)
            })
            || !wait_root_visibility(child, true, ROOT_TIMEOUT)
        {
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                "H16 cleanup launcher tap did not dismiss radial and restore the grid".into(),
            ));
        }
        let (fallback, artifacts) = h16_fallback_result_for_case(run_hotkey_legacy_fallback_case(
            executable, profile, output, anchor, hotkey,
        ));
        fallback_artifacts = artifacts;
        let fallback = fallback?;
        Ok(format!(
            "evidence:v1; hotkey={}; direct_trigger=opened; native_launcher_tap=dismissed+grid_toggled; legacy_launcher_tap=dismissed+grid_toggled; legacy_source=HotkeyTrigger+LegacyTrigger; direct_trigger_preserved=true; root_refreshed_after_direct=true; root_start_visible=true; root_hide_then_show=true; direct_chord=Ctrl+Alt+T; tap_invocations={}; legacy_child_pid={}; legacy_profile_sha256={}; legacy_profile_cleanup=verified; legacy_trace_artifact={}; legacy_profile_artifact={}",
            hotkey.as_str(),
            first_tap.invocation_ids.len() + second_tap.invocation_ids.len(),
            fallback.child_process_id,
            fallback.profile_sha256,
            fallback.artifacts[0].display(),
            fallback.artifacts[1].display(),
        ))
    })();
    if let Err(error) = complete_open_hotkey_capture_at_current_trace(trace_path) {
        result = Err(match result {
            Ok(_) => error,
            Err(mut existing) => {
                existing.message = format!(
                    "{}; could not materialize H16 trace before fallback cleanup: {}",
                    existing.message, error.message
                );
                existing
            }
        });
    }
    append_h16_case_result(
        report,
        started,
        result,
        Some(child),
        output,
        trace_path,
        &fallback_artifacts,
    );
}

struct LegacyFallbackEvidence {
    child_process_id: u32,
    profile_sha256: String,
    artifacts: Vec<PathBuf>,
}

struct LegacyFallbackFailure {
    failure: CaseFailure,
    artifacts: Vec<PathBuf>,
}

impl From<CaseFailure> for LegacyFallbackFailure {
    fn from(failure: CaseFailure) -> Self {
        Self {
            failure,
            artifacts: Vec::new(),
        }
    }
}

fn h16_fallback_result_for_case(
    result: Result<LegacyFallbackEvidence, LegacyFallbackFailure>,
) -> (Result<LegacyFallbackEvidence, CaseFailure>, Vec<PathBuf>) {
    match result {
        Ok(evidence) => {
            let artifacts = evidence.artifacts.clone();
            (Ok(evidence), artifacts)
        }
        Err(failure) => (Err(failure.failure), failure.artifacts),
    }
}

fn legacy_fallback_trace_path(profile: &Path) -> PathBuf {
    profile.join("acceptance.log")
}

fn run_hotkey_legacy_fallback_case(
    executable: &str,
    profile: &Path,
    output: &Path,
    anchor: &FocusAnchor,
    primary_hotkey: AcceptanceHotkey,
) -> Result<LegacyFallbackEvidence, LegacyFallbackFailure> {
    let legacy_hotkey = match primary_hotkey {
        AcceptanceHotkey::F11 => AcceptanceHotkey::ShiftAltWinEnd,
        AcceptanceHotkey::ShiftAltWinEnd => AcceptanceHotkey::F11,
    };
    let alternate_profile = tempfile::Builder::new()
        .prefix("radial-acceptance-h16-legacy-")
        .tempdir_in(profile)
        .map_err(|error| CaseFailure::new(FailureStage::Environment, error.to_string()))?;
    let trace_path = legacy_fallback_trace_path(alternate_profile.path());
    register_hotkey_candidate_stream(&trace_path, HotkeyCandidateStream::LegacyFallbackCandidate);
    let log_path = trace_path.clone();
    let fixture = super::super::deterministic_fixture_for_hotkey_with_direct_trigger_chord(
        &log_path,
        super::super::MouseGestureMode::Enabled,
        legacy_hotkey,
        "Ctrl+Alt+Y",
        false,
    )
    .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
    for (name, bytes) in [
        ("settings.json", fixture.settings_json.as_slice()),
        ("radial.json", fixture.radial_json.as_slice()),
        ("actions.json", fixture.actions_json.as_slice()),
    ] {
        super::super::write_new(&alternate_profile.path().join(name), bytes)
            .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
    }
    let profile_sha256 = super::super::sha256_bytes(
        &[
            fixture.settings_json.as_slice(),
            fixture.radial_json.as_slice(),
            fixture.actions_json.as_slice(),
        ]
        .concat(),
    );
    let mut child = NativeChild::launch(
        Path::new(executable),
        alternate_profile.path(),
        &log_path,
        &alternate_profile.path().join("child.stdout.log"),
        &alternate_profile.path().join("child.stderr.log"),
    )
    .map_err(|error| {
        CaseFailure::new(
            FailureStage::Environment,
            format!("launch isolated legacy-route H16 profile: {error}"),
        )
    })?;
    let child_process_id = child.process_id();
    let proof = (|| {
        if !wait_root_visibility(&child, true, ROOT_TIMEOUT) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "legacy-route H16 ROOT did not become physically visible".into(),
            ));
        }
        if !runtime_windows(&child).is_empty() {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "legacy-route H16 runtime radial baseline was not empty".into(),
            ));
        }
        let direct = run_acceptance_direct_trigger(
            &child,
            anchor,
            &trace_path,
            &runtime_windows(&child),
            0x59,
        )?;
        if !radial_surfaces_are_active(&child, &direct) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "legacy-route Ctrl+Alt+Y did not open a runtime radial".into(),
            ));
        }

        let target = current_hotkey_target(&child, anchor)?;
        let keys = hotkey_observer_keys(legacy_hotkey);
        let mut observer = RunnerHookObserver::start()
            .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
        let probe = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
        observer
            .pump_roundtrip(probe, Duration::from_millis(500))
            .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
        if !radial_surfaces_are_active(&child, &direct) || capture_foreground() != target {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "legacy-route radial or owned foreground changed before launcher key-down".into(),
            ));
        }
        let tap_cursor = trace_lines(&trace_path).len();
        let input = child
            .send_acceptance_hotkey(
                target.0,
                target.1,
                legacy_hotkey,
                Duration::from_millis(100),
            )
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let observation = observer.wait_for_chord_burst(&keys, 1, Duration::from_secs(3));
        capture_hotkey_attempt_evidence(
            &child,
            HotkeyCandidateStream::LegacyFallbackCandidate,
            &trace_path,
            tap_cursor,
            &observation,
            HotkeyRunnerInputPurpose::LauncherChord,
        );
        observer
            .stop_and_report()
            .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
        if !observation.exact_injected_pairs(1)
            || !observation.exact_injected_sequence(&expected_hotkey_edges(legacy_hotkey, 1))
            || input.observed_vks != keys
        {
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "legacy-route launcher tap edges were not exact: {}",
                    observation.describe()
                ),
            ));
        }
        input_modifiers_clear().map_err(|error| {
            CaseFailure::new(
                FailureStage::InputInjection,
                format!("legacy-route key cleanup failed: {error}"),
            )
        })?;
        let legacy_visibility = wait_trace(&trace_path, tap_cursor, ROOT_TIMEOUT, |events| {
            events.iter().any(|line| {
                line.contains("trace_event=\"desired_visibility\"")
                    && trace_field_value(line, "source") == Some("LegacyTrigger")
                    && trace_field_value(line, "visible") == Some("false")
                    && trace_field_value(line, "invocation_id") == Some("none")
            })
        });
        if !legacy_visibility.iter().any(|line| {
            line.contains("trace_event=\"desired_visibility\"")
                && trace_field_value(line, "source") == Some("LegacyTrigger")
                && trace_field_value(line, "visible") == Some("false")
                && trace_field_value(line, "invocation_id") == Some("none")
        }) || !wait_root_visibility(&child, false, ROOT_TIMEOUT)
            || !wait_until(ROOT_TIMEOUT, || {
                radial_surfaces_are_inactive(&child, &direct)
            })
            || trace_lines(&trace_path)
                .iter()
                .skip(tap_cursor)
                .any(|line| line.contains("trace_event=\"radial_dispatch_requested\""))
        {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                "legacy HotkeyTrigger did not dismiss radial and produce a LegacyTrigger grid hide without selection".into(),
            ));
        }
        complete_hotkey_capture_at_current_trace(&trace_path)?;
        Ok(())
    })();

    let mut cleanup_errors = Vec::new();
    if let Err(error) = complete_open_hotkey_capture_at_current_trace(&trace_path) {
        cleanup_errors.push(format!(
            "materialize legacy-route trace before temporary profile cleanup: {}",
            error.message
        ));
    }
    if let Ok(root) = child.refresh_root() {
        if let Err(error) = request_window_close(&child, &root) {
            cleanup_errors.push(format!("request legacy-route child close: {error}"));
        }
    }
    let mut status = wait_child(&mut child, Duration::from_secs(5));
    if status.is_none() {
        if let Err(error) = child.kill() {
            cleanup_errors.push(format!("terminate legacy-route child: {error}"));
        }
        status = wait_child(&mut child, Duration::from_secs(5));
    }
    if status.is_none_or(|status| !status.success()) {
        cleanup_errors.push(format!(
            "legacy-route child exited unsuccessfully: {status:?}"
        ));
    }
    if !child.windows().is_empty() {
        cleanup_errors.push("legacy-route child left owned HWNDs after exit".into());
    }

    let (mut artifacts, trace_copy_errors) =
        persist_h16_legacy_trace_artifacts(&trace_path, output);
    cleanup_errors.extend(trace_copy_errors);
    if let Err(error) = alternate_profile.close() {
        cleanup_errors.push(format!("remove legacy-route temporary profile: {error}"));
    }
    let receipt_artifact = output.join("case-H16-legacy-profile.json");
    let proof_error = proof.as_ref().err().map(|error| {
        serde_json::json!({
            "stage": format!("{:?}", error.stage),
            "message": error.message,
        })
    });
    let receipt = serde_json::json!({
        "child_process_id": child_process_id,
        "configured_hotkey": legacy_hotkey.as_str(),
        "direct_trigger": "Ctrl+Alt+Y",
        "shared_tap_hold": false,
        "route": "HotkeyTrigger -> LegacyTrigger",
        "profile_sha256": profile_sha256,
        "child_exited_successfully": status.is_some_and(NativeExitStatus::success),
        "child_owned_windows_closed": child.windows().is_empty(),
        "proof_error": proof_error,
        "trace_artifacts": artifacts.iter().map(|path| path.to_string_lossy().to_string()).collect::<Vec<_>>(),
        "cleanup_errors": cleanup_errors.clone(),
    });
    match serde_json::to_vec_pretty(&receipt) {
        Ok(bytes) => match fs::write(&receipt_artifact, bytes) {
            Ok(()) => artifacts.push(receipt_artifact),
            Err(error) => {
                cleanup_errors.push(format!("write legacy-route profile receipt: {error}"));
            }
        },
        Err(error) => {
            cleanup_errors.push(format!("serialize legacy-route profile receipt: {error}"))
        }
    }
    match (proof, cleanup_errors.is_empty()) {
        (Ok(()), true) => Ok(LegacyFallbackEvidence {
            child_process_id,
            profile_sha256,
            artifacts,
        }),
        (Ok(()), false) => Err(LegacyFallbackFailure {
            failure: CaseFailure::new(
                FailureStage::Cleanup,
                format!(
                    "legacy-route H16 cleanup/evidence persistence failed: {}",
                    cleanup_errors.join("; ")
                ),
            ),
            artifacts,
        }),
        (Err(error), true) => Err(LegacyFallbackFailure {
            failure: error,
            artifacts,
        }),
        (Err(error), false) => Err(LegacyFallbackFailure {
            failure: CaseFailure::new(
                error.stage,
                format!(
                    "{}; legacy-route cleanup/evidence persistence failed: {}",
                    error.message,
                    cleanup_errors.join("; ")
                ),
            ),
            artifacts,
        }),
    }
}

fn record_h16_fallback_artifacts(report: &mut AcceptanceReport, artifacts: &[PathBuf]) {
    for artifact in artifacts {
        report.push_artifact(artifact.to_string_lossy());
    }
    if let Some(case) = report.cases.iter_mut().find(|case| case.id == "H16") {
        for artifact in artifacts {
            case.artifacts
                .push(bounded_text(&artifact.to_string_lossy(), MAX_PATH_BYTES));
        }
    }
}

fn append_h16_case_result(
    report: &mut AcceptanceReport,
    started: Instant,
    result: Result<String, CaseFailure>,
    child: Option<&NativeChild>,
    output: &Path,
    trace_path: &Path,
    fallback_artifacts: &[PathBuf],
) {
    append_case(
        report,
        "H16",
        expected("H16"),
        started,
        result,
        child,
        output,
        trace_path,
    );
    record_h16_fallback_artifacts(report, fallback_artifacts);
}

fn persist_h16_legacy_trace_artifacts(source: &Path, output: &Path) -> (Vec<PathBuf>, Vec<String>) {
    let primary = output.join("case-H16-legacy-trace.log");
    match copy_bounded_trace_file(source, &primary) {
        Ok(()) => (vec![primary], Vec::new()),
        Err(primary_error) => {
            let mut errors = vec![format!(
                "copy legacy-route trace evidence to {}: {primary_error}",
                primary.display()
            )];
            let recovery = output.join("case-H16-legacy-trace-recovered.log");
            let artifacts = match copy_bounded_trace_file(source, &recovery) {
                Ok(()) => vec![recovery],
                Err(recovery_error) => {
                    errors.push(format!(
                        "recover legacy-route trace evidence to {}: {recovery_error}",
                        recovery.display()
                    ));
                    Vec::new()
                }
            };
            (artifacts, errors)
        }
    }
}

fn copy_bounded_trace_file(source: &Path, destination: &Path) -> Result<(), String> {
    let mut source_file = File::open(source).map_err(|error| error.to_string())?;
    let length = source_file
        .metadata()
        .map_err(|error| error.to_string())?
        .len();
    if length > MAX_TRACE_BYTES as u64 {
        source_file
            .seek(SeekFrom::End(-(MAX_TRACE_BYTES as i64)))
            .map_err(|error| error.to_string())?;
    }
    let mut bytes = Vec::with_capacity(length.min(MAX_TRACE_BYTES as u64) as usize);
    source_file
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_TRACE_BYTES {
        bytes.drain(..bytes.len() - MAX_TRACE_BYTES);
    }
    let mut destination_file = File::create(destination).map_err(|error| error.to_string())?;
    destination_file
        .write_all(&bytes)
        .map_err(|error| error.to_string())
}

fn run_acceptance_direct_trigger(
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    before: &[WindowSnapshot],
    trigger_key: u32,
) -> Result<Vec<WindowSnapshot>, CaseFailure> {
    anchor
        .focus()
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    focus_is_validated(anchor.hwnd(), anchor.process_id())
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let mut observer = RunnerHookObserver::start()
        .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
    let probe = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
    observer
        .pump_roundtrip(probe, Duration::from_millis(500))
        .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
    let cursor = trace_lines(trace_path).len();
    let input = child
        .send_acceptance_direct_trigger(
            anchor.hwnd(),
            anchor.process_id(),
            trigger_key as u16,
            Duration::from_millis(40),
        )
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let keys = vec![0xA2, 0xA4, trigger_key];
    let observation = observer.wait_for_chord_burst(&keys, 1, Duration::from_secs(3));
    capture_hotkey_attempt_evidence(
        child,
        HotkeyCandidateStream::MainCandidate,
        trace_path,
        cursor,
        &observation,
        HotkeyRunnerInputPurpose::DirectTrigger,
    );
    observer
        .stop_and_report()
        .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
    let expected = [
        (0xA2, true),
        (0xA4, true),
        (trigger_key, true),
        (trigger_key, false),
        (0xA4, false),
        (0xA2, false),
    ];
    if !observation.exact_injected_pairs(1)
        || !observation.exact_injected_sequence(&expected)
        || input.observed_vks != keys
    {
        return Err(CaseFailure::new(
            FailureStage::HookAdmission,
            format!(
                "H16 direct-trigger chord was not exact: {}",
                observation.describe()
            ),
        ));
    }
    input_modifiers_clear().map_err(|error| {
        CaseFailure::new(
            FailureStage::InputInjection,
            format!("H16 direct-trigger cleanup failed: {error}"),
        )
    })?;
    let surfaces = wait_runtime_windows(child, before, TRACE_TIMEOUT)
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    if trace_lines(trace_path)
        .iter()
        .skip(cursor)
        .any(|line| line.contains("trace_event=\"radial_dispatch_requested\""))
    {
        return Err(CaseFailure::new(
            FailureStage::GestureDecision,
            "H16 direct trigger unexpectedly dispatched an action".into(),
        ));
    }
    complete_hotkey_capture_at_current_trace(trace_path)?;
    Ok(surfaces)
}

fn run_hotkey_hidden_root_wake_case(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) {
    let started = Instant::now();
    begin_hotkey_evidence_capture("H18", trace_path);
    let result = (|| {
        ensure_hotkey_root_visibility(child, anchor, hotkey, false)?;
        if child.designer().is_some() {
            return Err(CaseFailure::new(
                FailureStage::WindowDiscovery,
                "Designer must be closed before hidden-root wake case".into(),
            ));
        }
        let parked = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        let displays = native_display_bounds()
            .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
        if intersects_display_bounds(parked.bounds, &displays) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "hidden ROOT is still on a physical display: {:?}",
                    parked.bounds
                ),
            ));
        }
        let pointer = cursor_position()
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let surfaces = run_hotkey_hold_attempt(
            child,
            anchor,
            trace_path,
            hotkey,
            false,
            hold_threshold_ms,
            true,
            None,
        )?;
        if !wait_until(ROOT_TIMEOUT, || {
            radial_surfaces_are_active(child, &surfaces)
        }) || !wait_root_visibility(child, false, ROOT_TIMEOUT)
            || !child.designer().is_none()
        {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "hold on parked ROOT did not leave radial active, ROOT parked, and Designer closed"
                    .into(),
            ));
        }
        if !radial_surfaces_are_active(child, &surfaces) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "parked-root radial closed before wake tap preflight".into(),
            ));
        }
        let (target_hwnd, target_pid) = current_hotkey_target(child, anchor)?;
        let tap = run_hotkey_burst_attempt_on_target_checked(
            child,
            target_hwnd,
            target_pid,
            None,
            trace_path,
            hotkey,
            1,
            false,
            hold_threshold_ms,
            || {
                if !radial_surfaces_are_active(child, &surfaces) {
                    return Err(CaseFailure::new(
                        FailureStage::NativeRootState,
                        "parked-root radial closed before the wake key-down".into(),
                    ));
                }
                if capture_foreground() != (target_hwnd, target_pid) {
                    return Err(CaseFailure::new(
                        FailureStage::InputInjection,
                        "foreground changed before parked-root wake key-down".into(),
                    ));
                }
                Ok(())
            },
        )?;
        if !tap.final_visible
            || !wait_until(ROOT_TIMEOUT, || {
                radial_surfaces_are_inactive(child, &surfaces)
            })
        {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "short tap did not wake ROOT and dismiss the radial surface".into(),
            ));
        }
        if trace_lines(trace_path)
            .iter()
            .any(|line| line.contains("trace_event=\"radial_dispatch_requested\""))
        {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                "hidden-root wake tap dispatched a radial action".into(),
            ));
        }
        let after_pointer = cursor_position()
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        if pointer.x != after_pointer.x || pointer.y != after_pointer.y {
            return Err(CaseFailure::new(
                FailureStage::InputInjection,
                format!(
                    "pointer moved during hidden-root hold/tap: before=({},{}), after=({}, {})",
                    pointer.x, pointer.y, after_pointer.x, after_pointer.y
                ),
            ));
        }
        Ok(format!(
            "evidence:v1; hotkey={}; root_start=parked; pointer_stationary=true; tap_woke_root=true; hold_opened_radial=true; designer=closed; radial_dismissed_by_tap=true",
            hotkey.as_str()
        ))
    })();
    append_case(
        report,
        "H18",
        expected("H18"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_hotkey_dual_profile_case(
    executable: &str,
    _profile: &Path,
    report: &mut AcceptanceReport,
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) {
    let started = Instant::now();
    begin_hotkey_evidence_capture("H17", trace_path);
    let result = (|| {
        let configured = report.profile.configured_hotkey == hotkey.as_str();
        let mouse_gestures_enabled =
            report.mouse_gesture_mode == super::super::MouseGestureMode::Enabled;
        if !configured || !mouse_gestures_enabled {
            return Err(CaseFailure::new(
                FailureStage::Environment,
                format!(
                    "main profile mismatch: configured={}; requested={}; mouse_gesture_mode={:?}",
                    report.profile.configured_hotkey,
                    hotkey.as_str(),
                    report.mouse_gesture_mode
                ),
            ));
        }
        run_hotkey_burst_attempt(
            child,
            anchor,
            trace_path,
            hotkey,
            1,
            false,
            hold_threshold_ms,
        )?;
        let alternate = match hotkey {
            AcceptanceHotkey::F11 => AcceptanceHotkey::ShiftAltWinEnd,
            AcceptanceHotkey::ShiftAltWinEnd => AcceptanceHotkey::F11,
        };
        let temporary_profile = tempfile::Builder::new()
            .prefix("radial-acceptance-h17-")
            .tempdir()
            .map_err(|error| {
                CaseFailure::new(
                    FailureStage::Cleanup,
                    format!("create H17 isolated profile: {error}"),
                )
            })?;
        let profile_path = temporary_profile.path();
        let log_path = profile_path.join("candidate.log");
        let fixture = super::super::deterministic_fixture_for_hotkey(
            &log_path,
            super::super::MouseGestureMode::Enabled,
            alternate,
        )
        .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
        for (name, bytes) in [
            ("settings.json", fixture.settings_json.as_slice()),
            ("radial.json", fixture.radial_json.as_slice()),
            ("actions.json", fixture.actions_json.as_slice()),
        ] {
            super::super::write_new(&profile_path.join(name), bytes)
                .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
        }
        let settings: serde_json::Value = serde_json::from_slice(&fixture.settings_json)
            .map_err(|error| CaseFailure::new(FailureStage::Environment, error.to_string()))?;
        if settings.get("hotkey").and_then(serde_json::Value::as_str) != Some(alternate.as_str())
            || settings.get("plugin_settings").is_some_and(|plugins| {
                plugins
                    .get("mouse_gestures")
                    .and_then(|plugin| plugin.get("enabled"))
                    .and_then(serde_json::Value::as_bool)
                    == Some(false)
            })
        {
            return Err(CaseFailure::new(
                FailureStage::Environment,
                "alternate fixture does not match its configured hotkey or disables mouse gestures"
                    .into(),
            ));
        }
        // The application writes acceptance trace events to Settings.log_file.
        // Keep launch, readiness, gesture validation, and saved artifacts on
        // that exact path so H17 observes this isolated profile's trace.
        let alternate_trace = log_path.clone();
        register_hotkey_candidate_stream(
            &alternate_trace,
            HotkeyCandidateStream::AlternateProfileCandidate,
        );
        let mut alternate_child = NativeChild::launch(
            Path::new(executable),
            profile_path,
            &alternate_trace,
            &profile_path.join("child.stdout.log"),
            &profile_path.join("child.stderr.log"),
        )
        .map_err(|error| {
            CaseFailure::new(
                FailureStage::Environment,
                format!("launch alternate hotkey fixture: {error}"),
            )
        })?;
        let alternate_process_id = alternate_child.process_id();
        let alternate_profile_id = profile_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown-profile")
            .to_string();
        let mut alternate_tap = wait_hotkey_fixture_ready(
            &alternate_child,
            &alternate_trace,
            HOTKEY_FIXTURE_STARTUP_TIMEOUT,
        )
        .map_err(|error| {
            CaseFailure::new(
                FailureStage::Environment,
                format!(
                    "alternate profile {alternate_profile_id} pid={alternate_process_id} did not reach bounded fixture readiness: {error}"
                ),
            )
        })
        .and_then(|_| {
            run_hotkey_burst_attempt(
                &alternate_child,
                anchor,
                &alternate_trace,
                alternate,
                1,
                false,
                fixture.hold_threshold_ms,
            )
        })
        .map_err(|mut error| {
            error.message = format!(
                "alternate profile {alternate_profile_id} pid={alternate_process_id}: {}",
                error.message
            );
            error
        });
        if let Err(error) = complete_open_hotkey_capture_at_current_trace(&alternate_trace) {
            alternate_tap = Err(match alternate_tap {
                Ok(_) => error,
                Err(mut existing) => {
                    existing.message = format!(
                        "{}; alternate trace materialization failed before profile cleanup: {}",
                        existing.message, error.message
                    );
                    existing
                }
            });
        }
        let mut close_errors = Vec::new();
        if let Ok(root) = alternate_child.refresh_root() {
            if let Err(error) = request_window_close(&alternate_child, &root) {
                close_errors.push(error);
            }
        }
        let mut alternate_status = wait_child(&mut alternate_child, Duration::from_secs(5));
        if alternate_status.is_none() {
            if let Err(error) = alternate_child.kill() {
                close_errors.push(format!("terminate alternate profile process: {error}"));
            }
            alternate_status = wait_child(&mut alternate_child, Duration::from_secs(5));
        }
        let remaining = alternate_child.windows();
        if !remaining.is_empty() {
            close_errors.push(format!(
                "alternate profile left {} child-owned HWND(s)",
                remaining.len()
            ));
        }
        let alternate_exit_succeeded = alternate_status.is_some_and(|status| status.success());
        if !alternate_exit_succeeded {
            close_errors.push(format!(
                "alternate hotkey fixture did not exit successfully: {alternate_status:?}"
            ));
        }

        // Snapshot bounded evidence while the isolated profile still exists.
        // A successful tap can still be followed by a shutdown or TempDir
        // cleanup failure, so decide whether to publish it only after close.
        let alternate_artifacts =
            capture_h17_alternate_artifacts(&alternate_child, &alternate_trace);
        if let Err(error) = temporary_profile.close() {
            close_errors.push(format!("remove alternate temporary profile: {error}"));
        }
        let tap_failure = alternate_tap.err();
        if h17_should_preserve_alternate_artifacts(tap_failure.is_none(), close_errors.is_empty()) {
            let (paths, artifact_errors) =
                persist_h17_alternate_artifacts(&alternate_artifacts, output);
            for path in paths {
                report.push_artifact(path.to_string_lossy());
            }
            let mut failure_details = close_errors;
            failure_details.extend(artifact_errors);
            if let Some(mut failure) = tap_failure {
                if !failure_details.is_empty() {
                    failure.message = format!(
                        "{}; alternate cleanup/diagnostic details: {}",
                        failure.message,
                        failure_details.join("; ")
                    );
                }
                return Err(failure);
            }
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                format!(
                    "alternate hotkey fixture cleanup failed: {}",
                    failure_details.join("; ")
                ),
            ));
        }
        let f11_passed = hotkey == AcceptanceHotkey::F11 || alternate == AcceptanceHotkey::F11;
        let chord_passed = hotkey == AcceptanceHotkey::ShiftAltWinEnd
            || alternate == AcceptanceHotkey::ShiftAltWinEnd;
        Ok(format!(
            "evidence:v1; f11_control={}; exact_chord={}; mouse_gestures=enabled; profile_matches=true; alternate_profile_cleanup=verified; main_profile={}; alternate_profile={}; alternate_profile_id={}; alternate_child_pid={}",
            if f11_passed { "passed" } else { "failed" },
            if chord_passed { "passed" } else { "failed" },
            hotkey.as_str(),
            alternate.as_str(),
            alternate_profile_id,
            alternate_process_id
        ))
    })();
    append_case(
        report,
        "H17",
        expected("H17"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

struct HotkeyBurstEvidence {
    final_visible: bool,
    invocation_ids: Vec<u64>,
    input_group_id: u32,
    hold_min_ms: u128,
    hold_max_ms: u128,
    gap_min_ms: u128,
    gap_max_ms: u128,
    preflight_quiet_ms: Option<u128>,
    preflight_matching_edges: usize,
    foreign_matching_edges: usize,
    trace_fence: HotkeyTraceFence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisibleBurstSettlePolicy {
    ActivateRoot,
    PreserveForeground {
        target_hwnd: u64,
        target_process_id: u32,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct HotkeyTraceFence {
    cursor: usize,
    probe_id: u64,
    baseline_invocation_id: Option<u64>,
    baseline_visibility_revision: Option<u64>,
}

impl HotkeyTraceFence {
    fn report_token(self) -> String {
        format!(
            "probe:{},cursor:{},invocation:{},revision:{}",
            self.probe_id,
            self.cursor,
            self.baseline_invocation_id
                .map_or_else(|| "none".into(), |id| id.to_string()),
            self.baseline_visibility_revision
                .map_or_else(|| "none".into(), |revision| revision.to_string())
        )
    }
}

fn run_hotkey_burst_attempt(
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    hotkey: AcceptanceHotkey,
    taps: usize,
    initial_visible: bool,
    hold_threshold_ms: u64,
) -> Result<HotkeyBurstEvidence, CaseFailure> {
    run_hotkey_burst_attempt_on_target(
        child,
        anchor.hwnd(),
        anchor.process_id(),
        Some(anchor),
        trace_path,
        hotkey,
        taps,
        initial_visible,
        hold_threshold_ms,
    )
}

fn run_hotkey_burst_attempt_on_target(
    child: &NativeChild,
    target_hwnd: HWND,
    target_process_id: u32,
    focus_anchor: Option<&FocusAnchor>,
    trace_path: &Path,
    hotkey: AcceptanceHotkey,
    taps: usize,
    initial_visible: bool,
    hold_threshold_ms: u64,
) -> Result<HotkeyBurstEvidence, CaseFailure> {
    run_hotkey_burst_attempt_on_target_checked(
        child,
        target_hwnd,
        target_process_id,
        focus_anchor,
        trace_path,
        hotkey,
        taps,
        initial_visible,
        hold_threshold_ms,
        || Ok(()),
    )
}

fn run_hotkey_burst_attempt_on_target_checked<F>(
    child: &NativeChild,
    target_hwnd: HWND,
    target_process_id: u32,
    focus_anchor: Option<&FocusAnchor>,
    trace_path: &Path,
    hotkey: AcceptanceHotkey,
    taps: usize,
    initial_visible: bool,
    hold_threshold_ms: u64,
    before_injection: F,
) -> Result<HotkeyBurstEvidence, CaseFailure>
where
    F: FnOnce() -> Result<(), CaseFailure>,
{
    run_hotkey_burst_attempt_on_target_with_policy_checked(
        child,
        target_hwnd,
        target_process_id,
        focus_anchor,
        trace_path,
        hotkey,
        taps,
        initial_visible,
        hold_threshold_ms,
        before_injection,
        VisibleBurstSettlePolicy::ActivateRoot,
    )
}

fn run_hotkey_burst_attempt_on_target_with_policy(
    child: &NativeChild,
    target_hwnd: HWND,
    target_process_id: u32,
    focus_anchor: Option<&FocusAnchor>,
    trace_path: &Path,
    hotkey: AcceptanceHotkey,
    taps: usize,
    initial_visible: bool,
    hold_threshold_ms: u64,
    settle_policy: VisibleBurstSettlePolicy,
) -> Result<HotkeyBurstEvidence, CaseFailure> {
    run_hotkey_burst_attempt_on_target_with_policy_checked(
        child,
        target_hwnd,
        target_process_id,
        focus_anchor,
        trace_path,
        hotkey,
        taps,
        initial_visible,
        hold_threshold_ms,
        || Ok(()),
        settle_policy,
    )
}

fn run_hotkey_burst_attempt_on_target_with_policy_checked<F>(
    child: &NativeChild,
    target_hwnd: HWND,
    target_process_id: u32,
    focus_anchor: Option<&FocusAnchor>,
    trace_path: &Path,
    hotkey: AcceptanceHotkey,
    taps: usize,
    initial_visible: bool,
    hold_threshold_ms: u64,
    before_injection: F,
    settle_policy: VisibleBurstSettlePolicy,
) -> Result<HotkeyBurstEvidence, CaseFailure>
where
    F: FnOnce() -> Result<(), CaseFailure>,
{
    let root = child
        .refresh_root()
        .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
    let displays = native_display_bounds()
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    let currently_visible =
        root.visible && !root.minimized && intersects_display_bounds(root.bounds, &displays);
    let mut setup_trace_cursor = None;
    if currently_visible != initial_visible {
        setup_trace_cursor = Some(trace_lines(trace_path).len());
        if let Some(anchor) = focus_anchor {
            anchor
                .focus()
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        } else {
            focus_is_validated(target_hwnd, target_process_id)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        }
        child
            .send_acceptance_hotkey(
                target_hwnd,
                target_process_id,
                hotkey,
                Duration::from_millis(25),
            )
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        if !wait_root_visibility(child, initial_visible, ROOT_TIMEOUT) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!("separate setup tap did not establish initial_visible={initial_visible}"),
            ));
        }
    }
    if !wait_root_visibility(child, initial_visible, Duration::ZERO) {
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            format!("ROOT did not start H04 burst in visible={initial_visible} state"),
        ));
    }
    run_after_hotkey_setup_settled(
        || {
            if let Some(trace_cursor) = setup_trace_cursor {
                settle_hotkey_setup_toggle(
                    child,
                    trace_path,
                    trace_cursor,
                    initial_visible,
                    settle_policy,
                )
            } else {
                Ok(())
            }
        },
        || {
            if let Some(anchor) = focus_anchor {
                anchor
                    .focus()
                    .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            } else {
                focus_is_validated(target_hwnd, target_process_id)
                    .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            }
            Ok(())
        },
    )?;
    let cursor_before =
        cursor_position().map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let mut observer = RunnerHookObserver::start()
        .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
    if !observer.desktop.eq_ignore_ascii_case("Default") {
        return Err(CaseFailure::new(
            FailureStage::HookAdmission,
            format!(
                "runner observer desktop {:?} is not Default",
                observer.desktop
            ),
        ));
    }
    let probe_id = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
    observer
        .pump_roundtrip(probe_id, Duration::from_millis(500))
        .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
    focus_is_validated(target_hwnd, target_process_id)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let trace_fence = establish_hotkey_trace_fence(child, trace_path, initial_visible)?;
    focus_is_validated(target_hwnd, target_process_id)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let observed_vks = match hotkey {
        AcceptanceHotkey::F11 => vec![0x7A],
        AcceptanceHotkey::ShiftAltWinEnd => vec![0xA0, 0xA4, 0x5B, 0x23],
    };
    let preflight = if h04_matrix_capture_active() {
        input_modifiers_clear().map_err(|error| {
            CaseFailure::new(
                FailureStage::InputInjection,
                format!("H04 matrix preflight found uncleared modifier state: {error}"),
            )
        })?;
        let quiet = observer
            .wait_for_key_quiet(
                &observed_vks,
                Duration::from_millis(80),
                Duration::from_secs(2),
            )
            .map_err(|error| {
                CaseFailure::new(
                    FailureStage::InputInjection,
                    format!("H04 matrix matching-key quiet preflight failed: {error}"),
                )
            })?;
        input_modifiers_clear().map_err(|error| {
            CaseFailure::new(
                FailureStage::InputInjection,
                format!("H04 matrix preflight modifier recheck failed: {error}"),
            )
        })?;
        focus_is_validated(target_hwnd, target_process_id)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        Some(quiet)
    } else {
        None
    };
    before_injection()?;
    let trace_cursor = trace_fence.cursor;
    let (down_time, released_time) = hotkey_burst_intervals(preflight.is_some());
    let injection = child
        .send_acceptance_hotkey_burst(
            target_hwnd,
            target_process_id,
            hotkey,
            taps,
            down_time,
            released_time,
        )
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    if injection.input_desktop != "thread=Default;active=Default" {
        return Err(CaseFailure::new(
            FailureStage::InputInjection,
            format!(
                "hotkey burst input desktop was not WinSta0\\Default: {}",
                injection.input_desktop
            ),
        ));
    }

    let observation = observer.wait_for_chord_burst(&observed_vks, taps, Duration::from_secs(10));
    let input_group_id = capture_hotkey_attempt_evidence(
        child,
        HotkeyCandidateStream::MainCandidate,
        trace_path,
        trace_cursor,
        &observation,
        HotkeyRunnerInputPurpose::LauncherChord,
    );
    let foreign_overlap = observation.foreign_edges_interfering_with_owned_gestures();
    let foreign_matching_edges = observation.foreign_edges.len();
    let observer_description = observation.describe();
    let observer_exact = observation.exact_injected_pairs(taps)
        && observation.exact_injected_sequence(&expected_hotkey_edges(hotkey, taps));
    observer
        .stop_and_report()
        .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
    if let Some(failure) = foreign_edge_contamination_failure(
        input_group_id,
        &foreign_overlap,
        h04_matrix_capture_active(),
        &observer_description,
    ) {
        return Err(failure);
    }
    if !observer_exact {
        return Err(CaseFailure::new(
            FailureStage::HookAdmission,
            format!("observer did not see exact injected hotkey edges: {observer_description}"),
        ));
    }
    let timing = observation
        .timing(hotkey, taps)
        .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
    if !hotkey_cadence_is_valid(&timing, hold_threshold_ms) {
        return Err(CaseFailure::new(
            FailureStage::HookAdmission,
            format!(
                "observed physical cadence fell outside 10–100ms or approached hold threshold {hold_threshold_ms}ms: holds={:?}; gaps={:?}; observer={observer_description}",
                timing.primary_hold_ms, timing.released_gap_ms
            ),
        ));
    }
    input_modifiers_clear().map_err(|error| {
        CaseFailure::new(
            FailureStage::InputInjection,
            format!(
                "modifier/key cleanup verification failed: {error}; {}",
                injection.cleanup
            ),
        )
    })?;
    let hook_events = wait_trace(trace_path, trace_cursor, TRACE_TIMEOUT, |events| {
        let expected_edges = taps.saturating_mul(2);
        hotkey_trace_edge_count(events, "hook_primary") >= expected_edges
            && hotkey_trace_edge_count(events, "configured_primary") >= expected_edges
    });
    validate_hotkey_production_admission(&hook_events, taps).map_err(|error| {
        CaseFailure::new(
            FailureStage::HookAdmission,
            format!("runner observed exact injected chord edges but production hook admission failed: {error}; observer={observer_description}"),
        )
    })?;
    let events = wait_trace(trace_path, trace_cursor, TRACE_TIMEOUT, |events| {
        validate_hotkey_burst_trace_with_baseline(
            events,
            taps,
            initial_visible,
            trace_fence.baseline_visibility_revision,
            trace_fence.baseline_invocation_id,
        )
        .is_ok()
    });
    let summary = validate_hotkey_burst_trace_with_baseline(
        &events,
        taps,
        initial_visible,
        trace_fence.baseline_visibility_revision,
        trace_fence.baseline_invocation_id,
    )
    .map_err(|error| {
        CaseFailure::new(
            FailureStage::GestureDecision,
            format!("uninterrupted burst trace validation failed: {error}; observer={observer_description}"),
        )
    })?;
    let expected_end_visible = initial_visible ^ (taps % 2 == 1);
    if summary.final_visible != expected_end_visible
        || !wait_root_visibility(child, expected_end_visible, ROOT_TIMEOUT)
    {
        let current = child.refresh_root().ok();
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            format!(
                "ROOT failed initial={initial_visible} taps={taps} parity={expected_end_visible}: {:?}; trace final={}; observer={observer_description}",
                current.map(|root| (
                    root.hwnd.0,
                    root.process_id,
                    root.visible,
                    root.minimized,
                    root.bounds
                )),
                summary.final_visible
            ),
        ));
    }
    if let Some(settle_policy) = hotkey_burst_settle_policy(settle_policy, expected_end_visible) {
        settle_hotkey_burst(
            child,
            trace_path,
            trace_cursor,
            settle_policy,
            expected_end_visible,
            TRACE_TIMEOUT,
        )?;
    }
    let cursor_after =
        cursor_position().map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    if cursor_before.x != cursor_after.x || cursor_before.y != cursor_after.y {
        return Err(CaseFailure::new(
            FailureStage::InputInjection,
            format!(
                "cursor moved during uninterrupted burst: before=({},{}), after=({},{})",
                cursor_before.x, cursor_before.y, cursor_after.x, cursor_after.y
            ),
        ));
    }
    let expected_events = match hotkey {
        AcceptanceHotkey::F11 => taps,
        AcceptanceHotkey::ShiftAltWinEnd => taps.saturating_mul(4),
    };
    if injection.down_inserted != expected_events || injection.up_inserted != expected_events {
        return Err(CaseFailure::new(
            FailureStage::InputInjection,
            format!(
                "SendInput inserted {}/{} down/up edges, expected {expected_events}/{expected_events}",
                injection.down_inserted, injection.up_inserted
            ),
        ));
    }
    complete_hotkey_capture_at_current_trace(trace_path)?;
    Ok(HotkeyBurstEvidence {
        final_visible: summary.final_visible,
        invocation_ids: summary.invocation_ids,
        input_group_id,
        hold_min_ms: *timing.primary_hold_ms.iter().min().unwrap_or(&0),
        hold_max_ms: *timing.primary_hold_ms.iter().max().unwrap_or(&0),
        gap_min_ms: *timing.released_gap_ms.iter().min().unwrap_or(&0),
        gap_max_ms: *timing.released_gap_ms.iter().max().unwrap_or(&0),
        preflight_quiet_ms: preflight.map(|quiet| quiet.quiet_ms),
        preflight_matching_edges: preflight.map_or(0, |quiet| quiet.matching_edges),
        foreign_matching_edges,
        trace_fence,
    })
}

fn foreign_edge_contamination_failure(
    input_group_id: u32,
    foreign_overlap: &[RunnerChordEdge],
    retryable_h04_matrix: bool,
    observer_description: &str,
) -> Option<CaseFailure> {
    if foreign_overlap.is_empty() {
        return None;
    }
    let details = foreign_overlap
        .iter()
        .map(|edge| {
            format!(
                "vk=0x{:02x}:{}:injected={}:extra=0x{:x}",
                edge.vk,
                if edge.down { "down" } else { "up" },
                edge.injected,
                edge.extra_info
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let message = format!(
        "foreign matching-key edge overlapped owned burst interval: {details}; observer={observer_description}"
    );
    Some(if retryable_h04_matrix && input_group_id != 0 {
        CaseFailure::input_contamination(input_group_id, message)
    } else {
        CaseFailure::new(FailureStage::InputInjection, message)
    })
}

fn run_after_hotkey_setup_settled<S, N, T>(settle_setup: S, next: N) -> Result<T, CaseFailure>
where
    S: FnOnce() -> Result<(), CaseFailure>,
    N: FnOnce() -> Result<T, CaseFailure>,
{
    settle_setup()?;
    next()
}

fn settle_hotkey_setup_toggle(
    child: &NativeChild,
    trace_path: &Path,
    trace_cursor: usize,
    expected_visible: bool,
    policy: VisibleBurstSettlePolicy,
) -> Result<(), CaseFailure> {
    match policy {
        VisibleBurstSettlePolicy::ActivateRoot if expected_visible => {
            wait_for_root_restore_after(trace_path, trace_cursor, TRACE_TIMEOUT).map_err(|error| {
                CaseFailure::new(
                    FailureStage::RootCommand,
                    format!("visible setup tap did not complete its correlated ROOT restore: {error}"),
                )
            })?;
        }
        VisibleBurstSettlePolicy::ActivateRoot => {
            wait_for_no_native_activation_quiet(trace_path, trace_cursor, TRACE_TIMEOUT).map_err(
                |error| {
                    CaseFailure::new(
                        FailureStage::RootCommand,
                        format!("hidden setup tap unexpectedly activated ROOT or did not settle: {error}"),
                    )
                },
            )?;
        }
        VisibleBurstSettlePolicy::PreserveForeground {
            target_hwnd: preserved_hwnd,
            target_process_id: preserved_process_id,
        } => {
            wait_for_preserved_foreground_burst(
                child,
                trace_path,
                trace_cursor,
                preserved_hwnd,
                preserved_process_id,
                expected_visible,
                TRACE_TIMEOUT,
            )
            .map_err(|error| {
                CaseFailure::new(
                    FailureStage::RootCommand,
                    format!("PreserveForeground setup tap did not settle cleanly: {error}"),
                )
            })?;
        }
    }
    if !wait_root_visibility(child, expected_visible, Duration::ZERO) {
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            format!("ROOT left expected visible={expected_visible} state after setup settle"),
        ));
    }
    Ok(())
}

fn wait_for_no_native_activation_quiet(
    trace_path: &Path,
    trace_cursor: usize,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let quiet_since = Instant::now();
    loop {
        let events = trace_lines(trace_path);
        validate_no_native_root_activation_after(&events, trace_cursor)?;
        if quiet_since.elapsed() >= HOTKEY_TRACE_QUIET_WINDOW {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "native ROOT activation trace did not remain quiet for {:?}",
                HOTKEY_TRACE_QUIET_WINDOW
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn hotkey_cadence_is_valid(timing: &RunnerChordTiming, hold_threshold_ms: u64) -> bool {
    timing
        .primary_hold_ms
        .iter()
        .all(|hold| (10..=100).contains(hold) && *hold < u128::from(hold_threshold_ms))
        && timing
            .released_gap_ms
            .iter()
            .all(|gap| (10..=100).contains(gap))
}

fn hotkey_burst_intervals(h04_matrix_preflight: bool) -> (Duration, Duration) {
    (
        Duration::from_millis(25),
        Duration::from_millis(if h04_matrix_preflight { 75 } else { 25 }),
    )
}

fn hotkey_burst_settle_policy(
    policy: VisibleBurstSettlePolicy,
    expected_end_visible: bool,
) -> Option<VisibleBurstSettlePolicy> {
    match (policy, expected_end_visible) {
        (VisibleBurstSettlePolicy::ActivateRoot, true) => Some(policy),
        (VisibleBurstSettlePolicy::ActivateRoot, false) => None,
        (VisibleBurstSettlePolicy::PreserveForeground { .. }, _) => Some(policy),
    }
}

fn settle_hotkey_burst(
    child: &NativeChild,
    trace_path: &Path,
    trace_cursor: usize,
    policy: VisibleBurstSettlePolicy,
    expected_end_visible: bool,
    timeout: Duration,
) -> Result<(), CaseFailure> {
    match policy {
        VisibleBurstSettlePolicy::ActivateRoot => {
            wait_for_root_restore_after(trace_path, trace_cursor, timeout).map_err(|error| {
                CaseFailure::new(
                    FailureStage::RootCommand,
                    format!(
                        "visible burst did not settle through its correlated native ROOT restore: {error}"
                    ),
                )
            })?;
            Ok(())
        }
        VisibleBurstSettlePolicy::PreserveForeground {
            target_hwnd,
            target_process_id,
        } => wait_for_preserved_foreground_burst(
            child,
            trace_path,
            trace_cursor,
            target_hwnd,
            target_process_id,
            expected_end_visible,
            timeout,
        )
        .map_err(|error| {
            CaseFailure::new(
                FailureStage::RootCommand,
                format!("PreserveForeground burst did not settle cleanly: {error}"),
            )
        }),
    }
}

fn wait_for_preserved_foreground_burst(
    child: &NativeChild,
    trace_path: &Path,
    trace_cursor: usize,
    target_hwnd: u64,
    target_process_id: u32,
    require_root_presented: bool,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let mut stable_since = None;
    loop {
        let events = trace_lines(trace_path);
        if visible_burst_trace_settled(
            &events,
            trace_cursor,
            VisibleBurstSettlePolicy::PreserveForeground {
                target_hwnd,
                target_process_id,
            },
        )?
        .is_none()
        {
            return Err("PreserveForeground trace did not settle".into());
        }

        let root_presentation_satisfied =
            require_root_presented.then(|| wait_root_visibility(child, true, Duration::ZERO));
        let (foreground, foreground_pid) = capture_foreground();
        if preserved_target_retained_foreground(
            root_presentation_satisfied,
            hwnd_id(foreground),
            foreground_pid,
            target_hwnd,
            target_process_id,
        ) {
            let since = stable_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= HOTKEY_TRACE_QUIET_WINDOW {
                return Ok(());
            }
        } else {
            stable_since = None;
        }

        if Instant::now() >= deadline {
            return Err(format!(
                "ROOT presentation check={root_presentation_satisfied:?}, retained target foreground=({},{}), expected=({target_hwnd},{target_process_id})",
                hwnd_id(foreground),
                foreground_pid
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn preserved_target_retained_foreground(
    root_presentation_satisfied: Option<bool>,
    foreground_hwnd: u64,
    foreground_process_id: u32,
    target_hwnd: u64,
    target_process_id: u32,
) -> bool {
    root_presentation_satisfied != Some(false)
        && foreground_hwnd == target_hwnd
        && foreground_process_id == target_process_id
}

fn validate_no_native_root_activation_after(
    events: &[String],
    cursor: usize,
) -> Result<(), String> {
    if let Some(event) = events
        .iter()
        .skip(cursor)
        .find(|line| line.contains("trace_event=\"native_activation\""))
    {
        return Err(format!(
            "PreserveForeground burst emitted an unexpected native activation: {event}"
        ));
    }
    Ok(())
}

fn run_readable_hotkey_transitions(
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) -> Result<Vec<HotkeyTraceFence>, CaseFailure> {
    let initial_visible = false;
    if !wait_root_visibility(child, initial_visible, ROOT_TIMEOUT) {
        anchor
            .focus()
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        child
            .send_acceptance_hotkey(
                anchor.hwnd(),
                anchor.process_id(),
                hotkey,
                Duration::from_millis(25),
            )
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        if !wait_root_visibility(child, false, ROOT_TIMEOUT) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                "could not prepare readable cadence with hidden ROOT".into(),
            ));
        }
    }
    anchor
        .focus()
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let mut visible = false;
    let mut trace_fences = Vec::with_capacity(3);
    for tap in 0..3 {
        let mut observer = RunnerHookObserver::start()
            .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
        let probe_id = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
        observer
            .pump_roundtrip(probe_id, Duration::from_millis(500))
            .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
        let (target_hwnd, target_pid) = current_hotkey_target(child, anchor)?;
        if tap == 1 {
            let root = child
                .refresh_root()
                .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
            if !visible
                || target_hwnd != root.hwnd
                || target_pid != child.process_id()
                || !wait_root_visibility(child, true, Duration::ZERO)
            {
                return Err(CaseFailure::new(
                    FailureStage::InputInjection,
                    format!(
                        "second readable tap must hide currently focused ROOT without refocusing: foreground=({},{}), ROOT=({},{}), visible={visible}",
                        hwnd_id(target_hwnd),
                        target_pid,
                        hwnd_id(root.hwnd),
                        child.process_id()
                    ),
                ));
            }
        }
        let trace_fence = establish_hotkey_trace_fence(child, trace_path, visible)?;
        focus_is_validated(target_hwnd, target_pid)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        if tap == 1 {
            let root = child
                .refresh_root()
                .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
            if !visible
                || target_hwnd != root.hwnd
                || target_pid != child.process_id()
                || !wait_root_visibility(child, true, Duration::ZERO)
            {
                return Err(CaseFailure::new(
                    FailureStage::InputInjection,
                    format!(
                        "second readable tap lost focused ROOT before key-down: foreground=({},{}), ROOT=({},{}), visible={visible}",
                        hwnd_id(target_hwnd),
                        target_pid,
                        hwnd_id(root.hwnd),
                        child.process_id()
                    ),
                ));
            }
        }
        let cursor = trace_fence.cursor;
        child
            .send_acceptance_hotkey(target_hwnd, target_pid, hotkey, Duration::from_millis(25))
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let keys = match hotkey {
            AcceptanceHotkey::F11 => vec![0x7A],
            AcceptanceHotkey::ShiftAltWinEnd => vec![0xA0, 0xA4, 0x5B, 0x23],
        };
        let observation = observer.wait_for_chord_burst(&keys, 1, Duration::from_secs(2));
        capture_hotkey_attempt_evidence(
            child,
            HotkeyCandidateStream::MainCandidate,
            trace_path,
            cursor,
            &observation,
            HotkeyRunnerInputPurpose::LauncherChord,
        );
        observer
            .stop_and_report()
            .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
        if !observation.exact_injected_pairs(1)
            || !observation.exact_injected_sequence(&expected_hotkey_edges(hotkey, 1))
        {
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                "readable cadence did not observe one exact injected tap".into(),
            ));
        }
        let timing = observation
            .timing(hotkey, 1)
            .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
        if timing.primary_hold_ms[0] >= u128::from(hold_threshold_ms)
            || !(10..=100).contains(&timing.primary_hold_ms[0])
        {
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "readable tap was not a bounded short press: {}ms",
                    timing.primary_hold_ms[0]
                ),
            ));
        }
        let admission_events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
            hotkey_trace_edge_count(events, "hook_primary") >= 2
                && hotkey_trace_edge_count(events, "configured_primary") >= 2
        });
        validate_hotkey_production_admission(&admission_events, 1).map_err(|error| {
            CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "readable tap observer saw exact input but production admission failed: {error}"
                ),
            )
        })?;
        let events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
            validate_hotkey_burst_trace_with_baseline(
                events,
                1,
                visible,
                trace_fence.baseline_visibility_revision,
                trace_fence.baseline_invocation_id,
            )
            .is_ok()
        });
        let summary = validate_hotkey_burst_trace_with_baseline(
            &events,
            1,
            visible,
            trace_fence.baseline_visibility_revision,
            trace_fence.baseline_invocation_id,
        )
        .map_err(|error| CaseFailure::new(FailureStage::GestureDecision, error))?;
        trace_fences.push(trace_fence);
        visible = !visible;
        if summary.final_visible != visible || !wait_root_visibility(child, visible, ROOT_TIMEOUT) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "readable tap {} did not transition ROOT to {visible}",
                    tap + 1
                ),
            ));
        }
        if visible {
            wait_for_root_restore_after(trace_path, cursor, TRACE_TIMEOUT).map_err(|error| {
                CaseFailure::new(
                    FailureStage::RootCommand,
                    format!(
                        "readable tap {} did not settle through its correlated native ROOT restore: {error}",
                        tap + 1
                    ),
                )
            })?;
        }
        complete_hotkey_capture_at_current_trace(trace_path)?;
    }
    Ok(trace_fences)
}

fn hotkey_key_names(hotkey: AcceptanceHotkey) -> &'static str {
    match hotkey {
        AcceptanceHotkey::F11 => "F11",
        AcceptanceHotkey::ShiftAltWinEnd => "LeftShift+LeftAlt+LeftWin+End",
    }
}

fn expected_hotkey_edges(hotkey: AcceptanceHotkey, taps: usize) -> Vec<(u32, bool)> {
    let tap_edges: &[(u32, bool)] = match hotkey {
        AcceptanceHotkey::F11 => &[(0x7A, true), (0x7A, false)],
        AcceptanceHotkey::ShiftAltWinEnd => &[
            (0xA0, true),
            (0xA4, true),
            (0x5B, true),
            (0x23, true),
            (0x23, false),
            (0x5B, false),
            (0xA4, false),
            (0xA0, false),
        ],
    };
    let mut expected = Vec::with_capacity(tap_edges.len().saturating_mul(taps));
    for _ in 0..taps {
        expected.extend_from_slice(tap_edges);
    }
    expected
}

fn hotkey_observer_keys(hotkey: AcceptanceHotkey) -> Vec<u32> {
    match hotkey {
        AcceptanceHotkey::F11 => vec![0x7A],
        AcceptanceHotkey::ShiftAltWinEnd => vec![0xA0, 0xA4, 0x5B, 0x23],
    }
}

pub fn run_copied_profile_suite(
    executable: &str,
    profile: &Path,
    output: &Path,
    trace_path: &Path,
    cursor_restore: Option<POINT>,
    _desktop: &InputDesktopAttachment,
    report: &mut AcceptanceReport,
    runner_log: &mut File,
    copied: CopiedAuthoringOptions,
) -> Option<FocusAnchor> {
    let mut child = None;
    let mut anchor = None;
    if let Err(error) = preflight_acceptance_hotkey(AcceptanceHotkey::F11) {
        let _ = writeln!(
            runner_log,
            "copied-profile hotkey preflight failed: {error}"
        );
    } else {
        let stdout_path = profile.join("child.stdout.log");
        let stderr_path = profile.join("child.stderr.log");
        match NativeChild::launch(
            Path::new(executable),
            profile,
            trace_path,
            &stdout_path,
            &stderr_path,
        ) {
            Ok(mut launched) => {
                report.environment.child_process_id = Some(launched.process_id());
                report.environment.child_started_unix_ms = launched
                    .started()
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|duration| duration.as_millis());
                let created_anchor = FocusAnchor::create();
                match created_anchor {
                    Ok(created_anchor) => {
                        anchor = Some(created_anchor);
                        let anchor_ref = anchor.as_ref().expect("anchor was just created");
                        run_tap_case(
                            report,
                            "H0",
                            &mut launched,
                            anchor_ref,
                            trace_path,
                            output,
                            expected("H0"),
                            true,
                            false,
                            1,
                        );
                        run_tap_case(
                            report,
                            "H1",
                            &mut launched,
                            anchor_ref,
                            trace_path,
                            output,
                            expected("H1"),
                            false,
                            true,
                            1,
                        );
                        run_other_focus_case(report, &mut launched, anchor_ref, trace_path, output);

                        match UiAutomation::new() {
                            Ok(automation) => match run_designer_entry(
                                &mut launched,
                                &automation,
                                anchor_ref,
                                trace_path,
                            ) {
                                Ok(entry) => {
                                    append_case(
                                        report,
                                        "D0",
                                        expected("D0"),
                                        started_now(),
                                        Ok("one child-owned Designer HWND is ready for copied-profile native input".into()),
                                        None,
                                        output,
                                        trace_path,
                                    );
                                    run_failure_artifact_case(
                                        report, &launched, output, trace_path,
                                    );
                                    run_designer_focus_case(
                                        report,
                                        &mut launched,
                                        &automation,
                                        &entry.window,
                                        trace_path,
                                        output,
                                    );
                                    run_designer_pointer_case(
                                        report,
                                        &mut launched,
                                        &automation,
                                        &entry.window,
                                        trace_path,
                                        output,
                                        "the copied Designer exercises checked Tree pointer transitions; CP_D2 separately selects Menus",
                                        true,
                                    );
                                    run_tab_case(
                                        report,
                                        &mut launched,
                                        &automation,
                                        &entry.window,
                                        output,
                                        trace_path,
                                        &copied.restore_menu_name,
                                        true,
                                    );
                                    run_skins_command_case(
                                        report,
                                        &mut launched,
                                        &automation,
                                        &entry.window,
                                        trace_path,
                                        output,
                                    );
                                    run_designer_close_case(
                                        report,
                                        &mut launched,
                                        &entry.window,
                                        output,
                                        trace_path,
                                    );
                                    match run_designer_entry(
                                        &mut launched,
                                        &automation,
                                        anchor_ref,
                                        trace_path,
                                    ) {
                                        Ok(authoring_entry) => run_authoring_geometry_cases(
                                            report,
                                            &mut launched,
                                            &automation,
                                            anchor_ref,
                                            profile,
                                            &authoring_entry.window,
                                            authoring_entry.session_id,
                                            output,
                                            trace_path,
                                            Some(&copied),
                                        ),
                                        Err(failure) => append_blocked_authoring_cases(
                                            report,
                                            &failure,
                                            Some(&launched),
                                            output,
                                            trace_path,
                                        ),
                                    }
                                }
                                Err(failure) => {
                                    append_case(
                                        report,
                                        "D0",
                                        expected("D0"),
                                        started_now(),
                                        Err(failure),
                                        Some(&launched),
                                        output,
                                        trace_path,
                                    );
                                }
                            },
                            Err(error) => append_case(
                                report,
                                "D0",
                                expected("D0"),
                                started_now(),
                                Err(CaseFailure::new(FailureStage::Environment, error)),
                                Some(&launched),
                                output,
                                trace_path,
                            ),
                        }
                    }
                    Err(error) => {
                        let _ =
                            writeln!(runner_log, "copied-profile anchor creation failed: {error}");
                    }
                }
                restore_cursor_before_shutdown(cursor_restore, runner_log);
                stop_child(&mut launched, report, runner_log, output, trace_path);
                child = Some(launched);
            }
            Err(error) => {
                report.environment.child_process_id = error.process_id;
                report.environment.child_started_unix_ms = error.started.and_then(|started| {
                    started
                        .duration_since(UNIX_EPOCH)
                        .ok()
                        .map(|duration| duration.as_millis())
                });
                let _ = writeln!(
                    runner_log,
                    "copied-profile candidate startup failed: {error}"
                );
            }
        }
    }
    drop(child);
    let renamed = [
        ("R1", "CP_R1"),
        ("H0", "CP_H0"),
        ("H1", "CP_H1"),
        ("H2", "CP_H2"),
        ("H3", "CP_H3"),
        ("D0", "CP_D0"),
        ("D1", "CP_D1"),
        ("D2", "CP_D2"),
        ("D4", "CP_D4"),
        ("D5", "CP_D5"),
        ("A0", "CP_A0"),
        ("A1", "CP_A1"),
        ("G0", "CP_G0"),
        ("A2", "CP_A2"),
        ("A3", "CP_A3"),
        ("A4", "CP_A4"),
        ("A5", "CP_A5"),
        ("A6", "CP_A6"),
        ("A7", "CP_A7"),
        ("A8", "CP_A8"),
        ("D3", "CP_D3"),
        ("D6", "CP_D6"),
        ("D7", "CP_D7"),
    ];
    for case in &mut report.cases {
        if let Some((_, copied_id)) = renamed.iter().find(|(old, _)| case.id == *old) {
            case.id = (*copied_id).to_string();
        }
    }
    for id in super::super::COPIED_CASE_IDS {
        if matches!(
            id,
            "CP_PREFLIGHT" | "CP_SOURCE_INTEGRITY" | "R0" | "R2" | "CLEANUP"
        ) || report.cases.iter().any(|case| case.id == id)
        {
            continue;
        }
        append_case_without_artifacts(
            report,
            id,
            Err(CaseFailure::new(
                FailureStage::Cleanup,
                "copied-profile native case was not reached because an earlier boundary failed"
                    .into(),
            )),
        );
    }
    anchor
}

pub fn record_environment_failure(
    message: String,
    report: &mut AcceptanceReport,
    output: &Path,
    trace_path: &Path,
    runner_log: &mut File,
) {
    let failure = CaseFailure::new(FailureStage::Environment, message);
    let ids: &[&str] = match report.suite {
        AcceptanceSuite::All => &CASE_IDS,
        AcceptanceSuite::Hotkey => &HOTKEY_CASE_IDS,
    };
    for (index, id) in ids
        .iter()
        .copied()
        .filter(|id| *id != "R0" && !DEFERRED_REPORT_CASE_IDS.contains(id))
        .enumerate()
    {
        if index == 0 {
            append_case(
                report,
                id,
                expected(id),
                started_now(),
                Err(failure.clone()),
                None,
                output,
                trace_path,
            );
        } else {
            append_case_without_artifacts(
                report,
                id,
                Err(CaseFailure::new(
                    failure.stage,
                    "not run because native environment setup failed; see the first case result"
                        .into(),
                )),
            );
        }
    }
    let _ = writeln!(runner_log, "native environment setup failed: {failure}");
}

fn restore_cursor_before_shutdown(cursor_restore: Option<POINT>, runner_log: &mut File) {
    let Some(point) = cursor_restore else {
        return;
    };

    match set_cursor_position(point) {
        Ok(()) => {
            let _ = writeln!(
                runner_log,
                "cursor restored to captured position ({},{}) before candidate shutdown",
                point.x, point.y
            );
        }
        Err(error) => {
            let _ = writeln!(
                runner_log,
                "cursor restoration before candidate shutdown failed: {error}"
            );
        }
    }
}

fn run_tap_case(
    report: &mut AcceptanceReport,
    id: &str,
    child: &mut NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    expected: &str,
    start_visible: bool,
    end_visible: bool,
    taps: usize,
) {
    let started = Instant::now();
    let result = (|| {
        // NativeChild becomes discoverable as soon as USER creates the HWND. Wait for the
        // actual initial visible state before testing a focused ROOT tap; input must never
        // be used to repair an unobserved startup state.
        if id == "H0" && start_visible && !wait_root_visibility(child, true, ROOT_TIMEOUT) {
            let root = child.refresh_root().ok();
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "ROOT did not become initially visible and drawable before H0; no input was sent: {:?}",
                    root.map(|window| (window.visible, window.minimized, window.bounds))
                ),
            ));
        }
        let mut root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        if start_visible {
            require_visible(&root)
                .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
            child
                .focus_window(&root)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        } else {
            require_hidden(&root)
                .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
            anchor
                .focus()
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        }
        let mut runner_observer = None;
        let mut hook_diagnostic = String::new();
        if id == "H0" {
            let startup_events = trace_lines(trace_path);
            let hook_ready = startup_events
                .iter()
                .find(|line| line.contains("trace_event=\"hook_service_ready\""))
                .cloned()
                .unwrap_or_else(|| "hook_service_ready was not recorded before H0".into());
            let sentinel_cursor = startup_events.len();
            let (observer, observer_error) = match RunnerHookObserver::start() {
                Ok(observer) => (Some(observer), None),
                Err(error) => (None, Some(error)),
            };
            runner_observer = observer;
            let sentinel_input = child
                .press_hook_sentinel(root.hwnd, child.process_id())
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            let sentinel_events =
                wait_trace(trace_path, sentinel_cursor, TRACE_TIMEOUT, |events| {
                    has_trace(events, "hook_observed", &["vk=135", "down=true"])
                        && has_trace(events, "hook_observed", &["vk=135", "down=false"])
                });
            let sentinel_events = if has_trace(&sentinel_events, "frontend_key", &["key=F24"]) {
                sentinel_events
            } else {
                wait_trace(
                    trace_path,
                    sentinel_cursor,
                    Duration::from_millis(250),
                    |events| has_trace(events, "frontend_key", &["key=F24"]),
                )
            };
            let down_seen = has_trace(
                &sentinel_events,
                "hook_observed",
                &["vk=135", "down=true", "injected=true"],
            );
            let up_seen = has_trace(
                &sentinel_events,
                "hook_observed",
                &["vk=135", "down=false", "injected=true"],
            );
            let runner_observation = match runner_observer.as_mut() {
                Some(observer) => {
                    let observation = observer.wait_for_vk(0x87, Duration::from_secs(1));
                    observation.describe()
                }
                None => format!(
                    "runner observer unavailable: {}",
                    observer_error.unwrap_or_else(|| "not started".into())
                ),
            };
            hook_diagnostic = format!(
                "pre-H0 hook={hook_ready}; F24 sentinel production callback down/up={down_seen}/{up_seen} injected, frontend WM_KEYDOWN={}; {runner_observation}, checked input=[{}]",
                has_trace(&sentinel_events, "frontend_key", &["key=F24"]),
                sentinel_input.describe()
            );
        }
        let mut cursor = trace_lines(trace_path).len();
        let mut input_evidence = Vec::with_capacity(taps);
        for index in 0..taps {
            if index > 0 {
                anchor
                    .focus()
                    .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            }
            let (target_hwnd, target_pid) = if index == 0 && start_visible {
                (root.hwnd, child.process_id())
            } else {
                (anchor.hwnd(), anchor.process_id())
            };
            let input = child
                .send_f11(target_hwnd, target_pid, TAP_TIME)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            input_evidence.push(input.describe());
            let wanted_visible = if taps > 1 {
                index % 2 == 1
            } else {
                end_visible
            };
            let events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
                tap_trace_complete(events, 1, wanted_visible)
            });
            if index == 0 && id == "H0" {
                let runner_observation = runner_observer.as_mut().map(|observer| {
                    let observation = observer.wait_for_vk(0x7A, Duration::from_secs(1));
                    observation.describe()
                });
                let production_down = has_trace(
                    &events,
                    "hook_observed",
                    &["vk=122", "down=true", "injected=true"],
                );
                let production_up = has_trace(
                    &events,
                    "hook_observed",
                    &["vk=122", "down=false", "injected=true"],
                );
                hook_diagnostic.push_str(&format!(
                    "; production F11 callback down/up={production_down}/{production_up} injected; {}",
                    runner_observation.unwrap_or_else(|| "runner F11 observer unavailable".into())
                ));
            }
            if !tap_trace_complete(&events, 1, wanted_visible) {
                let observed_root = child.refresh_root().ok();
                let stage = tap_trace_failure_stage(&events, wanted_visible);
                return Err(CaseFailure::new(
                    stage,
                    format!(
                        "F11 tap {} input edges [{}] lacked complete hook/gesture/visibility evidence for visible={wanted_visible}; root={:?}; observed production edges: {}; {}",
                        index + 1,
                        input.describe(),
                        observed_root.map(|window| (
                            window.visible,
                            window.minimized,
                            window.bounds
                        )),
                        input_trace_summary(&events),
                        hook_diagnostic
                    ),
                ));
            }
            if !wait_root_visibility(child, wanted_visible, ROOT_TIMEOUT) {
                let observed_root = child.refresh_root().ok();
                return Err(CaseFailure::new(
                    FailureStage::NativeRootState,
                    format!(
                        "ROOT did not reach visible={wanted_visible} after tap {}; checked F11 edges [{}]; root={:?}; production edges: {}",
                        index + 1,
                        input.describe(),
                        observed_root.map(|window| (
                            window.visible,
                            window.minimized,
                            window.bounds
                        )),
                        input_trace_summary(&events)
                    ),
                ));
            }
            cursor = trace_lines(trace_path).len();
            root = child
                .refresh_root()
                .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        }
        if end_visible {
            require_visible(&root)
                .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        } else {
            require_hidden(&root)
                .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        }
        Ok(format!(
            "{} checked F11 input pairs reached expected ROOT state; HWND={} bounds={:?}; edges={:?}; {}",
            taps,
            hwnd_id(root.hwnd),
            root.bounds,
            input_evidence,
            hook_diagnostic
        ))
    })();
    append_case(
        report,
        id,
        expected,
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_other_focus_case(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
) {
    let started = Instant::now();
    let result = (|| {
        let root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        require_visible(&root)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        let restore_request = wait_for_latest_root_restore(trace_path, ROOT_TIMEOUT).ok_or_else(|| {
            CaseFailure::new(
                FailureStage::RootCommand,
                "ROOT did not produce a matching native restore-completion edge after H1; H2 anchor input was not sent".into(),
            )
        })?;
        anchor
            .focus()
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let mut cursor = trace_lines(trace_path).len();
        let mut inputs = Vec::with_capacity(2);
        for visible in [false, true] {
            anchor
                .focus()
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            focus_is_validated(anchor.hwnd(), anchor.process_id()).map_err(|error| {
                CaseFailure::new(
                    FailureStage::InputInjection,
                    format!("runner-owned H2 anchor was not foreground immediately before input: {error}"),
                )
            })?;
            let input = child
                .send_f11(anchor.hwnd(), anchor.process_id(), TAP_TIME)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            inputs.push(input.describe());
            if !wait_root_visibility(child, visible, ROOT_TIMEOUT) {
                return Err(CaseFailure::new(
                    FailureStage::NativeRootState,
                    format!("runner-owned focus F11 failed to toggle ROOT to visible={visible}"),
                ));
            }
            let trace = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
                tap_trace_complete(events, 1, visible)
            });
            if !tap_trace_complete(&trace, 1, visible) {
                return Err(CaseFailure::new(FailureStage::GestureDecision, "missing hook admission or one short-tap visibility decision from runner-owned focus".into()));
            }
            cursor = trace_lines(trace_path).len();
        }
        Ok(format!(
            "H1 ROOT restore request {restore_request} completed before runner-owned anchor HWND={} focus; both checked F11 taps targeted the exact runner HWND/PID and toggled ROOT offscreen/back on-screen: {inputs:?}",
            hwnd_id(anchor.hwnd()),
        ))
    })();
    append_case(
        report,
        "H2",
        expected("H2"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_hold_open_case<'a>(
    child: &'a NativeChild,
    anchor: &'a FocusAnchor,
    trace_path: &Path,
    hold_threshold_ms: u64,
    held_windows: &mut Option<Vec<WindowSnapshot>>,
) -> (
    Result<String, CaseFailure>,
    Option<F11HoldGuard<'a>>,
    Option<RunnerHookObserver>,
    Option<String>,
) {
    let mut hold_guard = None;
    let mut hook_observer = None;
    let mut hook_observer_error = None;
    let result = (|| {
        let root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        require_visible(&root)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        child
            .focus_window(&root)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let before = runtime_windows(child);
        let cursor = trace_lines(trace_path).len();
        match RunnerHookObserver::start() {
            Ok(observer) => hook_observer = Some(observer),
            Err(error) => hook_observer_error = Some(error),
        }
        let input = child
            .press_f11(root.hwnd, child.process_id())
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        hold_guard = Some(F11HoldGuard::new(child, anchor));
        std::thread::sleep(Duration::from_millis(
            hold_threshold_ms.saturating_add(250).min(5_000),
        ));
        let after = wait_runtime_windows(child, &before, Duration::from_secs(2))
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        let traces = trace_lines(trace_path)
            .into_iter()
            .skip(cursor)
            .collect::<Vec<_>>();
        let (foreground_hwnd, foreground_pid) = capture_foreground();
        if !has_trace(
            &traces,
            "hook_primary",
            &["transition=Press", "provenance=ExternalInjected"],
        ) || !has_trace(
            &traces,
            "configured_primary",
            &[
                "transition=Press",
                "provenance=ExternalInjected",
                "modifiers_match=true",
            ],
        ) {
            let runner_observation = hook_observer
                .as_mut()
                .map(|observer| observer.wait_for_vk(0x7A, Duration::from_millis(50)))
                .map(|observation| observation.describe())
                .or_else(|| {
                    hook_observer_error
                        .as_ref()
                        .map(|error| format!("runner hook observer unavailable: {error}"))
                })
                .unwrap_or_else(|| "runner hook observer unavailable".into());
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "held F11 down edge [{}] targeted ROOT HWND={} PID={}; foreground after hold HWND={} PID={}; observed {} production trace event(s), but no matching hook/configured press edge; {runner_observation}",
                    input.describe(),
                    hwnd_id(root.hwnd),
                    child.process_id(),
                    hwnd_id(foreground_hwnd),
                    foreground_pid,
                    traces.len()
                ),
            ));
        }
        if has_trace(&traces, "short_tap", &[]) || has_trace(&traces, "desired_visibility", &[]) {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                "hold incorrectly emitted short-tap or ROOT visibility work".into(),
            ));
        }
        if after.len() < 2 {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "hold produced {} stable new visible child-owned radial HWND(s); expected the input and visual surfaces",
                    after.len()
                ),
            ));
        }
        validate_radial_surfaces(child, &after)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        let current_root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        require_visible(&current_root).map_err(|error| {
            CaseFailure::new(
                FailureStage::NativeRootState,
                format!("ROOT changed while opening radial: {error}"),
            )
        })?;
        if !same_window_state(&root, &current_root) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "opening the radial changed ROOT HWND/state/bounds: before={}; after={}",
                    describe_root_snapshot(&root),
                    describe_root_snapshot(&current_root)
                ),
            ));
        }
        *held_windows = Some(after.clone());
        Ok(format!(
            "held F11 opened stable visible child radial surfaces [{}]; ROOT stayed on-screen; down edge=[{}]",
            describe_radial_surfaces(&after),
            input.describe()
        ))
    })();
    if result.is_err() {
        drop(hold_guard.take());
        (result, None, hook_observer, hook_observer_error)
    } else {
        (result, hold_guard, hook_observer, hook_observer_error)
    }
}

fn run_hold_release_case(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    _anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    held_windows: Option<&[WindowSnapshot]>,
    hold_guard: Option<F11HoldGuard<'_>>,
    mut hook_observer: Option<RunnerHookObserver>,
    hook_observer_error: Option<String>,
    h6_repeat_mode: H6RepeatMode,
) -> HoldReleaseHandoff {
    let started = Instant::now();
    let mut release_at_unix_ms = None;
    let mut sentinel_at_unix_ms = None;
    let mut quiescent_acknowledged = false;
    let result = (|| {
        let mut hold_guard = hold_guard.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::InputInjection,
                "H4 did not leave F11 held for a checked release".into(),
            )
        })?;
        let runner_edges_drained = hook_observer
            .as_mut()
            .map_or(0, RunnerHookObserver::drain_pending);
        let cursor = trace_lines(trace_path).len();
        let release = hold_guard
            .release()
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        release_at_unix_ms = Some(release.at_unix_ms);
        let runner_observation = hook_observer
            .as_mut()
            .map(|observer| observer.wait_for_vk_edge(0x7A, false, Duration::from_secs(1)));
        let runner_observation_text = runner_observation
            .as_ref()
            .map(|observation| observation.describe())
            .or_else(|| {
                hook_observer_error
                    .as_ref()
                    .map(|error| format!("observer unavailable: {error}"))
            })
            .unwrap_or_else(|| "observer unavailable".into());
        let lines = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
            has_trace(
                events,
                "hook_primary",
                &["transition=Release", "provenance=ExternalInjected"],
            ) && has_trace(
                events,
                "configured_primary",
                &[
                    "transition=Release",
                    "provenance=ExternalInjected",
                    "modifiers_match=true",
                ],
            )
        });
        if !has_trace(
            &lines,
            "hook_primary",
            &["transition=Release", "provenance=ExternalInjected"],
        ) || !has_trace(
            &lines,
            "configured_primary",
            &[
                "transition=Release",
                "provenance=ExternalInjected",
                "modifiers_match=true",
            ],
        ) {
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "held F11 release edge [{}] did not reach the production hook and configured chord; {runner_observation_text}",
                    release.describe()
                ),
            ));
        }
        let runner_release_observed = runner_observation
            .as_ref()
            .is_some_and(|observation| observation.up_seen && observation.up_injected);
        if !runner_release_observed {
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "held F11 release was not acknowledged by the independent observer; release edge [{}]; {runner_observation_text}",
                    release.describe()
                ),
            ));
        }
        if has_trace(&lines, "short_tap", &[]) || has_trace(&lines, "desired_visibility", &[]) {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                format!(
                    "release after threshold co-fired the short-tap path; release edge [{}]",
                    release.describe()
                ),
            ));
        }
        let held_windows = held_windows.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::NativeRootState,
                "H4 did not identify the radial surface set to check after release".into(),
            )
        })?;
        if !radial_surfaces_are_active(child, held_windows) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "one or more radial surfaces did not remain active after releasing held F11; surfaces=[{}]; release edge [{}]",
                    describe_radial_surfaces(held_windows),
                    release.describe()
                ),
            ));
        }
        let mut handoff_text = String::new();
        if h6_repeat_mode != H6RepeatMode::Immediate {
            let root = child
                .refresh_root()
                .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
            require_visible(&root).map_err(|error| {
                CaseFailure::new(
                    FailureStage::NativeRootState,
                    format!("ROOT changed visibility before the repeat handoff: {error}"),
                )
            })?;
            focus_is_validated(root.hwnd, child.process_id()).map_err(|error| {
                CaseFailure::new(
                    FailureStage::InputInjection,
                    format!(
                        "H5 release did not leave ROOT foreground for the repeat handoff: {error}"
                    ),
                )
            })?;

            let sentinel_cursor = trace_lines(trace_path).len();
            let sentinel = child
                .press_hook_sentinel(root.hwnd, child.process_id())
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            sentinel_at_unix_ms = Some(sentinel.at_unix_ms);
            let runner_sentinel = hook_observer
                .as_mut()
                .map(|observer| observer.wait_for_vk(0x87, Duration::from_secs(1)));
            let sentinel_trace = wait_trace(
                trace_path,
                sentinel_cursor,
                Duration::from_secs(1),
                |events| {
                    has_trace(
                        events,
                        "hook_observed",
                        &["vk=135", "down=true", "injected=true"],
                    ) && has_trace(
                        events,
                        "hook_observed",
                        &["vk=135", "down=false", "injected=true"],
                    )
                },
            );
            let production_sentinel = has_trace(
                &sentinel_trace,
                "hook_observed",
                &["vk=135", "down=true", "injected=true"],
            ) && has_trace(
                &sentinel_trace,
                "hook_observed",
                &["vk=135", "down=false", "injected=true"],
            );
            let observer_sentinel = runner_sentinel.as_ref().is_some_and(|observation| {
                observation.down_seen
                    && observation.up_seen
                    && observation.down_injected
                    && observation.up_injected
            });
            let sentinel_text = runner_sentinel
                .as_ref()
                .map(RunnerHookObservation::describe)
                .or_else(|| {
                    hook_observer_error
                        .as_ref()
                        .map(|error| format!("observer unavailable: {error}"))
                })
                .unwrap_or_else(|| "observer unavailable".into());
            if !production_sentinel || !observer_sentinel {
                return Err(CaseFailure::new(
                    FailureStage::HookAdmission,
                    format!(
                        "quiescent H5→H6 handoff sentinel did not reach both hooks; production_down_up={production_sentinel}; {sentinel_text}; checked F24=[{}]",
                        sentinel.describe()
                    ),
                ));
            }
            quiescent_acknowledged = true;
            handoff_text = format!(
                "; quiescent F24 handoff reached both hooks before H6: production down/up=true/true, {sentinel_text}, checked input=[{}]",
                sentinel.describe()
            );
        }
        Ok(format!(
            "checked F11 release generated no tap or ROOT visibility edge; radial surfaces [{}] remain visible; release edge=[{}]; discarded {runner_edges_drained} pre-release observer edge(s) before correlating this F11 up; {runner_observation_text}{handoff_text}",
            describe_radial_surfaces(held_windows),
            release.describe()
        ))
    })();
    append_case(
        report,
        "H5",
        expected("H5"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
    HoldReleaseHandoff {
        release_at_unix_ms,
        sentinel_at_unix_ms,
        quiescent_acknowledged,
        observer: if h6_repeat_mode != H6RepeatMode::Immediate {
            hook_observer
        } else {
            None
        },
    }
}

fn run_second_hold_case(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hold_threshold_ms: u64,
    held_windows: Option<&[WindowSnapshot]>,
    h6_repeat_mode: H6RepeatMode,
    mut h5_handoff: HoldReleaseHandoff,
) {
    let started = Instant::now();
    let result = (|| {
        let held_windows = held_windows.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::NativeRootState,
                "H4 did not identify the radial surface set to toggle closed".into(),
            )
        })?;
        validate_radial_surfaces(child, held_windows)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        let root_before = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        require_visible(&root_before)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        let root = &root_before;
        if h6_repeat_mode != H6RepeatMode::Immediate {
            if !h5_handoff.quiescent_acknowledged {
                return Err(CaseFailure::new(
                    FailureStage::HookAdmission,
                    "H5 did not complete the production/independent F24 quiescence handoff; H6 input was not sent".into(),
                ));
            }
            focus_is_validated(root.hwnd, child.process_id()).map_err(|error| {
                CaseFailure::new(
                    FailureStage::InputInjection,
                    format!("foreground changed after the acknowledged H5 handoff: {error}"),
                )
            })?;
        } else {
            child
                .focus_window(&root)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        }
        let cursor = trace_lines(trace_path).len();
        let mut observer_drop_probe = None;
        let (mut hook_observer, hook_observer_error) = match h6_repeat_mode {
            H6RepeatMode::Immediate => match RunnerHookObserver::start() {
                Ok(observer) => (Some(observer), None),
                Err(error) => (None, Some(error)),
            },
            H6RepeatMode::Quiescent => {
                let observer = h5_handoff.observer.take();
                let error = observer
                    .is_none()
                    .then(|| "H5 observer was not retained through the quiescent handoff".into());
                (observer, error)
            }
            H6RepeatMode::ProductionOnlyDiagnostic => {
                let mut previous_observer = h5_handoff.observer.take();
                let previous_thread = previous_observer
                    .as_ref()
                    .map(RunnerHookObserver::thread_id);
                let previous_hook = previous_observer.as_ref().map(RunnerHookObserver::hook_id);
                let unhook_result = match previous_observer.as_mut() {
                    Some(observer) => observer
                        .stop_and_report()
                        .map(|()| {
                            format!(
                                "UnhookWindowsHookEx succeeded for runner PID {} owned HHOOK=0x{:x}",
                                std::process::id(),
                                observer.hook_id()
                            )
                        })
                        .unwrap_or_else(|error| format!("observer stop failed: {error}")),
                    None => "H5 observer was unavailable to stop".into(),
                };
                drop(previous_observer);
                let post_unhook_probe =
                    checked_production_f24_without_runner_observer(child, root.hwnd, trace_path);
                observer_drop_probe = Some(format!(
                    "stopped runner observer thread {previous_thread:?} HHOOK={previous_hook:?}: {unhook_result}; immediate checked F24 probe: {post_unhook_probe}"
                ));
                (
                    None,
                    Some(
                        "runner observer intentionally absent for production-only H6 diagnostic"
                            .into(),
                    ),
                )
            }
        };
        let app_hook_thread = hook_service_thread_id(trace_path);
        let runner_thread_before = hook_observer
            .as_ref()
            .map(|observer| format!("runner observer {}", thread_liveness(observer.thread_id())));
        let app_thread_before = app_hook_thread
            .map(|thread_id| format!("app hook {}", thread_liveness(thread_id)))
            .unwrap_or_else(|| "app hook thread id unavailable".into());
        let down = child
            .press_f11(root.hwnd, child.process_id())
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let handoff_description = match h6_repeat_mode {
            H6RepeatMode::Immediate => format!(
                "mode=immediate; H5-release-to-H6-down={}ms",
                h5_handoff
                    .release_at_unix_ms
                    .map_or(0, |released| down.at_unix_ms.saturating_sub(released))
            ),
            H6RepeatMode::Quiescent => format!(
                "mode=quiescent; H5-release-to-F24={}ms; F24-to-H6-down={}ms; both hooks acknowledged H5/F24 before H6",
                h5_handoff
                    .release_at_unix_ms
                    .zip(h5_handoff.sentinel_at_unix_ms)
                    .map_or(0, |(released, sentinel)| sentinel.saturating_sub(released)),
                h5_handoff
                    .sentinel_at_unix_ms
                    .map_or(0, |sentinel| down.at_unix_ms.saturating_sub(sentinel))
            ),
            H6RepeatMode::ProductionOnlyDiagnostic => {
                format!(
                    "mode=production_only_diagnostic; H5 quiescence acknowledged; {}; no runner LL hook installed during H6 down, hold, release, or initial post-release F24",
                    observer_drop_probe
                        .as_deref()
                        .unwrap_or("observer-drop probe unavailable")
                )
            }
        };
        let mut hold_guard = F11HoldGuard::new(child, anchor);
        std::thread::sleep(Duration::from_millis(
            hold_threshold_ms.saturating_add(250).min(5_000),
        ));
        let deadline_events =
            wait_trace(trace_path, cursor, Duration::from_millis(250), |events| {
                has_trace(
                    events,
                    "hook_deadline",
                    &["edge=Fired", "radial_intent=true"],
                )
            });
        let deadline_fired = has_trace(
            &deadline_events,
            "hook_deadline",
            &["edge=Fired", "radial_intent=true"],
        );
        let production_pre_release_pump = app_hook_thread.map_or_else(
            || "production pump probe unavailable: service thread id missing".into(),
            |thread_id| {
                probe_production_hook_pump(
                    thread_id,
                    child.process_id(),
                    trace_path,
                    Duration::from_millis(500),
                )
            },
        );
        let runner_pre_release_pump = hook_observer.as_ref().map_or_else(
            || "runner pump probe unavailable: observer intentionally absent".into(),
            |observer| {
                let probe_id = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
                match observer.pump_roundtrip(probe_id, Duration::from_millis(500)) {
                    Ok(()) => format!(
                        "runner thread {} acknowledged pre-release probe {probe_id}",
                        observer.thread_id()
                    ),
                    Err(error) => error,
                }
            },
        );
        let handoff_description = format!(
            "{handoff_description}; after hold deadline fired={deadline_fired}, before F11 up: {production_pre_release_pump}; {runner_pre_release_pump}"
        );
        let down_observation = hook_observer
            .as_mut()
            .map(|observer| observer.wait_for_vk(0x7A, Duration::from_millis(50)));
        let up = hold_guard
            .release()
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let up_observation = hook_observer
            .as_mut()
            .map(|observer| observer.wait_for_vk(0x7A, Duration::from_millis(250)));
        let runner_thread_after_release = hook_observer
            .as_ref()
            .map(|observer| format!("runner observer {}", thread_liveness(observer.thread_id())));
        let app_thread_after_release = app_hook_thread
            .map(|thread_id| format!("app hook {}", thread_liveness(thread_id)))
            .unwrap_or_else(|| "app hook thread id unavailable".into());
        // Never inject a second key while the toggle chord is still held.  The
        // native hook must observe the actual key-up edge before an independent
        // sentinel is sent to prove the hook chain remains active afterward.
        let sentinel_cursor = trace_lines(trace_path).len();
        let sentinel_input = child
            .press_hook_sentinel(root.hwnd, child.process_id())
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let sentinel_observation = hook_observer
            .as_mut()
            .map(|observer| observer.wait_for_vk(0x87, Duration::from_millis(100)));
        let runner_thread_after_sentinel = hook_observer
            .as_ref()
            .map(|observer| format!("runner observer {}", thread_liveness(observer.thread_id())));
        let app_thread_after_sentinel = app_hook_thread
            .map(|thread_id| format!("app hook {}", thread_liveness(thread_id)))
            .unwrap_or_else(|| "app hook thread id unavailable".into());
        let sentinel_trace = wait_trace(
            trace_path,
            sentinel_cursor,
            Duration::from_millis(250),
            |events| {
                (has_trace(
                    events,
                    "hook_observed",
                    &["vk=135", "down=true", "injected=true"],
                ) && has_trace(
                    events,
                    "hook_observed",
                    &["vk=135", "down=false", "injected=true"],
                )) || has_trace(events, "frontend_key", &["key=F24"])
            },
        );
        let runner_observation = down_observation
            .as_ref()
            .zip(up_observation.as_ref())
            .map(|(down, up)| down.merge(up))
            .or_else(|| down_observation.or(up_observation));
        let runner_observation_text = runner_observation
            .as_ref()
            .map(|observation| {
                let sentinel = sentinel_observation.as_ref().map_or_else(
                    || "runner F24 sentinel observer unavailable".to_string(),
                    |sentinel| format!("{}", sentinel.describe()),
                );
                format!(
                    "{}; after-hold sentinel input=[{}] observer=[{}] production_pair={}",
                    observation.describe(),
                    sentinel_input.describe(),
                    sentinel,
                    has_trace(
                        &sentinel_trace,
                        "hook_observed",
                        &["vk=135", "down=true", "injected=true"]
                    ) && has_trace(
                        &sentinel_trace,
                        "hook_observed",
                        &["vk=135", "down=false", "injected=true"]
                    )
                )
            })
            .or_else(|| hook_observer_error.map(|error| format!("observer unavailable: {error}")))
            .unwrap_or_else(|| "observer unavailable".into());
        let thread_liveness_text = format!(
            "thread liveness before=[{}; {}], after sentinel=[{}; {}], after release=[{}; {}]",
            runner_thread_before
                .as_deref()
                .unwrap_or("runner observer unavailable"),
            app_thread_before,
            runner_thread_after_sentinel
                .as_deref()
                .unwrap_or("runner observer unavailable"),
            app_thread_after_sentinel,
            runner_thread_after_release
                .as_deref()
                .unwrap_or("runner observer unavailable"),
            app_thread_after_release
        );
        let frontend_key_observed = has_trace(&sentinel_trace, "frontend_key", &["key=F24"]);
        let runner_observation_text = format!(
            "{runner_observation_text}; foreground framework received F24 WM_KEYDOWN={frontend_key_observed}; {thread_liveness_text}"
        );
        let lines = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
            has_trace(
                events,
                "hook_primary",
                &["transition=Release", "provenance=ExternalInjected"],
            ) && has_trace(
                events,
                "configured_primary",
                &[
                    "transition=Release",
                    "provenance=ExternalInjected",
                    "modifiers_match=true",
                ],
            )
        });
        let release_traced = has_trace(
            &lines,
            "hook_primary",
            &["transition=Release", "provenance=ExternalInjected"],
        ) && has_trace(
            &lines,
            "configured_primary",
            &[
                "transition=Release",
                "provenance=ExternalInjected",
                "modifiers_match=true",
            ],
        );
        let release_observed = runner_observation.as_ref().is_some_and(|observation| {
            observation.down_seen
                && observation.up_seen
                && observation.down_injected
                && observation.up_injected
        });
        let sentinel_observed = sentinel_observation.as_ref().is_some_and(|observation| {
            observation.down_seen
                && observation.up_seen
                && observation.down_injected
                && observation.up_injected
        });
        let sentinel_traced = has_trace(
            &sentinel_trace,
            "hook_observed",
            &["vk=135", "down=true", "injected=true"],
        ) && has_trace(
            &sentinel_trace,
            "hook_observed",
            &["vk=135", "down=false", "injected=true"],
        );
        let recovery_diagnostic =
            if !release_traced || !release_observed || !sentinel_observed || !sentinel_traced {
                Some(diagnose_hook_delivery_after_hold(
                    child,
                    root.hwnd,
                    app_hook_thread,
                    &mut hook_observer,
                    trace_path,
                ))
            } else {
                None
            };
        let closed = wait_until(Duration::from_secs(2), || {
            radial_surfaces_are_inactive(child, held_windows)
        });
        if !closed {
            let current = child.windows();
            let active_radial = active_radial_surfaces(&current, child.process_id());
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "second full hold did not remove, hide, or park every radial surface [{}]; final active class-matched surfaces=[{}]; production_release={release_traced}; after_hold_production_sentinel={sentinel_traced}; down=[{}] up=[{}]; {runner_observation_text}; {handoff_description}; recovery diagnostic=[{}]",
                    describe_radial_surfaces(held_windows),
                    describe_radial_surfaces(&active_radial),
                    down.describe(),
                    up.describe(),
                    recovery_diagnostic.as_deref().unwrap_or("not run")
                ),
            ));
        }
        if !release_traced || !release_observed || !sentinel_observed || !sentinel_traced {
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "second hold release was not proven by both production hook and independent observer; production_release={release_traced}; after_hold_observer_sentinel={sentinel_observed}; after_hold_production_sentinel={sentinel_traced}; {runner_observation_text}; {handoff_description}; recovery diagnostic=[{}]; down=[{}] up=[{}]",
                    recovery_diagnostic.as_deref().unwrap_or("not run"),
                    down.describe(),
                    up.describe()
                ),
            ));
        }
        if has_trace(&lines, "short_tap", &[]) || has_trace(&lines, "desired_visibility", &[]) {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                format!(
                    "second hold co-fired a short tap or ROOT visibility edge; down=[{}] up=[{}]",
                    down.describe(),
                    up.describe()
                ),
            ));
        }
        let root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        require_visible(&root).map_err(|error| {
            CaseFailure::new(
                FailureStage::NativeRootState,
                format!("ROOT visibility changed during second hold: {error}"),
            )
        })?;
        if !same_window_state(&root_before, &root) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "second hold changed ROOT HWND/state/bounds: before={}; after={}",
                    describe_root_snapshot(&root_before),
                    describe_root_snapshot(&root)
                ),
            ));
        }
        let final_windows = child.windows();
        let active_radial = active_radial_surfaces(&final_windows, child.process_id());
        if !radial_surface_set_and_owner_are_inactive(
            held_windows,
            &final_windows,
            child.process_id(),
        ) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "H6 left candidate-owned radial surfaces active after closing the captured pair [{}]; final active class-matched surfaces=[{}]",
                    describe_radial_surfaces(held_windows),
                    describe_radial_surfaces(&active_radial)
                ),
            ));
        }
        Ok(format!(
            "second threshold hold closed radial surfaces [{}] while ROOT stayed unchanged; down=[{}] up=[{}]; {runner_observation_text}; {handoff_description}",
            describe_radial_surfaces(held_windows),
            down.describe(),
            up.describe()
        ))
    })();
    append_case(
        report,
        "H6",
        expected("H6"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn diagnose_hook_delivery_after_hold(
    child: &NativeChild,
    expected_root_hwnd: windows::Win32::Foundation::HWND,
    app_hook_thread: Option<u32>,
    old_observer: &mut Option<RunnerHookObserver>,
    trace_path: &Path,
) -> String {
    let mut evidence = Vec::new();
    let app_probe_id = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
    let app_probe_cursor = trace_lines(trace_path).len();
    let app_pump_acknowledged = if let Some(thread_id) = app_hook_thread {
        match post_validated_hook_pump_probe(thread_id, child.process_id(), app_probe_id) {
            Ok(()) => {
                let events = wait_trace(
                    trace_path,
                    app_probe_cursor,
                    Duration::from_secs(1),
                    |events| {
                        has_trace(
                            events,
                            "hook_pump_probe",
                            &[&format!("probe_id={app_probe_id}")],
                        )
                    },
                );
                let acknowledged = has_trace(
                    &events,
                    "hook_pump_probe",
                    &[&format!("probe_id={app_probe_id}")],
                );
                evidence.push(format!(
                    "production thread {thread_id} belongs to child PID {}; posted probe {app_probe_id}; pump_ack={acknowledged}",
                    child.process_id()
                ));
                acknowledged
            }
            Err(error) => {
                evidence.push(format!("production pump probe failed: {error}"));
                false
            }
        }
    } else {
        evidence.push("production hook thread id unavailable".into());
        false
    };

    let old_thread_id = old_observer.as_ref().map(RunnerHookObserver::thread_id);
    let runner_probe_id = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
    let old_runner_pump_acknowledged = if let Some(observer) = old_observer.as_ref() {
        match observer.pump_roundtrip(runner_probe_id, Duration::from_secs(1)) {
            Ok(()) => {
                evidence.push(format!(
                    "existing runner observer thread {} acknowledged pump probe {runner_probe_id}",
                    observer.thread_id()
                ));
                true
            }
            Err(error) => {
                evidence.push(error);
                false
            }
        }
    } else {
        evidence.push("existing runner observer unavailable for pump probe".into());
        false
    };

    // Remove the original independent hook before installing a new observer, so
    // this diagnostic can distinguish a stale hook registration from input loss.
    drop(old_observer.take());
    let mut fresh_observer = match RunnerHookObserver::start() {
        Ok(observer) => observer,
        Err(error) => {
            evidence.push(format!("fresh runner observer install failed: {error}"));
            return format!(
                "{}; no fresh observer could be installed",
                evidence.join("; ")
            );
        }
    };
    let fresh_thread_id = fresh_observer.thread_id();
    let fresh_desktop = fresh_observer.desktop.clone();
    let fresh_desktop_is_default = fresh_desktop == "Default";
    let fresh_probe_id = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
    let fresh_pump_acknowledged =
        match fresh_observer.pump_roundtrip(fresh_probe_id, Duration::from_secs(1)) {
            Ok(()) => true,
            Err(error) => {
                evidence.push(error);
                false
            }
        };
    evidence.push(format!(
        "fresh runner observer thread {fresh_thread_id} replaced {:?}; desktop={fresh_desktop:?} default={fresh_desktop_is_default}; pump_ack={fresh_pump_acknowledged}",
        old_thread_id
    ));

    let fresh_sentinel = (|| {
        if !fresh_desktop_is_default {
            return Err(format!(
                "refused fresh-observer F24 because its desktop was {fresh_desktop:?}, expected Default"
            ));
        }
        let root = child.refresh_root()?;
        if root.hwnd != expected_root_hwnd {
            return Err(format!(
                "refused fresh-observer F24 because ROOT HWND changed from {} to {}",
                hwnd_id(expected_root_hwnd),
                hwnd_id(root.hwnd)
            ));
        }
        focus_is_validated(expected_root_hwnd, child.process_id())?;
        let cursor = trace_lines(trace_path).len();
        let input = child.press_hook_sentinel(expected_root_hwnd, child.process_id())?;
        let runner = fresh_observer.wait_for_vk(0x87, Duration::from_secs(1));
        let events = wait_trace(trace_path, cursor, Duration::from_secs(1), |events| {
            has_trace(
                events,
                "hook_observed",
                &["vk=135", "down=true", "injected=true"],
            ) && has_trace(
                events,
                "hook_observed",
                &["vk=135", "down=false", "injected=true"],
            )
        });
        let production_pair = has_trace(
            &events,
            "hook_observed",
            &["vk=135", "down=true", "injected=true"],
        ) && has_trace(
            &events,
            "hook_observed",
            &["vk=135", "down=false", "injected=true"],
        );
        let runner_pair =
            runner.down_seen && runner.up_seen && runner.down_injected && runner.up_injected;
        Ok(format!(
            "checked F24 targeted foreground ROOT HWND={} PID={}; fresh observer desktop={} runner_pair={runner_pair}; production_pair={production_pair}; input=[{}]",
            hwnd_id(expected_root_hwnd),
            child.process_id(),
            fresh_desktop,
            input.describe()
        ))
    })();
    match fresh_sentinel {
        Ok(result) => evidence.push(result),
        Err(error) => evidence.push(format!("fresh checked F24 diagnostic failed: {error}")),
    }

    evidence.push(format!(
        "classification: production_pump_ack={app_pump_acknowledged}, old_runner_pump_ack={old_runner_pump_acknowledged}, fresh_runner_pump_ack={fresh_pump_acknowledged}"
    ));
    evidence.join("; ")
}

fn checked_production_f24_without_runner_observer(
    child: &NativeChild,
    expected_root_hwnd: windows::Win32::Foundation::HWND,
    trace_path: &Path,
) -> String {
    if let Err(error) = focus_is_validated(expected_root_hwnd, child.process_id()) {
        return format!(
            "refused F24 because ROOT HWND={} PID={} was not foreground: {error}",
            hwnd_id(expected_root_hwnd),
            child.process_id()
        );
    }
    let cursor = trace_lines(trace_path).len();
    let input = match child.press_hook_sentinel(expected_root_hwnd, child.process_id()) {
        Ok(input) => input,
        Err(error) => return format!("checked production-only F24 insertion failed: {error}"),
    };
    let events = wait_trace(trace_path, cursor, Duration::from_secs(1), |events| {
        (has_trace(
            events,
            "hook_observed",
            &["vk=135", "down=true", "injected=true"],
        ) && has_trace(
            events,
            "hook_observed",
            &["vk=135", "down=false", "injected=true"],
        )) || has_trace(events, "frontend_key", &["key=F24"])
    });
    let production_pair = has_trace(
        &events,
        "hook_observed",
        &["vk=135", "down=true", "injected=true"],
    ) && has_trace(
        &events,
        "hook_observed",
        &["vk=135", "down=false", "injected=true"],
    );
    let frontend_received = has_trace(&events, "frontend_key", &["key=F24"]);
    format!(
        "checked F24 targeted foreground ROOT HWND={} PID={} with no runner observer; production_pair={production_pair}; frontend_received={frontend_received}; input=[{}]",
        hwnd_id(expected_root_hwnd),
        child.process_id(),
        input.describe()
    )
}

fn probe_production_hook_pump(
    thread_id: u32,
    child_process_id: u32,
    trace_path: &Path,
    timeout: Duration,
) -> String {
    let probe_id = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
    let cursor = trace_lines(trace_path).len();
    if let Err(error) = post_validated_hook_pump_probe(thread_id, child_process_id, probe_id) {
        return format!("production pre-release pump probe {probe_id} failed: {error}");
    }
    let events = wait_trace(trace_path, cursor, timeout, |events| {
        has_trace(
            events,
            "hook_pump_probe",
            &[&format!("probe_id={probe_id}")],
        )
    });
    let acknowledged = has_trace(
        &events,
        "hook_pump_probe",
        &[&format!("probe_id={probe_id}")],
    );
    format!(
        "production thread {thread_id} acknowledged pre-release pump probe {probe_id}={acknowledged}"
    )
}

fn record_post_g2_root_snapshot(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    output: &Path,
    trace_path: &Path,
) -> String {
    let result = collect_post_g2_root_snapshot(child, trace_path);
    match result {
        Ok(snapshot) => {
            let summary = format!(
                "ROOT HWND={} PID={} visible={} minimized={} bounds={:?} intersects_physical_display={} drawable_on_display_samples={}/{} stable_samples={} stable_on_display={}",
                snapshot.root_hwnd,
                snapshot.child_process_id,
                snapshot.visible,
                snapshot.minimized,
                snapshot.bounds,
                snapshot.intersects_physical_display,
                snapshot.drawable_on_display_samples,
                snapshot.sample_count,
                snapshot.consecutive_stable_samples,
                snapshot.stable_on_display
            );
            let path = output.join("case-G2-root-snapshot.json");
            let serialized = serde_json::to_vec_pretty(&snapshot)
                .map_err(|error| format!("serialize post-G2 ROOT snapshot: {error}"))
                .and_then(|bytes| {
                    fs::write(&path, bytes)
                        .map_err(|error| format!("write post-G2 ROOT snapshot: {error}"))
                });
            match serialized {
                Ok(()) => {
                    if let Some(case) = report.cases.iter_mut().find(|case| case.id == "G2") {
                        case.observed = bounded_text(
                            &format!("{}; post-G2 ROOT snapshot: {summary}", case.observed),
                            MAX_RESULT_BYTES,
                        );
                        case.artifacts
                            .push(bounded_text(&path.to_string_lossy(), MAX_PATH_BYTES));
                        if !snapshot.stable_on_display {
                            case.status = CaseStatus::Failed;
                            case.failure_stage = Some(FailureStage::NativeRootState);
                            case.observed = bounded_text(
                                &format!(
                                    "{}; ROOT did not remain visible, drawable, and stable on a physical display for the required post-G2 samples",
                                    case.observed
                                ),
                                MAX_RESULT_BYTES,
                            );
                        }
                    }
                    report.push_artifact(path.to_string_lossy());
                }
                Err(error) => {
                    mark_post_g2_snapshot_failure(report, &error);
                    return format!("snapshot artifact failed: {error}; state={summary}");
                }
            }
            summary
        }
        Err(error) => {
            mark_post_g2_snapshot_failure(report, &error);
            format!("snapshot collection failed: {error}")
        }
    }
}

fn mark_post_g2_snapshot_failure(report: &mut AcceptanceReport, error: &str) {
    if let Some(case) = report.cases.iter_mut().find(|case| case.id == "G2") {
        case.status = CaseStatus::Failed;
        case.failure_stage = Some(FailureStage::NativeRootState);
        case.observed = bounded_text(
            &format!("{}; post-G2 ROOT snapshot failed: {error}", case.observed),
            MAX_RESULT_BYTES,
        );
    }
}

fn collect_post_g2_root_snapshot(
    child: &NativeChild,
    trace_path: &Path,
) -> Result<PostG2RootSnapshot, String> {
    let displays = native_display_bounds()?;
    let deadline = Instant::now() + POST_G2_ROOT_STABILITY_WINDOW;
    let mut previous = None;
    let mut stable_samples = 0_u8;
    let mut sample_count = 0_u8;
    let mut drawable_on_display_samples = 0_u8;
    let root = loop {
        let root = child.refresh_root()?;
        if root.role != WindowRole::Root || root.process_id != child.process_id() {
            return Err(format!(
                "post-G2 snapshot found a non-child ROOT identity: role={:?} pid={} expected_pid={}",
                root.role,
                root.process_id,
                child.process_id()
            ));
        }
        if root.visible
            && !root.minimized
            && root.is_nonzero()
            && intersects_display_bounds(root.bounds, &displays)
        {
            drawable_on_display_samples = drawable_on_display_samples.saturating_add(1);
        }
        let fingerprint = (
            hwnd_id(root.hwnd),
            root.process_id,
            root.visible,
            root.minimized,
            root.bounds,
        );
        if previous == Some(fingerprint) {
            stable_samples = stable_samples.saturating_add(1);
        } else {
            stable_samples = 1;
            previous = Some(fingerprint);
        }
        sample_count = sample_count.saturating_add(1);
        if Instant::now() >= deadline {
            break root;
        }
        std::thread::sleep(WINDOW_POLL);
    };
    let intersects_physical_display = intersects_display_bounds(root.bounds, &displays);
    let stable_on_display = stable_samples >= POST_G2_ROOT_STABLE_SAMPLES
        && drawable_on_display_samples == sample_count
        && root.visible
        && !root.minimized
        && root.is_nonzero()
        && intersects_physical_display;
    Ok(PostG2RootSnapshot {
        captured_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        child_process_id: child.process_id(),
        root_hwnd: hwnd_id(root.hwnd),
        visible: root.visible,
        minimized: root.minimized,
        bounds: root.bounds,
        physical_displays: displays,
        intersects_physical_display,
        sample_count,
        drawable_on_display_samples,
        consecutive_stable_samples: stable_samples,
        stable_on_display,
        root_trace_tail: sanitized_root_trace_tail(trace_path),
    })
}

fn sanitized_root_trace_tail(trace_path: &Path) -> Vec<String> {
    const ROOT_EVENTS: &[&str] = &[
        "root_command",
        "desired_visibility",
        "native_window_snapshot",
        "native_activation",
        "window_sample_truncated",
        "restore",
        "budget_exhausted",
    ];
    trace_lines(trace_path)
        .into_iter()
        .filter(|line| ROOT_EVENTS.iter().any(|event| line.contains(event)))
        .filter_map(|line| sanitize_trace_line(&line))
        .map(|line| bounded_text(&line, 768))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .take(16)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn run_designer_entry(
    child: &mut NativeChild,
    uia: &UiAutomation,
    anchor: &FocusAnchor,
    trace_path: &Path,
) -> Result<DesignerEntry, CaseFailure> {
    if let Some(existing) = child.designer() {
        return Err(CaseFailure::new(
            FailureStage::DesignerEntry,
            format!(
                "cannot claim a fresh Designer InitialSnapshot while an owned Designer HWND is already open: HWND={} PID={}",
                hwnd_id(existing.hwnd),
                existing.process_id
            ),
        ));
    }
    let root = child
        .refresh_root()
        .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
    if !root.visible || root.minimized {
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            format!(
                "ROOT is not drawable before Designer entry: visible={} minimized={} bounds={:?}",
                root.visible, root.minimized, root.bounds
            ),
        ));
    }
    let display_bounds = native_display_bounds().map_err(|error| {
        CaseFailure::new(
            FailureStage::Environment,
            format!("could not inspect display bounds before Designer entry: {error}"),
        )
    })?;
    let (mut root, mut restored) =
        ensure_root_on_physical_display(child, anchor, &root, &display_bounds, ROOT_TIMEOUT)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    require_visible(&root)
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    if !uia.root_is_queryable(root.hwnd, child.process_id()) {
        return Err(CaseFailure::new(
            FailureStage::DesignerEntry,
            "ROOT UIA root is not owned by the launched candidate".into(),
        ));
    }
    // The Apps menu is nested inside File. UIA can retain the submenu command
    // while its popup is closed, so use the production ROOT menu trace as the
    // open-state oracle and click the File parent only when a popup is active.
    let apps_popup_item = uia
        .find_named(root.hwnd, child.process_id(), "Edit Radial Menus")
        .map_err(|error| CaseFailure::new(FailureStage::DesignerEntry, error))?;
    let menu_state = root_menu_state_from_trace(&trace_lines(trace_path)).ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerEntry,
            "ROOT menu state is unavailable or its bounded trace overflowed; refusing speculative menu input".into(),
        )
    })?;
    let closed_apps_menu = if menu_state.any_open() {
        let click = activate_named(
            uia,
            child,
            anchor,
            &root,
            "File",
            FailureStage::DesignerEntry,
            trace_path,
        )
        .map_err(|error| {
            CaseFailure::new(
                error.stage,
                format!(
                    "could not close the published ROOT File/Apps menu by clicking the parent toggle: {}",
                    error.message
                ),
            )
        })?;
        let menus_closed = wait_until(UIA_TIMEOUT, || {
            root_menu_state_from_trace(&trace_lines(trace_path))
                .is_some_and(|state| !state.any_open())
        });
        let closed_state = root_menu_state_from_trace(&trace_lines(trace_path));
        if !menus_closed || !closed_state.is_some_and(|state| !state.any_open()) {
            return Err(CaseFailure::new(
                FailureStage::DesignerEntry,
                format!(
                    "ROOT File/Apps popup remained open in production trace after checked File-parent toggle click=[{}]; refusing to enter Designer with an unknown menu state",
                    click.describe()
                ),
            ));
        }
        Some(format!(
            "checked File-parent click=[{}] closed production menu state from {:?}; UIA submenu node present before click={}",
            click.describe(),
            menu_state,
            apps_popup_item.is_some()
        ))
    } else if apps_popup_item.is_some() {
        Some(format!(
            "no popup input sent: production trace reports File/Apps closed despite the UIA submenu node being published; state={menu_state:?}"
        ))
    } else {
        None
    };
    let (fresh_root, restored_after_menu) =
        ensure_root_on_physical_display(child, anchor, &root, &display_bounds, ROOT_TIMEOUT)
            .map_err(|error| {
                CaseFailure::new(
                    FailureStage::NativeRootState,
                    format!(
                        "ROOT normalization after Apps popup close click={closed_apps_menu:?} failed: {error}"
                    ),
                )
            })?;
    root = fresh_root;
    if restored.is_none() {
        restored = restored_after_menu;
    }
    let trace_cursor = trace_lines(trace_path).len();
    let file_menu = open_root_file_menu(uia, child, anchor, &root, trace_path).map_err(
        |mut error| {
            error.message = format!(
                "{}; entry ROOT bounds={:?}, screenshot display bounds={display_bounds:?}, display_intersection={}, USER32 virtual_intersection={}, checked_F11_restore={}, closed_apps_menu_click={closed_apps_menu:?}",
                error.message,
                root.bounds,
                intersects_display_bounds(root.bounds, &display_bounds),
                root.intersects_virtual_screen(),
                restored.as_deref().unwrap_or("not needed")
            );
            error
        },
    )?;
    let apps_click = activate_named(
        uia,
        child,
        anchor,
        &root,
        "Apps",
        FailureStage::DesignerEntry,
        trace_path,
    )
    .map_err(|error| {
        CaseFailure::new(
            error.stage,
            format!(
                "{}; checked File menu transition={file_menu}",
                error.message
            ),
        )
    })?;
    let edit_click = activate_named(
        uia,
        child,
        anchor,
        &root,
        "Edit Radial Menus",
        FailureStage::DesignerEntry,
        trace_path,
    )?;
    let mut enter_events = None;
    if !wait_until(Duration::from_millis(750), || {
        find_child_window(child, WindowRole::Designer).is_some()
    }) {
        let (fresh_root, _) =
            ensure_root_on_physical_display(child, anchor, &root, &display_bounds, ROOT_TIMEOUT)
                .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        root = fresh_root;
        let edit_control =
            wait_named_control_in_client(uia, child, &root, "Edit Radial Menus", UIA_TIMEOUT)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerEntry, error))?;
        child
            .focus_window(&root)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        uia.focus(&edit_control)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        enter_events = Some(
            send_enter_current(child, &root)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?,
        );
    }
    let designer = wait_for_designer(child)
        .map_err(|error| {
            CaseFailure::new(
                FailureStage::DesignerEntry,
                format!(
                    "{error}; checked File menu transition={file_menu}; semantic pointer events inserted: Apps=[{}], Edit Radial Menus=[{}]; validated UIA focus+Enter events inserted={enter_events:?}",
                    apps_click.describe(),
                    edit_click.describe()
                ),
            )
        })?;
    if child
        .windows()
        .iter()
        .filter(|window| window.role == WindowRole::Designer)
        .count()
        != 1
    {
        return Err(CaseFailure::new(
            FailureStage::DesignerNativeTarget,
            "expected exactly one child-owned Radial Designer HWND after entry".into(),
        ));
    }
    if !uia.root_is_queryable(designer.hwnd, child.process_id()) {
        return Err(CaseFailure::new(
            FailureStage::DesignerNativeTarget,
            "Designer HWND was found but its UIA root was not queryable as the child PID".into(),
        ));
    }
    let snapshot_accepted = wait_trace(
        trace_path,
        trace_cursor,
        Duration::from_secs(10),
        |events| {
            has_trace(
                events,
                "authoring",
                &["edge=ReplyAccepted", "request_kind=Snapshot"],
            )
        },
    );
    let Some(snapshot_event) = snapshot_accepted.iter().rev().find(|line| {
        line.contains("trace_event=\"authoring\"")
            && line.contains("edge=ReplyAccepted")
            && line.contains("request_kind=Snapshot")
    }) else {
        return Err(CaseFailure::new(
            FailureStage::DesignerReadiness,
            "Designer did not emit an accepted InitialSnapshot reply; UIA enabled state alone is not sufficient readiness evidence".into(),
        ));
    };
    if !has_trace(
        &snapshot_accepted,
        "authoring",
        &["edge=ReplyAccepted", "request_kind=Snapshot"],
    ) {
        return Err(CaseFailure::new(
            FailureStage::DesignerReadiness,
            "Designer did not emit an accepted InitialSnapshot reply; UIA enabled state alone is not sufficient readiness evidence".into(),
        ));
    }
    // The accepted InitialSnapshot is the entry-readiness signal. D1 separately proves
    // readiness through a real semantic Tree click, production Enabled body trace, and
    // changed widget state; AccessKit's enabled bit alone is not a reliable loading signal.
    let session_id = trace_field(snapshot_event, "session_id")
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "accepted InitialSnapshot reply omitted its Designer session identity".into(),
            )
        })?;
    Ok(DesignerEntry {
        window: designer,
        session_id,
        root_recovery: restored,
        root_menu_resolution: closed_apps_menu,
    })
}

fn open_root_file_menu(
    uia: &UiAutomation,
    child: &NativeChild,
    anchor: &FocusAnchor,
    root: &WindowSnapshot,
    trace_path: &Path,
) -> Result<String, CaseFailure> {
    const FILE_MENU_ATTEMPTS: usize = 2;
    const FILE_MENU_OPEN_TIMEOUT: Duration = Duration::from_millis(900);

    let mut clicks = Vec::with_capacity(FILE_MENU_ATTEMPTS);
    for attempt in 0..FILE_MENU_ATTEMPTS {
        let state = root_menu_state_from_trace(&trace_lines(trace_path)).ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerEntry,
                "ROOT menu state became unavailable or its bounded trace overflowed while opening File".into(),
            )
        })?;
        if root_file_menu_ready_for_apps(state) {
            return Ok(format!(
                "production File menu was already open with Apps closed; checked clicks={}",
                clicks
                    .iter()
                    .map(PointerClickEvidence::describe)
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if state.any_open() {
            return Err(CaseFailure::new(
                FailureStage::DesignerEntry,
                format!(
                    "ROOT menu state changed to {state:?} after popup normalization; refusing to toggle an unknown submenu"
                ),
            ));
        }

        let click = activate_named(
            uia,
            child,
            anchor,
            root,
            "File",
            FailureStage::DesignerEntry,
            trace_path,
        )
        .map_err(|error| {
            CaseFailure::new(
                error.stage,
                format!(
                    "checked File-parent click attempt {} failed: {}",
                    attempt + 1,
                    error.message
                ),
            )
        })?;
        clicks.push(click);

        let opened = wait_until(FILE_MENU_OPEN_TIMEOUT, || {
            root_menu_state_from_trace(&trace_lines(trace_path))
                .is_some_and(root_file_menu_ready_for_apps)
        });
        let state = root_menu_state_from_trace(&trace_lines(trace_path)).ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerEntry,
                "ROOT menu state became unavailable or its bounded trace overflowed after the checked File click".into(),
            )
        })?;
        if opened && root_file_menu_ready_for_apps(state) {
            return Ok(format!(
                "production File menu opened with Apps closed after {} checked click(s): [{}]",
                clicks.len(),
                clicks
                    .iter()
                    .map(PointerClickEvidence::describe)
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if !should_retry_root_file_menu_open(state) {
            return Err(CaseFailure::new(
                FailureStage::DesignerEntry,
                format!(
                    "checked File-parent click(s) did not reach the expected File-open/Apps-closed state; production state={state:?}, clicks=[{}]",
                    clicks
                        .iter()
                        .map(PointerClickEvidence::describe)
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            ));
        }
    }

    let displays = native_display_bounds().map_err(|error| {
        CaseFailure::new(
            FailureStage::Environment,
            format!("could not inspect displays before checked File keyboard activation: {error}"),
        )
    })?;
    let fresh_root = child
        .refresh_root()
        .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
    if fresh_root.hwnd != root.hwnd || fresh_root.process_id != root.process_id {
        return Err(CaseFailure::new(
            FailureStage::WindowDiscovery,
            "ROOT identity changed before the checked File keyboard activation".into(),
        ));
    }
    let (stable_root, _) =
        ensure_root_on_physical_display(child, anchor, &fresh_root, &displays, ROOT_TIMEOUT)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    let current_state = root_menu_state_from_trace(&trace_lines(trace_path)).ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerEntry,
            "ROOT menu state became unavailable or its bounded trace overflowed before the checked File keyboard activation".into(),
        )
    })?;
    if !should_retry_root_file_menu_open(current_state) {
        return Err(CaseFailure::new(
            FailureStage::DesignerEntry,
            format!(
                "ROOT entered an ambiguous menu state before keyboard activation: {current_state:?}"
            ),
        ));
    }
    let file_control = wait_named_control_in_client(uia, child, &stable_root, "File", UIA_TIMEOUT)
        .map_err(|error| CaseFailure::new(FailureStage::DesignerEntry, error))?;
    child
        .focus_window(&stable_root)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    uia.focus(&file_control)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let enter_events = send_enter_current(child, &stable_root)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let opened = wait_until(FILE_MENU_OPEN_TIMEOUT, || {
        root_menu_state_from_trace(&trace_lines(trace_path))
            .is_some_and(root_file_menu_ready_for_apps)
    });
    let final_state = root_menu_state_from_trace(&trace_lines(trace_path)).ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerEntry,
            "ROOT menu state became unavailable or its bounded trace overflowed after the checked File keyboard activation".into(),
        )
    })?;
    if opened && root_file_menu_ready_for_apps(final_state) {
        return Ok(format!(
            "production File menu opened with Apps closed after {} checked pointer click(s) and UIA-focused Enter inserted={enter_events}; clicks=[{}]",
            clicks.len(),
            clicks
                .iter()
                .map(PointerClickEvidence::describe)
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }

    Err(CaseFailure::new(
        FailureStage::DesignerEntry,
        format!(
            "production File menu remained closed after {} checked pointer click(s) and UIA-focused Enter inserted={enter_events}; state={final_state:?}, clicks=[{}]",
            clicks.len(),
            clicks
                .iter()
                .map(PointerClickEvidence::describe)
                .collect::<Vec<_>>()
                .join("; ")
        ),
    ))
}

pub(super) fn native_display_bounds() -> Result<Vec<[i32; 4]>, String> {
    let screens = screenshots::Screen::all()
        .map_err(|error| format!("enumerate physical display bounds: {error:?}"))?;
    let bounds = screens
        .into_iter()
        .take(16)
        .map(|screen| {
            let display = screen.display_info;
            [
                display.x,
                display.y,
                display
                    .x
                    .saturating_add(i32::try_from(display.width).unwrap_or(i32::MAX)),
                display
                    .y
                    .saturating_add(i32::try_from(display.height).unwrap_or(i32::MAX)),
            ]
        })
        .collect::<Vec<_>>();
    if bounds.is_empty() {
        return Err("display enumeration returned no monitors".into());
    }
    Ok(bounds)
}

pub(super) fn intersects_display_bounds(window: [i32; 4], displays: &[[i32; 4]]) -> bool {
    displays.iter().any(|display| {
        window[0] < display[2]
            && window[2] > display[0]
            && window[1] < display[3]
            && window[3] > display[1]
    })
}

fn ensure_root_on_physical_display(
    child: &NativeChild,
    anchor: &FocusAnchor,
    expected: &WindowSnapshot,
    displays: &[[i32; 4]],
    timeout: Duration,
) -> Result<(WindowSnapshot, Option<String>), String> {
    if expected.process_id != child.process_id() || expected.role != WindowRole::Root {
        return Err(
            "refused to normalize a ROOT snapshot not owned by the launched candidate".into(),
        );
    }

    let initial = child.refresh_root()?;
    if initial.hwnd != expected.hwnd || initial.process_id != expected.process_id {
        return Err(format!(
            "ROOT identity changed before UI interaction: expected HWND={} PID={}, found HWND={} PID={}",
            hwnd_id(expected.hwnd),
            expected.process_id,
            hwnd_id(initial.hwnd),
            initial.process_id
        ));
    }
    if !initial.visible || initial.minimized || !initial.is_nonzero() {
        return Err(format!(
            "ROOT is not drawable before UI interaction: visible={} minimized={} bounds={:?}",
            initial.visible, initial.minimized, initial.bounds
        ));
    }

    let mut restored = None;
    if !intersects_display_bounds(initial.bounds, displays) {
        anchor.focus().map_err(|error| {
            format!(
                "ROOT requires checked recovery from offscreen bounds {:?} (visible={}, minimized={}, displays={displays:?}); runner anchor focus failed: {error}",
                initial.bounds, initial.visible, initial.minimized
            )
        })?;
        let tap = child.send_f11(anchor.hwnd(), anchor.process_id(), TAP_TIME)?;
        restored = Some(format!(
            "checked runner-anchored F11 restored parked ROOT HWND={} with down/up events=[{},{}]",
            hwnd_id(initial.hwnd),
            tap.down.inserted,
            tap.up.inserted
        ));
    }

    let deadline = Instant::now() + timeout;
    let mut previous_bounds = None;
    let mut stable_samples = 0_u8;
    loop {
        let fresh = child.refresh_root()?;
        if fresh.hwnd != expected.hwnd || fresh.process_id != expected.process_id {
            return Err(format!(
                "ROOT identity changed while stabilizing: expected HWND={} PID={}, found HWND={} PID={}",
                hwnd_id(expected.hwnd),
                expected.process_id,
                hwnd_id(fresh.hwnd),
                fresh.process_id
            ));
        }
        if !fresh.visible || fresh.minimized || !fresh.is_nonzero() {
            return Err(format!(
                "ROOT stopped being drawable while stabilizing: visible={} minimized={} bounds={:?}",
                fresh.visible, fresh.minimized, fresh.bounds
            ));
        }
        if intersects_display_bounds(fresh.bounds, displays) {
            if previous_bounds == Some(fresh.bounds) {
                stable_samples = stable_samples.saturating_add(1);
            } else {
                stable_samples = 1;
                previous_bounds = Some(fresh.bounds);
            }
            if stable_samples >= 2 {
                return Ok((fresh, restored));
            }
        } else {
            previous_bounds = None;
            stable_samples = 0;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "ROOT did not stabilize on a physical display within {} ms: HWND={} PID={} bounds={:?} displays={displays:?}; restoration={}",
                timeout.as_millis(),
                hwnd_id(fresh.hwnd),
                fresh.process_id,
                fresh.bounds,
                restored.as_deref().unwrap_or("not needed")
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn run_designer_focus_case(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    uia: &UiAutomation,
    designer: &WindowSnapshot,
    trace_path: &Path,
    output: &Path,
) {
    let started = Instant::now();
    let result = (|| {
        if !uia.root_is_queryable(designer.hwnd, child.process_id()) {
            return Err(CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "Designer HWND stopped responding before H3".into(),
            ));
        }
        let mut cursor = trace_lines(trace_path).len();
        for visible in [false, true] {
            child
                .focus_window(designer)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            let (mut observer, observer_error) = match RunnerHookObserver::start() {
                Ok(observer) => (Some(observer), None),
                Err(error) => (None, Some(error)),
            };
            let tap = child
                .send_f11(designer.hwnd, child.process_id(), TAP_TIME)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            let observation = observer
                .as_mut()
                .map(|observer| observer.wait_for_vk(0x7A, Duration::from_secs(1)));
            let observer_text = observation
                .as_ref()
                .map(RunnerHookObservation::describe)
                .or_else(|| observer_error.map(|error| format!("observer unavailable: {error}")))
                .unwrap_or_else(|| "observer unavailable".into());
            if !wait_root_visibility(child, visible, ROOT_TIMEOUT) {
                let events = trace_lines(trace_path)
                    .into_iter()
                    .skip(cursor)
                    .filter(|line| {
                        line.contains("trace_event=\"hook_observed\"")
                            || line.contains("trace_event=\"hook_primary\"")
                            || line.contains("trace_event=\"configured_primary\"")
                            || line.contains("trace_event=\"hook_callback\"")
                    })
                    .collect::<Vec<_>>();
                return Err(CaseFailure::new(
                    FailureStage::NativeRootState,
                    format!(
                        "Designer-focused F11 failed to toggle ROOT to visible={visible}; checked tap=[{}]; {observer_text}; production hook edges={:?}",
                        tap.describe(),
                        events
                    ),
                ));
            }
            if !uia.root_is_queryable(designer.hwnd, child.process_id()) {
                return Err(CaseFailure::new(
                    FailureStage::DesignerReadiness,
                    "Designer stopped servicing UIA after the ROOT toggle".into(),
                ));
            }
            let events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
                tap_trace_complete(events, 1, visible)
            });
            if !tap_trace_complete(&events, 1, visible) {
                return Err(CaseFailure::new(
                    FailureStage::GestureDecision,
                    "Designer-focused tap did not produce the configured short-tap path".into(),
                ));
            }
            cursor = trace_lines(trace_path).len();
        }
        Ok(format!(
            "Designer HWND={} remained queryable while two checked focused taps toggled ROOT only",
            hwnd_id(designer.hwnd)
        ))
    })();
    append_case(
        report,
        "H3",
        expected("H3"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_designer_pointer_case(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    uia: &UiAutomation,
    designer: &WindowSnapshot,
    trace_path: &Path,
    output: &Path,
    case_expected: &str,
    exercise_tree_round_trip: bool,
) {
    let started = Instant::now();
    let result = (|| {
        save_uia_snapshot("D1", "designer", uia, designer.hwnd, output);
        let mut before = wait_for_designer_semantic_target(
            trace_path,
            DesignerSemanticTarget::Tree,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "production Designer did not publish its Tree semantic target".into(),
            )
        })?;
        let mut tree_round_trip = false;
        if before.selected && exercise_tree_round_trip {
            let deselect_cursor = trace_lines(trace_path).len();
            click_designer_client_bounds(child, designer, before.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
            let deselect_events =
                wait_trace(trace_path, deselect_cursor, TRACE_TIMEOUT, |events| {
                    checked_designer_toggle_transition(
                        events,
                        DesignerSemanticTarget::Tree,
                        before,
                        false,
                    )
                    .is_some()
                });
            before = checked_designer_toggle_transition(
                &deselect_events,
                DesignerSemanticTarget::Tree,
                before,
                false,
            )
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerFrameworkInput,
                    "checked CP_D1 pointer input did not clear the selected Tree through a same-session production widget transition".into(),
                )
            })?;
            tree_round_trip = true;
        }
        if before.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Tree semantic target was already selected before the native click".into(),
            ));
        }
        let cursor = trace_lines(trace_path).len();
        let click_evidence =
            click_designer_client_bounds(child, designer, before.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
        let events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
            checked_designer_toggle_transition(events, DesignerSemanticTarget::Tree, before, true)
                .is_some()
        });
        if checked_designer_toggle_transition(&events, DesignerSemanticTarget::Tree, before, true)
            .is_none()
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerFrameworkInput,
                format!(
                    "native click did not reach the same-session production Designer pointer/body/Tree accepted boundaries; click proof=[{}]",
                    click_evidence.describe()
                ),
            ));
        }
        if exercise_tree_round_trip {
            return Ok(format!(
                "evidence:v1; tree_round_trip={tree_round_trip}; checked_pointer=true; tree_selected=true"
            ));
        }
        Ok(format!(
            "native click on production egui Tree SelectableLabel changed selected {} -> true and reached Enabled body/accepted widget; client bounds={:?}; click edges=[{}]",
            before.selected,
            before.bounds,
            click_evidence.describe()
        ))
    })();
    append_case(
        report,
        "D1",
        case_expected,
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_tab_case(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    uia: &UiAutomation,
    designer: &WindowSnapshot,
    output: &Path,
    trace_path: &Path,
    restore_menu_name: &str,
    select_menus_first: bool,
) {
    let started = Instant::now();
    let result = (|| {
        save_uia_snapshot("D2", "designer", uia, designer.hwnd, output);
        let mut click_proofs = Vec::new();
        let mut menus_click_count = 0usize;
        if select_menus_first {
            let menus = wait_for_designer_semantic_target(
                trace_path,
                DesignerSemanticTarget::Menus,
                UIA_TIMEOUT,
                |_| true,
            )
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerNativeTarget,
                    "production Designer did not publish its Menus semantic target".into(),
                )
            })?;
            let cursor = trace_lines(trace_path).len();
            let click = click_designer_client_bounds(child, designer, menus.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
            click_proofs.push(format!("Menus=[{}]", click.describe()));
            menus_click_count = 1;
            let events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
                checked_designer_toggle_transition(
                    events,
                    DesignerSemanticTarget::Menus,
                    menus,
                    true,
                )
                .is_some()
            });
            if checked_designer_toggle_transition(
                &events,
                DesignerSemanticTarget::Menus,
                menus,
                true,
            )
            .is_none()
            {
                return Err(CaseFailure::new(
                    FailureStage::DesignerFrameworkInput,
                    format!(
                        "checked native click did not select Menus in the same Designer session and generation; proof=[{}]",
                        click.describe()
                    ),
                ));
            }
        }
        let tree = wait_for_designer_semantic_target(
            trace_path,
            DesignerSemanticTarget::Tree,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "production Designer did not publish its Tree semantic target".into(),
            )
        })?;
        if !tree.selected {
            let click = click_designer_client_bounds(child, designer, tree.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
            click_proofs.push(format!("Tree=[{}]", click.describe()));
            wait_for_designer_semantic_target(
                trace_path,
                DesignerSemanticTarget::Tree,
                TRACE_TIMEOUT,
                |state| state.selected,
            )
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerFrameworkInput,
                    "native Tree click did not show the Tree pane".into(),
                )
            })?;
        }
        let inspector = wait_for_designer_semantic_target(
            trace_path,
            DesignerSemanticTarget::Inspector,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "production Designer did not publish its Inspector toolbar target".into(),
            )
        })?;
        if !inspector.selected {
            let inspector_cursor = trace_lines(trace_path).len();
            let click = click_designer_client_bounds(child, designer, inspector.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
            click_proofs.push(format!("Inspector=[{}]", click.describe()));
            let inspector_events =
                wait_trace(trace_path, inspector_cursor, TRACE_TIMEOUT, |events| {
                    events.iter().any(|line| {
                        parse_designer_semantic_target(line, DesignerSemanticTarget::Inspector)
                            .is_some_and(|state| state.selected)
                    })
                });
            let inspector_selected = inspector_events.iter().any(|line| {
                parse_designer_semantic_target(line, DesignerSemanticTarget::Inspector)
                    .is_some_and(|state| state.selected)
            });
            if !inspector_selected {
                return Err(CaseFailure::new(
                    FailureStage::DesignerFrameworkInput,
                    format!(
                        "native Inspector click did not show the Inspector pane; native setup clicks={click_proofs:?}; Inspector pointer/semantic edges={:?}",
                        inspector_events
                            .iter()
                            .filter(|line| {
                                line.contains("designer_widget_pointer")
                                    || line.contains("designer_widget")
                                    || line.contains("target=Inspector")
                            })
                            .collect::<Vec<_>>()
                    ),
                ));
            }
        }
        let default_menu = wait_for_designer_semantic_target(
            trace_path,
            DesignerSemanticTarget::DefaultMenu,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "production Designer did not publish the default menu tree target".into(),
            )
        })?;
        if !default_menu.selected {
            let click =
                click_designer_client_bounds(child, designer, default_menu.bounds, trace_path)
                    .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
            click_proofs.push(format!("default menu=[{}]", click.describe()));
            wait_for_designer_semantic_target(
                trace_path,
                DesignerSemanticTarget::DefaultMenu,
                TRACE_TIMEOUT,
                |state| state.selected,
            )
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerFrameworkInput,
                    "native default-menu click did not select the production menu".into(),
                )
            })?;
        }
        let menu_name = wait_for_designer_semantic_target(
            trace_path,
            DesignerSemanticTarget::MenuName,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "production menu Inspector did not publish its real TextEdit".into(),
            )
        })?;
        let focus_cursor = trace_lines(trace_path).len();
        let name_click =
            click_designer_client_bounds(child, designer, menu_name.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
        click_proofs.push(format!("menu TextEdit=[{}]", name_click.describe()));
        let focused_name = wait_trace(trace_path, focus_cursor, TRACE_TIMEOUT, |events| {
            events.iter().any(|line| {
                parse_designer_semantic_target(line, DesignerSemanticTarget::MenuName)
                    .is_some_and(|state| state.focused)
            })
        });
        if !focused_name.iter().any(|line| {
            parse_designer_semantic_target(line, DesignerSemanticTarget::MenuName)
                .is_some_and(|state| state.focused)
        }) {
            return Err(CaseFailure::new(
                FailureStage::DesignerFrameworkInput,
                format!(
                    "native click on the production menu TextEdit did not acquire egui focus; click=[{}]",
                    name_click.describe()
                ),
            ));
        }
        let uia_menu_edit_focus = uia
            .edit_focus_at_screen_point(designer.hwnd, child.process_id(), name_click.screen_point)
            .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
        let uia_limitation = match uia_menu_edit_focus {
            Some(true) => {
                "UIA Edit at the production TextEdit point reports focus; production egui trace independently confirms the target"
            }
            Some(false) => {
                "UIA exposes an Edit at the production TextEdit point but does not report its keyboard focus; production egui trace is the focus fallback"
            }
            None => {
                "UIA exposes no Edit node at the production TextEdit point; production egui semantic trace supplies target bounds and focus"
            }
        };

        let edit_cursor = trace_lines(trace_path).len();
        let probe_input = replace_focused_designer_text(child, designer, DESIGNER_TEXT_PROBE);
        let (probe_input_count, probe_error) = match probe_input {
            Ok(count) => (Some(count), None),
            Err(error) => (None, Some(error)),
        };
        let edited = wait_trace(trace_path, edit_cursor, TRACE_TIMEOUT, |events| {
            has_trace(
                events,
                "designer_edit_state",
                &[
                    "widget_changed=true",
                    "model_changed=true",
                    "input_matches_model=true",
                    "draft_dirty=true",
                ],
            )
        });
        let model_edit_proved = has_trace(
            &edited,
            "designer_edit_state",
            &[
                "widget_changed=true",
                "model_changed=true",
                "input_matches_model=true",
                "draft_dirty=true",
            ],
        );
        if !model_edit_proved {
            let restored = replace_focused_designer_text(child, designer, restore_menu_name);
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "native Unicode input did not prove a production TextEdit-to-model mutation; input={probe_input_count:?}, error={probe_error:?}, baseline restore={restored:?}"
                ),
            ));
        }

        let restore_cursor = trace_lines(trace_path).len();
        let restore_input = replace_focused_designer_text(child, designer, restore_menu_name)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let restored = wait_trace(trace_path, restore_cursor, TRACE_TIMEOUT, |events| {
            has_trace(
                events,
                "designer_edit_state",
                &[
                    "widget_changed=true",
                    "input_matches_model=true",
                    "draft_dirty=false",
                ],
            )
        });
        if !has_trace(
            &restored,
            "designer_edit_state",
            &[
                "widget_changed=true",
                "input_matches_model=true",
                "draft_dirty=false",
            ],
        ) {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "native TextEdit input changed the production draft, but the deterministic starter value was not restored to a clean checkpoint; restore input events={restore_input}"
                ),
            ));
        }

        let cursor = trace_lines(trace_path).len();
        let count = send_tab(child, designer)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
            events.iter().any(|line| {
                parse_designer_semantic_target(line, DesignerSemanticTarget::MenuDefaultSkin)
                    .is_some_and(|state| state.focused)
            })
        });
        let next = events.iter().find_map(|line| {
            parse_designer_semantic_target(line, DesignerSemanticTarget::MenuDefaultSkin)
                .filter(|state| state.focused)
        });
        let Some(next_state) = next else {
            return Err(CaseFailure::new(
                FailureStage::DesignerFrameworkInput,
                "checked Tab did not move focus from the production menu TextEdit to its menu-skin ComboBox"
                    .into(),
            ));
        };
        let _ = (menu_name, next_state, uia_limitation, click_proofs);
        let evidence = if select_menus_first {
            format!(
                "evidence:v1; menus_selected=true; menus_clicks={menus_click_count}; text_edit=restored; tab_focus=menu_combo; unsaved=false; tab_events={count}; unicode_events={probe_input_count:?}"
            )
        } else {
            format!(
                "evidence:v1; text_edit=restored; tab_focus=menu_combo; unsaved=false; tab_events={count}; unicode_events={probe_input_count:?}"
            )
        };
        Ok(evidence)
    })();
    append_case(
        report,
        "D2",
        expected("D2"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn replace_focused_designer_text(
    child: &NativeChild,
    designer: &WindowSnapshot,
    value: &str,
) -> Result<usize, String> {
    let selection_events = send_select_all_to_focused_window(child, designer)?;
    let text_events = send_text_to_focused_window(child, designer, value)?;
    Ok(selection_events.saturating_add(text_events))
}

fn run_skins_command_case(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    uia: &UiAutomation,
    designer: &WindowSnapshot,
    trace_path: &Path,
    output: &Path,
) {
    let started = Instant::now();
    let result = (|| {
        let root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        require_visible(&root)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        child
            .focus_window(&root)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        save_uia_snapshot("D4", "root", uia, root.hwnd, output);
        save_uia_snapshot("D4", "designer", uia, designer.hwnd, output);
        let edit = uia
            .find_first_edit(root.hwnd, child.process_id())
            .map_err(|error| CaseFailure::new(FailureStage::DesignerEntry, error))?
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerEntry,
                    "ROOT did not publish its semantic Edit query control in UIA".into(),
                )
            })?;
        let (selected_all, typed) = replace_text(child, &root, &edit, &uia, "radial skins")
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        if !uia
            .wait_edit_value(&edit, "radial skins", Duration::from_secs(2))
            .map_err(|error| CaseFailure::new(FailureStage::DesignerEntry, error))?
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerEntry,
                "ROOT did not publish the checked replacement query in its UIA edit value".into(),
            ));
        }
        save_uia_snapshot("D4", "root-after-type", uia, root.hwnd, output);
        let result_control = uia
            .wait_named_containing(
                root.hwnd,
                child.process_id(),
                "Edit radial skins",
                UIA_TIMEOUT,
            )
            .map_err(|error| CaseFailure::new(FailureStage::DesignerEntry, error))?;
        let semantic_cursor = trace_lines(trace_path).len();
        let click = click_semantic_control(child, &root, &result_control, trace_path)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let action_events = wait_trace(trace_path, semantic_cursor, TRACE_TIMEOUT, |events| {
            has_trace(events, "radial_action", &["stage=Activated", "skins=true"])
                && has_trace(events, "radial_action", &["stage=Parsed", "skins=true"])
                && has_trace(events, "radial_action", &["stage=Dispatched", "skins=true"])
                && has_trace(
                    events,
                    "radial_action",
                    &[
                        "stage=EditorModeApplied",
                        "skins=true",
                        "editor_open=Some(true)",
                        "skins_selected=Some(true)",
                    ],
                )
        });
        let radial_action_dispatched = has_trace(
            &action_events,
            "radial_action",
            &["stage=Dispatched", "skins=true"],
        );
        if !radial_action_dispatched {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "checked command-result click did not produce the production radial Skins dispatch trace".into(),
            ));
        }
        let radial_skins_applied = has_trace(
            &action_events,
            "radial_action",
            &[
                "stage=EditorModeApplied",
                "skins=true",
                "editor_open=Some(true)",
                "skins_selected=Some(true)",
            ],
        );
        if !radial_skins_applied {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "production radial Skins dispatch did not publish the typed Designer mode transition".into(),
            ));
        }
        let same = wait_until(Duration::from_secs(4), || {
            find_child_window(child, WindowRole::Designer)
                .is_some_and(|window| window.hwnd == designer.hwnd)
        });
        if !same {
            return Err(CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "radial skins command replaced or closed the existing Designer HWND".into(),
            ));
        }
        let skins_selected = wait_trace(trace_path, semantic_cursor, TRACE_TIMEOUT, |events| {
            events.iter().any(|line| {
                parse_designer_semantic_target(line, DesignerSemanticTarget::Skins)
                    .is_some_and(|state| state.selected)
            })
        });
        if !skins_selected.iter().any(|line| {
            parse_designer_semantic_target(line, DesignerSemanticTarget::Skins)
                .is_some_and(|state| state.selected)
        }) {
            let (foreground_hwnd, foreground_pid) = capture_foreground();
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                format!(
                    "production radial skins command did not select the Skins semantic target; native click proof=[{}]; exact command bounds={:?}; foreground=HWND:{} PID:{}; post-click Designer/command traces={:?}",
                    click.describe(),
                    result_control.bounds,
                    hwnd_id(foreground_hwnd),
                    foreground_pid,
                    skins_selected
                        .iter()
                        .filter(|line| {
                            line.contains("designer_semantic_target")
                                || line.contains("root_command")
                                || line.contains("designer_focus")
                        })
                        .collect::<Vec<_>>()
                ),
            ));
        }
        if !uia.root_is_queryable(designer.hwnd, child.process_id()) {
            return Err(CaseFailure::new(
                FailureStage::DesignerReadiness,
                "same Designer HWND stopped responding after Skins entry".into(),
            ));
        }
        Ok(format!(
            "replaced the prior ROOT query with {typed} checked Unicode events after {selected_all} checked Ctrl+A events; UIA confirmed the replacement query, the radial skins command label was published at {:?}, and a checked native pointer click=[{}] selected Skins on the same queryable Designer HWND={}",
            result_control.bounds,
            click.describe(),
            hwnd_id(designer.hwnd)
        ))
    })();
    append_case(
        report,
        "D4",
        expected("D4"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_designer_close_case(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    designer: &WindowSnapshot,
    output: &Path,
    trace_path: &Path,
) {
    let started = Instant::now();
    let result = (|| {
        child
            .validate_window(designer.hwnd)
            .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
        let close_cursor = trace_lines(trace_path).len();
        let events = send_alt_f4(child, designer)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let close_state = wait_trace(trace_path, close_cursor, TRACE_TIMEOUT, |events| {
            events
                .iter()
                .any(|line| line.contains("trace_event=\"designer_close\""))
        })
        .into_iter()
        .rev()
        .find(|line| line.contains("trace_event=\"designer_close\""));
        if !close_state
            .as_ref()
            .is_some_and(|line| line.contains("open=false"))
        {
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                format!(
                    "checked Alt+F4 did not produce a terminal clean production close state: {}; {events} checked input events",
                    close_state.as_deref().unwrap_or("no close decision trace")
                ),
            ));
        }
        let closed = wait_until(Duration::from_secs(4), || {
            find_child_window(child, WindowRole::Designer).is_none()
        });
        if !closed {
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                format!(
                    "Designer HWND remained after checked child-focused Alt+F4; production state={}",
                    close_state.as_deref().unwrap_or("missing")
                ),
            ));
        }
        if child
            .try_wait()
            .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?
            .is_some()
        {
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                "closing Designer also terminated the launcher process".into(),
            ));
        }
        if find_child_window(child, WindowRole::Root).is_none() {
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                "ROOT HWND disappeared while closing Designer".into(),
            ));
        }
        Ok(format!(
            "{} checked Alt+F4 events closed Designer HWND={} while child process and ROOT remained alive; production close state={}",
            events,
            hwnd_id(designer.hwnd),
            close_state.as_deref().unwrap_or("missing")
        ))
    })();
    append_case(
        report,
        "D5",
        expected("D5"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn wait_for_geometry_state(
    trace_path: &Path,
    session_id: u64,
    timeout: Duration,
    mut predicate: impl FnMut(&GeometryStateSnapshot) -> bool,
) -> Result<GeometryStateSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(state) = latest_geometry_state(trace_path)?
            && state.session_id == session_id
            && predicate(&state)
        {
            return Ok(state);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer geometry state for session {session_id} did not reach the required state"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_geometry_state_matching(
    trace_path: &Path,
    session_id: u64,
    timeout: Duration,
    mut predicate: impl FnMut(&GeometryStateSnapshot) -> bool,
) -> Result<GeometryStateSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(state) = geometry_states(trace_path)?
            .into_iter()
            .rev()
            .find(|state| state.session_id == session_id && predicate(state))
        {
            return Ok(state);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer geometry trace did not publish a matching state for session {session_id}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_unique_authoring_control_any_index(
    trace_path: &Path,
    session_id: u64,
    target: AuthoringControlTarget,
    role: AuthoringControlRole,
    timeout: Duration,
) -> Result<AuthoringControlSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let matches = list_authoring_controls(trace_path, session_id)?
            .into_iter()
            .filter(|control| control.target == target && control.role == role)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [control] => return Ok(*control),
            [] => {}
            _ => {
                return Err(format!(
                    "ambiguous Designer semantic target {target:?} at multiple indices for session {session_id}"
                ));
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer did not publish target {target:?} with role {role:?} for session {session_id}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_authoring_control(
    trace_path: &Path,
    session_id: u64,
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
    timeout: Duration,
) -> Result<AuthoringControlSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(control) = find_authoring_control(trace_path, session_id, target, index, role)?
        {
            return Ok(control);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer did not publish target {target:?} index={index:?} role={role:?} for session {session_id}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_authoring_control_after(
    trace_path: &Path,
    first_line: usize,
    session_id: u64,
    expected_client_size: [i32; 2],
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
    timeout: Duration,
) -> Result<AuthoringControlSnapshot, String> {
    let deadline = Instant::now() + timeout;
    let mut latest_client_size = None;
    loop {
        if let Some(control) =
            find_authoring_control_after(trace_path, first_line, session_id, target, index, role)?
        {
            latest_client_size = Some(control.client_size);
            if client_size_matches(control.client_size, expected_client_size) {
                return Ok(control);
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer did not render target {target:?} index={index:?} role={role:?} for session {session_id} at compact client size {expected_client_size:?} after trace line {first_line}; last frame size={latest_client_size:?}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn client_size_matches(rendered: [i32; 2], native: [i32; 2]) -> bool {
    rendered[0].abs_diff(native[0]) <= 1 && rendered[1].abs_diff(native[1]) <= 1
}

fn committed_cell_ids_match(
    candidate_digest_available: bool,
    candidate_digest: u64,
    committed_digest: u64,
) -> bool {
    candidate_digest_available && candidate_digest == committed_digest
}

fn click_authoring_target(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
) -> Result<(AuthoringControlSnapshot, PointerClickEvidence), String> {
    let control =
        wait_for_authoring_control(trace_path, session_id, target, index, role, UIA_TIMEOUT)?;
    if !control.enabled {
        return Err(format!(
            "refused native click on disabled Designer target {target:?} index={index:?}"
        ));
    }
    let evidence = click_designer_client_bounds(child, designer, control.bounds, trace_path)?;
    Ok((control, evidence))
}

fn set_requested_slots(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
    value: usize,
) -> Result<String, String> {
    let click_cursor = trace_lines(trace_path).len();
    let (control, click) = click_authoring_target(
        child,
        designer,
        trace_path,
        session_id,
        AuthoringControlTarget::Slots,
        None,
        AuthoringControlRole::DragValue,
    )?;
    let click_cycle = wait_trace(trace_path, click_cursor, TRACE_TIMEOUT, |events| {
        authoring_control_click_finished(
            events,
            session_id,
            AuthoringControlTarget::Slots,
            None,
            AuthoringControlRole::DragValue,
        )
    });
    if !authoring_control_click_finished(
        &click_cycle,
        session_id,
        AuthoringControlTarget::Slots,
        None,
        AuthoringControlRole::DragValue,
    ) {
        return Err(
            "native Slots click was not followed by a fresh Designer frame before text entry"
                .into(),
        );
    }
    let select_all = send_select_all_to_focused_window(child, designer)?;
    let text_edges = send_text_to_focused_window(child, designer, &value.to_string())?;
    let enter_commit = send_enter_current(child, designer)?;
    let state = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
        state.requested_slots == value
    })?;
    Ok(format!(
        "semantic Slots {:?}, native click=[{}], waited for its next rendered frame, Ctrl+A events={select_all}, checked text events={text_edges}, Enter commit={enter_commit}; requested slots={} observed while committed ring slots remained {}",
        control.bounds,
        click.describe(),
        state.requested_slots,
        state.selected_ring_slots
    ))
}

fn append_authoring_case(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    output: &Path,
    trace_path: &Path,
    id: &str,
    operation: impl FnOnce() -> Result<String, CaseFailure>,
) {
    let started = Instant::now();
    append_case(
        report,
        id,
        expected(id),
        started,
        operation(),
        Some(child),
        output,
        trace_path,
    );
}

fn authoring_case_failure(stage: FailureStage, error: impl std::fmt::Display) -> CaseFailure {
    CaseFailure::new(stage, error.to_string())
}

fn run_authoring_geometry_cases(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    uia: &UiAutomation,
    anchor: &FocusAnchor,
    profile: &Path,
    designer: &WindowSnapshot,
    session_id: u64,
    output: &Path,
    trace_path: &Path,
    copied: Option<&CopiedAuthoringOptions>,
) {
    let mut authored_menu_graph = None;
    let mut overflow_root_graph = None;
    append_authoring_case(report, child, output, trace_path, "A0", || {
        let menus = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Menus,
            session_id,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                format!(
                    "the reopened Designer session {session_id} did not publish its Menus mode target"
                ),
            )
        })?;
        let mode_evidence = if menus.selected {
            "reopened Designer already selected Menus".to_string()
        } else {
            let mode_click =
                click_designer_client_bounds(child, designer, menus.bounds, trace_path).map_err(
                    |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
                )?;
            let selected = wait_for_designer_semantic_target_in_session(
                trace_path,
                DesignerSemanticTarget::Menus,
                session_id,
                TRACE_TIMEOUT,
                |state| state.selected,
            )
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerMutation,
                    format!(
                        "checked native Menus mode click did not select Menus in session {session_id}"
                    ),
                )
            })?;
            format!(
                "reopened Designer switched from Skins to Menus by checked click [{}], selected target bounds={:?}",
                mode_click.describe(),
                selected.bounds
            )
        };
        let before = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |_| true)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let (control, click) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::NewMenu,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let after = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            state.menu_count == before.menu_count + 1
                && state.selected_menu_index == Some(before.menu_count)
                && !state.proposal_active
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        Ok(format!(
            "{mode_evidence}; New Menu target role={:?} client_bounds={:?}; checked click=[{}]; menu count {} -> {}, selected menu index {:?}, generation {}",
            control.role,
            control.bounds,
            click.describe(),
            before.menu_count,
            after.menu_count,
            after.selected_menu_index,
            after.generation
        ))
    });

    append_authoring_case(report, child, output, trace_path, "A1", || {
        let before = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            !state.proposal_active && state.selected_menu_index.is_some()
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if before.selected_menu_index != Some(before.menu_count.saturating_sub(1))
            || before.ring_count == 0
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "New Menu selection was not retained before Add Ring: menus={}, selected={:?}, rings={}",
                    before.menu_count, before.selected_menu_index, before.ring_count
                ),
            ));
        }
        let (control, click) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::AddRing,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let preview = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            state.proposal_active && state.proposal_kind == AuthoringProposalKind::NewRing
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if preview.ring_count != before.ring_count
            || preview.selected_ring_slots != before.selected_ring_slots
            || preview.generation != before.generation
            || preview.proposal_candidate_rings != before.ring_count + 1
            || preview.proposal_slots != 8
            || !preview.proposal_cell_ids_preserved
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "Add Ring preview changed the draft or proposed an unexpected candidate: rings={} -> {}, slots={} -> {}, candidate_rings={}, proposal_slots={}, existing_cell_ids_preserved={}",
                    before.ring_count,
                    preview.ring_count,
                    before.selected_ring_slots,
                    preview.selected_ring_slots,
                    preview.proposal_candidate_rings,
                    preview.proposal_slots,
                    preview.proposal_cell_ids_preserved
                ),
            ));
        }
        let ready = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            state.proposal_active && state.proposal_ready
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerPresentation, error))?;
        let apply = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::ApplyProposal,
            None,
            AuthoringControlRole::Button,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if !apply.enabled || !ready.proposal_ready {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "Apply proposal did not become enabled for the exact prepared candidate".into(),
            ));
        }
        Ok(format!(
            "Add Ring role={:?} checked click=[{}]; proposal remains uncommitted at generation {} with committed rings/slots {}/{}, candidate rings={} and slots={} are previewed; candidate preserves existing cell IDs={}; Apply control enabled={} and is reserved for G0",
            control.role,
            click.describe(),
            preview.generation,
            preview.ring_count,
            preview.selected_ring_slots,
            ready.proposal_candidate_rings,
            ready.proposal_slots,
            ready.proposal_cell_ids_preserved,
            apply.enabled
        ))
    });

    append_authoring_case(report, child, output, trace_path, "G0", || {
        let prepared = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            state.proposal_active
                && state.proposal_kind == AuthoringProposalKind::NewRing
                && state.proposal_ready
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if prepared.proposal_candidate_rings != prepared.ring_count + 1
            || prepared.proposal_slots != 8
            || !prepared.proposal_cell_ids_preserved
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "outer-ring proposal was not the exact safe candidate to apply: committed rings={}, candidate rings={}, slots={}, existing cell IDs preserved={}",
                    prepared.ring_count,
                    prepared.proposal_candidate_rings,
                    prepared.proposal_slots,
                    prepared.proposal_cell_ids_preserved
                ),
            ));
        }
        let apply = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::ApplyProposal,
            None,
            AuthoringControlRole::Button,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if !apply.enabled {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "Apply proposal was disabled for the ready outer-ring candidate".into(),
            ));
        }
        let apply_click = click_designer_client_bounds(child, designer, apply.bounds, trace_path)
            .map_err(|error| {
            authoring_case_failure(FailureStage::DesignerNativeTarget, error)
        })?;
        let applied = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            !state.proposal_active
                && state.ring_count == prepared.proposal_candidate_rings
                && state.selected_menu_index == prepared.selected_menu_index
                && state.selected_ring_index == Some(prepared.ring_count)
                && state.selected_ring_slots == prepared.proposal_slots
                && state.generation > prepared.generation
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let post_apply_ids_preserved = committed_cell_ids_match(
            prepared.proposal_cell_ids_digest_available,
            prepared.proposal_cell_ids_digest,
            applied.draft_cell_ids_digest,
        );
        if !post_apply_ids_preserved {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "committed outer-ring draft did not match the prepared candidate cell IDs after Apply".into(),
            ));
        }
        if applied.menu_populated != prepared.menu_populated {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "applying an empty outer ring changed existing menu contents: populated cells {} -> {}",
                    prepared.menu_populated, applied.menu_populated
                ),
            ));
        }
        let menu_index = applied.selected_menu_index.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "applied outer-ring draft lost its selected menu identity".into(),
            )
        })?;
        if menu_index != prepared.selected_menu_index.unwrap_or(usize::MAX)
            || applied.ring_count != 2
            || applied.selected_ring_index != Some(1)
            || applied.selected_ring_slots != 8
            || applied.menu_populated != 0
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "A0/G0 geometry was not a selected two-ring blank menu after Apply: index={menu_index}, rings={}, selected ring={:?}, selected slots={}, populated={}",
                    applied.ring_count,
                    applied.selected_ring_index,
                    applied.selected_ring_slots,
                    applied.menu_populated
                ),
            ));
        }
        Ok(format!(
            "ready outer-ring candidate with {} committed and {} proposed rings, {} proposed slots, and existing cell IDs preserved=true; Apply enabled={}; checked click=[{}]; committed state has {} rings, selects new ring {:?}, {} slots, unchanged populated cells={}, post-Apply stable cell IDs match candidate={}, generation {} -> {}",
            prepared.ring_count,
            prepared.proposal_candidate_rings,
            prepared.proposal_slots,
            apply.enabled,
            apply_click.describe(),
            applied.ring_count,
            applied.selected_ring_index,
            applied.selected_ring_slots,
            applied.menu_populated,
            post_apply_ids_preserved,
            prepared.generation,
            applied.generation
        ))
    });

    append_authoring_case(report, child, output, trace_path, "A2", || {
        let start = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            !state.proposal_active && state.ring_count >= 2 && state.selected_ring_index == Some(1)
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if start.selected_ring_slots != 8 {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "new ring has unexpected slot count {}",
                    start.selected_ring_slots
                ),
            ));
        }
        let (_, selector_click) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::RingSelector,
            None,
            AuthoringControlRole::ComboBox,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let ring_zero = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::RingOption,
            Some(0),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if ring_zero.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Ring selector unexpectedly reported its first option as selected".into(),
            ));
        }
        let ring_zero_click =
            click_designer_client_bounds(child, designer, ring_zero.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        let selected_zero =
            wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
                !state.proposal_active && state.selected_ring_index == Some(0)
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if selected_zero.selected_ring_slots == 0 {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "selecting Ring 1 unexpectedly produced an empty ring".into(),
            ));
        }
        let (_, selector_reopen) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::RingSelector,
            None,
            AuthoringControlRole::ComboBox,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let ring_one = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::RingOption,
            Some(1),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if ring_one.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Ring selector unexpectedly reported its second option selected after choosing Ring 1".into(),
            ));
        }
        let ring_one_click =
            click_designer_client_bounds(child, designer, ring_one.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        let selected_one =
            wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
                !state.proposal_active && state.selected_ring_index == Some(1)
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if selected_one.selected_ring_slots != 8 {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "selecting Ring 2 changed its committed slot count to {}",
                    selected_one.selected_ring_slots
                ),
            ));
        }
        let slots_evidence = set_requested_slots(child, designer, trace_path, session_id, 10)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerFrameworkInput, error))?;
        let (_, preview_click) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::PreviewProposal,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let prepared = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            state.proposal_active
                && state.proposal_kind == AuthoringProposalKind::Resize
                && state.proposal_ready
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerPresentation, error))?;
        if prepared.selected_ring_slots != 8
            || prepared.requested_slots != 10
            || prepared.proposal_slots != 10
            || prepared.proposal_candidate_rings != 2
            || !prepared.proposal_cell_ids_preserved
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "slot growth preview did not preserve draft geometry and original cell IDs: slots={} requested={} candidate={} rings={} IDs_preserved={}",
                    prepared.selected_ring_slots,
                    prepared.requested_slots,
                    prepared.proposal_slots,
                    prepared.proposal_candidate_rings,
                    prepared.proposal_cell_ids_preserved
                ),
            ));
        }
        let apply = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::ApplyProposal,
            None,
            AuthoringControlRole::Button,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if !apply.enabled {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "Apply proposal remained disabled after the exact slot-growth candidate was prepared".into(),
            ));
        }
        let apply_click = click_designer_client_bounds(child, designer, apply.bounds, trace_path)
            .map_err(|error| {
            authoring_case_failure(FailureStage::DesignerNativeTarget, error)
        })?;
        let applied = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            !state.proposal_active
                && state.selected_ring_index == Some(1)
                && state.selected_ring_slots == 10
                && state.generation > prepared.generation
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let post_apply_ids_preserved = committed_cell_ids_match(
            prepared.proposal_cell_ids_digest_available,
            prepared.proposal_cell_ids_digest,
            applied.draft_cell_ids_digest,
        );
        if !post_apply_ids_preserved {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "committed grown-ring draft did not match the prepared candidate cell IDs after Apply".into(),
            ));
        }
        let menu_index = applied.selected_menu_index.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "grown-ring A0 menu no longer had a selected menu index".into(),
            )
        })?;
        if menu_index != start.selected_menu_index.unwrap_or(usize::MAX)
            || applied.ring_count != 2
            || applied.selected_ring_index != Some(1)
            || applied.selected_ring_slots != 10
            || applied.menu_populated != 0
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "A2 did not retain the A0/G0 two-ring blank menu with [8,10] slots: index={menu_index}, rings={}, selected ring={:?}, selected slots={}, populated={}",
                    applied.ring_count,
                    applied.selected_ring_index,
                    applied.selected_ring_slots,
                    applied.menu_populated
                ),
            ));
        }
        authored_menu_graph = Some(PersistedMenuGraphExpectation {
            menu_index,
            ring_slots: vec![8, 10],
            populated_cells: 0,
            cell_ids_digest: applied.draft_cell_ids_digest,
        });
        let _ = (
            selector_click,
            ring_zero_click,
            selector_reopen,
            ring_one_click,
            slots_evidence,
            preview_click,
            apply_click,
        );
        Ok(format!(
            "evidence:v1; geometry=[8,10]; candidate_ids_preserved={}; committed={}; stable_ids={}; generation={}",
            prepared.proposal_cell_ids_preserved,
            !applied.proposal_active,
            post_apply_ids_preserved,
            applied.generation
        ))
    });

    if copied.is_none() {
        append_authoring_case(report, child, output, trace_path, "G1", || {
            let evidence = run_populated_shrink_resolution(child, designer, trace_path, session_id)
                .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
            let root = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
                !state.proposal_active
                    && state.selected_menu_index == Some(0)
                    && state.selected_ring_index == Some(0)
                    && state.ring_count == 2
                    && state.selected_ring_slots == 8
                    && state.menu_populated == 9
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
            let (_, selector_click) = click_authoring_target(
                child,
                designer,
                trace_path,
                session_id,
                AuthoringControlTarget::RingSelector,
                None,
                AuthoringControlRole::ComboBox,
            )
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
            let ring_one = wait_for_authoring_control(
                trace_path,
                session_id,
                AuthoringControlTarget::RingOption,
                Some(1),
                AuthoringControlRole::Selectable,
                UIA_TIMEOUT,
            )
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
            if ring_one.selected {
                return Err(CaseFailure::new(
                    FailureStage::DesignerMutation,
                    "G1 selected the new overflow ring before its exact slot count was checked"
                        .into(),
                ));
            }
            let ring_one_click =
                click_designer_client_bounds(child, designer, ring_one.bounds, trace_path)
                    .map_err(|error| {
                        authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                    })?;
            let overflow =
                wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
                    !state.proposal_active
                        && state.selected_menu_index == Some(0)
                        && state.selected_ring_index == Some(1)
                        && state.ring_count == 2
                        && state.selected_ring_slots == 1
                        && state.menu_populated == 9
                })
                .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
            if overflow.draft_cell_ids_digest != root.draft_cell_ids_digest {
                return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "selecting the G1 overflow ring changed the complete root menu/ring/cell ID graph"
                    .into(),
            ));
            }
            let (_, selector_reopen) = click_authoring_target(
                child,
                designer,
                trace_path,
                session_id,
                AuthoringControlTarget::RingSelector,
                None,
                AuthoringControlRole::ComboBox,
            )
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
            let ring_zero = wait_for_authoring_control(
                trace_path,
                session_id,
                AuthoringControlTarget::RingOption,
                Some(0),
                AuthoringControlRole::Selectable,
                UIA_TIMEOUT,
            )
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
            let ring_zero_click =
                click_designer_client_bounds(child, designer, ring_zero.bounds, trace_path)
                    .map_err(|error| {
                        authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                    })?;
            let restored =
                wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
                    !state.proposal_active
                        && state.selected_menu_index == Some(0)
                        && state.selected_ring_index == Some(0)
                        && state.ring_count == 2
                        && state.selected_ring_slots == 8
                        && state.menu_populated == 9
                })
                .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
            if restored.draft_cell_ids_digest != overflow.draft_cell_ids_digest {
                return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "returning to the G1 root ring changed the complete root menu/ring/cell ID graph"
                    .into(),
            ));
            }
            overflow_root_graph = Some(PersistedMenuGraphExpectation {
                menu_index: 0,
                ring_slots: vec![8, 1],
                populated_cells: 9,
                cell_ids_digest: restored.draft_cell_ids_digest,
            });
            let _ = (
                evidence,
                selector_click,
                ring_one_click,
                ring_zero_click,
                selector_reopen,
                overflow,
            );
            Ok(format!(
                "evidence:v1; overflow_root=[8,1]/9; stable_ids={}; root_ring_slots={}; populated_cells={}",
                restored.draft_cell_ids_digest == root.draft_cell_ids_digest,
                root.selected_ring_slots,
                root.menu_populated
            ))
        });

        append_authoring_case(report, child, output, trace_path, "G2", || {
            let compact = run_compact_geometry_case(child, designer, trace_path, session_id)
                .map_err(|error| {
                    authoring_case_failure(FailureStage::DesignerPresentation, error)
                })?;
            let working_viewport = restore_authoring_viewport(
                child, designer, trace_path, session_id,
            )
            .map_err(|error| authoring_case_failure(FailureStage::DesignerPresentation, error))?;
            Ok(format!(
                "{compact}; {working_viewport}; retaining the authored A0/G0/A2 menu and G1 overflow-root draft for the A3-A6 Save/reopen workflow"
            ))
        });
    }

    let post_g2_observation = copied
        .is_none()
        .then(|| record_post_g2_root_snapshot(report, child, output, trace_path));
    let side_effect_baseline = match capture_action_side_effect_baseline(profile, trace_path) {
        Ok(baseline) => baseline,
        Err(error) => {
            let failure = CaseFailure::new(FailureStage::DesignerReadiness, error);
            append_authoring_case(report, child, output, trace_path, "A3", || {
                Err(failure.clone())
            });
            append_blocked_ids(
                report,
                &["A4", "A5", "A6", "A7", "A8", "D3", "D6", "D7"],
                &failure,
                output,
                trace_path,
                "strict pre-A3 leaf-side-effect baseline could not be captured",
            );
            return;
        }
    };
    let entry_evidence = post_g2_observation.map_or_else(
        || format!("copied-profile derived menu geometry remains open in Designer session {session_id}"),
        |observation| format!("post-G2 ROOT snapshot={observation}; same Designer session {session_id} and authored geometry remain open for A3-A6"),
    );
    let Some((entry, saved_glow)) = run_radial_action_authoring_cases(
        report,
        child,
        uia,
        anchor,
        designer,
        output,
        trace_path,
        profile,
        session_id,
        copied.map_or(ACCEPTANCE_TARGET_ACTION_INDEX, |options| {
            options.target_action_index
        }),
        &entry_evidence,
        authored_menu_graph,
        overflow_root_graph,
        &side_effect_baseline,
        copied,
    ) else {
        append_blocked_lifecycle_cases(report, output, trace_path);
        return;
    };
    run_designer_lifecycle_cases(
        report,
        child,
        uia,
        anchor,
        &entry,
        output,
        trace_path,
        profile,
        &side_effect_baseline,
        saved_glow,
        copied.map_or(0, |options| options.skin_index),
    );
}

fn run_radial_action_authoring_cases(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    uia: &UiAutomation,
    anchor: &FocusAnchor,
    designer: &WindowSnapshot,
    output: &Path,
    trace_path: &Path,
    profile: &Path,
    session_id: u64,
    target_action_index: usize,
    entry_evidence: &str,
    authored_menu_graph: Option<PersistedMenuGraphExpectation>,
    overflow_root_graph: Option<PersistedMenuGraphExpectation>,
    side_effect_baseline: &ActionSideEffectBaseline,
    copied: Option<&CopiedAuthoringOptions>,
) -> Option<(DesignerEntry, bool)> {
    let mut selected_canvas_cell_index = None;
    let mut selected_cell_slot_index = None;
    let mut authored_cell_identity = None;
    let mut action_mutation_generation = None;
    let mut style_mutation_generation = None;
    let mut style_glow_after_edit = None;
    append_authoring_case(report, child, output, trace_path, "A3", || {
        let expected_graph = authored_menu_graph.as_ref().ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "A0/G0/A2 did not retain a typed two-ring menu identity graph for action authoring"
                    .into(),
            )
        })?;
        let before_menu = latest_geometry_state(trace_path)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?
            .filter(|state| state.session_id == session_id && !state.proposal_active)
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerReadiness,
                    "Designer did not publish pre-authoring geometry before selecting the blank A0 menu"
                        .into(),
                )
            })?;
        if before_menu.menu_count <= expected_graph.menu_index {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "captured A0 menu index {} is outside the live menu list of {}",
                    expected_graph.menu_index, before_menu.menu_count
                ),
            ));
        }
        let menu_row = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::MenuRow,
            Some(expected_graph.menu_index),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let menu_click = if menu_row.selected {
            None
        } else {
            Some(
                click_designer_client_bounds(child, designer, menu_row.bounds, trace_path)
                    .map_err(|error| {
                        authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                    })?,
            )
        };
        let selected_menu_state =
            wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
                state.menu_count > expected_graph.menu_index
                    && state.selected_menu_index == Some(expected_graph.menu_index)
                    && state.ring_count == expected_graph.ring_slots.len()
                    && state.menu_populated == expected_graph.populated_cells
                    && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if expected_graph.ring_slots != [8, 10] || expected_graph.populated_cells != 0 {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "captured A0/G0/A2 graph did not describe the approved blank [8,10] menu: rings={:?}, populated={}",
                    expected_graph.ring_slots, expected_graph.populated_cells
                ),
            ));
        }
        let (_, selector_click) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::RingSelector,
            None,
            AuthoringControlRole::ComboBox,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let ring_one = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::RingOption,
            Some(1),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if ring_one.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "A3 expected the retained A2 outer ring to be available for selection".into(),
            ));
        }
        let ring_click = click_designer_client_bounds(child, designer, ring_one.bounds, trace_path)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let blank_menu = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            state.selected_menu_index == Some(expected_graph.menu_index)
                && state.generation >= selected_menu_state.generation
                && state.selected_ring_index == Some(1)
                && state.selected_ring_slots == 10
                && state.ring_count == 2
                && state.menu_populated == 0
                && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let cell_slot_index = 0;
        let cell_index = flat_canvas_cell_index(&expected_graph.ring_slots, 1, cell_slot_index)
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerMutation,
                    "authored outer ring did not contain blank slot 0".into(),
                )
            })?;
        let cell = wait_for_canvas_cell_in_generation(
            trace_path,
            session_id,
            blank_menu.generation,
            cell_index,
            expected_graph.cell_ids_digest,
            1,
            cell_slot_index,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let index = cell.index.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "authored canvas cell omitted its numeric slot identity".into(),
            )
        })?;
        if index != cell_index {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "authored outer-ring slot 0 mapped to CanvasCell index {index}, expected flat index {cell_index}"
                ),
            ));
        }
        selected_canvas_cell_index = Some(index);
        selected_cell_slot_index = Some(cell_slot_index);
        let click_cursor = trace_lines(trace_path).len();
        let click = click_designer_client_bounds(child, designer, cell.bounds, trace_path)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        wait_for_authoring_control_click_finished(
            trace_path,
            click_cursor,
            session_id,
            AuthoringControlTarget::CanvasCell,
            Some(index),
            AuthoringControlRole::Region,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        wait_for_authoring_control_selected(
            trace_path,
            session_id,
            AuthoringControlTarget::CanvasCell,
            Some(index),
            AuthoringControlRole::Region,
            true,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;

        click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::CellType,
            None,
            AuthoringControlRole::ComboBox,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::ActionTypeOption,
            None,
            AuthoringControlRole::Selectable,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;

        let target_rank = wait_for_action_catalog_rank(
            trace_path,
            session_id,
            target_action_index,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if target_rank.rank < 50 {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "fixture action source index {} was only rank {} of {} before search",
                    target_rank.custom_action_index, target_rank.rank, target_rank.catalog_len
                ),
            ));
        }
        let search = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::ActionSearch,
            None,
            AuthoringControlRole::TextEdit,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        click_designer_client_bounds(child, designer, search.bounds, trace_path)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let query = format!("Radial Acceptance Harmless Action {target_action_index:03}");
        send_text_to_focused_window(child, designer, &query)
            .map_err(|error| authoring_case_failure(FailureStage::InputInjection, error))?;
        let action_row = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::ActionRow,
            Some(target_action_index),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if !action_row.enabled {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "searched custom action row was not assignable".into(),
            ));
        }
        let action_click =
            click_designer_client_bounds(child, designer, action_row.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        let assigned_row = wait_for_authoring_control_selected(
            trace_path,
            session_id,
            AuthoringControlTarget::ActionRow,
            Some(target_action_index),
            AuthoringControlRole::Selectable,
            true,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if assigned_row.index != Some(target_action_index) || !assigned_row.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "native custom action click did not retain the exact selected source row".into(),
            ));
        }
        let _ = (
            entry_evidence,
            menu_click,
            menu_row,
            selector_click,
            ring_click,
            click,
            action_click,
        );
        Ok(format!(
            "evidence:v1; blank_cell_selected=true; geometry=[8,10]; cell_slot={cell_slot_index}; catalog_rank_gt_50={}; catalog_rank={}/{}; searched_action_assigned=true; source_index={}; menu_graph={}",
            target_rank.rank >= 50,
            target_rank.rank,
            target_rank.catalog_len,
            target_rank.custom_action_index,
            expected_graph.cell_ids_digest
        ))
    });

    append_authoring_case(report, child, output, trace_path, "A4", || {
        let cell_index = selected_canvas_cell_index.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "A3 did not identify an authored cell for the Inspector handoff".into(),
            )
        })?;
        let cell_slot_index = selected_cell_slot_index.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "A3 did not identify the authored slot within its ring for the Inspector handoff"
                    .into(),
            )
        })?;
        let row = wait_for_authoring_control_selected(
            trace_path,
            session_id,
            AuthoringControlTarget::ActionRow,
            Some(target_action_index),
            AuthoringControlRole::Selectable,
            true,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let before = latest_geometry_state(trace_path)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?
            .filter(|state| state.session_id == session_id)
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerReadiness,
                    "Designer did not publish pre-handoff geometry state".into(),
                )
            })?;
        let cell_identity = before.selected_cell_id_digest.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "selected authored cell did not expose its hashed stable identity before handoff"
                    .into(),
            )
        })?;
        click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::PopupOpenInspector,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::PopupApplyAndOpen,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let applied =
            wait_for_geometry_state_matching(trace_path, session_id, TRACE_TIMEOUT, |state| {
                state.generation > before.generation
                    && !state.proposal_active
                    && state.selected_cell_index == Some(cell_slot_index)
                    && state.selected_cell_id_digest == Some(cell_identity)
                    && state.selected_cell_custom_action_index_known
                    && state.selected_cell_custom_action_index == Some(target_action_index)
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let inspector = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::InspectorCell,
            Some(cell_index),
            AuthoringControlRole::Region,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerPresentation, error))?;
        if !row.selected || !inspector.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "Apply and open did not retain the authored binding/cell handoff: pre-apply action row selected={}, post-apply model cell={:?} identity={:?} action source={:?} (known={}), Inspector selected={}, expected cell/action={cell_index}/{target_action_index}",
                    row.selected,
                    applied.selected_cell_index,
                    applied.selected_cell_id_digest,
                    applied.selected_cell_custom_action_index,
                    applied.selected_cell_custom_action_index_known,
                    inspector.selected
                ),
            ));
        }
        authored_cell_identity = Some(cell_identity);
        action_mutation_generation = Some(applied.generation);
        Ok(format!(
            "dirty popup Apply and open advanced generation {} -> {}; production model retained outer-ring slot {} (CanvasCell global index {}) identity digest {} and resolves it to custom action source index {}; Inspector reported that same slot selected",
            before.generation,
            applied.generation,
            cell_slot_index,
            cell_index,
            cell_identity,
            target_action_index
        ))
    });

    append_authoring_case(report, child, output, trace_path, "A5", || {
        let skin_index = copied.map_or(0, |options| options.skin_index);
        let skins = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Skins,
            session_id,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "Skins mode did not publish a current semantic target".into(),
            )
        })?;
        let mode_click = if skins.selected {
            None
        } else {
            Some(
                click_designer_client_bounds(child, designer, skins.bounds, trace_path).map_err(
                    |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
                )?,
            )
        };
        let selected_skins = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Skins,
            session_id,
            TRACE_TIMEOUT,
            |state| state.selected,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "checked mode click did not select Skins".into(),
            )
        })?;
        let skin = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::SkinRow,
            Some(skin_index),
            AuthoringControlRole::Button,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let skin_click = if skin.selected {
            None
        } else {
            Some(
                click_designer_client_bounds(child, designer, skin.bounds, trace_path).map_err(
                    |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
                )?,
            )
        };
        wait_for_authoring_control_selected(
            trace_path,
            session_id,
            AuthoringControlTarget::SkinRow,
            Some(skin_index),
            AuthoringControlRole::Button,
            true,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let glow = wait_for_unique_authoring_control_any_index(
            trace_path,
            session_id,
            AuthoringControlTarget::SkinGlowEnabled,
            AuthoringControlRole::Checkbox,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let glow_before = glow.selected;
        let glow_after = !glow_before;
        let before = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |_| true)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let preview_cursor = trace_lines(trace_path).len();
        let glow_click = click_designer_client_bounds(child, designer, glow.bounds, trace_path)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let after = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            state.generation > before.generation
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let _disabled = wait_for_authoring_control_selected(
            trace_path,
            session_id,
            AuthoringControlTarget::SkinGlowEnabled,
            glow.index,
            AuthoringControlRole::Checkbox,
            glow_after,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let preview_request = wait_for_authoring_request_generation(
            trace_path,
            preview_cursor,
            session_id,
            "PrepareEmbeddedPreview",
            after.generation,
            TRACE_TIMEOUT,
        )
        .ok_or_else(|| {
            authoring_case_failure(
                FailureStage::DesignerPresentation,
                "glow draft generation did not request a fresh embedded preview",
            )
        })?;
        let preview_reply = wait_trace(trace_path, preview_cursor, TRACE_TIMEOUT, |events| {
            events.iter().any(|line| {
                authoring_reply_matches(line, preview_request, "PrepareEmbeddedPreview")
            })
        })
        .into_iter()
        .find(|line| authoring_reply_matches(line, preview_request, "PrepareEmbeddedPreview"))
        .ok_or_else(|| {
            authoring_case_failure(
                FailureStage::DesignerPresentation,
                "glow-generation preview request did not receive its correlated accepted reply",
            )
        })?;
        // Skins mode replaces the canvas with the resource editor. Return to
        // Menus through the production mode selector before requiring visual
        // evidence that the edited preview generation rendered.
        let menus = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Menus,
            session_id,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            authoring_case_failure(
                FailureStage::DesignerReadiness,
                "Menus mode target was not available after the glow edit",
            )
        })?;
        let menu_mode_click = if menus.selected {
            None
        } else {
            Some(
                click_designer_client_bounds(child, designer, menus.bounds, trace_path).map_err(
                    |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
                )?,
            )
        };
        let _returned_to_menus = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Menus,
            session_id,
            TRACE_TIMEOUT,
            |state| state.selected,
        )
        .ok_or_else(|| {
            authoring_case_failure(
                FailureStage::DesignerMutation,
                "checked Menus mode click did not restore the preview canvas after the glow edit",
            )
        })?;
        let rendered_preview = wait_trace(trace_path, preview_cursor, TRACE_TIMEOUT, |events| {
            events.iter().any(|line| {
                line.contains("trace_event=\"designer_preview_rendered\"")
                    && trace_field_value(line, "session_id")
                        .and_then(|value| value.parse::<u64>().ok())
                        == Some(session_id)
                    && trace_field_value(line, "generation")
                        .and_then(|value| value.parse::<u64>().ok())
                        == Some(after.generation)
            })
        })
        .into_iter()
        .find(|line| {
            line.contains("trace_event=\"designer_preview_rendered\"")
                && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
                    == Some(session_id)
                && trace_field_value(line, "generation").and_then(|value| value.parse::<u64>().ok())
                    == Some(after.generation)
        })
        .ok_or_else(|| {
            authoring_case_failure(
                FailureStage::DesignerPresentation,
                "Designer did not render a preview frame from the glow draft generation",
            )
        })?;
        style_mutation_generation = Some(after.generation);
        style_glow_after_edit = Some(glow_after);
        let _ = (
            selected_skins,
            mode_click,
            skin_click,
            glow_click,
            preview_reply,
            menus,
            menu_mode_click,
            rendered_preview,
        );
        Ok(format!(
            "evidence:v1; glow={}->{}; generation={}->{}; preview_reply=accepted; preview_request={}; preview_generation={}; preview_rendered=true; render_session={session_id}",
            glow_before,
            glow_after,
            before.generation,
            after.generation,
            preview_request.request_id,
            preview_request.generation
        ))
    });

    let mut reopened_entry = None;
    let a6_started = Instant::now();
    let a6_result = (|| {
        let cell_identity = authored_cell_identity.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "Save is blocked until A4 proves the same authored cell retained its action binding".into(),
            )
        })?;
        let action_generation = action_mutation_generation.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "Save is blocked until A4 proves the action-binding draft mutation".into(),
            )
        })?;
        let style_generation = style_mutation_generation.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "Save is blocked until A5 proves the glow style draft mutation".into(),
            )
        })?;
        if style_generation <= action_generation {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "style mutation generation {style_generation} did not follow action-binding generation {action_generation}"
                ),
            ));
        }
        let menus = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Menus,
            session_id,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "Menus mode did not publish before Save".into(),
            )
        })?;
        if !menus.selected {
            click_designer_client_bounds(child, designer, menus.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        }
        wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Menus,
            session_id,
            TRACE_TIMEOUT,
            |state| state.selected,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "checked mode click did not select Menus before Save".into(),
            )
        })?;
        let expected_graph = authored_menu_graph.as_ref().ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "A6 has no captured A0/G0/A2 menu graph to save".into(),
            )
        })?;
        let saved_glow = style_glow_after_edit.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "A6 has no captured skin style value to persist".into(),
            )
        })?;
        let before = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            !state.proposal_active && state.menu_count > expected_graph.menu_index
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if before.menu_count <= expected_graph.menu_index {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "captured A0 menu index was not present in the live save draft".into(),
            ));
        }
        let menu_index = expected_graph.menu_index;
        let menu_row = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::MenuRow,
            Some(menu_index),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let menu_click = if menu_row.selected {
            None
        } else {
            Some(
                click_designer_client_bounds(child, designer, menu_row.bounds, trace_path)
                    .map_err(|error| {
                        authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                    })?,
            )
        };
        let menu_geometry =
            wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
                !state.proposal_active
                    && state.selected_menu_index == Some(menu_index)
                    && state.ring_count == expected_graph.ring_slots.len()
                    && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if expected_graph.ring_slots != [8, 10] || expected_graph.populated_cells != 0 {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "A6 captured authored graph is not the A0/G0/A2 [8,10] menu: rings={:?}, initial populated={}",
                    expected_graph.ring_slots, expected_graph.populated_cells
                ),
            ));
        }
        if menu_geometry.selected_menu_after_action
            != Some(multi_launcher::radial::model::AfterActionPolicy::Inherit)
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "new authored menu had unexpected initial after-action policy {:?}",
                    menu_geometry.selected_menu_after_action
                ),
            ));
        }
        let (_, policy_combo_click) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::MenuAfterAction,
            None,
            AuthoringControlRole::ComboBox,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let close_tree_option = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::AfterActionOption,
            Some(3),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if close_tree_option.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "new authored menu unexpectedly already selected Close tree".into(),
            ));
        }
        let policy_click =
            click_designer_client_bounds(child, designer, close_tree_option.bounds, trace_path)
                .map_err(|error| {
                    authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                })?;
        let policy_applied =
            wait_for_geometry_state_matching(trace_path, session_id, TRACE_TIMEOUT, |state| {
                state.generation > menu_geometry.generation
                    && state.selected_menu_index == Some(menu_index)
                    && state.selected_menu_after_action
                        == Some(multi_launcher::radial::model::AfterActionPolicy::CloseTree)
            })
            .map_err(|error| {
                authoring_case_failure(
                    FailureStage::DesignerMutation,
                    format!("new menu did not retain the checked Close tree policy edit: {error}"),
                )
            })?;
        let ring_one_click = if policy_applied.selected_ring_index == Some(1) {
            None
        } else {
            click_authoring_target(
                child,
                designer,
                trace_path,
                session_id,
                AuthoringControlTarget::RingSelector,
                None,
                AuthoringControlRole::ComboBox,
            )
            .and_then(|_| {
                wait_for_authoring_control(
                    trace_path,
                    session_id,
                    AuthoringControlTarget::RingOption,
                    Some(1),
                    AuthoringControlRole::Selectable,
                    UIA_TIMEOUT,
                )
            })
            .and_then(|ring| click_designer_client_bounds(child, designer, ring.bounds, trace_path))
            .map(Some)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?
        };
        let selected_authored_ring =
            wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
                !state.proposal_active
                    && state.selected_menu_index == Some(menu_index)
                    && state.selected_ring_index == Some(1)
                    && state.selected_ring_slots == 10
                    && state.menu_populated == 1
                    && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
                    && state.selected_menu_after_action
                        == Some(multi_launcher::radial::model::AfterActionPolicy::CloseTree)
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let cell_index = selected_canvas_cell_index.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "A3 did not identify the action cell for durable Save verification".into(),
            )
        })?;
        let cell_slot_index = selected_cell_slot_index.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "A3 did not identify the persisted slot within its authored ring".into(),
            )
        })?;
        if selected_authored_ring.selected_ring_index != Some(1)
            || selected_authored_ring.selected_ring_slots <= cell_slot_index
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "authored menu geometry no longer contains the selected cell".into(),
            ));
        }
        let authored_cell = wait_for_canvas_cell_in_generation(
            trace_path,
            session_id,
            policy_applied.generation,
            cell_index,
            expected_graph.cell_ids_digest,
            1,
            cell_slot_index,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if authored_cell.index != Some(cell_index) {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "selected authored menu did not publish expected canvas cell {cell_index}; observed {:?}",
                    authored_cell.index
                ),
            ));
        }
        let cell_click_cursor = trace_lines(trace_path).len();
        let cell_click =
            click_designer_client_bounds(child, designer, authored_cell.bounds, trace_path)
                .map_err(|error| {
                    authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                })?;
        wait_for_authoring_control_click_finished(
            trace_path,
            cell_click_cursor,
            session_id,
            AuthoringControlTarget::CanvasCell,
            Some(cell_index),
            AuthoringControlRole::Region,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let saved_draft = wait_for_geometry_state_matching(
            trace_path,
            session_id,
            TRACE_TIMEOUT,
            |state| {
                state.generation >= policy_applied.generation
                    && state.generation >= style_generation
                    && state.generation > action_generation
                    && state.selected_cell_index == Some(cell_slot_index)
                    && state.selected_cell_id_digest == Some(cell_identity)
                    && state.selected_ring_index == Some(1)
                    && state.selected_ring_slots == 10
                    && state.menu_populated == 1
                    && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
                    && state.selected_menu_after_action
                        == Some(multi_launcher::radial::model::AfterActionPolicy::CloseTree)
                    && state.selected_cell_custom_action_index_known
                    && state.selected_cell_custom_action_index == Some(target_action_index)
            },
        )
        .map_err(|error| {
            authoring_case_failure(
                FailureStage::DesignerMutation,
                format!(
                    "Save is blocked until the same selected cell proves the action and style draft mutations: {error}"
                ),
            )
        })?;
        let save = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::Save,
            None,
            AuthoringControlRole::Button,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if !save.enabled {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Save was disabled for the valid authoring draft".into(),
            ));
        }
        let save_cursor = trace_lines(trace_path).len();
        let save_click = click_designer_client_bounds(child, designer, save.bounds, trace_path)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let saved = wait_trace(trace_path, save_cursor, UIA_TIMEOUT, |events| {
            events.iter().any(|line| {
                line.contains("trace_event=\"authoring\"")
                    && line.contains("edge=ReplyAccepted")
                    && line.contains("request_kind=CommitSave")
                    && line.contains("terminal=true")
            })
        });
        let Some(save_reply) = saved.iter().rev().find(|line| {
            line.contains("trace_event=\"authoring\"")
                && line.contains("edge=ReplyAccepted")
                && line.contains("request_kind=CommitSave")
                && line.contains("terminal=true")
        }) else {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Save did not receive its terminal accepted CommitSave reply".into(),
            ));
        };
        if !wait_until(Duration::from_secs(4), || child.designer().is_none()) {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                format!("accepted Save did not close Designer: {save_reply}"),
            ));
        }
        if child.refresh_root().is_err()
            || child
                .try_wait()
                .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?
                .is_some()
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "Save also removed ROOT or terminated the candidate process".into(),
            ));
        }
        let persisted = verify_saved_authoring_fixture(
            profile,
            expected_graph,
            overflow_root_graph.as_ref(),
            cell_slot_index,
            target_action_index,
            copied.map_or(0, |options| options.skin_index),
            saved_glow,
            copied.map(|options| options.original_menu_sha256.as_slice()),
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let reopened = run_designer_entry(child, uia, anchor, trace_path)?;
        let reopened_menus = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Menus,
            reopened.session_id,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "reopened Designer did not publish Menus mode".into(),
            )
        })?;
        if !reopened_menus.selected {
            click_designer_client_bounds(
                child,
                &reopened.window,
                reopened_menus.bounds,
                trace_path,
            )
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        }
        let row = wait_for_authoring_control_selected(
            trace_path,
            reopened.session_id,
            AuthoringControlTarget::MenuRow,
            Some(menu_index),
            AuthoringControlRole::Selectable,
            true,
            UIA_TIMEOUT,
        );
        let row = match row {
            Ok(row) => row,
            Err(_) => {
                let row = wait_for_authoring_control(
                    trace_path,
                    reopened.session_id,
                    AuthoringControlTarget::MenuRow,
                    Some(menu_index),
                    AuthoringControlRole::Selectable,
                    UIA_TIMEOUT,
                )
                .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
                click_designer_client_bounds(child, &reopened.window, row.bounds, trace_path)
                    .map_err(|error| {
                        authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                    })?;
                wait_for_authoring_control_selected(
                    trace_path,
                    reopened.session_id,
                    AuthoringControlTarget::MenuRow,
                    Some(menu_index),
                    AuthoringControlRole::Selectable,
                    true,
                    TRACE_TIMEOUT,
                )
                .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?
            }
        };
        let reloaded =
            wait_for_geometry_state(trace_path, reopened.session_id, TRACE_TIMEOUT, |state| {
                state.selected_menu_index == Some(menu_index)
                    && state.ring_count == expected_graph.ring_slots.len()
                    && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
                    && state.selected_menu_after_action
                        == Some(multi_launcher::radial::model::AfterActionPolicy::CloseTree)
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if reloaded.draft_cell_ids_digest != expected_graph.cell_ids_digest
            || reloaded.menu_populated != 1
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Save/reopen changed the authored [8,10] menu identity graph or one action population".into(),
            ));
        }
        if reloaded.selected_ring_index != Some(1) {
            let _ = click_authoring_target(
                child,
                &reopened.window,
                trace_path,
                reopened.session_id,
                AuthoringControlTarget::RingSelector,
                None,
                AuthoringControlRole::ComboBox,
            )
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
            let ring = wait_for_authoring_control(
                trace_path,
                reopened.session_id,
                AuthoringControlTarget::RingOption,
                Some(1),
                AuthoringControlRole::Selectable,
                UIA_TIMEOUT,
            )
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
            click_designer_client_bounds(child, &reopened.window, ring.bounds, trace_path)
                .map_err(|error| {
                    authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                })?;
        }
        let reloaded_ring =
            wait_for_geometry_state(trace_path, reopened.session_id, TRACE_TIMEOUT, |state| {
                state.selected_menu_index == Some(menu_index)
                    && state.selected_ring_index == Some(1)
                    && state.selected_ring_slots == 10
                    && state.menu_populated == 1
                    && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
                    && state.selected_menu_after_action
                        == Some(multi_launcher::radial::model::AfterActionPolicy::CloseTree)
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let reloaded_canvas_cell = wait_for_canvas_cell_in_generation(
            trace_path,
            reopened.session_id,
            reloaded_ring.generation,
            cell_index,
            expected_graph.cell_ids_digest,
            1,
            cell_slot_index,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if reloaded_canvas_cell.index != Some(cell_index) {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "reopened authored menu did not publish the same ring-scoped CanvasCell index {cell_index}; observed {:?}",
                    reloaded_canvas_cell.index
                ),
            ));
        }
        let reloaded_cell_click_cursor = trace_lines(trace_path).len();
        let reloaded_cell_click = click_designer_client_bounds(
            child,
            &reopened.window,
            reloaded_canvas_cell.bounds,
            trace_path,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        wait_for_authoring_control_click_finished(
            trace_path,
            reloaded_cell_click_cursor,
            reopened.session_id,
            AuthoringControlTarget::CanvasCell,
            Some(cell_index),
            AuthoringControlRole::Region,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        wait_for_authoring_control_selected(
            trace_path,
            reopened.session_id,
            AuthoringControlTarget::CanvasCell,
            Some(cell_index),
            AuthoringControlRole::Region,
            true,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let reloaded_cell =
            wait_for_geometry_state(trace_path, reopened.session_id, TRACE_TIMEOUT, |state| {
                state.selected_menu_index == Some(menu_index)
                    && state.selected_ring_index == Some(1)
                    && state.selected_ring_slots == 10
                    && state.menu_populated == 1
                    && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
                    && state.selected_cell_index == Some(cell_slot_index)
                    && state.selected_cell_id_digest == Some(cell_identity)
                    && state.selected_cell_custom_action_index_known
                    && state.selected_cell_custom_action_index == Some(target_action_index)
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let root_summary = if let Some(root_graph) = overflow_root_graph.as_ref() {
            let root_row = wait_for_authoring_control(
                trace_path,
                reopened.session_id,
                AuthoringControlTarget::MenuRow,
                Some(root_graph.menu_index),
                AuthoringControlRole::Selectable,
                UIA_TIMEOUT,
            )
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
            if !root_row.selected {
                click_designer_client_bounds(child, &reopened.window, root_row.bounds, trace_path)
                    .map_err(|error| {
                        authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                    })?;
            }
            let reloaded_root_menu =
                wait_for_geometry_state(trace_path, reopened.session_id, TRACE_TIMEOUT, |state| {
                    state.selected_menu_index == Some(root_graph.menu_index)
                        && state.ring_count == root_graph.ring_slots.len()
                        && state.draft_cell_ids_digest == root_graph.cell_ids_digest
                })
                .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
            let root_ring_click = if reloaded_root_menu.selected_ring_index == Some(0) {
                None
            } else {
                click_authoring_target(
                    child,
                    &reopened.window,
                    trace_path,
                    reopened.session_id,
                    AuthoringControlTarget::RingSelector,
                    None,
                    AuthoringControlRole::ComboBox,
                )
                .and_then(|_| {
                    wait_for_authoring_control(
                        trace_path,
                        reopened.session_id,
                        AuthoringControlTarget::RingOption,
                        Some(0),
                        AuthoringControlRole::Selectable,
                        UIA_TIMEOUT,
                    )
                })
                .and_then(|ring| {
                    click_designer_client_bounds(child, &reopened.window, ring.bounds, trace_path)
                })
                .map(Some)
                .map_err(|error| {
                    authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                })?
            };
            let reloaded_root =
                wait_for_geometry_state(trace_path, reopened.session_id, TRACE_TIMEOUT, |state| {
                    state.selected_menu_index == Some(root_graph.menu_index)
                        && state.ring_count == root_graph.ring_slots.len()
                        && state.selected_ring_index == Some(0)
                        && state.selected_ring_slots == 8
                        && state.menu_populated == root_graph.populated_cells
                        && state.draft_cell_ids_digest == root_graph.cell_ids_digest
                })
                .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
            if root_graph.ring_slots != [8, 1] || root_graph.populated_cells != 9 {
                return Err(CaseFailure::new(
                    FailureStage::DesignerMutation,
                    format!(
                        "G1 expected persisted root graph is not [8,1] / 9 populated: rings={:?}, populated={}",
                        root_graph.ring_slots, root_graph.populated_cells
                    ),
                ));
            }
            let _ = (root_row, root_ring_click, reloaded_root);
            format!(
                "overflow_root=[8,1]/9; root_ids={}",
                root_graph.cell_ids_digest
            )
        } else {
            "original_menus_preserved=true".to_string()
        };
        verify_no_leaf_side_effects(profile, trace_path, &side_effect_baseline)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        reopened_entry = Some(reopened);
        let _ = (
            persisted,
            menu_click,
            ring_one_click,
            policy_combo_click,
            policy_click,
            cell_click,
            save_click,
            row,
            reloaded_cell_click,
        );
        let saved_glow = saved_glow;
        Ok(format!(
            "evidence:v1; typed_radial=decoded; authored_geometry=[8,10]; action_binding=true; cell_index={cell_index}; cell_identity={cell_identity}; action_index={target_action_index}; after_action=close_tree; {root_summary}; glow={saved_glow}; reopened=true; save_generation={}; reopened_generation={}",
            saved_draft.generation, reloaded_cell.generation
        ))
    })();
    append_case(
        report,
        "A6",
        expected("A6"),
        a6_started,
        a6_result,
        Some(child),
        output,
        trace_path,
    );
    reopened_entry.map(|entry| (entry, style_glow_after_edit.unwrap_or(false)))
}

fn verify_saved_authoring_fixture(
    profile: &Path,
    authored_graph: &PersistedMenuGraphExpectation,
    root_graph: Option<&PersistedMenuGraphExpectation>,
    cell_index: usize,
    target_action_index: usize,
    skin_index: usize,
    expected_glow: bool,
    original_menu_sha256: Option<&[String]>,
) -> Result<String, String> {
    let radial_bytes = fs::read(profile.join("radial.json"))
        .map_err(|error| format!("read saved radial.json: {error}"))?;
    let decoded = multi_launcher::radial::migration::decode_document(&radial_bytes)
        .map_err(|error| format!("typed decode saved radial.json: {error:?}"))?;
    let actions_bytes = fs::read(profile.join("actions.json"))
        .map_err(|error| format!("read deterministic action fixture: {error}"))?;
    let actions = serde_json::from_slice::<Vec<multi_launcher::actions::Action>>(&actions_bytes)
        .map_err(|error| format!("decode deterministic action fixture: {error}"))?;
    let expected_action = actions
        .get(target_action_index)
        .ok_or_else(|| "target action index was absent from deterministic fixture".to_string())?;
    let menu = decoded
        .document
        .menus
        .get(authored_graph.menu_index)
        .ok_or_else(|| "saved document omitted the authored menu".to_string())?;
    if menu.after_action != multi_launcher::radial::model::AfterActionPolicy::CloseTree {
        return Err(format!(
            "saved authored menu did not retain its checked Close tree after-action policy: {:?}",
            menu.after_action
        ));
    }
    let ring = menu
        .rings
        .get(1)
        .ok_or_else(|| "saved authored menu omitted its selected ring".to_string())?;
    let cell = ring
        .cells
        .get(cell_index)
        .ok_or_else(|| "saved authored ring omitted the selected cell".to_string())?;
    let authored_slots = menu
        .rings
        .iter()
        .map(|ring| ring.cells.len())
        .collect::<Vec<_>>();
    if authored_slots != authored_graph.ring_slots
        || count_menu_populated_cells(menu) != authored_graph.populated_cells + 1
    {
        return Err(format!(
            "saved A0/G0/A2 menu geometry/population changed: expected rings {:?} with {} blank cells plus the assigned action, observed rings {authored_slots:?} and {} populated cells",
            authored_graph.ring_slots,
            authored_graph.populated_cells,
            count_menu_populated_cells(menu)
        ));
    }
    let binding = match &cell.content {
        multi_launcher::radial::model::CellContent::Action {
            binding: multi_launcher::radial::model::ActionBinding::Persisted { action },
        } => action,
        _ => return Err("saved selected cell was not a persisted action binding".into()),
    };
    match binding.target.as_ref() {
        Some(multi_launcher::universal_actions::PersistableActionTargetRef::CustomAction {
            action,
        }) if action == expected_action => {}
        _ => {
            return Err(
                "saved action binding did not match the exact target fixture source row".into(),
            );
        }
    }
    if binding.action_id.as_str().is_empty() {
        return Err("saved action binding omitted its semantic action ID".into());
    }
    validate_menu_identity_graph(menu, "authored")?;

    let root_summary = if let Some(root_graph) = root_graph {
        let root_menu = decoded
            .document
            .menus
            .get(root_graph.menu_index)
            .ok_or_else(|| "saved document omitted the G1 root menu".to_string())?;
        let root_slots = root_menu
            .rings
            .iter()
            .map(|ring| ring.cells.len())
            .collect::<Vec<_>>();
        if root_slots != root_graph.ring_slots
            || count_menu_populated_cells(root_menu) != root_graph.populated_cells
        {
            return Err(format!(
                "saved G1 root graph did not preserve overflow resolution: expected rings {:?} / {} populated cells, observed {root_slots:?} / {}",
                root_graph.ring_slots,
                root_graph.populated_cells,
                count_menu_populated_cells(root_menu)
            ));
        }
        validate_menu_identity_graph(root_menu, "G1 root")?;
        format!(
            "G1 root index {} retained resolved geometry {:?}, {} populated cells and stable menu/ring/cell IDs",
            root_graph.menu_index,
            root_slots,
            count_menu_populated_cells(root_menu)
        )
    } else if let Some(expected_menus) = original_menu_sha256 {
        if decoded.document.menus.len() != expected_menus.len() + 1 {
            return Err("saved copied profile did not preserve its original menu count plus one derived menu".into());
        }
        for (index, expected) in expected_menus.iter().enumerate() {
            let menu = decoded.document.menus.get(index).ok_or_else(|| {
                "saved copied profile omitted an original radial menu".to_string()
            })?;
            let actual = serde_json::to_vec(menu)
                .map(|bytes| sha256_bytes(&bytes))
                .map_err(|_| "saved original radial menu could not be hashed".to_string())?;
            if &actual != expected {
                return Err("authoring changed an original copied radial menu definition".into());
            }
        }
        "original_menus_preserved=true".to_string()
    } else {
        return Err(
            "saved authoring had neither a fixture root graph nor copied menu baseline".into(),
        );
    };
    let skin = decoded
        .document
        .skins
        .get(skin_index)
        .ok_or_else(|| "saved document omitted the edited skin".to_string())?;
    if !matches!(
        &skin.style.values.effects.glow_enabled,
        multi_launcher::radial::model::Override::Value(value) if *value == expected_glow
    ) {
        return Err("saved skin did not retain the requested harmless glow edit".into());
    }
    Ok(format!(
        "typed radial.json decoded the authored menu index {} with geometry {:?}, {} populated action cells including action source index {target_action_index}, stable authored ID graph, and persisted Close tree policy; {root_summary}; selected skin glow={expected_glow} persisted",
        authored_graph.menu_index,
        authored_slots,
        count_menu_populated_cells(menu),
    ))
}

fn count_menu_populated_cells(menu: &multi_launcher::radial::model::MenuDefinition) -> usize {
    menu.rings
        .iter()
        .flat_map(|ring| &ring.cells)
        .filter(|cell| {
            !matches!(
                &cell.content,
                multi_launcher::radial::model::CellContent::Spacer
            )
        })
        .count()
}

fn validate_menu_identity_graph(
    menu: &multi_launcher::radial::model::MenuDefinition,
    label: &str,
) -> Result<(), String> {
    if menu.id.as_str().is_empty() {
        return Err(format!("saved {label} menu had an empty stable ID"));
    }
    let mut ring_ids = std::collections::BTreeSet::new();
    let mut cell_ids = std::collections::BTreeSet::new();
    for ring in &menu.rings {
        if ring.id.as_str().is_empty() || !ring_ids.insert(ring.id.as_str()) {
            return Err(format!("saved {label} ring IDs were empty or repeated"));
        }
        for cell in &ring.cells {
            if cell.id.as_str().is_empty() || !cell_ids.insert(cell.id.as_str()) {
                return Err(format!("saved {label} cell IDs were empty or repeated"));
            }
        }
    }
    Ok(())
}

fn capture_action_side_effect_baseline(
    profile: &Path,
    trace_path: &Path,
) -> Result<ActionSideEffectBaseline, String> {
    let history_path = profile.join("history.json");
    let history = match fs::read(&history_path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("read pre-A3 history baseline: {error}")),
    };
    let trace = fs::read_to_string(trace_path)
        .map_err(|error| format!("read pre-A3 leaf-dispatch trace baseline: {error}"))?;
    let trace_event_count = trace
        .lines()
        .filter(|line| line.contains("trace_event=\""))
        .count();
    Ok(ActionSideEffectBaseline {
        history,
        trace_event_count,
    })
}

fn verify_no_leaf_side_effects(
    profile: &Path,
    trace_path: &Path,
    baseline: &ActionSideEffectBaseline,
) -> Result<String, String> {
    let history_path = profile.join("history.json");
    let history_after = match fs::read(&history_path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("read history after Designer workflow: {error}")),
    };
    if history_after != baseline.history {
        return Err("Design authoring changed the candidate history file".into());
    }
    let trace = fs::read_to_string(trace_path)
        .map_err(|error| format!("read full leaf-dispatch trace: {error}"))?;
    let event_lines = trace
        .lines()
        .filter(|line| line.contains("trace_event=\""))
        .collect::<Vec<_>>();
    if event_lines.len() < baseline.trace_event_count {
        return Err(format!(
            "trace lost events after baseline: {} before A3, {} now",
            baseline.trace_event_count,
            event_lines.len()
        ));
    }
    let after = &event_lines[baseline.trace_event_count..];
    if after
        .iter()
        .any(|line| line.contains("trace_event=\"radial_action\""))
    {
        return Err("Designer workflow emitted a real radial leaf action dispatch".into());
    }
    let mut dispatch_samples = 0usize;
    for line in after
        .iter()
        .filter(|line| line.contains("trace_event=\"native_preview_dispatch_count\""))
    {
        let count = trace_field(line, "count")
            .ok_or_else(|| "preview dispatch trace omitted its count".to_string())?
            .parse::<usize>()
            .map_err(|error| format!("parse preview dispatch count: {error}"))?;
        dispatch_samples = dispatch_samples.saturating_add(1);
        if count != 0 {
            return Err(format!(
                "Designer workflow observed a nonzero native preview leaf-dispatch count {count}"
            ));
        }
    }
    Ok(format!(
        "full trace from the pre-A3 baseline through this point contains no radial_action event; all {dispatch_samples} native preview dispatch samples are zero and history is byte-identical"
    ))
}

fn append_blocked_continued_designer_cases(
    report: &mut AcceptanceReport,
    cause: &CaseFailure,
    child: Option<&NativeChild>,
    output: &Path,
    trace_path: &Path,
) {
    if !report.cases.iter().any(|case| case.id == "A3") {
        append_case(
            report,
            "A3",
            expected("A3"),
            started_now(),
            Err(cause.clone()),
            child,
            output,
            trace_path,
        );
    }
    append_blocked_ids(
        report,
        &["A4", "A5", "A6", "A7", "A8", "D3", "D6", "D7"],
        cause,
        output,
        trace_path,
        "A3 post-geometry Designer entry failed",
    );
}

fn append_blocked_lifecycle_cases(report: &mut AcceptanceReport, output: &Path, trace_path: &Path) {
    let cause = CaseFailure::new(
        FailureStage::DesignerReadiness,
        "lifecycle sequence was not run because Save/reopen did not produce a ready Designer"
            .into(),
    );
    append_blocked_ids(
        report,
        &["A7", "A8", "D3", "D6", "D7"],
        &cause,
        output,
        trace_path,
        "Save/reopen did not produce a ready Designer",
    );
}

fn append_blocked_ids(
    report: &mut AcceptanceReport,
    ids: &[&str],
    cause: &CaseFailure,
    output: &Path,
    trace_path: &Path,
    reason: &str,
) {
    for id in ids {
        if report.cases.iter().any(|case| case.id == *id) {
            continue;
        }
        append_case(
            report,
            id,
            expected(id),
            started_now(),
            Err(CaseFailure::new(
                cause.stage,
                format!("not run because {reason}: {}", cause.message),
            )),
            None,
            output,
            trace_path,
        );
    }
}

fn run_designer_lifecycle_cases(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    uia: &UiAutomation,
    anchor: &FocusAnchor,
    entry: &DesignerEntry,
    output: &Path,
    trace_path: &Path,
    profile: &Path,
    side_effect_baseline: &ActionSideEffectBaseline,
    saved_glow: bool,
    skin_index: usize,
) {
    append_case(
        report,
        "D3",
        expected("D3"),
        started_now(),
        run_root_hidden_designer_case(child, uia, &entry.window, entry.session_id, trace_path),
        Some(child),
        output,
        trace_path,
    );

    append_authoring_case(report, child, output, trace_path, "A7", || {
        let skins = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Skins,
            entry.session_id,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "Skins mode did not publish for Undo/Redo".into(),
            )
        })?;
        if !skins.selected {
            click_designer_client_bounds(child, &entry.window, skins.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        }
        let skin = wait_for_authoring_control(
            trace_path,
            entry.session_id,
            AuthoringControlTarget::SkinRow,
            Some(skin_index),
            AuthoringControlRole::Button,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if !skin.selected {
            click_designer_client_bounds(child, &entry.window, skin.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        }
        wait_for_authoring_control_selected(
            trace_path,
            entry.session_id,
            AuthoringControlTarget::SkinRow,
            Some(skin_index),
            AuthoringControlRole::Button,
            true,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let glow = wait_for_unique_authoring_control_any_index(
            trace_path,
            entry.session_id,
            AuthoringControlTarget::SkinGlowEnabled,
            AuthoringControlRole::Checkbox,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if glow.selected != saved_glow {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "saved copied skin glow value was not present before Undo".into(),
            ));
        }
        let before_edit =
            wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |_| true)
                .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let edit_click =
            click_designer_client_bounds(child, &entry.window, glow.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        let edited =
            wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |state| {
                state.generation > before_edit.generation
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        wait_for_authoring_control_selected(
            trace_path,
            entry.session_id,
            AuthoringControlTarget::SkinGlowEnabled,
            glow.index,
            AuthoringControlRole::Checkbox,
            !saved_glow,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let undo = click_authoring_target(
            child,
            &entry.window,
            trace_path,
            entry.session_id,
            AuthoringControlTarget::Undo,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let undone =
            wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |state| {
                state.generation > edited.generation
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let restored = wait_for_authoring_control_selected(
            trace_path,
            entry.session_id,
            AuthoringControlTarget::SkinGlowEnabled,
            glow.index,
            AuthoringControlRole::Checkbox,
            saved_glow,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let redo = click_authoring_target(
            child,
            &entry.window,
            trace_path,
            entry.session_id,
            AuthoringControlTarget::Redo,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let redone =
            wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |state| {
                state.generation > undone.generation
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let reapplied = wait_for_authoring_control_selected(
            trace_path,
            entry.session_id,
            AuthoringControlTarget::SkinGlowEnabled,
            glow.index,
            AuthoringControlRole::Checkbox,
            !saved_glow,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let _ = (edit_click, undo.1, redo.1, restored, reapplied);
        Ok(format!(
            "evidence:v1; undo_restored={saved_glow}; redo_restored={}; edit_generation={}; undo_generation={}; redo_generation={}",
            !saved_glow, edited.generation, undone.generation, redone.generation
        ))
    });

    append_authoring_case(report, child, output, trace_path, "A8", || {
        let cursor = trace_lines(trace_path).len();
        let visible_hosts_before = visible_radial_host_windows(child);
        let (start_control, start_click) = click_authoring_target(
            child,
            &entry.window,
            trace_path,
            entry.session_id,
            AuthoringControlTarget::OpenDesktopPreview,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let start_reply = wait_for_terminal_authoring_request(
            trace_path,
            cursor,
            entry.session_id,
            "StartNativePreview",
            UIA_TIMEOUT,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "safe desktop preview did not receive its correlated same-session terminal start reply".into(),
            )
        })?;
        if !wait_until(TRACE_TIMEOUT, || {
            visible_radial_host_windows(child) != visible_hosts_before
        }) {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "accepted safe preview start did not create a visible child-owned radial preview surface".into(),
            ));
        }
        let stop_cursor = trace_lines(trace_path).len();
        let (stop_control, tab_events, enter_events) = tab_and_activate_authoring_control(
            child,
            &entry.window,
            trace_path,
            entry.session_id,
            start_reply.identity.generation,
            AuthoringControlTarget::StopDesktopPreview,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerFrameworkInput, error))?;
        let stop_click = wait_for_authoring_control_clicked(
            trace_path,
            stop_cursor,
            entry.session_id,
            AuthoringControlTarget::StopDesktopPreview,
            TRACE_TIMEOUT,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerFrameworkInput,
                "checked focused Enter did not reach the production Stop desktop preview widget"
                    .into(),
            )
        })?;
        let stop_reply = wait_for_terminal_authoring_request(
            trace_path,
            stop_cursor,
            entry.session_id,
            "StopNativePreview",
            UIA_TIMEOUT,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "safe desktop preview did not receive its correlated same-session terminal stop reply".into(),
            )
        })?;
        if !wait_until(Duration::from_secs(2), || {
            visible_radial_host_windows(child) == visible_hosts_before
        }) {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "accepted safe preview stop left a visible child-owned radial preview surface"
                    .into(),
            ));
        }
        let dispatch_counts = trace_lines(trace_path)
            .into_iter()
            .skip(cursor)
            .filter(|line| {
                line.contains("trace_event=\"native_preview_dispatch_count\"")
                    && trace_field(line, "editor_session")
                        .is_some_and(|value| value == entry.session_id.to_string())
            })
            .filter_map(|line| trace_field(&line, "count")?.parse::<usize>().ok())
            .collect::<Vec<_>>();
        if dispatch_counts.len() < 2 || dispatch_counts.iter().any(|count| *count != 0) {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "safe preview did not prove a zero-dispatch baseline and terminal count: observations={dispatch_counts:?}"
                ),
            ));
        }
        let no_side_effects =
            verify_no_leaf_side_effects(profile, trace_path, side_effect_baseline)
                .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        Ok(format!(
            "checked {:?} click=[{}] started safe preview with correlated request={} generation={}; {} checked Tab events focused the current enabled Stop desktop preview button {:?} and {} checked Enter events reached widget evidence [{}] and terminal request={} generation={}; native preview surface returned to its pre-start window set, dispatch count stayed zero across {} samples, and complete A3-through-A8 side-effect oracle passed: {no_side_effects}",
            start_control.target,
            start_click.describe(),
            start_reply.identity.request_id,
            start_reply.identity.generation,
            tab_events,
            stop_control.target,
            enter_events,
            stop_click,
            stop_reply.identity.request_id,
            stop_reply.identity.generation,
            dispatch_counts.len()
        ))
    });

    append_authoring_case(report, child, output, trace_path, "D6", || {
        let saved_radial = fs::read(profile.join("radial.json"))
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let before = wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |_| true)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let prompt_cursor = trace_lines(trace_path).len();
        let alt_f4 = send_alt_f4(child, &entry.window)
            .map_err(|error| authoring_case_failure(FailureStage::InputInjection, error))?;
        let prompt = wait_trace(trace_path, prompt_cursor, TRACE_TIMEOUT, |events| {
            events
                .iter()
                .any(|line| designer_close_matches(line, entry.session_id, true, true))
        })
        .into_iter()
        .rev()
        .find(|line| designer_close_matches(line, entry.session_id, true, true))
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                format!("dirty Alt+F4 input ({alt_f4} events) did not open the close prompt"),
            )
        })?;
        let keep_cursor = trace_lines(trace_path).len();
        let (keep, keep_click) = click_authoring_target(
            child,
            &entry.window,
            trace_path,
            entry.session_id,
            AuthoringControlTarget::KeepEditing,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let keep_clicked = wait_for_authoring_control_clicked(
            trace_path,
            keep_cursor,
            entry.session_id,
            AuthoringControlTarget::KeepEditing,
            TRACE_TIMEOUT,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "native Keep Editing input did not reach the same session's current widget".into(),
            )
        })?;
        let kept = wait_trace(trace_path, keep_cursor, TRACE_TIMEOUT, |events| {
            events
                .iter()
                .any(|line| designer_close_matches(line, entry.session_id, false, true))
        });
        if !kept
            .iter()
            .any(|line| designer_close_matches(line, entry.session_id, false, true))
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Keep Editing did not clear the prompt while retaining a dirty draft".into(),
            ));
        }
        if child.designer().is_none()
            || !uia.root_is_queryable(entry.window.hwnd, child.process_id())
            || child
                .try_wait()
                .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?
                .is_some()
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "Keep Editing did not retain the same live, queryable Designer and candidate"
                    .into(),
            ));
        }
        let glow = wait_for_unique_authoring_control_any_index(
            trace_path,
            entry.session_id,
            AuthoringControlTarget::SkinGlowEnabled,
            AuthoringControlRole::Checkbox,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if glow.selected != !saved_glow {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Keep Editing did not retain the unsaved style draft from Redo".into(),
            ));
        }
        let after_keep =
            wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |_| true)
                .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if after_keep.generation != before.generation {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Keep Editing changed the retained authoring generation".into(),
            ));
        }
        let discard = discard_dirty_designer(child, &entry.window, trace_path, entry.session_id)
            .map_err(|error| authoring_case_failure(FailureStage::Cleanup, error))?;
        let discarded_radial = fs::read(profile.join("radial.json"))
            .map_err(|error| authoring_case_failure(FailureStage::Cleanup, error))?;
        if discarded_radial != saved_radial {
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                "explicit discard changed the saved radial.json instead of dropping the dirty style draft".into(),
            ));
        }
        let _ = (prompt, keep, keep_click, keep_clicked, glow, discard);
        Ok(format!(
            "evidence:v1; keep_editing=retained_dirty; draft_glow={}; generation={}; discard=saved_json_unchanged; same_designer=true",
            !saved_glow, after_keep.generation
        ))
    });

    append_case(
        report,
        "D7",
        expected("D7"),
        started_now(),
        run_disposable_close_case(child, uia, anchor, trace_path, profile),
        Some(child),
        output,
        trace_path,
    );
}

fn run_root_hidden_designer_case(
    child: &mut NativeChild,
    uia: &UiAutomation,
    designer: &WindowSnapshot,
    session_id: u64,
    trace_path: &Path,
) -> Result<String, CaseFailure> {
    let initial = child
        .refresh_root()
        .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
    require_visible(&initial)
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    let app_hook_thread = hook_service_thread_id(trace_path);
    let mut runner_observer = RunnerHookObserver::start().map_err(|error| {
        CaseFailure::new(
            FailureStage::InputInjection,
            format!("could not install independent D3 keyboard-hook observer: {error}"),
        )
    })?;
    let runner_thread = runner_observer.thread_id();
    let runner_liveness_before = thread_liveness(runner_thread);
    let runner_probe_before = runner_observer
        .pump_roundtrip(
            NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed),
            Duration::from_millis(500),
        )
        .map(|()| "acknowledged".to_string())
        .unwrap_or_else(|error| format!("failed: {error}"));
    let app_liveness_before = app_hook_thread
        .map(thread_liveness)
        .unwrap_or_else(|| "production hook thread id unavailable".into());
    let app_probe_before = app_hook_thread.map_or_else(
        || "production pre-hide pump probe unavailable: service thread id missing".into(),
        |thread_id| {
            probe_production_hook_pump(
                thread_id,
                child.process_id(),
                trace_path,
                Duration::from_millis(500),
            )
        },
    );
    child
        .focus_window(designer)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    focus_is_validated(designer.hwnd, child.process_id()).map_err(|error| {
        CaseFailure::new(
            FailureStage::InputInjection,
            format!("D3 hide requires the exact Designer foreground HWND/PID: {error}"),
        )
    })?;
    let mut cursor = trace_lines(trace_path).len();
    let drained_before_hide = runner_observer.drain_pending();
    let hide_input = child
        .send_f11(designer.hwnd, child.process_id(), TAP_TIME)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let hide_runner = runner_observer.wait_for_vk(0x7A, TRACE_TIMEOUT);
    let hidden_events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
        tap_trace_complete(events, 1, false)
            || (has_trace(
                events,
                "hook_observed",
                &["vk=122", "down=true", "injected=true"],
            ) && has_trace(
                events,
                "hook_observed",
                &["vk=122", "down=false", "injected=true"],
            ))
    });
    if !tap_trace_complete(&hidden_events, 1, false) {
        let product_pair = has_trace(
            &hidden_events,
            "hook_observed",
            &["vk=122", "down=true", "injected=true"],
        ) && has_trace(
            &hidden_events,
            "hook_observed",
            &["vk=122", "down=false", "injected=true"],
        );
        let runner_pair = hide_runner.down_seen
            && hide_runner.up_seen
            && hide_runner.down_injected
            && hide_runner.up_injected;
        let app_liveness_after = app_hook_thread
            .map(thread_liveness)
            .unwrap_or_else(|| "production hook thread id unavailable".into());
        let app_probe_after = app_hook_thread.map_or_else(
            || "production post-hide pump probe unavailable: service thread id missing".into(),
            |thread_id| {
                probe_production_hook_pump(
                    thread_id,
                    child.process_id(),
                    trace_path,
                    Duration::from_millis(500),
                )
            },
        );
        let runner_probe_after = runner_observer
            .pump_roundtrip(
                NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed),
                Duration::from_millis(500),
            )
            .map(|()| "acknowledged".to_string())
            .unwrap_or_else(|error| format!("failed: {error}"));
        let actual_foreground = capture_foreground();
        let root_after_input = child.refresh_root().ok();
        let classification = if runner_pair && !product_pair {
            "independent runner hook observed the exact injected pair while the production callback did not; this localizes to production hook-chain delivery/lifetime"
        } else if !runner_pair && !product_pair {
            "neither independent nor production hook observed the pair; native input delivery/environment remains upstream"
        } else if product_pair {
            "production hook observed the pair but did not complete the short-tap/admission/visibility chain"
        } else {
            "production hook evidence was incomplete"
        };
        let _ = runner_observer.stop_and_report();
        return Err(CaseFailure::new(
            tap_trace_failure_stage(&hidden_events, false),
            format!(
                "Designer-focused hide input [{}] did not complete one short ROOT toggle: {}; classification={classification}; runner_observer=[{}], drained_before_hide={drained_before_hide}, runner_thread_before=[{}], runner_pump_before={runner_probe_before}, runner_thread_after=[{}], runner_pump_after={runner_probe_after}, production_thread_before=[{}], production_pump_before=[{}], production_thread_after=[{}], production_pump_after=[{}], foreground_after=HWND:{} PID:{}, ROOT_before={:?}, ROOT_after_input={:?}, hook_and_admission_trace={:?}",
                hide_input.describe(),
                input_trace_summary(&hidden_events),
                hide_runner.describe(),
                runner_liveness_before,
                thread_liveness(runner_thread),
                app_liveness_before,
                app_probe_before,
                app_liveness_after,
                app_probe_after,
                hwnd_id(actual_foreground.0),
                actual_foreground.1,
                initial.bounds,
                root_after_input
                    .as_ref()
                    .map(|root| (hwnd_id(root.hwnd), root.bounds)),
                hidden_events
                    .iter()
                    .filter(|line| {
                        line.contains("hook_observed")
                            || line.contains("hook_primary")
                            || line.contains("hook_admission")
                            || line.contains("configured_primary")
                            || line.contains("short_tap")
                            || line.contains("desired_visibility")
                            || line.contains("root_command")
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            ),
        ));
    }
    if !(hide_runner.down_seen
        && hide_runner.up_seen
        && hide_runner.down_injected
        && hide_runner.up_injected)
    {
        let _ = runner_observer.stop_and_report();
        return Err(CaseFailure::new(
            FailureStage::InputInjection,
            format!(
                "production short-tap visibility trace completed, but independent runner observer did not prove injected F11 down/up: {}",
                hide_runner.describe()
            ),
        ));
    }
    let hide_hook_observed = has_trace(
        &hidden_events,
        "hook_observed",
        &["vk=122", "down=true", "injected=true"],
    ) && has_trace(
        &hidden_events,
        "hook_observed",
        &["vk=122", "down=false", "injected=true"],
    );
    if !hide_hook_observed {
        let _ = runner_observer.stop_and_report();
        return Err(CaseFailure::new(
            FailureStage::InputInjection,
            format!(
                "ROOT hide trace completed without production HookObserved down/up despite independent runner proof; events={:?}",
                hidden_events
            ),
        ));
    }
    if !wait_root_visibility(child, false, ROOT_TIMEOUT) {
        let current = child.refresh_root().ok();
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            format!(
                "production F11 hide trace completed, but ROOT did not settle outside the virtual screen within {:?}; latest bounds={:?}",
                ROOT_TIMEOUT,
                current.as_ref().map(|root| root.bounds)
            ),
        ));
    }
    let hidden = child
        .refresh_root()
        .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
    require_hidden(&hidden)
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    let physical_displays = native_display_bounds()
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    let app_liveness_after_hide = app_hook_thread
        .map(thread_liveness)
        .unwrap_or_else(|| "production hook thread id unavailable".into());
    let app_probe_after_hide = app_hook_thread.map_or_else(
        || "production post-hide pump probe unavailable: service thread id missing".into(),
        |thread_id| {
            probe_production_hook_pump(
                thread_id,
                child.process_id(),
                trace_path,
                Duration::from_millis(500),
            )
        },
    );
    let runner_liveness_after_hide = thread_liveness(runner_thread);
    let runner_probe_after_hide = runner_observer
        .pump_roundtrip(
            NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed),
            Duration::from_millis(500),
        )
        .map(|()| "acknowledged".to_string())
        .unwrap_or_else(|error| format!("failed: {error}"));
    if !uia.root_is_queryable(designer.hwnd, child.process_id()) {
        return Err(CaseFailure::new(
            FailureStage::DesignerNativeTarget,
            "Designer stopped answering UIA while ROOT was parked".into(),
        ));
    }
    let mode = wait_for_designer_semantic_target_in_session(
        trace_path,
        DesignerSemanticTarget::Skins,
        session_id,
        UIA_TIMEOUT,
        |_| true,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerReadiness,
            "Designer did not publish Skins target while ROOT was hidden".into(),
        )
    })?;
    let mode_click = if mode.selected {
        None
    } else {
        Some(
            click_designer_client_bounds(child, designer, mode.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?,
        )
    };
    let _selected_mode = wait_for_designer_semantic_target_in_session(
        trace_path,
        DesignerSemanticTarget::Skins,
        session_id,
        TRACE_TIMEOUT,
        |state| state.selected,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerFrameworkInput,
            "checked Designer pointer click did not select Skins while ROOT was hidden".into(),
        )
    })?;
    let visible_hosts_before = visible_radial_host_windows(child);
    let preview_cursor = trace_lines(trace_path).len();
    let (preview_control, preview_click) = click_authoring_target(
        child,
        designer,
        trace_path,
        session_id,
        AuthoringControlTarget::OpenDesktopPreview,
        None,
        AuthoringControlRole::Button,
    )
    .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
    let preview_start = wait_for_terminal_authoring_request(
        trace_path,
        preview_cursor,
        session_id,
        "StartNativePreview",
        UIA_TIMEOUT,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerReadiness,
            "Designer accepted Skins input while ROOT was hidden but its safe preview request did not receive a correlated same-session service reply".into(),
        )
    })?;
    if !wait_until(TRACE_TIMEOUT, || {
        visible_radial_host_windows(child) != visible_hosts_before
    }) {
        return Err(CaseFailure::new(
            FailureStage::DesignerPresentation,
            "accepted D3 preview start did not create a visible child-owned radial preview surface"
                .into(),
        ));
    }
    let root_after_preview_start = require_parked_root_state(child, &hidden, &physical_displays)
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    let stop_cursor = trace_lines(trace_path).len();
    let (stop_control, tab_events, enter_events) = tab_and_activate_authoring_control(
        child,
        designer,
        trace_path,
        session_id,
        preview_start.identity.generation,
        AuthoringControlTarget::StopDesktopPreview,
    )
    .map_err(|error| CaseFailure::new(FailureStage::DesignerFrameworkInput, error))?;
    let stop_click = wait_for_authoring_control_clicked(
        trace_path,
        stop_cursor,
        session_id,
        AuthoringControlTarget::StopDesktopPreview,
        TRACE_TIMEOUT,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerFrameworkInput,
            "checked focused Enter did not reach the production Stop desktop preview widget while ROOT was hidden".into(),
        )
    })?;
    let preview_stop = wait_for_terminal_authoring_request(
        trace_path,
        stop_cursor,
        session_id,
        "StopNativePreview",
        UIA_TIMEOUT,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerReadiness,
            "Designer accepted Stop desktop preview while ROOT was hidden but its same-session stop request did not receive a terminal service reply".into(),
        )
    })?;
    if !wait_until(Duration::from_secs(2), || {
        visible_radial_host_windows(child) == visible_hosts_before
    }) {
        return Err(CaseFailure::new(
            FailureStage::DesignerPresentation,
            "terminal Stop desktop preview reply left a visible child-owned radial preview window"
                .into(),
        ));
    }
    let root_after_preview_stop = require_parked_root_state(child, &hidden, &physical_displays)
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    if !uia.root_is_queryable(designer.hwnd, child.process_id()) {
        return Err(CaseFailure::new(
            FailureStage::DesignerNativeTarget,
            "Designer lost its child-owned UIA root while ROOT was hidden".into(),
        ));
    }
    child
        .focus_window(designer)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    focus_is_validated(designer.hwnd, child.process_id()).map_err(|error| {
        CaseFailure::new(
            FailureStage::InputInjection,
            format!("D3 show requires the exact Designer foreground HWND/PID: {error}"),
        )
    })?;
    cursor = trace_lines(trace_path).len();
    let drained_before_show = runner_observer.drain_pending();
    let show_input = child
        .send_f11(designer.hwnd, child.process_id(), TAP_TIME)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let show_runner = runner_observer.wait_for_vk(0x7A, TRACE_TIMEOUT);
    let show_events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
        tap_trace_complete(events, 1, true)
            || (has_trace(
                events,
                "hook_observed",
                &["vk=122", "down=true", "injected=true"],
            ) && has_trace(
                events,
                "hook_observed",
                &["vk=122", "down=false", "injected=true"],
            ))
    });
    if !tap_trace_complete(&show_events, 1, true) {
        let _ = runner_observer.stop_and_report();
        return Err(CaseFailure::new(
            tap_trace_failure_stage(&show_events, true),
            format!(
                "Designer-focused show input [{}] did not complete one short ROOT toggle: {}; independent observer=[{}], drained before show={drained_before_show}, production HookObserved pair={}, trace={:?}",
                show_input.describe(),
                input_trace_summary(&show_events),
                show_runner.describe(),
                has_trace(
                    &show_events,
                    "hook_observed",
                    &["vk=122", "down=true", "injected=true"],
                ) && has_trace(
                    &show_events,
                    "hook_observed",
                    &["vk=122", "down=false", "injected=true"],
                ),
                show_events
            ),
        ));
    }
    let show_hook_observed = has_trace(
        &show_events,
        "hook_observed",
        &["vk=122", "down=true", "injected=true"],
    ) && has_trace(
        &show_events,
        "hook_observed",
        &["vk=122", "down=false", "injected=true"],
    );
    if !show_hook_observed
        || !(show_runner.down_seen
            && show_runner.up_seen
            && show_runner.down_injected
            && show_runner.up_injected)
    {
        let _ = runner_observer.stop_and_report();
        return Err(CaseFailure::new(
            FailureStage::InputInjection,
            format!(
                "production show tap completed without both independent and product F11 pairs: runner=[{}], production_pair={show_hook_observed}, trace={:?}",
                show_runner.describe(),
                show_events
            ),
        ));
    }
    let app_liveness_after_show = app_hook_thread
        .map(thread_liveness)
        .unwrap_or_else(|| "production hook thread id unavailable".into());
    let app_probe_after_show = app_hook_thread.map_or_else(
        || "production post-show pump probe unavailable: service thread id missing".into(),
        |thread_id| {
            probe_production_hook_pump(
                thread_id,
                child.process_id(),
                trace_path,
                Duration::from_millis(500),
            )
        },
    );
    let runner_liveness_after_show = thread_liveness(runner_thread);
    let runner_probe_after_show = runner_observer
        .pump_roundtrip(
            NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed),
            Duration::from_millis(500),
        )
        .map(|()| "acknowledged".to_string())
        .unwrap_or_else(|error| format!("failed: {error}"));
    if !wait_root_visibility(child, true, ROOT_TIMEOUT) {
        let current = child.refresh_root().ok();
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            format!(
                "production F11 show trace completed, but ROOT did not settle on-screen within {:?}; latest bounds={:?}",
                ROOT_TIMEOUT,
                current.as_ref().map(|root| root.bounds)
            ),
        ));
    }
    let shown = child
        .refresh_root()
        .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
    require_visible(&shown)
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    if !uia.root_is_queryable(designer.hwnd, child.process_id())
        || child.try_wait().ok().flatten().is_some()
    {
        return Err(CaseFailure::new(
            FailureStage::DesignerNativeTarget,
            "Designer UIA/service or child process failed after ROOT show".into(),
        ));
    }
    let settled_cursor = trace_lines(trace_path).len();
    let extra_visibility = wait_trace(
        trace_path,
        settled_cursor,
        Duration::from_millis(250),
        |events| {
            events
                .iter()
                .any(|line| line.contains("trace_event=\"desired_visibility\""))
        },
    );
    if extra_visibility
        .iter()
        .any(|line| line.contains("trace_event=\"desired_visibility\""))
    {
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            "unexpected additional ROOT visibility toggle followed the checked hide/show pair"
                .into(),
        ));
    }
    let observer_cleanup = runner_observer
        .stop_and_report()
        .map(|()| "independent D3 hook observer uninstalled cleanly".to_string())
        .unwrap_or_else(|error| format!("independent D3 observer cleanup failed: {error}"));
    if !observer_cleanup.ends_with("cleanly") {
        return Err(CaseFailure::new(FailureStage::Cleanup, observer_cleanup));
    }
    let _ = (
        hide_input,
        hidden,
        mode_click,
        preview_control,
        preview_click,
        root_after_preview_start,
        stop_control,
        tab_events,
        enter_events,
        stop_click,
        root_after_preview_stop,
        show_input,
        shown,
        runner_liveness_before,
        runner_probe_before,
        runner_liveness_after_hide,
        runner_probe_after_hide,
        runner_liveness_after_show,
        runner_probe_after_show,
        app_liveness_before,
        app_probe_before,
        app_liveness_after_hide,
        app_probe_after_hide,
        app_liveness_after_show,
        app_probe_after_show,
        observer_cleanup,
        extra_visibility,
    );
    Ok(format!(
        "evidence:v1; root_hidden=true; designer_responsive=true; preview_start=accepted; preview_stop=accepted; root_shown=true; hook_pairs=true; start_request={}; stop_request={}; no_extra_toggle=true",
        preview_start.identity.request_id, preview_stop.identity.request_id
    ))
}

fn run_disposable_close_case(
    child: &mut NativeChild,
    uia: &UiAutomation,
    anchor: &FocusAnchor,
    trace_path: &Path,
    profile: &Path,
) -> Result<String, CaseFailure> {
    let entry = run_designer_entry(child, uia, anchor, trace_path)?;
    let menus = wait_for_designer_semantic_target_in_session(
        trace_path,
        DesignerSemanticTarget::Menus,
        entry.session_id,
        UIA_TIMEOUT,
        |_| true,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerReadiness,
            "fresh D7 Designer session did not publish its Menus mode target".into(),
        )
    })?;
    let menu_mode_click = if menus.selected {
        None
    } else {
        Some(
            click_designer_client_bounds(child, &entry.window, menus.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?,
        )
    };
    wait_for_designer_semantic_target_in_session(
        trace_path,
        DesignerSemanticTarget::Menus,
        entry.session_id,
        TRACE_TIMEOUT,
        |state| state.selected,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerMutation,
            "checked D7 mode transition did not select Menus before preparing New Menu".into(),
        )
    })?;
    let baseline = wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |_| true)
        .map_err(|error| CaseFailure::new(FailureStage::DesignerReadiness, error))?;
    let mut hold = AcceptancePrepareHold::create(profile, "D7 preview preparation")
        .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
    let preparation_cursor = trace_lines(trace_path).len();
    let (new_menu, new_menu_click) = click_authoring_target(
        child,
        &entry.window,
        trace_path,
        entry.session_id,
        AuthoringControlTarget::NewMenu,
        None,
        AuthoringControlRole::Button,
    )
    .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
    let changed = wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |state| {
        state.generation > baseline.generation && state.menu_count == baseline.menu_count + 1
    })
    .map_err(|error| CaseFailure::new(FailureStage::DesignerMutation, error))?;
    if !new_menu.enabled || changed.selected_menu_index != Some(changed.menu_count - 1) {
        return Err(CaseFailure::new(
            FailureStage::DesignerMutation,
            "checked D7 New Menu did not leave a selected dirty menu for preview preparation"
                .into(),
        ));
    }
    let request = wait_for_authoring_request_generation(
        trace_path,
        preparation_cursor,
        entry.session_id,
        "PrepareEmbeddedPreview",
        changed.generation,
        TRACE_TIMEOUT,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerReadiness,
            "dirty New Menu selection did not enqueue its real PrepareEmbeddedPreview request"
                .into(),
        )
    })?;
    let held = wait_trace(trace_path, preparation_cursor, TRACE_TIMEOUT, |events| {
        events
            .iter()
            .any(|line| acceptance_prepare_gate_matches(line, request, "Held"))
    });
    let gate_held = held
        .iter()
        .find(|line| acceptance_prepare_gate_matches(line, request, "Held"))
        .cloned()
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "D7 service did not enter the bounded gate for the real preview request".into(),
            )
        })?;
    if held.iter().any(|line| {
        authoring_edge_matches(line, request, "PrepareEmbeddedPreview", "ReplyEnqueued")
    }) {
        return Err(CaseFailure::new(
            FailureStage::DesignerReadiness,
            "D7 preview service replied before the controlled pending interval began".into(),
        ));
    }
    let close_cursor = trace_lines(trace_path).len();
    let key_events = send_alt_f4(child, &entry.window)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let close_events = wait_trace(trace_path, close_cursor, TRACE_TIMEOUT, |events| {
        let cancelled = events
            .iter()
            .any(|line| disposable_cancel_matches(line, request, "PrepareEmbeddedPreview"));
        let close_prompt = events
            .iter()
            .any(|line| designer_close_matches(line, entry.session_id, true, true));
        cancelled && close_prompt
    });
    let cancelled = close_events
        .iter()
        .find(|line| disposable_cancel_matches(line, request, "PrepareEmbeddedPreview"))
        .cloned()
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "close did not cancel the exact pending PrepareEmbeddedPreview request".into(),
            )
        })?;
    let close_state = close_events
        .iter()
        .find(|line| designer_close_matches(line, entry.session_id, true, true))
        .cloned()
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                format!(
                    "Alt+F4 events={key_events} did not produce a same-session dirty close prompt"
                ),
            )
        })?;
    if trace_field_value(&close_state, "pending_disposable") != Some("false") {
        return Err(CaseFailure::new(
            FailureStage::DesignerMutation,
            "close state did not reflect local cancellation before prompt publication".into(),
        ));
    }
    let close_index = close_events
        .iter()
        .position(|line| designer_close_matches(line, entry.session_id, true, true))
        .unwrap_or(usize::MAX);
    let cancel_index = close_events
        .iter()
        .position(|line| disposable_cancel_matches(line, request, "PrepareEmbeddedPreview"))
        .unwrap_or(usize::MAX);
    if cancel_index >= close_index {
        return Err(CaseFailure::new(
            FailureStage::DesignerMutation,
            "request cancellation was not observed before the close prompt state".into(),
        ));
    }
    let release_cursor = trace_lines(trace_path).len();
    let release_evidence = hold
        .release()
        .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?;
    let released_events = wait_trace(trace_path, release_cursor, TRACE_TIMEOUT, |events| {
        events
            .iter()
            .any(|line| acceptance_prepare_gate_matches(line, request, "Released"))
            && events.iter().any(|line| {
                authoring_edge_matches(line, request, "PrepareEmbeddedPreview", "ReplyEnqueued")
            })
    });
    let gate_released = released_events
        .iter()
        .find(|line| acceptance_prepare_gate_matches(line, request, "Released"))
        .cloned()
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::Cleanup,
                "D7 service did not observe removal of the hold marker before its bounded timeout"
                    .into(),
            )
        })?;
    let late_reply = wait_for_authoring_edge(
        trace_path,
        release_cursor,
        request,
        "PrepareEmbeddedPreview",
        "ReplyRejected",
        TRACE_TIMEOUT,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerMutation,
            "the late prepared-frame reply was not rejected after the close path cancelled it"
                .into(),
        )
    })?;
    let cursor = trace_lines(trace_path).len();
    let (discard, click) = click_authoring_target(
        child,
        &entry.window,
        trace_path,
        entry.session_id,
        AuthoringControlTarget::DiscardDraft,
        None,
        AuthoringControlRole::Button,
    )
    .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
    let stop = wait_for_terminal_authoring_request(
        trace_path,
        cursor,
        entry.session_id,
        "StopNativePreview",
        TRACE_TIMEOUT,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::Cleanup,
            "D7 discard did not receive its same-session terminal preview-stop reply".into(),
        )
    })?;
    if !wait_until(Duration::from_secs(4), || child.designer().is_none()) {
        return Err(CaseFailure::new(
            FailureStage::DesignerPresentation,
            "Designer did not close after D7 terminal disposal".into(),
        ));
    }
    if child.refresh_root().is_err()
        || child
            .try_wait()
            .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?
            .is_some()
    {
        return Err(CaseFailure::new(
            FailureStage::Cleanup,
            "D7 disposal closed ROOT or terminated the child process".into(),
        ));
    }
    let _closed_observation = verify_designer_stays_closed_for(child, Duration::from_secs(1))
        .map_err(|error| CaseFailure::new(FailureStage::DesignerPresentation, error))?;
    if hold.path.exists() {
        return Err(CaseFailure::new(
            FailureStage::Cleanup,
            "D7 preview hold marker remained after the terminal path".into(),
        ));
    }
    let _ = (
        key_events,
        menu_mode_click,
        new_menu,
        new_menu_click,
        gate_held,
        cancelled,
        close_state,
        release_evidence,
        gate_released,
        late_reply,
        discard,
        click,
    );
    Ok(format!(
        "evidence:v1; pending_request=true; request_id={}; generation={}; cancelled_before_prompt=true; late_reply=rejected; stop=accepted; stop_request={}; no_reopen=1s; marker_clean=true; child_alive=true",
        request.request_id, changed.generation, stop.identity.request_id
    ))
}

fn wait_for_authoring_control_selected(
    trace_path: &Path,
    session_id: u64,
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
    selected: bool,
    timeout: Duration,
) -> Result<AuthoringControlSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(control) = find_authoring_control(trace_path, session_id, target, index, role)?
            && control.selected == selected
        {
            return Ok(control);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer target {target:?} index={index:?} did not become selected={selected} for session {session_id}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_canvas_cell_in_generation(
    trace_path: &Path,
    session_id: u64,
    generation: u64,
    flat_index: usize,
    menu_cell_ids_digest: u64,
    ring_index: usize,
    slot_index: usize,
    timeout: Duration,
) -> Result<AuthoringControlSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let controls = list_authoring_controls(trace_path, session_id)?;
        if let Some(cell) = fresh_canvas_cell_for_generation(
            &controls,
            session_id,
            generation,
            flat_index,
            menu_cell_ids_digest,
            ring_index,
            slot_index,
        ) {
            return Ok(cell);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "selected blank menu did not publish a fresh authored spacer in session {session_id} generation {generation}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn flat_canvas_cell_index(
    ring_slots: &[usize],
    ring_index: usize,
    slot_index: usize,
) -> Option<usize> {
    let current_ring_slots = *ring_slots.get(ring_index)?;
    if slot_index >= current_ring_slots {
        return None;
    }
    ring_slots
        .iter()
        .take(ring_index)
        .try_fold(slot_index, |flat_index, slots| {
            flat_index.checked_add(*slots)
        })
}

fn wait_for_authoring_control_click_finished(
    trace_path: &Path,
    first_line: usize,
    session_id: u64,
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let lines = trace_lines(trace_path);
        if authoring_control_click_finished(
            lines.get(first_line..).unwrap_or_default(),
            session_id,
            target,
            index,
            role,
        ) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer target {target:?} index={index:?} did not publish a completed native click in session {session_id}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_action_catalog_rank(
    trace_path: &Path,
    session_id: u64,
    custom_action_index: usize,
    timeout: Duration,
) -> Result<ActionCatalogRankSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(rank) = action_catalog_ranks(trace_path)?.into_iter().find(|rank| {
            rank.session_id == session_id && rank.custom_action_index == custom_action_index
        }) {
            return Ok(rank);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "unfiltered catalog rank for custom action index {custom_action_index} was not published in session {session_id}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn run_populated_shrink_resolution(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
) -> Result<String, String> {
    let root_row = wait_for_authoring_control(
        trace_path,
        session_id,
        AuthoringControlTarget::MenuRow,
        Some(0),
        AuthoringControlRole::Selectable,
        UIA_TIMEOUT,
    )?;
    let root_select = click_designer_client_bounds(child, designer, root_row.bounds, trace_path)?;
    let root = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
        !state.proposal_active
            && !state.resize_prompt_open
            && state.selected_menu_index == Some(0)
            && state.selected_ring_index == Some(0)
            && state.ring_count > 0
    })?;
    if root.selected_ring_slots <= 1 || root.selected_ring_populated == 0 {
        return Err(format!(
            "starter root ring is not suitable for a populated shrink: rings={} slots={} populated={}",
            root.ring_count, root.selected_ring_slots, root.selected_ring_populated
        ));
    }

    let requested = root.selected_ring_slots - 1;
    let slots_input = set_requested_slots(child, designer, trace_path, session_id, requested)?;
    let (_, preview_click) = click_authoring_target(
        child,
        designer,
        trace_path,
        session_id,
        AuthoringControlTarget::PreviewProposal,
        None,
        AuthoringControlRole::Button,
    )?;
    let prompted = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
        state.resize_prompt_open && state.resize_prompt_populated > 0
    })?;
    if prompted.selected_menu_index != Some(0)
        || prompted.selected_ring_index != Some(0)
        || prompted.selected_ring_slots != root.selected_ring_slots
        || prompted.selected_ring_populated != root.selected_ring_populated
        || prompted.menu_populated != root.menu_populated
        || prompted.requested_slots != requested
        || prompted.proposal_active
        || prompted.resize_prompt_populated > root.selected_ring_populated
    {
        return Err(format!(
            "populated shrink prompt changed committed state or described an impossible number of overflow cells: root slots/population/menu={}/{}/{}, prompt slots/population/menu={}/{}/{}, requested={}, prompt overflow cells={}",
            root.selected_ring_slots,
            root.selected_ring_populated,
            root.menu_populated,
            prompted.selected_ring_slots,
            prompted.selected_ring_populated,
            prompted.menu_populated,
            prompted.requested_slots,
            prompted.resize_prompt_populated
        ));
    }
    let (_, cancel_click) = click_authoring_target(
        child,
        designer,
        trace_path,
        session_id,
        AuthoringControlTarget::CancelResolution,
        None,
        AuthoringControlRole::Button,
    )?;
    let cancelled = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
        !state.resize_prompt_open && !state.proposal_active && state.requested_slots == requested
    })?;
    if cancelled.selected_ring_slots != root.selected_ring_slots
        || cancelled.selected_ring_populated != root.selected_ring_populated
        || cancelled.menu_populated != root.menu_populated
        || cancelled.ring_count != root.ring_count
    {
        return Err(format!(
            "Cancel changed committed geometry or removed content: slots {}/{} population {}/{} menu population {}/{} rings {}/{}",
            root.selected_ring_slots,
            cancelled.selected_ring_slots,
            root.selected_ring_populated,
            cancelled.selected_ring_populated,
            root.menu_populated,
            cancelled.menu_populated,
            root.ring_count,
            cancelled.ring_count
        ));
    }

    let slots_again = set_requested_slots(child, designer, trace_path, session_id, requested)?;
    let (_, preview_again) = click_authoring_target(
        child,
        designer,
        trace_path,
        session_id,
        AuthoringControlTarget::PreviewProposal,
        None,
        AuthoringControlRole::Button,
    )?;
    let prompted_again = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
        state.resize_prompt_open && state.resize_prompt_populated > 0
    })?;
    if prompted_again.menu_populated != root.menu_populated
        || prompted_again.selected_ring_slots != root.selected_ring_slots
        || prompted_again.resize_prompt_populated != prompted.resize_prompt_populated
    {
        return Err("reopening populated shrink resolution changed committed content".into());
    }
    let (_, overflow_click) = click_authoring_target(
        child,
        designer,
        trace_path,
        session_id,
        AuthoringControlTarget::MoveToOverflow,
        None,
        AuthoringControlRole::Button,
    )?;
    let resolved = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
        state.proposal_active
            && state.proposal_kind == AuthoringProposalKind::ResolvedResize
            && state.proposal_ready
    })?;
    if resolved.resize_prompt_open
        || resolved.ring_count != root.ring_count
        || resolved.selected_ring_slots != root.selected_ring_slots
        || resolved.menu_populated != root.menu_populated
        || resolved.proposal_candidate_rings != root.ring_count + 1
        || resolved.proposal_slots != requested
        || resolved.proposal_resolution_populated != prompted.resize_prompt_populated
        || !resolved.proposal_cell_ids_preserved
    {
        return Err(format!(
            "overflow resolution did not prepare a preserving candidate: rings={} committed_slots={} menu_populated={} candidate_rings={} proposal_slots={} resolved_populated={} original_populated={} stable_ids_preserved={} ready={}",
            resolved.ring_count,
            resolved.selected_ring_slots,
            resolved.menu_populated,
            resolved.proposal_candidate_rings,
            resolved.proposal_slots,
            resolved.proposal_resolution_populated,
            prompted.resize_prompt_populated,
            resolved.proposal_cell_ids_preserved,
            resolved.proposal_ready
        ));
    }
    let apply = wait_for_authoring_control(
        trace_path,
        session_id,
        AuthoringControlTarget::ApplyProposal,
        None,
        AuthoringControlRole::Button,
        UIA_TIMEOUT,
    )?;
    if !apply.enabled {
        return Err(
            "Apply proposal remained disabled after overflow resolution was prepared".into(),
        );
    }
    let apply_click = click_designer_client_bounds(child, designer, apply.bounds, trace_path)?;
    let applied = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
        !state.proposal_active
            && state.ring_count == root.ring_count + 1
            && state.selected_menu_index == Some(0)
            && state.selected_ring_index == Some(0)
            && state.selected_ring_slots == requested
            && state.generation > resolved.generation
    })?;
    let post_apply_ids_preserved = committed_cell_ids_match(
        resolved.proposal_cell_ids_digest_available,
        resolved.proposal_cell_ids_digest,
        applied.draft_cell_ids_digest,
    );
    if applied.menu_populated != root.menu_populated
        || applied.selected_ring_populated >= root.selected_ring_populated
        || !post_apply_ids_preserved
    {
        return Err(format!(
            "overflow Apply did not preserve menu content and prepared stable cell IDs while shrinking the selected ring: menu populated {} -> {}, selected ring populated {} -> {}, stable cell IDs match candidate={}",
            root.menu_populated,
            applied.menu_populated,
            root.selected_ring_populated,
            applied.selected_ring_populated,
            post_apply_ids_preserved
        ));
    }
    Ok(format!(
        "selected starter root through MenuRow 0 {:?} with checked click=[{}]; {slots_input}; Preview=[{}] retained {} slots, {} populated cells, menu population {} while showing a {}-cell resolution prompt; Cancel=[{}] preserved all counts; {slots_again}; reopened Preview=[{}], Move to overflow=[{}] prepared {} slots and {} rings with {} populated cells resolved and all prior cell IDs preserved; Apply enabled={}, checked click=[{}] yielded {} rings, {} slots, {} selected-ring populated cells and unchanged menu population {}; committed stable cell IDs match the prepared candidate={}",
        root_row.bounds,
        root_select.describe(),
        preview_click.describe(),
        root.selected_ring_slots,
        root.selected_ring_populated,
        root.menu_populated,
        prompted.resize_prompt_populated,
        cancel_click.describe(),
        preview_again.describe(),
        overflow_click.describe(),
        resolved.proposal_slots,
        resolved.proposal_candidate_rings,
        resolved.proposal_resolution_populated,
        apply.enabled,
        apply_click.describe(),
        applied.ring_count,
        applied.selected_ring_slots,
        applied.selected_ring_populated,
        applied.menu_populated,
        post_apply_ids_preserved
    ))
}

fn run_compact_geometry_case(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
) -> Result<String, String> {
    let post_resize_trace_cursor = trace_lines(trace_path).len();
    child.resize_window(designer, 640, 480)?;
    let deadline = Instant::now() + TRACE_TIMEOUT;
    let (compact, client) = loop {
        let compact = child
            .designer()
            .ok_or_else(|| "Designer HWND disappeared during compact resize".to_string())?;
        if compact.hwnd != designer.hwnd {
            return Err("Designer HWND changed during compact resize".into());
        }
        let client = child.client_bounds(&compact)?;
        let outer_width = compact.bounds[2] - compact.bounds[0];
        let outer_height = compact.bounds[3] - compact.bounds[1];
        if compact.visible
            && !compact.minimized
            && compact.is_nonzero()
            && outer_width <= 700
            && outer_height <= 540
            && client[2] - client[0] >= 520
            && client[3] - client[1] >= 380
        {
            break (compact, client);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "compact Designer did not reach visible bounded outer/client geometry: outer={:?} client={client:?}",
                compact.bounds
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    };
    // Authoring-control events carry the client size seen by that egui frame. The
    // pre-resize cursor retains the resize-triggered frame even if it is rendered
    // before USER's refreshed HWND snapshot becomes observable; only a frame matching
    // the compact native client size can satisfy the check.
    let targets = [
        (
            AuthoringControlTarget::NewMenu,
            None,
            AuthoringControlRole::Button,
        ),
        (
            AuthoringControlTarget::AddRing,
            None,
            AuthoringControlRole::Button,
        ),
        (
            AuthoringControlTarget::RingSelector,
            None,
            AuthoringControlRole::ComboBox,
        ),
        (
            AuthoringControlTarget::Slots,
            None,
            AuthoringControlRole::DragValue,
        ),
        (
            AuthoringControlTarget::PreviewProposal,
            None,
            AuthoringControlRole::Button,
        ),
        (
            AuthoringControlTarget::Canvas,
            None,
            AuthoringControlRole::Region,
        ),
    ];
    let mut controls = Vec::with_capacity(targets.len());
    for (target, index, role) in targets {
        let control = wait_for_authoring_control_after(
            trace_path,
            post_resize_trace_cursor,
            session_id,
            [client[2] - client[0], client[3] - client[1]],
            target,
            index,
            role,
            TRACE_TIMEOUT,
        )?;
        let [left, top, right, bottom] = control.bounds;
        if left < client[0] || top < client[1] || right > client[2] || bottom > client[3] {
            let allocation = designer_canvas_allocations_after(
                trace_path,
                post_resize_trace_cursor,
                session_id,
            )?
            .into_iter()
            .rev()
            .find(|allocation| allocation.requested_size[1] > 0);
            let allocation_detail = allocation.map_or_else(
                || "Canvas allocation/clip snapshot missing".to_string(),
                |allocation| {
                    format!(
                        "allocated={:?} clip={:?} requested_px={:?}",
                        allocation.allocated_rect, allocation.clip_rect, allocation.requested_size
                    )
                },
            );
            return Err(format!(
                "compact Designer target {target:?} bounds {:?} exceed client bounds {client:?}; egui {allocation_detail}",
                control.bounds,
            ));
        }
        if right <= left || bottom <= top {
            return Err(format!(
                "compact Designer target {target:?} has empty bounds {:?}",
                control.bounds
            ));
        }
        controls.push((target, control.bounds));
    }
    let canvas = controls
        .iter()
        .find_map(|(target, bounds)| (*target == AuthoringControlTarget::Canvas).then_some(*bounds))
        .ok_or_else(|| "compact Designer did not publish its canvas target".to_string())?;
    if canvas[2] - canvas[0] < 100 || canvas[3] - canvas[1] < 100 {
        return Err(format!(
            "compact Designer left too little usable canvas area: {canvas:?}"
        ));
    }
    let current = child
        .designer()
        .ok_or_else(|| "Designer HWND disappeared while validating compact geometry".to_string())?;
    let current_client = child.client_bounds(&current)?;
    if current.hwnd != compact.hwnd
        || !current.visible
        || current.minimized
        || !current.is_nonzero()
        || !client_size_matches(
            [
                current_client[2] - current_client[0],
                current_client[3] - current_client[1],
            ],
            [client[2] - client[0], client[3] - client[1]],
        )
        || current.bounds[2] - current.bounds[0] > 700
        || current.bounds[3] - current.bounds[1] > 540
    {
        return Err(format!(
            "compact Designer window changed after fresh controls were rendered: {:?}",
            current.bounds,
        ));
    }
    Ok(format!(
        "resized the checked child Designer HWND to compact outer bounds {:?}; client={client:?}; all authoring controls and canvas stayed inside the client; canvas={canvas:?} ({}x{}); inspected {} semantic targets",
        compact.bounds,
        canvas[2] - canvas[0],
        canvas[3] - canvas[1],
        controls.len()
    ))
}

fn restore_authoring_viewport(
    child: &NativeChild,
    original: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
) -> Result<String, String> {
    let width = original.bounds[2] - original.bounds[0];
    let height = original.bounds[3] - original.bounds[1];
    if width < 700 || height < 540 {
        return Err(format!(
            "original Designer viewport is too small for the retained menu list: {:?}",
            original.bounds
        ));
    }
    let first_line = trace_lines(trace_path).len();
    child.resize_window(original, width, height)?;
    let deadline = Instant::now() + TRACE_TIMEOUT;
    let (current, client) = loop {
        let current = child.designer().ok_or_else(|| {
            "Designer HWND disappeared while restoring its authoring viewport".to_string()
        })?;
        if current.hwnd != original.hwnd || current.process_id != original.process_id {
            return Err("Designer identity changed while restoring its authoring viewport".into());
        }
        let client = child.client_bounds(&current)?;
        let current_width = current.bounds[2] - current.bounds[0];
        let current_height = current.bounds[3] - current.bounds[1];
        if current.visible
            && !current.minimized
            && current_width == width
            && current_height == height
        {
            break (current, client);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "same Designer HWND did not restore to its original authoring viewport: expected={:?}, observed={:?}",
                original.bounds, current.bounds
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    };
    let geometry = latest_geometry_state(trace_path)?
        .filter(|state| state.session_id == session_id)
        .ok_or_else(|| {
            "Designer did not publish geometry before restoring its menu list".to_string()
        })?;
    let last_menu_row = geometry.menu_count.checked_sub(1).ok_or_else(|| {
        "Designer had no menu rows to verify after viewport restoration".to_string()
    })?;
    let client_size = [client[2] - client[0], client[3] - client[1]];
    let row = wait_for_authoring_control_after(
        trace_path,
        first_line,
        session_id,
        client_size,
        AuthoringControlTarget::MenuRow,
        Some(last_menu_row),
        AuthoringControlRole::Selectable,
        TRACE_TIMEOUT,
    )?;
    if row.bounds[0] < client[0]
        || row.bounds[1] < client[1]
        || row.bounds[2] > client[2]
        || row.bounds[3] > client[3]
    {
        return Err(format!(
            "restored last menu row {:?} remains outside the same Designer HWND client {:?}",
            row.bounds, client
        ));
    }
    Ok(format!(
        "restored the same checked Designer HWND={} to its original outer bounds {:?}; fresh client {:?} rendered last MenuRow {} at {:?} inside the client",
        hwnd_id(current.hwnd),
        current.bounds,
        client,
        last_menu_row,
        row.bounds
    ))
}

fn discard_dirty_designer(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
) -> Result<String, String> {
    child.validate_window(designer.hwnd)?;
    let root_recovery = make_root_visible_before_designer_close(child, designer)?;
    let cursor = trace_lines(trace_path).len();
    let key_events = send_alt_f4(child, designer)?;
    let prompt_trace = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
        events.iter().any(|line| {
            line.contains("trace_event=\"designer_close\"")
                && line.contains("close_prompt=true")
                && line.contains("dirty=true")
        })
    })
    .into_iter()
    .rev()
    .find(|line| {
        line.contains("trace_event=\"designer_close\"")
            && line.contains("close_prompt=true")
            && line.contains("dirty=true")
    })
    .ok_or_else(|| {
        format!("Alt+F4 did not present a dirty close prompt after {key_events} input events")
    })?;
    let (discard, click) = click_authoring_target(
        child,
        designer,
        trace_path,
        session_id,
        AuthoringControlTarget::DiscardDraft,
        None,
        AuthoringControlRole::Button,
    )?;
    let stop_reply = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
        events.iter().any(|line| {
            line.contains("trace_event=\"authoring\"")
                && line.contains("edge=ReplyAccepted")
                && line.contains("request_kind=StopNativePreview")
                && line.contains("terminal=true")
        })
    })
    .into_iter()
    .rev()
    .find(|line| {
        line.contains("trace_event=\"authoring\"")
            && line.contains("edge=ReplyAccepted")
            && line.contains("request_kind=StopNativePreview")
            && line.contains("terminal=true")
    })
    .ok_or_else(|| {
        format!("Discard did not receive its terminal preview-stop reply: {prompt_trace}")
    })?;
    if !wait_until(Duration::from_secs(4), || child.designer().is_none()) {
        return Err(format!(
            "child-owned Designer HWND remained after production Discard; terminal reply={stop_reply}"
        ));
    }
    let root = child
        .refresh_root()
        .map_err(|error| format!("production Discard removed ROOT: {error}"))?;
    let displays = native_display_bounds()?;
    if !root.visible || root.minimized || !intersects_display_bounds(root.bounds, &displays) {
        return Err(format!(
            "production Discard left ROOT offscreen or nondrawable: visible={} minimized={} bounds={:?}",
            root.visible, root.minimized, root.bounds
        ));
    }
    if child.try_wait()?.is_some() {
        return Err("production Discard terminated the candidate process".into());
    }
    let mut stable_bounds = None;
    let mut stable_samples = 0_u8;
    let stable_on_display = wait_until(Duration::from_millis(750), || {
        let Ok(current) = child.refresh_root() else {
            stable_bounds = None;
            stable_samples = 0;
            return false;
        };
        if current.visible
            && !current.minimized
            && intersects_display_bounds(current.bounds, &displays)
        {
            if stable_bounds == Some(current.bounds) {
                stable_samples = stable_samples.saturating_add(1);
            } else {
                stable_bounds = Some(current.bounds);
                stable_samples = 1;
            }
            stable_samples >= 3
        } else {
            stable_bounds = None;
            stable_samples = 0;
            false
        }
    });
    if !stable_on_display {
        return Err(format!(
            "production Discard did not leave ROOT stably drawable on a physical display; last ROOT={:?}",
            child.refresh_root().ok()
        ));
    }
    Ok(format!(
        "{}; checked Alt+F4={key_events} events reached the dirty close prompt ({prompt_trace}); production Discard target {:?} click=[{}] received terminal stop reply ({stop_reply}) and closed its child-owned Designer HWND while on-screen ROOT and candidate remained alive",
        root_recovery
            .as_deref()
            .unwrap_or("ROOT was already visible on a physical display before Designer close"),
        discard.bounds,
        click.describe()
    ))
}

fn make_root_visible_before_designer_close(
    child: &NativeChild,
    designer: &WindowSnapshot,
) -> Result<Option<String>, String> {
    child.validate_window(designer.hwnd)?;
    let displays = native_display_bounds()?;
    let root = child.refresh_root()?;
    if root.visible && !root.minimized && intersects_display_bounds(root.bounds, &displays) {
        return Ok(None);
    }

    child.focus_window(designer)?;
    let tap = child.send_f11(designer.hwnd, child.process_id(), TAP_TIME)?;
    if !wait_root_visibility(child, true, ROOT_TIMEOUT) {
        return Err(format!(
            "checked Designer-focused F11 did not restore ROOT before close; input=[{}]",
            tap.describe()
        ));
    }
    let restored = child.refresh_root()?;
    if !intersects_display_bounds(restored.bounds, &displays) {
        return Err(format!(
            "Designer-focused F11 restored ROOT only to offscreen bounds {:?}; physical displays={displays:?}",
            restored.bounds
        ));
    }
    if child
        .designer()
        .is_none_or(|current| current.hwnd != designer.hwnd)
        || child.try_wait()?.is_some()
    {
        return Err("Designer or candidate stopped while restoring ROOT before close".into());
    }
    Ok(Some(format!(
        "checked Designer-focused F11 restored ROOT onscreen at {:?} before close with input=[{}]",
        restored.bounds,
        tap.describe()
    )))
}

fn activate_named(
    uia: &UiAutomation,
    child: &NativeChild,
    anchor: &FocusAnchor,
    target: &WindowSnapshot,
    name: &str,
    stage: FailureStage,
    trace_path: &Path,
) -> Result<PointerClickEvidence, CaseFailure> {
    let displays = native_display_bounds()
        .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
    let deadline = Instant::now() + UIA_TIMEOUT;
    let mut last_retryable_error = None;
    loop {
        if Instant::now() >= deadline {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "could not obtain stable on-screen ROOT geometry for semantic control '{}' before timeout; last recoverable edge={:?}",
                    bounded_label(name),
                    last_retryable_error
                ),
            ));
        }
        let fresh = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        if fresh.hwnd != target.hwnd || fresh.process_id != target.process_id {
            return Err(CaseFailure::new(
                FailureStage::WindowDiscovery,
                "ROOT identity changed while entering the Designer; refusing stale UIA input"
                    .into(),
            ));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let (stable, _) =
            ensure_root_on_physical_display(child, anchor, &fresh, &displays, remaining)
                .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        let control = wait_named_control_in_client(
            uia,
            child,
            &stable,
            name,
            deadline.saturating_duration_since(Instant::now()),
        )
        .map_err(|error| CaseFailure::new(stage, error))?;
        if !control.enabled {
            return Err(CaseFailure::new(
                stage,
                format!("semantic control '{}' is disabled", bounded_label(name)),
            ));
        }

        // UIA bounds must be reacquired after ROOT has been refreshed and stabilized.
        // If ROOT moves between lookup and click, the native helper rejects input before
        // sending a mouse event and this bounded loop resolves a fresh target.
        let (click_root, _) = ensure_root_on_physical_display(
            child,
            anchor,
            &stable,
            &displays,
            deadline.saturating_duration_since(Instant::now()),
        )
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        if click_root.bounds != stable.bounds {
            last_retryable_error = Some("ROOT bounds changed after UIA lookup".to_string());
            continue;
        }
        let latest_control = uia
            .find_named(click_root.hwnd, child.process_id(), name)
            .map_err(|error| CaseFailure::new(stage, error))?
            .ok_or_else(|| {
                CaseFailure::new(
                    stage,
                    format!(
                        "semantic control '{}' disappeared after ROOT stabilization",
                        bounded_label(name)
                    ),
                )
            })?;
        let client = child
            .client_screen_bounds(&click_root)
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        if !semantic_bounds_center_inside(latest_control.bounds, client) {
            last_retryable_error = Some("UIA bounds left the fresh ROOT client".to_string());
            continue;
        }
        if !latest_control.enabled {
            return Err(CaseFailure::new(
                stage,
                format!("semantic control '{}' became disabled", bounded_label(name)),
            ));
        }
        // Menu controls may advertise UIA Invoke without opening a menu. Use the
        // process-validated native pointer path that mirrors the user's interaction.
        match click_semantic_control(child, &click_root, &latest_control, trace_path) {
            Ok(evidence) => return Ok(evidence),
            Err(error)
                if error.contains("blocked precondition: ROOT")
                    || error.contains("geometry changed after semantic bounds") =>
            {
                last_retryable_error = Some(error);
            }
            Err(error) => {
                return Err(CaseFailure::new(FailureStage::InputInjection, error));
            }
        }
    }
}

fn wait_named_control_in_client(
    uia: &UiAutomation,
    child: &NativeChild,
    target: &WindowSnapshot,
    name: &str,
    timeout: Duration,
) -> Result<SemanticControl, String> {
    child.validate_window(target.hwnd)?;
    let deadline = Instant::now() + timeout;
    let mut latest_bounds = None;
    loop {
        let client = child.client_screen_bounds(target)?;
        if let Some(control) = uia.find_named(target.hwnd, child.process_id(), name)? {
            latest_bounds = Some(control.bounds);
            if semantic_bounds_center_inside(control.bounds, client) {
                return Ok(control);
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "UIA control '{}' did not publish fresh bounds inside child client {:?} before timeout; last bounds={latest_bounds:?}",
                bounded_label(name),
                client
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn semantic_bounds_center_inside(bounds: [i32; 4], client: [i32; 4]) -> bool {
    if bounds[2] <= bounds[0]
        || bounds[3] <= bounds[1]
        || client[2] <= client[0]
        || client[3] <= client[1]
    {
        return false;
    }
    let center = [
        bounds[0] + (bounds[2] - bounds[0]) / 2,
        bounds[1] + (bounds[3] - bounds[1]) / 2,
    ];
    center[0] >= client[0]
        && center[0] < client[2]
        && center[1] >= client[1]
        && center[1] < client[3]
}

fn append_blocked_designer_cases(
    report: &mut AcceptanceReport,
    cause: &CaseFailure,
    child: Option<&NativeChild>,
    output: &Path,
    trace_path: &Path,
) {
    for id in BLOCKED_DESIGNER_CASE_IDS {
        if report.cases.iter().any(|case| case.id == id) {
            continue;
        }
        append_case(
            report,
            id,
            expected(id),
            started_now(),
            Err(CaseFailure::new(
                cause.stage,
                format!("not run because D0 failed first: {}", cause.message),
            )),
            child,
            output,
            trace_path,
        );
    }
}

fn append_blocked_authoring_cases(
    report: &mut AcceptanceReport,
    cause: &CaseFailure,
    child: Option<&NativeChild>,
    output: &Path,
    trace_path: &Path,
) {
    for id in ["A0", "A1", "G0", "A2", "G1", "G2"] {
        if report.cases.iter().any(|case| case.id == id) {
            continue;
        }
        append_case(
            report,
            id,
            expected(id),
            started_now(),
            Err(CaseFailure::new(
                cause.stage,
                format!(
                    "not run because authoring Designer entry failed: {}",
                    cause.message
                ),
            )),
            child,
            output,
            trace_path,
        );
    }
}

fn stop_child(
    child: &mut NativeChild,
    report: &mut AcceptanceReport,
    runner_log: &mut File,
    output: &Path,
    trace_path: &Path,
) {
    let started = Instant::now();
    let result = (|| {
        if child.try_wait().ok().flatten().is_some() {
            report.cleanup.child_closed_normally = false;
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                "candidate exited before the runner requested normal shutdown".into(),
            ));
        }
        let mut close_errors = Vec::new();
        if let Ok(root) = child.refresh_root() {
            if let Err(error) = request_window_close(child, &root) {
                close_errors.push(error);
            }
        }
        if let Some(designer) = child.designer() {
            if let Err(error) = request_window_close(child, &designer) {
                close_errors.push(error);
            }
        }
        if let Some(status) = wait_child(child, Duration::from_secs(5)) {
            report.cleanup.child_closed_normally = status.success();
            let hwnd_deadline = Instant::now() + Duration::from_secs(2);
            let mut remaining_windows = child.windows();
            while !remaining_windows.is_empty() && Instant::now() < hwnd_deadline {
                std::thread::sleep(WINDOW_POLL);
                remaining_windows = child.windows();
            }
            report.cleanup.child_owned_windows_closed = remaining_windows.is_empty();
            if status.success() {
                if !report.cleanup.child_owned_windows_closed {
                    let remaining = remaining_windows
                        .iter()
                        .map(|window| format!("{}:{:?}", hwnd_id(window.hwnd), window.role))
                        .collect::<Vec<_>>()
                        .join(",");
                    return Err(CaseFailure::new(
                        FailureStage::Cleanup,
                        format!(
                            "candidate exited normally with {status} but child-owned HWNDs remained: {remaining}"
                        ),
                    ));
                }
                return Ok(format!(
                    "candidate exited normally with {status} after bounded WM_CLOSE; all child-owned HWNDs closed"
                ));
            }
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                format!("candidate exited with failure status {status}"),
            ));
        }
        child
            .kill()
            .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?;
        let status = child
            .wait()
            .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?;
        report.cleanup.child_terminated_after_timeout = true;
        report.cleanup.child_closed_normally = false;
        let close_details = if close_errors.is_empty() {
            String::new()
        } else {
            format!("; WM_CLOSE errors: {}", close_errors.join("; "))
        };
        Err(CaseFailure::new(
            FailureStage::Cleanup,
            format!(
                "normal close timed out; terminated only isolated child PID {} ({status}){close_details}",
                child.process_id(),
            ),
        ))
    })();
    let result =
        if let Some(cleanup_case) = report.cases.iter_mut().find(|case| case.id == "CLEANUP") {
            cleanup_case.elapsed_ms = elapsed_ms(started);
            match result {
                Ok(observed) => {
                    cleanup_case.status = CaseStatus::Passed;
                    cleanup_case.observed = observed;
                    cleanup_case.failure_stage = None;
                }
                Err(error) => {
                    cleanup_case.status = CaseStatus::Failed;
                    cleanup_case.observed = error.message;
                    cleanup_case.failure_stage = Some(error.stage);
                }
            }
            return;
        } else {
            result
        };
    append_case(
        report,
        "CLEANUP",
        expected("CLEANUP"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
    let _ = writeln!(
        runner_log,
        "cleanup closed_normally={} terminated_after_timeout={}",
        report.cleanup.child_closed_normally, report.cleanup.child_terminated_after_timeout
    );
}

fn append_case(
    report: &mut AcceptanceReport,
    id: &str,
    expected: &str,
    started: Instant,
    result: Result<String, CaseFailure>,
    child: Option<&NativeChild>,
    output: &Path,
    trace_path: &Path,
) {
    let mut artifacts = Vec::new();
    let mut evidence_packet = if super::super::hotkey_expected_state(id).is_some() {
        finish_hotkey_evidence_capture(id)
    } else {
        None
    };
    let mut result = result;
    let evidence_validation = evidence_packet.as_ref().map(|packet| {
        crate::validate_hotkey_evidence_packet_with_context(
            packet,
            report.hotkey,
            report.profile.hold_threshold_ms,
        )
    });
    let evidence_error = match evidence_validation {
        Some(Ok(())) => None,
        Some(Err(error)) => Some(format!(
            "typed H evidence packet failed validation: {error}"
        )),
        None if super::super::hotkey_expected_state(id).is_some() => {
            Some("typed H evidence packet was not captured".to_owned())
        }
        None => None,
    };
    if let Some(error) = evidence_error {
        evidence_packet = None;
        result = Err(match result {
            Ok(_) => CaseFailure::new(FailureStage::GestureDecision, error),
            Err(existing) => {
                CaseFailure::new(existing.stage, format!("{}; {error}", existing.message))
            }
        });
    }
    if let Some(packet) = evidence_packet.take() {
        if report.hotkey_evidence.len() >= super::super::MAX_HOTKEY_EVIDENCE_GESTURES {
            report.capacity_saturated = true;
        } else {
            report.hotkey_evidence.push(packet);
        }
    }
    let (status, observed, failure_stage) = match result {
        Ok(observed) => (CaseStatus::Passed, observed, None),
        Err(error) => {
            let saved = if error.message.starts_with("not run because")
                || error
                    .message
                    .starts_with("runner omitted a required case result")
            {
                Vec::new()
            } else {
                save_failure_artifacts(id, child, output, trace_path)
            };
            for path in saved {
                report.push_artifact(path.to_string_lossy());
                artifacts.push(bounded_text(&path.to_string_lossy(), MAX_PATH_BYTES));
            }
            (CaseStatus::Failed, error.message, Some(error.stage))
        }
    };
    report.push_case(AcceptanceCaseResult {
        id: id.to_string(),
        status,
        elapsed_ms: elapsed_ms(started),
        expected: bounded_text(expected, MAX_RESULT_BYTES),
        observed: bounded_text(&observed, MAX_RESULT_BYTES),
        failure_stage,
        artifacts,
    });
}

fn append_case_without_artifacts(
    report: &mut AcceptanceReport,
    id: &str,
    result: Result<String, CaseFailure>,
) {
    let (status, observed, failure_stage) = match result {
        Ok(observed) => (CaseStatus::Passed, observed, None),
        Err(error) => (CaseStatus::Failed, error.message, Some(error.stage)),
    };
    report.push_case(AcceptanceCaseResult {
        id: id.to_string(),
        status,
        elapsed_ms: 0,
        expected: bounded_text(expected(id), MAX_RESULT_BYTES),
        observed: bounded_text(&observed, MAX_RESULT_BYTES),
        failure_stage,
        artifacts: Vec::new(),
    });
}

fn save_failure_artifacts(
    id: &str,
    child: Option<&NativeChild>,
    output: &Path,
    trace_path: &Path,
) -> Vec<PathBuf> {
    let mut written = Vec::new();
    let trace_destination = output.join(format!("case-{id}-trace.log"));
    let trace_excerpt = fs::read_to_string(trace_path)
        .map(|trace| safe_trace_excerpt(&trace))
        .unwrap_or_default();
    if fs::write(&trace_destination, trace_excerpt).is_ok() {
        written.push(trace_destination);
    }
    let diagnostic_prefix = format!("case-{id}-uia-");
    if let Ok(entries) = fs::read_dir(output) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with(&diagnostic_prefix) && name.ends_with(".txt")
                    })
            {
                written.push(path);
            }
        }
    }
    let inventory_destination = output.join(format!("case-{id}-windows.json"));
    let windows = child.map(NativeChild::windows).unwrap_or_default();
    let record = WindowInventory {
        runner_process_id: std::process::id(),
        child_process_id: child.map(NativeChild::process_id),
        windows: windows.iter().map(WindowRecord::from).collect(),
    };
    if serde_json::to_vec_pretty(&record)
        .ok()
        .is_some_and(|bytes| fs::write(&inventory_destination, bytes).is_ok())
    {
        written.push(inventory_destination);
    }
    if let Some(child) = child {
        let private_log_destination = output.join(format!("case-{id}-private.log"));
        if copy_bounded_log_tail(child.log_path(), &private_log_destination).is_ok() {
            written.push(private_log_destination);
        }
    }
    let screenshot_destination = output.join(format!("case-{id}.png"));
    if capture_diagnostic_screenshot(child, &screenshot_destination).is_ok() {
        written.push(screenshot_destination);
    }
    written
}

#[derive(Default)]
struct H17AlternateArtifactSnapshot {
    trace_excerpt: Option<Vec<u8>>,
    windows_inventory: Option<Vec<u8>>,
    private_log_tail: Option<Vec<u8>>,
    capture_errors: Vec<String>,
}

fn capture_h17_alternate_artifacts(
    child: &NativeChild,
    trace_path: &Path,
) -> H17AlternateArtifactSnapshot {
    let mut snapshot = H17AlternateArtifactSnapshot::default();
    match fs::read_to_string(trace_path) {
        Ok(trace) => snapshot.trace_excerpt = Some(safe_trace_excerpt(&trace).into_bytes()),
        Err(error) => snapshot.capture_errors.push(format!(
            "read alternate trace before profile cleanup: {error}"
        )),
    }

    let windows = child.windows();
    let inventory = WindowInventory {
        runner_process_id: std::process::id(),
        child_process_id: Some(child.process_id()),
        windows: windows.iter().map(WindowRecord::from).collect(),
    };
    match serde_json::to_vec_pretty(&inventory) {
        Ok(bytes) => snapshot.windows_inventory = Some(bytes),
        Err(error) => snapshot
            .capture_errors
            .push(format!("serialize alternate HWND inventory: {error}")),
    }

    match read_bounded_log_tail(child.log_path()) {
        Ok(bytes) => snapshot.private_log_tail = Some(bytes),
        Err(error) => snapshot
            .capture_errors
            .push(format!("capture alternate private log tail: {error}")),
    }
    snapshot
}

fn h17_should_preserve_alternate_artifacts(tap_succeeded: bool, cleanup_succeeded: bool) -> bool {
    !tap_succeeded || !cleanup_succeeded
}

fn persist_h17_alternate_artifacts(
    snapshot: &H17AlternateArtifactSnapshot,
    output: &Path,
) -> (Vec<PathBuf>, Vec<String>) {
    let mut written = Vec::new();
    let mut errors = snapshot.capture_errors.clone();
    for (suffix, content) in [
        ("trace.log", snapshot.trace_excerpt.as_deref()),
        ("windows.json", snapshot.windows_inventory.as_deref()),
        ("private.log", snapshot.private_log_tail.as_deref()),
    ] {
        let Some(content) = content else {
            continue;
        };
        let destination = output.join(format!("case-H17-alternate-{suffix}"));
        match fs::write(&destination, content) {
            Ok(()) => written.push(destination),
            Err(error) => errors.push(format!(
                "write alternate {suffix} evidence before temp profile deletion: {error}"
            )),
        }
    }
    (written, errors)
}

fn run_failure_artifact_case(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    output: &Path,
    trace_path: &Path,
) {
    let started = Instant::now();
    let artifacts = save_failure_artifacts("R1", Some(child), output, trace_path);
    let result = validate_r1_artifacts(&artifacts, child.process_id(), output);
    for path in &artifacts {
        report.push_artifact(path.to_string_lossy());
    }
    let (status, observed, failure_stage) = match result {
        Ok(()) => (
            CaseStatus::Passed,
            format!(
                "controlled harness failure produced a bounded sanitized trace, private log tail, validated child HWND inventory, and screenshot cropped from a visible child-owned window; artifact_count={}",
                artifacts.len()
            ),
            None,
        ),
        Err(error) => (
            CaseStatus::Failed,
            bounded_text(
                &format!("controlled diagnostic artifact validation failed: {error}"),
                MAX_RESULT_BYTES,
            ),
            Some(FailureStage::Environment),
        ),
    };
    let report_case_id = if report.mode == "native_windows_copied_profile" {
        "CP_R1"
    } else {
        "R1"
    };
    report.push_case(AcceptanceCaseResult {
        id: report_case_id.into(),
        status,
        elapsed_ms: elapsed_ms(started),
        expected: bounded_text(expected(report_case_id), MAX_RESULT_BYTES),
        observed,
        failure_stage,
        artifacts: artifacts
            .iter()
            .map(|path| bounded_text(&path.to_string_lossy(), MAX_PATH_BYTES))
            .collect(),
    });
}

fn validate_r1_artifacts(
    artifacts: &[PathBuf],
    child_process_id: u32,
    output: &Path,
) -> Result<(), String> {
    let required = [
        output.join("case-R1-trace.log"),
        output.join("case-R1-windows.json"),
        output.join("case-R1-private.log"),
        output.join("case-R1.png"),
    ];
    for path in &required {
        if !artifacts.contains(path) {
            return Err(format!(
                "required bounded artifact is missing: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        let length = fs::metadata(path)
            .map_err(|error| {
                format!(
                    "inspect artifact {}: {error}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )
            })?
            .len();
        let maximum = match path.file_name().and_then(|name| name.to_str()) {
            Some("case-R1-private.log") => MAX_PRIVATE_LOG_BYTES as u64,
            Some("case-R1-trace.log") => MAX_TRACE_BYTES as u64,
            Some("case-R1.png") => 16 * 1024 * 1024,
            _ => 64 * 1024,
        };
        if length == 0 || length > maximum {
            return Err(format!(
                "artifact {} is empty or exceeds the bound",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
    }
    let trace = fs::read_to_string(&required[0])
        .map_err(|error| format!("read sanitized trace: {error}"))?;
    if trace.trim().is_empty() || trace.len() > MAX_TRACE_BYTES {
        return Err("sanitized trace is empty or exceeds its byte bound".into());
    }
    let windows: serde_json::Value = serde_json::from_slice(
        &fs::read(&required[1]).map_err(|error| format!("read child HWND inventory: {error}"))?,
    )
    .map_err(|error| format!("parse child HWND inventory: {error}"))?;
    if windows["child_process_id"].as_u64() != Some(u64::from(child_process_id)) {
        return Err(
            "HWND inventory child process identity does not match the launched candidate".into(),
        );
    }
    let entries = windows["windows"]
        .as_array()
        .ok_or_else(|| "HWND inventory omitted its window list".to_string())?;
    if entries.is_empty()
        || entries
            .iter()
            .any(|entry| entry["process_id"].as_u64() != Some(u64::from(child_process_id)))
    {
        return Err("HWND inventory does not contain only child-owned native windows".into());
    }
    let (width, height) = image::image_dimensions(&required[3])
        .map_err(|error| format!("decode child-owned screenshot dimensions: {error}"))?;
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return Err("child-owned screenshot dimensions are invalid or exceed the bound".into());
    }
    Ok(())
}

fn safe_trace_excerpt(trace: &str) -> String {
    let lines = trace
        .lines()
        .filter_map(sanitize_trace_line)
        .collect::<Vec<_>>();

    if lines.len() <= MAX_TRACE_EXCERPT
        && lines.iter().map(|line| line.len() + 1).sum::<usize>() <= MAX_TRACE_BYTES
    {
        return lines.join("\n");
    }

    // Keep an early startup header and the newest events around the failure. A
    // tail-only excerpt can hide the trace-ready/root-creation context, while a
    // head-only excerpt can end long before a late failure (for example ROOT
    // parking after a Designer close). Bound the two regions independently so
    // an unusually large early event cannot evict the diagnostic tail.
    let startup_end = lines.len().min(STARTUP_TRACE_EVENTS);
    let startup = retain_trace_segment(lines[..startup_end].iter(), STARTUP_TRACE_BYTES, false);
    let tail_count = MAX_TRACE_EXCERPT.saturating_sub(startup.len());
    let tail_byte_budget = MAX_TRACE_BYTES.saturating_sub(STARTUP_TRACE_BYTES + 1);
    let tail = retain_trace_segment(lines.iter().rev().take(tail_count), tail_byte_budget, true);

    startup
        .into_iter()
        .chain(tail)
        .collect::<Vec<_>>()
        .join("\n")
}

fn retain_trace_segment<'a>(
    lines: impl Iterator<Item = &'a String>,
    byte_budget: usize,
    reverse_output: bool,
) -> Vec<String> {
    let mut retained = Vec::new();
    let mut bytes = 0usize;
    for line in lines {
        let next_bytes = bytes.saturating_add(line.len()).saturating_add(1);
        if next_bytes > byte_budget {
            break;
        }
        bytes = next_bytes;
        retained.push(line.clone());
    }
    if reverse_output {
        retained.reverse();
    }
    retained
}

fn sanitize_trace_line(line: &str) -> Option<String> {
    const EVENT_NAMES: &[&str] = &[
        "designer_callback",
        "designer_focus",
        "designer_pointer",
        "designer_pointer_moved",
        "root_pointer_moved",
        "root_pointer_button",
        "root_menu_interaction",
        "root_menu_body",
        "designer_submitted",
        "designer_body",
        "designer_widget",
        "designer_widget_pointer",
        "root_result_pointer",
        "radial_action",
        "designer_mutation",
        "authoring",
        "hook_primary",
        "hook_admission",
        "hook_deadline",
        "hook_service_ready",
        "hook_pump_probe",
        "hook_service_exit",
        "hook_observed",
        "hook_callback",
        "frontend_key",
        "designer_semantic_target",
        "designer_authoring_control",
        "designer_canvas_allocation",
        "designer_action_catalog_rank",
        "native_preview_dispatch_count",
        "designer_geometry_state",
        "designer_edit_state",
        "designer_close",
        "disposable_request_cancelled",
        "acceptance_prepare_gate",
        "designer_preview_rendered",
        "configured_primary",
        "short_tap",
        "desired_visibility",
        "root_command",
        "window_sample_truncated",
        "restore",
        "native_window_snapshot",
        "native_activation",
        "native_pointer",
        "budget_exhausted",
        "trace_ready",
    ];
    const SAFE_FIELDS: &[&str] = &[
        "elapsed_ms",
        "event_budget",
        "phase",
        "viewport",
        "edge",
        "request_id",
        "request_kind",
        "session_id",
        "generation",
        "menu_cell_ids_digest",
        "cell_ring_index",
        "cell_slot_index",
        "terminal",
        "pointer_down",
        "pointer_up",
        "window_under_cursor_hwnd",
        "window_under_cursor_owner",
        "cursor_screen_x",
        "cursor_screen_y",
        "client_x",
        "client_y",
        "screen_x",
        "screen_y",
        "menu",
        "hovered",
        "clicked",
        "open",
        "entered",
        "close_prompt",
        "dirty",
        "pending_disposable",
        "pending_durable",
        "pending_native_preview",
        "state",
        "category",
        "response",
        "pointer_pressed",
        "pointer_released",
        "button_down_on",
        "pointer_inside",
        "layer_is_topmost",
        "has_position",
        "pointer_x",
        "pointer_y",
        "kind",
        "result_index",
        "pressed",
        "released",
        "stage",
        "skins",
        "editor_open",
        "skins_selected",
        "panel_registered",
        "result",
        "transition",
        "provenance",
        "foreground_owner",
        "owner",
        "global_exclusive_owners",
        "adapter_exclusive",
        "recovery",
        "deadline_scheduled",
        "radial_intent",
        "invocation_id",
        "timer_id",
        "delay_ms",
        "thread_id",
        "desktop",
        "command",
        "source",
        "requested_x",
        "requested_y",
        "requested_width",
        "requested_height",
        "primary_vk",
        "probe_id",
        "message_result",
        "callback_elapsed_us",
        "shutdown_requested",
        "primary_down",
        "owned_input",
        "pending_deadlines",
        "vk",
        "down",
        "injected",
        "primary",
        "elapsed_us",
        "key",
        "focused",
        "left_px",
        "top_px",
        "right_px",
        "bottom_px",
        "enabled",
        "selected",
        "index",
        "client_width_px",
        "client_height_px",
        "allocated_rect_px",
        "clip_rect_px",
        "requested_size_px",
        "custom_action_index",
        "rank",
        "catalog_len",
        "count",
        "widget_changed",
        "model_changed",
        "input_matches_model",
        "draft_dirty",
        "request_kind",
        "request_id",
        "hwnd",
        "visible",
        "minimized",
        "terminal",
        "correlation",
        "stage",
    ];

    let tokens = split_trace_tokens(line);
    let mut event = None;
    let mut fields = Vec::new();
    for token in tokens {
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        if key == "trace_event" {
            let value = trace_value(value)?;
            if !EVENT_NAMES.contains(&value.as_str()) {
                return None;
            }
            event = Some(value);
            continue;
        }
        if SAFE_FIELDS.contains(&key) {
            let value = trace_value(value)?;
            if !safe_trace_field_value(key, &value) {
                continue;
            }
            fields.push(format!("{key}={value}"));
        }
    }
    let event = event?;
    let mut sanitized = format!("trace_event={event}");
    for field in fields {
        sanitized.push(' ');
        sanitized.push_str(&field);
    }
    Some(sanitized)
}

fn safe_trace_field_value(key: &str, value: &str) -> bool {
    match key {
        "focused" => matches!(value, "true" | "false"),
        "command" => matches!(
            value,
            "position" | "size" | "Show" | "Minimize" | "Focus" | "ParkingBoundary"
        ),
        "source" => matches!(value, "ToggleBatch" | "LegacyTrigger" | "Queued"),
        "requested_x" | "requested_y" | "requested_width" | "requested_height" => {
            value.parse::<i32>().is_ok()
        }
        "menu_cell_ids_digest" => value.parse::<u64>().is_ok(),
        "cell_ring_index" | "cell_slot_index" => value.parse::<i32>().is_ok(),
        _ => true,
    }
}

fn split_trace_tokens(line: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut token_start = None;
    let mut in_quotes = false;
    for (index, character) in line.char_indices() {
        if character == '"' {
            in_quotes = !in_quotes;
        } else if character.is_whitespace() && !in_quotes {
            if let Some(start) = token_start.take() {
                tokens.push(&line[start..index]);
            }
            continue;
        }
        token_start.get_or_insert(index);
    }
    if let Some(start) = token_start {
        tokens.push(&line[start..]);
    }
    tokens
}

fn trace_value(raw: &str) -> Option<String> {
    let value = raw.trim_matches('"');
    if value.is_empty()
        || value.len() > 96
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-.(),:?[]".contains(&byte))
    {
        return None;
    }
    Some(value.to_string())
}

fn copy_bounded_log_tail(source: &Path, destination: &Path) -> Result<(), String> {
    let bytes = read_bounded_log_tail(source)?;
    fs::write(destination, bytes)
        .map_err(|error| format!("write private bounded log tail: {error}"))
}

fn read_bounded_log_tail(source: &Path) -> Result<Vec<u8>, String> {
    let mut input = File::open(source).map_err(|error| format!("open candidate log: {error}"))?;
    let length = input
        .metadata()
        .map_err(|error| format!("inspect candidate log: {error}"))?
        .len();
    let start = length.saturating_sub(MAX_PRIVATE_LOG_BYTES as u64);
    input
        .seek(SeekFrom::Start(start))
        .map_err(|error| format!("seek bounded candidate log tail: {error}"))?;
    let mut bytes = Vec::with_capacity(length.saturating_sub(start) as usize);
    input
        .take(MAX_PRIVATE_LOG_BYTES as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read bounded candidate log tail: {error}"))?;
    if bytes.is_empty() {
        return Err("candidate log is empty".into());
    }
    Ok(bytes)
}

fn save_uia_snapshot(case_id: &str, role: &str, uia: &UiAutomation, hwnd: HWND, output: &Path) {
    let path = output.join(format!("case-{case_id}-uia-{role}.txt"));
    let content = uia
        .describe_tree(hwnd)
        .unwrap_or_else(|error| format!("UIA tree diagnostic failed: {error}"));
    let _ = fs::write(path, content);
}

fn save_launch_failure_inventory(failure: &NativeLaunchFailure, output: &Path) -> Option<PathBuf> {
    let destination = output.join("candidate-startup-windows.json");
    let record = WindowInventory {
        runner_process_id: std::process::id(),
        child_process_id: failure.process_id,
        windows: failure.windows.iter().map(WindowRecord::from).collect(),
    };
    let bytes = serde_json::to_vec_pretty(&record).ok()?;
    fs::write(&destination, bytes).ok()?;
    Some(destination)
}

fn capture_diagnostic_screenshot(child: Option<&NativeChild>, path: &Path) -> Result<(), String> {
    let child =
        child.ok_or_else(|| "diagnostic screenshot requires a child-owned HWND".to_string())?;
    let displays = native_display_bounds()?;
    let candidate = child
        .windows()
        .into_iter()
        .find(|window| {
            window.visible
                && !window.minimized
                && window.process_id == child.process_id()
                && intersects_display_bounds(window.bounds, &displays)
        })
        .ok_or_else(|| "no visible child-owned HWND intersects a physical display".to_string())?;
    let image = {
        let window = candidate;
        let center_x = window.bounds[0] + (window.bounds[2] - window.bounds[0]) / 2;
        let center_y = window.bounds[1] + (window.bounds[3] - window.bounds[1]) / 2;
        let screen = screenshots::Screen::from_point(center_x, center_y)
            .map_err(|error| format!("select screenshot monitor: {error}"))?;
        let info = screen.display_info;
        let left = (window.bounds[0] - info.x).max(0);
        let top = (window.bounds[1] - info.y).max(0);
        let right = (window.bounds[2] - info.x).min(info.width as i32);
        let bottom = (window.bounds[3] - info.y).min(info.height as i32);
        if right <= left || bottom <= top {
            return Err(
                "child-owned diagnostic HWND crop does not intersect its physical display".into(),
            );
        }
        screen
            .capture_area(left, top, (right - left) as u32, (bottom - top) as u32)
            .map_err(|error| format!("capture diagnostic window crop: {error}"))?
    };
    image
        .save(path)
        .map_err(|error| format!("save diagnostic screenshot: {error}"))
}

#[derive(Serialize)]
struct WindowInventory {
    runner_process_id: u32,
    child_process_id: Option<u32>,
    windows: Vec<WindowRecord>,
}

#[derive(Serialize)]
struct WindowRecord {
    hwnd: u64,
    process_id: u32,
    role: &'static str,
    visible: bool,
    minimized: bool,
    bounds: [i32; 4],
}

impl From<&WindowSnapshot> for WindowRecord {
    fn from(window: &WindowSnapshot) -> Self {
        Self {
            hwnd: hwnd_id(window.hwnd),
            process_id: window.process_id,
            role: match window.role {
                WindowRole::Root => "root",
                WindowRole::Designer => "designer",
                WindowRole::OtherChild => "other_child",
            },
            visible: window.visible,
            minimized: window.minimized,
            bounds: window.bounds,
        }
    }
}

#[derive(Clone, Debug)]
struct CaseFailure {
    stage: FailureStage,
    message: String,
    input_contamination_group: Option<u32>,
}

impl CaseFailure {
    fn new(stage: FailureStage, message: String) -> Self {
        Self {
            stage,
            message,
            input_contamination_group: None,
        }
    }

    fn input_contamination(group_id: u32, message: String) -> Self {
        Self {
            stage: FailureStage::InputInjection,
            message,
            input_contamination_group: (group_id != 0).then_some(group_id),
        }
    }
}

impl std::fmt::Display for CaseFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.stage, self.message)
    }
}

fn expected(id: &str) -> &'static str {
    match id {
        "H0" => "ROOT focused F11 tap parks ROOT offscreen; exactly one short tap; no radial open",
        "H1" => "hidden ROOT is shown from known runner-owned focus",
        "H2" => "runner-owned other focus toggles visible ROOT in both directions",
        "H3" => "Designer-focused F11 toggles only ROOT; Designer remains queryable",
        "H4" => "threshold hold opens runtime radial while ROOT stays visible",
        "H5" => "release after hold does not co-fire grid/ROOT visibility",
        "H6" => "second threshold hold closes runtime radial while ROOT stays visible",
        "H7" => {
            "three consecutive configured hotkey taps produce exactly three visibility toggles and leave ROOT hidden"
        }
        "H8" => {
            "four consecutive configured hotkey taps produce exactly four visibility toggles and leave ROOT visible"
        }
        "H01" => {
            "one configured short tap from runner-owned focus wakes hidden ROOT on a physical display and focuses ROOT; no runtime radial opens"
        }
        "H02" => {
            "one configured short tap from focused ROOT parks it off physical displays without a later restore or radial open"
        }
        "H04" => {
            "uninterrupted 1, 2, 5, 10, and 25 tap bursts from hidden and visible starts produce 86 one-to-one visibility decisions; three readable taps hide focused ROOT"
        }
        "H06" => {
            "from hidden ROOT, a short tap dismisses the active runtime radial, shows ROOT, and selects or dispatches no cell"
        }
        "H07" => {
            "a short tap dismisses a hovered executable runtime cell without dispatch, then later short taps show and hide ROOT"
        }
        "H08" => {
            "a real runtime preparation hold is canceled by a short tap before Ready; late preparation is rejected and cannot reopen radial"
        }
        "H09" => {
            "holds from hidden and visible ROOT states open runtime radial; release does not toggle ROOT; a second hold closes radial"
        }
        "H10" => {
            "a hold closes radial while an executable cell is hovered, preserving ROOT visibility with no selection, dispatch, or late reopen"
        }
        "H11" => {
            "five taps from visible and hidden starts preserve the same dirty Designer session, draft, and foreground without save or discard"
        }
        "H12" => {
            "runtime hold and tap dismiss only runtime radial while dirty Designer and native preview surfaces, session, and draft survive"
        }
        "H16" => {
            "a direct-trigger binding opens runtime radial; native and supported legacy-route launcher taps each dismiss it and toggle ROOT without selection, while the binding remains usable"
        }
        "H17" => {
            "main and isolated opposite-hotkey profiles each exercise their configured hotkey with mouse gestures enabled and clean teardown"
        }
        "H18" => {
            "a hold from parked ROOT opens radial without pointer movement; a short tap wakes ROOT and dismisses radial without dispatch"
        }
        "D0" => "production Edit Radial Menus entry opens one ready child-owned Designer",
        "D1" => {
            "validated native client click on production Tree semantic target reaches accepted widget and changes state"
        }
        "D2" => {
            "native TextEdit input changes and restores the production draft, then checked Tab moves focus to the next control"
        }
        "D4" => "production radial skins command selects the same Designer Skins semantic target",
        "D5" => "checked close closes Designer while ROOT and candidate remain alive",
        "A0" => "New Menu creates one stable menu and selects it through the production toolbar",
        "A1" => {
            "Menus mode creates a menu and Add Ring publishes a ready proposal without changing committed geometry"
        }
        "G0" => {
            "explicitly applying the prepared outer-ring proposal commits the candidate and preserves existing cell IDs"
        }
        "A2" => {
            "Ring selector and Slots grow apply a prepared candidate while preserving prior cell IDs"
        }
        "G1" => {
            "populated shrink cancels without loss, then moves cells to overflow and applies safely"
        }
        "G2" => {
            "compact Designer fits its controls and canvas; ROOT remains stable on a physical display after close"
        }
        "A3" => {
            "a searched harmless custom action beyond the unfiltered first 50 rows is assigned through the real popup"
        }
        "A4" => "dirty popup Apply and open preserves the selected authored cell in Inspector",
        "A5" => "a harmless skin style edit is made through the production Skins editor",
        "A6" => {
            "Save, close, and reopen persist stable radial identities, action binding, and style"
        }
        "A7" => "Undo and Redo restore the same authored edit through the production toolbar",
        "A8" => "Design and safe desktop preview do not dispatch a leaf action or append history",
        "D3" => "ROOT can hide and show while the open Designer remains interactive and serviced",
        "D6" => "dirty close Keep Editing retains the draft before explicit terminal discard",
        "D7" => "close cancels a pending disposable preview preparation with no late reopen",
        "R0" => {
            "valid bounded JSON and text reports identify source, profile, hashes, elapsed time, and evidence"
        }
        "R1" | "CP_R1" => {
            "controlled harness failure writes bounded privacy-safe trace and owned screenshot evidence"
        }
        "R2" => "child process, HWNDs, temp profile, and native input state are cleaned up",
        _ => "candidate exits normally through production close path",
    }
}

fn tap_trace_complete(events: &[String], taps: usize, visible: bool) -> bool {
    let hooks = events
        .iter()
        .filter(|line| {
            line.contains("trace_event=\"hook_primary\"")
                && line.contains("provenance=ExternalInjected")
                && (line.contains("transition=Press") || line.contains("transition=Release"))
        })
        .count();
    let configured = events
        .iter()
        .filter(|line| {
            line.contains("trace_event=\"configured_primary\"")
                && line.contains("provenance=ExternalInjected")
                && line.contains("modifiers_match=true")
                && (line.contains("transition=Press") || line.contains("transition=Release"))
        })
        .count();
    let short = events
        .iter()
        .filter(|line| line.contains("trace_event=\"short_tap\""))
        .count();
    let visibility = events
        .iter()
        .filter(|line| {
            line.contains("trace_event=\"desired_visibility\"")
                && line.contains(if visible {
                    "visible=true"
                } else {
                    "visible=false"
                })
        })
        .count();
    hooks >= taps * 2 && configured >= taps * 2 && short >= taps && visibility >= taps
}

fn tap_trace_failure_stage(events: &[String], visible: bool) -> FailureStage {
    let hook_press = has_trace(
        events,
        "hook_primary",
        &["transition=Press", "provenance=ExternalInjected"],
    );
    let hook_release = has_trace(
        events,
        "hook_primary",
        &["transition=Release", "provenance=ExternalInjected"],
    );
    let configured_press = has_trace(
        events,
        "configured_primary",
        &[
            "transition=Press",
            "provenance=ExternalInjected",
            "modifiers_match=true",
        ],
    );
    let configured_release = has_trace(
        events,
        "configured_primary",
        &[
            "transition=Release",
            "provenance=ExternalInjected",
            "modifiers_match=true",
        ],
    );
    if !(hook_press && hook_release && configured_press && configured_release) {
        FailureStage::HookAdmission
    } else if !has_trace(events, "short_tap", &[]) {
        FailureStage::GestureDecision
    } else if !events.iter().any(|line| {
        line.contains("trace_event=\"desired_visibility\"")
            && line.contains(if visible {
                "visible=true"
            } else {
                "visible=false"
            })
    }) {
        FailureStage::RootCommand
    } else {
        FailureStage::GestureDecision
    }
}

fn input_trace_summary(events: &[String]) -> String {
    let lines = events
        .iter()
        .filter(|line| {
            [
                "trace_event=\"hook_primary\"",
                "trace_event=\"configured_primary\"",
                "trace_event=\"short_tap\"",
                "trace_event=\"desired_visibility\"",
            ]
            .iter()
            .any(|marker| line.contains(marker))
        })
        .take(4)
        .cloned()
        .collect::<Vec<_>>();
    if lines.is_empty() {
        "none".to_string()
    } else {
        lines.join(" | ")
    }
}

fn has_trace(events: &[String], event: &str, fields: &[&str]) -> bool {
    events.iter().any(|line| {
        line.contains(&format!("trace_event=\"{event}\""))
            && fields.iter().all(|field| line.contains(field))
    })
}

fn parse_authoring_request_identity(
    line: &str,
    session_id: u64,
    request_kind: &str,
) -> Option<AuthoringRequestIdentity> {
    if !line.contains("trace_event=\"authoring\"")
        || trace_field_value(line, "edge")? != "RequestSent"
        || trace_field_value(line, "request_kind")? != request_kind
        || trace_field_value(line, "session_id")?.parse::<u64>().ok()? != session_id
        || trace_field_value(line, "terminal")? != "false"
    {
        return None;
    }
    Some(AuthoringRequestIdentity {
        request_id: trace_field_value(line, "request_id")?.parse().ok()?,
        generation: trace_field_value(line, "generation")?.parse().ok()?,
        session_id,
    })
}

fn wait_for_authoring_request_generation(
    trace_path: &Path,
    cursor: usize,
    session_id: u64,
    request_kind: &str,
    generation: u64,
    timeout: Duration,
) -> Option<AuthoringRequestIdentity> {
    let events = wait_trace(trace_path, cursor, timeout, |events| {
        events.iter().any(|line| {
            parse_authoring_request_identity(line, session_id, request_kind)
                .is_some_and(|identity| identity.generation == generation)
        })
    });
    events.iter().find_map(|line| {
        parse_authoring_request_identity(line, session_id, request_kind)
            .filter(|identity| identity.generation == generation)
    })
}

fn authoring_edge_matches(
    line: &str,
    identity: AuthoringRequestIdentity,
    request_kind: &str,
    edge: &str,
) -> bool {
    line.contains("trace_event=\"authoring\"")
        && trace_field_value(line, "edge") == Some(edge)
        && trace_field_value(line, "request_kind") == Some(request_kind)
        && trace_field_value(line, "request_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.request_id)
        && trace_field_value(line, "generation").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.generation)
        && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.session_id)
}

fn wait_for_authoring_edge(
    trace_path: &Path,
    cursor: usize,
    identity: AuthoringRequestIdentity,
    request_kind: &str,
    edge: &str,
    timeout: Duration,
) -> Option<String> {
    wait_trace(trace_path, cursor, timeout, |events| {
        events
            .iter()
            .any(|line| authoring_edge_matches(line, identity, request_kind, edge))
    })
    .into_iter()
    .find(|line| authoring_edge_matches(line, identity, request_kind, edge))
}

fn acceptance_prepare_gate_matches(
    line: &str,
    identity: AuthoringRequestIdentity,
    edge: &str,
) -> bool {
    line.contains("trace_event=\"acceptance_prepare_gate\"")
        && trace_field_value(line, "edge") == Some(edge)
        && trace_field_value(line, "request_kind") == Some("PrepareEmbeddedPreview")
        && trace_field_value(line, "request_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.request_id)
        && trace_field_value(line, "generation").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.generation)
        && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.session_id)
}

fn runtime_preparation_matches(
    line: &str,
    invocation_id: u64,
    generation: u64,
    edge: &str,
) -> bool {
    line.contains("trace_event=\"runtime_preparation\"")
        && trace_field_value(line, "edge") == Some(edge)
        && trace_field_value(line, "invocation_id").and_then(|value| value.parse::<u64>().ok())
            == Some(invocation_id)
        && trace_field_value(line, "generation").and_then(|value| value.parse::<u64>().ok())
            == Some(generation)
}

fn disposable_cancel_matches(
    line: &str,
    identity: AuthoringRequestIdentity,
    request_kind: &str,
) -> bool {
    line.contains("trace_event=\"disposable_request_cancelled\"")
        && trace_field_value(line, "request_kind") == Some(request_kind)
        && trace_field_value(line, "request_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.request_id)
        && trace_field_value(line, "generation").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.generation)
        && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.session_id)
}

fn designer_close_matches(line: &str, session_id: u64, close_prompt: bool, dirty: bool) -> bool {
    line.contains("trace_event=\"designer_close\"")
        && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
            == Some(session_id)
        && trace_field_value(line, "close_prompt")
            == Some(if close_prompt { "true" } else { "false" })
        && trace_field_value(line, "dirty") == Some(if dirty { "true" } else { "false" })
}

fn verify_designer_stays_closed_for(
    child: &mut NativeChild,
    duration: Duration,
) -> Result<String, String> {
    let started = Instant::now();
    let mut samples = 0usize;
    while started.elapsed() < duration {
        if child.designer().is_some() {
            return Err(format!(
                "Designer reappeared during the bounded closed interval after {} ms",
                started.elapsed().as_millis()
            ));
        }
        if child
            .try_wait()
            .map_err(|error| format!("poll candidate during close stability interval: {error}"))?
            .is_some()
        {
            return Err(
                "candidate process exited during the Designer close stability interval".into(),
            );
        }
        samples += 1;
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(format!(
        "{} ms with {samples} Designer-absent and candidate-alive samples",
        started.elapsed().as_millis()
    ))
}

fn authoring_reply_matches(
    line: &str,
    identity: AuthoringRequestIdentity,
    request_kind: &str,
) -> bool {
    line.contains("trace_event=\"authoring\"")
        && trace_field_value(line, "edge") == Some("ReplyAccepted")
        && trace_field_value(line, "request_kind") == Some(request_kind)
        && trace_field_value(line, "request_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.request_id)
        && trace_field_value(line, "generation").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.generation)
        && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.session_id)
        && trace_field_value(line, "terminal") == Some("true")
}

fn wait_for_terminal_authoring_request(
    trace_path: &Path,
    cursor: usize,
    session_id: u64,
    request_kind: &str,
    timeout: Duration,
) -> Option<AuthoringReplyEvidence> {
    let events = wait_trace(trace_path, cursor, timeout, |events| {
        let Some(identity) = events
            .iter()
            .find_map(|line| parse_authoring_request_identity(line, session_id, request_kind))
        else {
            return false;
        };
        events
            .iter()
            .any(|line| authoring_reply_matches(line, identity, request_kind))
    });
    let identity = events
        .iter()
        .find_map(|line| parse_authoring_request_identity(line, session_id, request_kind))?;
    if !events
        .iter()
        .any(|line| authoring_reply_matches(line, identity, request_kind))
    {
        return None;
    }
    Some(AuthoringReplyEvidence { identity })
}

fn wait_for_authoring_control_clicked(
    trace_path: &Path,
    cursor: usize,
    session_id: u64,
    target: AuthoringControlTarget,
    timeout: Duration,
) -> Option<String> {
    let target = format!("{target:?}");
    wait_trace(trace_path, cursor, timeout, |events| {
        events.iter().any(|line| {
            line.contains("trace_event=\"designer_authoring_control\"")
                && trace_field_value(line, "target") == Some(target.as_str())
                && trace_field_value(line, "clicked") == Some("true")
                && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
                    == Some(session_id)
        })
    })
    .into_iter()
    .find(|line| {
        line.contains("trace_event=\"designer_authoring_control\"")
            && trace_field_value(line, "target") == Some(target.as_str())
            && trace_field_value(line, "clicked") == Some("true")
            && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
                == Some(session_id)
    })
}

fn wait_for_authoring_control_focused_after(
    trace_path: &Path,
    first_line: usize,
    session_id: u64,
    generation: u64,
    target: AuthoringControlTarget,
    role: AuthoringControlRole,
    timeout: Duration,
) -> Result<AuthoringControlSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(control) =
            find_authoring_control_after(trace_path, first_line, session_id, target, None, role)?
            && control.generation == generation
            && control.enabled
            && control.focused
        {
            return Ok(control);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer did not focus enabled target {target:?} with role {role:?} in session {session_id} generation {generation} after trace line {first_line}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn tab_and_activate_authoring_control(
    child: &NativeChild,
    window: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
    generation: u64,
    target: AuthoringControlTarget,
) -> Result<(AuthoringControlSnapshot, usize, usize), String> {
    let mut tab_events = 0;
    let mut focused = None;
    for _ in 0..MAX_KEYBOARD_FOCUS_STEPS {
        let cursor = trace_lines(trace_path).len();
        tab_events += send_tab(child, window)?;
        match wait_for_authoring_control_focused_after(
            trace_path,
            cursor,
            session_id,
            generation,
            target,
            AuthoringControlRole::Button,
            Duration::from_millis(150),
        ) {
            Ok(control) => {
                focused = Some(control);
                break;
            }
            Err(error) if error.contains("did not focus enabled target") => {}
            Err(error) => return Err(error),
        }
    }
    let control = focused.ok_or_else(|| {
        format!(
            "checked Tab did not focus {target:?} after at most {MAX_KEYBOARD_FOCUS_STEPS} steps"
        )
    })?;
    let click_cursor = trace_lines(trace_path).len();
    let enter_events = send_enter_current(child, window)?;
    wait_for_authoring_control_clicked(trace_path, click_cursor, session_id, target, TRACE_TIMEOUT)
        .ok_or_else(|| format!("checked Enter did not activate focused {target:?}"))?;
    Ok((control, tab_events, enter_events))
}

fn visible_radial_host_windows(child: &NativeChild) -> std::collections::BTreeSet<u64> {
    child
        .windows()
        .into_iter()
        .filter(|window| window.visible && window.class_name == "MultiLauncherRadialHost")
        .map(|window| hwnd_id(window.hwnd))
        .collect()
}

fn visible_radial_host_snapshots(
    child: &NativeChild,
    baseline: &std::collections::BTreeSet<u64>,
    runtime_hold_attempted: bool,
) -> Vec<WindowSnapshot> {
    if !runtime_hold_attempted {
        return Vec::new();
    }
    child
        .windows()
        .into_iter()
        .filter(|window| {
            window.process_id == child.process_id()
                && window.visible
                && !window.minimized
                && window.class_name == "MultiLauncherRadialHost"
                && h12_surface_belongs_to_runtime(
                    runtime_hold_attempted,
                    hwnd_id(window.hwnd),
                    baseline,
                )
        })
        .collect()
}

fn h12_surface_belongs_to_runtime(
    runtime_hold_attempted: bool,
    hwnd: u64,
    known_preview_surfaces: &std::collections::BTreeSet<u64>,
) -> bool {
    runtime_hold_attempted && hwnd != 0 && !known_preview_surfaces.contains(&hwnd)
}

fn attempt_cleanup_step(
    errors: &mut Vec<String>,
    label: &str,
    action: impl FnOnce() -> Result<(), CaseFailure>,
) {
    if let Err(error) = action() {
        errors.push(format!("{label}: {}", error.message));
    }
}

#[derive(Clone, Copy, Debug)]
struct DesignerSemanticTargetState {
    bounds: [i32; 4],
    selected: bool,
    focused: bool,
    session_id: u64,
    generation: u64,
}

fn parse_designer_semantic_target(
    line: &str,
    target: DesignerSemanticTarget,
) -> Option<DesignerSemanticTargetState> {
    let name = format!("target={target:?}");
    let role = match target {
        DesignerSemanticTarget::Menus
        | DesignerSemanticTarget::Skins
        | DesignerSemanticTarget::Tree
        | DesignerSemanticTarget::Inspector => "SelectableLabel",
        DesignerSemanticTarget::DefaultMenu => "Button",
        DesignerSemanticTarget::MenuName => "TextEdit",
        DesignerSemanticTarget::MenuDefaultSkin => "ComboBox",
    };
    if !line.contains("trace_event=\"designer_semantic_target\"")
        || !line.contains(&name)
        || !line.contains(&format!("role=\"{role}\""))
        || !line.contains("viewport=Deferred")
    {
        return None;
    }
    let field = |name: &str| {
        line.split_whitespace()
            .find_map(|part| part.strip_prefix(&format!("{name}=")))
    };
    Some(DesignerSemanticTargetState {
        bounds: [
            field("left_px")?.parse().ok()?,
            field("top_px")?.parse().ok()?,
            field("right_px")?.parse().ok()?,
            field("bottom_px")?.parse().ok()?,
        ],
        selected: field("selected")?.parse().ok()?,
        focused: field("focused")?.parse().ok()?,
        session_id: field("session_id")?.parse().ok()?,
        generation: field("generation")?.parse().ok()?,
    })
}

fn checked_designer_toggle_transition(
    events: &[String],
    target: DesignerSemanticTarget,
    baseline: DesignerSemanticTargetState,
    selected: bool,
) -> Option<DesignerSemanticTargetState> {
    let current = |line: &str, event: &str| {
        line.contains(&format!("trace_event=\"{event}\""))
            && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
                == Some(baseline.session_id)
            && trace_field_value(line, "generation").and_then(|value| value.parse::<u64>().ok())
                == Some(baseline.generation)
    };
    let has_pointer_edge = |edge: &str| {
        events
            .iter()
            .any(|line| current(line, "designer_pointer") && line.contains(edge))
    };
    let body_accepted = events.iter().any(|line| {
        current(line, "designer_body") && trace_field_value(line, "state") == Some("Enabled")
    });
    let widget_name = format!("{target:?}");
    let widget_accepted = events.iter().any(|line| {
        current(line, "designer_widget")
            && trace_field_value(line, "category") == Some(widget_name.as_str())
            && trace_field_value(line, "response") == Some("Accepted")
    });
    let state = events
        .iter()
        .filter_map(|line| parse_designer_semantic_target(line, target))
        .find(|state| {
            state.session_id == baseline.session_id
                && state.generation == baseline.generation
                && state.selected == selected
        })?;
    (has_pointer_edge("pointer_down=true")
        && has_pointer_edge("pointer_up=true")
        && body_accepted
        && widget_accepted)
        .then_some(state)
}

fn wait_for_designer_semantic_target(
    trace_path: &Path,
    target: DesignerSemanticTarget,
    timeout: Duration,
    predicate: impl Fn(&DesignerSemanticTargetState) -> bool,
) -> Option<DesignerSemanticTargetState> {
    let deadline = Instant::now() + timeout;
    loop {
        let state = latest_designer_semantic_target(&trace_lines(trace_path), target, None);
        if state.as_ref().is_some_and(&predicate) {
            return state;
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_designer_semantic_target_in_session(
    trace_path: &Path,
    target: DesignerSemanticTarget,
    session_id: u64,
    timeout: Duration,
    predicate: impl Fn(&DesignerSemanticTargetState) -> bool,
) -> Option<DesignerSemanticTargetState> {
    let deadline = Instant::now() + timeout;
    loop {
        let state =
            latest_designer_semantic_target(&trace_lines(trace_path), target, Some(session_id));
        if state.as_ref().is_some_and(&predicate) {
            return state;
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn latest_designer_semantic_target(
    lines: &[String],
    target: DesignerSemanticTarget,
    session_id: Option<u64>,
) -> Option<DesignerSemanticTargetState> {
    lines
        .iter()
        .rev()
        .filter_map(|line| parse_designer_semantic_target(line, target))
        .find(|state| session_id.is_none_or(|session_id| state.session_id == session_id))
}

fn trace_lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("trace_event=\""))
        .map(str::to_owned)
        .collect()
}

fn root_menu_state_from_trace(lines: &[String]) -> Option<RootMenuState> {
    if lines
        .iter()
        .any(|line| line.contains("trace_event=\"budget_exhausted\""))
        || !lines
            .iter()
            .any(|line| line.contains("trace_event=\"trace_ready\""))
    {
        return None;
    }

    let mut state = RootMenuState::default();
    for line in lines
        .iter()
        .filter(|line| line.contains("trace_event=\"root_menu_interaction\""))
    {
        let open = match trace_field_value(line, "open")? {
            "true" => true,
            "false" => false,
            _ => return None,
        };
        match trace_field_value(line, "menu")? {
            "File" => state.file_open = open,
            "Apps" => state.apps_open = open,
            _ => return None,
        }
    }
    let mut file_body_entered = None;
    let mut apps_body_entered = None;
    for line in lines
        .iter()
        .filter(|line| line.contains("trace_event=\"root_menu_body\""))
    {
        let entered = match trace_field_value(line, "entered")? {
            "true" => true,
            "false" => false,
            _ => return None,
        };
        match trace_field_value(line, "menu")? {
            "File" => file_body_entered = Some(entered),
            "Apps" => apps_body_entered = Some(entered),
            _ => return None,
        }
    }
    if let Some(entered) = file_body_entered {
        state.file_open = entered;
    }
    if let Some(entered) = apps_body_entered {
        state.apps_open = entered;
    }
    Some(state)
}

fn trace_field_value<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    line.split_ascii_whitespace()
        .find_map(|part| part.strip_prefix(&format!("{field}=")))
        .map(|value| value.trim_matches('"'))
}

fn trace_line_is_executable_hover(line: &str) -> bool {
    line.contains("trace_event=\"runtime_radial_hover\"")
        && trace_field_value(line, "role") == Some("Action")
        && trace_field_value(line, "executable") == Some("true")
}

fn hook_service_thread_id(path: &Path) -> Option<u32> {
    trace_lines(path).into_iter().find_map(|line| {
        let event = line.split("trace_event=\"").nth(1)?.split('\"').next()?;
        if event != "hook_service_ready" {
            return None;
        }
        line.split("thread_id=")
            .nth(1)?
            .split_ascii_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

fn establish_hotkey_trace_fence(
    child: &NativeChild,
    trace_path: &Path,
    expected_visible: bool,
) -> Result<HotkeyTraceFence, CaseFailure> {
    let hook_thread_id = wait_until(Duration::from_secs(1), || {
        hook_service_thread_id(trace_path).is_some()
    })
    .then(|| hook_service_thread_id(trace_path))
    .flatten()
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::HookAdmission,
            "production hook service did not publish its thread identity before the trace fence"
                .into(),
        )
    })?;
    let probe_id = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
    let probe_cursor = trace_lines(trace_path).len();
    post_validated_hook_pump_probe(hook_thread_id, child.process_id(), probe_id)
        .map_err(|error| CaseFailure::new(FailureStage::HookAdmission, error))?;
    let probe_events = wait_trace(
        trace_path,
        probe_cursor,
        Duration::from_millis(750),
        |events| {
            has_trace(
                events,
                "hook_pump_probe",
                &[&format!("probe_id={probe_id}")],
            )
        },
    );
    if !has_trace(
        &probe_events,
        "hook_pump_probe",
        &[&format!("probe_id={probe_id}")],
    ) {
        return Err(CaseFailure::new(
            FailureStage::HookAdmission,
            format!(
                "production hook service thread {hook_thread_id} did not acknowledge trace-fence probe {probe_id}"
            ),
        ));
    }

    let deadline = Instant::now() + HOTKEY_TRACE_DRAIN_TIMEOUT;
    let mut signature = hotkey_trace_activity_signature(&trace_lines(trace_path));
    let mut quiet_since = Instant::now();
    loop {
        if Instant::now() >= deadline {
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "production hotkey and visibility trace did not settle within {:?} after pump probe {probe_id}",
                    HOTKEY_TRACE_DRAIN_TIMEOUT
                ),
            ));
        }
        std::thread::sleep(WINDOW_POLL);
        let next_signature = hotkey_trace_activity_signature(&trace_lines(trace_path));
        if next_signature != signature {
            signature = next_signature;
            quiet_since = Instant::now();
            continue;
        }
        if quiet_since.elapsed() >= HOTKEY_TRACE_QUIET_WINDOW {
            break;
        }
    }
    if !wait_root_visibility(child, expected_visible, Duration::ZERO) {
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            format!(
                "ROOT did not remain in expected visible={expected_visible} state while draining setup trace before probe {probe_id}"
            ),
        ));
    }
    let events = trace_lines(trace_path);
    Ok(hotkey_trace_fence_from_events(&events, probe_id))
}

fn hotkey_trace_fence_from_events(events: &[String], probe_id: u64) -> HotkeyTraceFence {
    let baseline_invocation_id = events.iter().rev().find_map(|line| {
        line.contains("trace_event=\"short_tap\"")
            .then(|| trace_field_value(line, "invocation_id"))
            .flatten()
            .and_then(|value| value.parse::<u64>().ok())
    });
    HotkeyTraceFence {
        cursor: events.len(),
        probe_id,
        baseline_invocation_id,
        baseline_visibility_revision: latest_desired_visibility_revision(events),
    }
}

fn hotkey_trace_activity_signature(events: &[String]) -> Vec<String> {
    events
        .iter()
        .filter(|line| {
            line.contains("trace_event=\"hook_primary\"")
                || line.contains("trace_event=\"configured_primary\"")
                || line.contains("trace_event=\"short_tap\"")
                || line.contains("trace_event=\"desired_visibility\"")
        })
        .cloned()
        .collect()
}

fn hotkey_trace_edge_count(events: &[String], event_name: &str) -> usize {
    let event_marker = format!("trace_event=\"{event_name}\"");
    events
        .iter()
        .filter(|line| line.contains(&event_marker))
        .count()
}

fn validate_hotkey_production_admission(events: &[String], taps: usize) -> Result<(), String> {
    let expected_edges = taps.saturating_mul(2);
    let hook_edges = hotkey_trace_edge_count(events, "hook_primary");
    if hook_edges != expected_edges {
        return Err(format!(
            "expected {expected_edges} production hook_primary edges after runner injection, observed {hook_edges}"
        ));
    }
    let configured_edges = hotkey_trace_edge_count(events, "configured_primary");
    if configured_edges != expected_edges {
        return Err(format!(
            "expected {expected_edges} configured_primary edges after runner injection, observed {configured_edges}"
        ));
    }
    Ok(())
}

fn wait_trace<F>(path: &Path, cursor: usize, timeout: Duration, mut predicate: F) -> Vec<String>
where
    F: FnMut(&[String]) -> bool,
{
    let deadline = Instant::now() + timeout;
    loop {
        let events = trace_lines(path)
            .into_iter()
            .skip(cursor)
            .collect::<Vec<_>>();
        if predicate(&events) || Instant::now() >= deadline {
            return events;
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_latest_root_restore(path: &Path, timeout: Duration) -> Option<String> {
    let mut request_id = None;
    let completed = wait_until(timeout, || {
        let events = trace_lines(path);
        let Some(request) = events.iter().rev().find(|line| {
            line.contains("trace_event=\"native_activation\"")
                && line.contains("edge=RestoreRequested")
        }) else {
            return false;
        };
        let Some(id) = request
            .split("request_id=")
            .nth(1)
            .and_then(|value| value.split_ascii_whitespace().next())
        else {
            return false;
        };
        request_id = Some(id.to_owned());
        events.iter().any(|line| {
            line.contains("trace_event=\"native_activation\"")
                && line.contains("edge=RestoreCompleted")
                && line.contains("terminal=true")
                && line.contains(&format!("request_id={id}"))
        })
    });
    if completed { request_id } else { None }
}

fn root_restore_terminal_after(events: &[String], cursor: usize) -> Option<Result<String, String>> {
    let request = events.iter().skip(cursor).rev().find(|line| {
        line.contains("trace_event=\"native_activation\"") && line.contains("edge=RestoreRequested")
    })?;
    let request_id = request
        .split("request_id=")
        .nth(1)?
        .split_ascii_whitespace()
        .next()?;
    let terminal = events.iter().skip(cursor).find(|line| {
        line.contains("trace_event=\"native_activation\"")
            && line.contains("terminal=true")
            && line.contains(&format!("request_id={request_id}"))
            && (line.contains("edge=RestoreCompleted") || line.contains("edge=RestoreFailed"))
    })?;
    if terminal.contains("edge=RestoreCompleted") {
        Some(Ok(request_id.to_owned()))
    } else {
        Some(Err(format!(
            "native ROOT restore request {request_id} ended with RestoreFailed"
        )))
    }
}

fn visible_burst_trace_settled(
    events: &[String],
    cursor: usize,
    policy: VisibleBurstSettlePolicy,
) -> Result<Option<String>, String> {
    match policy {
        VisibleBurstSettlePolicy::ActivateRoot => {
            root_restore_terminal_after(events, cursor).transpose()
        }
        VisibleBurstSettlePolicy::PreserveForeground { .. } => {
            validate_no_native_root_activation_after(events, cursor)?;
            Ok(Some("PreserveForeground".into()))
        }
    }
}

fn wait_for_root_restore_after(
    path: &Path,
    cursor: usize,
    timeout: Duration,
) -> Result<String, String> {
    wait_for_root_restore_after_events(timeout, || {
        trace_lines(path)
            .into_iter()
            .skip(cursor)
            .collect::<Vec<_>>()
    })
}

fn wait_for_root_restore_after_events<F>(
    timeout: Duration,
    mut events_after_cursor: F,
) -> Result<String, String>
where
    F: FnMut() -> Vec<String>,
{
    let deadline = Instant::now() + timeout;
    loop {
        let events = events_after_cursor();
        match visible_burst_trace_settled(&events, 0, VisibleBurstSettlePolicy::ActivateRoot)? {
            Some(request_id) => return Ok(request_id),
            None => {}
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "no matching terminal native ROOT restore appeared within {}ms",
                timeout.as_millis()
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_hotkey_fixture_ready(
    child: &NativeChild,
    trace_path: &Path,
    timeout: Duration,
) -> Result<WindowSnapshot, String> {
    let displays = native_display_bounds()
        .map_err(|error| format!("inspect displays during hotkey fixture startup: {error}"))?;
    let deadline = Instant::now() + timeout;
    let mut hook_ready = false;
    let mut stable_root: Option<WindowSnapshot> = None;
    let mut stable_samples = 0u8;
    let mut latest_root = None;
    while Instant::now() < deadline {
        hook_ready |= trace_lines(trace_path)
            .iter()
            .any(|line| line.contains("trace_event=\"hook_service_ready\""));
        let current = child.refresh_root().ok();
        if let Some(root) = current.as_ref() {
            latest_root = Some(root.clone());
            if hotkey_fixture_root_is_ready(root, child.root().hwnd, child.process_id(), &displays)
            {
                if stable_root
                    .as_ref()
                    .is_some_and(|previous| same_window_state(previous, root))
                {
                    stable_samples = stable_samples.saturating_add(1);
                } else {
                    stable_root = Some(root.clone());
                    stable_samples = 1;
                }
                if hook_ready && stable_samples >= HOTKEY_FIXTURE_STABLE_SAMPLES {
                    return Ok(root.clone());
                }
            } else {
                stable_root = None;
                stable_samples = 0;
            }
        } else {
            stable_root = None;
            stable_samples = 0;
        }
        std::thread::sleep(WINDOW_POLL);
    }
    Err(format!(
        "hotkey fixture did not publish hook readiness and a stable, physically presented ROOT at configured static placement within {}ms; hook_service_ready={hook_ready}; stable_samples={stable_samples}/{HOTKEY_FIXTURE_STABLE_SAMPLES}; latest_root={:?}",
        timeout.as_millis(),
        latest_root.map(|root| (root.visible, root.minimized, root.bounds))
    ))
}

fn hotkey_fixture_root_is_ready(
    root: &WindowSnapshot,
    expected_hwnd: HWND,
    expected_pid: u32,
    displays: &[[i32; 4]],
) -> bool {
    let width = root.bounds[2].saturating_sub(root.bounds[0]);
    let height = root.bounds[3].saturating_sub(root.bounds[1]);
    root.role == WindowRole::Root
        && root.hwnd == expected_hwnd
        && root.process_id == expected_pid
        && root.visible
        && !root.minimized
        && intersects_display_bounds(root.bounds, displays)
        && root.bounds[0].abs_diff(240) <= 80
        && root.bounds[1].abs_diff(180) <= 80
        && (760..=1_040).contains(&width)
        && (540..=780).contains(&height)
}

fn wait_root_visibility(child: &NativeChild, visible: bool, timeout: Duration) -> bool {
    let Ok(displays) = native_display_bounds() else {
        return false;
    };
    wait_until(timeout, || {
        child.refresh_root().is_ok_and(|root| {
            root_matches_requested_visibility(
                &root,
                child.root().hwnd,
                child.process_id(),
                visible,
                &displays,
            )
        })
    })
}

fn root_is_physically_presented(root: &WindowSnapshot, displays: &[[i32; 4]]) -> bool {
    root.visible && !root.minimized && intersects_display_bounds(root.bounds, displays)
}

fn root_matches_requested_visibility(
    root: &WindowSnapshot,
    expected_hwnd: HWND,
    expected_pid: u32,
    visible: bool,
    displays: &[[i32; 4]],
) -> bool {
    let owned_root = root.role == WindowRole::Root
        && root.process_id == expected_pid
        && root.hwnd == expected_hwnd;
    owned_root && root_is_physically_presented(root, displays) == visible
}

fn latest_desired_visibility_revision(events: &[String]) -> Option<u64> {
    events.iter().rev().find_map(|line| {
        line.contains("trace_event=\"desired_visibility\"")
            .then(|| trace_field_value(line, "revision"))
            .flatten()
            .and_then(|value| value.parse::<u64>().ok())
    })
}

fn trace_contains_hold_visibility_work(
    events: &[String],
    previous_revision: Option<u64>,
    hold_invocation_id: Option<u64>,
) -> bool {
    events.iter().any(|line| {
        if !line.contains("trace_event=\"desired_visibility\"") {
            return false;
        }
        let invocation_matches = hold_invocation_id.is_some_and(|expected| {
            trace_field_value(line, "invocation_id").and_then(|value| value.parse::<u64>().ok())
                == Some(expected)
        });
        let revision_advanced = match (
            previous_revision,
            trace_field_value(line, "revision").and_then(|value| value.parse::<u64>().ok()),
        ) {
            (Some(previous), Some(current)) => current > previous,
            (None, Some(_)) | (_, None) => true,
        };
        invocation_matches || revision_advanced
    })
}

fn require_visible(root: &WindowSnapshot) -> Result<(), String> {
    let displays = native_display_bounds()?;
    if root.role == WindowRole::Root && root_is_physically_presented(root, &displays) {
        Ok(())
    } else {
        Err(format!(
            "ROOT is not on-screen and drawable: visible={} minimized={} bounds={:?}",
            root.visible, root.minimized, root.bounds
        ))
    }
}

fn require_hidden(root: &WindowSnapshot) -> Result<(), String> {
    let displays = native_display_bounds()?;
    if root.role == WindowRole::Root && !root_is_physically_presented(root, &displays) {
        Ok(())
    } else {
        Err(format!(
            "ROOT remains physically presented after hide: visible={} minimized={} bounds={:?}",
            root.visible, root.minimized, root.bounds
        ))
    }
}

fn require_parked_root_state(
    child: &NativeChild,
    expected: &WindowSnapshot,
    physical_displays: &[[i32; 4]],
) -> Result<WindowSnapshot, String> {
    let current = child.refresh_root()?;
    if current.hwnd != expected.hwnd
        || current.process_id != expected.process_id
        || current.role != WindowRole::Root
        || current.visible != expected.visible
        || current.minimized != expected.minimized
        || intersects_display_bounds(current.bounds, physical_displays)
    {
        return Err(format!(
            "ROOT left its parked physical-display state during Designer preview: expected HWND={} PID={} visible={} minimized={} off_display_bounds={:?}, observed HWND={} PID={} visible={} minimized={} bounds={:?} physical_displays={physical_displays:?}",
            hwnd_id(expected.hwnd),
            expected.process_id,
            expected.visible,
            expected.minimized,
            expected.bounds,
            hwnd_id(current.hwnd),
            current.process_id,
            current.visible,
            current.minimized,
            current.bounds,
        ));
    }
    Ok(current)
}

fn runtime_windows(child: &NativeChild) -> Vec<WindowSnapshot> {
    child
        .windows()
        .into_iter()
        .filter(|window| {
            is_radial_surface(window, child.process_id())
                && window.visible
                && !window.minimized
                && window.intersects_virtual_screen()
        })
        .collect()
}

fn wait_runtime_windows(
    child: &NativeChild,
    before: &[WindowSnapshot],
    timeout: Duration,
) -> Result<Vec<WindowSnapshot>, String> {
    let deadline = Instant::now() + timeout;
    let mut previous_ids: Option<Vec<u64>> = None;
    loop {
        let found: Vec<WindowSnapshot> = runtime_windows(child)
            .into_iter()
            .filter(|window| {
                !before.iter().any(|previous| {
                    previous.process_id == window.process_id
                        && hwnd_id(previous.hwnd) == hwnd_id(window.hwnd)
                })
            })
            .collect();
        let mut ids = found
            .iter()
            .map(|window| hwnd_id(window.hwnd))
            .collect::<Vec<_>>();
        ids.sort_unstable();
        if ids.len() >= 2 && previous_ids.as_ref() == Some(&ids) {
            return Ok(found);
        }
        previous_ids = (ids.len() >= 2).then_some(ids);
        if Instant::now() >= deadline {
            return Err(format!(
                "hold did not produce a stable set of at least two new visible child-owned radial HWNDs; observed [{}]",
                describe_radial_surfaces(&found)
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn validate_radial_surfaces(
    child: &NativeChild,
    surfaces: &[WindowSnapshot],
) -> Result<(), String> {
    if surfaces.len() < 2 {
        return Err(format!(
            "radial surface set must contain input and visual HWNDs, got [{}]",
            describe_radial_surfaces(surfaces)
        ));
    }
    for (index, surface) in surfaces.iter().enumerate() {
        if !is_radial_surface(surface, child.process_id()) {
            return Err(format!(
                "radial HWND={} is not a {} surface owned by exact candidate PID {}; observed PID={} role={:?} class={:?}",
                hwnd_id(surface.hwnd),
                RADIAL_HOST_WINDOW_CLASS,
                child.process_id(),
                surface.process_id,
                surface.role,
                surface.class_name
            ));
        }
        if hwnd_id(surface.hwnd) == 0
            || surfaces[..index]
                .iter()
                .any(|previous| hwnd_id(previous.hwnd) == hwnd_id(surface.hwnd))
        {
            return Err("radial surface set contains a null or duplicate HWND".into());
        }
    }
    Ok(())
}

fn radial_surfaces_are_active(child: &NativeChild, surfaces: &[WindowSnapshot]) -> bool {
    let current = child.windows();
    radial_surface_set_matches_active_state(surfaces, &current, child.process_id(), true)
}

fn radial_surfaces_are_inactive(child: &NativeChild, surfaces: &[WindowSnapshot]) -> bool {
    let current = child.windows();
    radial_surface_set_and_owner_are_inactive(surfaces, &current, child.process_id())
}

fn radial_surface_set_and_owner_are_inactive(
    surfaces: &[WindowSnapshot],
    current: &[WindowSnapshot],
    process_id: u32,
) -> bool {
    radial_surface_set_matches_active_state(surfaces, current, process_id, false)
        && active_radial_surfaces(current, process_id).is_empty()
}

fn active_radial_surfaces(windows: &[WindowSnapshot], process_id: u32) -> Vec<WindowSnapshot> {
    windows
        .iter()
        .filter(|window| {
            is_radial_surface(window, process_id)
                && window.visible
                && !window.minimized
                && window.intersects_virtual_screen()
        })
        .cloned()
        .collect()
}

fn radial_surface_set_matches_active_state(
    surfaces: &[WindowSnapshot],
    current: &[WindowSnapshot],
    process_id: u32,
    expected_active: bool,
) -> bool {
    if surfaces.len() < 2
        || surfaces
            .iter()
            .any(|surface| !is_radial_surface(surface, process_id))
    {
        return false;
    }
    surfaces.iter().all(|surface| {
        let active = current.iter().any(|window| {
            is_radial_surface(window, process_id)
                && hwnd_id(window.hwnd) == hwnd_id(surface.hwnd)
                && window.visible
                && !window.minimized
                && window.intersects_virtual_screen()
        });
        active == expected_active
    })
}

fn is_radial_surface(window: &WindowSnapshot, process_id: u32) -> bool {
    window.process_id == process_id
        && window.role == WindowRole::OtherChild
        && window.class_name == RADIAL_HOST_WINDOW_CLASS
}

fn describe_radial_surfaces(surfaces: &[WindowSnapshot]) -> String {
    if surfaces.is_empty() {
        return "none".into();
    }
    surfaces
        .iter()
        .map(|surface| {
            format!(
                "HWND:{} PID:{} role={:?} class={:?} visible={} minimized={} bounds={:?}",
                hwnd_id(surface.hwnd),
                surface.process_id,
                surface.role,
                surface.class_name,
                surface.visible,
                surface.minimized,
                surface.bounds
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn same_window_state(before: &WindowSnapshot, after: &WindowSnapshot) -> bool {
    hwnd_id(before.hwnd) == hwnd_id(after.hwnd)
        && before.process_id == after.process_id
        && before.role == after.role
        && before.class_name == after.class_name
        && before.visible == after.visible
        && before.minimized == after.minimized
        && before.bounds == after.bounds
}

fn describe_root_snapshot(root: &WindowSnapshot) -> String {
    format!(
        "HWND:{} PID:{} role={:?} class={:?} visible={} minimized={} bounds={:?}",
        hwnd_id(root.hwnd),
        root.process_id,
        root.role,
        root.class_name,
        root.visible,
        root.minimized,
        root.bounds
    )
}

fn wait_child(child: &mut NativeChild, timeout: Duration) -> Option<NativeExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

fn started_now() -> Instant {
    Instant::now()
}

fn bounded_text(text: &str, maximum: usize) -> String {
    if text.len() <= maximum {
        return text.to_owned();
    }
    let mut end = maximum;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mandatory_hotkey_cases_have_case_specific_expected_states() {
        let ids = [
            "H01", "H02", "H04", "H06", "H07", "H08", "H09", "H10", "H11", "H12", "H16", "H17",
            "H18",
        ];
        let generic = "candidate exits normally through production close path";
        let descriptions = ids
            .into_iter()
            .map(|id| (id, expected(id)))
            .collect::<Vec<_>>();

        assert!(descriptions.iter().all(|(_, text)| *text != generic));
        assert_eq!(
            descriptions
                .iter()
                .map(|(_, text)| *text)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            ids.len()
        );
        assert_eq!(
            descriptions.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            HOTKEY_CASE_IDS
                .into_iter()
                .filter(|id| id.starts_with('H'))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn h06_expected_contract_matches_hidden_start_and_visible_grid_evidence() {
        assert_eq!(
            expected("H06"),
            "from hidden ROOT, a short tap dismisses the active runtime radial, shows ROOT, and selects or dispatches no cell"
        );

        let observed = "evidence:v1; hotkey=F11; runtime_radial=dismissed; grid_visible=true; selection=none; dispatch=none; child_surface=closed; hover_ack=executable_cell";
        crate::validate_required_case_evidence("H06", CaseStatus::Passed, observed)
            .expect("H06 observed evidence must satisfy the R0 contract");
        let inconsistent = observed.replace("grid_visible=true", "grid_visible=false");
        assert!(
            crate::validate_required_case_evidence("H06", CaseStatus::Passed, &inconsistent)
                .is_err(),
            "a hidden-ending H06 observation must not satisfy its show-ROOT expectation"
        );
    }

    #[test]
    fn h16_expected_contract_matches_dismissal_grid_toggle_and_binding_evidence() {
        assert_eq!(
            expected("H16"),
            "a direct-trigger binding opens runtime radial; native and supported legacy-route launcher taps each dismiss it and toggle ROOT without selection, while the binding remains usable"
        );

        let observed = "evidence:v1; hotkey=F11; direct_trigger=opened; native_launcher_tap=dismissed+grid_toggled; legacy_launcher_tap=dismissed+grid_toggled; legacy_source=HotkeyTrigger+LegacyTrigger; direct_trigger_preserved=true; root_refreshed_after_direct=true; legacy_profile_cleanup=verified; legacy_child_pid=1234; legacy_profile_sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; legacy_trace_artifact=trace.log; legacy_profile_artifact=profile.json";
        crate::validate_required_case_evidence("H16", CaseStatus::Passed, observed)
            .expect("H16 observed evidence must satisfy the R0 contract");
        let incorrect = observed.replace(
            "native_launcher_tap=dismissed+grid_toggled",
            "native_launcher_tap=survived",
        );
        assert!(
            crate::validate_required_case_evidence("H16", CaseStatus::Passed, &incorrect).is_err(),
            "the native launcher tap must dismiss radial and toggle ROOT"
        );
    }

    #[test]
    fn finished_hotkey_capture_emits_the_validator_schema_version() {
        let directory = tempfile::tempdir().unwrap();
        let trace_path = directory.path().join("trace.log");
        fs::write(
            &trace_path,
            [
                "trace_event=\"configured_primary\" elapsed_ms=10 transition=Press invocation_id=5 modifiers_match=true provenance=ExternalInjected",
                "trace_event=\"configured_primary\" elapsed_ms=20 transition=Release invocation_id=5 modifiers_match=false provenance=ExternalInjected",
                "trace_event=\"short_tap\" elapsed_ms=22 invocation_id=5 terminal=true",
                "trace_event=\"desired_visibility\" elapsed_ms=24 visible=true revision=7 source=ToggleBatch invocation_id=5",
                "trace_event=\"root_command\" elapsed_ms=26 command=Focus request_id=9 visibility_revision=7 invocation_id=5",
                "trace_event=\"native_window_snapshot\" elapsed_ms=32 hwnd=1001 process_id=202 left=100 top=100 right=900 bottom=700 visible=true minimized=false request_id=9 visibility_revision=7 invocation_id=5",
            ]
            .join("\n"),
        )
        .unwrap();
        let capture = ActiveHotkeyEvidenceCapture {
            case_id: "H01".into(),
            segments: vec![HotkeyCaptureSegment {
                stream: HotkeyCandidateStream::MainCandidate,
                path: trace_path,
                cursor: 0,
                end: None,
                materialized_events: None,
                input_group_id: 1,
                purpose: HotkeyRunnerInputPurpose::LauncherChord,
                root_hwnd: 1001,
                root_process_id: 202,
            }],
            stream_overrides: Vec::new(),
            runner_edges: vec![
                HotkeyRunnerEdgeEvidence {
                    runner_relative_us: 100_000,
                    input_group_id: 1,
                    stream: HotkeyCandidateStream::MainCandidate,
                    purpose: HotkeyRunnerInputPurpose::LauncherChord,
                    virtual_key: 0x7A,
                    transition: HotkeyEdgeTransition::Press,
                    injected: true,
                    runner_cookie_matched: true,
                },
                HotkeyRunnerEdgeEvidence {
                    runner_relative_us: 125_000,
                    input_group_id: 1,
                    stream: HotkeyCandidateStream::MainCandidate,
                    purpose: HotkeyRunnerInputPurpose::LauncherChord,
                    virtual_key: 0x7A,
                    transition: HotkeyEdgeTransition::Release,
                    injected: true,
                    runner_cookie_matched: true,
                },
            ],
            first_runner_edge: Some(Instant::now()),
            runner_edge_overflow: false,
            capture_segment_overflow: false,
            candidate_trace_overflow: false,
            physical_displays: vec![[0, 0, 1920, 1080]],
            next_input_group_id: 2,
            current_purpose: None,
        };
        ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| *slot.borrow_mut() = Some(capture));

        let packet = finish_hotkey_evidence_capture("H01").unwrap();
        assert_eq!(packet.schema_version, 4);
        crate::validate_hotkey_evidence_packet_with_context(&packet, AcceptanceHotkey::F11, 350)
            .unwrap();
    }

    #[test]
    fn benign_foreign_gap_edges_are_retained_in_hotkey_evidence() {
        let start = Instant::now();
        let owned = |virtual_key, down, offset_ms| RunnerChordEdge {
            vk: virtual_key,
            down,
            injected: true,
            extra_info: ACCEPTANCE_RUNNER_INPUT_COOKIE,
            at: start + Duration::from_millis(offset_ms),
        };
        let foreign = |virtual_key, down, offset_ms| RunnerChordEdge {
            vk: virtual_key,
            down,
            injected: true,
            extra_info: 0,
            at: start + Duration::from_millis(offset_ms),
        };
        let observation = RunnerChordObservation {
            desktop: "Default".into(),
            keys: Vec::new(),
            ordered_edges: vec![
                owned(0xA0, true, 0),
                owned(0xA4, true, 2),
                owned(0x5B, true, 4),
                owned(0x23, true, 6),
                owned(0x23, false, 16),
                owned(0x5B, false, 18),
                owned(0xA4, false, 20),
                owned(0xA0, false, 22),
            ],
            foreign_edges: vec![foreign(0xA4, true, 50), foreign(0xA4, false, 55)],
        };

        begin_hotkey_evidence_capture("H04", Path::new("unused-trace.log"));
        set_hotkey_capture_purpose(HotkeyRunnerInputPurpose::MatrixBurst);
        capture_hotkey_runner_edges(
            &observation,
            HotkeyCandidateStream::MainCandidate,
            1,
            HotkeyRunnerInputPurpose::MatrixBurst,
        );
        let packet = finish_hotkey_evidence_capture("H04").unwrap();
        let foreign_rows = packet
            .runner_edges
            .iter()
            .filter(|edge| !edge.runner_cookie_matched)
            .collect::<Vec<_>>();
        assert_eq!(foreign_rows.len(), 2);
        assert!(foreign_rows.iter().all(|edge| {
            edge.input_group_id == 1
                && edge.purpose == HotkeyRunnerInputPurpose::MatrixBurst
                && edge.virtual_key == 0xA4
                && edge.injected
        }));
        assert_eq!(packet.runner_edges.len(), 10);
    }

    fn snapshot_wait_context() -> HotkeySnapshotWaitContext {
        HotkeySnapshotWaitContext {
            cursor: 0,
            stream: HotkeyCandidateStream::MainCandidate,
            input_group_id: 3,
            purpose: HotkeyRunnerInputPurpose::MatrixBurst,
            root_hwnd: 1001,
            root_process_id: 202,
            physical_displays: vec![[0, 0, 1920, 1080]],
        }
    }

    fn snapshot_wait_events(include_snapshot: bool) -> Vec<HotkeyCandidateEventEvidence> {
        let mut lines = vec![
            "trace_event=\"short_tap\" elapsed_ms=22 invocation_id=5 terminal=true",
            "trace_event=\"desired_visibility\" elapsed_ms=24 visible=true revision=7 source=ToggleBatch invocation_id=5",
            "trace_event=\"root_command\" elapsed_ms=26 command=Focus request_id=9 visibility_revision=7 invocation_id=5",
        ];
        if include_snapshot {
            lines.push("trace_event=\"native_window_snapshot\" elapsed_ms=32 hwnd=1001 process_id=202 left=100 top=100 right=900 bottom=700 visible=true minimized=false request_id=9 visibility_revision=7 invocation_id=5");
        }
        lines
            .into_iter()
            .enumerate()
            .filter_map(|(index, line)| {
                parse_hotkey_candidate_event(
                    line,
                    HotkeyCandidateStream::MainCandidate,
                    3,
                    HotkeyRunnerInputPurpose::MatrixBurst,
                    index + 1,
                )
            })
            .collect()
    }

    #[test]
    fn applied_tap_waits_for_its_correlated_physical_snapshot() {
        let context = snapshot_wait_context();
        let mut calls = 0;
        let mut events = snapshot_wait_events(false);
        let settled = wait_for_hotkey_snapshot_proof(
            &context,
            Duration::from_millis(100),
            Duration::from_millis(1),
            || {
                calls += 1;
                if calls == 2 {
                    events = snapshot_wait_events(true);
                }
                events.clone()
            },
        );

        assert!(settled);
        assert!(calls >= 2, "the wait must observe the deferred snapshot");
    }

    #[test]
    fn missing_or_uncorrelated_snapshot_does_not_settle_an_applied_tap() {
        let context = snapshot_wait_context();
        assert!(!hotkey_segment_has_physical_snapshot_proof(
            &context,
            &snapshot_wait_events(false)
        ));

        let mut wrong_snapshot = snapshot_wait_events(true);
        let snapshot = wrong_snapshot.last_mut().unwrap();
        snapshot.process_id = Some(203);
        assert!(!hotkey_segment_has_physical_snapshot_proof(
            &context,
            &wrong_snapshot
        ));

        let mut wrong_visibility = snapshot_wait_events(true);
        let snapshot = wrong_visibility.last_mut().unwrap();
        snapshot.bounds = Some([2000, 2000, 2800, 2600]);
        assert!(!hotkey_segment_has_physical_snapshot_proof(
            &context,
            &wrong_visibility
        ));

        let mut without_display_oracle = context.clone();
        without_display_oracle.physical_displays.clear();
        assert!(!hotkey_segment_has_physical_snapshot_proof(
            &without_display_oracle,
            &snapshot_wait_events(true)
        ));

        assert!(!wait_for_hotkey_snapshot_proof(
            &context,
            Duration::ZERO,
            Duration::ZERO,
            || snapshot_wait_events(false),
        ));
    }

    #[test]
    fn measured_segment_closes_before_intervening_setup_tap() {
        let directory = tempfile::tempdir().unwrap();
        let trace_path = directory.path().join("trace.log");
        let measured = [
            "trace_event=\"configured_primary\" elapsed_ms=10 transition=Press invocation_id=5 modifiers_match=true provenance=ExternalInjected",
            "trace_event=\"configured_primary\" elapsed_ms=20 transition=Release invocation_id=5 modifiers_match=false provenance=ExternalInjected",
            "trace_event=\"short_tap\" elapsed_ms=22 invocation_id=5 terminal=true",
            "trace_event=\"desired_visibility\" elapsed_ms=24 visible=true revision=7 source=ToggleBatch invocation_id=5",
            "trace_event=\"root_command\" elapsed_ms=26 command=Focus request_id=9 visibility_revision=7 invocation_id=5",
            "trace_event=\"native_window_snapshot\" elapsed_ms=32 hwnd=1001 process_id=202 left=100 top=100 right=900 bottom=700 visible=true minimized=false request_id=9 visibility_revision=7 invocation_id=5",
        ];
        fs::write(&trace_path, measured.join("\n")).unwrap();
        let mut capture = ActiveHotkeyEvidenceCapture {
            case_id: "H01".into(),
            segments: Vec::new(),
            stream_overrides: Vec::new(),
            runner_edges: Vec::new(),
            first_runner_edge: None,
            runner_edge_overflow: false,
            capture_segment_overflow: false,
            candidate_trace_overflow: false,
            physical_displays: vec![[0, 0, 1920, 1080]],
            next_input_group_id: 1,
            current_purpose: None,
        };
        let (_, first_group, _) = open_hotkey_capture_segment(
            &mut capture,
            HotkeyCandidateStream::MainCandidate,
            &trace_path,
            0,
            HotkeyRunnerInputPurpose::LauncherChord,
            1001,
            202,
        );
        complete_hotkey_capture_segment_in(&mut capture, &trace_path, measured.len()).unwrap();

        let setup = [
            "trace_event=\"configured_primary\" elapsed_ms=40 transition=Press invocation_id=6 modifiers_match=true provenance=ExternalInjected",
            "trace_event=\"configured_primary\" elapsed_ms=50 transition=Release invocation_id=6 modifiers_match=false provenance=ExternalInjected",
            "trace_event=\"short_tap\" elapsed_ms=52 invocation_id=6 terminal=true",
            "trace_event=\"desired_visibility\" elapsed_ms=54 visible=false revision=8 source=ToggleBatch invocation_id=6",
        ];
        let mut trace_lines = measured.to_vec();
        trace_lines.extend(setup);
        fs::write(&trace_path, trace_lines.join("\n")).unwrap();
        let second_cursor = trace_lines.len();
        let (_, second_group, _) = open_hotkey_capture_segment(
            &mut capture,
            HotkeyCandidateStream::MainCandidate,
            &trace_path,
            second_cursor,
            HotkeyRunnerInputPurpose::MatrixBurst,
            1001,
            202,
        );
        trace_lines.push(
            "trace_event=\"configured_primary\" elapsed_ms=60 transition=Press invocation_id=7 modifiers_match=true provenance=ExternalInjected".into(),
        );
        trace_lines.push(
            "trace_event=\"configured_primary\" elapsed_ms=70 transition=Release invocation_id=7 modifiers_match=false provenance=ExternalInjected".into(),
        );
        trace_lines
            .push("trace_event=\"short_tap\" elapsed_ms=72 invocation_id=7 terminal=true".into());
        trace_lines.push(
            "trace_event=\"desired_visibility\" elapsed_ms=74 visible=true revision=9 source=ToggleBatch invocation_id=7".into(),
        );
        fs::write(&trace_path, trace_lines.join("\n")).unwrap();
        complete_hotkey_capture_segment_in(&mut capture, &trace_path, trace_lines.len()).unwrap();
        fs::remove_file(&trace_path).unwrap();
        ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| *slot.borrow_mut() = Some(capture));

        let packet = finish_hotkey_evidence_capture("H01").unwrap();
        let first_events = packet
            .candidate_events
            .iter()
            .filter(|event| event.input_group_id == first_group)
            .collect::<Vec<_>>();
        let second_events = packet
            .candidate_events
            .iter()
            .filter(|event| event.input_group_id == second_group)
            .collect::<Vec<_>>();
        assert_eq!(first_events.len(), measured.len());
        assert!(
            first_events
                .iter()
                .all(|event| event.invocation_id != Some(6))
        );
        assert!(first_events.iter().any(|event| {
            event.kind == HotkeyTraceEventKind::NativeWindowSnapshot
                && event.invocation_id == Some(5)
        }));
        assert_eq!(second_events.len(), 4);
        assert!(
            second_events
                .iter()
                .all(|event| event.invocation_id == Some(7))
        );
        assert!(!packet.capture_segment_overflow);
    }

    #[test]
    fn temp_profile_legacy_stream_override_materializes_before_profile_removal() {
        let directory = tempfile::tempdir().unwrap();
        let main_trace_path = directory.path().join("main.log");
        let main_trace = [
            "trace_event=\"configured_primary\" elapsed_ms=10 transition=Press invocation_id=5 modifiers_match=true provenance=ExternalInjected",
            "trace_event=\"configured_primary\" elapsed_ms=20 transition=Release invocation_id=5 modifiers_match=false provenance=ExternalInjected",
            "trace_event=\"short_tap\" elapsed_ms=22 invocation_id=5 terminal=true",
            "trace_event=\"desired_visibility\" elapsed_ms=24 visible=true revision=7 source=ToggleBatch invocation_id=5",
            "trace_event=\"root_command\" elapsed_ms=26 command=Focus request_id=9 visibility_revision=7 invocation_id=5",
            "trace_event=\"native_window_snapshot\" elapsed_ms=32 hwnd=1001 process_id=202 left=100 top=100 right=900 bottom=700 visible=true minimized=false request_id=9 visibility_revision=7 invocation_id=5",
        ];
        let legacy_profile = tempfile::tempdir_in(directory.path()).unwrap();
        let legacy_trace_path = legacy_profile.path().join("acceptance.log");
        let legacy_trace = [
            "trace_event=\"configured_primary\" elapsed_ms=10 transition=Press invocation_id=5 modifiers_match=true provenance=ExternalInjected",
            "trace_event=\"configured_primary\" elapsed_ms=20 transition=Release invocation_id=5 modifiers_match=false provenance=ExternalInjected",
            "trace_event=\"short_tap\" elapsed_ms=22 invocation_id=5 terminal=true",
            "trace_event=\"desired_visibility\" elapsed_ms=24 visible=true revision=8 source=ToggleBatch invocation_id=5",
            "trace_event=\"root_command\" elapsed_ms=26 command=Focus request_id=10 visibility_revision=8 invocation_id=5",
            "trace_event=\"native_window_snapshot\" elapsed_ms=32 hwnd=2002 process_id=303 left=100 top=100 right=900 bottom=700 visible=true minimized=false request_id=10 visibility_revision=8 invocation_id=5",
        ];
        fs::write(&main_trace_path, main_trace.join("\n")).unwrap();
        fs::write(&legacy_trace_path, legacy_trace.join("\n")).unwrap();
        let mut capture = ActiveHotkeyEvidenceCapture {
            case_id: "H01".into(),
            segments: Vec::new(),
            stream_overrides: Vec::new(),
            runner_edges: Vec::new(),
            first_runner_edge: None,
            runner_edge_overflow: false,
            capture_segment_overflow: false,
            candidate_trace_overflow: false,
            physical_displays: vec![[0, 0, 1920, 1080]],
            next_input_group_id: 1,
            current_purpose: None,
        };
        let main = open_hotkey_capture_segment(
            &mut capture,
            HotkeyCandidateStream::MainCandidate,
            &main_trace_path,
            0,
            HotkeyRunnerInputPurpose::LauncherChord,
            1001,
            202,
        );
        assert_eq!(main.0, HotkeyCandidateStream::MainCandidate);
        ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| *slot.borrow_mut() = Some(capture));

        assert!(complete_open_hotkey_capture_at_current_trace(&main_trace_path).is_ok());
        register_hotkey_candidate_stream(
            &legacy_trace_path,
            HotkeyCandidateStream::LegacyFallbackCandidate,
        );
        let legacy = ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| {
            let mut capture = slot.borrow_mut();
            let capture = capture.as_mut().unwrap();
            open_hotkey_capture_segment(
                capture,
                HotkeyCandidateStream::MainCandidate,
                &legacy_trace_path,
                0,
                HotkeyRunnerInputPurpose::LauncherChord,
                2002,
                303,
            )
        });
        assert_eq!(legacy.0, HotkeyCandidateStream::LegacyFallbackCandidate);
        assert!(complete_open_hotkey_capture_at_current_trace(&legacy_trace_path).is_ok());
        legacy_profile.close().unwrap();

        let packet = finish_hotkey_evidence_capture("H01").unwrap();
        assert_eq!(
            packet.candidate_events.len(),
            main_trace.len() + legacy_trace.len()
        );
        assert!(packet.candidate_events.iter().any(|event| {
            event.stream == HotkeyCandidateStream::MainCandidate
                && event.hwnd == Some(1001)
                && event.process_id == Some(202)
        }));
        assert!(packet.candidate_events.iter().any(|event| {
            event.stream == HotkeyCandidateStream::LegacyFallbackCandidate
                && event.hwnd == Some(2002)
                && event.process_id == Some(303)
        }));
        assert_eq!(packet.root_identities.len(), 2);
        assert!(!packet.candidate_trace_overflow);
        assert!(!packet.capture_segment_overflow);
    }

    #[test]
    fn hotkey_capture_retains_unmatched_intent_command_and_radial_action_events() {
        fn capture_case(extra_line: &str) -> HotkeyCaseEvidence {
            let directory = tempfile::tempdir().unwrap();
            let trace_path = directory.path().join("trace.log");
            let mut lines = vec![
                "trace_event=\"configured_primary\" elapsed_ms=10 transition=Press invocation_id=5 modifiers_match=true provenance=ExternalInjected",
                "trace_event=\"configured_primary\" elapsed_ms=20 transition=Release invocation_id=5 modifiers_match=false provenance=ExternalInjected",
                "trace_event=\"short_tap\" elapsed_ms=22 invocation_id=5 terminal=true",
                "trace_event=\"desired_visibility\" elapsed_ms=24 visible=true revision=7 source=ToggleBatch invocation_id=5",
                "trace_event=\"root_command\" elapsed_ms=26 command=Focus request_id=9 visibility_revision=7 invocation_id=5",
                "trace_event=\"native_window_snapshot\" elapsed_ms=32 hwnd=1001 process_id=202 left=100 top=100 right=900 bottom=700 visible=true minimized=false request_id=9 visibility_revision=7 invocation_id=5",
            ];
            lines.push(extra_line);
            fs::write(&trace_path, lines.join("\n")).unwrap();
            ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| {
                *slot.borrow_mut() = Some(ActiveHotkeyEvidenceCapture {
                    case_id: "H01".into(),
                    segments: vec![HotkeyCaptureSegment {
                        stream: HotkeyCandidateStream::MainCandidate,
                        path: trace_path,
                        cursor: 0,
                        end: None,
                        materialized_events: None,
                        input_group_id: 1,
                        purpose: HotkeyRunnerInputPurpose::LauncherChord,
                        root_hwnd: 1001,
                        root_process_id: 202,
                    }],
                    stream_overrides: Vec::new(),
                    runner_edges: vec![
                        HotkeyRunnerEdgeEvidence {
                            runner_relative_us: 100_000,
                            input_group_id: 1,
                            stream: HotkeyCandidateStream::MainCandidate,
                            purpose: HotkeyRunnerInputPurpose::LauncherChord,
                            virtual_key: 0x7A,
                            transition: HotkeyEdgeTransition::Press,
                            injected: true,
                            runner_cookie_matched: true,
                        },
                        HotkeyRunnerEdgeEvidence {
                            runner_relative_us: 125_000,
                            input_group_id: 1,
                            stream: HotkeyCandidateStream::MainCandidate,
                            purpose: HotkeyRunnerInputPurpose::LauncherChord,
                            virtual_key: 0x7A,
                            transition: HotkeyEdgeTransition::Release,
                            injected: true,
                            runner_cookie_matched: true,
                        },
                    ],
                    first_runner_edge: Some(Instant::now()),
                    runner_edge_overflow: false,
                    capture_segment_overflow: false,
                    candidate_trace_overflow: false,
                    physical_displays: vec![[0, 0, 1920, 1080]],
                    next_input_group_id: 2,
                    current_purpose: None,
                });
            });
            finish_hotkey_evidence_capture("H01").unwrap()
        }

        let unmatched_intent = capture_case(
            "trace_event=\"desired_visibility\" elapsed_ms=34 visible=false revision=8 source=ToggleBatch invocation_id=99",
        );
        assert!(unmatched_intent.candidate_events.iter().any(|event| {
            event.kind == HotkeyTraceEventKind::VisibilityIntent
                && event.invocation_id == Some(99)
                && event.visibility_revision == Some(8)
        }));
        assert!(crate::validate_hotkey_evidence_packet(&unmatched_intent).is_err());

        let unmatched_command = capture_case(
            "trace_event=\"root_command\" elapsed_ms=34 command=Focus request_id=10 visibility_revision=8 invocation_id=99",
        );
        assert!(unmatched_command.candidate_events.iter().any(|event| {
            event.kind == HotkeyTraceEventKind::RootCommand && event.request_id == Some(10)
        }));
        assert!(crate::validate_hotkey_evidence_packet(&unmatched_command).is_err());

        let radial_dispatch = capture_case(
            "trace_event=\"radial_action\" elapsed_ms=34 stage=Dispatched skins=false editor_open=None skins_selected=None panel_registered=None",
        );
        assert!(radial_dispatch.candidate_events.iter().any(|event| {
            event.kind == HotkeyTraceEventKind::RadialAction
                && event.radial_action_stage == Some(HotkeyRadialActionStage::Dispatched)
        }));
        assert!(crate::validate_hotkey_evidence_packet(&radial_dispatch).is_err());
    }

    #[test]
    fn hotkey_candidate_parser_keeps_invocation_revision_and_boundary_identity() {
        let intent = parse_hotkey_candidate_event(
            "WARN target trace_event=\"desired_visibility\" elapsed_ms=27 visible=true revision=7 source=ToggleBatch invocation_id=5",
            HotkeyCandidateStream::MainCandidate,
            3,
            HotkeyRunnerInputPurpose::MatrixBurst,
            10,
        )
        .expect("desired visibility event");
        assert_eq!(intent.kind, HotkeyTraceEventKind::VisibilityIntent);
        assert_eq!(intent.input_group_id, 3);
        assert_eq!(intent.invocation_id, Some(5));
        assert_eq!(intent.visibility_revision, Some(7));
        assert_eq!(intent.visible, Some(true));
        assert_eq!(
            intent.visibility_source,
            Some(HotkeyVisibilitySource::ToggleBatch)
        );

        let command = parse_hotkey_candidate_event(
            "WARN target trace_event=\"root_command\" elapsed_ms=31 command=Show request_id=99 request_kind=None session_id=0 generation=99 terminal=false visibility_revision=7 invocation_id=5",
            HotkeyCandidateStream::MainCandidate,
            4,
            HotkeyRunnerInputPurpose::MatrixBurst,
            11,
        )
        .expect("ROOT boundary command");
        assert_eq!(command.kind, HotkeyTraceEventKind::RootCommand);
        assert_eq!(command.visibility_revision, Some(7));
        assert_eq!(command.invocation_id, Some(5));
        assert_eq!(command.request_id, Some(99));
        assert_eq!(command.terminal, Some(false));
        let compact = crate::encode_hotkey_candidate_event(&command).unwrap();
        assert_eq!(compact[7], serde_json::json!(false));
        let round_trip = crate::decode_hotkey_candidate_event(
            command.stream,
            command.input_group_id,
            command.input_purpose,
            &compact,
        )
        .unwrap();
        assert_eq!(round_trip, command);
        let mut mutated = compact;
        mutated[7] = serde_json::Value::Null;
        let mutated_round_trip = crate::decode_hotkey_candidate_event(
            command.stream,
            command.input_group_id,
            command.input_purpose,
            &mutated,
        )
        .unwrap();
        assert_ne!(mutated_round_trip, command);

        let snapshot = parse_hotkey_candidate_event(
            "WARN target trace_event=\"native_window_snapshot\" elapsed_ms=39 hwnd=7 process_id=42 left=100 top=200 right=900 bottom=700 visible=true minimized=false request_id=99 request_kind=Snapshot session_id=0 generation=99 terminal=true visibility_revision=7 invocation_id=5",
            HotkeyCandidateStream::MainCandidate,
            5,
            HotkeyRunnerInputPurpose::MatrixBurst,
            12,
        )
        .expect("correlated native snapshot");
        assert_eq!(snapshot.kind, HotkeyTraceEventKind::NativeWindowSnapshot);
        assert_eq!(snapshot.request_id, Some(99));
        assert_eq!(snapshot.hwnd, Some(7));
        assert_eq!(snapshot.process_id, Some(42));
        assert_eq!(snapshot.bounds, Some([100, 200, 900, 700]));
        assert_eq!(snapshot.visible, Some(true));
        assert_eq!(snapshot.terminal, Some(true));
        let compact_snapshot = crate::encode_hotkey_candidate_event(&snapshot).unwrap();
        assert_eq!(compact_snapshot[11], serde_json::json!(true));
        let snapshot_round_trip = crate::decode_hotkey_candidate_event(
            snapshot.stream,
            snapshot.input_group_id,
            snapshot.input_purpose,
            &compact_snapshot,
        )
        .unwrap();
        assert_eq!(snapshot_round_trip, snapshot);
        let mut mutated_snapshot = compact_snapshot;
        mutated_snapshot[11] = serde_json::json!(false);
        let mutated_snapshot_round_trip = crate::decode_hotkey_candidate_event(
            snapshot.stream,
            snapshot.input_group_id,
            snapshot.input_purpose,
            &mutated_snapshot,
        )
        .unwrap();
        assert_ne!(mutated_snapshot_round_trip, snapshot);

        let snapshot_false = parse_hotkey_candidate_event(
            "WARN target trace_event=\"native_window_snapshot\" elapsed_ms=40 hwnd=7 process_id=42 left=100 top=200 right=900 bottom=700 visible=true minimized=false request_id=100 request_kind=Snapshot session_id=0 generation=100 terminal=false visibility_revision=7 invocation_id=5",
            HotkeyCandidateStream::MainCandidate,
            5,
            HotkeyRunnerInputPurpose::MatrixBurst,
            14,
        )
        .expect("correlated nonterminal native snapshot");
        assert_eq!(snapshot_false.terminal, Some(false));
        let compact_snapshot_false = crate::encode_hotkey_candidate_event(&snapshot_false).unwrap();
        assert_eq!(compact_snapshot_false[11], serde_json::json!(false));
        let snapshot_false_round_trip = crate::decode_hotkey_candidate_event(
            snapshot_false.stream,
            snapshot_false.input_group_id,
            snapshot_false.input_purpose,
            &compact_snapshot_false,
        )
        .unwrap();
        assert_eq!(snapshot_false_round_trip, snapshot_false);
        let mut mutated_snapshot_false = compact_snapshot_false;
        mutated_snapshot_false[11] = serde_json::Value::Null;
        let mutated_snapshot_false_round_trip = crate::decode_hotkey_candidate_event(
            snapshot_false.stream,
            snapshot_false.input_group_id,
            snapshot_false.input_purpose,
            &mutated_snapshot_false,
        )
        .unwrap();
        assert_ne!(mutated_snapshot_false_round_trip, snapshot_false);

        let legacy = parse_hotkey_candidate_event(
            "WARN target trace_event=\"desired_visibility\" elapsed_ms=42 visible=false revision=8 source=LegacyTrigger invocation_id=none",
            HotkeyCandidateStream::LegacyFallbackCandidate,
            6,
            HotkeyRunnerInputPurpose::LauncherChord,
            13,
        )
        .expect("legacy trigger decision");
        assert_eq!(legacy.invocation_id, None);
        assert_eq!(legacy.visibility_revision, Some(8));
        assert_eq!(
            legacy.visibility_source,
            Some(HotkeyVisibilitySource::LegacyTrigger)
        );
    }

    #[test]
    fn legacy_trigger_intent_pairs_with_its_configured_no_tap_gesture() {
        let stream = HotkeyCandidateStream::LegacyFallbackCandidate;
        let purpose = HotkeyRunnerInputPurpose::LauncherChord;
        let lines = [
            "WARN target trace_event=\"configured_primary\" elapsed_ms=10 transition=Press invocation_id=17 modifiers_match=true provenance=ExternalInjected",
            "WARN target trace_event=\"configured_primary\" elapsed_ms=20 transition=Release invocation_id=17 modifiers_match=false provenance=ExternalInjected",
            "WARN target trace_event=\"desired_visibility\" elapsed_ms=22 visible=false revision=8 source=LegacyTrigger invocation_id=none",
            "WARN target trace_event=\"root_command\" elapsed_ms=24 command=Minimize request_id=90 visibility_revision=8 invocation_id=none",
            "WARN target trace_event=\"native_window_snapshot\" elapsed_ms=26 hwnd=1001 process_id=202 left=3000 top=100 right=3800 bottom=700 visible=false minimized=true request_id=90 visibility_revision=8 invocation_id=none",
        ];
        let events = lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                parse_hotkey_candidate_event(*line, stream, 6, purpose, index + 1)
                    .expect("parse legacy chord trace event")
            })
            .collect::<Vec<_>>();

        let (gestures, standalone, proof_error) = build_hotkey_decision_proofs(&events);
        assert!(!proof_error);
        assert_eq!(gestures.len(), 1);
        assert!(gestures[0].short_tap_elapsed_ms.is_none());
        assert!(matches!(
            gestures[0].decision,
            HotkeyDecisionProof::NotApplicable {
                reason: HotkeyEvidenceNotApplicable::LegacyTriggerHasNoInvocationReducerId
            }
        ));
        assert_eq!(standalone.len(), 1);
        assert_eq!(standalone[0].input_group_id, 6);
        assert_eq!(standalone[0].source, HotkeyVisibilitySource::LegacyTrigger);

        let mut unrelated_group = events;
        for event in unrelated_group.iter_mut().skip(2) {
            event.input_group_id = 7;
        }
        let (unpaired_gestures, _, _) = build_hotkey_decision_proofs(&unrelated_group);
        assert!(matches!(
            unpaired_gestures[0].decision,
            HotkeyDecisionProof::NotApplicable {
                reason: HotkeyEvidenceNotApplicable::HoldGestureHasNoShortTap
            }
        ));
    }

    #[test]
    fn screen_draw_trace_builds_linked_follow_on_root_and_native_spans() {
        let stream = HotkeyCandidateStream::MainCandidate;
        let purpose = HotkeyRunnerInputPurpose::LauncherChord;
        let lines = [
            "WARN target trace_event=\"desired_visibility\" elapsed_ms=10 visible=true revision=7 source=ToggleBatch invocation_id=5",
            "WARN target trace_event=\"desired_visibility\" elapsed_ms=15 visible=true revision=8 source=ScreenDrawRestore invocation_id=5",
            "WARN target trace_event=\"screen_draw_restore_focus_intent\" elapsed_ms=15 revision=8 invocation_id=5 focus_intent=ActivateRoot",
            "WARN target trace_event=\"root_command\" elapsed_ms=16 command=Show request_id=99 visibility_revision=8 invocation_id=5",
            "WARN target trace_event=\"native_activation\" elapsed_ms=17 edge=RestoreRequested hwnd=7 request_id=100 visibility_revision=8 invocation_id=5 terminal=false",
            "WARN target trace_event=\"native_window_snapshot\" elapsed_ms=20 hwnd=7 process_id=42 left=100 top=200 right=900 bottom=700 visible=true minimized=false request_id=99 visibility_revision=8 invocation_id=5",
            "WARN target trace_event=\"native_activation\" elapsed_ms=22 edge=RestoreCompleted hwnd=7 request_id=100 visibility_revision=8 invocation_id=5 terminal=true",
        ];
        let events = lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                parse_hotkey_candidate_event(line, stream, 4, purpose, index + 1)
                    .expect("parse Screen Draw trace row")
            })
            .collect::<Vec<_>>();
        let follow_on = build_hotkey_follow_on_restorations(&events);
        assert_eq!(follow_on.len(), 1);
        assert_eq!(follow_on[0].parent_visibility_revision, Some(7));
        assert_eq!(follow_on[0].visibility_revision, 8);
        assert_eq!(follow_on[0].invocation_id, Some(5));
        assert_eq!(
            follow_on[0].focus_intent,
            HotkeyRootFocusIntent::ActivateRoot
        );
        assert_eq!(follow_on[0].root_commands.len(), 1);
        let activation = follow_on[0]
            .native_activation
            .as_ref()
            .expect("correlated terminal native activation");
        assert_eq!(activation.request_id, 100);
        assert_eq!(
            activation.terminal_edge,
            Some(HotkeyActivationEdge::RestoreCompleted)
        );

        let mut unrelated = events;
        for event in unrelated.iter_mut().skip(1) {
            if event.visibility_revision == Some(8) {
                event.invocation_id = None;
            }
        }
        let unrelated_follow_on = build_hotkey_follow_on_restorations(&unrelated);
        assert_eq!(unrelated_follow_on.len(), 1);
        assert_eq!(unrelated_follow_on[0].invocation_id, None);
        assert_eq!(unrelated_follow_on[0].parent_visibility_revision, None);

        let preserve_lines = [
            "WARN target trace_event=\"desired_visibility\" elapsed_ms=10 visible=true revision=7 source=ToggleBatch invocation_id=5",
            "WARN target trace_event=\"desired_visibility\" elapsed_ms=15 visible=true revision=8 source=ScreenDrawRestore invocation_id=5",
            "WARN target trace_event=\"screen_draw_restore_focus_intent\" elapsed_ms=15 revision=8 invocation_id=5 focus_intent=PreserveForeground",
            "WARN target trace_event=\"root_command\" elapsed_ms=16 command=Show request_id=99 visibility_revision=8 invocation_id=5",
            "WARN target trace_event=\"native_window_snapshot\" elapsed_ms=20 hwnd=7 process_id=42 left=100 top=200 right=900 bottom=700 visible=true minimized=false request_id=99 visibility_revision=8 invocation_id=5",
        ];
        let preserve_events = preserve_lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                parse_hotkey_candidate_event(line, stream, 4, purpose, index + 1)
                    .expect("parse PreserveForeground Screen Draw trace row")
            })
            .collect::<Vec<_>>();
        let preserve_follow_on = build_hotkey_follow_on_restorations(&preserve_events);
        assert_eq!(preserve_follow_on.len(), 1);
        assert_eq!(
            preserve_follow_on[0].focus_intent,
            HotkeyRootFocusIntent::PreserveForeground
        );
        assert!(preserve_follow_on[0].native_activation.is_none());
    }

    #[test]
    fn h16_legacy_fixture_log_matches_the_runner_proof_path() {
        let profile = tempfile::tempdir().expect("temporary legacy profile");
        let proof_path = legacy_fallback_trace_path(profile.path());
        let fixture = crate::deterministic_fixture_for_hotkey_with_direct_trigger_chord(
            &proof_path,
            crate::MouseGestureMode::Enabled,
            AcceptanceHotkey::F11,
            "Ctrl+Alt+Y",
            false,
        )
        .expect("deterministic legacy-route fixture");
        let settings: crate::Settings =
            serde_json::from_slice(&fixture.settings_json).expect("decode fixture settings");
        let configured_path = match settings.log_file {
            Some(crate::LogFile::Path(path)) => PathBuf::from(path),
            other => panic!("fixture did not configure its trace log path: {other:?}"),
        };

        assert_eq!(configured_path, proof_path);
        assert_eq!(configured_path.file_name().unwrap(), "acceptance.log");
    }

    #[test]
    fn h16_trace_copy_failure_keeps_bounded_recovery_evidence() {
        let output = tempfile::tempdir().expect("temporary H16 evidence directory");
        let source = output.path().join("isolated-acceptance.log");
        fs::write(&source, b"isolated child trace evidence").expect("write isolated trace");
        fs::create_dir(output.path().join("case-H16-legacy-trace.log"))
            .expect("block canonical trace destination");

        let (artifacts, errors) = persist_h16_legacy_trace_artifacts(&source, output.path());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("copy legacy-route trace evidence"));
        assert_eq!(artifacts.len(), 1);
        assert_eq!(
            artifacts[0].file_name().unwrap(),
            "case-H16-legacy-trace-recovered.log"
        );
        assert_eq!(
            fs::read(&artifacts[0]).expect("read recovered bounded trace"),
            b"isolated child trace evidence"
        );
    }

    #[test]
    fn h16_failed_result_still_attaches_isolated_trace_and_receipt_artifacts() {
        let output = tempfile::tempdir().expect("temporary H16 report directory");
        let main_trace = output.path().join("main-acceptance.log");
        fs::write(&main_trace, "main child trace").expect("write main trace");
        let isolated_source = output.path().join("isolated-acceptance.log");
        fs::write(&isolated_source, "isolated child trace").expect("write isolated trace");
        fs::create_dir(output.path().join("case-H16-legacy-trace.log"))
            .expect("block canonical trace destination");
        let (mut artifacts, copy_errors) =
            persist_h16_legacy_trace_artifacts(&isolated_source, output.path());
        assert_eq!(copy_errors.len(), 1);
        let receipt = output.path().join("case-H16-legacy-profile.json");
        fs::write(
            &receipt,
            serde_json::to_vec(&serde_json::json!({"cleanup_errors": copy_errors}))
                .expect("serialize cleanup receipt"),
        )
        .expect("write isolated cleanup receipt");
        artifacts.push(receipt.clone());
        let (case_result, fallback_artifacts) =
            h16_fallback_result_for_case(Err(LegacyFallbackFailure {
                failure: CaseFailure::new(
                    FailureStage::Cleanup,
                    "legacy-route cleanup/evidence persistence failed".into(),
                ),
                artifacts,
            }));
        let mut report = test_acceptance_report(AcceptanceHotkey::F11);

        append_h16_case_result(
            &mut report,
            Instant::now(),
            case_result.map(|_| "unexpected success".to_string()),
            None,
            output.path(),
            &main_trace,
            &fallback_artifacts,
        );

        let case = report
            .cases
            .iter()
            .find(|case| case.id == "H16")
            .expect("failed H16 case");
        assert!(matches!(case.status, CaseStatus::Failed));
        for artifact in &fallback_artifacts {
            let path = artifact.to_string_lossy().to_string();
            assert!(case.artifacts.contains(&path));
            assert!(report.artifacts.contains(&path));
        }
    }

    fn test_acceptance_report(hotkey: AcceptanceHotkey) -> AcceptanceReport {
        AcceptanceReport {
            schema_version: 7,
            run_id: "test-run".into(),
            mode: "native_windows",
            started_unix_ms: 1,
            finished_unix_ms: 2,
            copied_profile_status: super::super::super::CopiedProfileStatus::NotRun,
            copied_profile: None,
            private_artifacts: None,
            h6_repeat_mode: H6RepeatMode::Quiescent,
            mouse_gesture_mode: super::super::super::MouseGestureMode::Enabled,
            suite: AcceptanceSuite::Hotkey,
            hotkey,
            outcome: "failed",
            candidate: super::super::super::CandidateIdentity {
                executable: "candidate.exe".into(),
                sha256: "a".repeat(64),
            },
            environment: super::super::super::EnvironmentIdentity {
                os_version: "test".into(),
                architecture: "x64".into(),
                runner_process_id: 1,
                runner_sha256: Some("b".repeat(64)),
                child_process_id: None,
                child_started_unix_ms: None,
                source_revision: Some("test-revision".into()),
                monitors: Vec::new(),
            },
            profile: super::super::super::ProfileIdentity {
                mode: "test",
                temporary_data_root: "temporary".into(),
                settings_sha256: "c".repeat(64),
                radial_sha256: "d".repeat(64),
                actions_sha256: "e".repeat(64),
                configured_hotkey: hotkey.as_str(),
                hold_threshold_ms: 350,
            },
            cases: Vec::new(),
            hotkey_evidence: Vec::new(),
            artifacts: Vec::new(),
            cleanup: super::super::super::CleanupResult::default(),
            capacity_saturated: false,
            report_overflow: None,
        }
    }

    #[test]
    fn hotkey_observed_cadence_accepts_bounded_holds_and_releases() {
        let timing = RunnerChordTiming {
            primary_hold_ms: vec![10, 25, 100],
            released_gap_ms: vec![10, 25, 100],
        };
        assert!(hotkey_cadence_is_valid(&timing, 350));
        assert!(!hotkey_cadence_is_valid(
            &RunnerChordTiming {
                primary_hold_ms: vec![9],
                released_gap_ms: vec![],
            },
            350,
        ));
        assert!(!hotkey_cadence_is_valid(
            &RunnerChordTiming {
                primary_hold_ms: vec![101],
                released_gap_ms: vec![],
            },
            350,
        ));
        assert!(!hotkey_cadence_is_valid(
            &RunnerChordTiming {
                primary_hold_ms: vec![25],
                released_gap_ms: vec![9],
            },
            350,
        ));
        assert!(!hotkey_cadence_is_valid(
            &RunnerChordTiming {
                primary_hold_ms: vec![350],
                released_gap_ms: vec![],
            },
            350,
        ));
    }

    #[test]
    fn h04_uses_steady_released_cadence_without_changing_other_hotkey_cases() {
        let (matrix_down, matrix_release) = hotkey_burst_intervals(true);
        assert_eq!(matrix_down, Duration::from_millis(25));
        assert_eq!(matrix_release, Duration::from_millis(75));
        assert!((10..=100).contains(&matrix_down.as_millis()));
        assert!((10..=100).contains(&matrix_release.as_millis()));

        let (ordinary_down, ordinary_release) = hotkey_burst_intervals(false);
        assert_eq!(ordinary_down, Duration::from_millis(25));
        assert_eq!(ordinary_release, Duration::from_millis(25));
    }

    #[test]
    fn only_in_window_foreign_h04_edges_receive_retryable_classification() {
        let edge = RunnerChordEdge {
            vk: 0xA4,
            down: false,
            injected: true,
            extra_info: 0,
            at: Instant::now(),
        };
        let contaminated =
            foreign_edge_contamination_failure(4, &[edge], true, "observer").unwrap();
        assert!(matches!(contaminated.stage, FailureStage::InputInjection));
        assert_eq!(contaminated.input_contamination_group, Some(4));

        let non_matrix = foreign_edge_contamination_failure(4, &[edge], false, "observer").unwrap();
        assert_eq!(non_matrix.input_contamination_group, None);
        assert!(foreign_edge_contamination_failure(4, &[], true, "observer").is_none());
    }

    #[test]
    fn h04_retry_restarts_whole_matrix_once_and_retains_contamination_evidence() {
        let mut calls = Vec::new();
        let mut preserved = Vec::new();
        let outcome = run_h04_matrix_with_retry(
            |attempt| {
                calls.push(attempt);
                if attempt == 1 {
                    Err(CaseFailure::input_contamination(
                        7,
                        "foreign Alt-up overlapped owned chord".into(),
                    ))
                } else {
                    Ok("full clean 86-decision matrix".to_owned())
                }
            },
            |attempt, failure| {
                preserved.push((attempt, failure.message.clone()));
                Ok(format!("case-H04-attempt-{attempt}-contamination.json"))
            },
        );

        assert_eq!(calls, [1, 2]);
        assert_eq!(outcome.attempts, 2);
        assert_eq!(outcome.contamination_attempts, 1);
        assert_eq!(
            outcome.contamination_artifact_names,
            ["case-H04-attempt-1-contamination.json"]
        );
        assert_eq!(outcome.result.unwrap(), "full clean 86-decision matrix");
        assert_eq!(preserved.len(), 1);
        assert!(preserved[0].1.contains("foreign Alt-up"));
    }

    #[test]
    fn h04_retry_is_limited_to_two_contaminated_whole_matrix_attempts() {
        let mut calls = Vec::new();
        let mut preserved = Vec::new();
        let outcome: H04RetryOutcome<()> = run_h04_matrix_with_retry(
            |attempt| {
                calls.push(attempt);
                Err(CaseFailure::input_contamination(
                    u32::from(attempt),
                    format!("attempt {attempt} foreign Alt edge"),
                ))
            },
            |attempt, failure| {
                preserved.push((attempt, failure.message.clone()));
                Ok(format!("attempt-{attempt}-artifact"))
            },
        );

        assert_eq!(calls, [1, 2]);
        assert_eq!(outcome.attempts, 2);
        assert_eq!(outcome.contamination_attempts, 2);
        assert_eq!(
            preserved
                .iter()
                .map(|(attempt, _)| *attempt)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        let failure = outcome.result.unwrap_err();
        assert!(failure.message.contains("attempt 1 foreign Alt edge"));
        assert!(failure.message.contains("attempt 2 foreign Alt edge"));
        assert_eq!(outcome.contamination_artifact_names.len(), 2);
    }

    #[test]
    fn h04_product_mismatch_does_not_retry_or_lose_its_original_cause() {
        let mut calls = 0usize;
        let mut preserve_calls = 0usize;
        let outcome: H04RetryOutcome<()> = run_h04_matrix_with_retry(
            |_| {
                calls += 1;
                Err(CaseFailure::new(
                    FailureStage::GestureDecision,
                    "missing committed ToggleBatch intent".into(),
                ))
            },
            |_, _| {
                preserve_calls += 1;
                Ok("should-not-be-written".into())
            },
        );

        assert_eq!(calls, 1);
        assert_eq!(preserve_calls, 0);
        assert_eq!(outcome.attempts, 1);
        assert_eq!(outcome.contamination_attempts, 0);
        let failure = outcome.result.unwrap_err();
        assert!(matches!(failure.stage, FailureStage::GestureDecision));
        assert_eq!(failure.message, "missing committed ToggleBatch intent");
    }

    #[test]
    fn failed_attempt_preservation_prevents_retry_and_keeps_original_failure() {
        let mut calls = 0usize;
        let outcome: H04RetryOutcome<()> = run_h04_matrix_with_retry(
            |_| {
                calls += 1;
                Err(CaseFailure::input_contamination(
                    9,
                    "foreign Shift edge interrupted input".into(),
                ))
            },
            |_, _| {
                Err(CaseFailure::new(
                    FailureStage::Environment,
                    "attempt evidence exceeded its byte bound".into(),
                ))
            },
        );

        assert_eq!(calls, 1);
        assert_eq!(outcome.attempts, 1);
        let failure = outcome.result.unwrap_err();
        assert!(
            failure
                .message
                .contains("foreign Shift edge interrupted input")
        );
        assert!(
            failure
                .message
                .contains("attempt evidence exceeded its byte bound")
        );
        assert!(failure.message.contains("no retry was started"));
    }

    #[test]
    fn h04_retry_artifact_is_typed_bounded_and_linked_into_r0_input() {
        let output = tempfile::tempdir().unwrap();
        let stream = HotkeyCandidateStream::MainCandidate;
        let purpose = HotkeyRunnerInputPurpose::MatrixBurst;
        let group = 7;
        let edge = |time, virtual_key, transition, owned| HotkeyRunnerEdgeEvidence {
            runner_relative_us: time,
            input_group_id: group,
            stream,
            purpose,
            virtual_key,
            transition,
            injected: true,
            runner_cookie_matched: owned,
        };
        let owned_edges = vec![
            edge(1_000, 0xA0, HotkeyEdgeTransition::Press, true),
            edge(1_100, 0xA4, HotkeyEdgeTransition::Press, true),
            edge(1_200, 0x5B, HotkeyEdgeTransition::Press, true),
            edge(1_300, 0x23, HotkeyEdgeTransition::Press, true),
            edge(1_800, 0x23, HotkeyEdgeTransition::Release, true),
            edge(1_900, 0x5B, HotkeyEdgeTransition::Release, true),
            edge(2_000, 0xA4, HotkeyEdgeTransition::Release, true),
            edge(2_100, 0xA0, HotkeyEdgeTransition::Release, true),
        ];
        let completed_bursts = (0..6usize)
            .scan(100u64, |next_id, index| {
                let initial_visible = index >= 5;
                let taps = [1usize, 2, 5, 10, 25][index % 5];
                let invocation_ids = (*next_id..*next_id + taps as u64).collect::<Vec<_>>();
                *next_id += taps as u64;
                Some(H04CompletedBurstEvidence {
                    matrix_burst_index: (index + 1) as u8,
                    input_group_id: (index + 1) as u32,
                    initial_visible,
                    requested_taps: taps as u8,
                    final_visible: initial_visible ^ (taps % 2 == 1),
                    hold_min_ms: 25,
                    hold_max_ms: 25,
                    gap_min_ms: if taps == 1 { 0 } else { 75 },
                    gap_max_ms: if taps == 1 { 0 } else { 75 },
                    invocation_ids,
                    trace_probe_id: (index + 1) as u64,
                    trace_cursor: (index + 1) * 100,
                    baseline_invocation_id: None,
                    baseline_visibility_revision: None,
                })
            })
            .collect::<Vec<_>>();
        let artifact = H04InputContaminationArtifact {
            schema_version: 1,
            case_id: "H04".into(),
            attempt: 1,
            hotkey: AcceptanceHotkey::ShiftAltWinEnd,
            failure_stage: "InputInjection".into(),
            failure: "foreign Alt-up interleaved with the owned chord".into(),
            declared_initial_state: false,
            next_matrix_burst_index: 7,
            completed_bursts,
            input_group_id: group,
            stream,
            prior_group_ids: (1..=group).collect(),
            owned_edges,
            foreign_edges: vec![
                edge(600, 0xA4, HotkeyEdgeTransition::Press, false),
                edge(700, 0xA4, HotkeyEdgeTransition::Release, false),
                edge(1_500, 0xA4, HotkeyEdgeTransition::Release, false),
            ],
            candidate_events: Vec::new(),
            root_identity: HotkeyRootIdentityEvidence {
                stream,
                hwnd: 42,
                process_id: 7001,
            },
        };
        super::super::super::validate_h04_contamination_artifact(
            &artifact,
            AcceptanceHotkey::ShiftAltWinEnd,
        )
        .unwrap();

        let artifact_name = "case-H04-attempt-1-contamination.json";
        let artifact_path = output.path().join(artifact_name);
        fs::write(&artifact_path, serde_json::to_vec(&artifact).unwrap()).unwrap();
        let path_text = artifact_path.to_string_lossy().to_string();
        let mut second_attempt = artifact.clone();
        second_attempt.attempt = 2;
        second_attempt.next_matrix_burst_index = 1;
        second_attempt.completed_bursts.clear();
        second_attempt.input_group_id = 1;
        second_attempt.prior_group_ids = vec![1];
        for edge in second_attempt
            .owned_edges
            .iter_mut()
            .chain(second_attempt.foreign_edges.iter_mut())
        {
            edge.input_group_id = 1;
        }
        super::super::super::validate_h04_contamination_artifact(
            &second_attempt,
            AcceptanceHotkey::ShiftAltWinEnd,
        )
        .unwrap();
        let second_artifact_name = "case-H04-attempt-2-contamination.json";
        let second_artifact_path = output.path().join(second_artifact_name);
        fs::write(
            &second_artifact_path,
            serde_json::to_vec(&second_attempt).unwrap(),
        )
        .unwrap();
        let second_path_text = second_artifact_path.to_string_lossy().to_string();
        let mut report = test_acceptance_report(AcceptanceHotkey::ShiftAltWinEnd);
        report.artifacts.push(path_text.clone());
        report.artifacts.push(second_path_text.clone());
        report.cases.push(AcceptanceCaseResult {
            id: "H04".into(),
            status: CaseStatus::Failed,
            elapsed_ms: 1,
            expected: expected("H04").into(),
            observed: format!(
                "evidence:v1; hotkey={}; failure=InputInjection: both whole-matrix attempts were contaminated; matrix_attempts=2; contamination_attempts=2; contamination_artifacts={artifact_name}|{second_artifact_name}; attempt_restart_state=hidden; full_clean_matrix=false",
                AcceptanceHotkey::ShiftAltWinEnd.as_str()
            ),
            failure_stage: Some(FailureStage::InputInjection),
            artifacts: vec![path_text.clone(), second_path_text.clone()],
        });
        super::super::super::validate_hotkey_evidence_report(&report).unwrap();
        super::super::super::validate_case_hotkey_profile_relation(&report, &report.cases[0])
            .unwrap();

        let mut wrong_hotkey = report.cases[0].clone();
        wrong_hotkey.observed = wrong_hotkey
            .observed
            .replace("hotkey=Shift+Alt+Win+End", "hotkey=F11");
        assert!(
            super::super::super::validate_case_hotkey_profile_relation(&report, &wrong_hotkey)
                .is_err()
        );
        let mut missing_hotkey = report.cases[0].clone();
        missing_hotkey.observed = missing_hotkey.observed.replace(
            "evidence:v1; hotkey=Shift+Alt+Win+End; ",
            "InputInjection: ",
        );
        assert!(
            super::super::super::validate_case_hotkey_profile_relation(&report, &missing_hotkey)
                .is_err()
        );

        let mut mismatched_path = report.clone();
        mismatched_path.cases[0].artifacts[0] = format!("different-root/{artifact_name}");
        assert!(super::super::super::validate_hotkey_evidence_report(&mismatched_path).is_err());

        // Two retries with a single artifact for attempt 2 must not look like a
        // complete report: contamination records are a one-to-one prefix.
        let mut skipped_attempt = report.clone();
        let case = &mut skipped_attempt.cases[0];
        case.status = CaseStatus::Passed;
        case.failure_stage = None;
        case.observed = format!(
            "evidence:v1; hotkey={}; matrix=hidden+visible:1,2,5,10,25; decisions=86; unique_invocation_ids=86; invocation_ids_sha256={}; quiet_window_ms=80..80; preflight_matching_edges=0; foreign_matching_edges=0; modifiers_clear_each_burst=true; matrix_attempts=2; contamination_attempts=1; contamination_artifacts={second_artifact_name}; attempt_restart_state=hidden; full_clean_matrix=true",
            AcceptanceHotkey::ShiftAltWinEnd.as_str(),
            "a".repeat(64)
        );
        case.artifacts = vec![second_path_text.clone()];
        skipped_attempt.artifacts = vec![second_path_text.clone()];
        assert!(
            super::super::super::validate_h04_contamination_artifacts(
                &skipped_attempt,
                &skipped_attempt.cases[0]
            )
            .is_err()
        );

        report.artifacts.clear();
        assert!(super::super::super::validate_hotkey_evidence_report(&report).is_err());
        report.artifacts.push(path_text);
        report.artifacts.push(second_path_text);
        let mut malformed = artifact;
        malformed.foreign_edges[2].runner_cookie_matched = true;
        fs::write(&artifact_path, serde_json::to_vec(&malformed).unwrap()).unwrap();
        assert!(super::super::super::validate_hotkey_evidence_report(&report).is_err());
    }

    #[test]
    fn h12_preview_hosts_are_never_runtime_cleanup_targets_before_hold_attempt() {
        let mut preview_hosts = std::collections::BTreeSet::new();
        preview_hosts.insert(101);
        preview_hosts.insert(102);

        // OpenDesktopPreview may create hosts before its reply/window discovery
        // times out. Until the runtime hold phase starts, those HWNDs cannot be
        // classified as runtime radial surfaces.
        assert!(!h12_surface_belongs_to_runtime(false, 101, &preview_hosts));
        assert!(!h12_surface_belongs_to_runtime(false, 103, &preview_hosts));
        assert!(!h12_surface_belongs_to_runtime(true, 101, &preview_hosts));
        assert!(!h12_surface_belongs_to_runtime(true, 102, &preview_hosts));
        assert!(h12_surface_belongs_to_runtime(true, 103, &preview_hosts));
    }

    #[test]
    fn h17_successful_tap_still_preserves_evidence_when_later_cleanup_fails() {
        assert!(!h17_should_preserve_alternate_artifacts(true, true));
        assert!(h17_should_preserve_alternate_artifacts(true, false));
        assert!(h17_should_preserve_alternate_artifacts(false, true));

        let output = tempfile::tempdir().expect("temporary H17 evidence directory");
        let snapshot = H17AlternateArtifactSnapshot {
            trace_excerpt: Some(b"alternate trace".to_vec()),
            windows_inventory: Some(b"{\"windows\":[]}".to_vec()),
            private_log_tail: Some(b"alternate log tail".to_vec()),
            capture_errors: Vec::new(),
        };
        let (paths, errors) = if h17_should_preserve_alternate_artifacts(true, false) {
            persist_h17_alternate_artifacts(&snapshot, output.path())
        } else {
            (Vec::new(), Vec::new())
        };

        assert!(errors.is_empty());
        assert_eq!(paths.len(), 3);
        assert_eq!(
            fs::read(output.path().join("case-H17-alternate-trace.log")).unwrap(),
            b"alternate trace"
        );
        assert_eq!(
            fs::read(output.path().join("case-H17-alternate-windows.json")).unwrap(),
            b"{\"windows\":[]}"
        );
        assert_eq!(
            fs::read(output.path().join("case-H17-alternate-private.log")).unwrap(),
            b"alternate log tail"
        );
    }

    #[test]
    fn h12_cleanup_steps_continue_after_an_earlier_cleanup_error() {
        let attempts = std::cell::Cell::new(0usize);
        let mut errors = Vec::new();
        attempt_cleanup_step(&mut errors, "runtime", || {
            attempts.set(attempts.get() + 1);
            Err(CaseFailure::new(
                FailureStage::Cleanup,
                "dismiss failed".into(),
            ))
        });
        attempt_cleanup_step(&mut errors, "preview", || {
            attempts.set(attempts.get() + 1);
            Ok(())
        });
        attempt_cleanup_step(&mut errors, "designer", || {
            attempts.set(attempts.get() + 1);
            Err(CaseFailure::new(
                FailureStage::Cleanup,
                "close failed".into(),
            ))
        });
        assert_eq!(attempts.get(), 3);
        assert_eq!(errors.len(), 2);
        assert!(errors[0].contains("runtime: dismiss failed"));
        assert!(errors[1].contains("designer: close failed"));
    }

    fn root_snapshot(
        hwnd: usize,
        process_id: u32,
        visible: bool,
        minimized: bool,
        bounds: [i32; 4],
    ) -> WindowSnapshot {
        WindowSnapshot {
            hwnd: HWND(hwnd as *mut _),
            process_id,
            role: WindowRole::Root,
            class_name: "MultiLauncherRoot".into(),
            visible,
            minimized,
            bounds,
        }
    }

    #[test]
    fn hotkey_setup_failure_records_current_cases_before_r0() {
        let output = tempfile::tempdir().expect("temporary setup-failure artifact directory");
        let trace_path = output.path().join("acceptance.log");
        fs::write(&trace_path, "setup trace").expect("write setup trace");
        let hotkey = AcceptanceHotkey::ShiftAltWinEnd;
        let mut report = AcceptanceReport {
            schema_version: 7,
            run_id: "test-run".into(),
            mode: "native_windows",
            started_unix_ms: 1,
            finished_unix_ms: 2,
            copied_profile_status: super::super::super::CopiedProfileStatus::NotRun,
            copied_profile: None,
            private_artifacts: None,
            h6_repeat_mode: H6RepeatMode::Quiescent,
            mouse_gesture_mode: super::super::super::MouseGestureMode::Enabled,
            suite: AcceptanceSuite::Hotkey,
            hotkey,
            outcome: "failed",
            candidate: super::super::super::CandidateIdentity {
                executable: "candidate.exe".into(),
                sha256: "a".repeat(64),
            },
            environment: super::super::super::EnvironmentIdentity {
                os_version: "test".into(),
                architecture: "x64".into(),
                runner_process_id: 1,
                runner_sha256: Some("b".repeat(64)),
                child_process_id: None,
                child_started_unix_ms: None,
                source_revision: Some("test-revision".into()),
                monitors: Vec::new(),
            },
            profile: super::super::super::ProfileIdentity {
                mode: "test",
                temporary_data_root: "temporary".into(),
                settings_sha256: "c".repeat(64),
                radial_sha256: "d".repeat(64),
                actions_sha256: "e".repeat(64),
                configured_hotkey: hotkey.as_str(),
                hold_threshold_ms: 350,
            },
            cases: Vec::new(),
            hotkey_evidence: Vec::new(),
            artifacts: Vec::new(),
            cleanup: super::super::super::CleanupResult::default(),
            capacity_saturated: false,
            report_overflow: None,
        };

        append_hotkey_setup_failures(
            &mut report,
            "anchor hit-test setup failed".into(),
            None,
            output.path(),
            &trace_path,
            hotkey,
        );

        let expected_ids = HOTKEY_CASE_IDS
            .into_iter()
            .filter(|id| !matches!(*id, "CLEANUP" | "R0"))
            .collect::<Vec<_>>();
        let observed_ids = report
            .cases
            .iter()
            .map(|case| case.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(observed_ids, expected_ids);
        assert!(!observed_ids.contains(&"H7"));
        assert!(!observed_ids.contains(&"H8"));
        assert!(report.cases.iter().all(|case| {
            matches!(case.status, CaseStatus::Failed)
                && matches!(case.failure_stage, Some(FailureStage::Environment))
                && case.observed.contains(hotkey.as_str())
        }));
        assert!(
            report.cases[0]
                .observed
                .contains("anchor hit-test setup failed")
        );

        append_case_without_artifacts(
            &mut report,
            "CLEANUP",
            Ok("child and owned windows cleaned up".into()),
        );
        assert_eq!(report.cases.len() + 1, HOTKEY_CASE_IDS.len());
    }

    #[test]
    fn hotkey_trace_fence_records_completed_setup_baseline_and_cursor() {
        let events = vec![
            "trace_event=\"short_tap\" invocation_id=41 terminal=true".to_string(),
            "trace_event=\"desired_visibility\" visible=false revision=73 source=ToggleBatch invocation_id=41"
                .to_string(),
            "trace_event=\"desired_visibility\" visible=false revision=73 source=Queued invocation_id=none"
                .to_string(),
        ];
        let fence = hotkey_trace_fence_from_events(&events, 9);
        assert_eq!(fence.cursor, events.len());
        assert_eq!(fence.probe_id, 9);
        assert_eq!(fence.baseline_invocation_id, Some(41));
        assert_eq!(fence.baseline_visibility_revision, Some(73));
        assert!(fence.report_token().contains("invocation:41,revision:73"));
    }

    #[test]
    fn delayed_setup_restore_terminal_precedes_target_focus_and_burst_injection() {
        let request =
            "trace_event=\"native_activation\" edge=RestoreRequested request_id=28 terminal=false"
                .to_string();
        let terminal =
            "trace_event=\"native_activation\" edge=RestoreCompleted request_id=28 terminal=true"
                .to_string();
        let poll_count = std::cell::Cell::new(0usize);
        let order = std::cell::RefCell::new(Vec::new());

        let result = run_after_hotkey_setup_settled(
            || {
                order.borrow_mut().push("setup-settle-start");
                let request_id =
                    wait_for_root_restore_after_events(Duration::from_millis(500), || {
                        let poll = poll_count.get() + 1;
                        poll_count.set(poll);
                        if poll < 3 {
                            vec![request.clone()]
                        } else {
                            vec![request.clone(), terminal.clone()]
                        }
                    })
                    .map_err(|error| CaseFailure::new(FailureStage::RootCommand, error))?;
                assert_eq!(request_id, "28");
                order.borrow_mut().push("restore-terminal");
                Ok(())
            },
            || {
                order.borrow_mut().push("target-focus");
                order.borrow_mut().push("burst-injection");
                Ok(())
            },
        );

        assert!(
            result.is_ok(),
            "setup barrier should accept the terminal restore"
        );
        assert!(poll_count.get() >= 3);
        assert_eq!(
            *order.borrow(),
            [
                "setup-settle-start",
                "restore-terminal",
                "target-focus",
                "burst-injection"
            ]
        );
    }

    #[test]
    fn absent_setup_restore_terminal_prevents_target_focus_and_burst_injection() {
        let request =
            "trace_event=\"native_activation\" edge=RestoreRequested request_id=29 terminal=false"
                .to_string();
        let focus_or_injection_called = std::cell::Cell::new(false);

        let result = run_after_hotkey_setup_settled(
            || {
                wait_for_root_restore_after_events(Duration::from_millis(50), || {
                    vec![request.clone()]
                })
                .map(|_| ())
                .map_err(|error| CaseFailure::new(FailureStage::RootCommand, error))
            },
            || {
                focus_or_injection_called.set(true);
                Ok(())
            },
        );

        assert!(matches!(
            result,
            Err(CaseFailure {
                stage: FailureStage::RootCommand,
                ..
            })
        ));
        assert!(!focus_or_injection_called.get());
    }

    #[test]
    fn visible_burst_settle_uses_the_latest_correlated_restore_terminal() {
        let events = vec![
            "trace_event=\"native_activation\" edge=RestoreRequested request_id=8 terminal=false"
                .to_string(),
            "trace_event=\"native_activation\" edge=RestoreRequested request_id=9 terminal=false"
                .to_string(),
            "trace_event=\"native_activation\" edge=RestoreFailed request_id=8 terminal=true"
                .to_string(),
            "trace_event=\"native_activation\" edge=RestoreCompleted request_id=9 terminal=true"
                .to_string(),
        ];

        assert_eq!(
            visible_burst_trace_settled(&events, 1, VisibleBurstSettlePolicy::ActivateRoot,),
            Ok(Some("9".into()))
        );
    }

    #[test]
    fn visible_burst_settle_rejects_failed_or_unfinished_restore() {
        let failed = vec![
            "trace_event=\"native_activation\" edge=RestoreRequested request_id=11 terminal=false"
                .to_string(),
            "trace_event=\"native_activation\" edge=RestoreFailed request_id=11 terminal=true"
                .to_string(),
        ];
        assert!(matches!(
            visible_burst_trace_settled(
                &failed,
                0,
                VisibleBurstSettlePolicy::ActivateRoot,
            ),
            Err(message) if message.contains("request 11")
        ));

        let unfinished = vec![
            "trace_event=\"native_activation\" edge=RestoreRequested request_id=12 terminal=false"
                .to_string(),
        ];
        assert_eq!(
            visible_burst_trace_settled(&unfinished, 0, VisibleBurstSettlePolicy::ActivateRoot,),
            Ok(None)
        );
        assert_eq!(
            visible_burst_trace_settled(&unfinished, 1, VisibleBurstSettlePolicy::ActivateRoot,),
            Ok(None)
        );
    }

    #[test]
    fn preserve_foreground_settle_requires_visible_root_and_retained_target_without_activation() {
        let policy = VisibleBurstSettlePolicy::PreserveForeground {
            target_hwnd: 42,
            target_process_id: 9,
        };
        assert_eq!(
            visible_burst_trace_settled(&[], 0, policy),
            Ok(Some("PreserveForeground".into()))
        );
        assert!(preserved_target_retained_foreground(
            Some(true),
            42,
            9,
            42,
            9
        ));
        assert!(!preserved_target_retained_foreground(
            Some(false),
            42,
            9,
            42,
            9
        ));
        assert!(!preserved_target_retained_foreground(
            Some(true),
            7,
            9,
            42,
            9
        ));
        assert!(!preserved_target_retained_foreground(
            Some(true),
            42,
            8,
            42,
            9
        ));
        assert!(preserved_target_retained_foreground(None, 42, 9, 42, 9));

        let unexpected_activation = vec![
            "trace_event=\"native_activation\" edge=RestoreRequested request_id=13 terminal=false"
                .to_string(),
        ];
        assert!(matches!(
            visible_burst_trace_settled(&unexpected_activation, 0, policy),
            Err(message) if message.contains("unexpected native activation")
        ));
    }

    #[test]
    fn hidden_ending_preserve_foreground_burst_still_rejects_native_activation() {
        let policy = VisibleBurstSettlePolicy::PreserveForeground {
            target_hwnd: 42,
            target_process_id: 9,
        };
        assert_eq!(hotkey_burst_settle_policy(policy, false), Some(policy));
        assert_eq!(
            hotkey_burst_settle_policy(VisibleBurstSettlePolicy::ActivateRoot, false),
            None
        );

        let unexpected_activation = vec![
            "trace_event=\"native_activation\" edge=RestoreRequested request_id=14 terminal=false"
                .to_string(),
        ];
        assert!(matches!(
            visible_burst_trace_settled(&unexpected_activation, 0, policy),
            Err(message) if message.contains("unexpected native activation")
        ));
    }

    #[test]
    fn hotkey_production_admission_requires_exact_hook_and_configured_edges() {
        let missing = Vec::new();
        assert!(
            validate_hotkey_production_admission(&missing, 1)
                .expect_err("observer input without production edges must fail admission")
                .contains("hook_primary edges")
        );

        let hook_only = vec![
            "trace_event=\"hook_primary\" transition=Press provenance=ExternalInjected".into(),
            "trace_event=\"hook_primary\" transition=Release provenance=ExternalInjected".into(),
        ];
        assert!(
            validate_hotkey_production_admission(&hook_only, 1)
                .expect_err("hook callback without configured admission must fail")
                .contains("configured_primary edges")
        );

        let complete = vec![
            "trace_event=\"hook_primary\" transition=Press provenance=ExternalInjected".into(),
            "trace_event=\"configured_primary\" transition=Press provenance=ExternalInjected"
                .into(),
            "trace_event=\"hook_primary\" transition=Release provenance=ExternalInjected".into(),
            "trace_event=\"configured_primary\" transition=Release provenance=ExternalInjected"
                .into(),
        ];
        assert!(validate_hotkey_production_admission(&complete, 1).is_ok());
        let duplicated = [
            complete,
            vec![
                "trace_event=\"hook_primary\" transition=Press provenance=ExternalInjected".into(),
            ],
        ]
        .concat();
        assert!(
            validate_hotkey_production_admission(&duplicated, 1)
                .expect_err("extra production edge must be reported")
                .contains("observed 3")
        );
    }

    #[test]
    fn hotkey_trace_quiet_signature_ignores_non_gesture_events() {
        let before = vec![
            "trace_event=\"root_command\" command=Show".to_string(),
            "trace_event=\"desired_visibility\" visible=false revision=2 source=Queued invocation_id=none"
                .to_string(),
        ];
        let after = vec![
            "trace_event=\"root_command\" command=Focus".to_string(),
            before[1].clone(),
        ];
        assert_eq!(
            hotkey_trace_activity_signature(&before),
            hotkey_trace_activity_signature(&after)
        );
        assert_ne!(
            hotkey_trace_activity_signature(&before),
            hotkey_trace_activity_signature(&[
                after[1].clone(),
                "trace_event=\"desired_visibility\" visible=true revision=3 source=ToggleBatch invocation_id=2"
                    .into(),
            ])
        );
    }

    #[test]
    fn root_visibility_uses_owned_physical_presentation_predicate() {
        let displays = [[0, 0, 1920, 1080], [2200, 0, 4120, 1440]];
        let hidden_in_place = root_snapshot(10, 42, false, false, [40, 60, 940, 710]);
        assert!(root_matches_requested_visibility(
            &hidden_in_place,
            HWND(10 as *mut _),
            42,
            false,
            &displays,
        ));
        assert!(!root_matches_requested_visibility(
            &hidden_in_place,
            HWND(10 as *mut _),
            42,
            true,
            &displays,
        ));

        let minimized = root_snapshot(10, 42, true, true, [40, 60, 940, 710]);
        assert!(root_matches_requested_visibility(
            &minimized,
            HWND(10 as *mut _),
            42,
            false,
            &displays,
        ));

        let in_monitor_gap = root_snapshot(10, 42, true, false, [1960, 40, 2160, 240]);
        assert!(root_matches_requested_visibility(
            &in_monitor_gap,
            HWND(10 as *mut _),
            42,
            false,
            &displays,
        ));

        let visible = root_snapshot(10, 42, true, false, [2240, 40, 3140, 690]);
        assert!(root_matches_requested_visibility(
            &visible,
            HWND(10 as *mut _),
            42,
            true,
            &displays,
        ));
        assert!(!root_matches_requested_visibility(
            &visible,
            HWND(11 as *mut _),
            42,
            true,
            &displays,
        ));
        assert!(!root_matches_requested_visibility(
            &visible,
            HWND(10 as *mut _),
            43,
            true,
            &displays,
        ));
    }

    #[test]
    fn hold_ignores_delayed_setup_visibility_echo_but_rejects_new_decisions() {
        let delayed_setup = vec![
            "trace_event=\"desired_visibility\" revision=1 source=Queued invocation_id=none".into(),
        ];
        assert!(!trace_contains_hold_visibility_work(
            &delayed_setup,
            Some(1),
            Some(2)
        ));

        let new_revision = vec![
            "trace_event=\"desired_visibility\" revision=2 source=Queued invocation_id=none".into(),
        ];
        assert!(trace_contains_hold_visibility_work(
            &new_revision,
            Some(1),
            Some(2)
        ));

        let hold_invocation = vec![
            "trace_event=\"desired_visibility\" revision=1 source=ToggleBatch invocation_id=2"
                .into(),
        ];
        assert!(trace_contains_hold_visibility_work(
            &hold_invocation,
            Some(1),
            Some(2)
        ));
    }

    fn spacer_from(
        template: &multi_launcher::radial::model::CellDefinition,
        id: String,
    ) -> multi_launcher::radial::model::CellDefinition {
        let mut cell = template.clone();
        cell.id = multi_launcher::radial::model::CellId::new(id);
        cell.label = "Spacer".into();
        cell.content = multi_launcher::radial::model::CellContent::Spacer;
        cell.alternate_clicks.clear();
        cell.alternate_controls.clear();
        cell.shortcuts.clear();
        cell.hotstrings.clear();
        cell
    }

    fn write_saved_graph_fixture(
        profile: &Path,
    ) -> (PersistedMenuGraphExpectation, PersistedMenuGraphExpectation) {
        use multi_launcher::radial::model::{
            ActionBinding, AfterActionPolicy, CellContent, MenuId, Override, RadialDocument, RingId,
        };
        use multi_launcher::universal_actions::{
            PersistableActionTargetRef, PersistedUniversalActionRef, action_ids,
        };

        let mut document = RadialDocument::starter();
        let target_action_index = ACCEPTANCE_TARGET_ACTION_INDEX;
        let target_action = multi_launcher::actions::Action {
            label: format!("Radial Acceptance Harmless Action {target_action_index:03}"),
            desc: "Deterministic native authoring fixture".into(),
            action: format!("radial_acceptance_harmless_{target_action_index:03}"),
            args: None,
        };

        let root = &mut document.menus[0];
        let mut root_first = root.rings[0].clone();
        root_first.id = RingId::new("starter-main-after-overflow");
        let moved = root_first
            .cells
            .pop()
            .expect("starter root should have an overflow candidate");
        let mut root_overflow = root_first.clone();
        root_overflow.id = RingId::new("starter-overflow");
        root_overflow.radius = root_first.radius + 80.0;
        root_overflow.cells = vec![moved];
        root.rings = vec![root_first.clone(), root_overflow];

        let mut authored = root.clone();
        authored.id = MenuId::new("radial-acceptance-authored-menu");
        authored.name = "Radial acceptance authored graph".into();
        authored.after_action = AfterActionPolicy::CloseTree;
        let mut inner = root_first.clone();
        inner.id = RingId::new("radial-acceptance-inner");
        inner.cells = (0..8)
            .map(|index| {
                spacer_from(
                    &root_first.cells[0],
                    format!("radial-acceptance-inner-cell-{index}"),
                )
            })
            .collect();
        let mut outer = root_first.clone();
        outer.id = RingId::new("radial-acceptance-outer");
        outer.radius = inner.radius + 80.0;
        outer.cells = (0..10)
            .map(|index| {
                spacer_from(
                    &root_first.cells[0],
                    format!("radial-acceptance-outer-cell-{index}"),
                )
            })
            .collect();
        outer.cells[0].content = CellContent::Action {
            binding: ActionBinding::Persisted {
                action: PersistedUniversalActionRef {
                    target: Some(PersistableActionTargetRef::CustomAction {
                        action: target_action.clone(),
                    }),
                    action_id: action_ids::RESULT_EXECUTE,
                },
            },
        };
        authored.rings = vec![inner, outer];
        let authored_menu_index = document.menus.len();
        document.menus.push(authored);
        document.skins[0].style.values.effects.glow_enabled = Override::Value(false);

        fs::write(
            profile.join("radial.json"),
            serde_json::to_vec_pretty(&document).expect("serialize saved graph fixture"),
        )
        .expect("write saved graph fixture");
        fs::write(
            profile.join("actions.json"),
            serde_json::to_vec_pretty(&vec![target_action; target_action_index + 1])
                .expect("serialize action fixture"),
        )
        .expect("write action fixture");

        (
            PersistedMenuGraphExpectation {
                menu_index: authored_menu_index,
                ring_slots: vec![8, 10],
                populated_cells: 0,
                cell_ids_digest: 0,
            },
            PersistedMenuGraphExpectation {
                menu_index: 0,
                ring_slots: vec![8, 1],
                populated_cells: 9,
                cell_ids_digest: 0,
            },
        )
    }

    #[test]
    fn post_apply_cell_id_check_requires_available_equal_candidate_digest() {
        assert!(committed_cell_ids_match(true, 41, 41));
        assert!(!committed_cell_ids_match(true, 41, 42));
        assert!(!committed_cell_ids_match(false, 41, 41));
    }

    #[test]
    fn canvas_cell_index_maps_outer_ring_slot_to_flat_menu_index() {
        assert_eq!(flat_canvas_cell_index(&[8, 10], 0, 0), Some(0));
        assert_eq!(flat_canvas_cell_index(&[8, 10], 1, 0), Some(8));
        assert_eq!(flat_canvas_cell_index(&[8, 10], 1, 9), Some(17));
        assert_eq!(flat_canvas_cell_index(&[8, 10], 1, 10), None);
        assert_eq!(flat_canvas_cell_index(&[8, 10], 2, 0), None);
    }

    #[test]
    fn typed_save_oracle_requires_authored_geometry_and_g1_overflow_graph() {
        let profile = tempfile::tempdir().expect("temporary profile");
        let (authored, root) = write_saved_graph_fixture(profile.path());
        let saved = verify_saved_authoring_fixture(
            profile.path(),
            &authored,
            Some(&root),
            0,
            ACCEPTANCE_TARGET_ACTION_INDEX,
            0,
            false,
            None,
        )
        .expect("typed saved graph should retain both authored menus");
        assert!(saved.contains("[8, 10]"));
        assert!(saved.contains("[8, 1]"));
        assert!(saved.contains("9 populated cells"));

        let mut wrong_root = root.clone();
        wrong_root.ring_slots = vec![8, 7];
        let error = verify_saved_authoring_fixture(
            profile.path(),
            &authored,
            Some(&wrong_root),
            0,
            ACCEPTANCE_TARGET_ACTION_INDEX,
            0,
            false,
            None,
        )
        .expect_err("the oracle must reject a lost/changed overflow ring");
        assert!(error.contains("G1 root graph"));
    }

    #[test]
    fn leaf_side_effect_oracle_uses_a_strict_pre_a3_history_and_trace_baseline() {
        let profile = tempfile::tempdir().expect("temporary profile");
        let history_path = profile.path().join("history.json");
        fs::write(&history_path, b"[]").expect("write initial history");
        let trace_path = profile.path().join("acceptance.log");
        fs::write(
            &trace_path,
            "WARN target trace_event=\"trace_ready\" elapsed_ms=0\n",
        )
        .expect("write pre-A3 trace");
        let baseline = capture_action_side_effect_baseline(profile.path(), &trace_path)
            .expect("capture history and trace baseline");

        fs::OpenOptions::new()
            .append(true)
            .open(&trace_path)
            .expect("open trace append")
            .write_all(
                b"WARN target trace_event=\"designer_widget\" elapsed_ms=2\nWARN target trace_event=\"native_preview_dispatch_count\" elapsed_ms=3 count=0\n",
            )
            .expect("append harmless design events");
        verify_no_leaf_side_effects(profile.path(), &trace_path, &baseline)
            .expect("zero-dispatch design interaction should pass");

        fs::OpenOptions::new()
            .append(true)
            .open(&trace_path)
            .expect("open trace append")
            .write_all(b"WARN target trace_event=\"radial_action\" elapsed_ms=4\n")
            .expect("append real action dispatch");
        assert!(
            verify_no_leaf_side_effects(profile.path(), &trace_path, &baseline)
                .expect_err("real action dispatch must fail")
                .contains("real radial leaf action")
        );

        fs::write(&trace_path, "WARN target trace_event=\"trace_ready\" elapsed_ms=0\nWARN target trace_event=\"native_preview_dispatch_count\" elapsed_ms=3 count=1\n")
            .expect("rewrite trace with nonzero native dispatch");
        assert!(
            verify_no_leaf_side_effects(profile.path(), &trace_path, &baseline)
                .expect_err("nonzero native dispatch counter must fail")
                .contains("nonzero native preview")
        );

        fs::write(
            &trace_path,
            "WARN target trace_event=\"trace_ready\" elapsed_ms=0\n",
        )
        .expect("restore trace event count");
        fs::write(&history_path, b"[\"changed\"]").expect("mutate history");
        assert!(
            verify_no_leaf_side_effects(profile.path(), &trace_path, &baseline)
                .expect_err("history mutation must fail")
                .contains("history file")
        );
    }

    #[test]
    fn blocked_designer_and_tail_fill_preserve_g0_by_case_id() {
        assert!(BLOCKED_DESIGNER_CASE_IDS.contains(&"G0"));
        let prior_results = CASE_IDS
            .into_iter()
            .filter(|id| *id != "G0")
            .collect::<Vec<_>>();
        assert_eq!(prior_results.len(), CASE_IDS.len() - 1);
        assert_eq!(missing_case_ids(&prior_results), vec!["G0"]);
    }

    #[test]
    fn designer_semantic_targets_are_scoped_to_the_reopened_session() {
        let lines = vec![
            "trace_event=\"designer_semantic_target\" target=Menus role=\"SelectableLabel\" viewport=Deferred left_px=10 top_px=20 right_px=40 bottom_px=44 selected=true focused=false session_id=1 generation=7".to_string(),
            "trace_event=\"designer_semantic_target\" target=Menus role=\"SelectableLabel\" viewport=Deferred left_px=10 top_px=20 right_px=40 bottom_px=44 selected=false focused=false session_id=2 generation=2".to_string(),
        ];

        let reopened =
            latest_designer_semantic_target(&lines, DesignerSemanticTarget::Menus, Some(2))
                .expect("reopened Designer Menus target should be present");
        assert_eq!(reopened.session_id, 2);
        assert_eq!(reopened.generation, 2);
        assert!(!reopened.selected);

        let earlier =
            latest_designer_semantic_target(&lines, DesignerSemanticTarget::Menus, Some(1))
                .expect("earlier Designer Menus target should remain addressable");
        assert_eq!(earlier.session_id, 1);
        assert_eq!(earlier.generation, 7);
        assert!(earlier.selected);
    }

    #[test]
    fn checked_designer_toggle_requires_same_session_generation_and_widget_acceptance() {
        let make_events = |widget_session: u64, target_generation: u64| {
            vec![
                "trace_event=\"designer_pointer\" pointer_down=true session_id=4 generation=7"
                    .to_string(),
                "trace_event=\"designer_pointer\" pointer_up=true session_id=4 generation=7"
                    .to_string(),
                "trace_event=\"designer_body\" state=Enabled session_id=4 generation=7".to_string(),
                format!(
                    "trace_event=\"designer_widget\" category=Tree response=Accepted session_id={widget_session} generation=7"
                ),
                format!(
                    "trace_event=\"designer_semantic_target\" target=Tree role=\"SelectableLabel\" viewport=Deferred left_px=10 top_px=20 right_px=40 bottom_px=44 selected=false focused=false session_id=4 generation={target_generation}"
                ),
            ]
        };
        let baseline = DesignerSemanticTargetState {
            bounds: [10, 20, 40, 44],
            selected: true,
            focused: false,
            session_id: 4,
            generation: 7,
        };

        assert!(
            checked_designer_toggle_transition(
                &make_events(4, 7),
                DesignerSemanticTarget::Tree,
                baseline,
                false,
            )
            .is_some()
        );
        assert!(
            checked_designer_toggle_transition(
                &make_events(5, 7),
                DesignerSemanticTarget::Tree,
                baseline,
                false,
            )
            .is_none()
        );
        assert!(
            checked_designer_toggle_transition(
                &make_events(4, 8),
                DesignerSemanticTarget::Tree,
                baseline,
                false,
            )
            .is_none()
        );
    }

    #[test]
    fn disposable_close_oracle_correlates_cancel_and_prompt_to_one_request_session() {
        let identity = AuthoringRequestIdentity {
            request_id: 41,
            generation: 9,
            session_id: 7,
        };
        let sent = "trace_event=\"authoring\" edge=RequestSent request_kind=PrepareEmbeddedPreview request_id=41 generation=9 session_id=7 terminal=false";
        let cancelled = "trace_event=\"disposable_request_cancelled\" request_kind=PrepareEmbeddedPreview request_id=41 generation=9 session_id=7";
        let prompt = "trace_event=\"designer_close\" session_id=7 open=true close_prompt=true dirty=true pending_disposable=false";
        let other_session_prompt = "trace_event=\"designer_close\" session_id=8 open=true close_prompt=true dirty=true pending_disposable=false";

        assert_eq!(
            parse_authoring_request_identity(sent, 7, "PrepareEmbeddedPreview"),
            Some(identity)
        );
        assert!(disposable_cancel_matches(
            cancelled,
            identity,
            "PrepareEmbeddedPreview"
        ));
        assert!(designer_close_matches(prompt, 7, true, true));
        assert!(!designer_close_matches(other_session_prompt, 7, true, true));
        assert!(acceptance_prepare_gate_matches(
            "trace_event=\"acceptance_prepare_gate\" edge=Held request_kind=PrepareEmbeddedPreview request_id=41 generation=9 session_id=7",
            identity,
            "Held"
        ));
        assert!(authoring_edge_matches(
            "trace_event=\"authoring\" edge=ReplyRejected request_kind=PrepareEmbeddedPreview request_id=41 generation=9 session_id=7 terminal=false",
            identity,
            "PrepareEmbeddedPreview",
            "ReplyRejected"
        ));
    }

    #[test]
    fn designer_preview_render_trace_is_sanitized_with_only_typed_identity() {
        let excerpt = safe_trace_excerpt(
            "WARN target trace_event=\"designer_preview_rendered\" elapsed_ms=12 session_id=7 generation=9 menu_cell_ids_digest=123 private_title=secret",
        );
        assert_eq!(
            excerpt,
            "trace_event=designer_preview_rendered elapsed_ms=12 session_id=7 generation=9 menu_cell_ids_digest=123"
        );
        let gate = safe_trace_excerpt(
            "WARN target trace_event=\"acceptance_prepare_gate\" elapsed_ms=13 edge=Held request_kind=PrepareEmbeddedPreview request_id=41 generation=9 session_id=7 private_path=C:\\private",
        );
        assert_eq!(
            gate,
            "trace_event=acceptance_prepare_gate elapsed_ms=13 edge=Held request_kind=PrepareEmbeddedPreview request_id=41 generation=9 session_id=7"
        );
        assert!(!gate.contains("private"));
    }

    #[test]
    fn semantic_control_bounds_must_be_current_inside_the_native_client() {
        let client = [240, 180, 1_156, 869];
        assert!(semantic_bounds_center_inside([251, 189, 283, 207], client));
        assert!(!semantic_bounds_center_inside(
            [2_011, 2_033, 2_043, 2_051],
            client
        ));
        assert!(!semantic_bounds_center_inside([0, 0, 0, 10], client));
    }

    #[test]
    fn root_pointer_geometry_requires_intersection_with_a_physical_display() {
        let displays = [[0, 0, 1_920, 1_080], [-1_280, 0, 0, 1_024]];
        assert!(intersects_display_bounds(
            [1_800, 900, 2_000, 1_100],
            &displays
        ));
        assert!(intersects_display_bounds(
            [-1_300, 100, -1_200, 200],
            &displays
        ));
        assert!(!intersects_display_bounds(
            [2_000, 2_000, 2_900, 2_700],
            &displays
        ));
        assert!(!intersects_display_bounds(
            [1_920, 0, 2_000, 100],
            &displays
        ));
    }

    #[test]
    fn failure_trace_excerpt_keeps_typed_fields_and_drops_private_unknown_fields() {
        let excerpt = safe_trace_excerpt(
            "WARN target trace_event=\"authoring\" elapsed_ms=12 edge=ReplyAccepted request_kind=Snapshot secret_marker=NeverPublish label=\"private action title\"\nWARN target trace_event=\"not_a_schema_event\" token=private",
        );
        assert_eq!(
            excerpt,
            "trace_event=authoring elapsed_ms=12 edge=ReplyAccepted request_kind=Snapshot"
        );
        assert!(!excerpt.contains("secret_marker"));
        assert!(!excerpt.contains("private action title"));
        assert!(!excerpt.contains("not_a_schema_event"));
    }

    #[test]
    fn failure_trace_excerpt_keeps_numeric_canvas_scope_without_text_identity() {
        let excerpt = safe_trace_excerpt(
            "WARN target trace_event=\"designer_authoring_control\" elapsed_ms=9 menu_cell_ids_digest=123456 cell_ring_index=1 cell_slot_index=0 private_action=secret",
        );
        assert_eq!(
            excerpt,
            "trace_event=designer_authoring_control elapsed_ms=9 menu_cell_ids_digest=123456 cell_ring_index=1 cell_slot_index=0"
        );
        assert!(!excerpt.contains("private_action"));
    }

    #[test]
    fn failure_trace_excerpt_keeps_typed_authoring_focus_state() {
        let excerpt = safe_trace_excerpt(
            "WARN target trace_event=\"designer_authoring_control\" elapsed_ms=9 focused=true session_id=4 generation=5 private_label=secret",
        );
        assert_eq!(
            excerpt,
            "trace_event=designer_authoring_control elapsed_ms=9 focused=true session_id=4 generation=5"
        );
        assert!(!excerpt.contains("private_label"));

        let malformed = safe_trace_excerpt(
            "WARN target trace_event=\"designer_authoring_control\" elapsed_ms=9 focused=private session_id=4 generation=5",
        );
        assert_eq!(
            malformed,
            "trace_event=designer_authoring_control elapsed_ms=9 session_id=4 generation=5"
        );
    }

    #[test]
    fn failure_trace_excerpt_retains_designer_close_state_transition() {
        let excerpt = safe_trace_excerpt(
            "WARN target trace_event=\"designer_close\" elapsed_ms=7 open=true close_prompt=false dirty=true pending_disposable=false pending_durable=false pending_native_preview=false private_note=unpublished",
        );
        assert_eq!(
            excerpt,
            "trace_event=designer_close elapsed_ms=7 open=true close_prompt=false dirty=true pending_disposable=false pending_durable=false pending_native_preview=false"
        );
        assert!(!excerpt.contains("private_note"));
    }

    #[test]
    fn failure_trace_excerpt_keeps_startup_and_late_root_parking_context() {
        let mut trace =
            String::from("WARN target trace_event=\"trace_ready\" elapsed_ms=0 event_budget=256\n");
        for elapsed_ms in 1..600 {
            trace.push_str(&format!(
                "WARN target trace_event=\"authoring\" elapsed_ms={elapsed_ms} edge=ReplyAccepted request_kind=Snapshot\n"
            ));
        }
        trace.push_str(
            "WARN target trace_event=\"root_command\" elapsed_ms=600 command=ParkingBoundary request_id=1 request_kind=Snapshot session_id=0 generation=0 terminal=true\n",
        );

        let excerpt = safe_trace_excerpt(&trace);
        let lines = excerpt.lines().collect::<Vec<_>>();
        assert!(lines.len() <= MAX_TRACE_EXCERPT);
        assert!(excerpt.len() <= MAX_TRACE_BYTES);
        assert!(lines[0].contains("trace_event=trace_ready"));
        assert!(
            lines
                .last()
                .is_some_and(|line| line.contains("command=ParkingBoundary"))
        );
        assert!(excerpt.contains("elapsed_ms=600"));
        assert!(!excerpt.contains("elapsed_ms=100 "));

        let private_command = sanitize_trace_line(
            "WARN target trace_event=\"root_command\" elapsed_ms=601 command=private_action_name",
        )
        .expect("known event should be retained even if its command value is unknown");
        assert!(!private_command.contains("command="));
    }

    #[test]
    fn file_menu_body_trace_is_sanitized_with_only_typed_state() {
        let line = sanitize_trace_line(
            "WARN target trace_event=\"root_menu_body\" elapsed_ms=17 menu=File entered=true private_label=secret",
        )
        .expect("typed File body trace should be retained");
        assert_eq!(
            line,
            "trace_event=root_menu_body elapsed_ms=17 menu=File entered=true"
        );
        assert!(!line.contains("secret"));
    }

    #[test]
    fn root_menu_state_uses_production_open_transitions_not_uia_subtree_presence() {
        let open_trace = vec![
            "trace_event=\"trace_ready\" elapsed_ms=0".to_string(),
            "trace_event=\"root_menu_interaction\" menu=File hovered=true clicked=true open=true"
                .to_string(),
            "trace_event=\"root_menu_interaction\" menu=Apps hovered=true clicked=true open=true"
                .to_string(),
        ];
        assert_eq!(
            root_menu_state_from_trace(&open_trace),
            Some(RootMenuState {
                file_open: true,
                apps_open: true
            })
        );

        let closed_trace = open_trace
            .into_iter()
            .chain([
                "trace_event=\"root_menu_interaction\" menu=Apps hovered=false clicked=false open=false"
                    .to_string(),
                "trace_event=\"root_menu_interaction\" menu=File hovered=false clicked=false open=false"
                    .to_string(),
            ])
            .collect::<Vec<_>>();
        assert_eq!(
            root_menu_state_from_trace(&closed_trace),
            Some(RootMenuState::default())
        );

        let exhausted = vec![
            "trace_event=\"trace_ready\" elapsed_ms=0".to_string(),
            "trace_event=\"budget_exhausted\" elapsed_ms=100 event_budget=4096".to_string(),
        ];
        assert_eq!(root_menu_state_from_trace(&exhausted), None);
    }

    #[test]
    fn acknowledged_file_click_retries_only_while_production_trace_keeps_menu_closed() {
        let click_closed = vec![
            "trace_event=\"trace_ready\" elapsed_ms=0".to_string(),
            "trace_event=\"root_menu_interaction\" menu=File hovered=true clicked=true open=false"
                .to_string(),
            "trace_event=\"root_menu_interaction\" menu=File hovered=true clicked=false open=false"
                .to_string(),
        ];
        let closed = root_menu_state_from_trace(&click_closed).expect("closed menu state");
        assert_eq!(closed, RootMenuState::default());
        assert!(should_retry_root_file_menu_open(closed));

        let open_file = click_closed
            .into_iter()
            .chain([
                "trace_event=\"root_menu_interaction\" menu=File hovered=true clicked=true open=true"
                    .to_string(),
            ])
            .collect::<Vec<_>>();
        let open = root_menu_state_from_trace(&open_file).expect("open File menu state");
        assert!(root_file_menu_ready_for_apps(open));
        assert!(!should_retry_root_file_menu_open(open));

        let open_apps = open_file
            .into_iter()
            .chain([
                "trace_event=\"root_menu_interaction\" menu=Apps hovered=true clicked=true open=true"
                    .to_string(),
            ])
            .collect::<Vec<_>>();
        let nested = root_menu_state_from_trace(&open_apps).expect("open Apps menu state");
        assert!(!root_file_menu_ready_for_apps(nested));
        assert!(!should_retry_root_file_menu_open(nested));
    }

    #[test]
    fn root_menu_body_trace_proves_rendered_popup_when_button_trace_disagrees() {
        let file_body = vec![
            "trace_event=\"trace_ready\" elapsed_ms=0".to_string(),
            "trace_event=\"root_menu_interaction\" menu=File hovered=true clicked=true open=false"
                .to_string(),
            "trace_event=\"root_menu_body\" menu=File entered=true".to_string(),
        ];
        let state = root_menu_state_from_trace(&file_body).expect("File closure state");
        assert_eq!(
            state,
            RootMenuState {
                file_open: true,
                apps_open: false
            }
        );

        let closed = file_body
            .into_iter()
            .chain(["trace_event=\"root_menu_body\" menu=File entered=false".to_string()])
            .collect::<Vec<_>>();
        assert_eq!(
            root_menu_state_from_trace(&closed),
            Some(RootMenuState::default())
        );
    }

    fn window(hwnd: usize, process_id: u32, role: WindowRole, active: bool) -> WindowSnapshot {
        let class_name = match role {
            WindowRole::OtherChild => RADIAL_HOST_WINDOW_CLASS,
            WindowRole::Root => "MultiLauncherRoot",
            WindowRole::Designer => "RadialDesigner",
        };
        window_with_class(hwnd, process_id, role, class_name, active)
    }

    fn window_with_class(
        hwnd: usize,
        process_id: u32,
        role: WindowRole,
        class_name: &str,
        active: bool,
    ) -> WindowSnapshot {
        let (left, top, width, height) = super::super::virtual_screen_bounds();
        WindowSnapshot {
            hwnd: windows::Win32::Foundation::HWND(hwnd as *mut std::ffi::c_void),
            process_id,
            role,
            class_name: class_name.into(),
            visible: active,
            minimized: false,
            bounds: [
                left,
                top,
                left.saturating_add(width),
                top.saturating_add(height),
            ],
        }
    }

    #[test]
    fn radial_surface_set_requires_all_exact_child_surfaces_to_transition() {
        let radial = [
            window(101, 44, WindowRole::OtherChild, true),
            window(102, 44, WindowRole::OtherChild, true),
        ];
        let both_active = radial.to_vec();
        assert!(radial_surface_set_matches_active_state(
            &radial,
            &both_active,
            44,
            true
        ));
        let with_auxiliary = vec![
            radial[0].clone(),
            radial[1].clone(),
            window_with_class(103, 44, WindowRole::OtherChild, "ApplicationDialog", true),
        ];
        assert!(!is_radial_surface(&with_auxiliary[2], 44));
        assert!(radial_surface_set_matches_active_state(
            &radial,
            &with_auxiliary,
            44,
            true
        ));
        assert!(!radial_surface_set_matches_active_state(
            &radial,
            &both_active,
            44,
            false
        ));

        let one_active = vec![
            radial[0].clone(),
            window(102, 44, WindowRole::OtherChild, false),
        ];
        assert!(!radial_surface_set_matches_active_state(
            &radial,
            &one_active,
            44,
            true
        ));
        assert!(!radial_surface_set_matches_active_state(
            &radial,
            &one_active,
            44,
            false
        ));

        let both_closed = vec![window(102, 44, WindowRole::OtherChild, false)];
        assert!(radial_surface_set_matches_active_state(
            &radial,
            &both_closed,
            44,
            false
        ));
        assert!(radial_surface_set_and_owner_are_inactive(
            &radial,
            &both_closed,
            44
        ));
        let untracked_radial_open = vec![
            window(102, 44, WindowRole::OtherChild, false),
            window(103, 44, WindowRole::OtherChild, true),
        ];
        assert!(!radial_surface_set_and_owner_are_inactive(
            &radial,
            &untracked_radial_open,
            44
        ));

        let reused_by_other_process = vec![window(101, 55, WindowRole::OtherChild, true)];
        assert!(radial_surface_set_matches_active_state(
            &radial,
            &reused_by_other_process,
            44,
            false
        ));
    }

    #[test]
    fn radial_surface_state_keeps_root_identity_and_bounds_separate() {
        let radial = [
            window(101, 44, WindowRole::OtherChild, true),
            window(102, 44, WindowRole::OtherChild, true),
        ];
        let with_root = vec![
            radial[0].clone(),
            radial[1].clone(),
            window(100, 44, WindowRole::Root, true),
        ];
        assert!(radial_surface_set_matches_active_state(
            &radial, &with_root, 44, true
        ));
        assert!(!radial_surface_set_matches_active_state(
            &[radial[0].clone(), window(100, 44, WindowRole::Root, true)],
            &with_root,
            44,
            true
        ));

        let root_before = window(100, 44, WindowRole::Root, true);
        let mut root_after = root_before.clone();
        assert!(same_window_state(&root_before, &root_after));
        root_after.bounds[0] = root_after.bounds[0].saturating_add(1);
        assert!(!same_window_state(&root_before, &root_after));
    }

    #[test]
    fn hotkey_fixture_startup_requires_the_configured_physical_root_placement() {
        let displays = [[0, 0, 1920, 1080]];
        let ready = root_snapshot(100, 44, true, false, [240, 180, 1156, 869]);
        assert!(hotkey_fixture_root_is_ready(
            &ready,
            ready.hwnd,
            ready.process_id,
            &displays
        ));

        // A child can have a valid ROOT HWND before its configured viewport placement
        // arrives. A physically visible default-position window is not startup-ready.
        let before_placement = root_snapshot(100, 44, true, false, [800, 150, 1716, 839]);
        assert!(!hotkey_fixture_root_is_ready(
            &before_placement,
            before_placement.hwnd,
            before_placement.process_id,
            &displays
        ));
        let parked = root_snapshot(100, 44, true, false, [2000, 2000, 2916, 2689]);
        assert!(!hotkey_fixture_root_is_ready(
            &parked,
            parked.hwnd,
            parked.process_id,
            &displays
        ));
    }
}
