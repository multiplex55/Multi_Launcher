//! Opt-in native Windows acceptance runner for ROOT and the Radial Designer.

use multi_launcher::common::persistence::LoadState;
use multi_launcher::radial::model::{ActionBinding, AfterActionPolicy, CellContent};
use multi_launcher::radial::{
    RadialDocument, settings as radial_settings, validation::validate as validate_radial_document,
};
use multi_launcher::settings::{
    LogFile, Settings, SubmenuMigrationState, SubmenuPresentationMigrationReceipt,
};
use multi_launcher::universal_actions::{
    PersistableActionTargetRef, PersistedUniversalActionRef, action_ids,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[path = "radial_acceptance/copied_profile.rs"]
mod copied_profile;
#[cfg(windows)]
#[path = "radial_acceptance/native.rs"]
mod native;
#[path = "radial_acceptance/private_artifacts.rs"]
mod private_artifacts;

const MAX_CASES: usize = 48;
const MAX_ARTIFACTS: usize = 128;
const MAX_PATH_BYTES: usize = 2_048;
const MAX_RESULT_BYTES: usize = 2_048;
const MAX_JSON_REPORT_BYTES: usize = 512 * 1024;
const MAX_TEXT_REPORT_BYTES: usize = 256 * 1024;
const MAX_HOTKEY_EVIDENCE_EDGES: usize = 1_024;
const MAX_HOTKEY_EVIDENCE_EVENTS: usize = 2_200;
const MAX_HOTKEY_EVIDENCE_GESTURES: usize = 128;
const MAX_HOTKEY_EVIDENCE_CASES: usize = 16;
const MAX_HOTKEY_CASE_EVIDENCE_BYTES: usize = 256 * 1024;
const ACCEPTANCE_HOTKEY: &str = "F11";
const HOTKEY_CASE_IDS: [&str; 15] = [
    "H01", "H02", "H04", "H06", "H07", "H08", "H09", "H10", "H11", "H12", "H16", "H17", "H18",
    "CLEANUP", "R0",
];
const ACCEPTANCE_ACTION_COUNT: usize = 64;
const ACCEPTANCE_TARGET_ACTION_INDEX: usize = ACCEPTANCE_ACTION_COUNT - 1;
pub(crate) const CASE_IDS: [&str; 32] = [
    "H0", "H1", "H2", "H3", "H4", "H5", "H6", "D0", "D1", "D2", "D4", "D5", "A0", "A1", "G0", "A2",
    "G1", "G2", "A3", "A4", "A5", "A6", "A7", "A8", "D3", "D6", "D7", "H7", "H8", "R0", "R1", "R2",
];
const COPIED_CASE_IDS: [&str; 28] = [
    "CP_PREFLIGHT",
    "CP_H0",
    "CP_H1",
    "CP_H2",
    "CP_H3",
    "CP_D0",
    "CP_R1",
    "CP_D1",
    "CP_D2",
    "CP_D4",
    "CP_D5",
    "CP_A0",
    "CP_A1",
    "CP_G0",
    "CP_A2",
    "CP_A3",
    "CP_A4",
    "CP_A5",
    "CP_A6",
    "CP_D3",
    "CP_A7",
    "CP_A8",
    "CP_D6",
    "CP_D7",
    "CP_SOURCE_INTEGRITY",
    "R0",
    "R2",
    "CLEANUP",
];

#[derive(Clone, Debug)]
struct Arguments {
    launcher: Option<PathBuf>,
    output: Option<PathBuf>,
    report_file: Option<PathBuf>,
    profile_copy: Option<PathBuf>,
    source_revision: Option<String>,
    keep_profile_on_failure: bool,
    h6_repeat_mode: H6RepeatMode,
    mouse_gesture_mode: MouseGestureMode,
    suite: AcceptanceSuite,
    hotkey: AcceptanceHotkey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum AcceptanceSuite {
    All,
    Hotkey,
}

impl AcceptanceSuite {
    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Hotkey => "hotkey",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum AcceptanceHotkey {
    F11,
    ShiftAltWinEnd,
}

impl AcceptanceHotkey {
    fn as_str(self) -> &'static str {
        match self {
            Self::F11 => ACCEPTANCE_HOTKEY,
            Self::ShiftAltWinEnd => "Shift+Alt+Win+End",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum H6RepeatMode {
    Immediate,
    Quiescent,
    ProductionOnlyDiagnostic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum MouseGestureMode {
    Enabled,
    DisabledDiagnostic,
}

impl MouseGestureMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::DisabledDiagnostic => "disabled_diagnostic",
        }
    }
}

impl H6RepeatMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::Quiescent => "quiescent",
            Self::ProductionOnlyDiagnostic => "production_only_diagnostic",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum CaseStatus {
    Passed,
    Failed,
    Skipped,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum FailureStage {
    Environment,
    CandidateStartup,
    WindowDiscovery,
    InputInjection,
    HookAdmission,
    GestureDecision,
    RootCommand,
    NativeRootState,
    DesignerEntry,
    DesignerNativeTarget,
    DesignerFrameworkInput,
    DesignerReadiness,
    DesignerWidget,
    DesignerMutation,
    DesignerPresentation,
    Cleanup,
}

#[derive(Clone, Serialize)]
struct CandidateIdentity {
    executable: String,
    sha256: String,
}

#[derive(Clone, Serialize)]
struct MonitorIdentity {
    id: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale_factor: f32,
}

#[derive(Clone, Serialize)]
struct EnvironmentIdentity {
    os_version: String,
    architecture: String,
    runner_process_id: u32,
    runner_sha256: Option<String>,
    child_process_id: Option<u32>,
    child_started_unix_ms: Option<u128>,
    source_revision: Option<String>,
    monitors: Vec<MonitorIdentity>,
}

#[derive(Clone, Serialize)]
struct ProfileIdentity {
    mode: &'static str,
    temporary_data_root: String,
    settings_sha256: String,
    radial_sha256: String,
    actions_sha256: String,
    configured_hotkey: &'static str,
    hold_threshold_ms: u64,
}

#[derive(Clone, Serialize)]
struct AcceptanceCaseResult {
    id: String,
    status: CaseStatus,
    elapsed_ms: u64,
    expected: String,
    observed: String,
    failure_stage: Option<FailureStage>,
    artifacts: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum HotkeyExpectedState {
    HiddenRootWakeAndFocus,
    VisibleRootHideWithoutRestore,
    HiddenAndVisibleBurstParity,
    TapDismissesWithoutSelection,
    TapDismissesThenNextGestureWorks,
    PreparingRuntimeCancelledBeforeReady,
    HoldOpensAndReleaseClosesWithoutGridToggle,
    HoldDismissesHoveredActionWithoutDispatch,
    DesignerDraftAndForegroundPreserved,
    DesignerPreviewAndDraftPreserved,
    DirectAndLegacyTriggersPreserved,
    AlternateHotkeyProfilesIsolated,
    ParkedRootWakesAndRadialDismisses,
}

// Evidence v4 uses grouped, kind-specific positional trace records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, serde::Deserialize)]
enum HotkeyCandidateStream {
    #[serde(rename = "main")]
    MainCandidate,
    #[serde(rename = "alternate")]
    AlternateProfileCandidate,
    #[serde(rename = "legacy")]
    LegacyFallbackCandidate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
enum HotkeyTraceEventKind {
    #[serde(rename = "press")]
    PrimaryPress,
    #[serde(rename = "release")]
    PrimaryRelease,
    #[serde(rename = "tap")]
    ShortTap,
    #[serde(rename = "draw_focus")]
    ScreenDrawRestoreFocusIntent,
    #[serde(rename = "intent")]
    VisibilityIntent,
    #[serde(rename = "command")]
    RootCommand,
    #[serde(rename = "snapshot")]
    NativeWindowSnapshot,
    #[serde(rename = "activation")]
    NativeActivation,
    #[serde(rename = "action")]
    RadialAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
enum HotkeyRadialActionStage {
    #[serde(rename = "activated")]
    Activated,
    #[serde(rename = "parsed")]
    Parsed,
    #[serde(rename = "parse_rejected")]
    ParseRejected,
    #[serde(rename = "dispatched")]
    Dispatched,
    #[serde(rename = "host_entered")]
    HostEntered,
    #[serde(rename = "editor_mode_applied")]
    EditorModeApplied,
    #[serde(rename = "host_completed")]
    HostCompleted,
    #[serde(rename = "other")]
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
enum HotkeyRootCommand {
    #[serde(rename = "pos")]
    Position,
    #[serde(rename = "size")]
    Size,
    #[serde(rename = "show")]
    Show,
    #[serde(rename = "min")]
    Minimize,
    #[serde(rename = "focus")]
    Focus,
    #[serde(rename = "park")]
    ParkingBoundary,
    #[serde(rename = "other")]
    Other,
}

fn default_hotkey_root_command() -> HotkeyRootCommand {
    HotkeyRootCommand::Other
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
enum HotkeyVisibilitySource {
    #[serde(rename = "toggle")]
    ToggleBatch,
    #[serde(rename = "legacy")]
    LegacyTrigger,
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "draw")]
    ScreenDrawRestore,
    #[serde(rename = "other")]
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
enum HotkeyRootFocusIntent {
    #[serde(rename = "activate")]
    ActivateRoot,
    #[serde(rename = "preserve")]
    PreserveForeground,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
enum HotkeyInputProvenance {
    #[serde(rename = "owned")]
    Owned,
    #[serde(rename = "injected")]
    ExternalInjected,
    #[serde(rename = "physical")]
    Physical,
    #[serde(rename = "other")]
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, serde::Deserialize)]
enum HotkeyRunnerInputPurpose {
    #[serde(rename = "chord")]
    LauncherChord,
    #[serde(rename = "burst")]
    MatrixBurst,
    #[serde(rename = "readable")]
    ReadableCadence,
    #[serde(rename = "direct")]
    DirectTrigger,
    #[serde(rename = "probe")]
    AuxiliaryProbe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
enum HotkeyEdgeTransition {
    #[serde(rename = "down")]
    Press,
    #[serde(rename = "up")]
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
enum HotkeyActivationEdge {
    #[serde(rename = "requested")]
    RestoreRequested,
    #[serde(rename = "completed")]
    RestoreCompleted,
    #[serde(rename = "failed")]
    RestoreFailed,
    #[serde(rename = "superseded")]
    Superseded,
    #[serde(rename = "other")]
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
enum HotkeyEvidenceNotApplicable {
    #[serde(rename = "hold")]
    HoldGestureHasNoShortTap,
    #[serde(rename = "legacy")]
    LegacyTriggerHasNoInvocationReducerId,
    #[serde(rename = "superseded")]
    SupersededIntermediateVisibility,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
struct HotkeyRunnerEdgeEvidence {
    /// Runner monotonic microseconds from the first measured edge in this case.
    /// This clock is never compared with candidate `elapsed_ms`.
    ///
    /// The compact v2 keys keep uninterrupted burst evidence within the packet
    /// bound while retaining each edge as a typed, independently auditable row.
    #[serde(rename = "t")]
    runner_relative_us: u64,
    #[serde(rename = "g")]
    input_group_id: u32,
    #[serde(rename = "s")]
    stream: HotkeyCandidateStream,
    #[serde(rename = "p")]
    purpose: HotkeyRunnerInputPurpose,
    #[serde(rename = "v")]
    virtual_key: u32,
    #[serde(rename = "x")]
    transition: HotkeyEdgeTransition,
    #[serde(rename = "i")]
    injected: bool,
    #[serde(rename = "c")]
    runner_cookie_matched: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
struct HotkeyCandidateEventEvidence {
    #[serde(rename = "s")]
    stream: HotkeyCandidateStream,
    #[serde(rename = "g")]
    input_group_id: u32,
    #[serde(rename = "p")]
    input_purpose: HotkeyRunnerInputPurpose,
    /// Absolute trace event ordinal in that candidate log, used to audit the
    /// cursor fence without exposing the raw line or log path.
    #[serde(rename = "o")]
    event_ordinal: u32,
    #[serde(rename = "t")]
    elapsed_ms: u64,
    #[serde(rename = "k")]
    kind: HotkeyTraceEventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "i")]
    invocation_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "r")]
    visibility_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "q")]
    request_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "v")]
    visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "m")]
    minimized: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "b")]
    bounds: Option<[i32; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "h")]
    hwnd: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "d")]
    process_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "c")]
    command: Option<HotkeyRootCommand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "vs")]
    visibility_source: Option<HotkeyVisibilitySource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "mm")]
    modifiers_match: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "pr")]
    provenance: Option<HotkeyInputProvenance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "z")]
    terminal: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "a")]
    activation_edge: Option<HotkeyActivationEdge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "f")]
    focus_intent: Option<HotkeyRootFocusIntent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ra")]
    radial_action_stage: Option<HotkeyRadialActionStage>,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
struct HotkeyCandidateEventGroupWire {
    #[serde(rename = "s")]
    stream: HotkeyCandidateStream,
    #[serde(rename = "g")]
    input_group_id: u32,
    #[serde(rename = "p")]
    input_purpose: HotkeyRunnerInputPurpose,
    /// Records are compact arrays: kind code, ordinal, elapsed_ms, then only
    /// fields owned by that event kind. Group identity is stored once.
    #[serde(rename = "e")]
    events: Vec<Vec<serde_json::Value>>,
}

fn serialize_hotkey_candidate_event_groups<S>(
    events: &[HotkeyCandidateEventEvidence],
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut groups = Vec::<HotkeyCandidateEventGroupWire>::new();
    for event in events {
        let group_index = groups
            .iter()
            .position(|group| {
                group.stream == event.stream
                    && group.input_group_id == event.input_group_id
                    && group.input_purpose == event.input_purpose
            })
            .unwrap_or_else(|| {
                groups.push(HotkeyCandidateEventGroupWire {
                    stream: event.stream,
                    input_group_id: event.input_group_id,
                    input_purpose: event.input_purpose,
                    events: Vec::new(),
                });
                groups.len() - 1
            });
        let record = encode_hotkey_candidate_event(event).map_err(serde::ser::Error::custom)?;
        groups[group_index].events.push(record);
    }
    groups.serialize(serializer)
}

fn deserialize_hotkey_candidate_event_groups<'de, D>(
    deserializer: D,
) -> Result<Vec<HotkeyCandidateEventEvidence>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let groups =
        <Vec<HotkeyCandidateEventGroupWire> as serde::Deserialize>::deserialize(deserializer)?;
    let mut events = Vec::new();
    for group in groups {
        for record in group.events {
            events.push(
                decode_hotkey_candidate_event(
                    group.stream,
                    group.input_group_id,
                    group.input_purpose,
                    &record,
                )
                .map_err(serde::de::Error::custom)?,
            );
        }
    }
    events.sort_by_key(|event| (event.stream, event.event_ordinal));
    Ok(events)
}

fn encode_hotkey_candidate_event(
    event: &HotkeyCandidateEventEvidence,
) -> Result<Vec<serde_json::Value>, String> {
    use HotkeyTraceEventKind as Kind;
    let mut record = vec![
        serde_json::json!(match event.kind {
            Kind::PrimaryPress => 0,
            Kind::PrimaryRelease => 1,
            Kind::ShortTap => 2,
            Kind::ScreenDrawRestoreFocusIntent => 3,
            Kind::VisibilityIntent => 4,
            Kind::RootCommand => 5,
            Kind::NativeWindowSnapshot => 6,
            Kind::NativeActivation => 7,
            Kind::RadialAction => 8,
        }),
        serde_json::json!(event.event_ordinal),
        serde_json::json!(event.elapsed_ms),
    ];
    macro_rules! push_field {
        ($field:ident) => {
            record.push(
                serde_json::to_value(&event.$field)
                    .map_err(|error| format!("encode {:?} field: {error}", event.kind))?,
            );
        };
    }
    match event.kind {
        Kind::PrimaryPress | Kind::PrimaryRelease => {
            push_field!(invocation_id);
            push_field!(modifiers_match);
            push_field!(provenance);
        }
        Kind::ShortTap => {
            push_field!(invocation_id);
            push_field!(terminal);
        }
        Kind::ScreenDrawRestoreFocusIntent => {
            push_field!(visibility_revision);
            push_field!(invocation_id);
            push_field!(focus_intent);
        }
        Kind::VisibilityIntent => {
            push_field!(visibility_revision);
            push_field!(invocation_id);
            push_field!(visible);
            push_field!(visibility_source);
            push_field!(focus_intent);
        }
        Kind::RootCommand => {
            push_field!(visibility_revision);
            push_field!(invocation_id);
            push_field!(request_id);
            push_field!(command);
            push_field!(terminal);
        }
        Kind::NativeWindowSnapshot => {
            push_field!(visibility_revision);
            push_field!(invocation_id);
            push_field!(request_id);
            push_field!(visible);
            push_field!(minimized);
            push_field!(bounds);
            push_field!(hwnd);
            push_field!(process_id);
            push_field!(terminal);
        }
        Kind::NativeActivation => {
            push_field!(visibility_revision);
            push_field!(invocation_id);
            push_field!(request_id);
            push_field!(hwnd);
            push_field!(terminal);
            push_field!(activation_edge);
            push_field!(focus_intent);
        }
        Kind::RadialAction => {
            push_field!(visibility_revision);
            push_field!(invocation_id);
            push_field!(radial_action_stage);
        }
    }
    let decoded = decode_hotkey_candidate_event(
        event.stream,
        event.input_group_id,
        event.input_purpose,
        &record,
    )?;
    if decoded != *event {
        return Err(format!(
            "{:?} event contains fields unsupported by its compact wire record",
            event.kind
        ));
    }
    Ok(record)
}

fn decode_hotkey_candidate_event(
    stream: HotkeyCandidateStream,
    input_group_id: u32,
    input_purpose: HotkeyRunnerInputPurpose,
    record: &[serde_json::Value],
) -> Result<HotkeyCandidateEventEvidence, String> {
    use HotkeyTraceEventKind as Kind;
    let code = read_wire_field::<u8>(record, 0)?;
    let kind = match code {
        0 => Kind::PrimaryPress,
        1 => Kind::PrimaryRelease,
        2 => Kind::ShortTap,
        3 => Kind::ScreenDrawRestoreFocusIntent,
        4 => Kind::VisibilityIntent,
        5 => Kind::RootCommand,
        6 => Kind::NativeWindowSnapshot,
        7 => Kind::NativeActivation,
        8 => Kind::RadialAction,
        _ => return Err(format!("unknown compact candidate event kind {code}")),
    };
    let expected_len = match kind {
        Kind::PrimaryPress | Kind::PrimaryRelease => 6,
        Kind::ShortTap => 5,
        Kind::ScreenDrawRestoreFocusIntent => 6,
        Kind::VisibilityIntent => 8,
        Kind::RootCommand => 8,
        Kind::NativeWindowSnapshot => 12,
        Kind::NativeActivation => 10,
        Kind::RadialAction => 6,
    };
    if record.len() != expected_len {
        return Err(format!(
            "compact {kind:?} event has {} fields; expected {expected_len}",
            record.len()
        ));
    }
    let mut event = HotkeyCandidateEventEvidence {
        stream,
        input_group_id,
        input_purpose,
        event_ordinal: read_wire_field(record, 1)?,
        elapsed_ms: read_wire_field(record, 2)?,
        kind,
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
    match kind {
        Kind::PrimaryPress | Kind::PrimaryRelease => {
            event.invocation_id = read_wire_field(record, 3)?;
            event.modifiers_match = read_wire_field(record, 4)?;
            event.provenance = read_wire_field(record, 5)?;
        }
        Kind::ShortTap => {
            event.invocation_id = read_wire_field(record, 3)?;
            event.terminal = read_wire_field(record, 4)?;
        }
        Kind::ScreenDrawRestoreFocusIntent => {
            event.visibility_revision = read_wire_field(record, 3)?;
            event.invocation_id = read_wire_field(record, 4)?;
            event.focus_intent = read_wire_field(record, 5)?;
        }
        Kind::VisibilityIntent => {
            event.visibility_revision = read_wire_field(record, 3)?;
            event.invocation_id = read_wire_field(record, 4)?;
            event.visible = read_wire_field(record, 5)?;
            event.visibility_source = read_wire_field(record, 6)?;
            event.focus_intent = read_wire_field(record, 7)?;
        }
        Kind::RootCommand => {
            event.visibility_revision = read_wire_field(record, 3)?;
            event.invocation_id = read_wire_field(record, 4)?;
            event.request_id = read_wire_field(record, 5)?;
            event.command = read_wire_field(record, 6)?;
            event.terminal = read_wire_field(record, 7)?;
        }
        Kind::NativeWindowSnapshot => {
            event.visibility_revision = read_wire_field(record, 3)?;
            event.invocation_id = read_wire_field(record, 4)?;
            event.request_id = read_wire_field(record, 5)?;
            event.visible = read_wire_field(record, 6)?;
            event.minimized = read_wire_field(record, 7)?;
            event.bounds = read_wire_field(record, 8)?;
            event.hwnd = read_wire_field(record, 9)?;
            event.process_id = read_wire_field(record, 10)?;
            event.terminal = read_wire_field(record, 11)?;
        }
        Kind::NativeActivation => {
            event.visibility_revision = read_wire_field(record, 3)?;
            event.invocation_id = read_wire_field(record, 4)?;
            event.request_id = read_wire_field(record, 5)?;
            event.hwnd = read_wire_field(record, 6)?;
            event.terminal = read_wire_field(record, 7)?;
            event.activation_edge = read_wire_field(record, 8)?;
            event.focus_intent = read_wire_field(record, 9)?;
        }
        Kind::RadialAction => {
            event.visibility_revision = read_wire_field(record, 3)?;
            event.invocation_id = read_wire_field(record, 4)?;
            event.radial_action_stage = read_wire_field(record, 5)?;
        }
    }
    Ok(event)
}

fn read_wire_field<T: serde::de::DeserializeOwned>(
    record: &[serde_json::Value],
    index: usize,
) -> Result<T, String> {
    let value = record
        .get(index)
        .cloned()
        .ok_or_else(|| format!("compact candidate event is missing field {index}"))?;
    serde_json::from_value(value).map_err(|error| format!("invalid compact event field: {error}"))
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
#[serde(tag = "d", rename_all = "snake_case")]
enum HotkeyDecisionProof {
    Applied {
        #[serde(rename = "r")]
        visibility_revision: u64,
        #[serde(rename = "t")]
        intent_elapsed_ms: u64,
        #[serde(rename = "v")]
        visible: bool,
        #[serde(rename = "li")]
        release_to_intent_ms: u64,
        #[serde(rename = "c")]
        root_commands: Vec<HotkeyRootCommandSpan>,
    },
    Superseded {
        #[serde(rename = "r")]
        visibility_revision: u64,
        #[serde(rename = "b")]
        by_revision: u64,
        #[serde(rename = "t")]
        intent_elapsed_ms: u64,
        #[serde(rename = "v")]
        visible: bool,
        #[serde(rename = "li")]
        release_to_intent_ms: u64,
    },
    NotApplicable {
        #[serde(rename = "n")]
        reason: HotkeyEvidenceNotApplicable,
    },
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
struct HotkeyRootCommandSpan {
    #[serde(skip)]
    visibility_revision: u64,
    #[serde(skip)]
    invocation_id: Option<u64>,
    #[serde(skip)]
    request_id: u64,
    #[serde(skip)]
    command_elapsed_ms: u64,
    #[serde(skip, default = "default_hotkey_root_command")]
    command: HotkeyRootCommand,
    #[serde(rename = "lr")]
    release_to_root_command_ms: Option<u64>,
    /// Ordinal references avoid serializing command and snapshot fields a
    /// second time; their complete typed records remain in candidate_events.
    #[serde(rename = "ce")]
    command_event_ordinal: u32,
    #[serde(rename = "se", skip_serializing_if = "Option::is_none")]
    observed_snapshot_event_ordinal: Option<u32>,
    #[serde(rename = "co", skip_serializing_if = "Option::is_none")]
    command_to_observed_ms: Option<u64>,
    #[serde(skip)]
    observed_presentation: Option<HotkeyObservedPresentation>,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
struct HotkeyObservedPresentation {
    #[serde(rename = "t")]
    elapsed_ms: u64,
    #[serde(rename = "co")]
    command_to_observed_ms: u64,
    #[serde(rename = "v")]
    visible: bool,
    #[serde(rename = "m")]
    minimized: bool,
    #[serde(rename = "b")]
    bounds: [i32; 4],
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
struct HotkeyGestureEvidence {
    #[serde(rename = "s")]
    stream: HotkeyCandidateStream,
    #[serde(rename = "g")]
    input_group_id: u32,
    #[serde(rename = "p")]
    input_purpose: HotkeyRunnerInputPurpose,
    #[serde(rename = "i")]
    invocation_id: u64,
    #[serde(rename = "t")]
    release_elapsed_ms: u64,
    #[serde(rename = "m")]
    release_modifiers_match: bool,
    #[serde(rename = "st")]
    short_tap_elapsed_ms: Option<u64>,
    #[serde(rename = "d")]
    decision: HotkeyDecisionProof,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
struct HotkeyStandaloneDecisionEvidence {
    #[serde(rename = "s")]
    stream: HotkeyCandidateStream,
    #[serde(rename = "g")]
    input_group_id: u32,
    #[serde(rename = "p")]
    input_purpose: HotkeyRunnerInputPurpose,
    #[serde(rename = "r")]
    visibility_revision: u64,
    #[serde(rename = "i")]
    invocation_id: Option<u64>,
    #[serde(rename = "t")]
    intent_elapsed_ms: u64,
    #[serde(rename = "v")]
    visible: bool,
    #[serde(rename = "vs")]
    source: HotkeyVisibilitySource,
    #[serde(rename = "c")]
    root_commands: Vec<HotkeyRootCommandSpan>,
    #[serde(rename = "d")]
    decision: HotkeyDecisionProof,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
struct HotkeyRootIdentityEvidence {
    #[serde(rename = "s")]
    stream: HotkeyCandidateStream,
    #[serde(rename = "h")]
    hwnd: u64,
    #[serde(rename = "p")]
    process_id: u32,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
struct HotkeyFollowOnRestoreEvidence {
    #[serde(rename = "s")]
    stream: HotkeyCandidateStream,
    #[serde(rename = "g")]
    input_group_id: u32,
    #[serde(rename = "pr")]
    parent_visibility_revision: Option<u64>,
    #[serde(rename = "r")]
    visibility_revision: u64,
    #[serde(rename = "i")]
    invocation_id: Option<u64>,
    #[serde(rename = "t")]
    intent_elapsed_ms: u64,
    #[serde(rename = "v")]
    visible: bool,
    #[serde(rename = "f")]
    focus_intent: HotkeyRootFocusIntent,
    #[serde(rename = "c")]
    root_commands: Vec<HotkeyRootCommandSpan>,
    #[serde(rename = "a")]
    native_activation: Option<HotkeyNativeActivationSpan>,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
struct HotkeyNativeActivationSpan {
    #[serde(rename = "q")]
    request_id: u64,
    #[serde(rename = "h")]
    hwnd: u64,
    #[serde(rename = "t")]
    requested_elapsed_ms: u64,
    #[serde(rename = "tt")]
    terminal_elapsed_ms: Option<u64>,
    #[serde(rename = "e")]
    terminal_edge: Option<HotkeyActivationEdge>,
}

#[derive(Clone, Debug, Serialize)]
struct HotkeyCaseEvidence {
    schema_version: u16,
    case_id: String,
    expected_state: HotkeyExpectedState,
    runner_clock: String,
    runner_edges: Vec<HotkeyRunnerEdgeEvidence>,
    #[serde(serialize_with = "serialize_hotkey_candidate_event_groups")]
    candidate_events: Vec<HotkeyCandidateEventEvidence>,
    gestures: Vec<HotkeyGestureEvidence>,
    standalone_decisions: Vec<HotkeyStandaloneDecisionEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    follow_on_restorations: Vec<HotkeyFollowOnRestoreEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    root_identities: Vec<HotkeyRootIdentityEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    physical_displays: Vec<[i32; 4]>,
    candidate_event_count: usize,
    candidate_trace_overflow: bool,
    capture_segment_overflow: bool,
    runner_edge_overflow: bool,
    gesture_overflow: bool,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
struct H04CompletedBurstEvidence {
    #[serde(rename = "n")]
    matrix_burst_index: u8,
    #[serde(rename = "g")]
    input_group_id: u32,
    #[serde(rename = "iv")]
    initial_visible: bool,
    #[serde(rename = "t")]
    requested_taps: u8,
    #[serde(rename = "fv")]
    final_visible: bool,
    #[serde(rename = "h0")]
    hold_min_ms: u128,
    #[serde(rename = "h1")]
    hold_max_ms: u128,
    #[serde(rename = "g0")]
    gap_min_ms: u128,
    #[serde(rename = "g1")]
    gap_max_ms: u128,
    #[serde(rename = "i")]
    invocation_ids: Vec<u64>,
    #[serde(rename = "p")]
    trace_probe_id: u64,
    #[serde(rename = "c")]
    trace_cursor: usize,
    #[serde(rename = "bi")]
    baseline_invocation_id: Option<u64>,
    #[serde(rename = "br")]
    baseline_visibility_revision: Option<u64>,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
struct H04InputContaminationArtifact {
    schema_version: u16,
    case_id: String,
    attempt: u8,
    hotkey: AcceptanceHotkey,
    failure_stage: String,
    failure: String,
    declared_initial_state: bool,
    next_matrix_burst_index: u8,
    completed_bursts: Vec<H04CompletedBurstEvidence>,
    input_group_id: u32,
    stream: HotkeyCandidateStream,
    prior_group_ids: Vec<u32>,
    owned_edges: Vec<HotkeyRunnerEdgeEvidence>,
    foreign_edges: Vec<HotkeyRunnerEdgeEvidence>,
    candidate_events: Vec<HotkeyCandidateEventEvidence>,
    root_identity: HotkeyRootIdentityEvidence,
}

#[derive(serde::Deserialize)]
struct HotkeyCaseEvidenceWire {
    schema_version: u16,
    case_id: String,
    expected_state: HotkeyExpectedState,
    runner_clock: String,
    runner_edges: Vec<HotkeyRunnerEdgeEvidence>,
    #[serde(deserialize_with = "deserialize_hotkey_candidate_event_groups")]
    candidate_events: Vec<HotkeyCandidateEventEvidence>,
    gestures: Vec<HotkeyGestureEvidence>,
    standalone_decisions: Vec<HotkeyStandaloneDecisionEvidence>,
    #[serde(default)]
    follow_on_restorations: Vec<HotkeyFollowOnRestoreEvidence>,
    #[serde(default)]
    root_identities: Vec<HotkeyRootIdentityEvidence>,
    #[serde(default)]
    physical_displays: Vec<[i32; 4]>,
    candidate_event_count: usize,
    candidate_trace_overflow: bool,
    capture_segment_overflow: bool,
    runner_edge_overflow: bool,
    gesture_overflow: bool,
}

impl<'de> serde::Deserialize<'de> for HotkeyCaseEvidence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = HotkeyCaseEvidenceWire::deserialize(deserializer)?;
        let mut evidence = Self {
            schema_version: wire.schema_version,
            case_id: wire.case_id,
            expected_state: wire.expected_state,
            runner_clock: wire.runner_clock,
            runner_edges: wire.runner_edges,
            candidate_events: wire.candidate_events,
            gestures: wire.gestures,
            standalone_decisions: wire.standalone_decisions,
            follow_on_restorations: wire.follow_on_restorations,
            root_identities: wire.root_identities,
            physical_displays: wire.physical_displays,
            candidate_event_count: wire.candidate_event_count,
            candidate_trace_overflow: wire.candidate_trace_overflow,
            capture_segment_overflow: wire.capture_segment_overflow,
            runner_edge_overflow: wire.runner_edge_overflow,
            gesture_overflow: wire.gesture_overflow,
        };
        hydrate_hotkey_command_spans(&mut evidence);
        Ok(evidence)
    }
}

fn hydrate_hotkey_command_spans(packet: &mut HotkeyCaseEvidence) {
    for gesture in &mut packet.gestures {
        if let HotkeyDecisionProof::Applied { root_commands, .. } = &mut gesture.decision {
            for span in root_commands {
                hydrate_hotkey_command_span(&packet.candidate_events, gesture.stream, span);
            }
        }
    }
    for decision in &mut packet.standalone_decisions {
        for span in &mut decision.root_commands {
            hydrate_hotkey_command_span(&packet.candidate_events, decision.stream, span);
        }
    }
    for restore in &mut packet.follow_on_restorations {
        for span in &mut restore.root_commands {
            hydrate_hotkey_command_span(&packet.candidate_events, restore.stream, span);
        }
    }
}

fn hydrate_hotkey_command_span(
    events: &[HotkeyCandidateEventEvidence],
    stream: HotkeyCandidateStream,
    span: &mut HotkeyRootCommandSpan,
) {
    let command = events.iter().find(|event| {
        event.stream == stream
            && event.event_ordinal == span.command_event_ordinal
            && event.kind == HotkeyTraceEventKind::RootCommand
    });
    if let Some(command) = command {
        span.visibility_revision = command.visibility_revision.unwrap_or_default();
        span.invocation_id = command.invocation_id;
        span.request_id = command.request_id.unwrap_or_default();
        span.command_elapsed_ms = command.elapsed_ms;
        span.command = command.command.unwrap_or(HotkeyRootCommand::Other);
    }
    let observed = span.observed_snapshot_event_ordinal.and_then(|ordinal| {
        events.iter().find(|event| {
            event.stream == stream
                && event.event_ordinal == ordinal
                && event.kind == HotkeyTraceEventKind::NativeWindowSnapshot
        })
    });
    span.observed_presentation = observed.and_then(|event| {
        Some(HotkeyObservedPresentation {
            elapsed_ms: event.elapsed_ms,
            command_to_observed_ms: span.command_to_observed_ms?,
            visible: event.visible?,
            minimized: event.minimized?,
            bounds: event.bounds?,
        })
    });
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
struct ReportOverflowReceipt {
    reason: String,
    omitted_case_evidence: usize,
    omitted_artifact_references: usize,
    affected_case_ids: Vec<String>,
}

fn hotkey_expected_state(case_id: &str) -> Option<HotkeyExpectedState> {
    Some(match case_id {
        "H01" => HotkeyExpectedState::HiddenRootWakeAndFocus,
        "H02" => HotkeyExpectedState::VisibleRootHideWithoutRestore,
        "H04" => HotkeyExpectedState::HiddenAndVisibleBurstParity,
        "H06" => HotkeyExpectedState::TapDismissesWithoutSelection,
        "H07" => HotkeyExpectedState::TapDismissesThenNextGestureWorks,
        "H08" => HotkeyExpectedState::PreparingRuntimeCancelledBeforeReady,
        "H09" => HotkeyExpectedState::HoldOpensAndReleaseClosesWithoutGridToggle,
        "H10" => HotkeyExpectedState::HoldDismissesHoveredActionWithoutDispatch,
        "H11" => HotkeyExpectedState::DesignerDraftAndForegroundPreserved,
        "H12" => HotkeyExpectedState::DesignerPreviewAndDraftPreserved,
        "H16" => HotkeyExpectedState::DirectAndLegacyTriggersPreserved,
        "H17" => HotkeyExpectedState::AlternateHotkeyProfilesIsolated,
        "H18" => HotkeyExpectedState::ParkedRootWakesAndRadialDismisses,
        _ => return None,
    })
}

fn validate_hotkey_evidence_packet(packet: &HotkeyCaseEvidence) -> Result<(), String> {
    validate_hotkey_evidence_packet_with_context(packet, AcceptanceHotkey::F11, 350)
}

fn validate_hotkey_evidence_packet_with_context(
    packet: &HotkeyCaseEvidence,
    configured_hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) -> Result<(), String> {
    let encoded_len = serde_json::to_vec(packet)
        .map_err(|error| format!("{} evidence serialization failed: {error}", packet.case_id))?
        .len();
    if encoded_len > MAX_HOTKEY_CASE_EVIDENCE_BYTES {
        return Err(format!(
            "{} evidence packet exceeds its byte bound: bytes={encoded_len}, candidate_events={}, runner_edges={}, gestures={}, standalone_decisions={}, command_spans={}, root_commands={}, snapshots={}",
            packet.case_id,
            packet.candidate_events.len(),
            packet.runner_edges.len(),
            packet.gestures.len(),
            packet.standalone_decisions.len(),
            all_root_command_spans(packet).len(),
            packet
                .candidate_events
                .iter()
                .filter(|event| event.kind == HotkeyTraceEventKind::RootCommand)
                .count(),
            packet
                .candidate_events
                .iter()
                .filter(|event| event.kind == HotkeyTraceEventKind::NativeWindowSnapshot)
                .count(),
        ));
    }
    if packet.schema_version != 4 {
        return Err(format!(
            "{} evidence schema version is unsupported",
            packet.case_id
        ));
    }
    if hotkey_expected_state(&packet.case_id) != Some(packet.expected_state) {
        return Err(format!(
            "{} evidence expected state does not match its case",
            packet.case_id
        ));
    }
    if packet.runner_clock != "runner_monotonic_relative_us" {
        return Err(format!("{} runner clock label is invalid", packet.case_id));
    }
    if packet.candidate_trace_overflow
        || packet.capture_segment_overflow
        || packet.runner_edge_overflow
        || packet.gesture_overflow
        || packet.candidate_event_count != packet.candidate_events.len()
        || packet.candidate_events.len() > MAX_HOTKEY_EVIDENCE_EVENTS
        || packet.runner_edges.len() > MAX_HOTKEY_EVIDENCE_EDGES
        || packet.gestures.len() > MAX_HOTKEY_EVIDENCE_GESTURES
        || packet.standalone_decisions.len() > MAX_HOTKEY_EVIDENCE_GESTURES
    {
        return Err(format!(
            "{} evidence packet is incomplete or over capacity",
            packet.case_id
        ));
    }
    validate_packet_root_identity(packet)?;

    let mut ordinals = std::collections::BTreeSet::new();
    let mut root_request_ids = std::collections::BTreeSet::new();
    let mut snapshot_request_ids = std::collections::BTreeSet::new();
    let mut last_by_stream = std::collections::BTreeMap::new();
    for event in &packet.candidate_events {
        if !ordinals.insert((event.stream as u8, event.event_ordinal)) {
            return Err(format!(
                "{} candidate trace event ordinal is duplicated",
                packet.case_id
            ));
        }
        if event.input_group_id == 0 || event.event_ordinal == 0 {
            return Err(format!(
                "{} candidate event is outside a measured input group",
                packet.case_id
            ));
        }
        if let Some((last_ordinal, last_elapsed)) = last_by_stream.get(&event.stream) {
            if event.event_ordinal <= *last_ordinal || event.elapsed_ms < *last_elapsed {
                return Err(format!(
                    "{} candidate trace order is invalid",
                    packet.case_id
                ));
            }
        }
        last_by_stream.insert(event.stream, (event.event_ordinal, event.elapsed_ms));
        match event.kind {
            HotkeyTraceEventKind::PrimaryPress => {
                if event.invocation_id.is_none()
                    || event.modifiers_match != Some(true)
                    || event.provenance != Some(HotkeyInputProvenance::ExternalInjected)
                {
                    return Err(format!(
                        "{} primary press is uncorrelated, mismatched, or not external injected input",
                        packet.case_id
                    ));
                }
            }
            HotkeyTraceEventKind::PrimaryRelease => {
                if event.invocation_id.is_none()
                    || event.modifiers_match.is_none()
                    || event.provenance != Some(HotkeyInputProvenance::ExternalInjected)
                {
                    return Err(format!("{} primary release is malformed", packet.case_id));
                }
            }
            _ => {}
        }
        match event.kind {
            HotkeyTraceEventKind::RootCommand => {
                let Some(request_id) = event.request_id else {
                    return Err(format!(
                        "{} ROOT command has no boundary request ID",
                        packet.case_id
                    ));
                };
                if !root_request_ids.insert((event.stream, request_id)) {
                    return Err(format!(
                        "{} ROOT boundary request ID is duplicated",
                        packet.case_id
                    ));
                }
            }
            HotkeyTraceEventKind::NativeWindowSnapshot => {
                let Some(request_id) = event.request_id else {
                    return Err(format!(
                        "{} ROOT snapshot has no boundary request ID",
                        packet.case_id
                    ));
                };
                if !snapshot_request_ids.insert((event.stream, request_id)) {
                    return Err(format!(
                        "{} ROOT snapshot request ID is duplicated",
                        packet.case_id
                    ));
                }
                if !event_root_identity_matches(packet, event) {
                    return Err(format!(
                        "{} native presentation snapshot is not from the captured ROOT HWND/PID",
                        packet.case_id
                    ));
                }
            }
            _ => {}
        }
    }
    validate_hotkey_candidate_event_coverage(packet)?;

    for gesture in &packet.gestures {
        if gesture.input_group_id == 0 {
            return Err(format!("{} gesture evidence is malformed", packet.case_id));
        }
        if let Some(short_tap_elapsed_ms) = gesture.short_tap_elapsed_ms {
            if short_tap_elapsed_ms < gesture.release_elapsed_ms {
                return Err(format!(
                    "{} short tap precedes its primary release",
                    packet.case_id
                ));
            }
        }
        let release_events = packet
            .candidate_events
            .iter()
            .filter(|event| {
                event.stream == gesture.stream
                    && event.input_group_id == gesture.input_group_id
                    && event.kind == HotkeyTraceEventKind::PrimaryRelease
                    && event.invocation_id == Some(gesture.invocation_id)
                    && event.elapsed_ms == gesture.release_elapsed_ms
                    && event.modifiers_match == Some(gesture.release_modifiers_match)
            })
            .count();
        if release_events != 1 {
            return Err(format!(
                "{} gesture release does not map one-to-one to its trace event",
                packet.case_id
            ));
        }
        if let Some(short_tap_elapsed_ms) = gesture.short_tap_elapsed_ms {
            let short_tap_events = packet
                .candidate_events
                .iter()
                .filter(|event| {
                    event.stream == gesture.stream
                        && event.input_group_id == gesture.input_group_id
                        && event.kind == HotkeyTraceEventKind::ShortTap
                        && event.invocation_id == Some(gesture.invocation_id)
                        && event.elapsed_ms == short_tap_elapsed_ms
                        && event.terminal == Some(true)
                })
                .count();
            if short_tap_events != 1 {
                return Err(format!(
                    "{} short tap does not map one-to-one to its trace event",
                    packet.case_id
                ));
            }
        }
        if let HotkeyDecisionProof::Applied {
            visibility_revision,
            intent_elapsed_ms,
            visible,
            release_to_intent_ms,
            root_commands,
            ..
        } = &gesture.decision
        {
            let Some(short_tap_elapsed_ms) = gesture.short_tap_elapsed_ms else {
                return Err(format!(
                    "{} applied visibility decision has no short tap",
                    packet.case_id
                ));
            };
            if *intent_elapsed_ms < gesture.release_elapsed_ms
                || *intent_elapsed_ms < short_tap_elapsed_ms
                || *release_to_intent_ms
                    != intent_elapsed_ms.saturating_sub(gesture.release_elapsed_ms)
                || root_commands.is_empty()
                || !root_commands
                    .iter()
                    .any(|span| span.observed_presentation.is_some())
                || !packet.candidate_events.iter().any(|event| {
                    event.stream == gesture.stream
                        && event.kind == HotkeyTraceEventKind::VisibilityIntent
                        && event.input_group_id == gesture.input_group_id
                        && event.invocation_id == Some(gesture.invocation_id)
                        && event.visibility_revision == Some(*visibility_revision)
                        && event.elapsed_ms == *intent_elapsed_ms
                        && event.visible == Some(*visible)
                        && event.visibility_source == Some(HotkeyVisibilitySource::ToggleBatch)
                })
            {
                return Err(format!(
                    "{} applied gesture is missing a correlated latency span",
                    packet.case_id
                ));
            }
            for span in root_commands {
                if span.visibility_revision == 0
                    || span.request_id == 0
                    || span.visibility_revision != *visibility_revision
                    || span.invocation_id != Some(gesture.invocation_id)
                    || span.command_elapsed_ms < gesture.release_elapsed_ms
                    || span.release_to_root_command_ms
                        != Some(
                            span.command_elapsed_ms
                                .saturating_sub(gesture.release_elapsed_ms),
                        )
                    || !root_command_span_matches(packet, gesture.stream, span)
                {
                    return Err(format!(
                        "{} ROOT command span has invalid release correlation",
                        packet.case_id
                    ));
                }
                if let Some(observed) = &span.observed_presentation {
                    if observed.elapsed_ms < span.command_elapsed_ms
                        || observed.command_to_observed_ms
                            != observed.elapsed_ms.saturating_sub(span.command_elapsed_ms)
                    {
                        return Err(format!(
                            "{} ROOT presentation latency arithmetic is invalid",
                            packet.case_id
                        ));
                    }
                }
            }
            if !root_commands.iter().any(|span| {
                span.observed_presentation.as_ref().is_some_and(|observed| {
                    presentation_matches(packet, gesture.stream, span, observed, *visible)
                })
            }) {
                return Err(format!(
                    "{} applied gesture lacks a ROOT-owned physical presentation matching its visibility decision",
                    packet.case_id
                ));
            }
        }
        if let HotkeyDecisionProof::Superseded {
            visibility_revision,
            by_revision,
            intent_elapsed_ms,
            release_to_intent_ms,
            visible,
            ..
        } = &gesture.decision
        {
            if gesture.short_tap_elapsed_ms.is_none()
                || *visibility_revision == 0
                || *by_revision <= *visibility_revision
                || *intent_elapsed_ms < gesture.release_elapsed_ms
                || *release_to_intent_ms
                    != intent_elapsed_ms.saturating_sub(gesture.release_elapsed_ms)
            {
                return Err(format!(
                    "{} superseded decision is malformed",
                    packet.case_id
                ));
            }
            let has_source_intent = packet.candidate_events.iter().any(|event| {
                event.stream == gesture.stream
                    && event.kind == HotkeyTraceEventKind::VisibilityIntent
                    && event.invocation_id == Some(gesture.invocation_id)
                    && event.visibility_revision == Some(*visibility_revision)
                    && event.elapsed_ms == *intent_elapsed_ms
                    && event.visible == Some(*visible)
            });
            let successor_intent = packet.candidate_events.iter().find(|event| {
                event.stream == gesture.stream
                    && event.kind == HotkeyTraceEventKind::VisibilityIntent
                    && event.visibility_revision == Some(*by_revision)
                    && event.elapsed_ms >= *intent_elapsed_ms
                    && event.invocation_id.is_some()
            });
            let successor_gesture = successor_intent.and_then(|successor| {
                packet.gestures.iter().find(|candidate| {
                    candidate.stream == successor.stream
                        && Some(candidate.invocation_id) == successor.invocation_id
                        && candidate.short_tap_elapsed_ms.is_some()
                        && matches!(
                            &candidate.decision,
                            HotkeyDecisionProof::Applied {
                                visibility_revision,
                                root_commands,
                                ..
                            } if *visibility_revision == *by_revision
                                && root_commands.iter().any(|span| {
                                    span.observed_presentation.as_ref().is_some_and(|observed| {
                                        presentation_matches(
                                            packet,
                                            candidate.stream,
                                            span,
                                            observed,
                                            successor.visible.unwrap_or(false),
                                        )
                                    })
                                })
                        )
                })
            });
            if !has_source_intent || successor_gesture.is_none() {
                return Err(format!(
                    "{} superseded decision lacks a later paired gesture with terminal physical presentation",
                    packet.case_id
                ));
            }
        }
        if let HotkeyDecisionProof::NotApplicable { reason } = &gesture.decision {
            let has_toggle_intent = packet.candidate_events.iter().any(|event| {
                event.stream == gesture.stream
                    && event.invocation_id == Some(gesture.invocation_id)
                    && event.kind == HotkeyTraceEventKind::VisibilityIntent
                    && event.visibility_source == Some(HotkeyVisibilitySource::ToggleBatch)
            });
            match reason {
                HotkeyEvidenceNotApplicable::HoldGestureHasNoShortTap
                    if gesture.short_tap_elapsed_ms.is_none() && !has_toggle_intent => {}
                HotkeyEvidenceNotApplicable::LegacyTriggerHasNoInvocationReducerId
                    if gesture.short_tap_elapsed_ms.is_none() && !has_toggle_intent => {}
                _ => {
                    return Err(format!(
                        "{} not-applicable gesture has a tap/toggle or an unsupported reason",
                        packet.case_id
                    ));
                }
            }
        }
    }

    for decision in &packet.standalone_decisions {
        if decision.source != HotkeyVisibilitySource::LegacyTrigger
            || decision.invocation_id.is_some()
            || decision.visibility_revision == 0
            || decision.root_commands.is_empty()
            || !packet.candidate_events.iter().any(|event| {
                event.stream == decision.stream
                    && event.kind == HotkeyTraceEventKind::VisibilityIntent
                    && event.input_group_id == decision.input_group_id
                    && event.visibility_revision == Some(decision.visibility_revision)
                    && event.invocation_id.is_none()
                    && event.elapsed_ms == decision.intent_elapsed_ms
                    && event.visible == Some(decision.visible)
                    && event.visibility_source == Some(HotkeyVisibilitySource::LegacyTrigger)
            })
            || !matches!(
                &decision.decision,
                HotkeyDecisionProof::NotApplicable {
                    reason: HotkeyEvidenceNotApplicable::LegacyTriggerHasNoInvocationReducerId
                }
            )
            || !decision
                .root_commands
                .iter()
                .any(|span| span.observed_presentation.is_some())
            || decision.root_commands.iter().any(|span| {
                span.visibility_revision != decision.visibility_revision
                    || span.invocation_id.is_some()
                    || span.release_to_root_command_ms.is_some()
                    || !root_command_span_matches(packet, decision.stream, span)
            })
            || !decision.root_commands.iter().any(|span| {
                span.observed_presentation.as_ref().is_some_and(|observed| {
                    presentation_matches(packet, decision.stream, span, observed, decision.visible)
                })
            })
        {
            return Err(format!(
                "{} standalone legacy decision is incomplete",
                packet.case_id
            ));
        }
    }

    let screen_draw_events = packet
        .candidate_events
        .iter()
        .filter(|event| {
            event.kind == HotkeyTraceEventKind::VisibilityIntent
                && event.visibility_source == Some(HotkeyVisibilitySource::ScreenDrawRestore)
        })
        .collect::<Vec<_>>();
    let screen_draw_focus_events = packet
        .candidate_events
        .iter()
        .filter(|event| event.kind == HotkeyTraceEventKind::ScreenDrawRestoreFocusIntent)
        .collect::<Vec<_>>();
    if screen_draw_events.len() != packet.follow_on_restorations.len()
        || screen_draw_focus_events.len() != packet.follow_on_restorations.len()
    {
        return Err(format!(
            "{} Screen Draw restores are missing their focus-intent or follow-on proof records",
            packet.case_id
        ));
    }
    for restore in &packet.follow_on_restorations {
        let Some(event) = screen_draw_events.iter().find(|event| {
            event.stream == restore.stream
                && event.input_group_id == restore.input_group_id
                && event.visibility_revision == Some(restore.visibility_revision)
                && event.invocation_id == restore.invocation_id
                && event.elapsed_ms == restore.intent_elapsed_ms
                && event.visible == Some(restore.visible)
        }) else {
            return Err(format!(
                "{} Screen Draw follow-on does not match its trace event",
                packet.case_id
            ));
        };
        let focus_intent_event = screen_draw_focus_events.iter().find(|candidate| {
            candidate.stream == restore.stream
                && candidate.input_group_id == restore.input_group_id
                && candidate.visibility_revision == Some(restore.visibility_revision)
                && candidate.invocation_id == restore.invocation_id
                && candidate.focus_intent == Some(restore.focus_intent)
                && candidate.event_ordinal > event.event_ordinal
                && candidate.elapsed_ms >= event.elapsed_ms
        });
        let parent_matches = match (restore.invocation_id, restore.parent_visibility_revision) {
            (None, None) => true,
            (Some(invocation_id), Some(parent_revision)) => {
                packet.candidate_events.iter().any(|prior| {
                    prior.stream == restore.stream
                        && prior.kind == HotkeyTraceEventKind::VisibilityIntent
                        && prior.visibility_source == Some(HotkeyVisibilitySource::ToggleBatch)
                        && prior.invocation_id == Some(invocation_id)
                        && prior.visibility_revision == Some(parent_revision)
                        && parent_revision < restore.visibility_revision
                        && prior.elapsed_ms <= restore.intent_elapsed_ms
                })
            }
            _ => false,
        };
        let activation_is_valid = match restore.focus_intent {
            HotkeyRootFocusIntent::ActivateRoot => {
                restore.native_activation.as_ref().is_some_and(|span| {
                    native_activation_span_matches(packet, restore.stream, restore, span)
                })
            }
            HotkeyRootFocusIntent::PreserveForeground => {
                restore.native_activation.is_none()
                    && !packet.candidate_events.iter().any(|candidate| {
                        candidate.stream == restore.stream
                            && candidate.kind == HotkeyTraceEventKind::NativeActivation
                            && candidate.visibility_revision == Some(restore.visibility_revision)
                            && candidate.invocation_id == restore.invocation_id
                    })
            }
        };
        if !parent_matches
            || focus_intent_event.is_none()
            || event.visibility_revision.is_none()
            || restore.root_commands.is_empty()
            || restore.root_commands.iter().any(|span| {
                span.visibility_revision != restore.visibility_revision
                    || span.invocation_id != restore.invocation_id
                    || span.release_to_root_command_ms.is_some()
                    || !root_command_span_matches(packet, restore.stream, span)
            })
            || !restore.root_commands.iter().any(|span| {
                span.observed_presentation.as_ref().is_some_and(|observed| {
                    presentation_matches(packet, restore.stream, span, observed, restore.visible)
                })
            })
            || !activation_is_valid
        {
            return Err(format!(
                "{} Screen Draw follow-on lacks a correlated ROOT/native presentation span",
                packet.case_id
            ));
        }
    }

    if packet.case_id == "H11" {
        for intent in packet.candidate_events.iter().filter(|event| {
            event.kind == HotkeyTraceEventKind::VisibilityIntent
                && event.visibility_source == Some(HotkeyVisibilitySource::ToggleBatch)
        }) {
            if packet.candidate_events.iter().any(|event| {
                event.stream == intent.stream
                    && event.kind == HotkeyTraceEventKind::NativeActivation
                    && event.visibility_revision == intent.visibility_revision
                    && event.invocation_id == intent.invocation_id
                    && event.activation_edge == Some(HotkeyActivationEdge::RestoreRequested)
            }) {
                return Err(
                    "H11 PreserveForeground gesture requested native ROOT activation".into(),
                );
            }
        }
    }

    let matrix_taps = packet
        .gestures
        .iter()
        .filter(|gesture| {
            gesture.input_purpose == HotkeyRunnerInputPurpose::MatrixBurst
                && gesture.short_tap_elapsed_ms.is_some()
        })
        .count();
    let readable_taps = packet
        .gestures
        .iter()
        .filter(|gesture| {
            gesture.input_purpose == HotkeyRunnerInputPurpose::ReadableCadence
                && gesture.short_tap_elapsed_ms.is_some()
        })
        .count();
    if packet.case_id == "H04" && (matrix_taps != 86 || readable_taps != 3) {
        return Err("H04 evidence does not contain the complete 86+3 tap matrix".into());
    }
    let required_taps = match packet.case_id.as_str() {
        "H01" | "H02" | "H06" | "H08" | "H18" => 1,
        "H07" => 3,
        "H16" => 2,
        "H11" => 5,
        "H17" => 2,
        "H04" | "H09" | "H10" => 0,
        "H12" => 1,
        _ => 0,
    };
    let measured_taps = packet
        .gestures
        .iter()
        .filter(|gesture| gesture.short_tap_elapsed_ms.is_some())
        .count();
    if measured_taps < required_taps {
        return Err(format!(
            "{} evidence has {measured_taps} taps; expected at least {required_taps}",
            packet.case_id
        ));
    }
    let required_holds = match packet.case_id.as_str() {
        "H08" | "H12" | "H18" => 1,
        "H09" | "H10" => 2,
        _ => 0,
    };
    let measured_holds = packet
        .gestures
        .iter()
        .filter(|gesture| {
            gesture_has_tagged_hold(packet, gesture, configured_hotkey, hold_threshold_ms)
        })
        .count();
    if measured_holds < required_holds {
        return Err(format!(
            "{} evidence has {measured_holds} tagged holds; expected at least {required_holds}",
            packet.case_id
        ));
    }
    let visible_decisions = packet
        .gestures
        .iter()
        .filter_map(|gesture| {
            hotkey_decision_visible(&gesture.decision).map(|visible| {
                (
                    gesture.stream,
                    gesture.input_purpose,
                    gesture.input_group_id,
                    gesture.release_elapsed_ms,
                    visible,
                )
            })
        })
        .collect::<Vec<_>>();
    let require_sequence = |stream, purpose, expected: &[bool]| -> Result<(), String> {
        let observed = visible_decisions
            .iter()
            .filter(|(candidate_stream, candidate_purpose, _, _, _)| {
                *candidate_stream == stream && *candidate_purpose == purpose
            })
            .map(|(_, _, _, _, visible)| *visible)
            .collect::<Vec<_>>();
        if observed != expected {
            return Err(format!(
                "{} expected {expected:?} visibility decisions for {purpose:?}, observed {observed:?}",
                packet.case_id
            ));
        }
        Ok(())
    };
    match packet.case_id.as_str() {
        "H01" => require_sequence(
            HotkeyCandidateStream::MainCandidate,
            HotkeyRunnerInputPurpose::LauncherChord,
            &[true],
        )?,
        "H02" => require_sequence(
            HotkeyCandidateStream::MainCandidate,
            HotkeyRunnerInputPurpose::LauncherChord,
            &[false],
        )?,
        "H06" => require_sequence(
            HotkeyCandidateStream::MainCandidate,
            HotkeyRunnerInputPurpose::LauncherChord,
            &[true],
        )?,
        "H07" => require_sequence(
            HotkeyCandidateStream::MainCandidate,
            HotkeyRunnerInputPurpose::LauncherChord,
            &[false, true, false],
        )?,
        "H08" => require_sequence(
            HotkeyCandidateStream::MainCandidate,
            HotkeyRunnerInputPurpose::LauncherChord,
            &[false],
        )?,
        "H11" => require_sequence(
            HotkeyCandidateStream::MainCandidate,
            HotkeyRunnerInputPurpose::LauncherChord,
            &[false, true, false, true, false],
        )?,
        "H12" => require_sequence(
            HotkeyCandidateStream::MainCandidate,
            HotkeyRunnerInputPurpose::LauncherChord,
            &[false],
        )?,
        "H16" => require_sequence(
            HotkeyCandidateStream::MainCandidate,
            HotkeyRunnerInputPurpose::LauncherChord,
            &[false, true],
        )?,
        "H17" => {
            require_sequence(
                HotkeyCandidateStream::MainCandidate,
                HotkeyRunnerInputPurpose::LauncherChord,
                &[true],
            )?;
            require_sequence(
                HotkeyCandidateStream::AlternateProfileCandidate,
                HotkeyRunnerInputPurpose::LauncherChord,
                &[true],
            )?;
        }
        "H18" => require_sequence(
            HotkeyCandidateStream::MainCandidate,
            HotkeyRunnerInputPurpose::LauncherChord,
            &[true],
        )?,
        "H04" => {
            let matrix_groups = packet
                .gestures
                .iter()
                .filter(|gesture| gesture.input_purpose == HotkeyRunnerInputPurpose::MatrixBurst)
                .fold(
                    std::collections::BTreeMap::<u32, Vec<&HotkeyGestureEvidence>>::new(),
                    |mut groups, gesture| {
                        groups
                            .entry(gesture.input_group_id)
                            .or_default()
                            .push(gesture);
                        groups
                    },
                );
            let expected_taps = [1usize, 2, 5, 10, 25, 1, 2, 5, 10, 25];
            if matrix_groups.len() != expected_taps.len() {
                return Err(
                    "H04 matrix evidence has the wrong number of uninterrupted bursts".into(),
                );
            }
            for (index, (group_id, gestures)) in matrix_groups.iter().enumerate() {
                let mut ordered = gestures.iter().copied().collect::<Vec<_>>();
                ordered.sort_by_key(|gesture| gesture.release_elapsed_ms);
                if *group_id != u32::try_from(index + 1).unwrap_or(u32::MAX)
                    || ordered.len() != expected_taps[index]
                {
                    return Err(format!(
                        "H04 burst group {group_id} has an unexpected identity or {} decisions, expected group {} with {} decisions",
                        ordered.len(),
                        index + 1,
                        expected_taps[index]
                    ));
                }
                let mut group_visible = index >= 5;
                for gesture in ordered {
                    group_visible = !group_visible;
                    if hotkey_decision_visible(&gesture.decision) != Some(group_visible) {
                        return Err(format!(
                            "H04 burst group {group_id} did not alternate visible state per gesture"
                        ));
                    }
                }
            }
            require_sequence(
                HotkeyCandidateStream::MainCandidate,
                HotkeyRunnerInputPurpose::ReadableCadence,
                &[true, false, true],
            )?;
        }
        _ => {}
    }
    if packet.case_id == "H16"
        && !packet.standalone_decisions.iter().any(|decision| {
            decision.stream == HotkeyCandidateStream::LegacyFallbackCandidate
                && decision.source == HotkeyVisibilitySource::LegacyTrigger
        })
    {
        return Err("H16 evidence omitted the supported legacy launcher route".into());
    }
    if packet.case_id == "H17"
        && (!packet
            .gestures
            .iter()
            .any(|gesture| gesture.stream == HotkeyCandidateStream::MainCandidate)
            || !packet
                .gestures
                .iter()
                .any(|gesture| gesture.stream == HotkeyCandidateStream::AlternateProfileCandidate))
    {
        return Err("H17 evidence omitted a profile candidate stream".into());
    }

    validate_runner_edge_groups(packet, configured_hotkey, hold_threshold_ms)?;
    if packet.case_id == "H04" {
        validate_h04_matrix_foreign_edges(packet)?;
    }

    let mut previous_runner_us = None;
    for edge in &packet.runner_edges {
        if edge.input_group_id == 0 || edge.runner_relative_us > 24 * 60 * 60 * 1_000_000 {
            return Err(format!(
                "{} runner edge evidence is malformed",
                packet.case_id
            ));
        }
        if previous_runner_us.is_some_and(|previous| edge.runner_relative_us < previous) {
            return Err(format!(
                "{} runner edge timestamps are out of order",
                packet.case_id
            ));
        }
        previous_runner_us = Some(edge.runner_relative_us);
    }
    Ok(())
}

fn validate_hotkey_candidate_event_coverage(packet: &HotkeyCaseEvidence) -> Result<(), String> {
    for event in &packet.candidate_events {
        match event.kind {
            HotkeyTraceEventKind::VisibilityIntent => {
                if visibility_intent_proof_count(packet, event) != 1 {
                    return Err(format!(
                        "{} visibility intent has no unique matching decision proof",
                        packet.case_id
                    ));
                }
            }
            HotkeyTraceEventKind::RootCommand => {
                if root_command_event_reference_count(packet, event) != 1 {
                    return Err(format!(
                        "{} ROOT command has no unique matching command span",
                        packet.case_id
                    ));
                }
            }
            HotkeyTraceEventKind::RadialAction => {
                return Err(format!(
                    "{} launcher gesture selected or dispatched a radial action ({:?})",
                    packet.case_id, event.radial_action_stage
                ));
            }
            _ => {}
        }
    }

    for gesture in &packet.gestures {
        let (visibility_revision, intent_elapsed_ms, visible) = match &gesture.decision {
            HotkeyDecisionProof::Applied {
                visibility_revision,
                intent_elapsed_ms,
                visible,
                ..
            }
            | HotkeyDecisionProof::Superseded {
                visibility_revision,
                intent_elapsed_ms,
                visible,
                ..
            } => (*visibility_revision, *intent_elapsed_ms, *visible),
            HotkeyDecisionProof::NotApplicable { .. } => continue,
        };
        let intent_count = packet
            .candidate_events
            .iter()
            .filter(|event| {
                event.stream == gesture.stream
                    && event.input_group_id == gesture.input_group_id
                    && event.kind == HotkeyTraceEventKind::VisibilityIntent
                    && event.visibility_source == Some(HotkeyVisibilitySource::ToggleBatch)
                    && event.invocation_id == Some(gesture.invocation_id)
                    && event.visibility_revision == Some(visibility_revision)
                    && event.elapsed_ms == intent_elapsed_ms
                    && event.visible == Some(visible)
            })
            .count();
        if intent_count != 1 {
            return Err(format!(
                "{} gesture decision maps to {intent_count} trace intents instead of one",
                packet.case_id
            ));
        }
    }

    for decision in &packet.standalone_decisions {
        let intent_count = packet
            .candidate_events
            .iter()
            .filter(|event| {
                event.stream == decision.stream
                    && event.input_group_id == decision.input_group_id
                    && event.kind == HotkeyTraceEventKind::VisibilityIntent
                    && event.visibility_source == Some(HotkeyVisibilitySource::LegacyTrigger)
                    && event.invocation_id.is_none()
                    && event.visibility_revision == Some(decision.visibility_revision)
                    && event.elapsed_ms == decision.intent_elapsed_ms
                    && event.visible == Some(decision.visible)
            })
            .count();
        if intent_count != 1 {
            return Err(format!(
                "{} standalone legacy decision maps to {intent_count} trace intents instead of one",
                packet.case_id
            ));
        }
    }

    for restore in &packet.follow_on_restorations {
        let intent_count = packet
            .candidate_events
            .iter()
            .filter(|event| {
                event.stream == restore.stream
                    && event.input_group_id == restore.input_group_id
                    && event.kind == HotkeyTraceEventKind::VisibilityIntent
                    && event.visibility_source == Some(HotkeyVisibilitySource::ScreenDrawRestore)
                    && event.invocation_id == restore.invocation_id
                    && event.visibility_revision == Some(restore.visibility_revision)
                    && event.elapsed_ms == restore.intent_elapsed_ms
                    && event.visible == Some(restore.visible)
            })
            .count();
        if intent_count != 1 {
            return Err(format!(
                "{} Screen Draw follow-on maps to {intent_count} trace intents instead of one",
                packet.case_id
            ));
        }
    }

    let mut snapshot_event_references = std::collections::BTreeSet::new();
    for (stream, span) in all_root_command_spans(packet) {
        let event_count = packet
            .candidate_events
            .iter()
            .filter(|event| root_command_event_matches_span(event, stream, span))
            .count();
        if event_count != 1 || !root_command_span_matches(packet, stream, span) {
            return Err(format!(
                "{} ROOT command event reference is dangling, duplicated, or does not match its source record",
                packet.case_id
            ));
        }
        if let Some(ordinal) = span.observed_snapshot_event_ordinal {
            if !snapshot_event_references.insert((stream, ordinal)) {
                return Err(format!(
                    "{} native presentation event reference is duplicated",
                    packet.case_id
                ));
            }
            let snapshot_count = packet
                .candidate_events
                .iter()
                .filter(|event| event.stream == stream && event.event_ordinal == ordinal)
                .count();
            if snapshot_count != 1
                || !packet.candidate_events.iter().any(|event| {
                    event.stream == stream
                        && event.event_ordinal == ordinal
                        && event.kind == HotkeyTraceEventKind::NativeWindowSnapshot
                })
            {
                return Err(format!(
                    "{} native presentation event reference is dangling or has the wrong type",
                    packet.case_id
                ));
            }
        }
    }

    Ok(())
}

fn visibility_intent_proof_count(
    packet: &HotkeyCaseEvidence,
    event: &HotkeyCandidateEventEvidence,
) -> usize {
    match event.visibility_source {
        Some(HotkeyVisibilitySource::ToggleBatch) => packet
            .gestures
            .iter()
            .filter(|gesture| {
                gesture.stream == event.stream
                    && gesture.input_group_id == event.input_group_id
                    && Some(gesture.invocation_id) == event.invocation_id
                    && matches!(
                        &gesture.decision,
                        HotkeyDecisionProof::Applied {
                            visibility_revision,
                            intent_elapsed_ms,
                            visible,
                            ..
                        }
                        | HotkeyDecisionProof::Superseded {
                            visibility_revision,
                            intent_elapsed_ms,
                            visible,
                            ..
                        } if Some(*visibility_revision) == event.visibility_revision
                            && *intent_elapsed_ms == event.elapsed_ms
                            && Some(*visible) == event.visible
                    )
            })
            .count(),
        Some(HotkeyVisibilitySource::LegacyTrigger) => packet
            .standalone_decisions
            .iter()
            .filter(|decision| {
                decision.stream == event.stream
                    && decision.input_group_id == event.input_group_id
                    && decision.invocation_id.is_none()
                    && Some(decision.visibility_revision) == event.visibility_revision
                    && decision.intent_elapsed_ms == event.elapsed_ms
                    && Some(decision.visible) == event.visible
                    && decision.source == HotkeyVisibilitySource::LegacyTrigger
            })
            .count(),
        Some(HotkeyVisibilitySource::ScreenDrawRestore) => packet
            .follow_on_restorations
            .iter()
            .filter(|restore| {
                restore.stream == event.stream
                    && restore.input_group_id == event.input_group_id
                    && restore.invocation_id == event.invocation_id
                    && Some(restore.visibility_revision) == event.visibility_revision
                    && restore.intent_elapsed_ms == event.elapsed_ms
                    && Some(restore.visible) == event.visible
            })
            .count(),
        Some(HotkeyVisibilitySource::Queued) => {
            let Some(revision) = event.visibility_revision else {
                return 0;
            };
            packet
                .candidate_events
                .iter()
                .filter(|authoritative| {
                    authoritative.stream == event.stream
                        && authoritative.kind == HotkeyTraceEventKind::VisibilityIntent
                        && authoritative.visibility_source.is_some_and(|source| {
                            matches!(
                                source,
                                HotkeyVisibilitySource::ToggleBatch
                                    | HotkeyVisibilitySource::LegacyTrigger
                                    | HotkeyVisibilitySource::ScreenDrawRestore
                            )
                        })
                        && authoritative.visibility_revision == Some(revision)
                        && authoritative.visible == event.visible
                        && authoritative.elapsed_ms <= event.elapsed_ms
                        && event
                            .invocation_id
                            .is_none_or(|id| authoritative.invocation_id == Some(id))
                })
                .count()
        }
        Some(HotkeyVisibilitySource::Other) | None => 0,
    }
}

fn all_root_command_spans<'a>(
    packet: &'a HotkeyCaseEvidence,
) -> Vec<(HotkeyCandidateStream, &'a HotkeyRootCommandSpan)> {
    let mut spans = Vec::new();
    for gesture in &packet.gestures {
        if let HotkeyDecisionProof::Applied { root_commands, .. } = &gesture.decision {
            spans.extend(root_commands.iter().map(|span| (gesture.stream, span)));
        }
    }
    for decision in &packet.standalone_decisions {
        spans.extend(
            decision
                .root_commands
                .iter()
                .map(|span| (decision.stream, span)),
        );
    }
    for restore in &packet.follow_on_restorations {
        spans.extend(
            restore
                .root_commands
                .iter()
                .map(|span| (restore.stream, span)),
        );
    }
    spans
}

fn root_command_event_reference_count(
    packet: &HotkeyCaseEvidence,
    event: &HotkeyCandidateEventEvidence,
) -> usize {
    all_root_command_spans(packet)
        .iter()
        .filter(|(stream, span)| root_command_event_matches_span(event, *stream, span))
        .count()
}

fn root_command_event_matches_span(
    event: &HotkeyCandidateEventEvidence,
    stream: HotkeyCandidateStream,
    span: &HotkeyRootCommandSpan,
) -> bool {
    event.stream == stream
        && event.kind == HotkeyTraceEventKind::RootCommand
        && event.event_ordinal == span.command_event_ordinal
        && event.visibility_revision == Some(span.visibility_revision)
        && event.invocation_id == span.invocation_id
        && event.request_id == Some(span.request_id)
        && event.elapsed_ms == span.command_elapsed_ms
        && event.command == Some(span.command)
}

fn root_command_span_matches(
    packet: &HotkeyCaseEvidence,
    stream: HotkeyCandidateStream,
    span: &HotkeyRootCommandSpan,
) -> bool {
    let command_matches = packet
        .candidate_events
        .iter()
        .filter(|event| root_command_event_matches_span(event, stream, span))
        .count();
    if command_matches != 1 {
        return false;
    }
    match span.observed_snapshot_event_ordinal {
        Some(ordinal) => {
            let snapshots = packet
                .candidate_events
                .iter()
                .filter(|event| {
                    event.stream == stream
                        && event.kind == HotkeyTraceEventKind::NativeWindowSnapshot
                        && event.event_ordinal == ordinal
                        && event.visibility_revision == Some(span.visibility_revision)
                        && event.invocation_id == span.invocation_id
                        && event.request_id == Some(span.request_id)
                        && event.elapsed_ms >= span.command_elapsed_ms
                        && span.command_to_observed_ms
                            == Some(event.elapsed_ms.saturating_sub(span.command_elapsed_ms))
                        && event_root_identity_matches(packet, event)
                })
                .count();
            snapshots == 1
                && span.observed_presentation.as_ref().is_some_and(|observed| {
                    observed.elapsed_ms >= span.command_elapsed_ms
                        && observed.command_to_observed_ms
                            == observed.elapsed_ms.saturating_sub(span.command_elapsed_ms)
                })
        }
        None => span.command_to_observed_ms.is_none() && span.observed_presentation.is_none(),
    }
}

fn validate_packet_root_identity(packet: &HotkeyCaseEvidence) -> Result<(), String> {
    if packet.root_identities.is_empty()
        || packet.root_identities.len() > 3
        || packet.physical_displays.is_empty()
        || packet.physical_displays.len() > 32
        || packet
            .physical_displays
            .iter()
            .any(|bounds| bounds[2] <= bounds[0] || bounds[3] <= bounds[1])
    {
        return Err(format!(
            "{} ROOT identity or physical display evidence is missing or invalid",
            packet.case_id
        ));
    }
    for (index, identity) in packet.root_identities.iter().enumerate() {
        if identity.hwnd == 0
            || identity.process_id == 0
            || packet.root_identities[..index]
                .iter()
                .any(|prior| prior.stream == identity.stream)
        {
            return Err(format!(
                "{} captured ROOT identity is invalid or duplicated",
                packet.case_id
            ));
        }
    }
    for event in packet
        .candidate_events
        .iter()
        .filter(|event| event.kind == HotkeyTraceEventKind::NativeWindowSnapshot)
    {
        if event.hwnd.is_none()
            || event.process_id.is_none()
            || event.visible.is_none()
            || event.minimized.is_none()
            || event.bounds.is_none()
            || !event_root_identity_matches(packet, event)
        {
            return Err(format!(
                "{} qualifying native ROOT snapshot has incomplete or stale HWND/PID evidence",
                packet.case_id
            ));
        }
    }
    Ok(())
}

fn event_root_identity_matches(
    packet: &HotkeyCaseEvidence,
    event: &HotkeyCandidateEventEvidence,
) -> bool {
    let Some(hwnd) = event.hwnd else {
        return false;
    };
    let Some(process_id) = event.process_id else {
        return false;
    };
    packet.root_identities.iter().any(|identity| {
        identity.stream == event.stream
            && identity.hwnd == hwnd
            && identity.process_id == process_id
    })
}

fn presentation_matches(
    packet: &HotkeyCaseEvidence,
    stream: HotkeyCandidateStream,
    span: &HotkeyRootCommandSpan,
    observed: &HotkeyObservedPresentation,
    expected_visible: bool,
) -> bool {
    let Some(event) = packet.candidate_events.iter().find(|event| {
        event.stream == stream
            && event.kind == HotkeyTraceEventKind::NativeWindowSnapshot
            && event.visibility_revision == Some(span.visibility_revision)
            && event.invocation_id == span.invocation_id
            && event.request_id == Some(span.request_id)
            && event.elapsed_ms == observed.elapsed_ms
    }) else {
        return false;
    };
    if !root_command_span_matches(packet, stream, span)
        || !event_root_identity_matches(packet, event)
    {
        return false;
    }
    let Some(bounds) = event.bounds else {
        return false;
    };
    let on_physical_display = packet.physical_displays.iter().any(|display| {
        bounds[0] < display[2]
            && bounds[2] > display[0]
            && bounds[1] < display[3]
            && bounds[3] > display[1]
    });
    let physically_shown =
        event.visible == Some(true) && event.minimized == Some(false) && on_physical_display;
    physically_shown == expected_visible
}

fn native_activation_span_matches(
    packet: &HotkeyCaseEvidence,
    stream: HotkeyCandidateStream,
    restore: &HotkeyFollowOnRestoreEvidence,
    span: &HotkeyNativeActivationSpan,
) -> bool {
    let Some(identity) = packet
        .root_identities
        .iter()
        .find(|identity| identity.stream == stream)
    else {
        return false;
    };
    identity.hwnd == span.hwnd
        && span.request_id > 0
        && span.terminal_edge == Some(HotkeyActivationEdge::RestoreCompleted)
        && span
            .terminal_elapsed_ms
            .is_some_and(|terminal| terminal >= span.requested_elapsed_ms)
        && packet.candidate_events.iter().any(|event| {
            event.stream == stream
                && event.kind == HotkeyTraceEventKind::NativeActivation
                && event.activation_edge == Some(HotkeyActivationEdge::RestoreRequested)
                && event.request_id == Some(span.request_id)
                && event.hwnd == Some(span.hwnd)
                && event.visibility_revision == Some(restore.visibility_revision)
                && event.invocation_id == restore.invocation_id
                && event.elapsed_ms == span.requested_elapsed_ms
                && event.elapsed_ms >= restore.intent_elapsed_ms
        })
        && packet.candidate_events.iter().any(|event| {
            event.stream == stream
                && event.kind == HotkeyTraceEventKind::NativeActivation
                && event.activation_edge == Some(HotkeyActivationEdge::RestoreCompleted)
                && event.terminal == Some(true)
                && event.request_id == Some(span.request_id)
                && event.hwnd == Some(span.hwnd)
                && event.visibility_revision == Some(restore.visibility_revision)
                && event.invocation_id == restore.invocation_id
                && event.elapsed_ms == span.terminal_elapsed_ms.unwrap_or_default()
        })
}

fn configured_chord_edges(hotkey: AcceptanceHotkey, repeats: usize) -> Vec<(u32, bool)> {
    let per_gesture: &[(u32, bool)] = match hotkey {
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
    per_gesture
        .iter()
        .copied()
        .cycle()
        .take(per_gesture.len() * repeats)
        .collect()
}

fn opposite_hotkey(hotkey: AcceptanceHotkey) -> AcceptanceHotkey {
    match hotkey {
        AcceptanceHotkey::F11 => AcceptanceHotkey::ShiftAltWinEnd,
        AcceptanceHotkey::ShiftAltWinEnd => AcceptanceHotkey::F11,
    }
}

fn is_legacy_trigger_without_reducer_id(proof: &HotkeyDecisionProof) -> bool {
    matches!(
        proof,
        HotkeyDecisionProof::NotApplicable {
            reason: HotkeyEvidenceNotApplicable::LegacyTriggerHasNoInvocationReducerId
        }
    )
}

fn is_h16_legacy_trigger_pair_group(
    packet: &HotkeyCaseEvidence,
    stream: HotkeyCandidateStream,
    group_id: u32,
    purpose: HotkeyRunnerInputPurpose,
) -> bool {
    if packet.case_id != "H16"
        || stream != HotkeyCandidateStream::LegacyFallbackCandidate
        || purpose != HotkeyRunnerInputPurpose::LauncherChord
    {
        return false;
    }

    let paired_gestures = packet
        .gestures
        .iter()
        .filter(|gesture| {
            gesture.stream == stream
                && gesture.input_group_id == group_id
                && gesture.input_purpose == purpose
                && gesture.short_tap_elapsed_ms.is_none()
                && is_legacy_trigger_without_reducer_id(&gesture.decision)
        })
        .count();
    let standalone_legacy_triggers = packet
        .standalone_decisions
        .iter()
        .filter(|decision| {
            decision.stream == stream
                && decision.input_group_id == group_id
                && decision.input_purpose == purpose
                && decision.invocation_id.is_none()
                && decision.source == HotkeyVisibilitySource::LegacyTrigger
                && is_legacy_trigger_without_reducer_id(&decision.decision)
        })
        .count();

    paired_gestures == 1 && standalone_legacy_triggers == 1
}

fn validate_runner_edge_groups(
    packet: &HotkeyCaseEvidence,
    configured_hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) -> Result<(), String> {
    use std::collections::BTreeMap;

    let mut groups = BTreeMap::<
        (HotkeyCandidateStream, u32, HotkeyRunnerInputPurpose),
        Vec<&HotkeyRunnerEdgeEvidence>,
    >::new();
    for edge in &packet.runner_edges {
        groups
            .entry((edge.stream, edge.input_group_id, edge.purpose))
            .or_default()
            .push(edge);
    }
    if groups.is_empty() {
        return Err(format!(
            "{} has no runner input edge groups",
            packet.case_id
        ));
    }
    for ((stream, group_id, purpose), edges) in groups.iter_mut() {
        edges.sort_by_key(|edge| edge.runner_relative_us);
        let expected_repeats = if *purpose == HotkeyRunnerInputPurpose::DirectTrigger {
            1
        } else {
            let gesture_count = packet
                .gestures
                .iter()
                .filter(|gesture| {
                    gesture.stream == *stream
                        && gesture.input_group_id == *group_id
                        && gesture.input_purpose == *purpose
                })
                .count();
            let standalone_count = packet
                .standalone_decisions
                .iter()
                .filter(|decision| {
                    decision.stream == *stream
                        && decision.input_group_id == *group_id
                        && decision.input_purpose == *purpose
                })
                .count();
            gesture_count + standalone_count
                - usize::from(is_h16_legacy_trigger_pair_group(
                    packet, *stream, *group_id, *purpose,
                ))
        };
        let tagged = edges
            .iter()
            .filter(|edge| edge.injected && edge.runner_cookie_matched)
            .map(|edge| {
                (
                    edge.virtual_key,
                    edge.transition == HotkeyEdgeTransition::Press,
                )
            })
            .collect::<Vec<_>>();
        let group_hotkey = match (*stream, packet.case_id.as_str()) {
            (HotkeyCandidateStream::AlternateProfileCandidate, "H17")
            | (HotkeyCandidateStream::LegacyFallbackCandidate, "H16") => {
                opposite_hotkey(configured_hotkey)
            }
            _ => configured_hotkey,
        };
        let expected = if *purpose == HotkeyRunnerInputPurpose::DirectTrigger {
            let key = if *stream == HotkeyCandidateStream::LegacyFallbackCandidate {
                0x59
            } else {
                0x54
            };
            vec![
                (0xA2, true),
                (0xA4, true),
                (key, true),
                (key, false),
                (0xA4, false),
                (0xA2, false),
            ]
        } else {
            configured_chord_edges(group_hotkey, expected_repeats)
        };
        if expected_repeats == 0 || tagged != expected {
            return Err(format!(
                "{} tagged edge group {group_id} ({stream:?}/{purpose:?}) is partial, duplicated, or has the wrong key sequence",
                packet.case_id
            ));
        }
        if edges
            .iter()
            .any(|edge| edge.runner_cookie_matched && !edge.injected)
        {
            return Err(format!(
                "{} runner cookie is present on a non-injected edge",
                packet.case_id
            ));
        }
        if packet.case_id == "H04" && *purpose == HotkeyRunnerInputPurpose::MatrixBurst {
            let tagged_edges = edges
                .iter()
                .copied()
                .filter(|edge| edge.injected && edge.runner_cookie_matched)
                .collect::<Vec<_>>();
            validate_h04_burst_cadence(&tagged_edges, group_hotkey, expected_repeats, *group_id)?;
        }
    }

    for gesture in &packet.gestures {
        let chord = match (gesture.stream, packet.case_id.as_str()) {
            (HotkeyCandidateStream::AlternateProfileCandidate, "H17")
            | (HotkeyCandidateStream::LegacyFallbackCandidate, "H16") => {
                opposite_hotkey(configured_hotkey)
            }
            _ => configured_hotkey,
        };
        let duration_us = tagged_primary_duration_us(
            packet,
            gesture.stream,
            gesture.input_group_id,
            gesture.input_purpose,
            gesture.release_elapsed_ms,
            chord,
        );
        let Some(duration_us) = duration_us else {
            return Err(format!(
                "{} gesture has no complete tagged primary key down/up pair",
                packet.case_id
            ));
        };
        let threshold_us = hold_threshold_ms.saturating_mul(1_000);
        if gesture.short_tap_elapsed_ms.is_some() && duration_us >= threshold_us {
            return Err(format!(
                "{} short tap exceeded the configured hold threshold",
                packet.case_id
            ));
        }
        if gesture.short_tap_elapsed_ms.is_none() && duration_us < threshold_us {
            let paired_legacy_trigger =
                is_h16_legacy_trigger_pair_group(
                    packet,
                    gesture.stream,
                    gesture.input_group_id,
                    gesture.input_purpose,
                ) && is_legacy_trigger_without_reducer_id(&gesture.decision);
            if !paired_legacy_trigger {
                return Err(format!(
                    "{} no-tap gesture lacks a tagged hold at the configured threshold",
                    packet.case_id
                ));
            }
        }
    }
    Ok(())
}

fn validate_h04_burst_cadence(
    edges: &[&HotkeyRunnerEdgeEvidence],
    hotkey: AcceptanceHotkey,
    taps: usize,
    group_id: u32,
) -> Result<(), String> {
    let edges_per_tap = match hotkey {
        AcceptanceHotkey::F11 => 2,
        AcceptanceHotkey::ShiftAltWinEnd => 8,
    };
    if edges.len() != taps.saturating_mul(edges_per_tap) {
        return Err(format!(
            "H04 matrix burst group {group_id} has incomplete tagged cadence edges"
        ));
    }
    let primary_down_index = match hotkey {
        AcceptanceHotkey::F11 => 0,
        AcceptanceHotkey::ShiftAltWinEnd => 3,
    };
    let mut previous_release_us = None;
    for (tap_index, chunk) in edges.chunks_exact(edges_per_tap).enumerate() {
        let down = chunk[primary_down_index].runner_relative_us;
        let up = chunk[primary_down_index + 1].runner_relative_us;
        let hold_us = up.saturating_sub(down);
        if !(10_000..=100_000).contains(&hold_us) {
            return Err(format!(
                "H04 matrix burst group {group_id} tap {} has an out-of-band hold duration",
                tap_index + 1
            ));
        }
        if let Some(previous_up_us) = previous_release_us {
            let gap_us = chunk[0].runner_relative_us.saturating_sub(previous_up_us);
            if !(10_000..=100_000).contains(&gap_us) {
                return Err(format!(
                    "H04 matrix burst group {group_id} tap {} has an out-of-band released gap",
                    tap_index + 1
                ));
            }
        }
        previous_release_us = chunk.last().map(|edge| edge.runner_relative_us);
    }
    Ok(())
}

fn validate_h04_matrix_foreign_edges(packet: &HotkeyCaseEvidence) -> Result<(), String> {
    use std::collections::BTreeMap;

    let mut groups =
        BTreeMap::<(HotkeyCandidateStream, u32), Vec<&HotkeyRunnerEdgeEvidence>>::new();
    for edge in packet
        .runner_edges
        .iter()
        .filter(|edge| edge.purpose == HotkeyRunnerInputPurpose::MatrixBurst)
    {
        groups
            .entry((edge.stream, edge.input_group_id))
            .or_default()
            .push(edge);
    }
    for ((stream, group_id), edges) in groups {
        let mut ordered_owned = edges
            .iter()
            .copied()
            .filter(|edge| edge.injected && edge.runner_cookie_matched)
            .collect::<Vec<_>>();
        ordered_owned.sort_by_key(|edge| edge.runner_relative_us);
        let spans = owned_gesture_spans(ordered_owned.iter().map(|edge| {
            (
                edge.runner_relative_us,
                edge.virtual_key,
                edge.transition == HotkeyEdgeTransition::Press,
            )
        }));
        let mut foreign_edges = edges
            .iter()
            .copied()
            .filter(|edge| !edge.runner_cookie_matched)
            .collect::<Vec<_>>();
        foreign_edges.sort_by_key(|edge| edge.runner_relative_us);
        let foreign_timeline = foreign_edges
            .iter()
            .map(|edge| {
                (
                    edge.runner_relative_us,
                    edge.virtual_key,
                    edge.transition == HotkeyEdgeTransition::Press,
                )
            })
            .collect::<Vec<_>>();
        if !foreign_edge_indices_interfering_owned_spans(&spans, &foreign_timeline).is_empty() {
            return Err(format!(
                "H04 matrix burst group {group_id} ({stream:?}) contains an interfering foreign matching-key edge"
            ));
        }
    }
    Ok(())
}

fn tagged_primary_duration_us(
    packet: &HotkeyCaseEvidence,
    stream: HotkeyCandidateStream,
    group_id: u32,
    purpose: HotkeyRunnerInputPurpose,
    release_elapsed_ms: u64,
    hotkey: AcceptanceHotkey,
) -> Option<u64> {
    let edges_per_gesture = match hotkey {
        AcceptanceHotkey::F11 => 2,
        AcceptanceHotkey::ShiftAltWinEnd => 8,
    };
    let mut groups = packet
        .gestures
        .iter()
        .filter(|gesture| {
            gesture.stream == stream
                && gesture.input_group_id == group_id
                && gesture.input_purpose == purpose
        })
        .collect::<Vec<_>>();
    groups.sort_by_key(|gesture| gesture.release_elapsed_ms);
    let gesture_index = groups
        .iter()
        .position(|gesture| gesture.release_elapsed_ms == release_elapsed_ms)?;
    let mut edges = packet
        .runner_edges
        .iter()
        .filter(|edge| {
            edge.stream == stream
                && edge.input_group_id == group_id
                && edge.purpose == purpose
                && edge.injected
                && edge.runner_cookie_matched
        })
        .collect::<Vec<_>>();
    edges.sort_by_key(|edge| edge.runner_relative_us);
    let base = gesture_index.checked_mul(edges_per_gesture)?;
    let offset = match hotkey {
        AcceptanceHotkey::F11 => 0,
        AcceptanceHotkey::ShiftAltWinEnd => 3,
    };
    let down = *edges.get(base.checked_add(offset)?)?;
    let up = *edges.get(base.checked_add(offset + 1)?)?;
    if down.transition != HotkeyEdgeTransition::Press
        || up.transition != HotkeyEdgeTransition::Release
        || down.virtual_key != up.virtual_key
    {
        return None;
    }
    up.runner_relative_us.checked_sub(down.runner_relative_us)
}

fn gesture_has_tagged_hold(
    packet: &HotkeyCaseEvidence,
    gesture: &HotkeyGestureEvidence,
    hotkey: AcceptanceHotkey,
    threshold_ms: u64,
) -> bool {
    tagged_primary_duration_us(
        packet,
        gesture.stream,
        gesture.input_group_id,
        gesture.input_purpose,
        gesture.release_elapsed_ms,
        hotkey,
    )
    .is_some_and(|duration| duration >= threshold_ms.saturating_mul(1_000))
}

fn hotkey_decision_visible(decision: &HotkeyDecisionProof) -> Option<bool> {
    match decision {
        HotkeyDecisionProof::Applied { visible, .. }
        | HotkeyDecisionProof::Superseded { visible, .. } => Some(*visible),
        HotkeyDecisionProof::NotApplicable { .. } => None,
    }
}

fn validate_hotkey_evidence_report(report: &AcceptanceReport) -> Result<(), String> {
    if let Some(receipt) = &report.report_overflow {
        if !report.capacity_saturated
            || report.outcome != "failed"
            || (receipt.omitted_case_evidence > 0 && !report.hotkey_evidence.is_empty())
            || receipt.reason.is_empty()
            || receipt.affected_case_ids.len() > MAX_HOTKEY_EVIDENCE_CASES
            || receipt.affected_case_ids.iter().any(|id| {
                hotkey_expected_state(id).is_none()
                    || !report
                        .cases
                        .iter()
                        .any(|case| case.id == *id && matches!(case.status, CaseStatus::Failed))
            })
            || !["R0", "CLEANUP"].iter().all(|id| {
                report
                    .cases
                    .iter()
                    .any(|case| case.id == *id && matches!(case.status, CaseStatus::Failed))
            })
        {
            return Err(
                "report overflow receipt does not preserve failed case identity and cleanup".into(),
            );
        }
        if receipt.omitted_case_evidence > 0 {
            return Ok(());
        }
    }
    if report.hotkey_evidence.len() > MAX_HOTKEY_EVIDENCE_CASES {
        return Err("per-case H evidence packet count exceeds its bound".into());
    }
    for (index, packet) in report.hotkey_evidence.iter().enumerate() {
        if report.hotkey_evidence[..index]
            .iter()
            .any(|previous| previous.case_id == packet.case_id)
        {
            return Err(format!(
                "{} has duplicate H evidence packets",
                packet.case_id
            ));
        }
        validate_hotkey_evidence_packet_with_context(
            packet,
            report.hotkey,
            report.profile.hold_threshold_ms,
        )?;
        if !report.cases.iter().any(|case| case.id == packet.case_id) {
            return Err(format!(
                "{} evidence packet has no case result",
                packet.case_id
            ));
        }
    }
    for case in &report.cases {
        if !matches!(case.status, CaseStatus::Passed) || hotkey_expected_state(&case.id).is_none() {
            continue;
        }
        if report
            .hotkey_evidence
            .iter()
            .filter(|packet| packet.case_id == case.id)
            .count()
            != 1
        {
            return Err(format!(
                "{} passed without exactly one H evidence packet",
                case.id
            ));
        }
    }
    if let Some(case) = report
        .cases
        .iter()
        .find(|case| case.id == "H04" && matches!(case.status, CaseStatus::Passed))
    {
        let reported_foreign_edges = observed_field(&case.observed, "foreign_matching_edges=")
            .and_then(|value| value.parse::<usize>().ok())
            .ok_or_else(|| "H04 report omitted its foreign matching-edge count".to_string())?;
        let packet = report
            .hotkey_evidence
            .iter()
            .find(|packet| packet.case_id == "H04")
            .ok_or_else(|| "H04 report omitted its runner-edge packet".to_string())?;
        let captured_foreign_edges = packet
            .runner_edges
            .iter()
            .filter(|edge| {
                edge.purpose == HotkeyRunnerInputPurpose::MatrixBurst && !edge.runner_cookie_matched
            })
            .count();
        if reported_foreign_edges != captured_foreign_edges {
            return Err(format!(
                "H04 foreign matching-edge summary {reported_foreign_edges} disagrees with the {} captured matrix edges",
                captured_foreign_edges
            ));
        }
    }
    if let Some(case) = report.cases.iter().find(|case| case.id == "H04") {
        validate_h04_contamination_artifacts(report, case)?;
    }
    Ok(())
}

fn validate_h04_contamination_artifacts(
    report: &AcceptanceReport,
    case: &AcceptanceCaseResult,
) -> Result<(), String> {
    let observed = &case.observed;
    let (attempts, contamination_count, names) = h04_retry_ledger(case.status, observed)?;

    let mut artifact_attempts = Vec::new();
    for name in names {
        if name.len() > 96
            || !name.starts_with("case-H04-attempt-")
            || !name.ends_with("-contamination.json")
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        {
            return Err("H04 retry ledger contains an invalid contamination artifact name".into());
        }
        let case_paths = case
            .artifacts
            .iter()
            .filter(|path| {
                Path::new(path).file_name().and_then(|value| value.to_str()) == Some(name)
            })
            .collect::<Vec<_>>();
        let report_paths = report
            .artifacts
            .iter()
            .filter(|path| {
                Path::new(path.as_str())
                    .file_name()
                    .and_then(|value| value.to_str())
                    == Some(name)
            })
            .collect::<Vec<_>>();
        if case_paths.len() != 1 || report_paths.len() != 1 {
            return Err(format!(
                "H04 contamination artifact {name} is not linked exactly once from both case and report"
            ));
        }
        if case_paths[0] != report_paths[0] {
            return Err(format!(
                "H04 contamination artifact {name} has different case/report paths"
            ));
        }
        let path = Path::new(case_paths[0]);
        let bytes = fs::read(path)
            .map_err(|error| format!("read H04 contamination artifact {name}: {error}"))?;
        if bytes.is_empty() || bytes.len() > MAX_HOTKEY_CASE_EVIDENCE_BYTES {
            return Err(format!(
                "H04 contamination artifact {name} is empty or exceeds its byte bound"
            ));
        }
        let artifact: H04InputContaminationArtifact = serde_json::from_slice(&bytes)
            .map_err(|error| format!("decode H04 contamination artifact {name}: {error}"))?;
        validate_h04_contamination_artifact(&artifact, report.hotkey)?;
        let expected_name = format!("case-H04-attempt-{}-contamination.json", artifact.attempt);
        if name != expected_name {
            return Err(format!(
                "H04 contamination artifact {name} disagrees with its attempt identity"
            ));
        }
        artifact_attempts.push(artifact.attempt);
    }
    artifact_attempts.sort_unstable();
    let expected_attempts = (1..=contamination_count).collect::<Vec<_>>();
    if artifact_attempts != expected_attempts || attempts > 2 {
        return Err(
            "H04 retry artifacts do not identify every contaminated attempt in order".into(),
        );
    }
    Ok(())
}

fn validate_h04_contamination_artifact(
    artifact: &H04InputContaminationArtifact,
    report_hotkey: AcceptanceHotkey,
) -> Result<(), String> {
    let expected_vks: &[u32] = match report_hotkey {
        AcceptanceHotkey::F11 => &[0x7A],
        AcceptanceHotkey::ShiftAltWinEnd => &[0xA0, 0xA4, 0x5B, 0x23],
    };
    if artifact.schema_version != 1
        || artifact.case_id != "H04"
        || !(1..=2).contains(&artifact.attempt)
        || artifact.hotkey != report_hotkey
        || artifact.failure_stage != "InputInjection"
        || artifact.failure.is_empty()
        || artifact.failure.len() > MAX_RESULT_BYTES
        || artifact.declared_initial_state
        || !(1..=10).contains(&artifact.next_matrix_burst_index)
        || artifact.completed_bursts.len() + 1 != usize::from(artifact.next_matrix_burst_index)
        || artifact.input_group_id == 0
        || artifact.input_group_id != u32::from(artifact.next_matrix_burst_index)
        || artifact.root_identity.hwnd == 0
        || artifact.root_identity.process_id == 0
        || artifact.root_identity.stream != artifact.stream
        || artifact.prior_group_ids.is_empty()
        || artifact.prior_group_ids.len() > 13
        || artifact.prior_group_ids.iter().any(|group| *group == 0)
        || artifact
            .prior_group_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || !artifact.prior_group_ids.contains(&artifact.input_group_id)
        || artifact.prior_group_ids.len() != usize::from(artifact.next_matrix_burst_index)
        || artifact
            .prior_group_ids
            .iter()
            .enumerate()
            .any(|(index, group)| *group != u32::try_from(index + 1).unwrap_or(u32::MAX))
    {
        return Err("H04 contamination artifact identity or bounds are invalid".into());
    }
    let required_lengths = [1usize, 2, 5, 10, 25];
    let mut completed_invocations = std::collections::BTreeSet::new();
    for (index, burst) in artifact.completed_bursts.iter().enumerate() {
        let expected_initial = index >= required_lengths.len();
        let taps = required_lengths[index % required_lengths.len()];
        let expected_final = expected_initial ^ (taps % 2 == 1);
        let valid_cadence = (10..=100).contains(&burst.hold_min_ms)
            && (10..=100).contains(&burst.hold_max_ms)
            && burst.hold_min_ms <= burst.hold_max_ms
            && if taps == 1 {
                burst.gap_min_ms == 0 && burst.gap_max_ms == 0
            } else {
                (10..=100).contains(&burst.gap_min_ms)
                    && (10..=100).contains(&burst.gap_max_ms)
                    && burst.gap_min_ms <= burst.gap_max_ms
            };
        if burst.matrix_burst_index != u8::try_from(index + 1).unwrap_or(u8::MAX)
            || burst.input_group_id != u32::try_from(index + 1).unwrap_or(u32::MAX)
            || burst.initial_visible != expected_initial
            || usize::from(burst.requested_taps) != taps
            || burst.final_visible != expected_final
            || burst.invocation_ids.len() != taps
            || burst
                .invocation_ids
                .iter()
                .any(|id| *id == 0 || !completed_invocations.insert(*id))
            || burst.trace_probe_id == 0
            || !valid_cadence
        {
            return Err("H04 contamination artifact has an invalid completed-burst prefix".into());
        }
    }
    let group_edges = artifact
        .owned_edges
        .iter()
        .filter(|edge| {
            edge.input_group_id == artifact.input_group_id && edge.stream == artifact.stream
        })
        .collect::<Vec<_>>();
    let group_foreign_edges = artifact
        .foreign_edges
        .iter()
        .filter(|edge| {
            edge.input_group_id == artifact.input_group_id && edge.stream == artifact.stream
        })
        .collect::<Vec<_>>();
    if group_edges.len() != artifact.owned_edges.len()
        || group_foreign_edges.len() != artifact.foreign_edges.len()
        || group_edges.is_empty()
        || group_edges.iter().any(|edge| {
            edge.purpose != HotkeyRunnerInputPurpose::MatrixBurst
                || !edge.runner_cookie_matched
                || !edge.injected
                || !expected_vks.contains(&edge.virtual_key)
        })
        || group_foreign_edges.is_empty()
        || group_foreign_edges.iter().any(|edge| {
            edge.purpose != HotkeyRunnerInputPurpose::MatrixBurst
                || edge.runner_cookie_matched
                || !expected_vks.contains(&edge.virtual_key)
        })
    {
        return Err(
            "H04 contamination artifact does not preserve owned and foreign chord edges".into(),
        );
    }
    let edges_per_tap = if report_hotkey == AcceptanceHotkey::F11 {
        2
    } else {
        8
    };
    let taps = group_edges.len() / edges_per_tap;
    let expected_taps = [1usize, 2, 5, 10, 25];
    if group_edges.len() != taps.saturating_mul(edges_per_tap) || !expected_taps.contains(&taps) {
        return Err("H04 contamination artifact has an invalid measured-burst length".into());
    }
    let expected_edges = (0..taps)
        .flat_map(|_| match report_hotkey {
            AcceptanceHotkey::F11 => vec![
                (0x7A, HotkeyEdgeTransition::Press),
                (0x7A, HotkeyEdgeTransition::Release),
            ],
            AcceptanceHotkey::ShiftAltWinEnd => vec![
                (0xA0, HotkeyEdgeTransition::Press),
                (0xA4, HotkeyEdgeTransition::Press),
                (0x5B, HotkeyEdgeTransition::Press),
                (0x23, HotkeyEdgeTransition::Press),
                (0x23, HotkeyEdgeTransition::Release),
                (0x5B, HotkeyEdgeTransition::Release),
                (0xA4, HotkeyEdgeTransition::Release),
                (0xA0, HotkeyEdgeTransition::Release),
            ],
        })
        .collect::<Vec<_>>();
    if group_edges
        .iter()
        .zip(expected_edges)
        .any(|(edge, (vk, transition))| edge.virtual_key != vk || edge.transition != transition)
    {
        return Err(
            "H04 contamination artifact does not preserve the complete tagged chord sequence"
                .into(),
        );
    }
    let spans = owned_gesture_spans(group_edges.iter().map(|edge| {
        (
            edge.runner_relative_us,
            edge.virtual_key,
            edge.transition == HotkeyEdgeTransition::Press,
        )
    }));
    let foreign_timeline = group_foreign_edges
        .iter()
        .map(|edge| {
            (
                edge.runner_relative_us,
                edge.virtual_key,
                edge.transition == HotkeyEdgeTransition::Press,
            )
        })
        .collect::<Vec<_>>();
    let interfering_edges = foreign_edge_indices_interfering_owned_spans(&spans, &foreign_timeline);
    if interfering_edges.is_empty() {
        return Err(
            "H04 contamination artifact has no foreign edge inside an owned gesture or held across its boundary".into(),
        );
    }
    if artifact.candidate_events.iter().any(|event| {
        event.stream != artifact.stream || event.input_group_id != artifact.input_group_id
    }) {
        return Err("H04 contamination artifact contains unrelated candidate events".into());
    }
    Ok(())
}

fn owned_gesture_spans<T>(edges: impl IntoIterator<Item = (T, u32, bool)>) -> Vec<(T, T)>
where
    T: Copy + Ord,
{
    let mut edges = edges.into_iter().collect::<Vec<_>>();
    edges.sort_by_key(|(at, _, _)| *at);
    let mut active = std::collections::BTreeMap::<u32, usize>::new();
    let mut gesture_start = None;
    let mut spans = Vec::new();

    for (at, virtual_key, down) in &edges {
        if *down {
            if active.is_empty() {
                gesture_start = Some(*at);
            }
            *active.entry(*virtual_key).or_default() += 1;
            continue;
        }
        let Some(count) = active.get_mut(virtual_key) else {
            continue;
        };
        *count -= 1;
        if *count == 0 {
            active.remove(virtual_key);
        }
        if active.is_empty()
            && let Some(start) = gesture_start.take()
        {
            spans.push((start, *at));
        }
    }

    if !active.is_empty()
        && let (Some(start), Some((end, _, _))) = (gesture_start, edges.last())
    {
        spans.push((start, *end));
    }
    spans
}

fn foreign_edge_indices_interfering_owned_spans<T>(
    spans: &[(T, T)],
    foreign_edges: &[(T, u32, bool)],
) -> Vec<usize>
where
    T: Copy + Ord,
{
    if spans.is_empty() || foreign_edges.is_empty() {
        return Vec::new();
    }
    let mut ordered_indices = (0..foreign_edges.len()).collect::<Vec<_>>();
    ordered_indices.sort_by_key(|index| foreign_edges[*index].0);
    let mut edge_cursor = 0usize;
    let mut held = std::collections::BTreeMap::<u32, Vec<usize>>::new();
    let mut interfering = std::collections::BTreeSet::new();

    let apply_edge = |index: usize,
                      held: &mut std::collections::BTreeMap<u32, Vec<usize>>,
                      interfering: &mut std::collections::BTreeSet<usize>| {
        let (_, virtual_key, down) = foreign_edges[index];
        if down {
            held.entry(virtual_key).or_default().push(index);
        } else if let Some(indices) = held.get_mut(&virtual_key) {
            indices.pop();
            if indices.is_empty() {
                held.remove(&virtual_key);
            }
        } else {
            // A release without a captured matching down cannot be part of a
            // balanced pair in the released gap; it may have cleared a key
            // state needed by the next owned gesture.
            interfering.insert(index);
        }
    };
    let mark_held = |held: &std::collections::BTreeMap<u32, Vec<usize>>,
                     interfering: &mut std::collections::BTreeSet<usize>| {
        interfering.extend(held.values().flatten().copied());
    };

    for (start, end) in spans {
        if end < start {
            continue;
        }
        while edge_cursor < ordered_indices.len()
            && foreign_edges[ordered_indices[edge_cursor]].0 < *start
        {
            apply_edge(ordered_indices[edge_cursor], &mut held, &mut interfering);
            edge_cursor += 1;
        }
        mark_held(&held, &mut interfering);
        while edge_cursor < ordered_indices.len()
            && foreign_edges[ordered_indices[edge_cursor]].0 <= *end
        {
            let index = ordered_indices[edge_cursor];
            interfering.insert(index);
            apply_edge(index, &mut held, &mut interfering);
            edge_cursor += 1;
        }
    }

    while edge_cursor < ordered_indices.len() {
        apply_edge(ordered_indices[edge_cursor], &mut held, &mut interfering);
        edge_cursor += 1;
    }
    mark_held(&held, &mut interfering);
    interfering.into_iter().collect()
}

#[derive(Clone, Default, Serialize)]
struct CleanupResult {
    child_closed_normally: bool,
    child_terminated_after_timeout: bool,
    child_owned_windows_closed: bool,
    profile_removed: bool,
    foreground_restore_captured: bool,
    foreground_restore_attempted: bool,
    foreground_restored: bool,
    cursor_restored: bool,
    input_desktop_released: bool,
}

#[derive(Clone, Serialize)]
struct AcceptanceReport {
    schema_version: u16,
    run_id: String,
    mode: &'static str,
    started_unix_ms: u128,
    finished_unix_ms: u128,
    copied_profile_status: CopiedProfileStatus,
    copied_profile: Option<CopiedProfileSummary>,
    private_artifacts: Option<private_artifacts::PrivateArtifactSummary>,
    h6_repeat_mode: H6RepeatMode,
    mouse_gesture_mode: MouseGestureMode,
    suite: AcceptanceSuite,
    hotkey: AcceptanceHotkey,
    outcome: &'static str,
    candidate: CandidateIdentity,
    environment: EnvironmentIdentity,
    profile: ProfileIdentity,
    cases: Vec<AcceptanceCaseResult>,
    hotkey_evidence: Vec<HotkeyCaseEvidence>,
    artifacts: Vec<String>,
    cleanup: CleanupResult,
    capacity_saturated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    report_overflow: Option<ReportOverflowReceipt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CopiedProfileStatus {
    NotRun,
    Running,
    Passed,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
struct CopiedProfileSummary {
    source_tree_sha256_before: String,
    copied_initial_tree_sha256: String,
    source_tree_sha256_after: Option<String>,
    copied_file_count: usize,
    copied_total_bytes: u64,
    source_settings_sha256: String,
    source_radial_sha256: String,
    source_actions_sha256: Option<String>,
    copied_initial_settings_sha256: String,
    copied_initial_radial_sha256: String,
    copied_initial_actions_sha256: Option<String>,
    launch_settings_sha256: String,
    launch_radial_sha256: String,
    launch_actions_sha256: String,
    source_unchanged: bool,
}

impl AcceptanceReport {
    fn push_case(&mut self, mut case: AcceptanceCaseResult) {
        let is_reserved_evidence = matches!(case.id.as_str(), "CLEANUP" | "R0");
        let ordinary_limit = MAX_CASES.saturating_sub(2);
        let has_capacity = self.cases.len() < MAX_CASES
            && (is_reserved_evidence || self.cases.len() < ordinary_limit);
        if has_capacity {
            if is_reserved_evidence && self.capacity_saturated {
                case.status = CaseStatus::Failed;
                case.failure_stage = Some(FailureStage::Environment);
                case.observed =
                    "bounded report capacity was exceeded; reserved integrity and cleanup evidence is explicitly failed".into();
            }
            self.cases.push(case);
        } else {
            self.mark_capacity_saturated();
        }
    }

    fn push_artifact(&mut self, path: impl AsRef<str>) {
        if self.artifacts.len() < MAX_ARTIFACTS {
            self.artifacts
                .push(bounded_text(path.as_ref(), MAX_PATH_BYTES));
        } else {
            self.mark_capacity_saturated();
        }
    }

    fn mark_capacity_saturated(&mut self) {
        self.capacity_saturated = true;
        for case in self
            .cases
            .iter_mut()
            .filter(|case| matches!(case.id.as_str(), "CLEANUP" | "R0"))
        {
            case.status = CaseStatus::Failed;
            case.failure_stage = Some(FailureStage::Environment);
            case.observed =
                "bounded report capacity was exceeded; one or more evidence records were omitted"
                    .into();
        }
    }

    fn passed(&self) -> bool {
        self.passed_native_cases()
            && !matches!(
                self.copied_profile_status,
                CopiedProfileStatus::Running | CopiedProfileStatus::Failed
            )
    }

    fn passed_native_cases(&self) -> bool {
        let case_ids = if self.mode == "native_windows_copied_profile" {
            &COPIED_CASE_IDS[..]
        } else if self.suite == AcceptanceSuite::Hotkey {
            &HOTKEY_CASE_IDS[..]
        } else {
            &CASE_IDS[..]
        };
        !self.capacity_saturated
            && self.cases.len() == case_ids.len()
            && self.cases.iter().enumerate().all(|(index, case)| {
                self.cases[..index]
                    .iter()
                    .all(|previous| previous.id != case.id)
            })
            && self
                .cases
                .iter()
                .all(|case| matches!(case.status, CaseStatus::Passed))
            && case_ids
                .iter()
                .all(|id| self.cases.iter().any(|case| case.id == *id))
            && self.cleanup.child_closed_normally
            && self.cleanup.child_owned_windows_closed
            && self.cleanup.profile_removed
            && (!self.cleanup.foreground_restore_captured || self.cleanup.foreground_restored)
            && self.cleanup.cursor_restored
            && self.cleanup.input_desktop_released
    }
}

struct DeterministicFixture {
    settings_json: Vec<u8>,
    radial_json: Vec<u8>,
    actions_json: Vec<u8>,
    hold_threshold_ms: u64,
}

#[derive(Clone, Debug)]
struct CopiedProfileMetadata {
    initial_copy_tree_sha256: String,
    copied_file_count: usize,
    copied_total_bytes: u64,
    source_settings_sha256: String,
    source_radial_sha256: String,
    source_actions_sha256: Option<String>,
    copied_initial_settings_sha256: String,
    copied_initial_radial_sha256: String,
    copied_initial_actions_sha256: Option<String>,
    launch_settings_sha256: String,
    launch_radial_sha256: String,
    launch_actions_sha256: String,
    target_action_index: usize,
    skin_index: usize,
    restore_menu_name: String,
    original_menu_sha256: Vec<String>,
    expected_submenu_migration_receipt: Option<SubmenuPresentationMigrationReceipt>,
    hold_threshold_ms: u64,
}

impl CopiedProfileMetadata {
    fn report_summary(
        &self,
        source_inventory: &copied_profile::ProfileInventory,
        source_tree_sha256_after: Option<String>,
        source_unchanged: bool,
    ) -> CopiedProfileSummary {
        CopiedProfileSummary {
            source_tree_sha256_before: source_inventory.tree_sha256.clone(),
            copied_initial_tree_sha256: self.initial_copy_tree_sha256.clone(),
            source_tree_sha256_after,
            copied_file_count: self.copied_file_count,
            copied_total_bytes: self.copied_total_bytes,
            source_settings_sha256: self.source_settings_sha256.clone(),
            source_radial_sha256: self.source_radial_sha256.clone(),
            source_actions_sha256: self.source_actions_sha256.clone(),
            copied_initial_settings_sha256: self.copied_initial_settings_sha256.clone(),
            copied_initial_radial_sha256: self.copied_initial_radial_sha256.clone(),
            copied_initial_actions_sha256: self.copied_initial_actions_sha256.clone(),
            launch_settings_sha256: self.launch_settings_sha256.clone(),
            launch_radial_sha256: self.launch_radial_sha256.clone(),
            launch_actions_sha256: self.launch_actions_sha256.clone(),
            source_unchanged,
        }
    }
}

enum ParseResult {
    Help,
    Run(Arguments),
}

fn main() -> ExitCode {
    match parse_arguments(std::env::args_os().skip(1)) {
        Ok(ParseResult::Help) => {
            print_usage();
            ExitCode::SUCCESS
        }
        Ok(ParseResult::Run(arguments)) => match run(arguments) {
            Ok((path, passed)) => {
                println!(
                    "{} radial acceptance report: {}",
                    if passed { "PASS" } else { "FAIL" },
                    path.display()
                );
                if passed {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(error) => {
                eprintln!("radial_acceptance could not start: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("{error}");
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn parse_arguments(args: impl IntoIterator<Item = OsString>) -> Result<ParseResult, String> {
    let mut launcher = None;
    let mut output = None;
    let mut report_file = None;
    let mut profile_copy = None;
    let mut source_revision = None;
    let mut keep_profile_on_failure = false;
    let mut h6_repeat_mode = H6RepeatMode::Quiescent;
    let mut mouse_gesture_mode = MouseGestureMode::Enabled;
    let mut suite = AcceptanceSuite::All;
    let mut hotkey = AcceptanceHotkey::F11;
    let mut args = args.into_iter();

    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--help" | "-h") => return Ok(ParseResult::Help),
            Some("--keep-profile-on-failure") => keep_profile_on_failure = true,
            Some("--launcher" | "--candidate") => {
                launcher = Some(next_path(&mut args, "--launcher")?);
            }
            Some("--output") => output = Some(next_path(&mut args, "--output")?),
            Some("--report") => report_file = Some(next_path(&mut args, "--report")?),
            Some("--profile-copy") => {
                profile_copy = Some(next_path(&mut args, "--profile-copy")?);
            }
            Some("--suite") => {
                let value = args
                    .next()
                    .ok_or_else(|| "--suite requires all or hotkey".to_string())?;
                suite = match value.to_str() {
                    Some("all") => AcceptanceSuite::All,
                    Some("hotkey") => AcceptanceSuite::Hotkey,
                    _ => return Err("--suite must be all or hotkey".into()),
                };
            }
            Some("--hotkey") => {
                let value = args
                    .next()
                    .ok_or_else(|| "--hotkey requires f11 or shift-alt-win-end".to_string())?;
                hotkey = match value.to_str() {
                    Some("f11") => AcceptanceHotkey::F11,
                    Some("shift-alt-win-end") => AcceptanceHotkey::ShiftAltWinEnd,
                    _ => return Err("--hotkey must be f11 or shift-alt-win-end".into()),
                };
            }
            Some("--source-revision") => {
                let value = args
                    .next()
                    .ok_or_else(|| "--source-revision requires a value".to_string())?;
                source_revision = Some(
                    value
                        .into_string()
                        .map_err(|_| "--source-revision must be valid UTF-8".to_string())?,
                );
            }
            Some("--h6-repeat") => {
                let value = args.next().ok_or_else(|| {
                    "--h6-repeat requires immediate, quiescent, or production-only-diagnostic"
                        .to_string()
                })?;
                h6_repeat_mode = match value.to_str() {
                    Some("immediate") => H6RepeatMode::Immediate,
                    Some("quiescent") => H6RepeatMode::Quiescent,
                    Some("production-only-diagnostic") => H6RepeatMode::ProductionOnlyDiagnostic,
                    _ => {
                        return Err("--h6-repeat must be immediate, quiescent, or production-only-diagnostic".into());
                    }
                };
            }
            Some("--mouse-gestures") => {
                let value = args.next().ok_or_else(|| {
                    "--mouse-gestures requires enabled or disabled-diagnostic".to_string()
                })?;
                mouse_gesture_mode = match value.to_str() {
                    Some("enabled") => MouseGestureMode::Enabled,
                    Some("disabled-diagnostic") => MouseGestureMode::DisabledDiagnostic,
                    _ => {
                        return Err(
                            "--mouse-gestures must be enabled or disabled-diagnostic".into()
                        );
                    }
                };
            }
            Some(option) => return Err(format!("unknown option: {option}")),
            None => return Err("command-line options must be valid UTF-8".to_string()),
        }
    }

    if output.is_some() == report_file.is_some() {
        return Err("specify exactly one of --output <directory> or --report <file>".into());
    }
    if suite == AcceptanceSuite::All && hotkey != AcceptanceHotkey::F11 {
        return Err("--hotkey shift-alt-win-end requires --suite hotkey".into());
    }
    if suite == AcceptanceSuite::Hotkey && profile_copy.is_some() {
        return Err("--suite hotkey cannot be combined with --profile-copy".into());
    }
    let source_revision = match source_revision {
        Some(revision) => Some(validate_source_revision(revision)?),
        None => Some(derive_source_revision()?),
    };
    Ok(ParseResult::Run(Arguments {
        launcher,
        output,
        report_file,
        profile_copy,
        source_revision,
        keep_profile_on_failure,
        h6_repeat_mode,
        mouse_gesture_mode,
        suite,
        hotkey,
    }))
}

fn validate_source_revision(revision: String) -> Result<String, String> {
    if revision.is_empty()
        || revision.len() > 160
        || !revision.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(
            "source revision must be a non-empty printable token of at most 160 bytes".into(),
        );
    }
    Ok(revision)
}

fn derive_source_revision() -> Result<String, String> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let revision = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(repository)
        .output()
        .map_err(|error| format!("derive source revision with git: {error}"))?;
    if !revision.status.success() {
        return Err("derive source revision: git rev-parse failed".into());
    }
    let commit = String::from_utf8(revision.stdout)
        .map_err(|_| "derive source revision: git returned non-UTF-8 commit id".to_string())?;
    let commit = commit.trim();
    if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("derive source revision: git returned a malformed commit id".into());
    }
    let status = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(repository)
        .output()
        .map_err(|error| format!("inspect source worktree state: {error}"))?;
    if !status.status.success() {
        return Err("inspect source worktree state: git status failed".into());
    }
    let suffix = if status.stdout.is_empty() {
        "+clean"
    } else {
        "+dirty"
    };
    Ok(format!("{commit}{suffix}"))
}

fn next_path(args: &mut impl Iterator<Item = OsString>, option: &str) -> Result<PathBuf, String> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("{option} requires a path"))
}

fn run(arguments: Arguments) -> Result<(PathBuf, bool), String> {
    #[cfg(not(windows))]
    {
        let _ = arguments;
        return Err("native radial acceptance is available only on Windows".into());
    }

    #[cfg(windows)]
    run_windows(arguments)
}

#[cfg(windows)]
fn run_windows(arguments: Arguments) -> Result<(PathBuf, bool), String> {
    let proposed_output = proposed_output_directory(&arguments)?;
    let proposed_report = proposed_report_path(&arguments, &proposed_output)?;
    let proposed_copy_report = arguments
        .profile_copy
        .as_ref()
        .map(|_| copied_report_path(&proposed_report));
    let source_scan = arguments
        .profile_copy
        .as_deref()
        .map(copied_profile::ProfileInventory::scan);
    if let Some(source) = arguments.profile_copy.as_deref() {
        let canonical_source = copied_profile::canonical_profile_root(source)?;
        let mut targets = vec![proposed_output.as_path(), proposed_report.as_path()];
        if let Some(copy_report) = proposed_copy_report.as_deref() {
            targets.push(copy_report);
        }
        let private_copy_path = std::env::temp_dir().join(".radial-acceptance-copy-profile");
        targets.push(private_copy_path.as_path());
        copied_profile::ensure_no_overlap(&canonical_source, &targets)?;
    }
    let output = prepare_output_directory(&arguments)?;
    let report_path = if let Some(report) = arguments.report_file.as_deref() {
        output.join(
            report
                .file_name()
                .ok_or_else(|| "--report requires a file name".to_string())?,
        )
    } else {
        output.join("report.json")
    };
    let copied_path = arguments
        .profile_copy
        .as_ref()
        .map(|_| copied_report_path(&report_path));

    let mut deterministic_report = run_windows_deterministic(
        arguments.clone(),
        output.clone(),
        report_path.clone(),
        arguments.profile_copy.is_some(),
    )?;
    let deterministic_native_passed = deterministic_report.passed_native_cases();
    let mut copied_report = match (arguments.profile_copy.as_deref(), copied_path.as_deref()) {
        (Some(_), Some(copy_report_path)) => Some(match source_scan {
            Some(Ok(inventory)) => run_copied_profile_windows(
                &deterministic_report,
                inventory,
                copy_report_path,
                &arguments,
                &output,
            )?,
            Some(Err(error)) => copied_preflight_failure_report(
                &deterministic_report,
                copy_report_path,
                "profile inventory rejected the supplied directory",
                &error,
            )?,
            None => return Err("copied profile inventory was not initialized".into()),
        }),
        _ => None,
    };
    let mut copied_passed = copied_report
        .as_ref()
        .map_or(true, AcceptanceReport::passed_native_cases);
    let mut copied_reports_persisted = true;
    if let (Some(copy_path), Some(copied_report)) = (copied_path.as_deref(), copied_report.as_mut())
    {
        if let Err(error) = persist_copied_report_pair(copy_path, copied_report) {
            copied_reports_persisted = false;
            eprintln!("copied-profile report publication failed: {error}");
        } else {
            println!("Copied profile report: {}", copy_path.display());
        }
        copied_passed = copied_report.passed_native_cases();
    }
    let deterministic_r0_was_valid = deterministic_report
        .cases
        .iter()
        .find(|case| case.id == "R0")
        .is_some_and(|case| matches!(case.status, CaseStatus::Passed));
    if arguments.profile_copy.is_some() {
        deterministic_report.copied_profile_status = if copied_passed && copied_reports_persisted {
            CopiedProfileStatus::Passed
        } else {
            CopiedProfileStatus::Failed
        };
        deterministic_report.copied_profile = copied_report
            .as_ref()
            .and_then(|report| report.copied_profile.clone());
        revalidate_final_r0(&mut deterministic_report, deterministic_r0_was_valid);
    }
    let deterministic_passed = aggregate_native_cases_passed(
        deterministic_native_passed,
        copied_report.as_ref(),
        copied_reports_persisted,
    );
    deterministic_report.outcome = if deterministic_passed {
        "passed"
    } else {
        "failed"
    };
    write_report(&report_path, &mut deterministic_report)?;
    write_text_report(&report_path.with_extension("txt"), &deterministic_report)?;

    Ok((
        report_path,
        deterministic_report.passed() && copied_passed && copied_reports_persisted,
    ))
}

#[cfg(windows)]
fn revalidate_final_r0(report: &mut AcceptanceReport, profile_hashes_previously_valid: bool) {
    let Some(index) = report.cases.iter().position(|case| case.id == "R0") else {
        report.push_case(AcceptanceCaseResult {
            id: "R0".into(),
            status: CaseStatus::Failed,
            elapsed_ms: 0,
            expected: expected_final_case("R0").into(),
            observed: "final aggregate report omitted its R0 record".into(),
            failure_stage: Some(FailureStage::Environment),
            artifacts: Vec::new(),
        });
        return;
    };
    let mut r0 = report.cases.remove(index);
    match validate_r0_report(report, Path::new(""), profile_hashes_previously_valid) {
        Ok(observed) => {
            r0.status = CaseStatus::Passed;
            r0.observed = bounded_text(&observed, MAX_RESULT_BYTES);
            r0.failure_stage = None;
        }
        Err(error) => {
            r0.status = CaseStatus::Failed;
            r0.observed = bounded_text(
                &format!("final aggregate report integrity validation failed: {error}"),
                MAX_RESULT_BYTES,
            );
            r0.failure_stage = Some(FailureStage::Environment);
        }
    }
    report.push_case(r0);
}

fn aggregate_native_cases_passed(
    deterministic_passed: bool,
    copied_report: Option<&AcceptanceReport>,
    copied_reports_persisted: bool,
) -> bool {
    deterministic_passed
        && copied_report.map_or(true, AcceptanceReport::passed_native_cases)
        && (copied_report.is_none() || copied_reports_persisted)
}

fn proposed_output_directory(arguments: &Arguments) -> Result<PathBuf, String> {
    if let Some(output) = arguments.output.as_deref() {
        return copied_profile::resolve_future_path(output);
    }
    let report = arguments
        .report_file
        .as_deref()
        .ok_or_else(|| "specify --output or --report".to_string())?;
    let parent = report
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    parent
        .canonicalize()
        .map_err(|error| format!("resolve report parent before output isolation: {error}"))
}

fn proposed_report_path(arguments: &Arguments, output: &Path) -> Result<PathBuf, String> {
    let report = if let Some(report) = arguments.report_file.as_deref() {
        output.join(
            report
                .file_name()
                .ok_or_else(|| "--report requires a file name".to_string())?,
        )
    } else {
        output.join("report.json")
    };
    copied_profile::resolve_future_path(&report)
}

fn copied_report_path(report_path: &Path) -> PathBuf {
    let stem = report_path
        .file_stem()
        .unwrap_or_else(|| std::ffi::OsStr::new("report"));
    let mut name = stem.to_os_string();
    name.push(".copied-profile.json");
    report_path.with_file_name(name)
}

#[cfg(windows)]
fn persist_copied_report_pair(
    report_path: &Path,
    report: &mut AcceptanceReport,
) -> Result<(), String> {
    let text_path = report_path.with_extension("txt");
    for final_path in [report_path, text_path.as_path()] {
        match fs::symlink_metadata(final_path) {
            Ok(_) => return Err("copied-profile report target already exists".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("inspect copied-profile report target".into()),
        }
    }
    let parent = report_path
        .parent()
        .ok_or_else(|| "copied-profile report has no parent directory".to_string())?;
    let staging = tempfile::Builder::new()
        .prefix(".radial-acceptance-report-stage-")
        .tempdir_in(parent)
        .map_err(|_| "create copied-profile report staging directory".to_string())?;
    let staged_json = staging.path().join("copied.json");
    let staged_text = staging.path().join("copied.txt");
    write_report(&staged_json, report)?;
    write_text_report(&staged_text, report)?;
    if let Err(_) = fs::rename(&staged_json, report_path) {
        return Err("publish verified copied-profile JSON report".into());
    }
    if fs::rename(&staged_text, &text_path).is_err() {
        let _ = fs::remove_file(report_path);
        return Err("publish verified copied-profile text report".into());
    }
    Ok(())
}

fn private_artifact_evidence(summary: &private_artifacts::PrivateArtifactSummary) -> String {
    let disposition = match summary.status {
        private_artifacts::PrivateArtifactStatus::EphemeralValidated => "ephemeral",
        private_artifacts::PrivateArtifactStatus::Retained => "retained",
        private_artifacts::PrivateArtifactStatus::NotRun
        | private_artifacts::PrivateArtifactStatus::Failed => "failed",
    };
    format!(
        "evidence:v1; artifact_retention={disposition}; artifact_count={}; artifact_bytes={}; artifact_id={}; artifact_sha256={}",
        summary.file_count,
        summary.total_bytes,
        summary.artifact_id.as_deref().unwrap_or("none"),
        summary.manifest_sha256.as_deref().unwrap_or("missing")
    )
}

#[cfg(windows)]
fn mark_private_artifact_case_failed(report: &mut AcceptanceReport) {
    if let Some(case) = report.cases.iter_mut().find(|case| case.id == "CP_R1") {
        case.status = CaseStatus::Failed;
        case.failure_stage = Some(FailureStage::Environment);
        case.observed =
            "private copied-profile diagnostics were not included in the public report".into();
    } else {
        append_copied_case(
            report,
            "CP_R1",
            CaseStatus::Failed,
            "private copied-profile diagnostics were not included in the public report",
            Some(FailureStage::Environment),
            Instant::now(),
        );
    }
}

#[cfg(windows)]
fn run_copied_profile_windows(
    deterministic_report: &AcceptanceReport,
    source_inventory: copied_profile::ProfileInventory,
    _report_path: &Path,
    arguments: &Arguments,
    _output: &Path,
) -> Result<AcceptanceReport, String> {
    let run_started = Instant::now();
    let started_unix_ms = unix_ms();
    let mut report = copied_report_seed(deterministic_report, started_unix_ms);
    let private_profile = match tempfile::Builder::new()
        .prefix("multi-launcher-copied-profile-")
        .tempdir()
    {
        Ok(profile) => profile,
        Err(_) => {
            append_copied_case(
                &mut report,
                "CP_PREFLIGHT",
                CaseStatus::Failed,
                "copied-profile private temporary directory could not be created",
                Some(FailureStage::Environment),
                run_started,
            );
            let source_after = copied_profile::ProfileInventory::scan(&source_inventory.root).ok();
            report.cleanup.profile_removed = true;
            finish_copied_profile_report(
                &mut report,
                &source_inventory,
                source_after,
                None,
                false,
                run_started,
            );
            return Ok(report);
        }
    };
    let copy_root = private_profile.path().to_path_buf();
    if copied_profile::ensure_no_overlap(&source_inventory.root, &[copy_root.as_path()]).is_err() {
        append_copied_case(
            &mut report,
            "CP_PREFLIGHT",
            CaseStatus::Failed,
            "isolated temporary copy overlaps the supplied source profile",
            Some(FailureStage::Environment),
            run_started,
        );
        let source_after = copied_profile::ProfileInventory::scan(&source_inventory.root).ok();
        let removed = private_profile.close().is_ok();
        report.cleanup.profile_removed = removed;
        finish_copied_profile_report(
            &mut report,
            &source_inventory,
            source_after,
            None,
            false,
            run_started,
        );
        return Ok(report);
    }

    let metadata = match prepare_copied_profile(&source_inventory, &copy_root) {
        Ok(metadata) => metadata,
        Err(_) => {
            append_copied_case(
                &mut report,
                "CP_PREFLIGHT",
                CaseStatus::Failed,
                "copied-profile typed validation or copy-only safety normalization failed",
                Some(FailureStage::Environment),
                run_started,
            );
            let source_after = copied_profile::ProfileInventory::scan(&source_inventory.root).ok();
            report.cleanup.profile_removed = private_profile.close().is_ok();
            finish_copied_profile_report(
                &mut report,
                &source_inventory,
                source_after,
                None,
                false,
                run_started,
            );
            return Ok(report);
        }
    };
    append_copied_case(
        &mut report,
        "CP_PREFLIGHT",
        CaseStatus::Passed,
        "copied source bytes verified; typed settings and radial data validated; copy-only safety controls applied",
        None,
        run_started,
    );
    report.profile = ProfileIdentity {
        mode: "copied_profile",
        temporary_data_root: bounded_text(&copy_root.to_string_lossy(), MAX_PATH_BYTES),
        settings_sha256: metadata.launch_settings_sha256.clone(),
        radial_sha256: metadata.launch_radial_sha256.clone(),
        actions_sha256: metadata.launch_actions_sha256.clone(),
        configured_hotkey: ACCEPTANCE_HOTKEY,
        hold_threshold_ms: metadata.hold_threshold_ms,
    };
    report.copied_profile = Some(metadata.report_summary(&source_inventory, None, false));
    report.environment.child_process_id = None;
    report.environment.child_started_unix_ms = None;
    report.run_id = format!("{}-copied", deterministic_report.run_id);

    let runner_log_path = copy_root.join("acceptance-runner.log");
    // The copied profile's normalized Settings.log_file is the candidate trace. Keep this
    // aligned with the deterministic runner so native predicates read the child's actual
    // acceptance events instead of an unrelated, never-created filename.
    let trace_path = copied_profile_trace_path(&copy_root);
    let mut runner_log = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&runner_log_path)
    {
        Ok(log) => log,
        Err(_) => {
            append_copied_case(
                &mut report,
                "CP_H0",
                CaseStatus::Failed,
                "copied-profile private runner log could not be created",
                Some(FailureStage::Environment),
                run_started,
            );
            let source_after = copied_profile::ProfileInventory::scan(&source_inventory.root).ok();
            report.cleanup.profile_removed = private_profile.close().is_ok();
            finish_copied_profile_report(
                &mut report,
                &source_inventory,
                source_after,
                Some(&metadata),
                false,
                run_started,
            );
            return Ok(report);
        }
    };
    let _ = writeln!(runner_log, "copied-profile native suite starting");
    let candidate_executable = report.candidate.executable.clone();
    let driver_profile = copy_root.clone();
    let driver_trace = trace_path.clone();
    let copied_options = native::CopiedAuthoringOptions {
        target_action_index: metadata.target_action_index,
        skin_index: metadata.skin_index,
        original_menu_sha256: metadata.original_menu_sha256.clone(),
        restore_menu_name: metadata.restore_menu_name.clone(),
    };
    let fallback_report = report.clone();
    let driver = std::thread::Builder::new()
        .name("radial-acceptance-copied-profile-driver".to_string())
        .spawn(move || match native::attach_to_input_desktop() {
            Ok((observed, desktop_attachment)) => {
                let _ = writeln!(runner_log, "input desktop attached: {observed}");
                let before_foreground = native::capture_foreground();
                let before_cursor = native::cursor_position();
                report.cleanup.foreground_restore_captured = !before_foreground.0.is_invalid();
                let anchor = native::run_copied_profile_suite(
                    &candidate_executable,
                    &driver_profile,
                    &driver_profile,
                    &driver_trace,
                    before_cursor.as_ref().ok().copied(),
                    &desktop_attachment,
                    &mut report,
                    &mut runner_log,
                    copied_options,
                );

                if let Ok(point) = &before_cursor {
                    let point = *point;
                    let already_restored = native::cursor_position().is_ok_and(|current| {
                        current.x.abs_diff(point.x) <= 1 && current.y.abs_diff(point.y) <= 1
                    });
                    if already_restored {
                        report.cleanup.cursor_restored = true;
                    } else if native::set_cursor_position(point).is_ok() {
                        report.cleanup.cursor_restored =
                            native::cursor_position().is_ok_and(|current| {
                                current.x.abs_diff(point.x) <= 1 && current.y.abs_diff(point.y) <= 1
                            });
                    }
                }
                report.cleanup.foreground_restored = if before_foreground.0.is_invalid() {
                    false
                } else {
                    report.cleanup.foreground_restore_attempted = true;
                    if let Some(anchor) = anchor.as_ref() {
                        let _ = anchor.focus();
                    }
                    native::restore_foreground(before_foreground.0, before_foreground.1).is_ok()
                };
                if let Ok(point) = &before_cursor {
                    let point = *point;
                    if !native::cursor_position().is_ok_and(|current| {
                        current.x.abs_diff(point.x) <= 1 && current.y.abs_diff(point.y) <= 1
                    }) {
                        report.cleanup.cursor_restored = native::set_cursor_position(point)
                            .and_then(|()| native::cursor_position())
                            .is_ok_and(|current| {
                                current.x.abs_diff(point.x) <= 1 && current.y.abs_diff(point.y) <= 1
                            });
                    }
                }
                drop(anchor);
                let handle = desktop_attachment.release_for_thread_exit();
                (report, handle)
            }
            Err(_) => {
                append_copied_case(
                    &mut report,
                    "CP_H0",
                    CaseStatus::Failed,
                    "native input desktop could not be attached for copied-profile acceptance",
                    Some(FailureStage::Environment),
                    run_started,
                );
                (report, None)
            }
        });
    let (mut report, input_desktop_handle) = match driver {
        Ok(driver) => match driver.join() {
            Ok(result) => result,
            Err(_) => (fallback_report, None),
        },
        Err(_) => (fallback_report, None),
    };
    let mut runner_log = OpenOptions::new().append(true).open(&runner_log_path).ok();
    if report.environment.child_process_id.is_none() {
        if let Some(runner_log) = runner_log.as_mut() {
            let _ = writeln!(runner_log, "copied-profile native driver did not launch");
        }
    }
    report.cleanup.input_desktop_released = match input_desktop_handle {
        Some(handle) => native::close_input_desktop_after_driver_exit(handle).is_ok(),
        None => true,
    };
    if let Some(runner_log) = runner_log.as_mut() {
        let _ = runner_log.flush();
    }

    let launch_profile_audit = validate_copied_profile_after_run(&copy_root, &metadata);
    let launch_profile_matches = launch_profile_audit.is_ok();
    if let Err(reason) = &launch_profile_audit {
        // Keep the public report path-free and generic, but retain the bounded typed
        // audit reason in the owner-restricted runner log so a copied-profile R0
        // failure can be diagnosed after the temporary profile is removed.
        if let Some(runner_log) = runner_log.as_mut() {
            let _ = writeln!(
                runner_log,
                "copied profile post-run safety audit failed: {reason}"
            );
            let _ = runner_log.flush();
        }
    }
    let source_after = copied_profile::ProfileInventory::scan(&source_inventory.root).ok();
    let source_unchanged = source_after.as_ref() == Some(&source_inventory);
    let failure_requires_retention =
        copied_failure_requires_retention(&report, launch_profile_matches, source_unchanged);
    let mut diagnostic_paths = report
        .cases
        .iter()
        .filter(|case| case.id == "CP_R1" || matches!(case.status, CaseStatus::Failed))
        .flat_map(|case| case.artifacts.iter().map(PathBuf::from))
        .collect::<Vec<_>>();
    let mut failure_logs_complete = true;
    match private_artifacts::write_bounded_run_log_snapshots(&copy_root, &report.run_id) {
        Ok(snapshots) => {
            if report.environment.child_process_id.is_some() {
                let expected = [
                    format!("case-{}-child-acceptance-trace.log", report.run_id),
                    format!("case-{}-child-stdout-private.log", report.run_id),
                    format!("case-{}-child-stderr-private.log", report.run_id),
                ];
                failure_logs_complete = expected.iter().all(|expected_name| {
                    snapshots.iter().any(|path| {
                        path.file_name().and_then(|name| name.to_str())
                            == Some(expected_name.as_str())
                    })
                });
            }
            diagnostic_paths.extend(snapshots);
        }
        Err(_) => failure_logs_complete = false,
    }

    let mut staged_success = None;
    let mut retained_private_directory = None;
    match private_artifacts::stage_diagnostics(&copy_root, diagnostic_paths) {
        Ok(staged) => {
            let control_artifacts_complete = staged.control_artifacts_complete();
            let cp_r1_passed = report
                .cases
                .iter()
                .find(|case| case.id == "CP_R1")
                .is_some_and(|case| matches!(case.status, CaseStatus::Passed));
            let artifact_evidence_complete =
                control_artifacts_complete && failure_logs_complete && cp_r1_passed;
            if !artifact_evidence_complete {
                mark_private_artifact_case_failed(&mut report);
            }

            let retain_evidence = failure_requires_retention
                || report
                    .cases
                    .iter()
                    .find(|case| case.id == "CP_R1")
                    .is_none_or(|case| !matches!(case.status, CaseStatus::Passed));
            if retain_evidence {
                let retained = staged.retain();
                retained_private_directory = Some(retained.directory.clone());
                report.private_artifacts = Some(retained.summary.clone());
                if artifact_evidence_complete {
                    if let Some(case) = report.cases.iter_mut().find(|case| case.id == "CP_R1") {
                        case.observed = private_artifact_evidence(&retained.summary);
                        case.failure_stage = None;
                    }
                }
            } else {
                let summary = staged.ephemeral_summary();
                report.private_artifacts = Some(summary.clone());
                if let Some(case) = report.cases.iter_mut().find(|case| case.id == "CP_R1") {
                    case.observed = private_artifact_evidence(&summary);
                    case.failure_stage = None;
                }
                staged_success = Some(staged);
            }
        }
        Err(_) => {
            report.private_artifacts = Some(private_artifacts::PrivateArtifactSummary::failed());
            mark_private_artifact_case_failed(&mut report);
        }
    }

    let copied_cases_failed = report
        .cases
        .iter()
        .any(|case| !matches!(case.status, CaseStatus::Passed));
    let preserve_private_copy = arguments.keep_profile_on_failure
        && (copied_cases_failed || !launch_profile_matches || !source_unchanged);
    if preserve_private_copy {
        let _retained_profile_path = private_profile.keep();
        report.cleanup.profile_removed = false;
        eprintln!("private copied-profile debugging copy retained after failure");
    } else {
        report.cleanup.profile_removed = private_profile.close().is_ok();
    }

    let artifact_bundle_verified = if let (Some(directory), Some(summary)) = (
        retained_private_directory.as_deref(),
        report.private_artifacts.as_ref(),
    ) {
        !directory.starts_with(&copy_root)
            && private_artifacts::verify_retained_artifacts(directory, summary).is_ok()
    } else if let Some(staged) = staged_success.as_ref() {
        report.cleanup.profile_removed && staged.verify_after_profile_cleanup().is_ok()
    } else {
        false
    };
    if !artifact_bundle_verified {
        if let Some(staged) = staged_success.take() {
            // Preserve evidence if post-cleanup validation fails, while ensuring the
            // copied run cannot report success for an unverified evidence bundle.
            let retained = staged.retain();
            let retained_verified = private_artifacts::verify_retained_artifacts(
                &retained.directory,
                &retained.summary,
            )
            .is_ok();
            report.private_artifacts = Some(retained.summary);
            if !retained_verified {
                mark_private_artifact_case_failed(&mut report);
            }
        }
        mark_private_artifact_case_failed(&mut report);
    } else if let Some(staged) = staged_success.take() {
        // Passing runs expose only a hash and counts; deleting this temporary bundle
        // avoids retaining user-profile diagnostics after validation.
        drop(staged);
    }
    finish_copied_profile_report(
        &mut report,
        &source_inventory,
        source_after,
        Some(&metadata),
        launch_profile_matches,
        run_started,
    );
    Ok(report)
}

#[cfg(windows)]
fn copied_report_seed(deterministic: &AcceptanceReport, started_unix_ms: u128) -> AcceptanceReport {
    let mut environment = deterministic.environment.clone();
    environment.child_process_id = None;
    environment.child_started_unix_ms = None;
    let mut profile = deterministic.profile.clone();
    profile.mode = "copied_profile";
    profile.temporary_data_root = "private copied-profile temporary directory".into();
    AcceptanceReport {
        schema_version: deterministic.schema_version,
        run_id: format!("{}-copied", deterministic.run_id),
        mode: "native_windows_copied_profile",
        started_unix_ms,
        finished_unix_ms: 0,
        copied_profile_status: CopiedProfileStatus::Running,
        copied_profile: None,
        private_artifacts: Some(private_artifacts::PrivateArtifactSummary::not_run()),
        h6_repeat_mode: deterministic.h6_repeat_mode,
        mouse_gesture_mode: deterministic.mouse_gesture_mode,
        suite: AcceptanceSuite::All,
        hotkey: AcceptanceHotkey::F11,
        outcome: "running",
        candidate: deterministic.candidate.clone(),
        environment,
        profile,
        cases: Vec::with_capacity(COPIED_CASE_IDS.len()),
        hotkey_evidence: Vec::new(),
        artifacts: Vec::new(),
        cleanup: CleanupResult::default(),
        capacity_saturated: false,
        report_overflow: None,
    }
}

#[cfg(windows)]
fn copied_preflight_failure_report(
    deterministic: &AcceptanceReport,
    _report_path: &Path,
    _reason: &str,
    _private_error: &str,
) -> Result<AcceptanceReport, String> {
    let run_started = Instant::now();
    let mut report = copied_report_seed(deterministic, unix_ms());
    report.cleanup.profile_removed = true;
    for id in COPIED_CASE_IDS {
        let (status, observed, stage) = match id {
            "CP_PREFLIGHT" => (
                CaseStatus::Failed,
                "supplied profile was rejected by bounded regular-file inventory preflight",
                Some(FailureStage::Environment),
            ),
            "CP_SOURCE_INTEGRITY" => (
                CaseStatus::Failed,
                "source profile integrity could not be established from the rejected inventory",
                Some(FailureStage::Environment),
            ),
            "R0" => (
                CaseStatus::Failed,
                "copied-profile report integrity validation failed",
                Some(FailureStage::Environment),
            ),
            "R2" | "CLEANUP" => (
                CaseStatus::Failed,
                "copied-profile native process and private copy were not started",
                Some(FailureStage::Cleanup),
            ),
            _ => (
                CaseStatus::Failed,
                "copied-profile native case was not run after source inventory rejection",
                Some(FailureStage::Environment),
            ),
        };
        append_copied_case(&mut report, id, status, observed, stage, run_started);
    }
    report.finished_unix_ms = unix_ms();
    report.copied_profile_status = CopiedProfileStatus::Failed;
    report.outcome = "failed";
    sanitize_copied_report(&mut report);
    Ok(report)
}

#[cfg(windows)]
fn append_copied_case(
    report: &mut AcceptanceReport,
    id: &str,
    status: CaseStatus,
    observed: &str,
    failure_stage: Option<FailureStage>,
    started: Instant,
) {
    report.push_case(AcceptanceCaseResult {
        id: id.to_string(),
        status,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        expected: copied_case_expected(id).to_string(),
        observed: bounded_text(observed, MAX_RESULT_BYTES),
        failure_stage,
        artifacts: Vec::new(),
    });
}

#[cfg(windows)]
fn copied_case_expected(id: &str) -> &'static str {
    match id {
        "CP_PREFLIGHT" => {
            "source inventory, initial copy hashes, typed input, and safe copy normalization pass"
        }
        "CP_SOURCE_INTEGRITY" => {
            "source profile remains byte-for-byte unchanged after copied native acceptance"
        }
        "CP_R1" => {
            "bounded diagnostics are privately validated and retained only when copied acceptance fails"
        }
        "R0" => "copied-profile report identities, bounds, hashes, cases, and evidence validate",
        "R2" => "copied candidate and native runner restore system state and remove private copy",
        "CLEANUP" => "copied candidate and all child-owned windows close cleanly",
        _ => "copied profile passes the corresponding native ROOT or Designer acceptance behavior",
    }
}

#[cfg(windows)]
fn finish_copied_profile_report(
    report: &mut AcceptanceReport,
    source_inventory: &copied_profile::ProfileInventory,
    source_after: Option<copied_profile::ProfileInventory>,
    metadata: Option<&CopiedProfileMetadata>,
    launch_profile_matches: bool,
    run_started: Instant,
) {
    for id in COPIED_CASE_IDS {
        if matches!(
            id,
            "CP_PREFLIGHT" | "CP_SOURCE_INTEGRITY" | "R0" | "R2" | "CLEANUP"
        ) || report.cases.iter().any(|case| case.id == id)
        {
            continue;
        }
        append_copied_case(
            report,
            id,
            CaseStatus::Failed,
            "copied-profile native case did not produce a result",
            Some(FailureStage::Cleanup),
            run_started,
        );
    }
    if !report.cases.iter().any(|case| case.id == "CLEANUP") {
        append_copied_case(
            report,
            "CLEANUP",
            CaseStatus::Failed,
            "copied-profile candidate cleanup did not produce a result",
            Some(FailureStage::Cleanup),
            run_started,
        );
    }
    let source_unchanged = source_after
        .as_ref()
        .is_some_and(|after| after == source_inventory);
    append_copied_case(
        report,
        "CP_SOURCE_INTEGRITY",
        if source_unchanged {
            CaseStatus::Passed
        } else {
            CaseStatus::Failed
        },
        if source_unchanged {
            "pre-run and post-run source inventories have identical paths, sizes, and SHA-256 hashes"
        } else {
            "source profile integrity verification failed after copied-profile acceptance"
        },
        (!source_unchanged).then_some(FailureStage::Environment),
        run_started,
    );
    if let Some(metadata) = metadata {
        report.copied_profile = Some(metadata.report_summary(
            source_inventory,
            source_after.as_ref().map(|after| after.tree_sha256.clone()),
            source_unchanged,
        ));
    }
    let cleanup = &report.cleanup;
    let foreground_ok = !cleanup.foreground_restore_captured
        || (cleanup.foreground_restore_attempted && cleanup.foreground_restored);
    let cleanup_ok = report.environment.child_process_id.is_some()
        && cleanup.child_closed_normally
        && cleanup.child_owned_windows_closed
        && cleanup.profile_removed
        && cleanup.cursor_restored
        && cleanup.input_desktop_released
        && foreground_ok;
    append_copied_case(
        report,
        "R2",
        if cleanup_ok {
            CaseStatus::Passed
        } else {
            CaseStatus::Failed
        },
        if cleanup_ok {
            "copied candidate, owned windows, cursor, foreground, input desktop, and private profile cleaned up"
        } else {
            "copied candidate teardown or system-state restoration was not fully verified"
        },
        (!cleanup_ok).then_some(FailureStage::Cleanup),
        run_started,
    );
    report.finished_unix_ms = unix_ms();
    sanitize_copied_report(report);
    report.copied_profile_status = if copied_cases_passed_before_r0(report) {
        CopiedProfileStatus::Passed
    } else {
        CopiedProfileStatus::Failed
    };
    let validation = validate_r0_report(report, Path::new(""), launch_profile_matches);
    match validation {
        Ok(observed) => append_copied_case(
            report,
            "R0",
            CaseStatus::Passed,
            &observed,
            None,
            run_started,
        ),
        Err(_) => append_copied_case(
            report,
            "R0",
            CaseStatus::Failed,
            "copied-profile report integrity validation failed",
            Some(FailureStage::Environment),
            run_started,
        ),
    }
    report.outcome = if report.passed_native_cases() {
        "passed"
    } else {
        "failed"
    };
}

#[cfg(windows)]
fn copied_cases_passed_before_r0(report: &AcceptanceReport) -> bool {
    let required_cases = COPIED_CASE_IDS.len() - 1;
    !report.capacity_saturated
        && report.cases.len() == required_cases
        && report
            .cases
            .iter()
            .all(|case| case.id != "R0" && matches!(case.status, CaseStatus::Passed))
        && COPIED_CASE_IDS
            .iter()
            .filter(|id| **id != "R0")
            .all(|id| report.cases.iter().any(|case| case.id == *id))
}

fn copied_failure_requires_retention(
    report: &AcceptanceReport,
    launch_profile_matches: bool,
    source_unchanged: bool,
) -> bool {
    !launch_profile_matches
        || !source_unchanged
        || report
            .cases
            .iter()
            .any(|case| !matches!(case.status, CaseStatus::Passed))
        || !copied_native_cleanup_verified_before_profile_removal(report)
}

fn copied_native_cleanup_verified_before_profile_removal(report: &AcceptanceReport) -> bool {
    let cleanup = &report.cleanup;
    let foreground_ok = !cleanup.foreground_restore_captured
        || (cleanup.foreground_restore_attempted && cleanup.foreground_restored);
    report.environment.child_process_id.is_some()
        && cleanup.child_closed_normally
        && cleanup.child_owned_windows_closed
        && cleanup.cursor_restored
        && cleanup.input_desktop_released
        && foreground_ok
}

#[cfg(windows)]
fn validate_copied_profile_after_run(
    copy_root: &Path,
    metadata: &CopiedProfileMetadata,
) -> Result<(), String> {
    let settings = match Settings::load_typed(&copy_root.join("settings.json"))
        .map_err(|_| "copied settings could not be reloaded after native acceptance".to_string())?
    {
        LoadState::Loaded(settings) => settings,
        LoadState::Missing | LoadState::Empty => {
            return Err("copied settings were missing after native acceptance".into());
        }
    };
    let expected_log_path = copied_profile_trace_path(copy_root)
        .to_string_lossy()
        .into_owned();
    let expected_clipboard_settings =
        serde_json::to_value(multi_launcher::settings::ClipboardModifyPluginSettings::default())
            .map_err(|_| {
                "default clipboard settings could not be serialized for the safety audit"
                    .to_string()
            })?;
    let settings_checks = [
        (
            "hotkey",
            settings.hotkey.as_deref() == Some(ACCEPTANCE_HOTKEY),
        ),
        ("help_hotkey", settings.help_hotkey.is_none()),
        ("quit_hotkey", settings.quit_hotkey.is_none()),
        ("index_paths", settings.index_paths.is_none()),
        ("plugin_dirs", settings.plugin_dirs.is_none()),
        (
            "enabled_plugins",
            settings.enabled_plugins.as_ref().is_some_and(|plugins| {
                plugins.len() == 1
                    && plugins
                        .iter()
                        .any(|plugin| plugin.eq_ignore_ascii_case("radial"))
            }),
        ),
        (
            "log_file",
            matches!(settings.log_file.as_ref(), Some(LogFile::Path(path)) if path == &expected_log_path),
        ),
        (
            "plugin_settings",
            settings.plugin_settings.len() == 1
                && settings.plugin_settings.get("clipboard_modify")
                    == Some(&expected_clipboard_settings),
        ),
        ("pinned_panels", settings.pinned_panels.is_empty()),
        ("multi_manager.enabled", !settings.multi_manager.enabled),
        (
            "multi_manager.auto_reconnect_on_load",
            !settings.multi_manager.auto_reconnect_on_load,
        ),
        ("multi_manager.auto_save", !settings.multi_manager.auto_save),
        (
            "multi_manager.save_on_exit",
            !settings.multi_manager.save_on_exit,
        ),
        (
            "radial.global_item_inputs",
            !settings.radial.global_item_inputs,
        ),
        (
            "radial_submenu_migration",
            settings.radial_submenu_migration == metadata.expected_submenu_migration_receipt,
        ),
        (
            "multi_manager.workspaces_path",
            Path::new(&settings.multi_manager.workspaces_path).starts_with(copy_root),
        ),
        (
            "multi_manager.bindings_path",
            Path::new(&settings.multi_manager.bindings_path).starts_with(copy_root),
        ),
        (
            "screenshot_dir",
            settings
                .screenshot_dir
                .as_deref()
                .is_some_and(|path| Path::new(path).starts_with(copy_root)),
        ),
    ];
    if let Some((failed_check, _)) = settings_checks.iter().find(|(_, passed)| !passed) {
        return Err(format!(
            "copied settings failed safety check: {failed_check}"
        ));
    }
    if sha256_file(&copy_root.join("actions.json"))
        .map_err(|_| "copied actions could not be hashed after native acceptance".to_string())?
        != metadata.launch_actions_sha256
    {
        return Err("copied inert action catalog changed during native acceptance".into());
    }
    let radial_bytes = fs::read(copy_root.join("radial.json")).map_err(|_| {
        "copied radial document could not be read after native acceptance".to_string()
    })?;
    let radial = multi_launcher::radial::migration::decode_document(&radial_bytes)
        .map_err(|_| "copied radial document failed typed decoding after authoring".to_string())?;
    validate_radial_document(&radial.document)
        .map_err(|_| "copied radial document failed validation after authoring".to_string())?;
    Ok(())
}

fn sanitize_copied_report(report: &mut AcceptanceReport) {
    report.profile.temporary_data_root = "private copied-profile temporary directory".into();
    report.artifacts.clear();
    for case in &mut report.cases {
        case.artifacts.clear();
        case.observed = if matches!(case.status, CaseStatus::Passed) {
            copied_safe_evidence(case)
        } else {
            format!(
                "{}{}",
                case.failure_stage
                    .map(|stage| format!("failed at {stage:?}; "))
                    .unwrap_or_default(),
                "private copied-profile diagnostics were not included in the public report"
            )
        };
    }
}

fn copied_safe_evidence(case: &AcceptanceCaseResult) -> String {
    let id = case.id.as_str();
    let original = case.observed.as_str();
    if id == "CP_D1" {
        let round_trip =
            report_evidence_value(original, "tree_round_trip=").unwrap_or("unavailable");
        return retain_safe_evidence_owned(
            original,
            vec![
                format!("tree_round_trip={round_trip}"),
                "checked_pointer=true".into(),
                "tree_selected=true".into(),
            ],
        );
    }
    let fixed_tokens: Option<&[&str]> = match id {
        "CP_D2" => Some(&[
            "text_edit=restored",
            "tab_focus=menu_combo",
            "unsaved=false",
        ]),
        "CP_A2" => Some(&[
            "geometry=[8,10]",
            "candidate_ids_preserved=true",
            "committed=true",
        ]),
        "CP_A3" => Some(&[
            "blank_cell_selected=true",
            "catalog_rank_gt_50=true",
            "searched_action_assigned=true",
        ]),
        "CP_D3" => Some(&[
            "root_hidden=true",
            "designer_responsive=true",
            "preview_stop=accepted",
            "root_shown=true",
            "hook_pairs=true",
        ]),
        "CP_D6" => Some(&[
            "keep_editing=retained_dirty",
            "draft_glow=true",
            "discard=saved_json_unchanged",
        ]),
        "CP_D7" => Some(&[
            "pending_request=true",
            "cancelled_before_prompt=true",
            "late_reply=rejected",
            "stop=accepted",
            "no_reopen=1s",
            "marker_clean=true",
        ]),
        _ => None,
    };
    if let Some(tokens) = fixed_tokens {
        return retain_safe_evidence(original, tokens);
    }
    match id {
        "CP_PREFLIGHT" => {
            "typed copied profile validated; normalized writes are confined to the isolated copy"
                .into()
        }
        "CP_SOURCE_INTEGRITY" => {
            "source inventory hashes match before and after copied-profile acceptance".into()
        }
        "CP_R1" => {
            let values = [
                report_evidence_value(original, "artifact_retention="),
                report_evidence_value(original, "artifact_count="),
                report_evidence_value(original, "artifact_bytes="),
                report_evidence_value(original, "artifact_id="),
                report_evidence_value(original, "artifact_sha256="),
            ];
            let [
                Some(retention),
                Some(count),
                Some(bytes),
                Some(id),
                Some(hash),
            ] = values
            else {
                return "evidence:v1; required typed facts were not retained".into();
            };
            retain_safe_evidence_owned(
                original,
                vec![
                    format!("artifact_retention={retention}"),
                    format!("artifact_count={count}"),
                    format!("artifact_bytes={bytes}"),
                    format!("artifact_id={id}"),
                    format!("artifact_sha256={hash}"),
                ],
            )
        }
        "R0" => {
            "copied-profile report identities, case bounds, hashes, and typed evidence validated"
                .into()
        }
        "R2" | "CLEANUP" => "copied native child and private profile cleanup verified".into(),
        "CP_A5" => {
            let transition = report_evidence_value(original, "glow=").unwrap_or("unavailable");
            let mut tokens = vec![format!("glow={transition}")];
            tokens.extend([
                "preview_reply=accepted".to_string(),
                "preview_rendered=true".to_string(),
            ]);
            retain_safe_evidence_owned(original, tokens)
        }
        "CP_A6" => {
            let glow = report_evidence_value(original, "glow=").unwrap_or("unavailable");
            let mut tokens = vec![
                "typed_radial=decoded".to_string(),
                "authored_geometry=[8,10]".to_string(),
                "action_binding=true".to_string(),
                "after_action=close_tree".to_string(),
                "original_menus_preserved=true".to_string(),
                format!("glow={glow}"),
                "reopened=true".to_string(),
            ];
            tokens.shrink_to_fit();
            retain_safe_evidence_owned(original, tokens)
        }
        "CP_A7" => {
            let undo = report_evidence_value(original, "undo_restored=").unwrap_or("unavailable");
            let redo = report_evidence_value(original, "redo_restored=").unwrap_or("unavailable");
            retain_safe_evidence_owned(
                original,
                vec![
                    format!("undo_restored={undo}"),
                    format!("redo_restored={redo}"),
                ],
            )
        }
        _ => "copied-profile native behavior passed with bounded typed evidence".into(),
    }
}

fn retain_safe_evidence(original: &str, tokens: &[&str]) -> String {
    retain_safe_evidence_owned(
        original,
        tokens.iter().map(|token| (*token).to_string()).collect(),
    )
}

fn retain_safe_evidence_owned(original: &str, tokens: Vec<String>) -> String {
    if tokens
        .iter()
        .any(|token| !evidence_contains_token(original, token))
    {
        return "evidence:v1; required typed facts were not retained".into();
    }
    format!("evidence:v1; {}", tokens.join("; "))
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(windows)]
fn run_windows_deterministic(
    arguments: Arguments,
    output: PathBuf,
    _report_path: PathBuf,
    copied_profile_requested: bool,
) -> Result<AcceptanceReport, String> {
    let run_started = Instant::now();
    let started_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let candidate = inspect_candidate(arguments.launcher.as_deref())?;
    let runner_path = std::env::current_exe().ok();
    let runner_sha256 = runner_path
        .as_deref()
        .and_then(|path| sha256_file(path).ok());
    let run_id = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let profile = tempfile::Builder::new()
        .prefix("multi-launcher-radial-acceptance-")
        .tempdir()
        .map_err(|error| format!("create isolated profile directory: {error}"))?;
    let profile_path = profile.path().to_path_buf();
    let log_path = profile_path.join("acceptance.log");
    let fixture = if arguments.suite == AcceptanceSuite::Hotkey {
        deterministic_fixture_for_hotkey_with_direct_trigger(
            &log_path,
            arguments.mouse_gesture_mode,
            arguments.hotkey,
        )?
    } else {
        deterministic_fixture_for_hotkey(&log_path, arguments.mouse_gesture_mode, arguments.hotkey)?
    };
    write_new(&profile_path.join("settings.json"), &fixture.settings_json)?;
    write_new(&profile_path.join("radial.json"), &fixture.radial_json)?;
    write_new(&profile_path.join("actions.json"), &fixture.actions_json)?;
    let settings_sha256 = sha256_bytes(&fixture.settings_json);
    let radial_sha256 = sha256_bytes(&fixture.radial_json);
    let actions_sha256 = sha256_bytes(&fixture.actions_json);

    let mut report = AcceptanceReport {
        schema_version: 7,
        run_id,
        mode: "native_windows",
        started_unix_ms,
        finished_unix_ms: 0,
        copied_profile_status: if copied_profile_requested {
            CopiedProfileStatus::Running
        } else {
            CopiedProfileStatus::NotRun
        },
        copied_profile: None,
        private_artifacts: None,
        h6_repeat_mode: arguments.h6_repeat_mode,
        mouse_gesture_mode: arguments.mouse_gesture_mode,
        suite: arguments.suite,
        hotkey: arguments.hotkey,
        outcome: "running",
        candidate,
        environment: EnvironmentIdentity {
            os_version: sysinfo::System::long_os_version()
                .unwrap_or_else(|| "unknown Windows version".to_string()),
            architecture: bounded_text(std::env::consts::ARCH, 64),
            runner_process_id: std::process::id(),
            runner_sha256,
            child_process_id: None,
            child_started_unix_ms: None,
            source_revision: arguments.source_revision,
            monitors: monitor_inventory(),
        },
        profile: ProfileIdentity {
            mode: "deterministic_fixture",
            temporary_data_root: bounded_text(&profile_path.to_string_lossy(), MAX_PATH_BYTES),
            settings_sha256,
            radial_sha256,
            actions_sha256,
            configured_hotkey: arguments.hotkey.as_str(),
            hold_threshold_ms: fixture.hold_threshold_ms,
        },
        cases: Vec::with_capacity(10),
        hotkey_evidence: Vec::new(),
        artifacts: Vec::new(),
        cleanup: CleanupResult::default(),
        capacity_saturated: false,
        report_overflow: None,
    };
    let profile_hashes_match_at_launch =
        validate_profile_hashes(&profile_path, &report.profile).is_ok();

    let runner_log = output.join("runner.log");
    let mut log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&runner_log)
        .map_err(|error| format!("create runner log: {error}"))?;
    let _ = writeln!(log, "H6 repeat mode: {}", arguments.h6_repeat_mode.as_str());
    let _ = writeln!(log, "suite: {}", arguments.suite.as_str());
    let _ = writeln!(log, "hotkey: {}", arguments.hotkey.as_str());
    let _ = writeln!(
        log,
        "mouse gesture mode: {}",
        arguments.mouse_gesture_mode.as_str()
    );
    report.push_artifact(runner_log.to_string_lossy());
    let candidate_executable = report.candidate.executable.clone();
    let driver_profile = profile_path.clone();
    let driver_output = output.clone();
    let driver_trace = log_path.clone();
    let hold_threshold_ms = fixture.hold_threshold_ms;
    let (mut report, mut log, input_desktop_handle) = std::thread::Builder::new()
        .name("radial-acceptance-native-driver".to_string())
        .spawn(move || {
            // The terminal's main thread can already be bound to a private desktop. Windows
            // only permits SetThreadDesktop before a thread creates windows or installs hooks,
            // so all native work starts on this fresh thread and attaches before UIA or HWND use.
            match native::attach_to_input_desktop() {
                Ok((observed, desktop_attachment)) => {
                    let _ = writeln!(log, "input desktop attached: {observed}");
            let before_foreground = native::capture_foreground();
            let before_cursor = native::cursor_position();
            report.cleanup.foreground_restore_captured = !before_foreground.0.is_invalid();
                    let focus_anchor = match arguments.suite {
                        AcceptanceSuite::All => native::run_suite(
                            &candidate_executable,
                            &driver_profile,
                            &driver_output,
                            &driver_trace,
                            hold_threshold_ms,
                            arguments.h6_repeat_mode,
                            before_cursor.as_ref().ok().copied(),
                            &desktop_attachment,
                            &mut report,
                            &mut log,
                        ),
                        AcceptanceSuite::Hotkey => native::run_hotkey_suite(
                            &candidate_executable,
                            &driver_profile,
                            &driver_output,
                            &driver_trace,
                            arguments.hotkey,
                            before_cursor.as_ref().ok().copied(),
                            &desktop_attachment,
                            &mut report,
                            &mut log,
                        ),
                    };

                    match &before_cursor {
                        Ok(point) => {
                            let point = *point;
                            let already_restored = native::cursor_position().is_ok_and(|current| {
                                current.x == point.x && current.y == point.y
                            });
                            if already_restored {
                                report.cleanup.cursor_restored = true;
                            } else {
                                match native::set_cursor_position(point) {
                                    Ok(()) => {
                                        report.cleanup.cursor_restored = true;
                                        let _ = writeln!(
                                            log,
                                            "cursor restoration before foreground reached ({},{})",
                                            point.x,
                                            point.y
                                        );
                                    }
                                    Err(error) => {
                                        let _ = writeln!(log, "cursor restoration failed: {error}");
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            let _ = writeln!(
                                log,
                                "cursor position capture failed; restoration is unverified: {error}"
                            );
                        }
                    }
                    report.cleanup.foreground_restored = if before_foreground.0.is_invalid() {
                        let _ = writeln!(log, "foreground restoration skipped: no foreground HWND was captured");
                        false
                    } else {
                        report.cleanup.foreground_restore_attempted = true;
                        if let Some(anchor) = focus_anchor.as_ref() {
                            if let Err(error) = anchor.focus() {
                                let _ = writeln!(log, "could not focus retained runner anchor before foreground restore: {error}");
                            }
                        }
                        match native::restore_foreground(before_foreground.0, before_foreground.1) {
                            Ok(()) => {
                                let _ = writeln!(
                                    log,
                                    "foreground restored to captured HWND={} PID={}",
                                    before_foreground.0.0 as usize,
                                    before_foreground.1
                                );
                                true
                            }
                            Err(error) => {
                                let _ = writeln!(
                                    log,
                                    "foreground restoration failed for captured HWND={} PID={}: {error}",
                                    before_foreground.0.0 as usize,
                                    before_foreground.1
                                );
                                false
                            }
                        }
                    };
                    if let Ok(point) = &before_cursor {
                        let point = *point;
                        let restored_after_foreground = native::cursor_position().is_ok_and(|current| {
                            current.x.abs_diff(point.x) <= 1 && current.y.abs_diff(point.y) <= 1
                        });
                        report.cleanup.cursor_restored = restored_after_foreground;
                        if !restored_after_foreground {
                            let observed = native::cursor_position()
                                .map(|current| format!("({}, {})", current.x, current.y))
                                .unwrap_or_else(|error| format!("unavailable: {error}"));
                            let _ = writeln!(
                                log,
                                "cursor changed during foreground restoration: expected=({},{}), observed={observed}",
                                point.x,
                                point.y
                            );
                            report.cleanup.cursor_restored = match native::set_cursor_position(point) {
                                Ok(()) => native::cursor_position().is_ok_and(|current| {
                                    current.x.abs_diff(point.x) <= 1
                                        && current.y.abs_diff(point.y) <= 1
                                }),
                                Err(error) => {
                                    let _ = writeln!(log, "post-foreground cursor restoration failed: {error}");
                                    false
                                }
                            };
                        }
                    }
                    drop(focus_anchor);
                    let handle = desktop_attachment.release_for_thread_exit();
                    let _ = writeln!(
                        log,
                        "native driver thread leaving input desktop; desktop handle deferred until thread exit"
                    );
                    (report, log, handle)
                }
                Err(error) => {
                    let _ = writeln!(log, "input desktop attachment failed: {error}");
                    native::record_environment_failure(
                        error,
                        &mut report,
                        &driver_output,
                        &driver_trace,
                        &mut log,
                    );
                    (report, log, None)
                }
            }
        })
        .map_err(|error| format!("start native driver thread: {error}"))?
        .join()
        .map_err(|_| "native driver thread panicked".to_string())?;

    report.cleanup.input_desktop_released = match input_desktop_handle {
        Some(handle) => match native::close_input_desktop_after_driver_exit(handle) {
            Ok(()) => true,
            Err(error) => {
                let _ = writeln!(log, "input desktop release failed: {error}");
                false
            }
        },
        None => true,
    };

    let failed = report
        .cases
        .iter()
        .any(|case| !matches!(case.status, CaseStatus::Passed));
    if failed && arguments.keep_profile_on_failure {
        let retained = profile.keep();
        report.profile.temporary_data_root =
            bounded_text(&retained.to_string_lossy(), MAX_PATH_BYTES);
        report.cleanup.profile_removed = false;
        let _ = writeln!(log, "failure profile retained at {}", retained.display());
    } else {
        let cleanup = profile.close();
        report.cleanup.profile_removed = cleanup.is_ok();
        if let Err(error) = cleanup {
            report.push_artifact(format!("profile cleanup failed: {error}"));
        }
    }

    report.finished_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    ensure_final_acceptance_cases(
        &mut report,
        &profile_path,
        profile_hashes_match_at_launch,
        run_started,
    );
    report.outcome = if report.passed_native_cases() {
        "passed"
    } else {
        "failed"
    };
    Ok(report)
}

#[cfg(windows)]
fn validate_profile_hashes(profile_path: &Path, identity: &ProfileIdentity) -> Result<(), String> {
    for (name, file, expected) in [
        ("settings", "settings.json", &identity.settings_sha256),
        ("radial document", "radial.json", &identity.radial_sha256),
        ("actions", "actions.json", &identity.actions_sha256),
    ] {
        let actual = sha256_file(&profile_path.join(file))
            .map_err(|error| format!("hash deterministic {name} fixture: {error}"))?;
        if &actual != expected {
            return Err(format!(
                "deterministic {name} profile hash changed during acceptance"
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn ensure_final_acceptance_cases(
    report: &mut AcceptanceReport,
    profile_path: &Path,
    profile_hashes_match: bool,
    run_started: Instant,
) {
    if report.suite == AcceptanceSuite::All {
        if !report.cases.iter().any(|case| case.id == "R1") {
            push_final_case(
                report,
                if report.mode == "native_windows_copied_profile" {
                    "CP_R1"
                } else {
                    "R1"
                },
                CaseStatus::Failed,
                "controlled artifact capture was not run because the native child was unavailable",
                Some(FailureStage::CandidateStartup),
                Vec::new(),
                run_started,
            );
        }

        let cleanup = &report.cleanup;
        let foreground_ok = !cleanup.foreground_restore_captured
            || (cleanup.foreground_restore_attempted && cleanup.foreground_restored);
        let r2_passed = report.environment.child_process_id.is_some()
            && cleanup.child_closed_normally
            && cleanup.child_owned_windows_closed
            && cleanup.profile_removed
            && cleanup.cursor_restored
            && cleanup.input_desktop_released
            && foreground_ok;
        let foreground_observed = if !cleanup.foreground_restore_captured {
            "no foreground HWND was present to restore"
        } else if cleanup.foreground_restored {
            "captured foreground HWND/PID restored"
        } else {
            "captured foreground HWND/PID restoration was not verified"
        };
        push_final_case(
            report,
            "R2",
            if r2_passed {
                CaseStatus::Passed
            } else {
                CaseStatus::Failed
            },
            &format!(
                "child_closed_normally={}, child_owned_windows_closed={}, temp_profile_removed={}, cursor_restored={}, input_desktop_released={}, foreground_restore_captured={}, foreground_restore_attempted={}, foreground_restored={} ({foreground_observed})",
                cleanup.child_closed_normally,
                cleanup.child_owned_windows_closed,
                cleanup.profile_removed,
                cleanup.cursor_restored,
                cleanup.input_desktop_released,
                cleanup.foreground_restore_captured,
                cleanup.foreground_restore_attempted,
                cleanup.foreground_restored,
            ),
            (!r2_passed).then_some(FailureStage::Cleanup),
            Vec::new(),
            run_started,
        );
    }

    let validation = validate_r0_report(report, profile_path, profile_hashes_match);
    let (status, observed, failure_stage) = match validation {
        Ok(observed) => (CaseStatus::Passed, observed, None),
        Err(error) => (
            CaseStatus::Failed,
            format!("report integrity validation failed: {error}"),
            Some(FailureStage::Environment),
        ),
    };
    push_final_case(
        report,
        "R0",
        status,
        &observed,
        failure_stage,
        Vec::new(),
        run_started,
    );
}

#[cfg(windows)]
fn validate_r0_report(
    report: &AcceptanceReport,
    _profile_path: &Path,
    profile_hashes_match: bool,
) -> Result<String, String> {
    if report.capacity_saturated {
        return Err("bounded report capacity was exceeded".into());
    }
    if !profile_hashes_match {
        return Err(
            "on-disk deterministic profile hashes did not match the recorded fixture".into(),
        );
    }
    let is_copied_profile = report.mode == "native_windows_copied_profile";
    if !copied_status_contract_is_valid(report) {
        return Err("copied-profile status does not match the active report mode".into());
    }
    if report.profile.configured_hotkey != report.hotkey.as_str()
        || (report.suite == AcceptanceSuite::All && report.hotkey != AcceptanceHotkey::F11)
        || (is_copied_profile
            && (report.suite != AcceptanceSuite::All || report.hotkey != AcceptanceHotkey::F11))
    {
        return Err("suite, configured profile hotkey, and report hotkey disagree".into());
    }
    let revision = report
        .environment
        .source_revision
        .as_deref()
        .filter(|revision| !revision.trim().is_empty())
        .ok_or_else(|| "source revision identity was not supplied".to_string())?;
    if revision.len() > 160 || !revision.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err("source revision identity is not a bounded printable token".into());
    }
    for (label, hash) in [
        ("candidate", report.candidate.sha256.as_str()),
        (
            "runner",
            report
                .environment
                .runner_sha256
                .as_deref()
                .ok_or_else(|| "runner executable hash is unavailable".to_string())?,
        ),
        ("settings profile", report.profile.settings_sha256.as_str()),
        ("radial profile", report.profile.radial_sha256.as_str()),
        ("actions profile", report.profile.actions_sha256.as_str()),
    ] {
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("{label} SHA-256 identity is malformed"));
        }
    }
    let candidate_path = Path::new(&report.candidate.executable);
    let candidate_actual = sha256_file(candidate_path)
        .map_err(|error| format!("rehash source-matched candidate executable: {error}"))?;
    if candidate_actual != report.candidate.sha256 {
        return Err("candidate executable hash changed during acceptance".into());
    }
    let runner_path = std::env::current_exe()
        .map_err(|error| format!("resolve source-matched runner executable: {error}"))?;
    let runner_actual = sha256_file(&runner_path)
        .map_err(|error| format!("rehash source-matched runner executable: {error}"))?;
    if report.environment.runner_sha256.as_deref() != Some(runner_actual.as_str()) {
        return Err("runner executable hash changed during acceptance".into());
    }
    let case_ids: &[&str] = if is_copied_profile {
        &COPIED_CASE_IDS
    } else if report.suite == AcceptanceSuite::Hotkey {
        &HOTKEY_CASE_IDS
    } else {
        &CASE_IDS
    };
    if report.started_unix_ms == 0
        || report.finished_unix_ms < report.started_unix_ms
        || report.cases.len() + 1 != case_ids.len()
    {
        return Err("report elapsed time or pre-final case count is invalid".into());
    }
    for (index, case) in report.cases.iter().enumerate() {
        if !case_ids.contains(&case.id.as_str())
            || report.cases[..index]
                .iter()
                .any(|previous| previous.id == case.id)
            || case.expected.is_empty()
            || case.observed.is_empty()
            || case.expected.len() > MAX_RESULT_BYTES
            || case.observed.len() > MAX_RESULT_BYTES
            || (matches!(case.status, CaseStatus::Failed) != case.failure_stage.is_some())
            || case
                .artifacts
                .iter()
                .any(|path| path.is_empty() || path.len() > MAX_PATH_BYTES)
        {
            return Err(format!(
                "case {} is missing typed stage/evidence or has invalid bounds",
                case.id
            ));
        }
        validate_required_case_evidence(&case.id, case.status, &case.observed)?;
        validate_case_hotkey_profile_relation(report, case)?;
    }
    validate_hotkey_evidence_report(report)?;
    if is_copied_profile {
        validate_private_artifact_report(report)?;
        validate_copied_style_evidence_relations(report)?;
    }
    if !case_ids
        .iter()
        .filter(|id| **id != "R0")
        .all(|id| report.cases.iter().any(|case| case.id == *id))
    {
        return Err("required native case identifiers are missing before R0".into());
    }
    Ok(format!(
        "source revision and candidate/runner hashes verified; {} profile hashes validated; copied_profile_status={}; {} bounded typed case records have unique IDs and evidence fields; elapsed_ms={}",
        if is_copied_profile {
            "copied launch profile"
        } else {
            "deterministic fixture"
        },
        match report.copied_profile_status {
            CopiedProfileStatus::NotRun => "not_run",
            CopiedProfileStatus::Running => "running",
            CopiedProfileStatus::Passed => "passed",
            CopiedProfileStatus::Failed => "failed",
        },
        report.cases.len(),
        report.finished_unix_ms - report.started_unix_ms
    ))
}

#[cfg(windows)]
fn copied_status_contract_is_valid(report: &AcceptanceReport) -> bool {
    let copied_summary_is_valid = report.copied_profile.as_ref().map_or(true, |summary| {
        validate_copied_profile_summary(report, summary).is_ok()
    });
    if !copied_summary_is_valid {
        return false;
    }
    if report.mode == "native_windows_copied_profile" {
        match report.copied_profile_status {
            CopiedProfileStatus::Passed => {
                report
                    .copied_profile
                    .as_ref()
                    .is_some_and(|summary| summary.source_unchanged)
                    && copied_cases_passed_before_r0(report)
            }
            CopiedProfileStatus::Failed => !copied_cases_passed_before_r0(report),
            CopiedProfileStatus::NotRun | CopiedProfileStatus::Running => false,
        }
    } else if report.mode == "native_windows" {
        match report.copied_profile_status {
            CopiedProfileStatus::NotRun | CopiedProfileStatus::Running => {
                report.copied_profile.is_none()
            }
            CopiedProfileStatus::Passed => report
                .copied_profile
                .as_ref()
                .is_some_and(|summary| summary.source_unchanged),
            CopiedProfileStatus::Failed => true,
        }
    } else {
        false
    }
}

#[cfg(windows)]
fn validate_copied_profile_summary(
    report: &AcceptanceReport,
    summary: &CopiedProfileSummary,
) -> Result<(), String> {
    let hashes = [
        &summary.source_tree_sha256_before,
        &summary.copied_initial_tree_sha256,
        &summary.source_settings_sha256,
        &summary.source_radial_sha256,
        &summary.copied_initial_settings_sha256,
        &summary.copied_initial_radial_sha256,
        &summary.launch_settings_sha256,
        &summary.launch_radial_sha256,
        &summary.launch_actions_sha256,
    ];
    if hashes.iter().any(|hash| !is_sha256(hash))
        || summary
            .source_actions_sha256
            .as_deref()
            .is_some_and(|hash| !is_sha256(hash))
        || summary
            .copied_initial_actions_sha256
            .as_deref()
            .is_some_and(|hash| !is_sha256(hash))
        || summary
            .source_tree_sha256_after
            .as_ref()
            .is_some_and(|hash| !is_sha256(hash))
    {
        return Err("copied-profile summary contains a malformed SHA-256 identity".into());
    }
    if summary.source_tree_sha256_before != summary.copied_initial_tree_sha256
        || summary.source_settings_sha256 != summary.copied_initial_settings_sha256
        || summary.source_radial_sha256 != summary.copied_initial_radial_sha256
        || summary.source_actions_sha256 != summary.copied_initial_actions_sha256
        || (report.mode == "native_windows_copied_profile"
            && (summary.launch_settings_sha256 != report.profile.settings_sha256
                || summary.launch_radial_sha256 != report.profile.radial_sha256
                || summary.launch_actions_sha256 != report.profile.actions_sha256))
        || !(2..=copied_profile::MAX_PROFILE_FILES).contains(&summary.copied_file_count)
        || summary.copied_total_bytes == 0
        || summary.copied_total_bytes > copied_profile::MAX_PROFILE_TOTAL_BYTES
    {
        return Err("copied-profile summary identities, counts, or launch hashes disagree".into());
    }
    let after_matches = summary
        .source_tree_sha256_after
        .as_ref()
        .is_some_and(|after| after == &summary.source_tree_sha256_before);
    if summary.source_unchanged != after_matches {
        return Err("copied-profile source-integrity flag disagrees with its post-run hash".into());
    }
    Ok(())
}

#[cfg(windows)]
fn validate_private_artifact_report(report: &AcceptanceReport) -> Result<(), String> {
    let summary = report.private_artifacts.as_ref().ok_or_else(|| {
        "copied-profile private diagnostic artifact summary is missing".to_string()
    })?;
    let expected_retention = match summary.status {
        private_artifacts::PrivateArtifactStatus::Retained
            if summary
                .artifact_id
                .as_deref()
                .is_some_and(private_artifacts::is_opaque_id) =>
        {
            "retained"
        }
        private_artifacts::PrivateArtifactStatus::EphemeralValidated
            if summary.artifact_id.is_none() =>
        {
            "ephemeral"
        }
        _ => return Err("copied-profile private diagnostic disposition is invalid".into()),
    };
    if !(4..=private_artifacts::MAX_PRIVATE_ARTIFACT_FILES).contains(&summary.file_count)
        || summary.total_bytes == 0
        || summary.total_bytes > private_artifacts::MAX_PRIVATE_ARTIFACT_BYTES
        || summary
            .manifest_sha256
            .as_deref()
            .is_none_or(|hash| !is_sha256(hash))
    {
        return Err(
            "copied-profile private diagnostic summary is incomplete or out of bounds".into(),
        );
    }
    let case = report
        .cases
        .iter()
        .find(|case| case.id == "CP_R1")
        .ok_or_else(|| "copied-profile CP_R1 artifact case is missing".to_string())?;
    let observed = case.observed.as_str();
    if !matches!(case.status, CaseStatus::Passed)
        || report_evidence_value(observed, "artifact_retention=") != Some(expected_retention)
        || report_evidence_value(observed, "artifact_count=")
            .and_then(|value| value.parse::<usize>().ok())
            != Some(summary.file_count)
        || report_evidence_value(observed, "artifact_bytes=")
            .and_then(|value| value.parse::<u64>().ok())
            != Some(summary.total_bytes)
        || report_evidence_value(observed, "artifact_id=")
            != Some(summary.artifact_id.as_deref().unwrap_or("none"))
        || report_evidence_value(observed, "artifact_sha256=") != summary.manifest_sha256.as_deref()
    {
        return Err("copied-profile CP_R1 evidence disagrees with private artifact summary".into());
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_required_case_evidence(
    id: &str,
    status: CaseStatus,
    observed: &str,
) -> Result<(), String> {
    if id == "H04" {
        h04_retry_ledger(status, observed)?;
        if matches!(status, CaseStatus::Failed) {
            return Ok(());
        }
        if !matches!(status, CaseStatus::Passed)
            || !observed.starts_with("evidence:v1;")
            || observed.len() >= MAX_RESULT_BYTES
            || !matches!(
                report_evidence_value(observed, "hotkey="),
                Some("F11" | "Shift+Alt+Win+End")
            )
            || !evidence_contains_token(observed, "matrix=hidden+visible:1,2,5,10,25")
            || report_evidence_value(observed, "matrix_groups=") != Some("10")
            || report_evidence_value(observed, "decisions=")
                .and_then(|value| value.parse::<usize>().ok())
                != Some(86)
            || report_evidence_value(observed, "unique_invocation_ids=")
                .and_then(|value| value.parse::<usize>().ok())
                != Some(86)
            || report_evidence_value(observed, "invocation_ids_sha256=")
                .is_none_or(|value| !is_sha256(value))
            || report_evidence_value(observed, "readable_cadence=")
                != Some("3_individual_short_taps")
            || report_evidence_value(observed, "readable_decisions=") != Some("3")
            || report_evidence_value(observed, "readable_target=")
                != Some("owned_foreground_each_tap")
            || report_evidence_value(observed, "focused_root_hide=") != Some("true")
            || report_evidence_value(observed, "readable_refocus=") != Some("none")
            || report_evidence_value(observed, "down_ms=") != Some("25")
            || report_evidence_value(observed, "released_ms=") != Some("75")
            || report_evidence_value(observed, "inter_tap_ui_poll=") != Some("0")
            || report_evidence_value(observed, "inter_tap_refocus=") != Some("0")
            || report_evidence_value(observed, "inter_tap_preflight=") != Some("0")
            || report_evidence_value(observed, "pointer_movement=") != Some("0")
            || !evidence_contains_token(observed, "input_desktop=thread=Default,active=Default")
            || report_evidence_value(observed, "preflight=") != Some("once_per_burst")
            || report_evidence_value(observed, "uninterrupted=") != Some("true")
            || report_evidence_value(observed, "per_gesture_correlation=")
                != Some("release_short_tap_visibility")
            || report_evidence_value(observed, "no_hold_promotion=") != Some("true")
            || report_evidence_value(observed, "key_cleanup=") != Some("verified")
        {
            return Err(
                "H04 summary omitted the bounded matrix, decision, cadence, or cleanup facts"
                    .into(),
            );
        }
        return Ok(());
    }
    if id == "H7" || id == "H8" {
        let (taps, parity, final_visible) = if id == "H7" {
            (3, "odd", "false")
        } else {
            (4, "even", "true")
        };
        let expected_hotkey = report_evidence_value(observed, "hotkey=");
        let expected_sendinput =
            expected_hotkey.map(|hotkey| if hotkey == "F11" { taps } else { taps * 4 });
        let expected_observer_keys = if expected_hotkey == Some("F11") {
            "F11"
        } else {
            "LeftShift+LeftAlt+LeftWin+End"
        };
        let expected_observer_order = if expected_hotkey == Some("F11") {
            "alternating_down_up"
        } else {
            "down_then_reverse_up"
        };
        if !matches!(status, CaseStatus::Passed)
            || !observed.starts_with("evidence:v1;")
            || !matches!(
                report_evidence_value(observed, "hotkey="),
                Some("F11" | "Shift+Alt+Win+End")
            )
            || report_evidence_value(observed, "burst=")
                .and_then(|value| value.parse::<usize>().ok())
                != Some(taps)
            || report_evidence_value(observed, "parity=") != Some(parity)
            || report_evidence_value(observed, "initial_visible=") != Some("true")
            || report_evidence_value(observed, "final_visible=") != Some(final_visible)
            || report_evidence_value(observed, "hook_pairs=")
                .and_then(|value| value.parse::<usize>().ok())
                != Some(taps)
            || report_evidence_value(observed, "configured_pairs=")
                .and_then(|value| value.parse::<usize>().ok())
                != Some(taps)
            || report_evidence_value(observed, "short_taps=")
                .and_then(|value| value.parse::<usize>().ok())
                != Some(taps)
            || report_evidence_value(observed, "visibility_edges=")
                .and_then(|value| value.parse::<usize>().ok())
                != Some(taps)
            || report_evidence_value(observed, "uninterrupted=") != Some("true")
            || report_evidence_value(observed, "inter_tap_ui_poll=") != Some("0")
            || report_evidence_value(observed, "inter_tap_refocus=") != Some("0")
            || report_evidence_value(observed, "preflight=") != Some("registered_unregistered")
            || report_evidence_value(observed, "runner_observer=") != Some("exact_injected_pairs")
            || report_evidence_value(observed, "observer_order=") != Some(expected_observer_order)
            || report_evidence_value(observed, "observer_keys=") != Some(expected_observer_keys)
            || report_evidence_value(observed, "sendinput_down=")
                .and_then(|value| value.parse::<usize>().ok())
                != expected_sendinput
            || report_evidence_value(observed, "sendinput_up=")
                .and_then(|value| value.parse::<usize>().ok())
                != expected_sendinput
            || report_evidence_value(observed, "invocation_count=")
                .and_then(|value| value.parse::<usize>().ok())
                != Some(taps)
            || report_evidence_value(observed, "setup_tap=").is_none()
            || report_evidence_value(observed, "input_desktop=") != Some("Default")
        {
            return Err(format!(
                "case {id} omitted valid odd/even burst, exact observer, native release, or visibility evidence"
            ));
        }
        return Ok(());
    }
    if id == "CP_R1" {
        let retention = report_evidence_value(observed, "artifact_retention=");
        let artifact_id = report_evidence_value(observed, "artifact_id=");
        if !matches!(status, CaseStatus::Passed)
            || !observed.starts_with("evidence:v1;")
            || !matches!(retention, Some("ephemeral" | "retained"))
            || report_evidence_value(observed, "artifact_count=")
                .and_then(|value| value.parse::<usize>().ok())
                .is_none_or(|count| {
                    !(4..=private_artifacts::MAX_PRIVATE_ARTIFACT_FILES).contains(&count)
                })
            || report_evidence_value(observed, "artifact_bytes=")
                .and_then(|value| value.parse::<u64>().ok())
                .is_none_or(|bytes| {
                    bytes == 0 || bytes > private_artifacts::MAX_PRIVATE_ARTIFACT_BYTES
                })
            || match retention {
                Some("retained") => {
                    artifact_id.is_none_or(|id| !private_artifacts::is_opaque_id(id))
                }
                Some("ephemeral") => artifact_id != Some("none"),
                _ => true,
            }
            || report_evidence_value(observed, "artifact_sha256=")
                .is_none_or(|hash| !is_sha256(hash))
        {
            return Err("copied case CP_R1 omitted bounded private artifact evidence".into());
        }
        return Ok(());
    }
    if id == "CP_D1" {
        if !matches!(status, CaseStatus::Passed)
            || !observed.starts_with("evidence:v1;")
            || report_evidence_value(observed, "tree_round_trip=")
                .is_none_or(|value| !["true", "false"].contains(&value))
            || !evidence_contains_token(observed, "checked_pointer=true")
            || !evidence_contains_token(observed, "tree_selected=true")
        {
            return Err("copied case CP_D1 omitted checked Tree pointer evidence".into());
        }
        return Ok(());
    }
    if id == "CP_A5" {
        validate_copied_style_evidence(
            id,
            status,
            observed,
            &["preview_reply=accepted", "preview_rendered=true"],
        )?;
        if !matches!(
            report_evidence_value(observed, "glow="),
            Some("true->false" | "false->true")
        ) {
            return Err("copied case CP_A5 omitted its reversible style transition".into());
        }
        return Ok(());
    }
    if id == "CP_A6" {
        validate_copied_style_evidence(
            id,
            status,
            observed,
            &[
                "typed_radial=decoded",
                "authored_geometry=[8,10]",
                "action_binding=true",
                "after_action=close_tree",
                "original_menus_preserved=true",
                "reopened=true",
            ],
        )?;
        if !matches!(
            report_evidence_value(observed, "glow="),
            Some("true" | "false")
        ) {
            return Err("copied case CP_A6 omitted its persisted style value".into());
        }
        return Ok(());
    }
    if id == "CP_A7" {
        if !matches!(status, CaseStatus::Passed)
            || !observed.starts_with("evidence:v1;")
            || !observed.contains("undo_restored=")
            || !observed.contains("redo_restored=")
        {
            return Err("copied case CP_A7 omitted its saved-style Undo/Redo evidence".into());
        }
        let saved = report_evidence_value(observed, "undo_restored=")
            .ok_or_else(|| "copied case CP_A7 omitted the saved style value".to_string())?;
        let redone = report_evidence_value(observed, "redo_restored=")
            .ok_or_else(|| "copied case CP_A7 omitted the edited style value".to_string())?;
        if !["true", "false"].contains(&saved)
            || !["true", "false"].contains(&redone)
            || saved == redone
        {
            return Err(
                "copied case CP_A7 did not distinguish saved and redone style values".into(),
            );
        }
        return Ok(());
    }
    let evidence_id = id.strip_prefix("CP_").unwrap_or(id);
    let Some(required) = required_case_evidence(evidence_id) else {
        return Ok(());
    };
    if !matches!(status, CaseStatus::Passed) {
        return Err(format!("case {id} is not passed"));
    }
    if observed.len() >= MAX_RESULT_BYTES || !observed.starts_with("evidence:v1;") {
        return Err(format!(
            "case {id} report summary is missing its versioned, untruncated evidence prefix"
        ));
    }
    if let Some(missing) = required.iter().find(|fact| !observed.contains(**fact)) {
        return Err(format!(
            "case {id} report summary omitted required fact {missing}"
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HotkeyBurstSummary {
    invocation_ids: Vec<u64>,
    final_visible: bool,
}

#[cfg(test)]
fn validate_hotkey_burst_trace(
    events: &[String],
    taps: usize,
    initial_visible: bool,
) -> Result<HotkeyBurstSummary, String> {
    validate_hotkey_burst_trace_with_baseline(events, taps, initial_visible, None, None)
}

fn validate_hotkey_burst_trace_with_baseline(
    events: &[String],
    taps: usize,
    initial_visible: bool,
    baseline_visibility_revision: Option<u64>,
    baseline_invocation_id: Option<u64>,
) -> Result<HotkeyBurstSummary, String> {
    if taps == 0 {
        return Err("hotkey burst must contain at least one tap".into());
    }
    let hook_events = events
        .iter()
        .filter(|event| event.contains("trace_event=\"hook_primary\""))
        .collect::<Vec<_>>();
    if hook_events.len() != taps * 2 {
        return Err(format!(
            "expected {} primary hook edges, observed {}",
            taps * 2,
            hook_events.len()
        ));
    }
    let mut hook_transitions = [0usize; 2];
    for event in hook_events {
        if trace_field_value(event, "provenance") != Some("ExternalInjected") {
            return Err("primary hook edge was not externally injected".into());
        }
        let transition = match trace_field_value(event, "transition") {
            Some("Press") => 0,
            Some("Release") => 1,
            other => return Err(format!("invalid primary hook transition {other:?}")),
        };
        hook_transitions[transition] += 1;
    }
    if hook_transitions != [taps, taps] {
        return Err(format!(
            "primary hook press/release counts were {:?}, expected {taps}/{taps}",
            hook_transitions
        ));
    }

    let configured_events = events
        .iter()
        .enumerate()
        .filter(|(_, event)| event.contains("trace_event=\"configured_primary\""))
        .collect::<Vec<_>>();
    if configured_events.len() != taps * 2 {
        return Err(format!(
            "expected {} configured primary edges, observed {}",
            taps * 2,
            configured_events.len()
        ));
    }
    let mut invocation_edges = std::collections::BTreeMap::<u64, [Option<usize>; 2]>::new();
    for (index, event) in configured_events {
        if trace_field_value(event, "provenance") != Some("ExternalInjected") {
            return Err("configured primary edge had incorrect provenance".into());
        }
        let invocation_id = trace_field_value(event, "invocation_id")
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| "configured primary edge omitted its invocation ID".to_string())?;
        let transition = match trace_field_value(event, "transition") {
            Some("Press") => 0,
            Some("Release") => 1,
            other => return Err(format!("invalid configured primary transition {other:?}")),
        };
        let modifiers_match = trace_field_value(event, "modifiers_match");
        match transition {
            0 if modifiers_match != Some("true") => {
                return Err("configured primary press did not match its modifiers".into());
            }
            1 if !matches!(modifiers_match, Some("true" | "false")) => {
                return Err("configured primary release had a malformed modifiers flag".into());
            }
            _ => {}
        }
        let edge = &mut invocation_edges
            .entry(invocation_id)
            .or_insert([None, None])[transition];
        if edge.replace(index).is_some() {
            return Err("configured primary invocation contained a duplicate edge".into());
        }
    }
    if invocation_edges.len() != taps
        || invocation_edges
            .values()
            .any(|edges| edges[0].is_none() || edges[1].is_none() || edges[0] >= edges[1])
    {
        return Err(format!(
            "configured primary transitions did not form {taps} unique press/release invocation pairs"
        ));
    }

    let short_taps = events
        .iter()
        .enumerate()
        .filter(|(_, event)| event.contains("trace_event=\"short_tap\""))
        .collect::<Vec<_>>();
    if short_taps.len() != taps {
        return Err(format!(
            "expected {taps} terminal short taps, observed {}",
            short_taps.len()
        ));
    }
    let mut short_positions = std::collections::BTreeMap::<u64, usize>::new();
    for (index, event) in short_taps {
        let id = trace_field_value(event, "invocation_id")
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| "short tap omitted its invocation ID".to_string())?;
        if trace_field_value(event, "terminal") != Some("true")
            || short_positions.insert(id, index).is_some()
        {
            return Err("short tap was non-terminal or duplicated an invocation ID".into());
        }
    }
    let invocation_ids = invocation_edges.keys().copied().collect::<Vec<_>>();
    if baseline_invocation_id.is_some_and(|baseline| {
        invocation_ids
            .iter()
            .any(|invocation_id| *invocation_id <= baseline)
    }) {
        return Err(format!(
            "post-fence invocation IDs did not advance beyond baseline invocation {baseline_invocation_id:?}"
        ));
    }
    if short_positions.keys().copied().collect::<Vec<_>>() != invocation_ids {
        return Err("configured invocations and terminal short taps did not match".into());
    }
    for (invocation_id, edges) in &invocation_edges {
        let release = edges[1].expect("configured release was checked above");
        let short = short_positions[invocation_id];
        if release >= short {
            return Err(format!(
                "invocation {invocation_id} short-tap decision did not follow its configured release"
            ));
        }
    }

    let visibility_events = events
        .iter()
        .enumerate()
        .filter(|(_, event)| event.contains("trace_event=\"desired_visibility\""))
        .collect::<Vec<_>>();
    let mut committed_visibility = Vec::with_capacity(taps);
    let mut queued_visibility = Vec::new();
    for (index, event) in visibility_events {
        match trace_field_value(event, "source") {
            Some("ToggleBatch" | "LegacyTrigger") => committed_visibility.push((index, event)),
            Some("Queued") => queued_visibility.push((index, event)),
            Some(source) => {
                return Err(format!("visibility event had unrelated source {source}"));
            }
            None => return Err("desired-visibility event omitted its source".into()),
        }
    }
    if committed_visibility.len() != taps {
        return Err(format!(
            "expected {taps} committed desired-visibility decisions, observed {}",
            committed_visibility.len()
        ));
    }
    let mut visible = initial_visible;
    let mut decision_ids = Vec::with_capacity(taps);
    for (index, event) in committed_visibility {
        let invocation_id = trace_field_value(event, "invocation_id")
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| "desired-visibility edge omitted its invocation ID".to_string())?;
        if !invocation_edges.contains_key(&invocation_id) || decision_ids.contains(&invocation_id) {
            return Err("desired-visibility edge was uncorrelated or duplicated".into());
        }
        if index <= short_positions[&invocation_id] {
            return Err(format!(
                "invocation {invocation_id} visibility decision did not follow its terminal short tap"
            ));
        }
        visible = !visible;
        let observed = trace_field_value(event, "visible")
            .and_then(|value| match value {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            })
            .ok_or_else(|| "desired-visibility edge omitted a boolean state".to_string())?;
        if observed != visible {
            return Err(format!(
                "desired-visibility states did not alternate from initial={initial_visible}"
            ));
        }
        decision_ids.push(invocation_id);
    }
    let mut committed_before = Vec::with_capacity(taps);
    for (index, event) in events.iter().enumerate() {
        if !event.contains("trace_event=\"desired_visibility\"") {
            continue;
        }
        let source = trace_field_value(event, "source");
        if matches!(source, Some("ToggleBatch" | "LegacyTrigger")) {
            let observed = trace_field_value(event, "visible")
                .and_then(parse_trace_bool)
                .ok_or_else(|| "desired-visibility decision omitted a boolean state".to_string())?;
            let revision =
                trace_field_value(event, "revision").and_then(|value| value.parse::<u64>().ok());
            let revision = revision
                .ok_or_else(|| "desired-visibility decision omitted its revision".to_string())?;
            committed_before.push((index, observed, revision));
        }
    }
    for (index, event) in queued_visibility {
        let Some((_, committed_visible, committed_revision)) = committed_before
            .iter()
            .rev()
            .find(|(decision_index, _, _)| *decision_index < index)
        else {
            let baseline_echo = baseline_visibility_revision.is_some_and(|revision| {
                trace_field_value(event, "invocation_id") == Some("none")
                    && trace_field_value(event, "revision")
                        .and_then(|value| value.parse::<u64>().ok())
                        == Some(revision)
                    && trace_field_value(event, "visible").and_then(parse_trace_bool)
                        == Some(initial_visible)
            });
            if baseline_echo {
                continue;
            }
            return Err("queued visibility echo preceded any committed burst decision".into());
        };
        let echoed_visible = trace_field_value(event, "visible")
            .and_then(parse_trace_bool)
            .ok_or_else(|| "queued visibility echo omitted a boolean state".to_string())?;
        let echoed_revision =
            trace_field_value(event, "revision").and_then(|value| value.parse::<u64>().ok());
        if trace_field_value(event, "invocation_id") != Some("none")
            || echoed_visible != *committed_visible
            || echoed_revision != Some(*committed_revision)
        {
            return Err(format!(
                "queued visibility echo did not match the latest committed ROOT state: visible={echoed_visible}; revision={echoed_revision:?}; expected_visible={committed_visible}; expected_revision={committed_revision:?}"
            ));
        }
    }
    let release_order = invocation_edges
        .iter()
        .map(|(invocation_id, edges)| {
            (
                edges[1].expect("configured release was checked above"),
                *invocation_id,
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>()
        .into_values()
        .collect::<Vec<_>>();
    if decision_ids != release_order {
        return Err("visibility decisions did not follow one-to-one gesture release order".into());
    }
    Ok(HotkeyBurstSummary {
        invocation_ids,
        final_visible: visible,
    })
}

fn parse_trace_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn trace_field_value<'a>(event: &'a str, key: &str) -> Option<&'a str> {
    event
        .split_ascii_whitespace()
        .find_map(|field| field.strip_prefix(&format!("{key}=")))
        .map(|value| value.trim_matches('"').trim_end_matches(','))
}

fn validate_copied_style_evidence(
    id: &str,
    status: CaseStatus,
    observed: &str,
    required: &[&str],
) -> Result<(), String> {
    if !matches!(status, CaseStatus::Passed) || !observed.starts_with("evidence:v1;") {
        return Err(format!(
            "copied case {id} omitted its versioned evidence summary"
        ));
    }
    if let Some(missing) = required.iter().find(|fact| !observed.contains(**fact)) {
        return Err(format!("copied case {id} omitted required fact {missing}"));
    }
    Ok(())
}

fn report_evidence_value<'a>(observed: &'a str, prefix: &str) -> Option<&'a str> {
    report_evidence_field(observed, prefix)
        .map(|value| value.split([' ', ',']).next().unwrap_or_default())
}

fn validate_case_hotkey_profile_relation(
    report: &AcceptanceReport,
    case: &AcceptanceCaseResult,
) -> Result<(), String> {
    if matches!(
        case.id.as_str(),
        "H01" | "H02" | "H04" | "H06" | "H07" | "H09" | "H10" | "H11" | "H18" | "H7" | "H8"
    ) && report_evidence_value(&case.observed, "hotkey=") != Some(report.hotkey.as_str())
    {
        return Err(format!(
            "case {} tested a different hotkey from the profile fixture",
            case.id
        ));
    }

    if case.id == "H17" {
        validate_h17_hotkey_profile_relation(report, &case.observed)?;
    }
    Ok(())
}

fn validate_h17_hotkey_profile_relation(
    report: &AcceptanceReport,
    observed: &str,
) -> Result<(), String> {
    let main_child_pid = report
        .environment
        .child_process_id
        .filter(|process_id| *process_id > 0)
        .ok_or_else(|| "H17 main child process identity is missing or invalid".to_string())?;

    let main_profile = report_evidence_value(observed, "main_profile=")
        .ok_or_else(|| "H17 main profile hotkey is missing".to_string())?;
    if main_profile != report.hotkey.as_str() {
        return Err("H17 main profile hotkey does not match the report fixture".into());
    }

    let expected_alternate = match report.hotkey {
        AcceptanceHotkey::F11 => AcceptanceHotkey::ShiftAltWinEnd,
        AcceptanceHotkey::ShiftAltWinEnd => AcceptanceHotkey::F11,
    };
    let alternate_profile = report_evidence_value(observed, "alternate_profile=")
        .ok_or_else(|| "H17 alternate profile hotkey is missing".to_string())?;
    if alternate_profile != expected_alternate.as_str() {
        return Err("H17 alternate profile hotkey is not the opposite configured hotkey".into());
    }

    let alternate_profile_id = report_evidence_field(observed, "alternate_profile_id=")
        .ok_or_else(|| "H17 alternate profile identity is missing".to_string())?;
    if alternate_profile_id.is_empty()
        || alternate_profile_id.len() > 128
        || !alternate_profile_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("H17 alternate profile identity is empty, malformed, or unbounded".into());
    }

    let alternate_child_pid = report_evidence_field(observed, "alternate_child_pid=")
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|process_id| *process_id > 0)
        .ok_or_else(|| "H17 alternate child process identity is missing or invalid".to_string())?;
    if main_child_pid == alternate_child_pid {
        return Err("H17 alternate and main profiles report the same child process".into());
    }
    Ok(())
}

fn report_evidence_field<'a>(observed: &'a str, prefix: &str) -> Option<&'a str> {
    observed
        .split_once("evidence:v1;")?
        .1
        .split(';')
        .map(str::trim)
        .find_map(|field| field.strip_prefix(prefix))
}

fn observed_field<'a>(observed: &'a str, prefix: &str) -> Option<&'a str> {
    observed
        .split(';')
        .map(str::trim)
        .find_map(|field| field.strip_prefix(prefix))
}

fn h04_retry_ledger<'a>(
    status: CaseStatus,
    observed: &'a str,
) -> Result<(u8, u8, Vec<&'a str>), String> {
    let attempts = observed_field(observed, "matrix_attempts=")
        .and_then(|value| value.parse::<u8>().ok())
        .ok_or_else(|| "H04 retry ledger has no bounded matrix-attempt count".to_string())?;
    let contaminations = observed_field(observed, "contamination_attempts=")
        .and_then(|value| value.parse::<u8>().ok())
        .ok_or_else(|| "H04 retry ledger has no bounded contamination count".to_string())?;
    let artifact_names = observed_field(observed, "contamination_artifacts=")
        .ok_or_else(|| "H04 retry ledger has no contamination-artifact list".to_string())?;
    let names = if artifact_names == "none" {
        Vec::new()
    } else {
        artifact_names.split('|').collect::<Vec<_>>()
    };
    if !(1..=2).contains(&attempts)
        || contaminations > attempts
        || names.len() != usize::from(contaminations)
        || observed_field(observed, "attempt_restart_state=") != Some("hidden")
        || observed_field(observed, "full_clean_matrix=")
            != Some(if matches!(status, CaseStatus::Passed) {
                "true"
            } else {
                "false"
            })
        || (matches!(status, CaseStatus::Passed) && contaminations != attempts.saturating_sub(1))
        || (contaminations == 0 && attempts != 1)
        || (contaminations > 0 && attempts != 2)
    {
        return Err("H04 retry ledger does not describe a bounded whole-matrix attempt".into());
    }
    if matches!(status, CaseStatus::Passed) {
        let quiet_range = observed_field(observed, "quiet_window_ms=").and_then(|range| {
            let (min, max) = range.split_once("..")?;
            Some((min.parse::<u128>().ok()?, max.parse::<u128>().ok()?))
        });
        let preflight_matching_edges = observed_field(observed, "preflight_matching_edges=")
            .and_then(|value| value.parse::<usize>().ok());
        let foreign_matching_edges = observed_field(observed, "foreign_matching_edges=")
            .and_then(|value| value.parse::<usize>().ok());
        if quiet_range.is_none_or(|(min, max)| min < 75 || max > 100 || min > max)
            || preflight_matching_edges.is_none()
            || foreign_matching_edges.is_none()
            || observed_field(observed, "modifiers_clear_each_burst=") != Some("true")
        {
            return Err(
                "H04 retry ledger omitted its bounded quiet/modifier preflight proof".into(),
            );
        }
    }
    Ok((attempts, contaminations, names))
}

fn format_h04_matrix_evidence(
    hotkey: AcceptanceHotkey,
    invocation_ids_sha256: &str,
    quiet_min_ms: u128,
    quiet_max_ms: u128,
    preflight_matching_edges: usize,
    foreign_matching_edges: usize,
) -> String {
    format!(
        "evidence:v1; hotkey={}; matrix=hidden+visible:1,2,5,10,25; matrix_groups=10; decisions=86; unique_invocation_ids=86; invocation_ids_sha256={invocation_ids_sha256}; readable_cadence=3_individual_short_taps; readable_decisions=3; readable_target=owned_foreground_each_tap; focused_root_hide=true; readable_refocus=none; down_ms=25; released_ms=75; quiet_window_ms={quiet_min_ms}..{quiet_max_ms}; preflight_matching_edges={preflight_matching_edges}; foreign_matching_edges={foreign_matching_edges}; modifiers_clear_each_burst=true; uninterrupted=true; inter_tap_ui_poll=0; inter_tap_refocus=0; inter_tap_preflight=0; pointer_movement=0; per_gesture_correlation=release_short_tap_visibility; no_hold_promotion=true; observer=exact_injected_chord_edges; preflight=once_per_burst; input_desktop=thread=Default,active=Default; key_cleanup=verified",
        hotkey.as_str()
    )
}

fn evidence_contains_token(observed: &str, expected: &str) -> bool {
    observed
        .split_once("evidence:v1;")
        .map(|(_, fields)| fields)
        .is_some_and(|fields| fields.split(';').any(|field| field.trim() == expected))
}

fn validate_copied_style_evidence_relations(report: &AcceptanceReport) -> Result<(), String> {
    let observed = |id: &str| {
        report
            .cases
            .iter()
            .find(|case| case.id == id)
            .map(|case| case.observed.as_str())
    };
    let a5 =
        observed("CP_A5").ok_or_else(|| "copied CP_A5 style evidence is missing".to_string())?;
    let a6 = observed("CP_A6")
        .ok_or_else(|| "copied CP_A6 persistence evidence is missing".to_string())?;
    let a7 = observed("CP_A7")
        .ok_or_else(|| "copied CP_A7 lifecycle evidence is missing".to_string())?;
    let transition = report_evidence_value(a5, "glow=")
        .and_then(|value| value.split_once("->"))
        .ok_or_else(|| "copied CP_A5 style transition is malformed".to_string())?;
    let saved = report_evidence_value(a6, "glow=")
        .ok_or_else(|| "copied CP_A6 persisted style value is missing".to_string())?;
    let undone = report_evidence_value(a7, "undo_restored=")
        .ok_or_else(|| "copied CP_A7 undo value is missing".to_string())?;
    let redone = report_evidence_value(a7, "redo_restored=")
        .ok_or_else(|| "copied CP_A7 redo value is missing".to_string())?;
    if transition.1 != saved
        || transition.0 == transition.1
        || undone != saved
        || !["true", "false"].contains(&redone)
        || redone == saved
    {
        return Err("copied style edit, saved value, Undo, and Redo evidence disagree".into());
    }
    Ok(())
}

fn required_case_evidence(id: &str) -> Option<&'static [&'static str]> {
    match id {
        "H01" => Some(&[
            "initial_hidden=true",
            "target=runner_owned",
            "grid_visible=true",
            "focused_root=true",
            "radial=closed",
        ]),
        "H02" => Some(&[
            "initial_visible=true",
            "target=root_focused",
            "grid_visible=false",
            "stays_hidden=true",
            "restore=none",
        ]),
        "H08" => Some(&[
            "gate=real_runtime_radial_prepare",
            "tap_before_ready=true",
            "cancelled_by_tap=true",
            "late_reply=rejected",
            "native_ready=none",
            "radial_reopened=none",
            "grid_hidden=true",
        ]),
        "H06" => Some(&[
            "runtime_radial=dismissed",
            "grid_visible=true",
            "selection=none",
            "dispatch=none",
            "child_surface=closed",
            "hover_ack=executable_cell",
        ]),
        "H07" => Some(&[
            "runtime_radial=dismissed",
            "grid_visible=false",
            "selection=none",
            "dispatch=none",
            "next_gesture=usable",
        ]),
        "H09" => Some(&[
            "initial_visible=hidden+visible",
            "radial_opened=true",
            "grid_visibility_unchanged=true",
            "release_no_toggle=true",
        ]),
        "H10" => Some(&[
            "radial_close=hold",
            "hover_ack=executable_cell",
            "release_keys=clear",
            "grid_toggle=none",
            "selection=none",
            "dispatch=none",
            "late_reopen=none",
        ]),
        "H11" => Some(&[
            "designer_session_preserved=true",
            "draft_digest_preserved=true",
            "save_discard_close=none",
            "ordinary_focus_forced=false",
            "repeated_taps=5",
            "designer_start_states=visible+hidden",
            "hidden_start_burst=true",
            "designer_cleanup=closed_cleanly",
        ]),
        "H12" => Some(&[
            "designer_dirty=true",
            "native_preview=active",
            "runtime_hold_opened=true",
            "runtime_tap_dismissed=true",
            "preview_survived=true",
            "preview_baseline_preserved=true",
            "runtime_surfaces_new=true",
            "designer_session_preserved=true",
            "draft_digest_preserved=true",
            "preview_cleanup=stopped",
            "designer_cleanup=closed_cleanly",
        ]),
        "H16" => Some(&[
            "direct_trigger=opened",
            "native_launcher_tap=dismissed+grid_toggled",
            "legacy_launcher_tap=dismissed+grid_toggled",
            "legacy_source=HotkeyTrigger+LegacyTrigger",
            "direct_trigger_preserved=true",
            "root_refreshed_after_direct=true",
            "legacy_profile_cleanup=verified",
            "legacy_child_pid=",
            "legacy_profile_sha256=",
            "legacy_trace_artifact=",
            "legacy_profile_artifact=",
        ]),
        "H17" => Some(&[
            "f11_control=passed",
            "exact_chord=passed",
            "mouse_gestures=enabled",
            "profile_matches=true",
            "alternate_profile_cleanup=verified",
            "main_profile=",
            "alternate_profile=",
            "alternate_profile_id=",
            "alternate_child_pid=",
        ]),
        "H18" => Some(&[
            "root_start=parked",
            "pointer_stationary=true",
            "tap_woke_root=true",
            "hold_opened_radial=true",
            "designer=closed",
        ]),
        "D2" => Some(&[
            "text_edit=restored",
            "tab_focus=menu_combo",
            "unsaved=false",
        ]),
        "A2" => Some(&[
            "geometry=[8,10]",
            "candidate_ids_preserved=true",
            "committed=true",
        ]),
        "G1" => Some(&["overflow_root=[8,1]/9", "stable_ids=true"]),
        "A3" => Some(&[
            "blank_cell_selected=true",
            "catalog_rank_gt_50=true",
            "searched_action_assigned=true",
        ]),
        "A5" => Some(&[
            "glow=true->false",
            "preview_reply=accepted",
            "preview_rendered=true",
        ]),
        "A6" => Some(&[
            "typed_radial=decoded",
            "authored_geometry=[8,10]",
            "action_binding=true",
            "after_action=close_tree",
            "overflow_root=[8,1]/9",
            "glow=false",
        ]),
        "A7" => Some(&["undo_restored=false", "redo_restored=true"]),
        "D3" => Some(&[
            "root_hidden=true",
            "designer_responsive=true",
            "preview_stop=accepted",
            "root_shown=true",
            "hook_pairs=true",
        ]),
        "D6" => Some(&[
            "keep_editing=retained_dirty",
            "draft_glow=true",
            "discard=saved_json_unchanged",
        ]),
        "D7" => Some(&[
            "pending_request=true",
            "cancelled_before_prompt=true",
            "late_reply=rejected",
            "stop=accepted",
            "no_reopen=1s",
            "marker_clean=true",
        ]),
        _ => None,
    }
}

#[cfg(windows)]
fn push_final_case(
    report: &mut AcceptanceReport,
    id: &str,
    status: CaseStatus,
    observed: &str,
    failure_stage: Option<FailureStage>,
    artifacts: Vec<String>,
    run_started: Instant,
) {
    report.push_case(AcceptanceCaseResult {
        id: id.into(),
        status,
        elapsed_ms: u64::try_from(run_started.elapsed().as_millis()).unwrap_or(u64::MAX),
        expected: expected_final_case(id).into(),
        observed: bounded_text(observed, MAX_RESULT_BYTES),
        failure_stage,
        artifacts,
    });
}

fn expected_final_case(id: &str) -> &'static str {
    match id {
        "R0" => {
            "valid bounded JSON and text reports identify source, profile, hashes, elapsed time, and evidence"
        }
        "R1" => {
            "controlled harness failure writes bounded privacy-safe trace and owned screenshot evidence"
        }
        "CP_R1" => {
            "copied-profile diagnostic evidence validates; failing bundles persist outside the copied profile"
        }
        "R2" => "child process, HWNDs, temp profile, and native input state are cleaned up",
        _ => "required native acceptance case is recorded",
    }
}

#[cfg(windows)]
fn prepare_output_directory(arguments: &Arguments) -> Result<PathBuf, String> {
    if let Some(report) = &arguments.report_file {
        let parent = report
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if !parent.is_dir() {
            return Err("--report parent directory must already exist".into());
        }
        return parent
            .canonicalize()
            .map_err(|error| format!("resolve report parent: {error}"));
    }
    let output = arguments
        .output
        .as_deref()
        .ok_or_else(|| "--output is required".to_string())?;
    fs::create_dir_all(output).map_err(|error| format!("create output directory: {error}"))?;
    let output = output
        .canonicalize()
        .map_err(|error| format!("resolve output directory: {error}"))?;
    let mut entries =
        fs::read_dir(&output).map_err(|error| format!("inspect output directory: {error}"))?;
    if entries.next().is_some() {
        return Err("output directory must be empty so run artifacts are never overwritten".into());
    }
    Ok(output)
}

fn deterministic_fixture(
    log_path: &Path,
    mouse_gesture_mode: MouseGestureMode,
) -> Result<DeterministicFixture, String> {
    deterministic_fixture_for_hotkey(log_path, mouse_gesture_mode, AcceptanceHotkey::F11)
}

fn deterministic_fixture_for_hotkey(
    log_path: &Path,
    mouse_gesture_mode: MouseGestureMode,
    hotkey: AcceptanceHotkey,
) -> Result<DeterministicFixture, String> {
    let mut settings = Settings::default();
    settings.hotkey = Some(hotkey.as_str().to_string());
    settings.help_hotkey = None;
    settings.quit_hotkey = None;
    settings.debug_logging = true;
    settings.log_file = Some(LogFile::Path(log_path.to_string_lossy().to_string()));
    settings.follow_mouse = false;
    settings.static_location_enabled = true;
    settings.static_pos = Some((240, 180));
    settings.static_size = Some((900, 650));
    settings.window_size = Some((900, 650));
    settings.radial.enabled = true;
    settings.radial.shared_tap_hold = true;
    if mouse_gesture_mode == MouseGestureMode::DisabledDiagnostic {
        settings.plugin_settings.insert(
            "mouse_gestures".into(),
            serde_json::json!({ "enabled": false }),
        );
    }
    let mut document = RadialDocument::starter();
    let actions = (0..ACCEPTANCE_ACTION_COUNT)
        .map(|index| multi_launcher::actions::Action {
            label: format!("Radial Acceptance Harmless Action {index:03}"),
            desc: "Deterministic native authoring fixture".into(),
            action: format!("radial_acceptance_harmless_{index:03}"),
            args: None,
        })
        .collect::<Vec<_>>();
    bind_hotkey_hover_probe_action(
        &mut document,
        actions.first().ok_or_else(|| {
            "the deterministic fixture has no harmless hover-probe action".to_string()
        })?,
    )?;
    validate_radial_document(&document)
        .map_err(|error| format!("starter radial document is invalid: {error:?}"))?;
    let reserved = [("launcher", settings.hotkey.as_deref())]
        .into_iter()
        .filter_map(|(owner, chord)| chord.map(|chord| (owner.to_string(), chord.to_string())))
        .collect::<Vec<_>>();
    let issues = radial_settings::validate(&settings.radial, &document, &reserved);
    if !issues.is_empty() {
        return Err(format!("radial settings are invalid: {issues:?}"));
    }
    multi_launcher::hotkey::parse_hotkey(hotkey.as_str()).ok_or_else(|| {
        format!(
            "the deterministic {} acceptance chord is unsupported",
            hotkey.as_str()
        )
    })?;

    Ok(DeterministicFixture {
        settings_json: serde_json::to_vec_pretty(&settings)
            .map_err(|error| format!("serialize settings: {error}"))?,
        radial_json: serde_json::to_vec_pretty(&document)
            .map_err(|error| format!("serialize radial document: {error}"))?,
        actions_json: serde_json::to_vec_pretty(&actions)
            .map_err(|error| format!("serialize custom action fixture: {error}"))?,
        hold_threshold_ms: settings.radial.hold_threshold_ms,
    })
}

fn bind_hotkey_hover_probe_action(
    document: &mut RadialDocument,
    action: &multi_launcher::actions::Action,
) -> Result<(), String> {
    let default_menu_id = document.default_menu_id.clone();
    let menu = document
        .menus
        .iter_mut()
        .find(|menu| menu.id == default_menu_id)
        .ok_or_else(|| "the starter radial has no default menu for the hover probe".to_string())?;
    let cell = menu
        .rings
        .first_mut()
        .and_then(|ring| ring.cells.first_mut())
        .ok_or_else(|| {
            "the starter default menu has no root cell for the hover probe".to_string()
        })?;
    cell.label = action.label.clone();
    cell.after_action = AfterActionPolicy::CloseTree;
    cell.content = CellContent::Action {
        binding: ActionBinding::Persisted {
            action: PersistedUniversalActionRef {
                target: Some(PersistableActionTargetRef::CustomAction {
                    action: action.clone(),
                }),
                action_id: action_ids::RESULT_EXECUTE,
            },
        },
    };
    Ok(())
}

fn deterministic_fixture_for_hotkey_with_direct_trigger(
    log_path: &Path,
    mouse_gesture_mode: MouseGestureMode,
    hotkey: AcceptanceHotkey,
) -> Result<DeterministicFixture, String> {
    deterministic_fixture_for_hotkey_with_direct_trigger_chord(
        log_path,
        mouse_gesture_mode,
        hotkey,
        "Ctrl+Alt+T",
        true,
    )
}

fn deterministic_fixture_for_hotkey_with_direct_trigger_chord(
    log_path: &Path,
    mouse_gesture_mode: MouseGestureMode,
    hotkey: AcceptanceHotkey,
    direct_trigger_chord: &str,
    shared_tap_hold: bool,
) -> Result<DeterministicFixture, String> {
    let mut fixture = deterministic_fixture_for_hotkey(log_path, mouse_gesture_mode, hotkey)?;
    let mut settings: Settings = serde_json::from_slice(&fixture.settings_json)
        .map_err(|error| format!("decode acceptance settings fixture: {error}"))?;
    settings.radial.shared_tap_hold = shared_tap_hold;
    let mut document: RadialDocument = serde_json::from_slice(&fixture.radial_json)
        .map_err(|error| format!("decode acceptance radial fixture: {error}"))?;
    document
        .custom_triggers
        .push(multi_launcher::radial::model::TriggerDefinition {
            id: multi_launcher::radial::model::TriggerId::new("acceptance-direct-trigger"),
            chord: direct_trigger_chord.into(),
            menu_id: document.default_menu_id.clone(),
            scope: multi_launcher::radial::model::TriggerScope::Global,
        });
    validate_radial_document(&document)
        .map_err(|error| format!("direct-trigger radial fixture is invalid: {error:?}"))?;
    let issues = radial_settings::validate(
        &settings.radial,
        &document,
        &[("launcher".into(), hotkey.as_str().into())],
    );
    if !issues.is_empty() {
        return Err(format!(
            "direct-trigger radial settings are invalid: {issues:?}"
        ));
    }
    fixture.radial_json = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("serialize direct-trigger radial fixture: {error}"))?;
    fixture.settings_json = serde_json::to_vec_pretty(&settings)
        .map_err(|error| format!("serialize direct-trigger settings fixture: {error}"))?;
    Ok(fixture)
}

#[cfg(windows)]
fn copied_profile_trace_path(copy_root: &Path) -> PathBuf {
    copy_root.join("acceptance.log")
}

#[cfg(windows)]
fn prepare_copied_profile(
    source_inventory: &copied_profile::ProfileInventory,
    copy_root: &Path,
) -> Result<CopiedProfileMetadata, String> {
    let initial_copy = source_inventory.copy_to(copy_root)?;
    let source_hash = |name: &str| {
        source_inventory
            .file_hash(name)
            .map(str::to_owned)
            .ok_or_else(|| format!("profile copy requires a regular root {name}"))
    };
    let source_settings_sha256 = source_hash("settings.json")?;
    let source_radial_sha256 = source_hash("radial.json")?;
    let source_actions_sha256 = match source_inventory.file_hash("actions.json") {
        Some(hash) => Some(hash.to_owned()),
        None => match fs::symlink_metadata(source_inventory.root.join("actions.json")) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Ok(_) => return Err("profile root actions.json must be a regular file".into()),
            Err(_) => return Err("profile root actions.json could not be inspected".into()),
        },
    };
    let copied_initial_settings_sha256 = initial_copy
        .file_hash("settings.json")
        .ok_or_else(|| "copied settings.json was not inventoried".to_string())?
        .to_owned();
    let copied_initial_radial_sha256 = initial_copy
        .file_hash("radial.json")
        .ok_or_else(|| "copied radial.json was not inventoried".to_string())?
        .to_owned();
    let copied_initial_actions_sha256 = initial_copy.file_hash("actions.json").map(str::to_owned);
    if copied_initial_settings_sha256 != source_settings_sha256
        || copied_initial_radial_sha256 != source_radial_sha256
        || copied_initial_actions_sha256 != source_actions_sha256
    {
        return Err(
            "copied critical profile bytes differ from the initial source inventory".into(),
        );
    }

    let settings_path = copy_root.join("settings.json");
    let mut settings = match Settings::load_typed(&settings_path)
        .map_err(|_| "copied settings.json could not be typed-loaded".to_string())?
    {
        LoadState::Loaded(settings) => settings,
        LoadState::Missing | LoadState::Empty => {
            return Err("copied settings.json must contain a typed settings object".into());
        }
    };
    if settings
        .radial_submenu_migration
        .as_ref()
        .is_some_and(|receipt| {
            matches!(
                receipt.state,
                SubmenuMigrationState::Prepared | SubmenuMigrationState::UndoPrepared
            )
        })
    {
        return Err(
            "copied profile has an unfinished submenu migration receipt with external recovery paths".into(),
        );
    }
    let expected_submenu_migration_receipt = settings.radial_submenu_migration.clone();
    let radial_bytes = fs::read(copy_root.join("radial.json"))
        .map_err(|_| "copied radial.json could not be read".to_string())?;
    let decoded = multi_launcher::radial::migration::decode_document(&radial_bytes)
        .map_err(|_| "copied radial.json failed typed decode or validation".to_string())?;
    validate_radial_document(&decoded.document)
        .map_err(|_| "copied radial.json failed current document validation".to_string())?;
    if decoded.document.skins.is_empty() {
        return Err("copied radial document has no skin to exercise in the Designer".into());
    }
    let reserved_hotkey = [(
        "acceptance launcher".to_string(),
        ACCEPTANCE_HOTKEY.to_string(),
    )];
    let issues = radial_settings::validate(&settings.radial, &decoded.document, &reserved_hotkey);
    if !issues.is_empty() {
        return Err(
            "copied radial settings conflict with the safe acceptance hotkey or document".into(),
        );
    }
    let original_menu_sha256 = decoded
        .document
        .menus
        .iter()
        .map(|menu| {
            serde_json::to_vec(menu)
                .map(|bytes| sha256_bytes(&bytes))
                .map_err(|_| "could not hash an initial radial menu definition".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let restore_menu_name = decoded
        .document
        .menus
        .iter()
        .find(|menu| menu.id == decoded.document.default_menu_id)
        .map(|menu| menu.name.clone())
        .ok_or_else(|| "copied radial document default menu could not be resolved".to_string())?;

    let startup_actions =
        multi_launcher::actions::load_startup_actions(copy_root.join("actions.json"));
    if startup_actions.diagnostic.is_some() {
        return Err("copied actions.json could not be typed-loaded".into());
    }
    let mut actions = startup_actions.actions;
    let target_action_index = actions
        .len()
        .checked_add(ACCEPTANCE_ACTION_COUNT - 1)
        .ok_or_else(|| "copied action index overflowed".to_string())?;
    for index in actions.len()..=target_action_index {
        let label = format!("Radial Acceptance Harmless Action {index:03}");
        if actions.iter().any(|action| action.label == label) {
            return Err("copied profile already contains an acceptance action label".into());
        }
        actions.push(multi_launcher::actions::Action {
            label,
            desc: "Inert native acceptance authoring target; never dispatched".into(),
            action: format!("radial_acceptance_inert_{index:03}"),
            args: None,
        });
    }

    settings.hotkey = Some(ACCEPTANCE_HOTKEY.to_string());
    settings.help_hotkey = None;
    settings.quit_hotkey = None;
    settings.index_paths = None;
    settings.plugin_dirs = None;
    settings.enabled_plugins = Some(std::collections::HashSet::from(["radial".to_string()]));
    settings.enabled_capabilities = Some(std::collections::HashMap::new());
    settings.plugin_settings.clear();
    settings.plugin_settings.insert(
        "clipboard_modify".into(),
        serde_json::to_value(multi_launcher::settings::ClipboardModifyPluginSettings::default())
            .map_err(|_| "default clipboard settings could not be serialized".to_string())?,
    );
    settings.pinned_panels.clear();
    settings.debug_logging = true;
    settings.log_file = Some(LogFile::Path(
        copied_profile_trace_path(copy_root)
            .to_string_lossy()
            .into_owned(),
    ));
    settings.screenshot_dir = Some(
        copy_root
            .join("acceptance_screenshots")
            .to_string_lossy()
            .into_owned(),
    );
    settings.screenshot_save_file = false;
    settings.screenshot_auto_save = false;
    settings.screenshot_use_editor = false;
    settings.radial.global_item_inputs = false;
    settings.dashboard.enabled = false;
    settings.dashboard.config_path = None;
    settings.dashboard.default_location = None;
    settings.multi_manager.enabled = false;
    settings.multi_manager.workspaces_path = copy_root
        .join("multi_manager/workspaces.json")
        .to_string_lossy()
        .into_owned();
    settings.multi_manager.bindings_path = copy_root
        .join("multi_manager/bindings.json")
        .to_string_lossy()
        .into_owned();
    settings.multi_manager.auto_reconnect_on_load = false;
    settings.multi_manager.auto_save = false;
    settings.multi_manager.save_on_exit = false;

    let post_issues =
        radial_settings::validate(&settings.radial, &decoded.document, &reserved_hotkey);
    if !post_issues.is_empty() || multi_launcher::hotkey::parse_hotkey(ACCEPTANCE_HOTKEY).is_none()
    {
        return Err("normalized copied profile failed acceptance settings validation".into());
    }
    let settings_bytes = serde_json::to_vec_pretty(&settings)
        .map_err(|_| "normalized copied settings could not be serialized".to_string())?;
    let actions_bytes = serde_json::to_vec_pretty(&actions)
        .map_err(|_| "copied acceptance action catalog could not be serialized".to_string())?;
    fs::write(&settings_path, settings_bytes).map_err(|_| {
        "normalized copied settings could not be written inside the copy".to_string()
    })?;
    fs::write(copy_root.join("actions.json"), actions_bytes)
        .map_err(|_| "copied action catalog could not be written inside the copy".to_string())?;

    let launch_settings_sha256 = sha256_file(&settings_path)
        .map_err(|_| "could not hash normalized copied settings".to_string())?;
    let launch_radial_sha256 = sha256_file(&copy_root.join("radial.json"))
        .map_err(|_| "could not hash launch radial document".to_string())?;
    let launch_actions_sha256 = sha256_file(&copy_root.join("actions.json"))
        .map_err(|_| "could not hash copied action catalog".to_string())?;
    let (copied_file_count, copied_total_bytes) = source_inventory.file_count_and_bytes();
    Ok(CopiedProfileMetadata {
        initial_copy_tree_sha256: initial_copy.tree_sha256,
        copied_file_count,
        copied_total_bytes,
        source_settings_sha256,
        source_radial_sha256,
        source_actions_sha256,
        copied_initial_settings_sha256,
        copied_initial_radial_sha256,
        copied_initial_actions_sha256,
        launch_settings_sha256,
        launch_radial_sha256,
        launch_actions_sha256,
        target_action_index,
        skin_index: 0,
        restore_menu_name,
        original_menu_sha256,
        expected_submenu_migration_receipt,
        hold_threshold_ms: settings.radial.hold_threshold_ms,
    })
}

fn inspect_candidate(explicit_path: Option<&Path>) -> Result<CandidateIdentity, String> {
    let path = if let Some(path) = explicit_path {
        path.to_path_buf()
    } else {
        let runner = std::env::current_exe()
            .map_err(|error| format!("find radial_acceptance executable: {error}"))?;
        runner
            .parent()
            .ok_or_else(|| "runner executable has no parent directory".to_string())?
            .join("multi_launcher.exe")
    };
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("launcher candidate is not accessible: {error}"))?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        return Err("launcher candidate must be a regular non-reparse executable".into());
    }
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Err("launcher candidate must have an .exe extension".into());
    }
    let executable = path
        .canonicalize()
        .map_err(|error| format!("resolve launcher candidate: {error}"))?;
    let sha256 =
        sha256_file(&executable).map_err(|error| format!("hash launcher candidate: {error}"))?;
    Ok(CandidateIdentity {
        executable: bounded_text(&executable.to_string_lossy(), MAX_PATH_BYTES),
        sha256,
    })
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            format!(
                "create {}: {error}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )
        })?;
    output.write_all(bytes).map_err(|error| {
        format!(
            "write {}: {error}",
            path.file_name().unwrap_or_default().to_string_lossy()
        )
    })
}

fn write_report(path: &Path, report: &mut AcceptanceReport) -> Result<(), String> {
    bound_report_for_persistence(report)?;
    let expected_value = serde_json::to_value(&*report)
        .map_err(|error| format!("build acceptance report JSON value: {error}"))?;
    let bytes = serde_json::to_vec(report)
        .map_err(|error| format!("serialize acceptance report: {error}"))?;
    if bytes.is_empty() || bytes.len() > MAX_JSON_REPORT_BYTES {
        return Err(format!(
            "serialized acceptance JSON is empty or exceeds {} bytes",
            MAX_JSON_REPORT_BYTES
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create acceptance report: {error}"))?;
    file.write_all(&bytes)
        .map_err(|error| format!("write acceptance report: {error}"))?;
    drop(file);
    let stored = fs::read(path).map_err(|error| format!("read acceptance report back: {error}"))?;
    let decoded: serde_json::Value = serde_json::from_slice(&stored)
        .map_err(|error| format!("validate persisted acceptance JSON: {error}"))?;
    if decoded != expected_value {
        return Err("persisted JSON report differs from the serialized report model".into());
    }
    Ok(())
}

fn report_serialized_sizes(report: &AcceptanceReport) -> Result<(usize, usize), String> {
    let json_size = serde_json::to_vec(report)
        .map_err(|error| format!("size acceptance JSON report: {error}"))?
        .len();
    let text_size = render_text_report(report).len();
    Ok((json_size, text_size))
}

fn bound_report_for_persistence(report: &mut AcceptanceReport) -> Result<(), String> {
    let fits = |report: &AcceptanceReport| {
        report_serialized_sizes(report).is_ok_and(|(json, text)| {
            json > 0 && json <= MAX_JSON_REPORT_BYTES && text > 0 && text <= MAX_TEXT_REPORT_BYTES
        })
    };
    if fits(report) {
        return Ok(());
    }

    report.mark_capacity_saturated();
    report.outcome = "failed";
    let omitted_artifact_references = report.artifacts.len()
        + report
            .cases
            .iter()
            .map(|case| case.artifacts.len())
            .sum::<usize>();
    report.artifacts.clear();
    for case in &mut report.cases {
        case.artifacts.clear();
    }
    report.candidate.executable = bounded_text(&report.candidate.executable, 256);
    report.profile.temporary_data_root = bounded_text(&report.profile.temporary_data_root, 256);
    report.environment.os_version = bounded_text(&report.environment.os_version, 256);
    report.environment.architecture = bounded_text(&report.environment.architecture, 128);
    report.environment.monitors.truncate(8);
    for case in &mut report.cases {
        case.expected = bounded_text(&case.expected, 256);
        case.observed = bounded_text(&case.observed, 256);
    }
    report.report_overflow = Some(ReportOverflowReceipt {
        reason: "serialized report exceeded the bounded output size; excess artifact references were omitted".into(),
        omitted_case_evidence: 0,
        omitted_artifact_references,
        affected_case_ids: Vec::new(),
    });
    mark_persistence_overflow_case(report, "R0");
    mark_persistence_overflow_case(report, "CLEANUP");
    report.capacity_saturated = true;
    if fits(report) {
        return Ok(());
    }

    // All report-controlled lists and long free-form fields have been reduced.
    // Keep a final smaller evidence envelope for unusual control-heavy strings.
    report.candidate.executable = bounded_text(&report.candidate.executable, 64);
    report.profile.temporary_data_root = bounded_text(&report.profile.temporary_data_root, 64);
    report.environment.os_version = bounded_text(&report.environment.os_version, 64);
    report.environment.architecture = bounded_text(&report.environment.architecture, 64);
    report.environment.monitors.truncate(4);
    for case in &mut report.cases {
        case.expected = bounded_text(&case.expected, 64);
        case.observed = bounded_text(&case.observed, 64);
    }
    if fits(report) {
        return Ok(());
    }

    let affected_case_ids = report
        .cases
        .iter()
        .filter(|case| {
            hotkey_expected_state(&case.id).is_some()
                && (matches!(case.status, CaseStatus::Passed)
                    || report
                        .hotkey_evidence
                        .iter()
                        .any(|packet| packet.case_id == case.id))
        })
        .map(|case| case.id.clone())
        .collect::<Vec<_>>();
    let omitted_case_evidence = report.hotkey_evidence.len();
    for id in &affected_case_ids {
        mark_persistence_overflow_case(report, id);
    }
    report.hotkey_evidence.clear();
    report.report_overflow = Some(ReportOverflowReceipt {
        reason:
            "serialized evidence exceeded the bounded report size; per-case H packets were omitted"
                .into(),
        omitted_case_evidence,
        omitted_artifact_references,
        affected_case_ids,
    });
    mark_persistence_overflow_case(report, "R0");
    mark_persistence_overflow_case(report, "CLEANUP");
    report.capacity_saturated = true;
    report.outcome = "failed";
    report.artifacts.clear();
    for case in &mut report.cases {
        case.artifacts.clear();
    }
    if fits(report) {
        return Ok(());
    }

    Err("fixed acceptance report metadata exceeds its serialized size limits after bounded evidence compaction".into())
}

fn mark_persistence_overflow_case(report: &mut AcceptanceReport, id: &str) {
    if let Some(case) = report.cases.iter_mut().find(|case| case.id == id) {
        case.status = CaseStatus::Failed;
        case.failure_stage = Some(FailureStage::Environment);
        case.observed = "report evidence overflow prevented complete bounded persistence".into();
    } else if report.cases.len() < MAX_CASES {
        report.cases.push(AcceptanceCaseResult {
            id: id.to_owned(),
            status: CaseStatus::Failed,
            elapsed_ms: 0,
            expected: "bounded acceptance evidence and cleanup".into(),
            observed: "report evidence overflow prevented complete bounded persistence".into(),
            failure_stage: Some(FailureStage::Environment),
            artifacts: Vec::new(),
        });
    }
}

fn write_text_report(path: &Path, report: &AcceptanceReport) -> Result<(), String> {
    let contents = render_text_report(report);
    if contents.is_empty() || contents.len() > MAX_TEXT_REPORT_BYTES {
        return Err(format!(
            "serialized text report is empty or exceeds {} bytes",
            MAX_TEXT_REPORT_BYTES
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create text report: {error}"))?;
    file.write_all(contents.as_bytes())
        .map_err(|error| format!("write text report: {error}"))?;
    drop(file);
    let stored =
        fs::read_to_string(path).map_err(|error| format!("read text report back: {error}"))?;
    let status_text = match report.copied_profile_status {
        CopiedProfileStatus::NotRun => "not_run",
        CopiedProfileStatus::Running => "running",
        CopiedProfileStatus::Passed => "passed",
        CopiedProfileStatus::Failed => "failed",
    };
    if stored != contents || !stored.contains(&format!("Copied profile: {status_text}")) {
        return Err(
            "persisted text report does not match the complete bounded report model".into(),
        );
    }
    Ok(())
}

fn render_text_report(report: &AcceptanceReport) -> String {
    let mut contents = String::new();
    contents.push_str(&format!("Radial native acceptance: {}\n", report.outcome));
    contents.push_str(&format!("Candidate SHA-256: {}\n", report.candidate.sha256));
    contents.push_str(&format!(
        "Runner SHA-256: {:?}\n",
        report.environment.runner_sha256
    ));
    contents.push_str(&format!(
        "Source revision: {:?}\n",
        report.environment.source_revision
    ));
    contents.push_str(&format!(
        "Child PID: {:?}\n",
        report.environment.child_process_id
    ));
    contents.push_str(&format!("Suite: {}\n", report.suite.as_str()));
    contents.push_str(&format!("Hotkey: {}\n", report.profile.configured_hotkey));
    contents.push_str(&format!("Started Unix ms: {}\n", report.started_unix_ms));
    contents.push_str(&format!("Finished Unix ms: {}\n", report.finished_unix_ms));
    let copied_profile_status = match report.copied_profile_status {
        CopiedProfileStatus::NotRun => "not_run",
        CopiedProfileStatus::Running => "running",
        CopiedProfileStatus::Passed => "passed",
        CopiedProfileStatus::Failed => "failed",
    };
    contents.push_str(&format!("Copied profile: {copied_profile_status}\n"));
    contents.push_str(&format!(
        "Profile SHA-256: settings={}, radial={}, actions={}\n",
        report.profile.settings_sha256, report.profile.radial_sha256, report.profile.actions_sha256
    ));
    if let Some(copied) = &report.copied_profile {
        contents.push_str(&format!(
            "Copied profile inventory: files={}, bytes={}, source_unchanged={}, source_tree_sha256={}, copied_initial_tree_sha256={}, source_tree_sha256_after={:?}\n",
            copied.copied_file_count,
            copied.copied_total_bytes,
            copied.source_unchanged,
            copied.source_tree_sha256_before,
            copied.copied_initial_tree_sha256,
            copied.source_tree_sha256_after
        ));
        contents.push_str(&format!(
            "Copied profile critical SHA-256: source_settings={}, source_radial={}, source_actions={:?}, initial_settings={}, initial_radial={}, initial_actions={:?}, launch_settings={}, launch_radial={}, launch_actions={}\n",
            copied.source_settings_sha256,
            copied.source_radial_sha256,
            copied.source_actions_sha256,
            copied.copied_initial_settings_sha256,
            copied.copied_initial_radial_sha256,
            copied.copied_initial_actions_sha256,
            copied.launch_settings_sha256,
            copied.launch_radial_sha256,
            copied.launch_actions_sha256
        ));
    }
    if let Some(private) = &report.private_artifacts {
        let status = match private.status {
            private_artifacts::PrivateArtifactStatus::NotRun => "not_run",
            private_artifacts::PrivateArtifactStatus::EphemeralValidated => "ephemeral_validated",
            private_artifacts::PrivateArtifactStatus::Retained => "retained",
            private_artifacts::PrivateArtifactStatus::Failed => "failed",
        };
        contents.push_str(&format!(
            "Private diagnostic evidence: status={status}, id={:?}, files={}, bytes={}, manifest_sha256={:?}\n",
            private.artifact_id,
            private.file_count,
            private.total_bytes,
            private.manifest_sha256
        ));
    }
    if let Some(receipt) = &report.report_overflow {
        contents.push_str(&format!(
            "Report overflow: omitted_case_evidence={}, omitted_artifact_references={}, affected_case_ids={:?}, reason={}\n",
            receipt.omitted_case_evidence,
            receipt.omitted_artifact_references,
            receipt.affected_case_ids,
            receipt.reason
        ));
    }
    for case in &report.cases {
        contents.push_str(&format!(
            "{}: {:?}{} elapsed_ms={} — {}\n",
            case.id,
            case.status,
            case.failure_stage
                .map(|stage| format!(" ({stage:?})"))
                .unwrap_or_default(),
            case.elapsed_ms,
            bounded_text(&case.observed, MAX_RESULT_BYTES)
        ));
    }
    contents.push_str(&format!(
        "Cleanup: child_closed_normally={}, child_owned_windows_closed={}, profile_removed={}, foreground_restore_captured={}, foreground_restore_attempted={}, foreground_restored={}, cursor_restored={}, input_desktop_released={}\n",
        report.cleanup.child_closed_normally,
        report.cleanup.child_owned_windows_closed,
        report.cleanup.profile_removed,
        report.cleanup.foreground_restore_captured,
        report.cleanup.foreground_restore_attempted,
        report.cleanup.foreground_restored,
        report.cleanup.cursor_restored,
        report.cleanup.input_desktop_released
    ));
    contents
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 32 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn bounded_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

#[cfg(windows)]
fn monitor_inventory() -> Vec<MonitorIdentity> {
    screenshots::Screen::all()
        .unwrap_or_default()
        .into_iter()
        .take(16)
        .map(|screen| MonitorIdentity {
            id: screen.display_info.id,
            x: screen.display_info.x,
            y: screen.display_info.y,
            width: screen.display_info.width,
            height: screen.display_info.height,
            scale_factor: screen.display_info.scale_factor,
        })
        .collect()
}

#[cfg(windows)]
fn is_reparse_point(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn print_usage() {
    println!(
        "Usage: radial_acceptance [--launcher <source-matched multi_launcher.exe>] --output <new-run-directory> [--suite all|hotkey] [--hotkey f11|shift-alt-win-end] [--profile-copy <profile-directory>] [--source-revision <id>] [--h6-repeat immediate|quiescent|production-only-diagnostic] [--mouse-gestures enabled|disabled-diagnostic] [--keep-profile-on-failure]\n       radial_acceptance [--candidate <multi_launcher.exe>] --report <new-report.json> [--profile-copy <profile-directory>]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h01_evidence_packet() -> HotkeyCaseEvidence {
        let stream = HotkeyCandidateStream::MainCandidate;
        let group = 1;
        let invocation = Some(5);
        let revision = Some(7);
        let request_id = Some(9);
        let mut events = vec![
            HotkeyCandidateEventEvidence {
                stream,
                input_group_id: group,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                event_ordinal: 1,
                elapsed_ms: 10,
                kind: HotkeyTraceEventKind::PrimaryPress,
                invocation_id: invocation,
                visibility_revision: None,
                request_id: None,
                visible: None,
                minimized: None,
                bounds: None,
                hwnd: None,
                process_id: None,
                command: None,
                visibility_source: None,
                modifiers_match: Some(true),
                provenance: Some(HotkeyInputProvenance::ExternalInjected),
                terminal: None,
                activation_edge: None,
                focus_intent: None,
                radial_action_stage: None,
            },
            HotkeyCandidateEventEvidence {
                stream,
                input_group_id: group,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                event_ordinal: 2,
                elapsed_ms: 20,
                kind: HotkeyTraceEventKind::PrimaryRelease,
                invocation_id: invocation,
                visibility_revision: None,
                request_id: None,
                visible: None,
                minimized: None,
                bounds: None,
                hwnd: None,
                process_id: None,
                command: None,
                visibility_source: None,
                modifiers_match: Some(false),
                provenance: Some(HotkeyInputProvenance::ExternalInjected),
                terminal: None,
                activation_edge: None,
                focus_intent: None,
                radial_action_stage: None,
            },
            HotkeyCandidateEventEvidence {
                stream,
                input_group_id: group,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                event_ordinal: 3,
                elapsed_ms: 22,
                kind: HotkeyTraceEventKind::ShortTap,
                invocation_id: invocation,
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
                terminal: Some(true),
                activation_edge: None,
                focus_intent: None,
                radial_action_stage: None,
            },
            HotkeyCandidateEventEvidence {
                stream,
                input_group_id: group,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                event_ordinal: 4,
                elapsed_ms: 24,
                kind: HotkeyTraceEventKind::VisibilityIntent,
                invocation_id: invocation,
                visibility_revision: revision,
                request_id: None,
                visible: Some(true),
                minimized: None,
                bounds: None,
                hwnd: None,
                process_id: None,
                command: None,
                visibility_source: Some(HotkeyVisibilitySource::ToggleBatch),
                modifiers_match: None,
                provenance: None,
                terminal: None,
                activation_edge: None,
                focus_intent: None,
                radial_action_stage: None,
            },
            HotkeyCandidateEventEvidence {
                stream,
                input_group_id: group,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                event_ordinal: 5,
                elapsed_ms: 26,
                kind: HotkeyTraceEventKind::RootCommand,
                invocation_id: invocation,
                visibility_revision: revision,
                request_id,
                visible: None,
                minimized: None,
                bounds: None,
                hwnd: None,
                process_id: None,
                command: Some(HotkeyRootCommand::Focus),
                visibility_source: None,
                modifiers_match: None,
                provenance: None,
                terminal: None,
                activation_edge: None,
                focus_intent: None,
                radial_action_stage: None,
            },
            HotkeyCandidateEventEvidence {
                stream,
                input_group_id: group,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                event_ordinal: 6,
                elapsed_ms: 32,
                kind: HotkeyTraceEventKind::NativeWindowSnapshot,
                invocation_id: invocation,
                visibility_revision: revision,
                request_id,
                visible: Some(true),
                minimized: Some(false),
                bounds: Some([100, 100, 900, 700]),
                hwnd: Some(1001),
                process_id: Some(202),
                command: None,
                visibility_source: None,
                modifiers_match: None,
                provenance: None,
                terminal: None,
                activation_edge: None,
                focus_intent: None,
                radial_action_stage: None,
            },
        ];
        events.sort_by_key(|event| event.event_ordinal);
        HotkeyCaseEvidence {
            schema_version: 4,
            case_id: "H01".into(),
            expected_state: HotkeyExpectedState::HiddenRootWakeAndFocus,
            runner_clock: "runner_monotonic_relative_us".into(),
            runner_edges: vec![
                HotkeyRunnerEdgeEvidence {
                    runner_relative_us: 500_000,
                    input_group_id: group,
                    stream,
                    purpose: HotkeyRunnerInputPurpose::LauncherChord,
                    virtual_key: 0x7A,
                    transition: HotkeyEdgeTransition::Press,
                    injected: true,
                    runner_cookie_matched: true,
                },
                HotkeyRunnerEdgeEvidence {
                    runner_relative_us: 500_025,
                    input_group_id: group,
                    stream,
                    purpose: HotkeyRunnerInputPurpose::LauncherChord,
                    virtual_key: 0x7A,
                    transition: HotkeyEdgeTransition::Release,
                    injected: true,
                    runner_cookie_matched: true,
                },
            ],
            candidate_events: events,
            gestures: vec![HotkeyGestureEvidence {
                stream,
                input_group_id: group,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                invocation_id: 5,
                release_elapsed_ms: 20,
                release_modifiers_match: false,
                short_tap_elapsed_ms: Some(22),
                decision: HotkeyDecisionProof::Applied {
                    visibility_revision: 7,
                    intent_elapsed_ms: 24,
                    visible: true,
                    release_to_intent_ms: 4,
                    root_commands: vec![HotkeyRootCommandSpan {
                        visibility_revision: 7,
                        invocation_id: Some(5),
                        request_id: 9,
                        command_elapsed_ms: 26,
                        command: HotkeyRootCommand::Focus,
                        release_to_root_command_ms: Some(6),
                        command_event_ordinal: 5,
                        observed_snapshot_event_ordinal: Some(6),
                        command_to_observed_ms: Some(6),
                        observed_presentation: Some(HotkeyObservedPresentation {
                            elapsed_ms: 32,
                            command_to_observed_ms: 6,
                            visible: true,
                            minimized: false,
                            bounds: [100, 100, 900, 700],
                        }),
                    }],
                },
            }],
            standalone_decisions: Vec::new(),
            follow_on_restorations: Vec::new(),
            root_identities: vec![HotkeyRootIdentityEvidence {
                stream,
                hwnd: 1001,
                process_id: 202,
            }],
            physical_displays: vec![[0, 0, 1920, 1080]],
            candidate_event_count: 6,
            candidate_trace_overflow: false,
            capture_segment_overflow: false,
            runner_edge_overflow: false,
            gesture_overflow: false,
        }
    }

    fn h16_legacy_fallback_group_packet() -> HotkeyCaseEvidence {
        let stream = HotkeyCandidateStream::LegacyFallbackCandidate;
        let group = 6;
        let purpose = HotkeyRunnerInputPurpose::LauncherChord;
        HotkeyCaseEvidence {
            schema_version: 4,
            case_id: "H16".into(),
            expected_state: HotkeyExpectedState::DirectAndLegacyTriggersPreserved,
            runner_clock: "runner_monotonic_relative_us".into(),
            runner_edges: vec![
                HotkeyRunnerEdgeEvidence {
                    runner_relative_us: 100,
                    input_group_id: group,
                    stream,
                    purpose,
                    virtual_key: 0x7A,
                    transition: HotkeyEdgeTransition::Press,
                    injected: true,
                    runner_cookie_matched: true,
                },
                HotkeyRunnerEdgeEvidence {
                    runner_relative_us: 200,
                    input_group_id: group,
                    stream,
                    purpose,
                    virtual_key: 0x7A,
                    transition: HotkeyEdgeTransition::Release,
                    injected: true,
                    runner_cookie_matched: true,
                },
            ],
            candidate_events: Vec::new(),
            gestures: vec![HotkeyGestureEvidence {
                stream,
                input_group_id: group,
                input_purpose: purpose,
                invocation_id: 17,
                release_elapsed_ms: 20,
                release_modifiers_match: false,
                short_tap_elapsed_ms: None,
                decision: HotkeyDecisionProof::NotApplicable {
                    reason: HotkeyEvidenceNotApplicable::LegacyTriggerHasNoInvocationReducerId,
                },
            }],
            standalone_decisions: vec![HotkeyStandaloneDecisionEvidence {
                stream,
                input_group_id: group,
                input_purpose: purpose,
                visibility_revision: 8,
                invocation_id: None,
                intent_elapsed_ms: 22,
                visible: false,
                source: HotkeyVisibilitySource::LegacyTrigger,
                root_commands: Vec::new(),
                decision: HotkeyDecisionProof::NotApplicable {
                    reason: HotkeyEvidenceNotApplicable::LegacyTriggerHasNoInvocationReducerId,
                },
            }],
            follow_on_restorations: Vec::new(),
            root_identities: Vec::new(),
            physical_displays: Vec::new(),
            candidate_event_count: 0,
            candidate_trace_overflow: false,
            capture_segment_overflow: false,
            runner_edge_overflow: false,
            gesture_overflow: false,
        }
    }

    fn h04_matrix_evidence_packet() -> HotkeyCaseEvidence {
        let stream = HotkeyCandidateStream::MainCandidate;
        let mut candidate_events = Vec::new();
        let mut gestures = Vec::new();
        let mut runner_edges = Vec::new();
        let mut event_ordinal = 1_u32;
        let mut gesture_index = 0_u64;

        let mut add_group = |group_id: u32,
                             purpose: HotkeyRunnerInputPurpose,
                             taps: usize,
                             initially_visible: bool| {
            let mut visible = initially_visible;
            let mut group_gesture_index = 0_u64;
            for _ in 0..taps {
                visible = !visible;
                let invocation_id = 1_000 + gesture_index;
                let visibility_revision = 2_000 + gesture_index;
                let release_elapsed_ms = gesture_index * 100 + 20;
                let short_tap_elapsed_ms = release_elapsed_ms + 2;
                let intent_elapsed_ms = release_elapsed_ms + 4;
                let bounds = if visible {
                    [100, 100, 900, 700]
                } else {
                    [3_000, 100, 3_800, 700]
                };

                let press = HotkeyCandidateEventEvidence {
                    stream,
                    input_group_id: group_id,
                    input_purpose: purpose,
                    event_ordinal,
                    elapsed_ms: release_elapsed_ms - 10,
                    kind: HotkeyTraceEventKind::PrimaryPress,
                    invocation_id: Some(invocation_id),
                    visibility_revision: None,
                    request_id: None,
                    visible: None,
                    minimized: None,
                    bounds: None,
                    hwnd: None,
                    process_id: None,
                    command: None,
                    visibility_source: None,
                    modifiers_match: Some(true),
                    provenance: Some(HotkeyInputProvenance::ExternalInjected),
                    terminal: None,
                    activation_edge: None,
                    focus_intent: None,
                    radial_action_stage: None,
                };
                event_ordinal += 1;
                let release = HotkeyCandidateEventEvidence {
                    event_ordinal,
                    elapsed_ms: release_elapsed_ms,
                    kind: HotkeyTraceEventKind::PrimaryRelease,
                    modifiers_match: Some(false),
                    ..press.clone()
                };
                event_ordinal += 1;
                let short_tap = HotkeyCandidateEventEvidence {
                    event_ordinal,
                    elapsed_ms: short_tap_elapsed_ms,
                    kind: HotkeyTraceEventKind::ShortTap,
                    modifiers_match: None,
                    provenance: None,
                    terminal: Some(true),
                    ..press.clone()
                };
                event_ordinal += 1;
                let visibility_intent = HotkeyCandidateEventEvidence {
                    event_ordinal,
                    elapsed_ms: intent_elapsed_ms,
                    kind: HotkeyTraceEventKind::VisibilityIntent,
                    visibility_revision: Some(visibility_revision),
                    visible: Some(visible),
                    visibility_source: Some(HotkeyVisibilitySource::ToggleBatch),
                    modifiers_match: None,
                    provenance: None,
                    ..press.clone()
                };
                event_ordinal += 1;
                candidate_events.extend([press.clone(), release, short_tap, visibility_intent]);

                let command_kinds = [
                    if visible {
                        HotkeyRootCommand::Show
                    } else {
                        HotkeyRootCommand::Minimize
                    },
                    HotkeyRootCommand::Position,
                    HotkeyRootCommand::Size,
                    HotkeyRootCommand::Focus,
                    HotkeyRootCommand::ParkingBoundary,
                    HotkeyRootCommand::Position,
                    HotkeyRootCommand::Size,
                    HotkeyRootCommand::Focus,
                ];
                let mut root_commands = Vec::with_capacity(command_kinds.len());
                for (command_index, command) in command_kinds.into_iter().enumerate() {
                    let request_id = 3_000 + gesture_index * 8 + command_index as u64;
                    let command_elapsed_ms = release_elapsed_ms + 6 + command_index as u64 * 2;
                    let observed_elapsed_ms = command_elapsed_ms + 1;
                    let root_command = HotkeyCandidateEventEvidence {
                        event_ordinal,
                        elapsed_ms: command_elapsed_ms,
                        kind: HotkeyTraceEventKind::RootCommand,
                        visibility_revision: Some(visibility_revision),
                        request_id: Some(request_id),
                        command: Some(command),
                        terminal: Some(false),
                        modifiers_match: None,
                        provenance: None,
                        ..press.clone()
                    };
                    event_ordinal += 1;
                    let snapshot = HotkeyCandidateEventEvidence {
                        event_ordinal,
                        elapsed_ms: observed_elapsed_ms,
                        kind: HotkeyTraceEventKind::NativeWindowSnapshot,
                        visibility_revision: Some(visibility_revision),
                        request_id: Some(request_id),
                        visible: Some(true),
                        minimized: Some(false),
                        bounds: Some(bounds),
                        hwnd: Some(1001),
                        process_id: Some(202),
                        terminal: Some(false),
                        command: None,
                        modifiers_match: None,
                        provenance: None,
                        ..press.clone()
                    };
                    event_ordinal += 1;
                    let command_event_ordinal = root_command.event_ordinal;
                    let snapshot_event_ordinal = snapshot.event_ordinal;
                    candidate_events.extend([root_command, snapshot]);
                    root_commands.push(HotkeyRootCommandSpan {
                        visibility_revision,
                        invocation_id: Some(invocation_id),
                        request_id,
                        command_elapsed_ms,
                        command,
                        release_to_root_command_ms: Some(
                            command_elapsed_ms.saturating_sub(release_elapsed_ms),
                        ),
                        command_event_ordinal,
                        observed_snapshot_event_ordinal: Some(snapshot_event_ordinal),
                        command_to_observed_ms: Some(
                            observed_elapsed_ms.saturating_sub(command_elapsed_ms),
                        ),
                        observed_presentation: Some(HotkeyObservedPresentation {
                            elapsed_ms: observed_elapsed_ms,
                            command_to_observed_ms: observed_elapsed_ms
                                .saturating_sub(command_elapsed_ms),
                            visible: true,
                            minimized: false,
                            bounds,
                        }),
                    });
                }

                let per_gesture_edges = configured_chord_edges(AcceptanceHotkey::ShiftAltWinEnd, 1);
                let runner_start_us =
                    u64::from(group_id) * 10_000_000 + group_gesture_index * 130_000;
                let chord_edge_offsets_us =
                    [0, 5_000, 10_000, 15_000, 40_000, 45_000, 50_000, 55_000];
                runner_edges.extend(per_gesture_edges.into_iter().enumerate().map(
                    |(edge_index, (virtual_key, down))| HotkeyRunnerEdgeEvidence {
                        runner_relative_us: runner_start_us + chord_edge_offsets_us[edge_index],
                        input_group_id: group_id,
                        stream,
                        purpose,
                        virtual_key,
                        transition: if down {
                            HotkeyEdgeTransition::Press
                        } else {
                            HotkeyEdgeTransition::Release
                        },
                        injected: true,
                        runner_cookie_matched: true,
                    },
                ));

                gestures.push(HotkeyGestureEvidence {
                    stream,
                    input_group_id: group_id,
                    input_purpose: purpose,
                    invocation_id,
                    release_elapsed_ms,
                    release_modifiers_match: false,
                    short_tap_elapsed_ms: Some(short_tap_elapsed_ms),
                    decision: HotkeyDecisionProof::Applied {
                        visibility_revision,
                        intent_elapsed_ms,
                        visible,
                        release_to_intent_ms: intent_elapsed_ms.saturating_sub(release_elapsed_ms),
                        root_commands,
                    },
                });
                gesture_index += 1;
                group_gesture_index += 1;
            }
        };

        for (index, taps) in [1, 2, 5, 10, 25, 1, 2, 5, 10, 25].into_iter().enumerate() {
            add_group(
                index as u32 + 1,
                HotkeyRunnerInputPurpose::MatrixBurst,
                taps,
                index >= 5,
            );
        }
        for group_index in 0..3 {
            add_group(
                11 + group_index,
                HotkeyRunnerInputPurpose::ReadableCadence,
                1,
                group_index == 1,
            );
        }

        HotkeyCaseEvidence {
            schema_version: 4,
            case_id: "H04".into(),
            expected_state: HotkeyExpectedState::HiddenAndVisibleBurstParity,
            runner_clock: "runner_monotonic_relative_us".into(),
            runner_edges,
            candidate_event_count: candidate_events.len(),
            candidate_events,
            gestures,
            standalone_decisions: Vec::new(),
            follow_on_restorations: Vec::new(),
            root_identities: vec![HotkeyRootIdentityEvidence {
                stream,
                hwnd: 1001,
                process_id: 202,
            }],
            physical_displays: vec![[0, 0, 1920, 1080]],
            candidate_trace_overflow: false,
            capture_segment_overflow: false,
            runner_edge_overflow: false,
            gesture_overflow: false,
        }
    }

    #[test]
    fn h04_packet_readback_allows_only_balanced_foreign_pairs_in_released_gaps() {
        fn add_foreign(
            packet: &mut HotkeyCaseEvidence,
            runner_relative_us: u64,
            transition: HotkeyEdgeTransition,
        ) {
            packet.runner_edges.push(HotkeyRunnerEdgeEvidence {
                runner_relative_us,
                input_group_id: 2,
                stream: HotkeyCandidateStream::MainCandidate,
                purpose: HotkeyRunnerInputPurpose::MatrixBurst,
                virtual_key: 0xA4,
                transition,
                injected: true,
                runner_cookie_matched: false,
            });
            packet
                .runner_edges
                .sort_by_key(|edge| edge.runner_relative_us);
        }
        let validate = |packet: &HotkeyCaseEvidence| {
            validate_hotkey_evidence_packet_with_context(
                packet,
                AcceptanceHotkey::ShiftAltWinEnd,
                350,
            )
        };

        let mut balanced_gap_pair = h04_matrix_evidence_packet();
        add_foreign(
            &mut balanced_gap_pair,
            20_080_000,
            HotkeyEdgeTransition::Press,
        );
        add_foreign(
            &mut balanced_gap_pair,
            20_090_000,
            HotkeyEdgeTransition::Release,
        );
        validate(&balanced_gap_pair).unwrap();

        let mut moved_inside = h04_matrix_evidence_packet();
        add_foreign(&mut moved_inside, 20_020_000, HotkeyEdgeTransition::Press);
        add_foreign(&mut moved_inside, 20_025_000, HotkeyEdgeTransition::Release);
        assert!(validate(&moved_inside).is_err());

        let mut held_across_boundary = h04_matrix_evidence_packet();
        add_foreign(
            &mut held_across_boundary,
            20_080_000,
            HotkeyEdgeTransition::Press,
        );
        add_foreign(
            &mut held_across_boundary,
            20_150_000,
            HotkeyEdgeTransition::Release,
        );
        assert!(validate(&held_across_boundary).is_err());

        let mut orphan_release = h04_matrix_evidence_packet();
        add_foreign(
            &mut orphan_release,
            20_080_000,
            HotkeyEdgeTransition::Release,
        );
        assert!(validate(&orphan_release).is_err());

        let mut unbalanced_gap_down = h04_matrix_evidence_packet();
        add_foreign(
            &mut unbalanced_gap_down,
            20_080_000,
            HotkeyEdgeTransition::Press,
        );
        assert!(validate(&unbalanced_gap_down).is_err());
    }

    fn h07_superseded_evidence_packet() -> HotkeyCaseEvidence {
        let stream = HotkeyCandidateStream::MainCandidate;
        let group = 1;
        let mut events = Vec::new();
        let mut gestures = Vec::new();
        let mut runner_edges = Vec::new();
        for (index, (invocation, revision, visible, release)) in [
            (5_u64, 7_u64, false, 20_u64),
            (6, 8, true, 40),
            (7, 9, false, 60),
        ]
        .into_iter()
        .enumerate()
        {
            let ordinal = (index * 6 + 1) as u32;
            let intent = release + 4;
            let short_tap = release + 2;
            events.extend([
                HotkeyCandidateEventEvidence {
                    stream,
                    input_group_id: group,
                    input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                    event_ordinal: ordinal,
                    elapsed_ms: release - 10,
                    kind: HotkeyTraceEventKind::PrimaryPress,
                    invocation_id: Some(invocation),
                    visibility_revision: None,
                    request_id: None,
                    visible: None,
                    minimized: None,
                    bounds: None,
                    hwnd: None,
                    process_id: None,
                    command: None,
                    visibility_source: None,
                    modifiers_match: Some(true),
                    provenance: Some(HotkeyInputProvenance::ExternalInjected),
                    terminal: None,
                    activation_edge: None,
                    focus_intent: None,
                    radial_action_stage: None,
                },
                HotkeyCandidateEventEvidence {
                    stream,
                    input_group_id: group,
                    input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                    event_ordinal: ordinal + 1,
                    elapsed_ms: release,
                    kind: HotkeyTraceEventKind::PrimaryRelease,
                    invocation_id: Some(invocation),
                    visibility_revision: None,
                    request_id: None,
                    visible: None,
                    minimized: None,
                    bounds: None,
                    hwnd: None,
                    process_id: None,
                    command: None,
                    visibility_source: None,
                    modifiers_match: Some(false),
                    provenance: Some(HotkeyInputProvenance::ExternalInjected),
                    terminal: None,
                    activation_edge: None,
                    focus_intent: None,
                    radial_action_stage: None,
                },
                HotkeyCandidateEventEvidence {
                    stream,
                    input_group_id: group,
                    input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                    event_ordinal: ordinal + 2,
                    elapsed_ms: short_tap,
                    kind: HotkeyTraceEventKind::ShortTap,
                    invocation_id: Some(invocation),
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
                    terminal: Some(true),
                    activation_edge: None,
                    focus_intent: None,
                    radial_action_stage: None,
                },
                HotkeyCandidateEventEvidence {
                    stream,
                    input_group_id: group,
                    input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                    event_ordinal: ordinal + 3,
                    elapsed_ms: intent,
                    kind: HotkeyTraceEventKind::VisibilityIntent,
                    invocation_id: Some(invocation),
                    visibility_revision: Some(revision),
                    request_id: None,
                    visible: Some(visible),
                    minimized: None,
                    bounds: None,
                    hwnd: None,
                    process_id: None,
                    command: None,
                    visibility_source: Some(HotkeyVisibilitySource::ToggleBatch),
                    modifiers_match: None,
                    provenance: None,
                    terminal: None,
                    activation_edge: None,
                    focus_intent: None,
                    radial_action_stage: None,
                },
            ]);
            if index > 0 {
                let request_id = 20 + index as u64;
                let command_at = intent + 2;
                let observed_at = command_at + 4;
                let command = if visible {
                    HotkeyRootCommand::Show
                } else {
                    HotkeyRootCommand::Minimize
                };
                events.extend([
                    HotkeyCandidateEventEvidence {
                        stream,
                        input_group_id: group,
                        input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                        event_ordinal: ordinal + 4,
                        elapsed_ms: command_at,
                        kind: HotkeyTraceEventKind::RootCommand,
                        invocation_id: Some(invocation),
                        visibility_revision: Some(revision),
                        request_id: Some(request_id),
                        visible: None,
                        minimized: None,
                        bounds: None,
                        hwnd: None,
                        process_id: None,
                        command: Some(command),
                        visibility_source: None,
                        modifiers_match: None,
                        provenance: None,
                        terminal: None,
                        activation_edge: None,
                        focus_intent: None,
                        radial_action_stage: None,
                    },
                    HotkeyCandidateEventEvidence {
                        stream,
                        input_group_id: group,
                        input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                        event_ordinal: ordinal + 5,
                        elapsed_ms: observed_at,
                        kind: HotkeyTraceEventKind::NativeWindowSnapshot,
                        invocation_id: Some(invocation),
                        visibility_revision: Some(revision),
                        request_id: Some(request_id),
                        visible: Some(visible),
                        minimized: Some(false),
                        bounds: Some([100, 100, 900, 700]),
                        hwnd: Some(1001),
                        process_id: Some(202),
                        command: None,
                        visibility_source: None,
                        modifiers_match: None,
                        provenance: None,
                        terminal: None,
                        activation_edge: None,
                        focus_intent: None,
                        radial_action_stage: None,
                    },
                ]);
            }
            let decision = if index == 0 {
                HotkeyDecisionProof::Superseded {
                    visibility_revision: revision,
                    by_revision: 8,
                    intent_elapsed_ms: intent,
                    visible,
                    release_to_intent_ms: 4,
                }
            } else {
                let request_id = 20 + index as u64;
                let command_at = intent + 2;
                let observed_at = command_at + 4;
                HotkeyDecisionProof::Applied {
                    visibility_revision: revision,
                    intent_elapsed_ms: intent,
                    visible,
                    release_to_intent_ms: 4,
                    root_commands: vec![HotkeyRootCommandSpan {
                        visibility_revision: revision,
                        invocation_id: Some(invocation),
                        request_id,
                        command_elapsed_ms: command_at,
                        command: if visible {
                            HotkeyRootCommand::Show
                        } else {
                            HotkeyRootCommand::Minimize
                        },
                        release_to_root_command_ms: Some(command_at - release),
                        command_event_ordinal: ordinal + 4,
                        observed_snapshot_event_ordinal: Some(ordinal + 5),
                        command_to_observed_ms: Some(4),
                        observed_presentation: Some(HotkeyObservedPresentation {
                            elapsed_ms: observed_at,
                            command_to_observed_ms: 4,
                            visible,
                            minimized: false,
                            bounds: [100, 100, 900, 700],
                        }),
                    }],
                }
            };
            gestures.push(HotkeyGestureEvidence {
                stream,
                input_group_id: group,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                invocation_id: invocation,
                release_elapsed_ms: release,
                release_modifiers_match: false,
                short_tap_elapsed_ms: Some(short_tap),
                decision,
            });
            runner_edges.extend([
                HotkeyRunnerEdgeEvidence {
                    runner_relative_us: (index * 50) as u64,
                    input_group_id: group,
                    stream,
                    purpose: HotkeyRunnerInputPurpose::LauncherChord,
                    virtual_key: 0x7A,
                    transition: HotkeyEdgeTransition::Press,
                    injected: true,
                    runner_cookie_matched: true,
                },
                HotkeyRunnerEdgeEvidence {
                    runner_relative_us: (index * 50 + 25) as u64,
                    input_group_id: group,
                    stream,
                    purpose: HotkeyRunnerInputPurpose::LauncherChord,
                    virtual_key: 0x7A,
                    transition: HotkeyEdgeTransition::Release,
                    injected: true,
                    runner_cookie_matched: true,
                },
            ]);
        }
        HotkeyCaseEvidence {
            schema_version: 4,
            case_id: "H07".into(),
            expected_state: HotkeyExpectedState::TapDismissesThenNextGestureWorks,
            runner_clock: "runner_monotonic_relative_us".into(),
            runner_edges,
            candidate_events: events,
            gestures,
            standalone_decisions: Vec::new(),
            follow_on_restorations: Vec::new(),
            root_identities: vec![HotkeyRootIdentityEvidence {
                stream,
                hwnd: 1001,
                process_id: 202,
            }],
            physical_displays: vec![[0, 0, 1920, 1080]],
            candidate_event_count: 16,
            candidate_trace_overflow: false,
            capture_segment_overflow: false,
            runner_edge_overflow: false,
            gesture_overflow: false,
        }
    }

    fn append_screen_draw_restore(
        packet: &mut HotkeyCaseEvidence,
        invocation_id: Option<u64>,
        focus_intent: HotkeyRootFocusIntent,
    ) {
        let stream = HotkeyCandidateStream::MainCandidate;
        let parent_revision = invocation_id.map(|_| 7);
        let restore_revision = 8;
        let request_id = 20;
        let activation_request_id = 30;
        let intent_elapsed_ms = 34;
        let root_command_elapsed_ms = 36;
        let activation_requested_elapsed_ms = 37;
        let snapshot_elapsed_ms = 40;
        let activation_completed_elapsed_ms = 41;
        packet.candidate_events.extend([
            HotkeyCandidateEventEvidence {
                stream,
                input_group_id: 1,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                event_ordinal: 7,
                elapsed_ms: intent_elapsed_ms,
                kind: HotkeyTraceEventKind::VisibilityIntent,
                invocation_id,
                visibility_revision: Some(restore_revision),
                request_id: None,
                visible: Some(true),
                minimized: None,
                bounds: None,
                hwnd: None,
                process_id: None,
                command: None,
                visibility_source: Some(HotkeyVisibilitySource::ScreenDrawRestore),
                modifiers_match: None,
                provenance: None,
                terminal: None,
                activation_edge: None,
                focus_intent: None,
                radial_action_stage: None,
            },
            HotkeyCandidateEventEvidence {
                stream,
                input_group_id: 1,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                event_ordinal: 8,
                elapsed_ms: intent_elapsed_ms,
                kind: HotkeyTraceEventKind::ScreenDrawRestoreFocusIntent,
                invocation_id,
                visibility_revision: Some(restore_revision),
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
                focus_intent: Some(focus_intent),
                radial_action_stage: None,
            },
            HotkeyCandidateEventEvidence {
                stream,
                input_group_id: 1,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                event_ordinal: 9,
                elapsed_ms: root_command_elapsed_ms,
                kind: HotkeyTraceEventKind::RootCommand,
                invocation_id,
                visibility_revision: Some(restore_revision),
                request_id: Some(request_id),
                visible: None,
                minimized: None,
                bounds: None,
                hwnd: None,
                process_id: None,
                command: Some(HotkeyRootCommand::Show),
                visibility_source: None,
                modifiers_match: None,
                provenance: None,
                terminal: None,
                activation_edge: None,
                focus_intent: None,
                radial_action_stage: None,
            },
            HotkeyCandidateEventEvidence {
                stream,
                input_group_id: 1,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                event_ordinal: 10,
                elapsed_ms: activation_requested_elapsed_ms,
                kind: HotkeyTraceEventKind::NativeActivation,
                invocation_id,
                visibility_revision: Some(restore_revision),
                request_id: Some(activation_request_id),
                visible: None,
                minimized: None,
                bounds: None,
                hwnd: Some(1001),
                process_id: None,
                command: None,
                visibility_source: None,
                modifiers_match: None,
                provenance: None,
                terminal: Some(false),
                activation_edge: Some(HotkeyActivationEdge::RestoreRequested),
                focus_intent: None,
                radial_action_stage: None,
            },
            HotkeyCandidateEventEvidence {
                stream,
                input_group_id: 1,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                event_ordinal: 11,
                elapsed_ms: snapshot_elapsed_ms,
                kind: HotkeyTraceEventKind::NativeWindowSnapshot,
                invocation_id,
                visibility_revision: Some(restore_revision),
                request_id: Some(request_id),
                visible: Some(true),
                minimized: Some(false),
                bounds: Some([100, 100, 900, 700]),
                hwnd: Some(1001),
                process_id: Some(202),
                command: None,
                visibility_source: None,
                modifiers_match: None,
                provenance: None,
                terminal: Some(true),
                activation_edge: None,
                focus_intent: None,
                radial_action_stage: None,
            },
            HotkeyCandidateEventEvidence {
                stream,
                input_group_id: 1,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                event_ordinal: 12,
                elapsed_ms: activation_completed_elapsed_ms,
                kind: HotkeyTraceEventKind::NativeActivation,
                invocation_id,
                visibility_revision: Some(restore_revision),
                request_id: Some(activation_request_id),
                visible: None,
                minimized: None,
                bounds: None,
                hwnd: Some(1001),
                process_id: None,
                command: None,
                visibility_source: None,
                modifiers_match: None,
                provenance: None,
                terminal: Some(true),
                activation_edge: Some(HotkeyActivationEdge::RestoreCompleted),
                focus_intent: None,
                radial_action_stage: None,
            },
        ]);
        if focus_intent == HotkeyRootFocusIntent::PreserveForeground {
            packet.candidate_events.retain(|event| {
                !(event.kind == HotkeyTraceEventKind::NativeActivation
                    && event.visibility_revision == Some(restore_revision))
            });
        }
        packet
            .follow_on_restorations
            .push(HotkeyFollowOnRestoreEvidence {
                stream,
                input_group_id: 1,
                parent_visibility_revision: parent_revision,
                visibility_revision: restore_revision,
                invocation_id,
                intent_elapsed_ms,
                visible: true,
                focus_intent,
                root_commands: vec![HotkeyRootCommandSpan {
                    visibility_revision: restore_revision,
                    invocation_id,
                    request_id,
                    command_elapsed_ms: root_command_elapsed_ms,
                    command: HotkeyRootCommand::Show,
                    release_to_root_command_ms: None,
                    command_event_ordinal: 9,
                    observed_snapshot_event_ordinal: Some(11),
                    command_to_observed_ms: Some(snapshot_elapsed_ms - root_command_elapsed_ms),
                    observed_presentation: Some(HotkeyObservedPresentation {
                        elapsed_ms: snapshot_elapsed_ms,
                        command_to_observed_ms: snapshot_elapsed_ms - root_command_elapsed_ms,
                        visible: true,
                        minimized: false,
                        bounds: [100, 100, 900, 700],
                    }),
                }],
                native_activation: (focus_intent == HotkeyRootFocusIntent::ActivateRoot).then_some(
                    HotkeyNativeActivationSpan {
                        request_id: activation_request_id,
                        hwnd: 1001,
                        requested_elapsed_ms: activation_requested_elapsed_ms,
                        terminal_elapsed_ms: Some(activation_completed_elapsed_ms),
                        terminal_edge: Some(HotkeyActivationEdge::RestoreCompleted),
                    },
                ),
            });
        packet
            .candidate_events
            .sort_by_key(|event| event.event_ordinal);
        packet.candidate_event_count = packet.candidate_events.len();
    }

    #[test]
    fn h_case_expected_states_cover_each_mandatory_case() {
        for case_id in [
            "H01", "H02", "H04", "H06", "H07", "H08", "H09", "H10", "H11", "H12", "H16", "H17",
            "H18",
        ] {
            assert!(hotkey_expected_state(case_id).is_some(), "{case_id}");
        }
        assert!(hotkey_expected_state("CLEANUP").is_none());
        assert!(hotkey_expected_state("R0").is_none());
    }

    #[test]
    fn typed_hotkey_evidence_round_trips_and_keeps_clocks_separate() {
        let mut packet = h01_evidence_packet();
        validate_hotkey_evidence_packet(&packet).unwrap();
        let encoded = serde_json::to_vec(&packet).unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        let span = &wire["gestures"][0]["d"]["c"][0];
        assert_eq!(span["ce"], 5);
        assert_eq!(span["se"], 6);
        assert_eq!(span["co"], 6);
        assert!(span.get("request_id").is_none());
        assert!(span.get("visibility_revision").is_none());
        assert!(span.get("observed_presentation").is_none());
        let decoded: HotkeyCaseEvidence = serde_json::from_slice(&encoded).unwrap();
        validate_hotkey_evidence_packet(&decoded).unwrap();
        let HotkeyDecisionProof::Applied { root_commands, .. } = &decoded.gestures[0].decision
        else {
            panic!("expected applied decision after wire hydration");
        };
        assert_eq!(root_commands[0].visibility_revision, 7);
        assert_eq!(root_commands[0].invocation_id, Some(5));
        assert_eq!(root_commands[0].request_id, 9);
        assert_eq!(root_commands[0].command, HotkeyRootCommand::Focus);
        assert_eq!(
            root_commands[0]
                .observed_presentation
                .as_ref()
                .map(|snapshot| snapshot.bounds),
            Some([100, 100, 900, 700])
        );

        // Candidate elapsed_ms and runner-relative microseconds intentionally
        // use unrelated origins; changing the runner origin cannot alter any
        // candidate latency span.
        packet.runner_edges[0].runner_relative_us = 1_500_000;
        packet.runner_edges[1].runner_relative_us = 1_525_000;
        validate_hotkey_evidence_packet(&packet).unwrap();
        assert_eq!(
            match &packet.gestures[0].decision {
                HotkeyDecisionProof::Applied {
                    release_to_intent_ms,
                    ..
                } => *release_to_intent_ms,
                _ => panic!("expected applied proof"),
            },
            4
        );
    }

    #[test]
    fn h04_exact_chord_packet_preserves_all_proofs_within_case_and_report_caps() {
        let packet = h04_matrix_evidence_packet();
        assert_eq!(packet.schema_version, 4);
        assert_eq!(packet.gestures.len(), 89);
        assert_eq!(packet.candidate_events.len(), 89 * 20);
        assert_eq!(all_root_command_spans(&packet).len(), 89 * 8);
        assert_eq!(
            packet
                .candidate_events
                .iter()
                .filter(|event| event.kind == HotkeyTraceEventKind::RootCommand)
                .count(),
            89 * 8
        );
        assert_eq!(
            packet
                .candidate_events
                .iter()
                .filter(|event| event.kind == HotkeyTraceEventKind::NativeWindowSnapshot)
                .count(),
            89 * 8
        );
        assert_eq!(packet.runner_edges.len(), 89 * 8);
        assert_eq!(
            packet
                .gestures
                .iter()
                .map(|gesture| gesture.invocation_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            89,
            "every tap decision retains its own invocation"
        );

        validate_hotkey_evidence_packet_with_context(
            &packet,
            AcceptanceHotkey::ShiftAltWinEnd,
            350,
        )
        .unwrap();
        let packet_bytes = serde_json::to_vec(&packet).unwrap();
        assert!(packet_bytes.len() <= MAX_HOTKEY_CASE_EVIDENCE_BYTES);
        let encoded_value: serde_json::Value = serde_json::from_slice(&packet_bytes).unwrap();
        assert!(encoded_value["runner_edges"][0].get("t").is_some());
        assert!(
            encoded_value["runner_edges"][0]
                .get("runner_relative_us")
                .is_none()
        );
        assert_eq!(encoded_value["runner_edges"][0]["p"], "burst");
        assert_eq!(encoded_value["candidate_events"][0]["s"], "main");
        assert_eq!(encoded_value["candidate_events"][0]["g"], 1);
        assert_eq!(encoded_value["candidate_events"][0]["e"][0][0], 0);
        assert!(encoded_value["candidate_events"][0]["e"][0].is_array());
        assert_eq!(encoded_value["gestures"][0]["d"]["d"], "applied");
        let serialized_span = &encoded_value["gestures"][1]["d"]["c"][0];
        assert!(serialized_span.get("ce").is_some());
        assert!(serialized_span.get("se").is_some());
        assert!(serialized_span.get("observed_presentation").is_none());
        let round_trip: HotkeyCaseEvidence = serde_json::from_slice(&packet_bytes).unwrap();
        assert_eq!(round_trip.runner_edges.len(), 89 * 8);
        assert_eq!(round_trip.gestures.len(), 89);
        assert_eq!(round_trip.candidate_events, packet.candidate_events);
        assert_eq!(round_trip.candidate_events.len(), 89 * 20);
        validate_hotkey_evidence_packet_with_context(
            &round_trip,
            AcceptanceHotkey::ShiftAltWinEnd,
            350,
        )
        .unwrap();

        let mut report = acceptance_report("native_windows");
        report.suite = AcceptanceSuite::Hotkey;
        report.hotkey = AcceptanceHotkey::ShiftAltWinEnd;
        report.profile.configured_hotkey = AcceptanceHotkey::ShiftAltWinEnd.as_str();
        report.profile.hold_threshold_ms = 350;
        report.outcome = "failed";
        for id in HOTKEY_CASE_IDS.iter().copied().filter(|id| *id != "R0") {
            let passed = matches!(id, "H04" | "CLEANUP");
            report.cases.push(AcceptanceCaseResult {
                id: id.into(),
                status: if passed {
                    CaseStatus::Passed
                } else {
                    CaseStatus::Failed
                },
                elapsed_ms: 250,
                expected: format!("{id} acceptance behavior"),
                observed: if id == "H04" {
                    hotkey_matrix_case_evidence(report.hotkey)
                } else if passed {
                    "recorded with complete evidence".into()
                } else {
                    "case failed in this packet-size fixture".into()
                },
                failure_stage: (!passed).then_some(FailureStage::GestureDecision),
                artifacts: Vec::new(),
            });
        }
        report.cases.push(AcceptanceCaseResult {
            id: "R0".into(),
            status: CaseStatus::Failed,
            elapsed_ms: 1,
            expected: "all required cases and evidence validate".into(),
            observed: "non-H04 fixture cases are intentionally failed".into(),
            failure_stage: Some(FailureStage::Environment),
            artifacts: Vec::new(),
        });
        report.hotkey_evidence.push(round_trip);
        validate_hotkey_evidence_report(&report).unwrap();
        assert!(report.cases.iter().any(|case| case.id == "R0"));
        let (json_bytes, text_bytes) = report_serialized_sizes(&report).unwrap();
        println!(
            "H04 packet/report sizes: packet={} JSON={} text={}",
            packet_bytes.len(),
            json_bytes,
            text_bytes
        );
        assert!(
            json_bytes <= MAX_JSON_REPORT_BYTES,
            "{json_bytes} JSON bytes"
        );
        assert!(
            text_bytes <= MAX_TEXT_REPORT_BYTES,
            "{text_bytes} text bytes"
        );
    }

    #[test]
    fn ordinal_referenced_command_and_snapshot_spans_reject_mutated_references() {
        let packet = h01_evidence_packet();
        let original = serde_json::to_value(&packet).unwrap();

        let mut dangling_command = original.clone();
        dangling_command["gestures"][0]["d"]["c"][0]["ce"] = serde_json::json!(999);
        let decoded: HotkeyCaseEvidence = serde_json::from_value(dangling_command).unwrap();
        assert!(validate_hotkey_evidence_packet(&decoded).is_err());

        let mut wrong_type_command = original.clone();
        wrong_type_command["gestures"][0]["d"]["c"][0]["ce"] = serde_json::json!(4);
        let decoded: HotkeyCaseEvidence = serde_json::from_value(wrong_type_command).unwrap();
        assert!(validate_hotkey_evidence_packet(&decoded).is_err());

        let mut wrong_correlation = serde_json::to_value(h04_matrix_evidence_packet()).unwrap();
        let gestures = wrong_correlation["gestures"]
            .as_array()
            .expect("gesture array");
        let applied_index = gestures
            .iter()
            .position(|gesture| gesture["d"]["d"] == "applied")
            .expect("applied gesture");
        let invocation_id = gestures[applied_index]["i"].as_u64().unwrap();
        let other_command_ordinal = wrong_correlation["candidate_events"]
            .as_array()
            .expect("candidate event array")
            .iter()
            .flat_map(|group| group["e"].as_array().expect("event group records"))
            .find(|record| record[0] == 5 && record[4].as_u64() != Some(invocation_id))
            .expect("command from a different invocation")[1]
            .clone();
        wrong_correlation["gestures"][applied_index]["d"]["c"][0]["ce"] = other_command_ordinal;
        let decoded: HotkeyCaseEvidence = serde_json::from_value(wrong_correlation).unwrap();
        assert!(
            validate_hotkey_evidence_packet_with_context(
                &decoded,
                AcceptanceHotkey::ShiftAltWinEnd,
                350,
            )
            .is_err()
        );

        let mut dangling_snapshot = original.clone();
        dangling_snapshot["gestures"][0]["d"]["c"][0]["se"] = serde_json::json!(999);
        let decoded: HotkeyCaseEvidence = serde_json::from_value(dangling_snapshot).unwrap();
        assert!(validate_hotkey_evidence_packet(&decoded).is_err());

        let mut wrong_type_snapshot = original.clone();
        wrong_type_snapshot["gestures"][0]["d"]["c"][0]["se"] = serde_json::json!(5);
        let decoded: HotkeyCaseEvidence = serde_json::from_value(wrong_type_snapshot).unwrap();
        assert!(validate_hotkey_evidence_packet(&decoded).is_err());

        let mut duplicate_span_reference = packet.clone();
        let HotkeyDecisionProof::Applied { root_commands, .. } =
            &mut duplicate_span_reference.gestures[0].decision
        else {
            panic!("fixture must have an applied decision");
        };
        root_commands.push(root_commands[0].clone());
        assert!(validate_hotkey_evidence_packet(&duplicate_span_reference).is_err());

        let mut duplicate_snapshot_reference =
            serde_json::to_value(h04_matrix_evidence_packet()).unwrap();
        let gestures = duplicate_snapshot_reference["gestures"]
            .as_array_mut()
            .expect("gesture array");
        let first_applied = gestures
            .iter()
            .position(|gesture| gesture["d"]["d"] == "applied")
            .expect("at least one applied gesture");
        let second_applied = gestures
            .iter()
            .skip(first_applied + 1)
            .position(|gesture| gesture["d"]["d"] == "applied")
            .map(|offset| first_applied + 1 + offset)
            .expect("two applied gestures");
        let first_snapshot = gestures[first_applied]["d"]["c"][0]["se"].clone();
        gestures[second_applied]["d"]["c"][0]["se"] = first_snapshot;
        let decoded: HotkeyCaseEvidence =
            serde_json::from_value(duplicate_snapshot_reference).unwrap();
        assert!(
            validate_hotkey_evidence_packet_with_context(
                &decoded,
                AcceptanceHotkey::ShiftAltWinEnd,
                350,
            )
            .is_err()
        );
    }

    #[test]
    fn compact_candidate_event_wire_rejects_unknown_kind_and_wrong_record_length() {
        let packet = h04_matrix_evidence_packet();
        let original = serde_json::to_value(&packet).unwrap();

        let mut unknown_kind = original.clone();
        unknown_kind["candidate_events"][0]["e"][0][0] = serde_json::json!(99);
        assert!(serde_json::from_value::<HotkeyCaseEvidence>(unknown_kind).is_err());

        let mut wrong_length = original;
        wrong_length["candidate_events"][0]["e"][0]
            .as_array_mut()
            .unwrap()
            .push(serde_json::Value::Null);
        assert!(serde_json::from_value::<HotkeyCaseEvidence>(wrong_length).is_err());
    }

    #[test]
    fn compact_candidate_event_wire_round_trips_non_burst_event_kinds() {
        let mut packet = h04_matrix_evidence_packet();
        let template = packet.candidate_events[0].clone();
        let draw_focus = HotkeyCandidateEventEvidence {
            stream: HotkeyCandidateStream::MainCandidate,
            input_group_id: 90,
            input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
            event_ordinal: 20_001,
            elapsed_ms: 20_001,
            kind: HotkeyTraceEventKind::ScreenDrawRestoreFocusIntent,
            invocation_id: Some(99),
            visibility_revision: Some(99),
            focus_intent: Some(HotkeyRootFocusIntent::PreserveForeground),
            visible: None,
            minimized: None,
            bounds: None,
            hwnd: None,
            process_id: None,
            request_id: None,
            command: None,
            visibility_source: None,
            modifiers_match: None,
            provenance: None,
            terminal: None,
            activation_edge: None,
            radial_action_stage: None,
            ..template.clone()
        };
        let native_activation = HotkeyCandidateEventEvidence {
            stream: HotkeyCandidateStream::MainCandidate,
            input_group_id: 91,
            input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
            event_ordinal: 20_002,
            elapsed_ms: 20_002,
            kind: HotkeyTraceEventKind::NativeActivation,
            invocation_id: Some(99),
            visibility_revision: Some(99),
            request_id: Some(199),
            hwnd: Some(1001),
            terminal: Some(true),
            activation_edge: Some(HotkeyActivationEdge::RestoreCompleted),
            focus_intent: Some(HotkeyRootFocusIntent::ActivateRoot),
            visible: None,
            minimized: None,
            bounds: None,
            process_id: None,
            command: None,
            visibility_source: None,
            modifiers_match: None,
            provenance: None,
            radial_action_stage: None,
            ..template.clone()
        };
        let radial_action = HotkeyCandidateEventEvidence {
            stream: HotkeyCandidateStream::MainCandidate,
            input_group_id: 92,
            input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
            event_ordinal: 20_003,
            elapsed_ms: 20_003,
            kind: HotkeyTraceEventKind::RadialAction,
            invocation_id: Some(99),
            visibility_revision: Some(99),
            radial_action_stage: Some(HotkeyRadialActionStage::Parsed),
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
            ..template
        };
        packet
            .candidate_events
            .extend([draw_focus, native_activation, radial_action]);
        packet.candidate_event_count = packet.candidate_events.len();

        let encoded = serde_json::to_vec(&packet).unwrap();
        let decoded: HotkeyCaseEvidence = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded.candidate_events, packet.candidate_events);
    }

    #[test]
    fn oversized_packet_error_reports_bounded_component_counts() {
        let mut packet = h04_matrix_evidence_packet();
        let mut next_ordinal = packet
            .candidate_events
            .last()
            .expect("fixture candidate events")
            .event_ordinal
            + 1;
        let mut next_elapsed = packet
            .candidate_events
            .last()
            .expect("fixture candidate events")
            .elapsed_ms
            + 1;
        let template = packet
            .candidate_events
            .iter()
            .find(|event| event.kind == HotkeyTraceEventKind::NativeWindowSnapshot)
            .expect("fixture physical snapshot")
            .clone();
        while packet.candidate_events.len() < MAX_HOTKEY_EVIDENCE_EVENTS {
            let mut event = template.clone();
            event.event_ordinal = next_ordinal;
            event.elapsed_ms = next_elapsed;
            next_ordinal += 1;
            next_elapsed += 1;
            packet.candidate_events.push(event);
        }
        packet.candidate_event_count = packet.candidate_events.len();

        let edge = packet.runner_edges.last().unwrap().clone();
        while packet.runner_edges.len() < MAX_HOTKEY_EVIDENCE_EDGES {
            let mut edge = edge.clone();
            edge.runner_relative_us = packet
                .runner_edges
                .last()
                .unwrap()
                .runner_relative_us
                .saturating_add(1);
            packet.runner_edges.push(edge);
        }
        let gesture = packet.gestures[1].clone();
        while packet.gestures.len() < MAX_HOTKEY_EVIDENCE_GESTURES {
            packet.gestures.push(gesture.clone());
        }

        // Keep component counts at their bounded maxima while independently
        // exercising the serialized byte guard with a hostile oversized tag.
        packet.runner_clock = "x".repeat(MAX_HOTKEY_CASE_EVIDENCE_BYTES);

        let encoded_len = serde_json::to_vec(&packet).unwrap().len();
        assert!(encoded_len > MAX_HOTKEY_CASE_EVIDENCE_BYTES);
        let error = validate_hotkey_evidence_packet_with_context(
            &packet,
            AcceptanceHotkey::ShiftAltWinEnd,
            350,
        )
        .unwrap_err();
        assert!(error.contains(&format!("bytes={encoded_len}")), "{error}");
        assert!(
            error.contains(&format!("candidate_events={MAX_HOTKEY_EVIDENCE_EVENTS}")),
            "{error}"
        );
        assert!(
            error.contains(&format!("runner_edges={MAX_HOTKEY_EVIDENCE_EDGES}")),
            "{error}"
        );
        assert!(
            error.contains(&format!("gestures={MAX_HOTKEY_EVIDENCE_GESTURES}")),
            "{error}"
        );
        assert!(error.contains("command_spans=") && error.contains("snapshots="));
    }

    #[test]
    fn hotkey_packet_requires_external_primary_edges_and_exact_tagged_chord() {
        let packet = h01_evidence_packet();
        // Release modifier matching is diagnostic; false is valid after the
        // owned chord has been released, while Press must match the chord.
        assert_eq!(packet.candidate_events[1].modifiers_match, Some(false));
        validate_hotkey_evidence_packet(&packet).unwrap();

        let mut press_not_external = packet.clone();
        press_not_external.candidate_events[0].provenance = Some(HotkeyInputProvenance::Owned);
        assert!(validate_hotkey_evidence_packet(&press_not_external).is_err());

        let mut release_not_external = packet.clone();
        release_not_external.candidate_events[1].provenance = Some(HotkeyInputProvenance::Physical);
        assert!(validate_hotkey_evidence_packet(&release_not_external).is_err());

        let mut malformed_release = packet.clone();
        malformed_release.candidate_events[1].modifiers_match = None;
        assert!(validate_hotkey_evidence_packet(&malformed_release).is_err());

        let mut partial = packet.clone();
        partial.runner_edges.pop();
        assert!(validate_hotkey_evidence_packet(&partial).is_err());

        let mut wrong_key = packet.clone();
        wrong_key.runner_edges[0].virtual_key = 0x23;
        assert!(validate_hotkey_evidence_packet(&wrong_key).is_err());

        let mut duplicate = packet.clone();
        duplicate
            .runner_edges
            .push(duplicate.runner_edges[1].clone());
        assert!(validate_hotkey_evidence_packet(&duplicate).is_err());
    }

    #[test]
    fn h16_legacy_gesture_and_standalone_intent_share_one_exact_f11_pair() {
        let packet = h16_legacy_fallback_group_packet();
        validate_runner_edge_groups(&packet, AcceptanceHotkey::ShiftAltWinEnd, 350).unwrap();

        let mut missing_release = packet.clone();
        missing_release.runner_edges.pop();
        assert!(
            validate_runner_edge_groups(&missing_release, AcceptanceHotkey::ShiftAltWinEnd, 350)
                .is_err()
        );

        let mut extra_chord = packet.clone();
        extra_chord.runner_edges.extend([
            HotkeyRunnerEdgeEvidence {
                runner_relative_us: 300,
                ..packet.runner_edges[0].clone()
            },
            HotkeyRunnerEdgeEvidence {
                runner_relative_us: 400,
                ..packet.runner_edges[1].clone()
            },
        ]);
        assert!(
            validate_runner_edge_groups(&extra_chord, AcceptanceHotkey::ShiftAltWinEnd, 350)
                .is_err()
        );

        let mut unrelated_intent = packet.clone();
        unrelated_intent.standalone_decisions[0].input_group_id += 1;
        assert!(
            validate_runner_edge_groups(&unrelated_intent, AcceptanceHotkey::ShiftAltWinEnd, 350)
                .is_err()
        );

        let mut wrong_gesture_reason = packet;
        wrong_gesture_reason.gestures[0].decision = HotkeyDecisionProof::NotApplicable {
            reason: HotkeyEvidenceNotApplicable::HoldGestureHasNoShortTap,
        };
        assert!(
            validate_runner_edge_groups(
                &wrong_gesture_reason,
                AcceptanceHotkey::ShiftAltWinEnd,
                350
            )
            .is_err()
        );
    }

    #[test]
    fn native_presentation_requires_captured_root_and_physical_monitor_bounds() {
        let packet = h01_evidence_packet();
        validate_hotkey_evidence_packet(&packet).unwrap();

        let mut replacement_hwnd = packet.clone();
        replacement_hwnd.candidate_events[5].hwnd = Some(9999);
        assert!(validate_hotkey_evidence_packet(&replacement_hwnd).is_err());

        let mut replacement_pid = packet.clone();
        replacement_pid.candidate_events[5].process_id = Some(9999);
        assert!(validate_hotkey_evidence_packet(&replacement_pid).is_err());

        let mut monitor_gap = packet;
        monitor_gap.physical_displays = vec![[0, 0, 50, 100], [1_000, 0, 1_100, 100]];
        assert!(validate_hotkey_evidence_packet(&monitor_gap).is_err());
    }

    #[test]
    fn screen_draw_restore_is_a_correlated_follow_on_not_a_second_gesture_intent() {
        let mut packet = h01_evidence_packet();
        append_screen_draw_restore(&mut packet, Some(5), HotkeyRootFocusIntent::ActivateRoot);
        validate_hotkey_evidence_packet(&packet).unwrap();
        assert_eq!(packet.gestures.len(), 1);
        assert_eq!(packet.follow_on_restorations.len(), 1);
        assert_eq!(
            packet.follow_on_restorations[0].parent_visibility_revision,
            Some(7)
        );
        assert_eq!(packet.follow_on_restorations[0].visibility_revision, 8);
        assert_eq!(packet.follow_on_restorations[0].invocation_id, Some(5));
        assert_eq!(
            packet.follow_on_restorations[0].focus_intent,
            HotkeyRootFocusIntent::ActivateRoot
        );

        let mut missing_native_completion = packet.clone();
        missing_native_completion.follow_on_restorations[0].native_activation = None;
        assert!(validate_hotkey_evidence_packet(&missing_native_completion).is_err());

        let mut unrelated_restore = h01_evidence_packet();
        append_screen_draw_restore(
            &mut unrelated_restore,
            None,
            HotkeyRootFocusIntent::ActivateRoot,
        );
        validate_hotkey_evidence_packet(&unrelated_restore).unwrap();
        assert_eq!(
            unrelated_restore.follow_on_restorations[0].invocation_id,
            None
        );
        assert_eq!(
            unrelated_restore.follow_on_restorations[0].parent_visibility_revision,
            None
        );

        let mut preserve_foreground = h01_evidence_packet();
        append_screen_draw_restore(
            &mut preserve_foreground,
            Some(5),
            HotkeyRootFocusIntent::PreserveForeground,
        );
        validate_hotkey_evidence_packet(&preserve_foreground).unwrap();
        assert!(
            preserve_foreground.follow_on_restorations[0]
                .native_activation
                .is_none()
        );

        let mut unexpected_preserve_activation = preserve_foreground.clone();
        unexpected_preserve_activation.follow_on_restorations[0].native_activation =
            packet.follow_on_restorations[0].native_activation.clone();
        assert!(validate_hotkey_evidence_packet(&unexpected_preserve_activation).is_err());

        let mut missing_focus_intent = preserve_foreground;
        missing_focus_intent
            .candidate_events
            .retain(|event| event.kind != HotkeyTraceEventKind::ScreenDrawRestoreFocusIntent);
        missing_focus_intent.candidate_event_count = missing_focus_intent.candidate_events.len();
        assert!(validate_hotkey_evidence_packet(&missing_focus_intent).is_err());
    }

    #[test]
    fn preserve_foreground_rejects_raw_correlated_native_activation_edges() {
        let mut activate_root = h01_evidence_packet();
        append_screen_draw_restore(
            &mut activate_root,
            Some(5),
            HotkeyRootFocusIntent::ActivateRoot,
        );
        validate_hotkey_evidence_packet(&activate_root).unwrap();

        let mut preserve = h01_evidence_packet();
        append_screen_draw_restore(
            &mut preserve,
            Some(5),
            HotkeyRootFocusIntent::PreserveForeground,
        );
        validate_hotkey_evidence_packet(&preserve).unwrap();

        let activation_events = activate_root
            .candidate_events
            .iter()
            .filter(|event| event.kind == HotkeyTraceEventKind::NativeActivation)
            .cloned()
            .collect::<Vec<_>>();
        let requested = activation_events
            .iter()
            .find(|event| event.activation_edge == Some(HotkeyActivationEdge::RestoreRequested))
            .unwrap();
        let completed = activation_events
            .iter()
            .find(|event| event.activation_edge == Some(HotkeyActivationEdge::RestoreCompleted))
            .unwrap();

        let mut failed = requested.clone();
        failed.activation_edge = Some(HotkeyActivationEdge::RestoreFailed);
        failed.terminal = Some(true);
        let mut malformed_request = requested.clone();
        malformed_request.activation_edge = Some(HotkeyActivationEdge::Other);
        malformed_request.request_id = None;
        malformed_request.terminal = None;

        for (name, unexpected) in [
            ("completion without request", completed.clone()),
            ("failed restore", failed),
            ("malformed request", malformed_request),
        ] {
            let mut mutation = preserve.clone();
            let next_ordinal = mutation
                .candidate_events
                .iter()
                .map(|event| event.event_ordinal)
                .max()
                .unwrap_or_default()
                + 1;
            let mut unexpected = unexpected;
            unexpected.event_ordinal = next_ordinal;
            mutation.candidate_events.push(unexpected);
            mutation
                .candidate_events
                .sort_by_key(|event| (event.stream, event.event_ordinal));
            mutation.candidate_event_count = mutation.candidate_events.len();

            assert!(
                validate_hotkey_evidence_packet(&mutation).is_err(),
                "PreserveForeground accepted raw {name} evidence"
            );
        }
    }

    #[test]
    fn hotkey_evidence_requires_exact_release_intent_command_and_snapshot_arithmetic() {
        let mut packet = h01_evidence_packet();
        let HotkeyDecisionProof::Applied {
            release_to_intent_ms,
            ..
        } = &mut packet.gestures[0].decision
        else {
            panic!("expected applied proof");
        };
        *release_to_intent_ms = 5;
        assert!(validate_hotkey_evidence_packet(&packet).is_err());

        let mut packet = h01_evidence_packet();
        let HotkeyDecisionProof::Applied { root_commands, .. } = &mut packet.gestures[0].decision
        else {
            panic!("expected applied proof");
        };
        root_commands[0].release_to_root_command_ms = Some(7);
        assert!(validate_hotkey_evidence_packet(&packet).is_err());

        let mut packet = h01_evidence_packet();
        let HotkeyDecisionProof::Applied { root_commands, .. } = &mut packet.gestures[0].decision
        else {
            panic!("expected applied proof");
        };
        root_commands[0]
            .observed_presentation
            .as_mut()
            .unwrap()
            .command_to_observed_ms = 5;
        assert!(validate_hotkey_evidence_packet(&packet).is_err());

        let mut packet = h01_evidence_packet();
        packet.candidate_events.pop();
        packet.candidate_event_count -= 1;
        assert!(validate_hotkey_evidence_packet(&packet).is_err());
    }

    #[test]
    fn hotkey_evidence_rejects_malformed_edges_and_duplicate_or_shuffled_correlations() {
        let mut packet = h01_evidence_packet();
        packet.candidate_events[0].modifiers_match = Some(false);
        assert!(validate_hotkey_evidence_packet(&packet).is_err());

        let mut packet = h01_evidence_packet();
        packet.candidate_events[1].modifiers_match = None;
        assert!(validate_hotkey_evidence_packet(&packet).is_err());

        let mut packet = h01_evidence_packet();
        let mut duplicate = packet.candidate_events[4].clone();
        duplicate.event_ordinal = 7;
        duplicate.elapsed_ms = 33;
        packet.candidate_events.push(duplicate);
        packet.candidate_event_count += 1;
        assert!(validate_hotkey_evidence_packet(&packet).is_err());

        let mut packet = h01_evidence_packet();
        packet.candidate_events.swap(0, 1);
        assert!(validate_hotkey_evidence_packet(&packet).is_err());

        let mut packet = h01_evidence_packet();
        packet.gestures.clear();
        assert!(validate_hotkey_evidence_packet(&packet).is_err());

        let mut packet = h01_evidence_packet();
        packet.runner_edge_overflow = true;
        assert!(validate_hotkey_evidence_packet(&packet).is_err());
    }

    #[test]
    fn coalesced_intermediate_gesture_is_explicitly_superseded() {
        let packet = h07_superseded_evidence_packet();
        validate_hotkey_evidence_packet(&packet).unwrap();
        assert!(matches!(
            &packet.gestures[0].decision,
            HotkeyDecisionProof::Superseded {
                visibility_revision: 7,
                by_revision: 8,
                ..
            }
        ));

        let mut missing_successor = packet.clone();
        missing_successor.candidate_events.retain(|event| {
            event.visibility_revision != Some(8)
                && !(event.kind == HotkeyTraceEventKind::RootCommand
                    && event.request_id == Some(21))
                && !(event.kind == HotkeyTraceEventKind::NativeWindowSnapshot
                    && event.request_id == Some(21))
        });
        missing_successor.candidate_event_count = missing_successor.candidate_events.len();
        assert!(validate_hotkey_evidence_packet(&missing_successor).is_err());
    }

    #[test]
    fn h04_compact_summary_and_typed_packet_pass_clean_and_retry_r0_readback() {
        let hotkey = AcceptanceHotkey::ShiftAltWinEnd;
        let compact = hotkey_matrix_case_evidence(hotkey);
        assert!(compact.len() < MAX_RESULT_BYTES, "{} bytes", compact.len());
        assert!(validate_required_case_evidence("H04", CaseStatus::Passed, &compact).is_ok());

        let mut report = acceptance_report("native_windows");
        report.suite = AcceptanceSuite::Hotkey;
        report.hotkey = hotkey;
        report.profile.configured_hotkey = hotkey.as_str();
        report.profile.hold_threshold_ms = 350;
        report.cases.push(AcceptanceCaseResult {
            id: "H04".into(),
            status: CaseStatus::Passed,
            elapsed_ms: 1,
            expected: "10 bounded hidden/visible bursts, 86 decisions".into(),
            observed: compact.clone(),
            failure_stage: None,
            artifacts: Vec::new(),
        });
        report.hotkey_evidence.push(h04_matrix_evidence_packet());
        validate_case_hotkey_profile_relation(&report, &report.cases[0]).unwrap();
        validate_hotkey_evidence_report(&report).unwrap();

        let output = tempfile::tempdir().unwrap();
        let artifact_name = "case-H04-attempt-1-contamination.json";
        let artifact_path = output.path().join(artifact_name);
        let stream = HotkeyCandidateStream::MainCandidate;
        let chord_offsets = [0, 5_000, 10_000, 15_000, 40_000, 45_000, 50_000, 55_000];
        let owned_edges = configured_chord_edges(hotkey, 1)
            .into_iter()
            .enumerate()
            .map(|(index, (virtual_key, down))| HotkeyRunnerEdgeEvidence {
                runner_relative_us: chord_offsets[index],
                input_group_id: 1,
                stream,
                purpose: HotkeyRunnerInputPurpose::MatrixBurst,
                virtual_key,
                transition: if down {
                    HotkeyEdgeTransition::Press
                } else {
                    HotkeyEdgeTransition::Release
                },
                injected: true,
                runner_cookie_matched: true,
            })
            .collect::<Vec<_>>();
        let foreign_edges = vec![
            HotkeyRunnerEdgeEvidence {
                runner_relative_us: 20_000,
                input_group_id: 1,
                stream,
                purpose: HotkeyRunnerInputPurpose::MatrixBurst,
                virtual_key: 0xA4,
                transition: HotkeyEdgeTransition::Press,
                injected: true,
                runner_cookie_matched: false,
            },
            HotkeyRunnerEdgeEvidence {
                runner_relative_us: 25_000,
                input_group_id: 1,
                stream,
                purpose: HotkeyRunnerInputPurpose::MatrixBurst,
                virtual_key: 0xA4,
                transition: HotkeyEdgeTransition::Release,
                injected: true,
                runner_cookie_matched: false,
            },
        ];
        let artifact = H04InputContaminationArtifact {
            schema_version: 1,
            case_id: "H04".into(),
            attempt: 1,
            hotkey,
            failure_stage: "InputInjection".into(),
            failure: "foreign Alt pair overlapped the owned chord".into(),
            declared_initial_state: false,
            next_matrix_burst_index: 1,
            completed_bursts: Vec::new(),
            input_group_id: 1,
            stream,
            prior_group_ids: vec![1],
            owned_edges,
            foreign_edges,
            candidate_events: Vec::new(),
            root_identity: HotkeyRootIdentityEvidence {
                stream,
                hwnd: 1001,
                process_id: 202,
            },
        };
        validate_h04_contamination_artifact(&artifact, hotkey).unwrap();
        fs::write(&artifact_path, serde_json::to_vec(&artifact).unwrap()).unwrap();
        let artifact_path = artifact_path.to_string_lossy().into_owned();

        let retried = compact
            .replace("matrix_attempts=1", "matrix_attempts=2")
            .replace("contamination_attempts=0", "contamination_attempts=1")
            .replace(
                "contamination_artifacts=none",
                &format!("contamination_artifacts={artifact_name}"),
            );
        assert!(retried.len() < MAX_RESULT_BYTES, "{} bytes", retried.len());
        assert!(validate_required_case_evidence("H04", CaseStatus::Passed, &retried).is_ok());
        report.cases[0].observed = retried;
        report.cases[0].artifacts = vec![artifact_path.clone()];
        report.artifacts = vec![artifact_path];
        validate_hotkey_evidence_report(&report).unwrap();
    }

    #[test]
    fn report_integrity_requires_exactly_one_valid_packet_for_each_passed_h_case() {
        let mut report = acceptance_report("native_windows");
        report.cases.push(AcceptanceCaseResult {
            id: "H01".into(),
            status: CaseStatus::Passed,
            elapsed_ms: 1,
            expected: "wake and focus ROOT".into(),
            observed: "visible and focused".into(),
            failure_stage: None,
            artifacts: Vec::new(),
        });
        report.hotkey_evidence.push(h01_evidence_packet());
        validate_hotkey_evidence_report(&report).unwrap();

        report.hotkey_evidence.clear();
        assert!(validate_hotkey_evidence_report(&report).is_err());

        report.hotkey_evidence = vec![h01_evidence_packet(), h01_evidence_packet()];
        assert!(validate_hotkey_evidence_report(&report).is_err());
    }

    #[cfg(windows)]
    fn migration_receipt(state: SubmenuMigrationState) -> SubmenuPresentationMigrationReceipt {
        SubmenuPresentationMigrationReceipt {
            migration_id: "radial-submenu-same-center-v1".into(),
            version: 1,
            state,
            source_settings_sha256: "1".repeat(64),
            source_radial_sha256: "2".repeat(64),
            settings_backup_path: r"G:\outside-source\settings.json.bak".into(),
            settings_backup_sha256: "3".repeat(64),
            settings_source_existed: true,
            radial_backup_path: r"G:\outside-source\radial.json.bak".into(),
            radial_backup_sha256: "4".repeat(64),
            settings_default_before: multi_launcher::radial::model::SubmenuPresentation::Cascade,
            settings_default_target: multi_launcher::radial::model::SubmenuPresentation::SameCenter,
            changed_menus: Vec::new(),
            target_radial_revision: 2,
            target_radial_sha256: "5".repeat(64),
            target_settings_content_sha256: "6".repeat(64),
            undo_restored_menu_ids: Vec::new(),
            undo_source_radial_sha256: None,
            undo_target_radial_revision: None,
            undo_target_radial_sha256: None,
            undo_source_settings_content_sha256: None,
            undo_target_settings_content_sha256: None,
            undo_settings_default_source: None,
            undo_restores_settings_default: false,
            failure: None,
        }
    }

    fn parse(args: &[&str]) -> Result<Arguments, String> {
        let parsed = parse_arguments(args.iter().map(OsString::from))?;
        match parsed {
            ParseResult::Run(arguments) => Ok(arguments),
            ParseResult::Help => Err("unexpected help result".into()),
        }
    }

    fn acceptance_report(mode: &'static str) -> AcceptanceReport {
        AcceptanceReport {
            schema_version: 7,
            run_id: "test-run".into(),
            mode,
            started_unix_ms: 1,
            finished_unix_ms: 2,
            copied_profile_status: CopiedProfileStatus::NotRun,
            copied_profile: None,
            private_artifacts: None,
            h6_repeat_mode: H6RepeatMode::Quiescent,
            mouse_gesture_mode: MouseGestureMode::Enabled,
            suite: AcceptanceSuite::All,
            hotkey: AcceptanceHotkey::F11,
            outcome: "running",
            candidate: CandidateIdentity {
                executable: "candidate.exe".into(),
                sha256: "a".repeat(64),
            },
            environment: EnvironmentIdentity {
                os_version: "test".into(),
                architecture: "x64".into(),
                runner_process_id: 1,
                runner_sha256: Some("b".repeat(64)),
                child_process_id: None,
                child_started_unix_ms: None,
                source_revision: Some("deadbeef".into()),
                monitors: Vec::new(),
            },
            profile: ProfileIdentity {
                mode: "test",
                temporary_data_root: "private profile".into(),
                settings_sha256: "c".repeat(64),
                radial_sha256: "d".repeat(64),
                actions_sha256: "e".repeat(64),
                configured_hotkey: ACCEPTANCE_HOTKEY,
                hold_threshold_ms: 1,
            },
            cases: Vec::new(),
            hotkey_evidence: Vec::new(),
            artifacts: Vec::new(),
            cleanup: CleanupResult::default(),
            capacity_saturated: false,
            report_overflow: None,
        }
    }

    fn passing_copied_report() -> AcceptanceReport {
        let mut report = acceptance_report("native_windows_copied_profile");
        report.copied_profile_status = CopiedProfileStatus::Passed;
        let private_summary = private_artifacts::PrivateArtifactSummary {
            status: private_artifacts::PrivateArtifactStatus::Retained,
            artifact_id: Some("multi-launcher-private-evidence-test-1234".into()),
            file_count: 4,
            total_bytes: 64,
            manifest_sha256: Some("f".repeat(64)),
        };
        report.private_artifacts = Some(private_summary.clone());
        for id in COPIED_CASE_IDS {
            report.push_case(AcceptanceCaseResult {
                id: id.into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "bounded expectation".into(),
                observed: if id == "CP_R1" {
                    private_artifact_evidence(&private_summary)
                } else {
                    "bounded evidence".into()
                },
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        report.cleanup.child_closed_normally = true;
        report.cleanup.child_owned_windows_closed = true;
        report.cleanup.profile_removed = true;
        report.cleanup.cursor_restored = true;
        report.cleanup.input_desktop_released = true;
        report
    }

    #[cfg(windows)]
    #[test]
    fn cleanup_only_failure_retains_private_diagnostic_bundle() {
        let mut report = acceptance_report("native_windows_copied_profile");
        report.environment.child_process_id = Some(4242);
        report.cleanup.child_closed_normally = true;
        report.cleanup.child_owned_windows_closed = true;
        report.cleanup.cursor_restored = true;
        report.cleanup.input_desktop_released = true;
        assert!(!copied_failure_requires_retention(&report, true, true));

        // Cursor restoration is the only failed native cleanup signal; all native
        // cases, the isolated profile audit, and the supplied source inventory pass.
        report.cleanup.cursor_restored = false;
        assert!(report.cases.is_empty());
        assert!(copied_failure_requires_retention(&report, true, true));

        let profile = tempfile::tempdir().unwrap();
        let names = [
            "case-R1-trace.log",
            "case-R1-windows.json",
            "case-R1-private.log",
            "case-R1.png",
        ];
        let paths = names
            .into_iter()
            .map(|name| {
                let path = profile.path().join(name);
                fs::write(&path, name.as_bytes()).unwrap();
                path
            })
            .collect::<Vec<_>>();
        let staged = private_artifacts::stage_diagnostics(profile.path(), paths).unwrap();
        let retained = if copied_failure_requires_retention(&report, true, true) {
            Some(staged.retain())
        } else {
            None
        };
        let profile_path = profile.path().to_path_buf();
        profile.close().unwrap();
        assert!(!profile_path.exists());

        let retained = retained.expect("cleanup-only failure must retain its private evidence");
        private_artifacts::verify_retained_artifacts(&retained.directory, &retained.summary)
            .unwrap();
        assert!(retained.directory.join("case-R1.png").is_file());
        fs::remove_dir_all(&retained.directory).unwrap();
    }

    #[test]
    fn h6_repeat_mode_defaults_to_quiescent_and_accepts_immediate_probe() {
        let default = parse(&["--output", "run-default"]).unwrap();
        assert_eq!(default.h6_repeat_mode, H6RepeatMode::Quiescent);
        assert_eq!(default.mouse_gesture_mode, MouseGestureMode::Enabled);

        let immediate = parse(&["--output", "run-immediate", "--h6-repeat", "immediate"]).unwrap();
        assert_eq!(immediate.h6_repeat_mode, H6RepeatMode::Immediate);
        let gestures_disabled = parse(&[
            "--output",
            "run-no-gestures",
            "--mouse-gestures",
            "disabled-diagnostic",
        ])
        .unwrap();
        assert_eq!(
            gestures_disabled.mouse_gesture_mode,
            MouseGestureMode::DisabledDiagnostic
        );

        let production_only = parse(&[
            "--output",
            "run-production-only",
            "--h6-repeat",
            "production-only-diagnostic",
        ])
        .unwrap();
        assert_eq!(
            production_only.h6_repeat_mode,
            H6RepeatMode::ProductionOnlyDiagnostic
        );
    }

    #[test]
    fn hotkey_suite_accepts_the_exact_chord_and_rejects_invalid_combinations() {
        let chord = parse(&[
            "--output",
            "hotkey-run",
            "--suite",
            "hotkey",
            "--hotkey",
            "shift-alt-win-end",
        ])
        .unwrap();
        assert_eq!(chord.suite, AcceptanceSuite::Hotkey);
        assert_eq!(chord.hotkey, AcceptanceHotkey::ShiftAltWinEnd);

        let f11 = parse(&["--output", "hotkey-f11", "--suite", "hotkey"]).unwrap();
        assert_eq!(f11.hotkey, AcceptanceHotkey::F11);

        assert!(parse(&["--output", "all-chord", "--hotkey", "shift-alt-win-end",]).is_err());
        assert!(
            parse(&[
                "--output",
                "copied-hotkey",
                "--suite",
                "hotkey",
                "--profile-copy",
                "supplied-profile",
            ])
            .is_err()
        );
    }

    #[test]
    fn chord_fixture_uses_shift_alt_win_end_as_the_configured_launcher_hotkey() {
        let profile = tempfile::tempdir().unwrap();
        let fixture = deterministic_fixture_for_hotkey(
            &profile.path().join("acceptance.log"),
            MouseGestureMode::Enabled,
            AcceptanceHotkey::ShiftAltWinEnd,
        )
        .unwrap();
        let settings: Settings = serde_json::from_slice(&fixture.settings_json).unwrap();
        assert_eq!(settings.hotkey.as_deref(), Some("Shift+Alt+Win+End"));
        assert!(multi_launcher::hotkey::parse_hotkey("Shift+Alt+Win+End").is_some());
    }

    #[test]
    fn hotkey_fixture_trace_uses_the_configured_settings_log_path() {
        let profile = tempfile::tempdir().unwrap();
        let trace_path = profile.path().join("candidate.log");
        let fixture = deterministic_fixture_for_hotkey(
            &trace_path,
            MouseGestureMode::Enabled,
            AcceptanceHotkey::F11,
        )
        .unwrap();
        let settings: Settings = serde_json::from_slice(&fixture.settings_json).unwrap();
        let configured_path = match settings.log_file {
            Some(LogFile::Path(path)) => path,
            _ => panic!("hotkey fixture must configure its trace log path"),
        };
        assert_eq!(Path::new(&configured_path), trace_path);
    }

    #[test]
    fn hotkey_fixture_binds_the_hover_probe_to_a_harmless_executable_root_cell() {
        let profile = tempfile::tempdir().unwrap();
        let fixture = deterministic_fixture_for_hotkey(
            &profile.path().join("acceptance.log"),
            MouseGestureMode::Enabled,
            AcceptanceHotkey::F11,
        )
        .unwrap();
        let document: RadialDocument = serde_json::from_slice(&fixture.radial_json).unwrap();
        let actions: Vec<multi_launcher::actions::Action> =
            serde_json::from_slice(&fixture.actions_json).unwrap();
        let cell = document
            .menus
            .iter()
            .find(|menu| menu.id == document.default_menu_id)
            .and_then(|menu| menu.rings.first())
            .and_then(|ring| ring.cells.first())
            .expect("fixture root hover-probe cell");
        let CellContent::Action {
            binding:
                ActionBinding::Persisted {
                    action:
                        PersistedUniversalActionRef {
                            target:
                                Some(PersistableActionTargetRef::CustomAction {
                                    action: bound_action,
                                }),
                            action_id,
                        },
                },
        } = &cell.content
        else {
            panic!("fixture root hover-probe cell must bind a persisted custom action");
        };
        assert_eq!(*action_id, action_ids::RESULT_EXECUTE);
        assert_eq!(bound_action, &actions[0]);
        assert!(bound_action.label.contains("Harmless Action"));
        validate_radial_document(&document).expect("hover-probe fixture validates");
    }

    fn synthetic_hotkey_burst(taps: usize, initial_visible: bool) -> Vec<String> {
        let mut events = Vec::with_capacity(taps * 6);
        let mut visible = initial_visible;
        for index in 0..taps {
            let invocation_id = 100 + index as u64;
            events.push(
                "trace_event=\"hook_primary\" transition=Press provenance=ExternalInjected".into(),
            );
            events.push(format!(
                "trace_event=\"configured_primary\" transition=Press provenance=ExternalInjected modifiers_match=true invocation_id={invocation_id}"
            ));
            events.push(
                "trace_event=\"hook_primary\" transition=Release provenance=ExternalInjected"
                    .into(),
            );
            events.push(format!(
                "trace_event=\"configured_primary\" transition=Release provenance=ExternalInjected modifiers_match=false invocation_id={invocation_id}"
            ));
            events.push(format!(
                "trace_event=\"short_tap\" invocation_id={invocation_id} terminal=true"
            ));
            visible = !visible;
            events.push(format!(
                "trace_event=\"desired_visibility\" visible={visible} revision={} source=ToggleBatch invocation_id={invocation_id}",
                index + 1
            ));
        }
        events
    }

    #[test]
    fn hotkey_burst_trace_correlates_release_short_tap_and_visibility_per_gesture() {
        let odd = validate_hotkey_burst_trace(&synthetic_hotkey_burst(3, true), 3, true).unwrap();
        assert_eq!(odd.invocation_ids, [100, 101, 102]);
        assert!(!odd.final_visible);

        let even = validate_hotkey_burst_trace(&synthetic_hotkey_burst(4, true), 4, true).unwrap();
        assert_eq!(even.invocation_ids, [100, 101, 102, 103]);
        assert!(even.final_visible);

        let mut press_without_modifiers = synthetic_hotkey_burst(1, true);
        press_without_modifiers[1] =
            press_without_modifiers[1].replace("modifiers_match=true", "modifiers_match=false");
        assert!(
            validate_hotkey_burst_trace(&press_without_modifiers, 1, true)
                .expect_err("press must match configured modifiers")
                .contains("press did not match")
        );

        let mut malformed_release = synthetic_hotkey_burst(1, true);
        malformed_release[3] =
            malformed_release[3].replace("modifiers_match=false", "modifiers_match=unknown");
        assert!(
            validate_hotkey_burst_trace(&malformed_release, 1, true)
                .expect_err("release modifier marker must be a boolean")
                .contains("malformed modifiers flag")
        );

        let mut missing_release = synthetic_hotkey_burst(3, true);
        missing_release.remove(2);
        assert!(validate_hotkey_burst_trace(&missing_release, 3, true).is_err());

        let mut extra_visibility = synthetic_hotkey_burst(3, true);
        extra_visibility.push(
            "trace_event=\"desired_visibility\" visible=true revision=4 source=LegacyTrigger"
                .into(),
        );
        assert!(validate_hotkey_burst_trace(&extra_visibility, 3, true).is_err());

        let mut queued_echoes = synthetic_hotkey_burst(3, true);
        queued_echoes.splice(
            6..6,
            [
                "trace_event=\"desired_visibility\" visible=false revision=1 source=Queued invocation_id=none".into(),
                "trace_event=\"desired_visibility\" visible=false revision=1 source=Queued invocation_id=none".into(),
            ],
        );
        assert!(validate_hotkey_burst_trace(&queued_echoes, 3, true).is_ok());

        let mut baseline_echo = synthetic_hotkey_burst(3, true);
        baseline_echo.insert(
            0,
            "trace_event=\"desired_visibility\" visible=true revision=41 source=Queued invocation_id=none"
                .into(),
        );
        assert!(
            validate_hotkey_burst_trace_with_baseline(&baseline_echo, 3, true, Some(41), Some(99))
                .is_ok()
        );
        assert!(validate_hotkey_burst_trace(&baseline_echo, 3, true).is_err());
        let wrong_baseline_echo = baseline_echo
            .iter()
            .cloned()
            .map(|line| line.replace("revision=41", "revision=40"))
            .collect::<Vec<_>>();
        assert!(
            validate_hotkey_burst_trace_with_baseline(
                &wrong_baseline_echo,
                3,
                true,
                Some(41),
                Some(99)
            )
            .is_err()
        );
        assert!(
            validate_hotkey_burst_trace_with_baseline(&baseline_echo, 3, true, Some(41), Some(100))
                .expect_err("fenced segments must not replay a baseline invocation")
                .contains("did not advance beyond baseline invocation")
        );

        let mut stale_queued_echo = synthetic_hotkey_burst(3, true);
        stale_queued_echo.splice(
            12..12,
            ["trace_event=\"desired_visibility\" visible=false revision=1 source=Queued invocation_id=none".into()],
        );
        assert!(validate_hotkey_burst_trace(&stale_queued_echo, 3, true).is_err());

        let mut nonalternating = synthetic_hotkey_burst(3, true);
        nonalternating[11] =
            "trace_event=\"desired_visibility\" visible=false revision=2 source=ToggleBatch invocation_id=101"
                .into();
        assert!(validate_hotkey_burst_trace(&nonalternating, 3, true).is_err());

        let mut unrelated_visibility = synthetic_hotkey_burst(3, true);
        unrelated_visibility[5] =
            "trace_event=\"desired_visibility\" visible=false revision=1 source=ToggleBatch invocation_id=9001"
                .into();
        assert!(validate_hotkey_burst_trace(&unrelated_visibility, 3, true).is_err());

        let mut visibility_before_short_tap = synthetic_hotkey_burst(3, true);
        visibility_before_short_tap.swap(4, 5);
        assert!(validate_hotkey_burst_trace(&visibility_before_short_tap, 3, true).is_err());

        let mut decision_before_release = synthetic_hotkey_burst(3, true);
        decision_before_release.swap(3, 4);
        assert!(validate_hotkey_burst_trace(&decision_before_release, 3, true).is_err());

        let mut wrong_source = synthetic_hotkey_burst(3, true);
        wrong_source[5] =
            "trace_event=\"desired_visibility\" visible=false revision=1 source=Queued invocation_id=100"
                .into();
        assert!(validate_hotkey_burst_trace(&wrong_source, 3, true).is_err());
    }

    fn hotkey_case_evidence(taps: usize, hotkey: AcceptanceHotkey) -> String {
        let is_f11 = hotkey == AcceptanceHotkey::F11;
        format!(
            "evidence:v1; hotkey={}; burst={taps}; parity={}; initial_visible=true; final_visible={}; hook_pairs={taps}; configured_pairs={taps}; short_taps={taps}; visibility_edges={taps}; uninterrupted=true; inter_tap_ui_poll=0; inter_tap_refocus=0; setup_tap=none; preflight=registered_unregistered; runner_observer=exact_injected_pairs; observer_order={}; observer_keys={}; sendinput_down={}; sendinput_up={}; input_desktop=Default; invocation_count={taps}; invocation_ids=100,101,102",
            hotkey.as_str(),
            if taps % 2 == 0 { "even" } else { "odd" },
            taps % 2 == 0,
            if is_f11 {
                "alternating_down_up"
            } else {
                "down_then_reverse_up"
            },
            if is_f11 {
                "F11"
            } else {
                "LeftShift+LeftAlt+LeftWin+End"
            },
            if is_f11 { taps } else { taps * 4 },
            if is_f11 { taps } else { taps * 4 },
        )
    }

    #[test]
    fn hotkey_report_cases_require_complete_parity_and_release_evidence() {
        for hotkey in [AcceptanceHotkey::F11, AcceptanceHotkey::ShiftAltWinEnd] {
            let h7 = hotkey_case_evidence(3, hotkey);
            let h8 = hotkey_case_evidence(4, hotkey);
            assert!(validate_required_case_evidence("H7", CaseStatus::Passed, &h7).is_ok());
            assert!(validate_required_case_evidence("H8", CaseStatus::Passed, &h8).is_ok());

            let incomplete = h7.replace("observer_order=down_then_reverse_up; ", "");
            if hotkey == AcceptanceHotkey::ShiftAltWinEnd {
                assert!(
                    validate_required_case_evidence("H7", CaseStatus::Passed, &incomplete).is_err()
                );
            }
            assert!(validate_required_case_evidence("H7", CaseStatus::Failed, &h7).is_err());
        }
    }

    fn hotkey_matrix_case_evidence(hotkey: AcceptanceHotkey) -> String {
        format!(
            "{}; matrix_attempts=1; contamination_attempts=0; contamination_artifacts=none; attempt_restart_state=hidden; full_clean_matrix=true",
            format_h04_matrix_evidence(hotkey, &"a".repeat(64), 80, 80, 0, 0)
        )
    }

    #[test]
    fn hotkey_matrix_formatter_round_trips_through_report_validator() {
        let evidence = hotkey_matrix_case_evidence(AcceptanceHotkey::ShiftAltWinEnd);
        assert!(evidence.len() < MAX_RESULT_BYTES);
        assert!(evidence.contains("input_desktop=thread=Default,active=Default"));
        assert!(validate_required_case_evidence("H04", CaseStatus::Passed, &evidence).is_ok());
        let missing_groups = evidence.replace("matrix_groups=10; ", "");
        assert!(
            validate_required_case_evidence("H04", CaseStatus::Passed, &missing_groups).is_err()
        );
        let missing_readable = evidence.replace("readable_decisions=3; ", "");
        assert!(
            validate_required_case_evidence("H04", CaseStatus::Passed, &missing_readable).is_err()
        );
        let duplicate = evidence.replace("unique_invocation_ids=86", "unique_invocation_ids=85");
        assert!(validate_required_case_evidence("H04", CaseStatus::Passed, &duplicate).is_err());
        let mut missing_hash = evidence.clone();
        missing_hash = missing_hash.replace(&"a".repeat(64), "bad");
        assert!(validate_required_case_evidence("H04", CaseStatus::Passed, &missing_hash).is_err());
        let missing_preflight = evidence.replace("inter_tap_preflight=0", "inter_tap_preflight=1");
        assert!(
            validate_required_case_evidence("H04", CaseStatus::Passed, &missing_preflight).is_err()
        );
        let missing_focused_root_hide = evidence.replace("focused_root_hide=true; ", "");
        assert!(
            validate_required_case_evidence("H04", CaseStatus::Passed, &missing_focused_root_hide)
                .is_err()
        );
        let readable_refocus = evidence.replace("readable_refocus=none", "readable_refocus=1");
        assert!(
            validate_required_case_evidence("H04", CaseStatus::Passed, &readable_refocus).is_err()
        );
    }

    #[test]
    fn h04_retry_ledger_accepts_only_a_bounded_whole_matrix_retry() {
        let complete = hotkey_matrix_case_evidence(AcceptanceHotkey::ShiftAltWinEnd);
        assert!(validate_required_case_evidence("H04", CaseStatus::Passed, &complete).is_ok());

        let retried = complete
            .replace("matrix_attempts=1", "matrix_attempts=2")
            .replace("contamination_attempts=0", "contamination_attempts=1")
            .replace(
                "contamination_artifacts=none",
                "contamination_artifacts=case-H04-attempt-1-contamination.json",
            );
        assert!(validate_required_case_evidence("H04", CaseStatus::Passed, &retried).is_ok());

        let failed_twice = "evidence:v1; hotkey=Shift+Alt+Win+End; failure=InputInjection: both matrix attempts were contaminated; matrix_attempts=2; contamination_attempts=2; contamination_artifacts=case-H04-attempt-1-contamination.json|case-H04-attempt-2-contamination.json; attempt_restart_state=hidden; full_clean_matrix=false";
        assert!(validate_required_case_evidence("H04", CaseStatus::Failed, failed_twice).is_ok());

        let mut report = acceptance_report("native_windows");
        report.suite = AcceptanceSuite::Hotkey;
        report.hotkey = AcceptanceHotkey::ShiftAltWinEnd;
        report.profile.configured_hotkey = report.hotkey.as_str();
        let mut failed_case = AcceptanceCaseResult {
            id: "H04".into(),
            status: CaseStatus::Failed,
            elapsed_ms: 1,
            expected: "configured hotkey matrix".into(),
            observed: failed_twice.into(),
            failure_stage: Some(FailureStage::InputInjection),
            artifacts: Vec::new(),
        };
        assert!(validate_case_hotkey_profile_relation(&report, &failed_case).is_ok());
        failed_case.observed = failed_case
            .observed
            .replace("hotkey=Shift+Alt+Win+End", "hotkey=F11");
        assert!(validate_case_hotkey_profile_relation(&report, &failed_case).is_err());
        failed_case.observed = failed_twice.replace(
            "evidence:v1; hotkey=Shift+Alt+Win+End; ",
            "InputInjection: ",
        );
        assert!(validate_case_hotkey_profile_relation(&report, &failed_case).is_err());

        let unbounded = complete.replace("matrix_attempts=1", "matrix_attempts=3");
        assert!(validate_required_case_evidence("H04", CaseStatus::Passed, &unbounded).is_err());
        let incomplete = retried.replace(
            "contamination_artifacts=case-H04-attempt-1-contamination.json",
            "contamination_artifacts=none",
        );
        assert!(validate_required_case_evidence("H04", CaseStatus::Passed, &incomplete).is_err());
        let failed_claiming_clean =
            failed_twice.replace("full_clean_matrix=false", "full_clean_matrix=true");
        assert!(
            validate_required_case_evidence("H04", CaseStatus::Failed, &failed_claiming_clean)
                .is_err()
        );
    }

    #[test]
    fn hotkey_case_contract_closes_designer_before_parked_root_case() {
        let designer = HOTKEY_CASE_IDS.iter().position(|id| *id == "H11").unwrap();
        let preview = HOTKEY_CASE_IDS.iter().position(|id| *id == "H12").unwrap();
        let parked_root = HOTKEY_CASE_IDS.iter().position(|id| *id == "H18").unwrap();
        assert!(designer < preview && preview < parked_root);
        assert!(
            required_case_evidence("H11")
                .unwrap()
                .contains(&"designer_cleanup=closed_cleanly")
        );
        assert!(
            required_case_evidence("H12")
                .unwrap()
                .contains(&"designer_cleanup=closed_cleanly")
        );
        assert!(
            required_case_evidence("H12")
                .unwrap()
                .contains(&"preview_baseline_preserved=true")
        );
        assert!(
            required_case_evidence("H12")
                .unwrap()
                .contains(&"runtime_surfaces_new=true")
        );
        assert!(
            required_case_evidence("H16")
                .unwrap()
                .contains(&"root_refreshed_after_direct=true")
        );
    }

    #[test]
    fn h17_report_validates_main_and_opposite_alternate_hotkey_profiles() {
        let mut report = acceptance_report("native_windows");
        report.environment.child_process_id = Some(101);
        let h17 = |observed: String| AcceptanceCaseResult {
            id: "H17".into(),
            status: CaseStatus::Passed,
            elapsed_ms: 1,
            expected: "independent hotkey profile control".into(),
            observed,
            failure_stage: None,
            artifacts: Vec::new(),
        };
        let evidence = |main: &str, alternate: &str, profile_id: &str, child_pid: &str| {
            format!(
                "evidence:v1; f11_control=passed; exact_chord=passed; mouse_gestures=enabled; profile_matches=true; alternate_profile_cleanup=verified; main_profile={main}; alternate_profile={alternate}; alternate_profile_id={profile_id}; alternate_child_pid={child_pid}"
            )
        };

        report.hotkey = AcceptanceHotkey::F11;
        report.profile.configured_hotkey = AcceptanceHotkey::F11.as_str();
        let alternate_profile = tempfile::Builder::new()
            .prefix("radial-acceptance-h17-")
            .tempdir()
            .expect("create H17-shaped alternate profile");
        let alternate_profile_id = alternate_profile
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .expect("alternate profile directory name");
        let f11_main = evidence("F11", "Shift+Alt+Win+End", alternate_profile_id, "202");
        assert!(validate_required_case_evidence("H17", CaseStatus::Passed, &f11_main).is_ok());
        assert!(validate_case_hotkey_profile_relation(&report, &h17(f11_main.clone())).is_ok());

        report.environment.child_process_id = None;
        assert!(validate_case_hotkey_profile_relation(&report, &h17(f11_main.clone())).is_err());
        report.environment.child_process_id = Some(0);
        assert!(validate_case_hotkey_profile_relation(&report, &h17(f11_main.clone())).is_err());
        report.environment.child_process_id = Some(101);

        report.hotkey = AcceptanceHotkey::ShiftAltWinEnd;
        report.profile.configured_hotkey = AcceptanceHotkey::ShiftAltWinEnd.as_str();
        let chord_main = evidence("Shift+Alt+Win+End", "F11", "alternate-profile-02", "303");
        assert!(validate_required_case_evidence("H17", CaseStatus::Passed, &chord_main).is_ok());
        assert!(validate_case_hotkey_profile_relation(&report, &h17(chord_main.clone())).is_ok());

        report.hotkey = AcceptanceHotkey::F11;
        report.profile.configured_hotkey = AcceptanceHotkey::F11.as_str();
        for incomplete in [
            f11_main.replace("main_profile=F11; ", ""),
            f11_main.replace("alternate_profile=Shift+Alt+Win+End; ", ""),
            f11_main.replace(
                &format!("alternate_profile_id={alternate_profile_id}; "),
                "",
            ),
            f11_main.replace("alternate_child_pid=202", ""),
        ] {
            assert!(validate_case_hotkey_profile_relation(&report, &h17(incomplete)).is_err());
        }
        let swapped = evidence("Shift+Alt+Win+End", "F11", "alternate-profile-03", "404");
        assert!(validate_case_hotkey_profile_relation(&report, &h17(swapped)).is_err());
        let same_profile = evidence("F11", "F11", "alternate-profile-04", "505");
        assert!(validate_case_hotkey_profile_relation(&report, &h17(same_profile)).is_err());
        assert!(
            validate_case_hotkey_profile_relation(
                &report,
                &h17(evidence("F11", "Shift+Alt+Win+End", "", "606"))
            )
            .is_err()
        );
        assert!(
            validate_case_hotkey_profile_relation(
                &report,
                &h17(evidence(
                    "F11",
                    "Shift+Alt+Win+End",
                    &"a".repeat(129),
                    "707"
                ))
            )
            .is_err()
        );
        assert!(
            validate_case_hotkey_profile_relation(
                &report,
                &h17(evidence(
                    "F11",
                    "Shift+Alt+Win+End",
                    "alternate-profile-05",
                    "0"
                ))
            )
            .is_err()
        );
        assert!(
            validate_case_hotkey_profile_relation(
                &report,
                &h17(evidence(
                    "F11",
                    "Shift+Alt+Win+End",
                    "alternate-profile-06",
                    "101"
                ))
            )
            .is_err()
        );

        let ordinary = AcceptanceCaseResult {
            id: "H04".into(),
            status: CaseStatus::Passed,
            elapsed_ms: 1,
            expected: "configured hotkey".into(),
            observed: "evidence:v1; hotkey=Shift+Alt+Win+End".into(),
            failure_stage: None,
            artifacts: Vec::new(),
        };
        assert!(validate_case_hotkey_profile_relation(&report, &ordinary).is_err());
    }

    #[test]
    fn all_suite_report_integrity_uses_legacy_case_inventory() {
        let mut report = acceptance_report("native_windows");
        report.suite = AcceptanceSuite::All;
        report.cleanup.child_closed_normally = true;
        report.cleanup.child_owned_windows_closed = true;
        report.cleanup.profile_removed = true;
        report.cleanup.cursor_restored = true;
        report.cleanup.input_desktop_released = true;
        for id in CASE_IDS {
            report.push_case(AcceptanceCaseResult {
                id: id.into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "legacy required case".into(),
                observed: "bounded native evidence".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        assert!(report.passed_native_cases());
        for id in [
            "H01", "H02", "H04", "H06", "H07", "H08", "H09", "H10", "H11", "H12", "H16", "H17",
            "H18",
        ] {
            assert!(!CASE_IDS.contains(&id));
            assert!(HOTKEY_CASE_IDS.contains(&id));
        }
        report.push_case(AcceptanceCaseResult {
            id: "H01".into(),
            status: CaseStatus::Passed,
            elapsed_ms: 1,
            expected: "hotkey-only case".into(),
            observed: "bounded native evidence".into(),
            failure_stage: None,
            artifacts: Vec::new(),
        });
        assert!(!report.passed_native_cases());
    }

    #[test]
    fn hotkey_report_requires_exact_case_capacity_and_cleanup_integrity() {
        let mut report = acceptance_report("native_windows");
        report.suite = AcceptanceSuite::Hotkey;
        report.hotkey = AcceptanceHotkey::ShiftAltWinEnd;
        report.profile.configured_hotkey = "Shift+Alt+Win+End";
        for id in HOTKEY_CASE_IDS {
            let observed = match id {
                "H04" => hotkey_matrix_case_evidence(report.hotkey),
                "CLEANUP" | "R0" => "bounded integrity evidence".into(),
                id => format!(
                    "evidence:v1; hotkey={}; {}",
                    report.hotkey.as_str(),
                    required_case_evidence(id).unwrap().join("; ")
                ),
            };
            assert!(
                validate_required_case_evidence(id, CaseStatus::Passed, &observed).is_ok(),
                "{id}: {observed}"
            );
            report.push_case(AcceptanceCaseResult {
                id: id.into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "bounded acceptance expectation".into(),
                observed,
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        report.cleanup.child_closed_normally = true;
        report.cleanup.child_owned_windows_closed = true;
        report.cleanup.profile_removed = true;
        report.cleanup.cursor_restored = true;
        report.cleanup.input_desktop_released = true;
        assert!(report.passed_native_cases());

        let mut saturated = report.clone();
        while saturated.cases.len() < MAX_CASES - 2 {
            let index = saturated.cases.len();
            saturated.push_case(AcceptanceCaseResult {
                id: format!("filler-{index}"),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "bounded".into(),
                observed: "bounded".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        saturated.push_case(AcceptanceCaseResult {
            id: "overflow".into(),
            status: CaseStatus::Passed,
            elapsed_ms: 1,
            expected: "bounded".into(),
            observed: "bounded".into(),
            failure_stage: None,
            artifacts: Vec::new(),
        });
        assert!(!saturated.passed_native_cases());
        for id in ["CLEANUP", "R0"] {
            assert!(matches!(
                saturated
                    .cases
                    .iter()
                    .find(|case| case.id == id)
                    .unwrap()
                    .status,
                CaseStatus::Failed
            ));
        }

        report.cases.pop();
        assert!(
            !report.passed_native_cases(),
            "missing CLEANUP must fail integrity"
        );
        report.cases.pop();
        report.cases.push(AcceptanceCaseResult {
            id: "CLEANUP".into(),
            status: CaseStatus::Passed,
            elapsed_ms: 1,
            expected: "bounded acceptance expectation".into(),
            observed: "bounded integrity evidence".into(),
            failure_stage: None,
            artifacts: Vec::new(),
        });
        report.cases.push(AcceptanceCaseResult {
            id: "H7".into(),
            status: CaseStatus::Passed,
            elapsed_ms: 1,
            expected: "bounded acceptance expectation".into(),
            observed: hotkey_case_evidence(3, report.hotkey),
            failure_stage: None,
            artifacts: Vec::new(),
        });
        assert!(
            !report.passed_native_cases(),
            "duplicate H7 must fail integrity"
        );
    }

    #[test]
    fn optional_source_revision_derives_git_commit_and_worktree_state_for_both_modes() {
        let parsed = parse(&["--output", "run-derived-source"])
            .expect("documented invocation should derive its source identity");
        let revision = parsed
            .source_revision
            .as_deref()
            .expect("output runs need a source identity");
        assert_eq!(revision.len(), 46);
        assert!(revision[..40].bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(matches!(&revision[40..], "+clean" | "+dirty"));

        let report_mode = parse(&["--report", "run-report.json"])
            .expect("documented report invocation should also derive its source identity");
        assert!(report_mode.report_file.is_some());
        let report_revision = report_mode
            .source_revision
            .as_deref()
            .expect("report mode needs a source identity for R0");
        assert_eq!(report_revision.len(), 46);
        assert!(
            report_revision[..40]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
        assert!(matches!(&report_revision[40..], "+clean" | "+dirty"));

        let supplied = parse(&[
            "--output",
            "run-explicit-source",
            "--source-revision",
            "reviewed-build-id",
        ])
        .expect("explicit source identity should remain supported");
        assert_eq!(
            supplied.source_revision.as_deref(),
            Some("reviewed-build-id")
        );
    }

    #[test]
    fn source_revision_rejects_empty_or_nonprintable_values() {
        assert!(validate_source_revision(String::new()).is_err());
        assert!(validate_source_revision("bad\nidentity".into()).is_err());
        assert!(validate_source_revision("good-id".into()).is_ok());
    }

    #[test]
    fn r0_requires_decisive_native_case_facts_in_untruncated_summaries() {
        for id in ["D2", "A2", "G1", "A3", "A5", "A6", "A7", "D3", "D6", "D7"] {
            let required = required_case_evidence(id).expect("required case evidence exists");
            let observed = format!("evidence:v1; {}", required.join("; "));
            assert!(
                validate_required_case_evidence(id, CaseStatus::Passed, &observed).is_ok(),
                "valid evidence was rejected for {id}"
            );
            assert!(
                validate_required_case_evidence(id, CaseStatus::Passed, &observed[..12]).is_err(),
                "truncated evidence was accepted for {id}"
            );
            let incomplete = format!("evidence:v1; {}", required[1..].join("; "));
            assert!(
                validate_required_case_evidence(id, CaseStatus::Passed, &incomplete).is_err(),
                "summary with a missing decisive fact was accepted for {id}"
            );
            assert!(
                validate_required_case_evidence(id, CaseStatus::Failed, &observed).is_err(),
                "failed case was accepted for {id}"
            );
        }
        assert!(
            validate_required_case_evidence(
                "D7",
                CaseStatus::Passed,
                &"x".repeat(MAX_RESULT_BYTES)
            )
            .is_err()
        );
    }

    #[test]
    fn h6_repeat_mode_rejects_unknown_values() {
        let error = parse(&["--output", "run", "--h6-repeat", "retry"]).unwrap_err();
        assert!(error.contains("production-only-diagnostic"));
    }

    #[test]
    fn disabled_mouse_gesture_diagnostic_is_written_to_isolated_profile() {
        let fixture = deterministic_fixture(
            Path::new("acceptance.log"),
            MouseGestureMode::DisabledDiagnostic,
        )
        .unwrap();
        let settings: Settings = serde_json::from_slice(&fixture.settings_json).unwrap();
        assert_eq!(
            settings.plugin_settings["mouse_gestures"]["enabled"],
            serde_json::Value::Bool(false)
        );
    }

    #[test]
    fn mouse_gesture_mode_rejects_unknown_values() {
        let error = parse(&["--output", "run", "--mouse-gestures", "off"]).unwrap_err();
        assert!(error.contains("--mouse-gestures must be"));
    }

    #[test]
    fn profile_copy_option_is_retained_without_changing_deterministic_defaults() {
        let arguments = parse(&["--output", "run", "--profile-copy", "supplied-profile"]).unwrap();
        assert_eq!(
            arguments.profile_copy,
            Some(PathBuf::from("supplied-profile"))
        );
        assert_eq!(arguments.h6_repeat_mode, H6RepeatMode::Quiescent);
        assert_eq!(arguments.mouse_gesture_mode, MouseGestureMode::Enabled);

        let without_copy = parse(&["--output", "run"]).unwrap();
        assert!(without_copy.profile_copy.is_none());
    }

    #[test]
    fn copied_case_capacity_and_dual_report_gate_are_explicit() {
        let copied = passing_copied_report();
        assert_eq!(copied.cases.len(), COPIED_CASE_IDS.len());
        assert!(copied.passed());
        assert!(!copied.capacity_saturated);

        assert!(aggregate_native_cases_passed(true, None, true));
        assert!(!aggregate_native_cases_passed(false, None, true));
        assert!(aggregate_native_cases_passed(true, Some(&copied), true));
        assert!(!aggregate_native_cases_passed(true, Some(&copied), false));

        let mut failed_copy = copied.clone();
        failed_copy.cases[0].status = CaseStatus::Failed;
        failed_copy.cases[0].failure_stage = Some(FailureStage::Environment);
        failed_copy.copied_profile_status = CopiedProfileStatus::Failed;
        assert!(!aggregate_native_cases_passed(
            true,
            Some(&failed_copy),
            true
        ));

        let mut saturated = copied;
        for index in 0..MAX_CASES {
            saturated.push_case(AcceptanceCaseResult {
                id: format!("extra-{index}"),
                status: CaseStatus::Passed,
                elapsed_ms: 0,
                expected: "bounded".into(),
                observed: "bounded".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        assert!(saturated.capacity_saturated);
        assert!(!aggregate_native_cases_passed(true, Some(&saturated), true));
    }

    #[cfg(windows)]
    #[test]
    fn copied_report_publication_failure_blocks_aggregate_pass_without_partial_json() {
        let output = tempfile::tempdir().unwrap();
        let deterministic_path = output.path().join("report.json");
        let copied_path = copied_report_path(&deterministic_path);
        let copied_text_path = copied_path.with_extension("txt");
        fs::write(&copied_text_path, "owned conflict sentinel").unwrap();
        let mut copied = passing_copied_report();

        assert!(persist_copied_report_pair(&copied_path, &mut copied).is_err());
        assert!(!copied_path.exists());
        assert_eq!(
            fs::read_to_string(copied_text_path).unwrap(),
            "owned conflict sentinel"
        );
        assert!(!aggregate_native_cases_passed(true, Some(&copied), false));
        assert!(fs::read_dir(output.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".radial-acceptance-report-stage-")
        }));
    }

    #[test]
    fn copied_report_sanitization_removes_private_paths_and_raw_observations() {
        let private_marker = "private copied UIA value and screenshot path";
        let mut report = acceptance_report("native_windows_copied_profile");
        report.copied_profile_status = CopiedProfileStatus::Failed;
        report.profile.temporary_data_root = private_marker.into();
        report.artifacts.push(private_marker.into());
        report.cases.push(AcceptanceCaseResult {
            id: "CP_D0".into(),
            status: CaseStatus::Failed,
            elapsed_ms: 1,
            expected: "bounded expectation".into(),
            observed: private_marker.into(),
            failure_stage: Some(FailureStage::DesignerReadiness),
            artifacts: vec![private_marker.into()],
        });
        sanitize_copied_report(&mut report);

        let json = serde_json::to_string(&report).unwrap();
        let text = render_text_report(&report);
        assert!(!json.contains(private_marker));
        assert!(!text.contains(private_marker));
        assert!(report.artifacts.is_empty());
        assert!(report.cases[0].artifacts.is_empty());
        assert_eq!(
            report.profile.temporary_data_root,
            "private copied-profile temporary directory"
        );
    }

    #[test]
    fn copied_style_sanitization_preserves_typed_values_and_relations() {
        let private_marker = "copied private menu title and path";
        let mut report = acceptance_report("native_windows_copied_profile");
        report.cases = vec![
            AcceptanceCaseResult {
                id: "CP_A5".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "preview style transition".into(),
                observed: format!(
                    "{private_marker}; evidence:v1; glow=true->false; preview_reply=accepted; preview_rendered=true"
                ),
                failure_stage: None,
                artifacts: vec![private_marker.into()],
            },
            AcceptanceCaseResult {
                id: "CP_A6".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "saved copied style".into(),
                observed: format!(
                    "{private_marker}; evidence:v1; typed_radial=decoded; authored_geometry=[8,10]; action_binding=true; after_action=close_tree; original_menus_preserved=true; glow=false; reopened=true"
                ),
                failure_stage: None,
                artifacts: Vec::new(),
            },
            AcceptanceCaseResult {
                id: "CP_A7".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "saved-style Undo and Redo".into(),
                observed: format!(
                    "{private_marker}; evidence:v1; undo_restored=false; redo_restored=true"
                ),
                failure_stage: None,
                artifacts: Vec::new(),
            },
        ];

        sanitize_copied_report(&mut report);
        for case in &report.cases {
            validate_required_case_evidence(&case.id, case.status, &case.observed).unwrap();
        }
        validate_copied_style_evidence_relations(&report).unwrap();
        let public = serde_json::to_string(&report).unwrap();
        assert!(!public.contains(private_marker));
        assert_eq!(
            report.cases[0].observed,
            "evidence:v1; glow=true->false; preview_reply=accepted; preview_rendered=true"
        );
        assert_eq!(
            report.cases[2].observed,
            "evidence:v1; undo_restored=false; redo_restored=true"
        );

        report.cases[2].observed = "evidence:v1; undo_restored=true; redo_restored=false".into();
        assert!(
            validate_copied_style_evidence_relations(&report)
                .unwrap_err()
                .contains("style edit, saved value, Undo, and Redo evidence disagree")
        );
    }

    #[test]
    fn copied_pointer_and_disposable_evidence_survive_privacy_sanitization() {
        let private_marker = "private copied profile path and close detail";
        let mut report = acceptance_report("native_windows_copied_profile");
        report.cases = vec![
            AcceptanceCaseResult {
                id: "CP_D1".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "checked copied Tree navigation".into(),
                observed: format!(
                    "{private_marker}; evidence:v1; tree_round_trip=true; checked_pointer=true; tree_selected=true"
                ),
                failure_stage: None,
                artifacts: Vec::new(),
            },
            AcceptanceCaseResult {
                id: "CP_D7".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "cancelled late disposable preview".into(),
                observed: format!(
                    "{private_marker}; evidence:v1; pending_request=true; request_id=42; generation=9; cancelled_before_prompt=true; late_reply=rejected; stop=accepted; stop_request=43; no_reopen=1s; marker_clean=true; child_alive=true"
                ),
                failure_stage: None,
                artifacts: Vec::new(),
            },
        ];

        sanitize_copied_report(&mut report);
        for case in &report.cases {
            validate_required_case_evidence(&case.id, case.status, &case.observed).unwrap();
        }
        let public = serde_json::to_string(&report).unwrap();
        assert!(!public.contains(private_marker));
        assert_eq!(
            report.cases[0].observed,
            "evidence:v1; tree_round_trip=true; checked_pointer=true; tree_selected=true"
        );
        assert_eq!(
            report.cases[1].observed,
            "evidence:v1; pending_request=true; cancelled_before_prompt=true; late_reply=rejected; stop=accepted; no_reopen=1s; marker_clean=true"
        );
    }

    #[test]
    fn copied_private_artifact_evidence_is_typed_and_path_free() {
        let marker = "C:\\Users\\profile\\private trace contents";
        let summary = private_artifacts::PrivateArtifactSummary {
            status: private_artifacts::PrivateArtifactStatus::Retained,
            artifact_id: Some("multi-launcher-private-evidence-test-5678".into()),
            file_count: 4,
            total_bytes: 512,
            manifest_sha256: Some("a".repeat(64)),
        };
        let mut report = acceptance_report("native_windows_copied_profile");
        report.private_artifacts = Some(summary.clone());
        report.cases.push(AcceptanceCaseResult {
            id: "CP_R1".into(),
            status: CaseStatus::Passed,
            elapsed_ms: 1,
            expected: "private durable copied-profile evidence".into(),
            observed: format!("{marker}; {}", private_artifact_evidence(&summary)),
            failure_stage: None,
            artifacts: vec![marker.into()],
        });

        sanitize_copied_report(&mut report);
        validate_required_case_evidence("CP_R1", report.cases[0].status, &report.cases[0].observed)
            .unwrap();
        let public = serde_json::to_string(&report).unwrap();
        let text = render_text_report(&report);
        assert!(!public.contains(marker));
        assert!(!text.contains(marker));
        assert!(public.contains("multi-launcher-private-evidence-test-5678"));
        assert!(public.contains(&"a".repeat(64)));
        assert_eq!(
            report.cases[0].observed,
            format!(
                "evidence:v1; artifact_retention=retained; artifact_count=4; artifact_bytes=512; artifact_id=multi-launcher-private-evidence-test-5678; artifact_sha256={}",
                "a".repeat(64)
            )
        );
    }

    #[cfg(windows)]
    #[test]
    fn copied_ephemeral_artifact_evidence_is_typed_and_has_no_retained_id() {
        let summary = private_artifacts::PrivateArtifactSummary {
            status: private_artifacts::PrivateArtifactStatus::EphemeralValidated,
            artifact_id: None,
            file_count: 4,
            total_bytes: 512,
            manifest_sha256: Some("b".repeat(64)),
        };
        let mut report = acceptance_report("native_windows_copied_profile");
        report.private_artifacts = Some(summary.clone());
        report.cases.push(AcceptanceCaseResult {
            id: "CP_R1".into(),
            status: CaseStatus::Passed,
            elapsed_ms: 1,
            expected: "validated ephemeral copied-profile evidence".into(),
            observed: private_artifact_evidence(&summary),
            failure_stage: None,
            artifacts: Vec::new(),
        });

        sanitize_copied_report(&mut report);
        validate_required_case_evidence("CP_R1", report.cases[0].status, &report.cases[0].observed)
            .unwrap();
        validate_private_artifact_report(&report).unwrap();
        let public = serde_json::to_string(&report).unwrap();
        assert!(public.contains("artifact_retention=ephemeral"));
        assert!(public.contains("artifact_id=none"));
        assert!(!public.contains("multi-launcher-private-evidence-"));
    }

    #[cfg(windows)]
    #[test]
    fn copied_r0_status_contract_matches_final_report_state() {
        let source_tree = "1".repeat(64);
        let source_settings = "2".repeat(64);
        let source_radial = "3".repeat(64);
        let source_actions = "4".repeat(64);
        let launch_settings = "5".repeat(64);
        let launch_radial = "6".repeat(64);
        let launch_actions = "7".repeat(64);
        let summary = CopiedProfileSummary {
            source_tree_sha256_before: source_tree.clone(),
            copied_initial_tree_sha256: source_tree.clone(),
            source_tree_sha256_after: Some(source_tree),
            copied_file_count: 3,
            copied_total_bytes: 100,
            source_settings_sha256: source_settings.clone(),
            source_radial_sha256: source_radial.clone(),
            source_actions_sha256: Some(source_actions.clone()),
            copied_initial_settings_sha256: source_settings,
            copied_initial_radial_sha256: source_radial,
            copied_initial_actions_sha256: Some(source_actions),
            launch_settings_sha256: launch_settings.clone(),
            launch_radial_sha256: launch_radial.clone(),
            launch_actions_sha256: launch_actions.clone(),
            source_unchanged: true,
        };
        let mut report = acceptance_report("native_windows_copied_profile");
        report.profile.settings_sha256 = launch_settings;
        report.profile.radial_sha256 = launch_radial;
        report.profile.actions_sha256 = launch_actions;
        report.copied_profile = Some(summary.clone());
        report.copied_profile_status = CopiedProfileStatus::Passed;
        for id in COPIED_CASE_IDS.into_iter().filter(|id| *id != "R0") {
            report.cases.push(AcceptanceCaseResult {
                id: id.into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "expected".into(),
                observed: "evidence:v1; bounded=true".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        assert!(copied_status_contract_is_valid(&report));

        let mut absent_actions_summary = summary.clone();
        absent_actions_summary.source_actions_sha256 = None;
        absent_actions_summary.copied_initial_actions_sha256 = None;
        absent_actions_summary.copied_file_count = 2;
        let serialized_summary = serde_json::to_value(&absent_actions_summary).unwrap();
        assert!(serialized_summary["source_actions_sha256"].is_null());
        assert!(serialized_summary["copied_initial_actions_sha256"].is_null());
        report.copied_profile = Some(absent_actions_summary.clone());
        assert!(validate_copied_profile_summary(&report, &absent_actions_summary).is_ok());
        assert!(copied_status_contract_is_valid(&report));

        let mut mismatched_actions_summary = absent_actions_summary.clone();
        mismatched_actions_summary.source_actions_sha256 = Some("4".repeat(64));
        report.copied_profile = Some(mismatched_actions_summary);
        assert!(!copied_status_contract_is_valid(&report));

        let mut malformed_actions_summary = absent_actions_summary.clone();
        malformed_actions_summary.source_actions_sha256 = Some("malformed".into());
        malformed_actions_summary.copied_initial_actions_sha256 = Some("malformed".into());
        report.copied_profile = Some(malformed_actions_summary);
        assert!(!copied_status_contract_is_valid(&report));

        report.copied_profile = Some(summary.clone());

        report.cases[1].status = CaseStatus::Failed;
        report.cases[1].failure_stage = Some(FailureStage::Environment);
        report.copied_profile_status = CopiedProfileStatus::Failed;
        assert!(copied_status_contract_is_valid(&report));

        report.cases[1].status = CaseStatus::Passed;
        report.cases[1].failure_stage = None;
        assert!(!copied_status_contract_is_valid(&report));
        report.copied_profile_status = CopiedProfileStatus::Running;
        assert!(!copied_status_contract_is_valid(&report));

        let mut aggregate = acceptance_report("native_windows");
        assert!(copied_status_contract_is_valid(&aggregate));
        aggregate.copied_profile_status = CopiedProfileStatus::Running;
        assert!(copied_status_contract_is_valid(&aggregate));
        aggregate.copied_profile = Some(summary);
        aggregate.copied_profile_status = CopiedProfileStatus::Passed;
        assert!(copied_status_contract_is_valid(&aggregate));
        aggregate.copied_profile_status = CopiedProfileStatus::Failed;
        assert!(copied_status_contract_is_valid(&aggregate));
    }

    #[cfg(windows)]
    #[test]
    fn copied_profile_normalization_changes_only_the_private_copy() {
        let source = tempfile::tempdir().unwrap();
        let fixture = deterministic_fixture(
            &source.path().join("acceptance.log"),
            MouseGestureMode::Enabled,
        )
        .unwrap();
        fs::write(source.path().join("settings.json"), &fixture.settings_json).unwrap();
        fs::write(source.path().join("radial.json"), &fixture.radial_json).unwrap();
        fs::write(source.path().join("actions.json"), &fixture.actions_json).unwrap();
        let source_inventory = copied_profile::ProfileInventory::scan(source.path()).unwrap();
        let original_settings = source_inventory
            .file_hash("settings.json")
            .unwrap()
            .to_owned();
        let original_radial = source_inventory
            .file_hash("radial.json")
            .unwrap()
            .to_owned();
        let original_actions = source_inventory
            .file_hash("actions.json")
            .unwrap()
            .to_owned();
        let copy = tempfile::tempdir().unwrap();

        let metadata = prepare_copied_profile(&source_inventory, copy.path()).unwrap();
        assert!(source_inventory.still_matches_source());
        assert_eq!(metadata.source_settings_sha256, original_settings);
        assert_eq!(metadata.source_radial_sha256, original_radial);
        assert_eq!(
            metadata.source_actions_sha256.as_deref(),
            Some(original_actions.as_str())
        );
        assert_eq!(metadata.copied_initial_settings_sha256, original_settings);
        assert_eq!(metadata.copied_initial_radial_sha256, original_radial);
        assert_eq!(
            metadata.copied_initial_actions_sha256.as_deref(),
            Some(original_actions.as_str())
        );
        assert_ne!(
            metadata.launch_settings_sha256,
            metadata.copied_initial_settings_sha256
        );
        assert_eq!(
            metadata.launch_radial_sha256,
            metadata.copied_initial_radial_sha256
        );
        assert_ne!(
            metadata.launch_actions_sha256,
            metadata.copied_initial_actions_sha256.as_deref().unwrap()
        );

        let mut settings = match Settings::load_typed(&copy.path().join("settings.json")).unwrap() {
            LoadState::Loaded(settings) => settings,
            LoadState::Missing | LoadState::Empty => panic!("normalized settings must load"),
        };
        let suite_trace_path = copied_profile_trace_path(copy.path());
        assert!(matches!(
            settings.log_file.as_ref(),
            Some(LogFile::Path(path)) if Path::new(path) == suite_trace_path
        ));
        assert_eq!(settings.hotkey.as_deref(), Some(ACCEPTANCE_HOTKEY));
        assert!(settings.help_hotkey.is_none());
        assert!(settings.quit_hotkey.is_none());
        assert!(settings.index_paths.is_none());
        assert!(settings.plugin_dirs.is_none());
        assert!(!settings.radial.global_item_inputs);
        assert_eq!(
            settings.plugin_settings.get("clipboard_modify"),
            Some(
                &serde_json::to_value(
                    multi_launcher::settings::ClipboardModifyPluginSettings::default()
                )
                .unwrap()
            )
        );
        assert_eq!(
            settings.enabled_plugins.as_ref().unwrap(),
            &std::collections::HashSet::from(["radial".to_string()])
        );
        assert!(!multi_launcher::plugins::clipboard_modify::migrate_enablement(&mut settings));
        assert_eq!(
            settings.enabled_plugins.as_ref().unwrap(),
            &std::collections::HashSet::from(["radial".to_string()])
        );
        validate_copied_profile_after_run(copy.path(), &metadata).unwrap();

        let mut migrated_settings = settings.clone();
        migrated_settings.plugin_settings.remove("clipboard_modify");
        assert!(
            multi_launcher::plugins::clipboard_modify::migrate_enablement(&mut migrated_settings)
        );
        fs::write(
            copy.path().join("settings.json"),
            serde_json::to_vec_pretty(&migrated_settings).unwrap(),
        )
        .unwrap();
        let migration_audit = validate_copied_profile_after_run(copy.path(), &metadata)
            .expect_err("startup migration must not enable Clipboard Modify in the copy");
        assert!(migration_audit.contains("enabled_plugins"));
        assert!(!settings.multi_manager.enabled);
        assert!(Path::new(&settings.multi_manager.workspaces_path).starts_with(copy.path()));
        assert!(Path::new(&settings.multi_manager.bindings_path).starts_with(copy.path()));

        let radial = sha256_file(&copy.path().join("radial.json")).unwrap();
        assert_eq!(radial, original_radial);
        let actions =
            multi_launcher::actions::load_actions_typed(copy.path().join("actions.json")).unwrap();
        let LoadState::Loaded(actions) = actions else {
            panic!("copied actions must load after adding inert entries");
        };
        let inert = &actions[metadata.target_action_index];
        assert!(inert.label.contains("Radial Acceptance Harmless Action"));
        assert!(inert.action.starts_with("radial_acceptance_inert_"));
        assert!(source_inventory.still_matches_source());
    }

    #[cfg(windows)]
    #[test]
    fn copied_profile_accepts_missing_and_empty_actions_without_touching_source() {
        let fixture_dir = tempfile::tempdir().unwrap();
        let fixture = deterministic_fixture(
            &fixture_dir.path().join("acceptance.log"),
            MouseGestureMode::Enabled,
        )
        .unwrap();

        for actions_present in [false, true] {
            let source = tempfile::tempdir().unwrap();
            fs::write(source.path().join("settings.json"), &fixture.settings_json).unwrap();
            fs::write(source.path().join("radial.json"), &fixture.radial_json).unwrap();
            if actions_present {
                fs::write(source.path().join("actions.json"), b"").unwrap();
            }
            let source_inventory = copied_profile::ProfileInventory::scan(source.path()).unwrap();
            let expected_actions_sha256 = actions_present.then(|| sha256_bytes(b""));
            assert_eq!(
                source_inventory
                    .file_hash("actions.json")
                    .map(str::to_owned),
                expected_actions_sha256
            );

            let untouched_copy = tempfile::tempdir().unwrap();
            let initial_copy = source_inventory.copy_to(untouched_copy.path()).unwrap();
            assert_eq!(initial_copy.tree_sha256, source_inventory.tree_sha256);
            assert_eq!(initial_copy.file_count, if actions_present { 3 } else { 2 });
            assert_eq!(
                initial_copy.file_hash("actions.json").map(str::to_owned),
                expected_actions_sha256
            );
            assert_eq!(
                untouched_copy.path().join("actions.json").exists(),
                actions_present
            );

            let copy = tempfile::tempdir().unwrap();
            let metadata = prepare_copied_profile(&source_inventory, copy.path()).unwrap();
            assert_eq!(metadata.source_actions_sha256, expected_actions_sha256);
            assert_eq!(
                metadata.copied_initial_actions_sha256,
                expected_actions_sha256
            );
            assert_eq!(
                metadata.initial_copy_tree_sha256,
                source_inventory.tree_sha256
            );
            assert!(source_inventory.still_matches_source());
            assert_eq!(source.path().join("actions.json").exists(), actions_present);

            let LoadState::Loaded(actions) =
                multi_launcher::actions::load_actions_typed(copy.path().join("actions.json"))
                    .unwrap()
            else {
                panic!("copied acceptance action catalog must load as typed actions");
            };
            assert_eq!(actions.len(), ACCEPTANCE_ACTION_COUNT);
            assert!(actions.iter().all(|action| {
                action
                    .label
                    .starts_with("Radial Acceptance Harmless Action ")
                    && action.action.starts_with("radial_acceptance_inert_")
                    && action.args.is_none()
            }));
            assert_eq!(
                metadata.target_action_index,
                actions.len().saturating_sub(1)
            );
            validate_copied_profile_after_run(copy.path(), &metadata).unwrap();

            let source_after = copied_profile::ProfileInventory::scan(source.path()).unwrap();
            assert_eq!(source_after, source_inventory);
            let summary = metadata.report_summary(
                &source_inventory,
                Some(source_after.tree_sha256.clone()),
                true,
            );
            assert_eq!(summary.source_actions_sha256, expected_actions_sha256);
            assert_eq!(
                summary.copied_initial_actions_sha256,
                expected_actions_sha256
            );
            let mut report = acceptance_report("native_windows_copied_profile");
            report.profile.settings_sha256 = metadata.launch_settings_sha256.clone();
            report.profile.radial_sha256 = metadata.launch_radial_sha256.clone();
            report.profile.actions_sha256 = metadata.launch_actions_sha256.clone();
            assert!(validate_copied_profile_summary(&report, &summary).is_ok());
        }
    }

    #[cfg(windows)]
    #[test]
    fn copied_profile_rejects_invalid_typed_settings_radial_and_actions() {
        let fixture_dir = tempfile::tempdir().unwrap();
        let fixture = deterministic_fixture(
            &fixture_dir.path().join("acceptance.log"),
            MouseGestureMode::Enabled,
        )
        .unwrap();
        for invalid_name in ["settings.json", "radial.json", "actions.json"] {
            let source = tempfile::tempdir().unwrap();
            fs::write(source.path().join("settings.json"), &fixture.settings_json).unwrap();
            fs::write(source.path().join("radial.json"), &fixture.radial_json).unwrap();
            fs::write(source.path().join("actions.json"), &fixture.actions_json).unwrap();
            fs::write(source.path().join(invalid_name), b"{ invalid json").unwrap();
            let inventory = copied_profile::ProfileInventory::scan(source.path()).unwrap();
            let copy = tempfile::tempdir().unwrap();
            assert!(prepare_copied_profile(&inventory, copy.path()).is_err());
            assert!(inventory.still_matches_source());
        }
    }

    #[cfg(windows)]
    #[test]
    fn copied_profile_disables_global_shortcuts_and_preserves_stable_migration_receipts() {
        for state in [
            SubmenuMigrationState::Applied,
            SubmenuMigrationState::Undone,
        ] {
            let source = tempfile::tempdir().unwrap();
            let fixture = deterministic_fixture(
                &source.path().join("acceptance.log"),
                MouseGestureMode::Enabled,
            )
            .unwrap();
            let mut settings: Settings = serde_json::from_slice(&fixture.settings_json).unwrap();
            settings.radial.global_item_inputs = true;
            let receipt = migration_receipt(state);
            settings.radial_submenu_migration = Some(receipt.clone());
            fs::write(
                source.path().join("settings.json"),
                serde_json::to_vec_pretty(&settings).unwrap(),
            )
            .unwrap();
            fs::write(source.path().join("radial.json"), &fixture.radial_json).unwrap();
            fs::write(source.path().join("actions.json"), &fixture.actions_json).unwrap();
            let source_inventory = copied_profile::ProfileInventory::scan(source.path()).unwrap();
            let copy = tempfile::tempdir().unwrap();

            let metadata = prepare_copied_profile(&source_inventory, copy.path()).unwrap();
            let normalized = match Settings::load_typed(&copy.path().join("settings.json")).unwrap()
            {
                LoadState::Loaded(settings) => settings,
                LoadState::Missing | LoadState::Empty => {
                    panic!("normalized settings must remain a typed object")
                }
            };
            assert!(!normalized.radial.global_item_inputs);
            assert_eq!(normalized.radial_submenu_migration, Some(receipt.clone()));
            assert_eq!(metadata.expected_submenu_migration_receipt, Some(receipt));
            assert_eq!(
                sha256_file(&copy.path().join("radial.json")).unwrap(),
                source_inventory.file_hash("radial.json").unwrap()
            );
            validate_copied_profile_after_run(copy.path(), &metadata).unwrap();

            let original = match Settings::load_typed(&source.path().join("settings.json")).unwrap()
            {
                LoadState::Loaded(settings) => settings,
                LoadState::Missing | LoadState::Empty => {
                    panic!("source settings must remain a typed object")
                }
            };
            assert!(original.radial.global_item_inputs);
            assert_eq!(original.radial_submenu_migration.unwrap().state, state);
            assert!(source_inventory.still_matches_source());
        }
    }

    #[cfg(windows)]
    #[test]
    fn copied_profile_rejects_unfinished_migration_receipts_before_normalization() {
        for state in [
            SubmenuMigrationState::Prepared,
            SubmenuMigrationState::UndoPrepared,
        ] {
            let source = tempfile::tempdir().unwrap();
            let fixture = deterministic_fixture(
                &source.path().join("acceptance.log"),
                MouseGestureMode::Enabled,
            )
            .unwrap();
            let mut settings: Settings = serde_json::from_slice(&fixture.settings_json).unwrap();
            let receipt = migration_receipt(state);
            settings.radial_submenu_migration = Some(receipt.clone());
            fs::write(
                source.path().join("settings.json"),
                serde_json::to_vec_pretty(&settings).unwrap(),
            )
            .unwrap();
            fs::write(source.path().join("radial.json"), &fixture.radial_json).unwrap();
            fs::write(source.path().join("actions.json"), &fixture.actions_json).unwrap();
            let source_inventory = copied_profile::ProfileInventory::scan(source.path()).unwrap();
            let copy = tempfile::tempdir().unwrap();

            let error = prepare_copied_profile(&source_inventory, copy.path()).unwrap_err();
            assert!(error.contains("unfinished submenu migration receipt"));
            let unchanged_copy =
                match Settings::load_typed(&copy.path().join("settings.json")).unwrap() {
                    LoadState::Loaded(settings) => settings,
                    LoadState::Missing | LoadState::Empty => {
                        panic!("rejected copy still retains source settings for diagnosis")
                    }
                };
            assert_eq!(unchanged_copy.radial_submenu_migration, Some(receipt));
            assert!(source_inventory.still_matches_source());
        }
    }

    #[test]
    fn report_capacity_saturation_is_explicit_and_fails_r0() {
        let mut report = AcceptanceReport {
            schema_version: 7,
            run_id: "test".into(),
            mode: "test",
            started_unix_ms: 1,
            finished_unix_ms: 2,
            copied_profile_status: CopiedProfileStatus::NotRun,
            copied_profile: None,
            private_artifacts: None,
            h6_repeat_mode: H6RepeatMode::Quiescent,
            mouse_gesture_mode: MouseGestureMode::Enabled,
            suite: AcceptanceSuite::All,
            hotkey: AcceptanceHotkey::F11,
            outcome: "running",
            candidate: CandidateIdentity {
                executable: "candidate.exe".into(),
                sha256: "a".repeat(64),
            },
            environment: EnvironmentIdentity {
                os_version: "test".into(),
                architecture: "x64".into(),
                runner_process_id: 1,
                runner_sha256: None,
                child_process_id: None,
                child_started_unix_ms: None,
                source_revision: None,
                monitors: Vec::new(),
            },
            profile: ProfileIdentity {
                mode: "test",
                temporary_data_root: "profile".into(),
                settings_sha256: "b".repeat(64),
                radial_sha256: "c".repeat(64),
                actions_sha256: "d".repeat(64),
                configured_hotkey: ACCEPTANCE_HOTKEY,
                hold_threshold_ms: 1,
            },
            cases: vec![AcceptanceCaseResult {
                id: "R0".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 0,
                expected: "valid report".into(),
                observed: "valid report".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            }],
            hotkey_evidence: Vec::new(),
            artifacts: Vec::new(),
            cleanup: CleanupResult::default(),
            capacity_saturated: false,
            report_overflow: None,
        };
        for index in 0..61 {
            report.push_artifact(format!("artifact-{index}"));
        }
        assert_eq!(report.artifacts.len(), 61);
        assert!(!report.capacity_saturated);
        for index in 61..=MAX_ARTIFACTS {
            report.push_artifact(format!("artifact-{index}"));
        }
        assert_eq!(report.artifacts.len(), MAX_ARTIFACTS);
        assert!(report.capacity_saturated);
        assert!(!report.passed());
        assert!(report.cases[0].observed.contains("capacity was exceeded"));
        assert!(matches!(report.cases[0].status, CaseStatus::Failed));

        report.cases.clear();
        report.capacity_saturated = false;
        for _ in 0..MAX_CASES - 2 {
            report.push_case(AcceptanceCaseResult {
                id: "overflow".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 0,
                expected: "bounded".into(),
                observed: "bounded".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        assert_eq!(report.cases.len(), MAX_CASES - 2);
        for id in ["CLEANUP", "R0"] {
            report.push_case(AcceptanceCaseResult {
                id: id.into(),
                status: CaseStatus::Passed,
                elapsed_ms: 0,
                expected: "reserved evidence".into(),
                observed: "reserved evidence".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        assert_eq!(report.cases.len(), MAX_CASES);
        assert!(!report.capacity_saturated);
        report.push_case(AcceptanceCaseResult {
            id: "overflow-after-reserved-slots".into(),
            status: CaseStatus::Passed,
            elapsed_ms: 0,
            expected: "bounded".into(),
            observed: "bounded".into(),
            failure_stage: None,
            artifacts: Vec::new(),
        });
        assert_eq!(report.cases.len(), MAX_CASES);
        assert!(report.capacity_saturated);
        for id in ["CLEANUP", "R0"] {
            let case = report.cases.iter().find(|case| case.id == id).unwrap();
            assert!(matches!(case.status, CaseStatus::Failed));
            assert!(case.observed.contains("capacity was exceeded"));
        }
    }

    #[test]
    fn oversized_report_is_compacted_and_persisted_as_a_failed_report() {
        let mut report = acceptance_report("native_windows");
        report.suite = AcceptanceSuite::Hotkey;
        report.hotkey = AcceptanceHotkey::ShiftAltWinEnd;
        report.profile.configured_hotkey = "Shift+Alt+Win+End";
        report.outcome = "passed";
        let oversized_path = "\\".repeat(MAX_PATH_BYTES);
        let oversized_result = "\\".repeat(MAX_RESULT_BYTES);
        for id in HOTKEY_CASE_IDS {
            report.push_case(AcceptanceCaseResult {
                id: id.into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: oversized_result.clone(),
                observed: oversized_result.clone(),
                failure_stage: None,
                artifacts: vec![oversized_path.clone(); 8],
            });
        }
        report.artifacts = vec![oversized_path; MAX_ARTIFACTS];
        report.cleanup.child_closed_normally = true;
        report.cleanup.child_owned_windows_closed = true;
        report.cleanup.profile_removed = true;
        report.cleanup.cursor_restored = true;
        report.cleanup.input_desktop_released = true;
        assert!(report_serialized_sizes(&report).unwrap().0 > MAX_JSON_REPORT_BYTES);

        let output = tempfile::tempdir().unwrap();
        let json_path = output.path().join("report.json");
        let text_path = output.path().join("report.txt");
        write_report(&json_path, &mut report).unwrap();
        write_text_report(&text_path, &report).unwrap();

        assert!(report.capacity_saturated);
        assert_eq!(report.outcome, "failed");
        assert!(!report.passed_native_cases());
        for id in ["CLEANUP", "R0"] {
            assert!(matches!(
                report
                    .cases
                    .iter()
                    .find(|case| case.id == id)
                    .unwrap()
                    .status,
                CaseStatus::Failed
            ));
        }
        assert!(fs::metadata(&json_path).unwrap().len() as usize <= MAX_JSON_REPORT_BYTES);
        assert!(fs::metadata(&text_path).unwrap().len() as usize <= MAX_TEXT_REPORT_BYTES);
        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(&json_path).unwrap()).unwrap();
        assert_eq!(persisted["capacity_saturated"], true);
        assert_eq!(persisted["outcome"], "failed");
        assert!(
            fs::read_to_string(&text_path)
                .unwrap()
                .contains("Radial native acceptance: failed")
        );
    }

    #[test]
    fn oversized_hotkey_packets_write_bounded_failed_reports_with_overflow_receipt() {
        let mut report = acceptance_report("native_windows");
        report.suite = AcceptanceSuite::Hotkey;
        report.hotkey = AcceptanceHotkey::F11;
        report.profile.configured_hotkey = "F11";
        report.outcome = "passed";
        let case_ids = [
            "H01", "H02", "H04", "H06", "H07", "H08", "H09", "H10", "H11", "H12", "H16", "H17",
            "H18",
        ];
        for id in case_ids {
            let mut packet = h01_evidence_packet();
            packet.case_id = id.into();
            packet.expected_state = hotkey_expected_state(id).unwrap();
            packet.runner_edges = (0..MAX_HOTKEY_EVIDENCE_EDGES)
                .map(|index| HotkeyRunnerEdgeEvidence {
                    runner_relative_us: (index as u64).saturating_mul(50),
                    input_group_id: 1,
                    stream: HotkeyCandidateStream::MainCandidate,
                    purpose: HotkeyRunnerInputPurpose::LauncherChord,
                    virtual_key: 0x7A,
                    transition: if index % 2 == 0 {
                        HotkeyEdgeTransition::Press
                    } else {
                        HotkeyEdgeTransition::Release
                    },
                    injected: true,
                    runner_cookie_matched: true,
                })
                .collect();
            report.hotkey_evidence.push(packet);
            report.push_case(AcceptanceCaseResult {
                id: id.into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "bounded hotkey evidence".into(),
                observed: "native state verified".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        for id in ["CLEANUP", "R0"] {
            report.push_case(AcceptanceCaseResult {
                id: id.into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "bounded integrity evidence".into(),
                observed: "verified".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        assert!(report_serialized_sizes(&report).unwrap().0 > MAX_JSON_REPORT_BYTES);

        let output = tempfile::tempdir().unwrap();
        let json_path = output.path().join("oversized-evidence.json");
        let text_path = output.path().join("oversized-evidence.txt");
        write_report(&json_path, &mut report).unwrap();
        write_text_report(&text_path, &report).unwrap();

        assert_eq!(report.hotkey_evidence.len(), 0);
        assert!(report.capacity_saturated);
        assert_eq!(report.outcome, "failed");
        let receipt = report.report_overflow.as_ref().unwrap();
        assert_eq!(receipt.omitted_case_evidence, case_ids.len());
        assert_eq!(receipt.affected_case_ids.len(), case_ids.len());
        validate_hotkey_evidence_report(&report).unwrap();
        for id in case_ids.into_iter().chain(["CLEANUP", "R0"]) {
            assert!(matches!(
                report
                    .cases
                    .iter()
                    .find(|case| case.id == id)
                    .unwrap()
                    .status,
                CaseStatus::Failed
            ));
        }
        assert!(fs::metadata(&json_path).unwrap().len() as usize <= MAX_JSON_REPORT_BYTES);
        assert!(fs::metadata(&text_path).unwrap().len() as usize <= MAX_TEXT_REPORT_BYTES);
        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(&json_path).unwrap()).unwrap();
        assert_eq!(
            persisted["report_overflow"]["omitted_case_evidence"],
            case_ids.len()
        );
        assert!(
            fs::read_to_string(&text_path)
                .unwrap()
                .contains("Report overflow: omitted_case_evidence=13")
        );
    }

    #[test]
    fn text_report_marks_copied_profile_as_not_run_and_has_timing_and_hashes() {
        let report = AcceptanceReport {
            schema_version: 7,
            run_id: "test".into(),
            mode: "test",
            started_unix_ms: 100,
            finished_unix_ms: 250,
            copied_profile_status: CopiedProfileStatus::NotRun,
            copied_profile: None,
            private_artifacts: None,
            h6_repeat_mode: H6RepeatMode::Quiescent,
            mouse_gesture_mode: MouseGestureMode::Enabled,
            suite: AcceptanceSuite::All,
            hotkey: AcceptanceHotkey::F11,
            outcome: "failed",
            candidate: CandidateIdentity {
                executable: "candidate.exe".into(),
                sha256: "a".repeat(64),
            },
            environment: EnvironmentIdentity {
                os_version: "test".into(),
                architecture: "x64".into(),
                runner_process_id: 1,
                runner_sha256: Some("b".repeat(64)),
                child_process_id: None,
                child_started_unix_ms: None,
                source_revision: Some("deadbeef".into()),
                monitors: Vec::new(),
            },
            profile: ProfileIdentity {
                mode: "test",
                temporary_data_root: "profile".into(),
                settings_sha256: "c".repeat(64),
                radial_sha256: "d".repeat(64),
                actions_sha256: "e".repeat(64),
                configured_hotkey: ACCEPTANCE_HOTKEY,
                hold_threshold_ms: 1,
            },
            cases: Vec::new(),
            hotkey_evidence: Vec::new(),
            artifacts: Vec::new(),
            cleanup: CleanupResult::default(),
            capacity_saturated: false,
            report_overflow: None,
        };
        let text = render_text_report(&report);
        assert!(text.contains("Copied profile: not_run"));
        assert!(text.contains("Started Unix ms: 100"));
        assert!(text.contains("Finished Unix ms: 250"));
        assert!(text.contains(&"a".repeat(64)));
        assert!(text.contains(&"c".repeat(64)));
    }
}
