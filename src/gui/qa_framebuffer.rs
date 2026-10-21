use super::LauncherApp;
#[cfg(feature = "qa-framebuffer-capture")]
use super::qr_dialog::QrViewportObservation;
use eframe::egui::{
    self, ColorImage, Event, Key, Modifiers, UserData, ViewportCommand, ViewportId,
};
use image::ImageEncoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const REQUEST_MANIFEST_ENV: &str = "ML_QA_FRAMEBUFFER_REQUEST_MANIFEST";
const BENCH_MODE_ENV: &str = "ML_TRACK_C_BENCH_MODE";
const PNG_FILE_NAME: &str = "framebuffer.png";
const RECEIPT_FILE_NAME: &str = "receipt.json";
const JUMP_OBSERVATION_FILE_NAME: &str = "jump-observation.json";
const SCREENSHOT_REPLY_TIMEOUT: Duration = Duration::from_secs(30);
const QR_STARTUP_CAPTURE_TIMEOUT: Duration = Duration::from_secs(120);
const WRITER_QUEUE_CAPACITY: usize = 2;
const MAX_CAPTURE_COUNT: u8 = 16;

fn selected_reply_timeout(target: CaptureTarget) -> Duration {
    match target {
        CaptureTarget::Root => SCREENSHOT_REPLY_TIMEOUT,
        CaptureTarget::Qr => QR_STARTUP_CAPTURE_TIMEOUT,
    }
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CaptureTrigger {
    Startup,
    F11,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
enum CaptureTarget {
    #[serde(rename = "ROOT")]
    Root,
    #[serde(rename = "QR")]
    Qr,
}

impl CaptureTarget {
    fn viewport_id(self) -> ViewportId {
        match self {
            Self::Root => ViewportId::ROOT,
            Self::Qr => super::qr_dialog::viewport_id(),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Root => "ROOT",
            Self::Qr => "QR",
        }
    }

    fn capture_semantics(self) -> &'static str {
        match self {
            Self::Root => "ROOT framebuffer pixels; CPU UI completion before GPU presentation",
            Self::Qr => {
                "QR native immediate-child framebuffer pixels; child paint and swap precede callback return; CPU observation is not GPU completion"
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestManifest {
    run_id: String,
    request_id: String,
    target: CaptureTarget,
    trigger: CaptureTrigger,
    #[serde(default = "default_capture_count")]
    capture_count: u8,
    #[serde(default)]
    diagnostics: bool,
    #[serde(default)]
    jump_mode_observation: Option<JumpModeObservationRequest>,
    output_dir: PathBuf,
    fixture_manifest_sha256: String,
    build_receipt_identity: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct JumpModeObservationRequest {
    #[serde(default)]
    wait_for_active_hints: bool,
    #[serde(default)]
    phase2: Option<Phase2DialogRequest>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase2Surface {
    Settings,
    Notes,
    Todo,
    Timer,
    Paste,
    #[serde(rename = "mkmacro")]
    MkMacro,
    #[serde(rename = "file-search")]
    FileSearch,
}

impl Phase2Surface {
    fn as_str(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::Notes => "notes",
            Self::Todo => "todo",
            Self::Timer => "timer",
            Self::Paste => "paste",
            Self::MkMacro => "mkmacro",
            Self::FileSearch => "file-search",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Phase2DialogRequest {
    surface: Phase2Surface,
    #[serde(default)]
    use_empty_paste_invocation_context: bool,
}

#[derive(Clone, Debug, Serialize)]
struct JumpModeObservation {
    observed_at_unix_ms: u64,
    fixture_manifest_sha256: String,
    process_pid: u32,
    logic_frame_id: u64,
    ui_pass_id: u64,
    ui_pass_index: u32,
    capture_index: u8,
    capture_count: u8,
    capture_armed: bool,
    query: String,
    effective_fuzzy_weight: f32,
    result_generation: u64,
    result_count: usize,
    grid_layout: bool,
    selected_index: Option<usize>,
    selected_target_id: Option<String>,
    selected_action: Option<QaActionObservation>,
    opened_note_slugs: Vec<String>,
    root_focused: bool,
    root_owner_allowed: bool,
    navigation_owner: QaNavigationOwner,
    jump_mode_enabled: bool,
    active_session: bool,
    prefix: Option<String>,
    outcome: Option<JumpModeOutcomeObservation>,
    query_target_id: Option<String>,
    query_focused: bool,
    focused_destination: Option<String>,
    focused_widget_id: Option<String>,
    focused_navigation_destination: Option<String>,
    dialog_state: Option<QaDialogState>,
    mkmacro_action_editor_open: bool,
    activation_count: u64,
    last_action: Option<QaActivationObservation>,
    tab_processed_count: u64,
    last_tab_processed_logic_frame_id: Option<u64>,
    enter_processed_count: u64,
    last_enter_processed_logic_frame_id: Option<u64>,
    prefix_target_ids: Vec<String>,
    targets: Vec<JumpModeTargetObservation>,
    navigation_targets: Vec<JumpModeTargetObservation>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
struct QaNavigationOwner {
    kind: &'static str,
    instance: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Default)]
struct QaDialogState {
    surface: Option<&'static str>,
    instance: Option<u64>,
    target_generation: Option<u64>,
    settings_draft_jump_mode: Option<bool>,
    settings_committed_jump_mode: Option<bool>,
    settings_draft_fuzzy_weight: Option<f32>,
    settings_committed_fuzzy_weight: Option<f32>,
    settings_draft_debug_logging: Option<bool>,
    settings_committed_debug_logging: Option<bool>,
    notes_open: Option<bool>,
    notes_entry_count: Option<usize>,
    notes_editing: Option<bool>,
    notes_draft_chars: Option<usize>,
    notes_draft_newlines: Option<usize>,
    todo_open: Option<bool>,
    todo_entry_ids: Option<Vec<String>>,
    todo_pending_clear_confirmation: Option<bool>,
    todo_draft_text_chars: Option<usize>,
    todo_draft_newlines: Option<usize>,
    timer_open: Option<bool>,
    timer_mode: Option<&'static str>,
    timer_duration_chars: Option<usize>,
    timer_label_chars: Option<usize>,
    timer_sound_index: Option<usize>,
    active_timer_count: Option<usize>,
    active_alarm_count: Option<usize>,
    paste_open: Option<bool>,
    paste_context_present: Option<bool>,
    paste_context_has_clipboard_text: Option<bool>,
    paste_saved_sequence_ids: Option<Vec<u64>>,
    paste_selected_sequence_id: Option<u64>,
    paste_draft_step_count: Option<usize>,
    paste_remove_confirmation_open: Option<bool>,
    file_search_open: Option<bool>,
    file_search_query_char_count: Option<usize>,
    file_search_status: Option<&'static str>,
    file_search_search_id: Option<u64>,
    file_search_form_generation: Option<u64>,
    mkmacro_open: Option<bool>,
    mkmacro_draft_revision: Option<u64>,
    mkmacro_selected_id: Option<u64>,
    mkmacro_selected_step_ids: Option<Vec<u64>>,
    mkmacro_table_focus_id: Option<String>,
    mkmacro_table_focused: Option<bool>,
    mkmacro_document_macro_count: Option<usize>,
    mkmacro_active_runtime: Option<bool>,
    mkmacro_recorder_active: Option<bool>,
    mkmacro_action_editor_open: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum JumpModeOutcomeObservation {
    NoTargets,
    Partial { matching_targets: usize },
    Completed { target_id: String },
    Invalid,
    NoTarget,
    Cancelled,
    SessionClosed,
}

#[derive(Clone, Debug, Serialize)]
struct JumpModeTargetObservation {
    target_id: String,
    semantic_id: QaSemanticTarget,
    target_generation: u64,
    surface_identity: QaSurfaceIdentity,
    surface: &'static str,
    label: String,
    kind: &'static str,
    rect: [f32; 4],
    clipped_rect: [f32; 4],
    eligible: bool,
    is_query_input: bool,
    focused: bool,
    hint: Option<String>,
    root_result_index: Option<usize>,
    root_action: Option<QaActionObservation>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum QaSemanticTarget {
    Named(String),
    RootResult(usize),
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum QaSurfaceIdentity {
    Dashboard,
    LauncherResults,
    Settings {
        instance: u64,
    },
    Dialog {
        dialog_kind: String,
        instance: u64,
    },
    Popup {
        popup_kind: String,
        instance: u64,
        owner: Box<QaSurfaceIdentity>,
    },
}

#[derive(Clone, Debug, Serialize)]
struct QaActionObservation {
    label: String,
    desc: String,
    action: String,
    args: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct QaActivationObservation {
    action_id: String,
    label: String,
    args: Option<String>,
    query_override: Option<String>,
    source: &'static str,
}

const fn default_capture_count() -> u8 {
    1
}

/// A validated framebuffer-capture request prepared by the production GUI
/// launcher. Test and benchmark constructors never load this from the process
/// environment.
pub struct QaFramebufferRequest {
    manifest: RequestManifest,
    manifest_sha256: String,
    output_dir: PathBuf,
    executable: ExecutableIdentity,
    process: ProcessIdentity,
}

impl QaFramebufferRequest {
    pub fn from_environment() -> Result<Option<Self>, String> {
        let Some(path) = std::env::var_os(REQUEST_MANIFEST_ENV) else {
            return Ok(None);
        };
        if path.is_empty() {
            return Err(format!("{REQUEST_MANIFEST_ENV} is empty"));
        }
        if let Some(reason) = reject_capture_mode(
            crate::performance::enabled(),
            std::env::var_os(BENCH_MODE_ENV).is_some(),
        ) {
            return Err(reason.to_owned());
        }

        let manifest_path = PathBuf::from(path);
        let manifest_bytes = fs::read(&manifest_path).map_err(|error| {
            format!(
                "failed to read capture manifest {}: {error}",
                manifest_path.display()
            )
        })?;
        let manifest: RequestManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|error| format!("invalid QA framebuffer manifest: {error}"))?;
        validate_manifest(&manifest)?;
        if let Some(observation) = manifest.jump_mode_observation.as_ref() {
            verify_fixture_manifest_binding(&manifest.fixture_manifest_sha256, observation.phase2)?;
        }

        let executable_path = std::env::current_exe()
            .map_err(|error| format!("failed to identify current executable: {error}"))?;
        let executable_sha256 = sha256_file(&executable_path)
            .map_err(|error| format!("failed to hash current executable: {error}"))?;
        let process = ProcessIdentity {
            pid: std::process::id(),
            probe_initialized_at_unix_ms: unix_millis(),
        };

        let output_dir = absolute_output_dir(&manifest.output_dir)?;
        fs::create_dir(&output_dir).map_err(|error| {
            format!(
                "capture output directory must be fresh and exclusively creatable at {}: {error}",
                output_dir.display()
            )
        })?;
        let output_dir = fs::canonicalize(&output_dir).map_err(|error| {
            format!(
                "failed to resolve exclusive capture output directory {}: {error}",
                output_dir.display()
            )
        })?;

        Ok(Some(Self {
            manifest,
            manifest_sha256: sha256_bytes(&manifest_bytes),
            output_dir,
            executable: ExecutableIdentity {
                path: executable_path,
                sha256: executable_sha256,
            },
            process,
        }))
    }
}

fn reject_capture_mode(
    performance_enabled: bool,
    benchmark_mode_present: bool,
) -> Option<&'static str> {
    if performance_enabled {
        Some("QA framebuffer capture cannot run with performance instrumentation enabled")
    } else if benchmark_mode_present {
        Some("QA framebuffer capture cannot run while ML_TRACK_C_BENCH_MODE is set")
    } else {
        None
    }
}

fn validate_manifest(manifest: &RequestManifest) -> Result<(), String> {
    for (name, value) in [
        ("run_id", manifest.run_id.as_str()),
        ("request_id", manifest.request_id.as_str()),
    ] {
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(format!(
                "manifest {name} must be 1-128 ASCII letters, digits, '-' or '_'"
            ));
        }
    }
    if manifest.target == CaptureTarget::Qr && manifest.trigger == CaptureTrigger::F11 {
        return Err("QR framebuffer capture supports the Startup trigger only".into());
    }
    if !(1..=MAX_CAPTURE_COUNT).contains(&manifest.capture_count) {
        return Err(format!(
            "manifest capture_count must be between 1 and {MAX_CAPTURE_COUNT}"
        ));
    }
    if manifest.capture_count > 1
        && (manifest.target != CaptureTarget::Root || manifest.trigger != CaptureTrigger::F11)
    {
        return Err("multiple captures require the ROOT/F11 trigger".into());
    }
    if manifest.diagnostics && manifest.target != CaptureTarget::Root {
        return Err("root geometry diagnostics require the ROOT capture target".into());
    }
    if manifest.jump_mode_observation.is_some() && manifest.target != CaptureTarget::Root {
        return Err("Jump Mode observation requires the ROOT capture target".into());
    }
    if let Some(observation) = manifest.jump_mode_observation.as_ref()
        && let Some(phase2) = observation.phase2
    {
        if manifest.trigger != CaptureTrigger::F11 {
            return Err("Phase 2 observation requires the ROOT/F11 trigger".into());
        }
        if observation.wait_for_active_hints {
            return Err("Phase 2 dialog observation cannot wait for root query hints".into());
        }
        if phase2.use_empty_paste_invocation_context && phase2.surface != Phase2Surface::Paste {
            return Err(
                "empty Paste invocation context is valid only for the Phase 2 Paste surface".into(),
            );
        }
    }
    if !is_sha256(&manifest.fixture_manifest_sha256) {
        return Err("manifest fixture_manifest_sha256 must be 64 hexadecimal characters".into());
    }
    if manifest.build_receipt_identity.trim().is_empty()
        || manifest.build_receipt_identity.len() > 512
    {
        return Err(
            "manifest build_receipt_identity must be nonempty and at most 512 bytes".into(),
        );
    }
    if manifest.output_dir.as_os_str().is_empty() || manifest.output_dir.file_name().is_none() {
        return Err("manifest output_dir must name a fresh directory".into());
    }
    Ok(())
}

fn absolute_output_dir(path: &Path) -> Result<PathBuf, String> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to resolve current directory: {error}"))?
            .join(path)
    };
    let parent = path
        .parent()
        .filter(|parent| parent.is_dir())
        .ok_or_else(|| format!("capture output parent does not exist: {}", path.display()))?;
    Ok(parent.join(
        path.file_name()
            .ok_or_else(|| "capture output_dir has no final path component".to_owned())?,
    ))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn verify_fixture_manifest_binding(
    expected_sha256: &str,
    phase2: Option<Phase2DialogRequest>,
) -> Result<(), String> {
    let working_dir = std::env::current_dir()
        .map_err(|error| format!("failed to identify fixture working directory: {error}"))?;
    let working_dir = fs::canonicalize(&working_dir).map_err(|error| {
        format!(
            "failed to canonicalize fixture working directory {}: {error}",
            working_dir.display()
        )
    })?;
    let manifest_path =
        fs::canonicalize(working_dir.join("fixture_manifest.json")).map_err(|error| {
            format!("failed to resolve fixture_manifest.json from app cwd: {error}")
        })?;
    if manifest_path.parent() != Some(working_dir.as_path()) {
        return Err("fixture_manifest.json resolved outside the isolated working directory".into());
    }
    let actual_sha256 = sha256_file(&manifest_path)
        .map_err(|error| format!("failed to hash isolated fixture_manifest.json: {error}"))?;
    if actual_sha256 != expected_sha256 {
        return Err("fixture_manifest.json hash does not match the QA request".into());
    }
    if let Some(phase2) = phase2 {
        let fixture: serde_json::Value = serde_json::from_slice(
            &fs::read(&manifest_path)
                .map_err(|error| format!("failed to read isolated fixture manifest: {error}"))?,
        )
        .map_err(|error| format!("invalid isolated fixture manifest: {error}"))?;
        validate_phase2_fixture_capability(&fixture, phase2)?;
    }
    Ok(())
}

fn validate_phase2_fixture_capability(
    fixture: &serde_json::Value,
    request: Phase2DialogRequest,
) -> Result<(), String> {
    let surface = request.surface.as_str();
    let expected_scenario = format!("phase2-{surface}");
    let expected_fixture_id = format!("jump-mode-m13-{surface}-v1");
    let expected_plugins: &[&str] = match request.surface {
        Phase2Surface::Settings => &["notes", "settings"],
        Phase2Surface::Notes => &["notes"],
        Phase2Surface::Todo => &["notes", "todo"],
        Phase2Surface::Timer => &["notes", "timer"],
        Phase2Surface::Paste => &["notes", "paste_sequence"],
        Phase2Surface::MkMacro => &["notes", "mkmacro"],
        Phase2Surface::FileSearch => &["notes", "file_search"],
    };
    if fixture.get("scenario").and_then(serde_json::Value::as_str)
        != Some(expected_scenario.as_str())
        || fixture
            .get("fixture_id")
            .and_then(serde_json::Value::as_str)
            != Some(expected_fixture_id.as_str())
        || fixture
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            != Some(4)
        || fixture
            .get("phase2_surface")
            .and_then(serde_json::Value::as_str)
            != Some(surface)
        || fixture
            .get("enabled_plugins")
            .and_then(serde_json::Value::as_array)
            .is_none_or(|plugins| {
                plugins.len() != expected_plugins.len()
                    || plugins
                        .iter()
                        .zip(expected_plugins)
                        .any(|(actual, expected)| actual.as_str() != Some(*expected))
            })
        || fixture
            .get("experimental_jump_mode_enabled")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || fixture
            .get("action_count")
            .and_then(serde_json::Value::as_u64)
            != Some(0)
        || fixture
            .get("qa_empty_paste_invocation_context")
            .and_then(serde_json::Value::as_bool)
            != Some(request.use_empty_paste_invocation_context)
    {
        return Err("Phase 2 QA request does not match the bound fixture identity".into());
    }
    let safety = fixture
        .get("safety")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Phase 2 fixture safety manifest is missing".to_owned())?;
    for (key, expected) in [
        ("profile_isolated_by_working_directory", true),
        ("notes_path_supplied_by_ml_notes_dir", true),
        (
            "mkmacro_plugin_enabled",
            request.surface == Phase2Surface::MkMacro,
        ),
        (
            "file_search_root_isolated",
            request.surface == Phase2Surface::FileSearch,
        ),
        ("file_search_everything_enabled", false),
    ] {
        if safety.get(key).and_then(serde_json::Value::as_bool) != Some(expected) {
            return Err(format!(
                "Phase 2 fixture safety flag {key} must be {expected}"
            ));
        }
    }
    let expected_file_search_count = if request.surface == Phase2Surface::FileSearch {
        1
    } else {
        0
    };
    if safety
        .get("file_search_fixture_file_count")
        .and_then(serde_json::Value::as_u64)
        != Some(expected_file_search_count)
    {
        return Err(
            "Phase 2 File Search fixture file count does not match the selected surface".into(),
        );
    }
    for key in [
        "clipboard_plugin_enabled",
        "clipboard_modify_plugin_enabled",
        "macros_enabled",
        "network_plugin_enabled",
        "custom_actions_present",
        "paste_next_hotkey_enabled",
        "paste_cancel_hotkey_enabled",
    ] {
        if safety.get(key).and_then(serde_json::Value::as_bool) != Some(false) {
            return Err(format!("Phase 2 fixture safety flag {key} must be false"));
        }
    }
    if safety
        .get("qa_empty_paste_invocation_context")
        .and_then(serde_json::Value::as_bool)
        != Some(request.use_empty_paste_invocation_context)
    {
        return Err("Phase 2 empty-context safety flag does not match the QA request".into());
    }
    if request.use_empty_paste_invocation_context && request.surface != Phase2Surface::Paste {
        return Err("empty Paste invocation context requires the Phase 2 Paste surface".into());
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
struct ExecutableIdentity {
    path: PathBuf,
    sha256: String,
}

#[derive(Clone, Debug, Serialize)]
struct ProcessIdentity {
    pid: u32,
    probe_initialized_at_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProbePhase {
    Armed,
    Ready,
    Issued,
    AwaitingReply,
    Saving,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, Default)]
struct SurfaceEligibility {
    logical_root_visible: bool,
    root_visibility_reported: Option<bool>,
    root_visible: bool,
    ocr_owner: bool,
    tool_owner: bool,
    modal_or_key_capture_owner: bool,
}

impl SurfaceEligibility {
    fn eligible(self) -> bool {
        self.logical_root_visible
            && self.root_visible
            && !self.ocr_owner
            && !self.tool_owner
            && !self.modal_or_key_capture_owner
    }

    fn failure_reason(self) -> &'static str {
        if !self.logical_root_visible || !self.root_visible {
            "ROOT became hidden before the retained capture boundary"
        } else if self.ocr_owner {
            "OCR took ownership of ROOT before the retained capture boundary"
        } else if self.tool_owner {
            "a native or selection tool took ownership of ROOT before the retained capture boundary"
        } else {
            "a modal or key-capture owner blocked the retained capture boundary"
        }
    }
}

#[derive(Clone, Debug)]
struct RetainedPassMarker {
    logic_frame_id: u64,
    ui_pass_id: u64,
    ui_pass_index: u32,
    eligibility: SurfaceEligibility,
    evidence: Option<FrameEvidence>,
    root_list_diagnostic: Option<RootListDiagnostic>,
}

#[derive(Debug, PartialEq, Eq)]
struct ScreenshotToken(u64);

struct PendingScreenshotReply {
    image: Arc<ColorImage>,
    evidence: ScreenshotReplyEvidence,
}

#[derive(Clone, Debug, Serialize)]
struct QrRenderEvidence {
    parent_logic_frame_id: u64,
    child_viewport_id: String,
    callback_class: &'static str,
    callback_frame_nr: u64,
    callback_pass_nr: u64,
    callback_pass_index: usize,
    completed_frame_nr: Option<u64>,
    completed_pass_nr: Option<u64>,
    visible: Option<bool>,
    minimized: Option<bool>,
    occluded: Option<bool>,
    focused: Option<bool>,
    body_rendered: bool,
    still_open: bool,
    eligible: bool,
    callback_observed_at_unix_ms: u64,
    renderer_returned_at_unix_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
struct FrameEvidence {
    viewport_id: &'static str,
    logic_frame_id: u64,
    ui_pass_id: u64,
    ui_pass_index: u32,
    evidence_stage: &'static str,
    completed_at_unix_ms: u64,
    query: String,
    result_generation: u64,
    pixels_per_point: Option<f32>,
    native_pixels_per_point: Option<f32>,
    zoom_factor: f32,
    logical_root_visible: bool,
    root_visibility_reported: Option<bool>,
    root_visible: bool,
    viewport_focused: bool,
    ocr_owner: bool,
    tool_owner: bool,
    modal_or_key_capture_owner: bool,
    eligible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    jump_mode_observation: Option<JumpModeObservation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    qr_observation: Option<QrRenderEvidence>,
}

#[derive(Clone, Debug, Serialize)]
struct ScreenshotReplyEvidence {
    viewport_id: &'static str,
    logic_frame_id: u64,
    received_at_unix_ms: u64,
    width: u64,
    height: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    qr_observation: Option<QrRenderEvidence>,
}

#[derive(Clone, Debug, Serialize)]
struct SaveEvidence {
    completed_at_unix_ms: u64,
    png_file: String,
    png_sha256: String,
    width: u32,
    height: u32,
    pixels_per_point: Option<f32>,
    native_pixels_per_point: Option<f32>,
    zoom_factor: f32,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ReceiptStatus {
    Saved,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
struct CompletionReceipt {
    schema_version: u32,
    status: ReceiptStatus,
    reply_wait_timeout_ms: u64,
    run_id: String,
    request_id: String,
    target: CaptureTarget,
    trigger: CaptureTrigger,
    capture_index: u8,
    capture_count: u8,
    fixture_manifest_sha256: String,
    build_receipt_identity: String,
    manifest_sha256: String,
    executable: ExecutableIdentity,
    process: ProcessIdentity,
    capture_semantics: &'static str,
    issued: Option<FrameEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dispatch_boundary_baseline: Option<QrRenderEvidence>,
    captured: Option<FrameEvidence>,
    screenshot_received: Option<ScreenshotReplyEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    root_list_diagnostic: Option<RootListDiagnostic>,
    saved: Option<SaveEvidence>,
    failure: Option<String>,
}

impl CompletionReceipt {
    fn new(request: &QaFramebufferRequest, reply_timeout: Duration) -> Self {
        Self {
            schema_version: 1,
            status: ReceiptStatus::Failed,
            reply_wait_timeout_ms: duration_millis(reply_timeout),
            run_id: request.manifest.run_id.clone(),
            request_id: request.manifest.request_id.clone(),
            target: request.manifest.target,
            trigger: request.manifest.trigger,
            capture_index: 1,
            capture_count: request.manifest.capture_count,
            fixture_manifest_sha256: request.manifest.fixture_manifest_sha256.clone(),
            build_receipt_identity: request.manifest.build_receipt_identity.clone(),
            manifest_sha256: request.manifest_sha256.clone(),
            executable: request.executable.clone(),
            process: request.process.clone(),
            capture_semantics: request.manifest.target.capture_semantics(),
            issued: None,
            dispatch_boundary_baseline: None,
            captured: None,
            screenshot_received: None,
            root_list_diagnostic: None,
            saved: None,
            failure: None,
        }
    }

    fn for_capture(&self, capture_index: u8) -> Self {
        let mut receipt = self.clone();
        receipt.status = ReceiptStatus::Failed;
        receipt.capture_index = capture_index;
        receipt.issued = None;
        receipt.dispatch_boundary_baseline = None;
        receipt.captured = None;
        receipt.screenshot_received = None;
        receipt.root_list_diagnostic = None;
        receipt.saved = None;
        receipt.failure = None;
        receipt
    }
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct RootListDiagnostic {
    pub(super) mode: &'static str,
    pub(super) result_count: usize,
    pub(super) selected_index: Option<usize>,
    pub(super) scroll_viewport: [f32; 4],
    pub(super) content_clip: [f32; 4],
    pub(super) content_origin: [f32; 2],
    pub(super) scroll_inner_rect: [f32; 4],
    pub(super) content_size: [f32; 2],
    pub(super) scroll_offset: [f32; 2],
    pub(super) total_content_height: f32,
    pub(super) visible_rows: Vec<VisibleRowDiagnostic>,
    pub(super) result_generation: u64,
    pub(super) cache_key: String,
    pub(super) geometry_rebuild_count: u64,
    pub(super) rebuilt_this_frame: bool,
    pub(super) font_revision: u64,
    pub(super) style_identity: String,
    pub(super) resolved_appearance: Option<String>,
    pub(super) overall_text_scale: Option<f32>,
    pub(super) query_scale: f32,
    pub(super) list_scale: f32,
    pub(super) pixels_per_point: f32,
    pub(super) native_pixels_per_point: Option<f32>,
    pub(super) zoom_factor: f32,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct VisibleRowDiagnostic {
    pub(super) index: usize,
    pub(super) top: f32,
    pub(super) bottom: f32,
    pub(super) visual_top: f32,
    pub(super) visual_bottom: f32,
    pub(super) intersects_viewport: bool,
}

pub(super) struct FramebufferProbe {
    output_dir: PathBuf,
    token: UserData,
    target: CaptureTarget,
    capture_count: u8,
    diagnostics_enabled: bool,
    jump_mode_observation_enabled: bool,
    wait_for_active_hints: bool,
    phase2_request: Option<Phase2DialogRequest>,
    activation_count: u64,
    last_action: Option<QaActivationObservation>,
    tab_processed_count: u64,
    last_tab_processed_logic_frame_id: Option<u64>,
    enter_processed_count: u64,
    last_enter_processed_logic_frame_id: Option<u64>,
    latest_jump_mode_observation: Option<JumpModeObservation>,
    geometry_rebuild_count: u64,
    latest_root_list_diagnostic: Option<RootListDiagnostic>,
    phase: ProbePhase,
    logic_frame_id: u64,
    ui_pass_id: u64,
    current_ui_pass_id: Option<u64>,
    current_ui_pass_index: u32,
    retained_pass: Option<RetainedPassMarker>,
    issue_logic_frame_id: Option<u64>,
    capture_logic_frame_id: Option<u64>,
    expected_reply_logic_frame_id: Option<u64>,
    reply_wait_armed: bool,
    latest_qr_observation: Option<QrRenderEvidence>,
    qr_acquired: bool,
    qr_capture_parent_logic_frame_id: Option<u64>,
    pending_reply: Option<PendingScreenshotReply>,
    receipt: CompletionReceipt,
    writer_tx: SyncSender<WriterJob>,
    writer_rx: Receiver<WriterOutcome>,
}

impl FramebufferProbe {
    fn use_empty_paste_invocation_context(&self) -> bool {
        self.phase2_request
            .is_some_and(|request| request.use_empty_paste_invocation_context)
    }

    fn new(request: QaFramebufferRequest, ctx: egui::Context) -> Result<Self, String> {
        let reply_timeout = selected_reply_timeout(request.manifest.target);
        Self::new_with_reply_timeout(request, ctx, reply_timeout)
    }

    fn new_with_reply_timeout(
        request: QaFramebufferRequest,
        ctx: egui::Context,
        reply_timeout: Duration,
    ) -> Result<Self, String> {
        let (writer_tx, writer_rx) = spawn_writer(request.output_dir.clone(), ctx, reply_timeout)?;
        let target = request.manifest.target;
        let capture_count = request.manifest.capture_count;
        let diagnostics_enabled = request.manifest.diagnostics;
        let observation_request = request.manifest.jump_mode_observation.as_ref();
        let token = UserData::new(ScreenshotToken(1));
        let receipt = CompletionReceipt::new(&request, reply_timeout);
        let mut probe = Self {
            output_dir: request.output_dir.clone(),
            token,
            target,
            capture_count,
            diagnostics_enabled,
            jump_mode_observation_enabled: observation_request.is_some(),
            wait_for_active_hints: observation_request
                .is_some_and(|request| request.wait_for_active_hints),
            phase2_request: observation_request.and_then(|request| request.phase2),
            activation_count: 0,
            last_action: None,
            tab_processed_count: 0,
            last_tab_processed_logic_frame_id: None,
            enter_processed_count: 0,
            last_enter_processed_logic_frame_id: None,
            latest_jump_mode_observation: None,
            geometry_rebuild_count: 0,
            latest_root_list_diagnostic: None,
            phase: ProbePhase::Armed,
            logic_frame_id: 0,
            ui_pass_id: 0,
            current_ui_pass_id: None,
            current_ui_pass_index: 0,
            retained_pass: None,
            issue_logic_frame_id: None,
            capture_logic_frame_id: None,
            expected_reply_logic_frame_id: None,
            reply_wait_armed: false,
            latest_qr_observation: None,
            qr_acquired: false,
            qr_capture_parent_logic_frame_id: None,
            pending_reply: None,
            receipt,
            writer_tx,
            writer_rx,
        };
        if target == CaptureTarget::Qr {
            probe.arm_reply_wait()?;
        }
        Ok(probe)
    }

    fn begin_root_logic(&mut self, ctx: &egui::Context, eligibility: Option<SurfaceEligibility>) {
        if !self.needs_root_logic() {
            return;
        }
        let was_armed_at_entry = self.phase == ProbePhase::Armed;
        self.logic_frame_id = self.logic_frame_id.saturating_add(1);
        self.retained_pass = None;
        self.current_ui_pass_id = None;

        if self.target == CaptureTarget::Root
            && matches!(self.phase, ProbePhase::Issued | ProbePhase::AwaitingReply)
            && let Some(image) =
                take_matching_screenshot_event(ctx, self.target.viewport_id(), &self.token)
            && self.pending_reply.is_none()
        {
            let [width, height] = image.size;
            let evidence = ScreenshotReplyEvidence {
                viewport_id: self.target.label(),
                logic_frame_id: self.logic_frame_id,
                received_at_unix_ms: unix_millis(),
                width: u64::try_from(width).unwrap_or(u64::MAX),
                height: u64::try_from(height).unwrap_or(u64::MAX),
                qr_observation: None,
            };
            self.receipt.screenshot_received = Some(evidence.clone());
            self.pending_reply = Some(PendingScreenshotReply { image, evidence });
        }

        if self.phase == ProbePhase::Saving || self.target == CaptureTarget::Qr {
            self.poll_writer_outcome();
        }
        if !self.needs_root_logic() {
            return;
        }

        if self.target == CaptureTarget::Root
            && self.receipt.trigger == CaptureTrigger::F11
            && was_armed_at_entry
            && self.phase == ProbePhase::Armed
            && let Some(eligibility) = eligibility
            && eligibility.eligible()
            && ctx.input(|input| input.viewport().focused == Some(true))
            && take_unmodified_f11(ctx)
        {
            self.phase = ProbePhase::Ready;
        }

        if self.phase_requires_eligibility()
            && let Some(eligibility) = eligibility
            && !eligibility.eligible()
        {
            self.fail(eligibility.failure_reason());
        }
    }

    fn revalidate_root_logic(&mut self, eligibility: SurfaceEligibility) {
        if self.phase_requires_eligibility() && !eligibility.eligible() {
            self.fail(eligibility.failure_reason());
        }
    }

    fn run_hidden_logic_tick(&mut self) {
        if self.phase == ProbePhase::Saving || self.target == CaptureTarget::Qr {
            self.poll_writer_outcome();
        }
        if self.phase == ProbePhase::Saving {
            return;
        }
        if self.phase_is_active()
            || (self.target == CaptureTarget::Qr && self.phase == ProbePhase::Armed)
        {
            self.fail("ROOT became hidden before the retained screenshot reply boundary");
        }
    }

    fn start_ui_pass(&mut self, pass_index: u32) {
        if !self.needs_retained_marker() {
            return;
        }
        self.ui_pass_id = self.ui_pass_id.saturating_add(1);
        self.current_ui_pass_id = Some(self.ui_pass_id);
        self.current_ui_pass_index = pass_index;
        self.retained_pass = None;
        self.latest_root_list_diagnostic = None;
    }

    fn record_root_list_diagnostic(
        &mut self,
        mut diagnostic: RootListDiagnostic,
        geometry_rebuilt: bool,
    ) {
        if !self.root_diagnostics_active() {
            return;
        }
        if geometry_rebuilt {
            self.geometry_rebuild_count = self.geometry_rebuild_count.saturating_add(1);
        }
        diagnostic.geometry_rebuild_count = self.geometry_rebuild_count;
        diagnostic.rebuilt_this_frame = geometry_rebuilt;
        self.latest_root_list_diagnostic = Some(diagnostic);
    }

    fn root_diagnostics_active(&self) -> bool {
        self.diagnostics_enabled
            && self.target == CaptureTarget::Root
            && self.needs_retained_marker()
    }

    fn finish_ui_pass(&mut self, eligibility: SurfaceEligibility, evidence: Option<FrameEvidence>) {
        let Some(ui_pass_id) = self.current_ui_pass_id else {
            return;
        };
        debug_assert!(
            evidence
                .as_ref()
                .is_none_or(|evidence| evidence.ui_pass_id == ui_pass_id)
        );
        self.retained_pass = Some(RetainedPassMarker {
            logic_frame_id: self.logic_frame_id,
            ui_pass_id,
            ui_pass_index: self.current_ui_pass_index,
            eligibility,
            evidence,
            root_list_diagnostic: self.latest_root_list_diagnostic.take(),
        });
    }

    fn issue_screenshot_if_ready(
        &mut self,
        ctx: &egui::Context,
        eligibility: SurfaceEligibility,
        jump_root_owner_allowed: bool,
        evidence: FrameEvidence,
    ) {
        if self.phase != ProbePhase::Ready {
            return;
        }
        if !eligibility.eligible() {
            self.fail(eligibility.failure_reason());
            return;
        }
        if self.current_ui_pass_id.is_none() {
            return;
        }
        if self.wait_for_active_hints {
            let observation = evidence.jump_mode_observation.as_ref();
            if !jump_root_owner_allowed
                || !observation.is_some_and(active_query_hint_snapshot)
                || observation.is_some_and(|observation| {
                    observation.query != evidence.query
                        || observation.activation_count != self.activation_count
                })
            {
                self.fail("Jump Mode query state regressed before screenshot issue");
                return;
            }
        }
        self.receipt.issued = Some(evidence);
        self.issue_logic_frame_id = Some(self.logic_frame_id);
        self.phase = ProbePhase::Issued;
        ctx.send_viewport_cmd_to(
            self.target.viewport_id(),
            ViewportCommand::Screenshot(self.token.clone()),
        );
        // Glow services the action after the selected viewport's next paint.
        // The ROOT repaint also preserves the established event-delivery path.
        ctx.request_repaint_of(ViewportId::ROOT);
    }

    fn after_qr_show(
        &mut self,
        ctx: &egui::Context,
        root_eligibility: SurfaceEligibility,
        qr_open: bool,
        observation: Option<QrViewportObservation>,
        frame_evidence: Option<FrameEvidence>,
    ) {
        if self.target != CaptureTarget::Qr || !self.needs_root_logic() {
            return;
        }

        let Some(observation) = observation else {
            if self.qr_acquired && !qr_open && self.phase_requires_eligibility() {
                self.fail("QR dialog closed before the selected child capture completed");
            }
            return;
        };

        let qr_evidence = qr_render_evidence(self.logic_frame_id, &observation);
        self.latest_qr_observation = Some(qr_evidence.clone());

        if !root_eligibility.eligible() {
            self.fail(root_eligibility.failure_reason());
            return;
        }
        if !qr_open || !observation.still_open {
            self.fail("QR dialog closed before the selected child capture completed");
            return;
        }
        if !qr_evidence.eligible {
            self.fail(qr_eligibility_failure(&observation));
            return;
        }
        self.qr_acquired = true;

        let Some(mut frame_evidence) = frame_evidence else {
            if matches!(self.phase, ProbePhase::Ready | ProbePhase::Issued) {
                self.fail("QR callback omitted its current ROOT UI-pass provenance");
            }
            return;
        };
        frame_evidence.viewport_id = self.target.label();
        frame_evidence.viewport_focused = observation.focused.unwrap_or(false);
        frame_evidence.eligible &= qr_evidence.eligible;
        frame_evidence.qr_observation = Some(qr_evidence.clone());

        match self.phase {
            ProbePhase::Ready => {
                frame_evidence.evidence_stage = "qr_screenshot_api_issued_after_child_callback";
                self.issue_screenshot_if_ready(ctx, root_eligibility, true, frame_evidence);
            }
            ProbePhase::Issued => {
                let Some(issue_parent_frame) = self.issue_logic_frame_id else {
                    self.fail("QR screenshot issue frame was missing from probe state");
                    return;
                };
                if self.logic_frame_id <= issue_parent_frame {
                    return;
                }
                let Some(dispatch_baseline) = self.receipt.dispatch_boundary_baseline.as_ref()
                else {
                    return;
                };
                if !qr_child_advanced(dispatch_baseline, &qr_evidence) {
                    return;
                }

                frame_evidence.evidence_stage =
                    "qr_expected_capture_child_render_after_dispatch_boundary";
                self.receipt.captured = Some(frame_evidence);
                self.qr_capture_parent_logic_frame_id = Some(self.logic_frame_id);
                self.phase = ProbePhase::AwaitingReply;
                // A single ROOT repaint allows the child to advance again and
                // deliver the event queued after its native paint and swap.
                ctx.request_repaint_of(ViewportId::ROOT);
            }
            ProbePhase::AwaitingReply => {
                let Some(captured) = self
                    .receipt
                    .captured
                    .as_ref()
                    .and_then(|captured| captured.qr_observation.as_ref())
                else {
                    self.fail("QR expected-capture child observation was missing");
                    return;
                };
                if !qr_child_advanced(captured, &qr_evidence) {
                    return;
                }
                let Some(image) =
                    take_matching_screenshot_event(ctx, self.target.viewport_id(), &self.token)
                else {
                    return;
                };
                if self.pending_reply.is_none() {
                    let [width, height] = image.size;
                    let evidence = ScreenshotReplyEvidence {
                        viewport_id: self.target.label(),
                        logic_frame_id: self.logic_frame_id,
                        received_at_unix_ms: unix_millis(),
                        width: u64::try_from(width).unwrap_or(u64::MAX),
                        height: u64::try_from(height).unwrap_or(u64::MAX),
                        qr_observation: Some(qr_evidence),
                    };
                    self.receipt.screenshot_received = Some(evidence.clone());
                    self.pending_reply = Some(PendingScreenshotReply { image, evidence });
                }
            }
            ProbePhase::Armed | ProbePhase::Saving | ProbePhase::Complete | ProbePhase::Failed => {}
        }
    }
    fn finish_retained_ui_frame(
        &mut self,
        ctx: &egui::Context,
        final_eligibility: SurfaceEligibility,
        jump_mode_observation: Option<JumpModeObservation>,
    ) {
        let Some(mut marker) = self.retained_pass.take() else {
            if self.phase_requires_eligibility() {
                self.fail("ROOT UI completed without a retained-pass eligibility marker");
            }
            return;
        };

        marker.eligibility = merge_eligibility(marker.eligibility, final_eligibility);
        if let Some(evidence) = marker.evidence.as_mut() {
            evidence.completed_at_unix_ms = unix_millis();
            evidence.logical_root_visible = marker.eligibility.logical_root_visible;
            evidence.root_visibility_reported = final_eligibility.root_visibility_reported;
            evidence.root_visible = marker.eligibility.root_visible;
            evidence.ocr_owner = marker.eligibility.ocr_owner;
            evidence.tool_owner = marker.eligibility.tool_owner;
            evidence.modal_or_key_capture_owner = marker.eligibility.modal_or_key_capture_owner;
            evidence.eligible = marker.eligibility.eligible()
                && evidence
                    .qr_observation
                    .as_ref()
                    .is_none_or(|observation| observation.eligible);
            evidence.jump_mode_observation = jump_mode_observation.clone();
        }
        if let Some(observation) = jump_mode_observation {
            self.latest_jump_mode_observation = Some(observation.clone());
            if let Err(error) = write_jump_mode_observation(&self.output_dir, &observation) {
                self.fail_jump_mode_observation(format!(
                    "failed to write Jump Mode observation: {error}"
                ));
                return;
            }
        }

        match self.phase {
            ProbePhase::Armed if self.receipt.trigger == CaptureTrigger::Startup => {
                let active_hints_ready = !self.wait_for_active_hints
                    || marker
                        .evidence
                        .as_ref()
                        .and_then(|evidence| evidence.jump_mode_observation.as_ref())
                        .is_some_and(active_query_hint_snapshot);
                if marker.eligibility.eligible() && active_hints_ready {
                    self.phase = ProbePhase::Ready;
                    ctx.request_repaint_of(ViewportId::ROOT);
                }
            }
            ProbePhase::Armed | ProbePhase::Complete | ProbePhase::Failed | ProbePhase::Saving => {}
            ProbePhase::Ready => {
                if !marker.eligibility.eligible() {
                    self.fail(marker.eligibility.failure_reason());
                }
            }
            ProbePhase::Issued => {
                let Some(issue_frame) = self.issue_logic_frame_id else {
                    self.fail("screenshot issue frame was missing from probe state");
                    return;
                };
                if marker.logic_frame_id == issue_frame {
                    if !marker.eligibility.eligible() {
                        self.fail(marker.eligibility.failure_reason());
                    } else if !self.current_capture_observation_matches(&marker) {
                        self.fail(
                            "Jump Mode query state regressed at screenshot issue frame completion",
                        );
                    } else if self.target == CaptureTarget::Qr {
                        let Some(baseline) =
                            self.latest_qr_observation.as_ref().filter(|observation| {
                                observation.parent_logic_frame_id == issue_frame
                                    && observation.eligible
                            })
                        else {
                            self.fail("QR dispatch-boundary child observation was unavailable");
                            return;
                        };
                        self.receipt.dispatch_boundary_baseline = Some(baseline.clone());
                    }
                } else if marker.logic_frame_id > issue_frame && self.target == CaptureTarget::Root
                {
                    if !marker.eligibility.eligible() {
                        self.fail(marker.eligibility.failure_reason());
                    } else if !self.current_capture_observation_matches(&marker) {
                        self.fail(
                            "Jump Mode query state regressed at expected capture frame completion",
                        );
                    } else {
                        let Some(mut captured) = marker.evidence.clone() else {
                            self.fail("retained capture frame omitted its provenance snapshot");
                            return;
                        };
                        debug_assert_eq!(captured.ui_pass_id, marker.ui_pass_id);
                        debug_assert_eq!(captured.ui_pass_index, marker.ui_pass_index);
                        captured.evidence_stage = "expected_capture_frame_cpu_ui_completion";
                        self.receipt.captured = Some(captured);
                        self.receipt.root_list_diagnostic = marker.root_list_diagnostic.clone();
                        self.capture_logic_frame_id = Some(marker.logic_frame_id);
                        self.expected_reply_logic_frame_id =
                            Some(marker.logic_frame_id.saturating_add(1));
                        self.phase = ProbePhase::AwaitingReply;
                        if let Err(error) = self.arm_reply_wait() {
                            self.fail(format!("failed to arm screenshot reply wait: {error}"));
                        } else {
                            // The capture frame is N+1; one bounded repaint
                            // requests the reply input on N+2.
                            ctx.request_repaint_of(ViewportId::ROOT);
                        }
                    }
                }
            }
            ProbePhase::AwaitingReply => {
                if !marker.eligibility.eligible() {
                    self.fail(marker.eligibility.failure_reason());
                    return;
                }
                if self.target == CaptureTarget::Qr
                    && !self
                        .latest_qr_observation
                        .as_ref()
                        .is_some_and(|observation| {
                            observation.parent_logic_frame_id == marker.logic_frame_id
                                && observation.eligible
                        })
                {
                    self.fail("QR child lost eligibility before retained ROOT completion");
                    return;
                }

                let reply_is_ready = match self.target {
                    CaptureTarget::Root => self
                        .expected_reply_logic_frame_id
                        .zip(
                            self.pending_reply
                                .as_ref()
                                .map(|reply| reply.evidence.logic_frame_id),
                        )
                        .is_some_and(|(expected, received)| {
                            self.logic_frame_id >= expected && received >= expected
                        }),
                    CaptureTarget::Qr => self
                        .qr_capture_parent_logic_frame_id
                        .zip(
                            self.pending_reply
                                .as_ref()
                                .map(|reply| reply.evidence.logic_frame_id),
                        )
                        .is_some_and(|(capture_parent, reply_parent)| {
                            self.logic_frame_id >= capture_parent && reply_parent >= capture_parent
                        }),
                };
                if reply_is_ready && let Some(reply) = self.pending_reply.take() {
                    self.receipt.screenshot_received = Some(reply.evidence);
                    match self.writer_tx.try_send(WriterJob::Save {
                        receipt: self.receipt.clone(),
                        image: reply.image,
                    }) {
                        Ok(()) => self.phase = ProbePhase::Saving,
                        Err(error) => {
                            self.fail(format!("failed to queue screenshot save: {error}"));
                        }
                    }
                }
            }
        }
    }

    fn arm_reply_wait(&mut self) -> Result<(), String> {
        if self.reply_wait_armed {
            return Ok(());
        }
        self.writer_tx
            .try_send(WriterJob::AwaitReply(self.receipt.clone()))
            .map_err(|error| error.to_string())?;
        self.reply_wait_armed = true;
        Ok(())
    }

    fn phase_is_active(&self) -> bool {
        matches!(
            self.phase,
            ProbePhase::Ready | ProbePhase::Issued | ProbePhase::AwaitingReply
        )
    }

    fn phase_requires_eligibility(&self) -> bool {
        self.phase_is_active()
            || (self.target == CaptureTarget::Qr && self.phase == ProbePhase::Armed)
    }

    fn needs_retained_marker(&self) -> bool {
        self.jump_mode_observation_enabled
            || matches!(
                self.phase,
                ProbePhase::Armed
                    | ProbePhase::Ready
                    | ProbePhase::Issued
                    | ProbePhase::AwaitingReply
            )
    }

    fn needs_input_eligibility(&self) -> bool {
        self.phase == ProbePhase::Armed || self.phase_is_active()
    }

    fn needs_root_logic(&self) -> bool {
        self.needs_input_eligibility()
            || self.phase == ProbePhase::Saving
            || self.jump_mode_observation_enabled
    }

    fn record_activation(
        &mut self,
        action: &crate::actions::Action,
        query_override: Option<&str>,
        source: crate::commands::ActivationSource,
    ) {
        if !self.jump_mode_observation_enabled {
            return;
        }
        self.activation_count = self.activation_count.saturating_add(1);
        self.last_action = Some(QaActivationObservation {
            action_id: action.action.clone(),
            label: action.label.clone(),
            args: action.args.clone(),
            query_override: query_override.map(str::to_owned),
            source: source.label(),
        });
    }

    fn record_enter_processed(&mut self) {
        if !self.jump_mode_observation_enabled {
            return;
        }
        self.enter_processed_count = self.enter_processed_count.saturating_add(1);
        self.last_enter_processed_logic_frame_id = Some(self.logic_frame_id);
    }

    fn record_tab_processed(&mut self) {
        if !self.jump_mode_observation_enabled {
            return;
        }
        self.tab_processed_count = self.tab_processed_count.saturating_add(1);
        self.last_tab_processed_logic_frame_id = Some(self.logic_frame_id);
    }

    fn current_capture_observation_matches(&self, marker: &RetainedPassMarker) -> bool {
        if !self.wait_for_active_hints {
            return true;
        }
        let Some(issued) = self
            .receipt
            .issued
            .as_ref()
            .and_then(|evidence| evidence.jump_mode_observation.as_ref())
        else {
            return false;
        };
        marker
            .evidence
            .as_ref()
            .and_then(|evidence| evidence.jump_mode_observation.as_ref())
            .is_some_and(|current| {
                active_query_hint_snapshot(current)
                    && current.query == issued.query
                    && current.activation_count == issued.activation_count
            })
    }

    fn fail(&mut self, reason: impl Into<String>) {
        if !self.phase_requires_eligibility() {
            return;
        }
        self.set_failed(reason.into());
    }

    fn fail_jump_mode_observation(&mut self, reason: impl Into<String>) {
        if !self.jump_mode_observation_enabled
            || matches!(self.phase, ProbePhase::Complete | ProbePhase::Failed)
        {
            return;
        }
        self.set_failed(reason.into());
    }

    fn set_failed(&mut self, reason: String) {
        self.receipt.status = ReceiptStatus::Failed;
        self.receipt.failure = Some(reason);
        self.phase = ProbePhase::Failed;
        let _ = self
            .writer_tx
            .try_send(WriterJob::Failure(self.receipt.clone()));
    }

    fn poll_writer_outcome(&mut self) {
        match self.writer_rx.try_recv() {
            Ok(outcome) => {
                let saved = outcome.status == ReceiptStatus::Saved && outcome.receipt_written;
                if saved
                    && outcome.capture_index == self.receipt.capture_index
                    && outcome.capture_count == self.capture_count
                {
                    self.receipt.status = ReceiptStatus::Saved;
                    if self.receipt.capture_index < self.capture_count {
                        self.arm_next_capture();
                    } else {
                        self.phase = ProbePhase::Complete;
                    }
                } else {
                    self.phase = ProbePhase::Failed;
                    self.receipt.status = ReceiptStatus::Failed;
                    self.receipt.failure = outcome.error.or_else(|| {
                        (outcome.capture_index != self.receipt.capture_index
                            || outcome.capture_count != self.capture_count)
                            .then(|| {
                                "QA framebuffer writer returned an out-of-sequence capture receipt"
                                    .into()
                            })
                            .or_else(|| {
                                (outcome.status == ReceiptStatus::Saved && !outcome.receipt_written)
                                    .then(|| "QA framebuffer receipt was not saved".into())
                            })
                    });
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                if self.phase == ProbePhase::Saving || self.phase == ProbePhase::AwaitingReply {
                    self.phase = ProbePhase::Failed;
                    self.receipt.failure =
                        Some("QA framebuffer writer exited without a receipt".into());
                }
            }
        }
    }

    fn arm_next_capture(&mut self) {
        let next_index = self.receipt.capture_index.saturating_add(1);
        self.token = UserData::new(ScreenshotToken(u64::from(next_index)));
        self.receipt = self.receipt.for_capture(next_index);
        self.phase = ProbePhase::Armed;
        self.issue_logic_frame_id = None;
        self.capture_logic_frame_id = None;
        self.expected_reply_logic_frame_id = None;
        self.reply_wait_armed = false;
        self.latest_qr_observation = None;
        self.qr_acquired = false;
        self.qr_capture_parent_logic_frame_id = None;
        self.pending_reply = None;
        self.retained_pass = None;
        self.latest_root_list_diagnostic = None;
    }
}

impl LauncherApp {
    pub(super) fn qa_framebuffer_use_empty_paste_invocation_context(&self) -> bool {
        self.qa_framebuffer
            .as_ref()
            .is_some_and(FramebufferProbe::use_empty_paste_invocation_context)
    }

    pub fn install_qa_framebuffer_request(
        &mut self,
        request: QaFramebufferRequest,
    ) -> Result<(), String> {
        if self.qa_framebuffer.is_some() {
            return Err("a QA framebuffer request is already installed".into());
        }
        self.qa_framebuffer = Some(FramebufferProbe::new(request, self.egui_ctx.clone())?);
        Ok(())
    }

    pub(super) fn qa_framebuffer_record_activation(
        &mut self,
        action: &crate::actions::Action,
        query_override: Option<&str>,
        source: crate::commands::ActivationSource,
    ) {
        if let Some(probe) = self.qa_framebuffer.as_mut() {
            probe.record_activation(action, query_override, source);
        }
    }

    pub(super) fn qa_framebuffer_record_enter_processed(&mut self) {
        if let Some(probe) = self.qa_framebuffer.as_mut() {
            probe.record_enter_processed();
        }
    }

    pub(super) fn qa_framebuffer_record_tab_processed(&mut self) {
        if let Some(probe) = self.qa_framebuffer.as_mut() {
            probe.record_tab_processed();
        }
    }

    pub(super) fn qa_framebuffer_root_diagnostics_enabled(&self) -> bool {
        self.qa_framebuffer
            .as_ref()
            .is_some_and(FramebufferProbe::root_diagnostics_active)
    }

    pub(super) fn qa_framebuffer_record_root_list_diagnostic(
        &mut self,
        diagnostic: RootListDiagnostic,
        geometry_rebuilt: bool,
    ) {
        if let Some(probe) = self.qa_framebuffer.as_mut() {
            probe.record_root_list_diagnostic(diagnostic, geometry_rebuilt);
        }
    }

    pub(super) fn qa_framebuffer_begin_root_logic(&mut self, ctx: &egui::Context) {
        if !self
            .qa_framebuffer
            .as_ref()
            .is_some_and(FramebufferProbe::needs_root_logic)
        {
            return;
        }
        let eligibility = self
            .qa_framebuffer
            .as_ref()
            .filter(|probe| probe.needs_input_eligibility())
            .map(|_| {
                surface_eligibility(
                    self,
                    ctx,
                    self.visible_flag.load(std::sync::atomic::Ordering::Relaxed),
                )
            });
        if let Some(probe) = self.qa_framebuffer.as_mut() {
            probe.begin_root_logic(ctx, eligibility);
        }
    }

    pub(super) fn qa_framebuffer_revalidate_root_logic(
        &mut self,
        ctx: &egui::Context,
        logical_root_visible: bool,
    ) {
        if !self
            .qa_framebuffer
            .as_ref()
            .is_some_and(FramebufferProbe::phase_requires_eligibility)
        {
            return;
        }
        let eligibility = surface_eligibility(self, ctx, logical_root_visible);
        if let Some(probe) = self.qa_framebuffer.as_mut() {
            probe.revalidate_root_logic(eligibility);
        }
    }

    pub(super) fn qa_framebuffer_run_hidden_logic_tick(&mut self) {
        if let Some(probe) = self
            .qa_framebuffer
            .as_mut()
            .filter(|probe| probe.phase_requires_eligibility() || probe.phase == ProbePhase::Saving)
        {
            probe.run_hidden_logic_tick();
        }
    }

    pub(super) fn qa_framebuffer_start_ui_pass(&mut self, pass_index: u32) {
        if let Some(probe) = self.qa_framebuffer.as_mut() {
            probe.start_ui_pass(pass_index);
        }
    }

    pub(super) fn qa_framebuffer_needs_qr_observation(&self) -> bool {
        self.qa_framebuffer
            .as_ref()
            .is_some_and(|probe| probe.target == CaptureTarget::Qr && probe.needs_root_logic())
    }

    pub(super) fn qa_framebuffer_after_qr_show(
        &mut self,
        ctx: &egui::Context,
        logical_root_visible: bool,
        qr_open: bool,
        observation: Option<QrViewportObservation>,
    ) {
        let Some((logic_frame_id, ui_pass_id, ui_pass_index)) = self
            .qa_framebuffer
            .as_ref()
            .filter(|probe| probe.target == CaptureTarget::Qr && probe.needs_root_logic())
            .and_then(|probe| {
                probe
                    .current_ui_pass_id
                    .map(|pass_id| (probe.logic_frame_id, pass_id, probe.current_ui_pass_index))
            })
        else {
            return;
        };
        let root_eligibility = surface_eligibility(self, ctx, logical_root_visible);
        let evidence = frame_evidence(
            self,
            ctx,
            logic_frame_id,
            ui_pass_id,
            ui_pass_index,
            "qr_child_callback_observed",
            logical_root_visible,
            root_eligibility,
        );
        if let Some(probe) = self.qa_framebuffer.as_mut() {
            probe.after_qr_show(ctx, root_eligibility, qr_open, observation, Some(evidence));
        }
    }

    pub(super) fn qa_framebuffer_issue_if_ready(
        &mut self,
        ctx: &egui::Context,
        logical_root_visible: bool,
    ) {
        let Some((logic_frame_id, ui_pass_id, ui_pass_index)) = self
            .qa_framebuffer
            .as_ref()
            .filter(|probe| probe.target == CaptureTarget::Root && probe.phase == ProbePhase::Ready)
            .and_then(|probe| {
                probe
                    .current_ui_pass_id
                    .map(|pass_id| (probe.logic_frame_id, pass_id, probe.current_ui_pass_index))
            })
        else {
            return;
        };
        let eligibility = surface_eligibility(self, ctx, logical_root_visible);
        let mut evidence = frame_evidence(
            self,
            ctx,
            logic_frame_id,
            ui_pass_id,
            ui_pass_index,
            "screenshot_issued",
            logical_root_visible,
            eligibility,
        );
        let (jump_root_owner_allowed, jump_mode_enabled, jump_session_active, activation_count) =
            self.qa_framebuffer
                .as_ref()
                .map_or((false, false, false, 0), |probe| {
                    (
                        self.qa_framebuffer_jump_root_owner_allowed(ctx, logical_root_visible),
                        self.jump_mode_enabled(),
                        self.jump_mode_ui.active_session().is_some(),
                        probe.activation_count,
                    )
                });
        let observation = self
            .qa_framebuffer
            .as_ref()
            .and_then(|probe| probe.latest_jump_mode_observation.clone());
        evidence.jump_mode_observation = observation.clone();
        let live_observation_matches = self.qa_framebuffer.as_ref().is_none_or(|probe| {
            !probe.wait_for_active_hints
                || (jump_root_owner_allowed
                    && jump_mode_enabled
                    && jump_session_active
                    && self.query == evidence.query
                    && observation.as_ref().is_some_and(|observation| {
                        active_query_hint_snapshot(observation)
                            && observation.query == self.query
                            && observation.activation_count == activation_count
                    }))
        });
        if let Some(probe) = self.qa_framebuffer.as_mut() {
            if !live_observation_matches {
                probe.fail("Jump Mode query state regressed before screenshot issue");
            } else {
                probe.issue_screenshot_if_ready(
                    ctx,
                    eligibility,
                    jump_root_owner_allowed,
                    evidence,
                );
            }
        }
    }

    pub(super) fn qa_framebuffer_finish_ui_pass(
        &mut self,
        ctx: &egui::Context,
        logical_root_visible: bool,
    ) {
        let Some((
            logic_frame_id,
            ui_pass_id,
            ui_pass_index,
            should_record_evidence,
            observe_jump_mode,
            activation_count,
            last_action,
            process_pid,
            fixture_manifest_sha256,
            capture_index,
            capture_count,
            capture_armed,
        )) = self
            .qa_framebuffer
            .as_ref()
            .filter(|probe| probe.needs_retained_marker())
            .and_then(|probe| {
                probe.current_ui_pass_id.map(|pass_id| {
                    (
                        probe.logic_frame_id,
                        pass_id,
                        probe.current_ui_pass_index,
                        probe.phase == ProbePhase::Issued
                            && probe
                                .issue_logic_frame_id
                                .is_some_and(|issue_frame| probe.logic_frame_id > issue_frame),
                        probe.jump_mode_observation_enabled,
                        probe.activation_count,
                        probe.last_action.clone(),
                        probe.receipt.process.pid,
                        probe.receipt.fixture_manifest_sha256.clone(),
                        probe.receipt.capture_index,
                        probe.capture_count,
                        probe.phase == ProbePhase::Armed,
                    )
                })
            })
        else {
            return;
        };
        let eligibility = surface_eligibility(self, ctx, logical_root_visible);
        let mut evidence = (should_record_evidence || observe_jump_mode).then(|| {
            frame_evidence(
                self,
                ctx,
                logic_frame_id,
                ui_pass_id,
                ui_pass_index,
                "retained_ui_pass_candidate",
                logical_root_visible,
                eligibility,
            )
        });
        let jump_root_owner_allowed =
            self.qa_framebuffer_jump_root_owner_allowed(ctx, logical_root_visible);
        if let Some(evidence) = evidence.as_mut()
            && observe_jump_mode
        {
            evidence.jump_mode_observation = Some(build_jump_mode_observation(
                self,
                ctx,
                logic_frame_id,
                ui_pass_id,
                ui_pass_index,
                jump_root_owner_allowed,
                self.jump_mode_ui.pass_registry(),
                activation_count,
                last_action,
                process_pid,
                fixture_manifest_sha256,
                capture_index,
                capture_count,
                capture_armed,
            ));
        }
        if let Some(probe) = self.qa_framebuffer.as_mut() {
            probe.finish_ui_pass(eligibility, evidence);
        }
    }

    pub(super) fn qa_framebuffer_finish_retained_ui_frame(&mut self) {
        if !self
            .qa_framebuffer
            .as_ref()
            .is_some_and(FramebufferProbe::needs_retained_marker)
        {
            return;
        }
        let ctx = self.egui_ctx.clone();
        let logical_root_visible = self
            .qa_framebuffer
            .as_ref()
            .and_then(|probe| probe.retained_pass.as_ref())
            .is_some_and(|marker| marker.eligibility.logical_root_visible);
        let eligibility = surface_eligibility(self, &ctx, logical_root_visible);
        let observation_details = self.qa_framebuffer.as_ref().and_then(|probe| {
            probe.retained_pass.as_ref().map(|marker| {
                (
                    marker.logic_frame_id,
                    marker.ui_pass_id,
                    marker.ui_pass_index,
                    probe.jump_mode_observation_enabled,
                    probe.activation_count,
                    probe.last_action.clone(),
                    probe.receipt.process.pid,
                    probe.receipt.fixture_manifest_sha256.clone(),
                    probe.receipt.capture_index,
                    probe.capture_count,
                    probe.phase == ProbePhase::Armed,
                )
            })
        });
        let jump_mode_observation = observation_details.and_then(
            |(
                logic_frame_id,
                ui_pass_id,
                ui_pass_index,
                enabled,
                activation_count,
                last_action,
                process_pid,
                fixture_hash,
                capture_index,
                capture_count,
                capture_armed,
            )| {
                enabled.then(|| {
                    let root_owner_allowed =
                        self.qa_framebuffer_jump_root_owner_allowed(&ctx, logical_root_visible);
                    build_jump_mode_observation(
                        self,
                        &ctx,
                        logic_frame_id,
                        ui_pass_id,
                        ui_pass_index,
                        root_owner_allowed,
                        self.jump_mode_ui.pass_registry(),
                        activation_count,
                        last_action,
                        process_pid,
                        fixture_hash,
                        capture_index,
                        capture_count,
                        capture_armed,
                    )
                })
            },
        );
        if let Some(probe) = self.qa_framebuffer.as_mut() {
            probe.finish_retained_ui_frame(&ctx, eligibility, jump_mode_observation);
        }
    }
}

fn viewport_class_name(class: egui::ViewportClass) -> &'static str {
    match class {
        egui::ViewportClass::Root => "root",
        egui::ViewportClass::Deferred => "deferred",
        egui::ViewportClass::Immediate => "immediate",
        egui::ViewportClass::EmbeddedWindow => "embedded_window",
    }
}

fn qr_render_evidence(
    parent_logic_frame_id: u64,
    observation: &QrViewportObservation,
) -> QrRenderEvidence {
    let actual_target = observation.callback_class == egui::ViewportClass::Immediate
        && observation.child_viewport_id == super::qr_dialog::viewport_id();
    let eligible = actual_target
        && observation.body_rendered
        && observation.still_open
        && observation.visible.unwrap_or(true)
        && !observation.minimized.unwrap_or(false)
        && !observation.occluded.unwrap_or(false);
    QrRenderEvidence {
        parent_logic_frame_id,
        child_viewport_id: format!("{:?}", observation.child_viewport_id),
        callback_class: viewport_class_name(observation.callback_class),
        callback_frame_nr: observation.callback_frame_nr,
        callback_pass_nr: observation.callback_pass_nr,
        callback_pass_index: observation.callback_pass_index,
        completed_frame_nr: observation.completed_frame_nr,
        completed_pass_nr: observation.completed_pass_nr,
        visible: observation.visible,
        minimized: observation.minimized,
        occluded: observation.occluded,
        focused: observation.focused,
        body_rendered: observation.body_rendered,
        still_open: observation.still_open,
        eligible,
        callback_observed_at_unix_ms: observation.callback_observed_at_unix_ms,
        renderer_returned_at_unix_ms: observation.renderer_returned_at_unix_ms,
    }
}

fn qr_eligibility_failure(observation: &QrViewportObservation) -> &'static str {
    if observation.callback_class != egui::ViewportClass::Immediate {
        "selected QR viewport callback was not an immediate native child"
    } else if observation.child_viewport_id != super::qr_dialog::viewport_id() {
        "selected QR viewport callback had an unexpected child identity"
    } else if !observation.body_rendered {
        "selected QR viewport did not render its dialog body"
    } else if !observation.still_open {
        "QR dialog closed before the selected child capture completed"
    } else if observation.visible == Some(false)
        || observation.minimized == Some(true)
        || observation.occluded == Some(true)
    {
        "selected QR viewport was explicitly hidden, minimized, or occluded"
    } else {
        "selected QR viewport was not eligible for native capture"
    }
}

fn qr_child_advanced(baseline: &QrRenderEvidence, current: &QrRenderEvidence) -> bool {
    let frame_advanced = baseline
        .completed_frame_nr
        .zip(current.completed_frame_nr)
        .is_some_and(|(before, after)| after > before);
    let pass_advanced = baseline
        .completed_pass_nr
        .zip(current.completed_pass_nr)
        .is_some_and(|(before, after)| after > before);
    frame_advanced || pass_advanced
}

fn merge_eligibility(
    retained: SurfaceEligibility,
    current: SurfaceEligibility,
) -> SurfaceEligibility {
    SurfaceEligibility {
        logical_root_visible: retained.logical_root_visible && current.logical_root_visible,
        root_visibility_reported: current.root_visibility_reported,
        root_visible: retained.root_visible && current.root_visible,
        ocr_owner: retained.ocr_owner || current.ocr_owner,
        tool_owner: retained.tool_owner || current.tool_owner,
        modal_or_key_capture_owner: retained.modal_or_key_capture_owner
            || current.modal_or_key_capture_owner,
    }
}

fn surface_eligibility(
    app: &LauncherApp,
    ctx: &egui::Context,
    logical_root_visible: bool,
) -> SurfaceEligibility {
    let root_visibility_reported = root_viewport_visibility_reported(ctx);
    SurfaceEligibility {
        logical_root_visible: logical_root_visible
            && app.visible_flag.load(std::sync::atomic::Ordering::Relaxed),
        root_visibility_reported,
        root_visible: root_visibility_from_reported(root_visibility_reported),
        ocr_owner: app.ocr_owns_root() || app.ocr_surface_visible(),
        tool_owner: app.color_pick_owns_root()
            || app.coordinate_tool.capture_pending()
            || app.coordinate_capture_parking.is_some()
            || app.screen_draw_launcher_parking.is_some()
            || app.screen_draw_controller.toolbar_open(),
        modal_or_key_capture_owner: app.confirm_modal.is_open()
            || app.action_sheet.is_open()
            || app.snippet_prompt_dialog.is_open()
            || app.macro_prompt.pending.is_some()
            || app.macro_parameter_prompt.owns_input()
            || app.mkmacro_dialog.hotkey_capture
            || app.mkmacro_dialog.record_hotkey_capture
            || app.mkmacro_dialog.pause_record_hotkey_capture
            || app.mkmacro_dialog.marker_record_hotkey_capture,
    }
}

fn root_viewport_visibility_reported(ctx: &egui::Context) -> Option<bool> {
    ctx.input(|input| input.viewport().visible())
}

fn root_visibility_from_reported(reported: Option<bool>) -> bool {
    // Match eframe's Glow integration: unknown occlusion is not evidence that
    // the viewport is hidden. Explicit minimized or occluded states still win.
    reported.unwrap_or(true)
}

fn frame_evidence(
    app: &LauncherApp,
    ctx: &egui::Context,
    logic_frame_id: u64,
    ui_pass_id: u64,
    ui_pass_index: u32,
    evidence_stage: &'static str,
    logical_root_visible: bool,
    eligibility: SurfaceEligibility,
) -> FrameEvidence {
    FrameEvidence {
        viewport_id: "ROOT",
        logic_frame_id,
        ui_pass_id,
        ui_pass_index,
        evidence_stage,
        completed_at_unix_ms: unix_millis(),
        query: app.query.clone(),
        result_generation: app.root_list_geometry.result_generation(),
        pixels_per_point: Some(ctx.pixels_per_point()),
        native_pixels_per_point: ctx.native_pixels_per_point(),
        zoom_factor: ctx.zoom_factor(),
        logical_root_visible: eligibility.logical_root_visible && logical_root_visible,
        root_visibility_reported: eligibility.root_visibility_reported,
        root_visible: eligibility.root_visible,
        viewport_focused: ctx.input(|input| input.viewport().focused.unwrap_or(false)),
        ocr_owner: eligibility.ocr_owner,
        tool_owner: eligibility.tool_owner,
        modal_or_key_capture_owner: eligibility.modal_or_key_capture_owner,
        eligible: eligibility.eligible(),
        jump_mode_observation: None,
        qr_observation: None,
    }
}

fn active_query_hint_snapshot(observation: &JumpModeObservation) -> bool {
    observation.root_focused
        && observation.root_owner_allowed
        && observation.jump_mode_enabled
        && observation.active_session
        && observation.query_target_id.is_some()
        && observation.targets.iter().any(|target| {
            target.is_query_input
                && target.kind == "text_entry"
                && target.eligible
                && target.hint.is_some()
        })
}

fn write_jump_mode_observation(
    output_dir: &Path,
    observation: &JumpModeObservation,
) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(observation).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    fs::write(output_dir.join(JUMP_OBSERVATION_FILE_NAME), bytes)
}

fn build_jump_mode_observation(
    app: &LauncherApp,
    ctx: &egui::Context,
    logic_frame_id: u64,
    ui_pass_id: u64,
    ui_pass_index: u32,
    root_owner_allowed: bool,
    registry: Option<&super::jump_mode::ui::TargetRegistry>,
    activation_count: u64,
    last_action: Option<QaActivationObservation>,
    process_pid: u32,
    fixture_manifest_sha256: String,
    capture_index: u8,
    capture_count: u8,
    capture_armed: bool,
) -> JumpModeObservation {
    let root_focused = ctx.input(|input| input.focused);
    let active_session = app.jump_mode_ui.active_session();
    let active_hint_map = active_session.map(|session| session.hint_map());
    let focused_widget_id = ctx
        .memory(|memory| memory.focused())
        .map(|focused| format!("{focused:?}"));
    let focused_egui_id = ctx.memory(|memory| memory.focused());
    let result_generation = app.root_list_geometry.result_generation();
    let result_count = app.results.len();
    let selected_index = app.selected;
    let selected_action = selected_index
        .filter(|index| *index < result_count)
        .and_then(|index| app.results.get(index))
        .map(qa_action_observation);
    let (tab_processed_count, last_tab_processed_logic_frame_id) = app
        .qa_framebuffer
        .as_ref()
        .map(|probe| {
            (
                probe.tab_processed_count,
                probe.last_tab_processed_logic_frame_id,
            )
        })
        .unwrap_or_default();
    let (enter_processed_count, last_enter_processed_logic_frame_id) = app
        .qa_framebuffer
        .as_ref()
        .map(|probe| {
            (
                probe.enter_processed_count,
                probe.last_enter_processed_logic_frame_id,
            )
        })
        .unwrap_or_default();
    let opened_note_slugs = app
        .note_panels
        .iter()
        .map(|panel| panel.note_slug().to_owned())
        .collect();
    let navigation_owner = app.jump_mode_ui.frame_navigation_owner();
    let collected_targets: Vec<_> = registry
        .into_iter()
        .flat_map(|registry| registry.targets())
        .map(|target| {
            observe_registered_target(
                app,
                target,
                result_generation,
                result_count,
                active_hint_map,
                focused_egui_id,
            )
        })
        .collect();
    let targets: Vec<_> = if root_owner_allowed {
        collected_targets.clone()
    } else {
        Vec::new()
    };
    let navigation_targets: Vec<_> = registry
        .into_iter()
        .flat_map(|registry| registry.targets())
        .filter(|target| navigation_owner.owns_surface(target.metadata().id().surface()))
        .map(|target| {
            observe_registered_target(
                app,
                target,
                result_generation,
                result_count,
                active_hint_map,
                focused_egui_id,
            )
        })
        .collect();
    let query_target = targets.iter().find(|target| target.is_query_input);
    let focused_destination = targets
        .iter()
        .find(|target| target.focused)
        .map(|target| target.target_id.clone());
    let focused_navigation_destination = navigation_targets
        .iter()
        .find(|target| target.focused)
        .map(|target| target.target_id.clone());
    let dialog_state = qa_dialog_state(app, ctx, navigation_owner, &navigation_targets);
    let selected_target_id = selected_index.and_then(|selected_index| {
        targets
            .iter()
            .find(|target| target.root_result_index == Some(selected_index))
            .map(|target| target.target_id.clone())
    });
    let prefix_target_ids = active_session
        .map(|session| {
            let prefix = session.prefix();
            targets
                .iter()
                .filter(|target| {
                    target.eligible
                        && target
                            .hint
                            .as_deref()
                            .is_some_and(|hint| hint.starts_with(prefix))
                })
                .map(|target| target.target_id.clone())
                .collect()
        })
        .unwrap_or_default();
    let outcome = app
        .jump_mode_ui
        .last_outcome()
        .map(|outcome| match outcome {
            super::jump_mode::MatchOutcome::NoTargets => JumpModeOutcomeObservation::NoTargets,
            super::jump_mode::MatchOutcome::Partial { matching_targets } => {
                JumpModeOutcomeObservation::Partial {
                    matching_targets: *matching_targets,
                }
            }
            super::jump_mode::MatchOutcome::Completed(target) => {
                JumpModeOutcomeObservation::Completed {
                    target_id: format!("{target:?}"),
                }
            }
            super::jump_mode::MatchOutcome::Invalid => JumpModeOutcomeObservation::Invalid,
            super::jump_mode::MatchOutcome::NoTarget => JumpModeOutcomeObservation::NoTarget,
            super::jump_mode::MatchOutcome::Cancelled => JumpModeOutcomeObservation::Cancelled,
            super::jump_mode::MatchOutcome::SessionClosed => {
                JumpModeOutcomeObservation::SessionClosed
            }
        });

    JumpModeObservation {
        observed_at_unix_ms: unix_millis(),
        fixture_manifest_sha256,
        process_pid,
        logic_frame_id,
        ui_pass_id,
        ui_pass_index,
        capture_index,
        capture_count,
        capture_armed,
        query: app.query.clone(),
        effective_fuzzy_weight: app.fuzzy_weight,
        result_generation,
        result_count,
        grid_layout: app.resolved_grid_layout,
        selected_index,
        selected_target_id,
        selected_action,
        opened_note_slugs,
        root_focused,
        root_owner_allowed,
        navigation_owner: qa_navigation_owner(navigation_owner),
        jump_mode_enabled: app.jump_mode_ui.is_enabled(),
        active_session: active_session.is_some(),
        prefix: active_session.map(|session| session.prefix().to_owned()),
        outcome,
        query_target_id: query_target.map(|target| target.target_id.clone()),
        query_focused: ctx.memory(|memory| memory.has_focus(egui::Id::new("query_input"))),
        focused_destination,
        focused_widget_id,
        focused_navigation_destination,
        dialog_state,
        mkmacro_action_editor_open: app.mkmacro_dialog.action_editor.draft.is_some(),
        activation_count,
        last_action,
        tab_processed_count,
        last_tab_processed_logic_frame_id,
        enter_processed_count,
        last_enter_processed_logic_frame_id,
        prefix_target_ids,
        targets,
        navigation_targets,
    }
}

fn observe_registered_target(
    app: &LauncherApp,
    target: &super::jump_mode::ui::RegisteredTarget,
    result_generation: u64,
    result_count: usize,
    active_hint_map: Option<&super::jump_mode::HintMap>,
    focused_egui_id: Option<egui::Id>,
) -> JumpModeTargetObservation {
    let rect = target.rect();
    let clipped_rect = target.clipped_rect();
    let metadata = target.metadata();
    let target_id = metadata.id();
    let is_query_input = target.response_id() == egui::Id::new("query_input")
        && metadata.kind() == super::jump_mode::TargetKind::TextEntry;
    let root_result_index = match target_id.semantic() {
        super::jump_mode::SemanticTargetId::RootResult(index)
            if target_id.generation().0 == result_generation && *index < result_count =>
        {
            Some(*index)
        }
        _ => None,
    };
    let root_action = root_result_index
        .and_then(|index| app.results.get(index))
        .map(qa_action_observation);
    JumpModeTargetObservation {
        target_id: format!("{:?}", target.id()),
        semantic_id: qa_semantic_target(target_id.semantic()),
        target_generation: target_id.generation().0,
        surface_identity: qa_surface_identity(target_id.surface()),
        surface: target_surface_name(target_id.surface()),
        label: metadata.label().to_owned(),
        kind: target_kind_name(metadata.kind()),
        rect: [rect.min.x, rect.min.y, rect.max.x, rect.max.y],
        clipped_rect: [
            clipped_rect.min.x,
            clipped_rect.min.y,
            clipped_rect.max.x,
            clipped_rect.max.y,
        ],
        eligible: target.eligible() && target.enabled(),
        is_query_input,
        focused: Some(target.response_id()) == focused_egui_id,
        hint: active_hint_map
            .and_then(|hint_map| hint_map.hint_for(target.id()))
            .map(|hint| hint.as_str().to_owned()),
        root_result_index,
        root_action,
    }
}

fn qa_navigation_owner(owner: super::jump_mode::ui::NavigationOwner) -> QaNavigationOwner {
    use super::jump_mode::ui::NavigationOwner as Owner;
    let (kind, instance) = match owner {
        Owner::Root => ("root", None),
        Owner::Settings { instance } => ("settings", Some(instance)),
        Owner::Notes { instance } => ("notes", Some(instance)),
        Owner::Todo { instance } => ("todo", Some(instance)),
        Owner::Timer { instance } => ("timer", Some(instance)),
        Owner::PasteSequence { instance } => ("paste_sequence", Some(instance)),
        Owner::MkMacro { instance } => ("mkmacro", Some(instance)),
        Owner::Dialog { kind, instance } => (kind.surface_kind(), Some(instance)),
        Owner::SnippetRemoval {
            confirmation_instance,
            ..
        } => ("snippet_removal", Some(confirmation_instance)),
        Owner::ClipboardRemoval {
            confirmation_instance,
            ..
        } => ("clipboard_removal", Some(confirmation_instance)),
        Owner::MouseGestureBinding {
            binding_instance, ..
        } => ("mouse_gesture_binding", Some(binding_instance)),
        Owner::MouseGestureDelete {
            confirmation_instance,
            ..
        } => ("mouse_gesture_delete", Some(confirmation_instance)),
        Owner::XmlOverwriteConfirmation {
            confirmation_instance,
            ..
        } => ("xml_overwrite_confirmation", Some(confirmation_instance)),
        Owner::SnippetPrompt { generation } => ("snippet_prompt", Some(generation)),
        Owner::Blocked => ("blocked", None),
    };
    QaNavigationOwner { kind, instance }
}

fn qa_semantic_target(semantic: &super::jump_mode::SemanticTargetId) -> QaSemanticTarget {
    match semantic {
        super::jump_mode::SemanticTargetId::Named(value) => QaSemanticTarget::Named(value.clone()),
        super::jump_mode::SemanticTargetId::RootResult(index) => {
            QaSemanticTarget::RootResult(*index)
        }
    }
}

fn qa_surface_identity(surface: &super::jump_mode::SurfaceId) -> QaSurfaceIdentity {
    match surface {
        super::jump_mode::SurfaceId::Dashboard => QaSurfaceIdentity::Dashboard,
        super::jump_mode::SurfaceId::LauncherResults => QaSurfaceIdentity::LauncherResults,
        super::jump_mode::SurfaceId::Settings { instance } => QaSurfaceIdentity::Settings {
            instance: *instance,
        },
        super::jump_mode::SurfaceId::Dialog { kind, instance } => QaSurfaceIdentity::Dialog {
            dialog_kind: kind.clone(),
            instance: *instance,
        },
        super::jump_mode::SurfaceId::Popup {
            kind,
            instance,
            owner,
        } => QaSurfaceIdentity::Popup {
            popup_kind: kind.clone(),
            instance: *instance,
            owner: Box::new(qa_surface_identity(owner)),
        },
    }
}

fn qa_dialog_state(
    app: &LauncherApp,
    ctx: &egui::Context,
    owner: super::jump_mode::ui::NavigationOwner,
    targets: &[JumpModeTargetObservation],
) -> Option<QaDialogState> {
    use super::jump_mode::ui::NavigationOwner as Owner;
    let (surface, instance) = match owner {
        Owner::Root | Owner::Blocked => return None,
        Owner::Settings { instance } => ("settings", instance),
        Owner::Notes { instance } => ("notes", instance),
        Owner::Todo { instance } => ("todo", instance),
        Owner::Timer { instance } => ("timer", instance),
        Owner::PasteSequence { instance } => ("paste_sequence", instance),
        Owner::MkMacro { instance } => ("mkmacro", instance),
        Owner::Dialog { kind, instance } => (kind.surface_kind(), instance),
        Owner::SnippetRemoval {
            confirmation_instance,
            ..
        } => ("snippet-removal", confirmation_instance),
        Owner::ClipboardRemoval {
            confirmation_instance,
            ..
        } => ("clipboard-removal", confirmation_instance),
        Owner::MouseGestureBinding {
            binding_instance, ..
        } => ("mouse-gesture-binding", binding_instance),
        Owner::MouseGestureDelete {
            confirmation_instance,
            ..
        } => ("mouse-gesture-delete", confirmation_instance),
        Owner::XmlOverwriteConfirmation {
            confirmation_instance,
            ..
        } => ("xml-overwrite-confirmation", confirmation_instance),
        Owner::SnippetPrompt { generation } => ("snippet-prompt", generation),
    };
    let mut state = QaDialogState {
        surface: Some(surface),
        instance: Some(instance),
        target_generation: targets.first().map(|target| target.target_generation),
        ..Default::default()
    };
    match owner {
        Owner::Settings { .. } => {
            state.settings_draft_jump_mode =
                Some(app.settings_editor.experimental_jump_mode_enabled);
            state.settings_committed_jump_mode = Some(app.jump_mode_ui.is_enabled());
            state.settings_draft_fuzzy_weight = Some(app.settings_editor.fuzzy_weight);
            state.settings_committed_fuzzy_weight = Some(app.fuzzy_weight);
            state.settings_draft_debug_logging = Some(app.settings_editor.debug_logging);
            state.settings_committed_debug_logging =
                Some(app.settings_editor.baseline_settings.debug_logging);
        }
        Owner::Notes { .. } => {
            let observation = app.notes_dialog.qa_observation();
            state.notes_open = Some(observation.open);
            state.notes_entry_count = Some(observation.entry_count);
            state.notes_editing = Some(observation.editing);
            state.notes_draft_chars = Some(observation.draft_text_chars);
            state.notes_draft_newlines = Some(observation.draft_newline_count);
        }
        Owner::Todo { .. } => {
            let observation = app.todo_dialog.qa_observation();
            state.todo_open = Some(observation.open);
            state.todo_entry_ids = Some(observation.entry_ids);
            state.todo_pending_clear_confirmation = Some(observation.pending_clear_confirmation);
            state.todo_draft_text_chars = Some(observation.draft_text_chars);
            state.todo_draft_newlines = Some(observation.draft_newline_count);
            state.target_generation = Some(app.todo_dialog.jump_mode_target_generation().0);
        }
        Owner::Timer { .. } => {
            let observation = app.timer_dialog.qa_observation();
            state.timer_open = Some(observation.open);
            state.timer_mode = Some(observation.mode);
            state.timer_duration_chars = Some(observation.duration_text_chars);
            state.timer_label_chars = Some(observation.label_text_chars);
            state.timer_sound_index = Some(observation.sound_index);
            state.active_timer_count = Some(crate::plugins::timer::active_timers().len());
            state.active_alarm_count = Some(
                crate::plugins::timer::ACTIVE_TIMERS
                    .lock()
                    .map(|timers| timers.values().filter(|entry| entry.persist).count())
                    .unwrap_or(usize::MAX),
            );
        }
        Owner::PasteSequence { .. } => {
            let observation = app.paste_sequence_dialog.qa_observation();
            state.paste_open = Some(observation.open);
            state.paste_context_present = Some(observation.invocation_context_present);
            state.paste_context_has_clipboard_text =
                Some(observation.invocation_context_has_clipboard_text);
            state.paste_saved_sequence_ids = Some(observation.saved_sequence_ids);
            state.paste_selected_sequence_id = observation.selected_sequence_id;
            state.paste_draft_step_count = Some(observation.draft_step_count);
            state.paste_remove_confirmation_open = Some(observation.remove_confirmation_open);
        }
        Owner::MkMacro { .. } => {
            let observation = app.mkmacro_dialog.qa_observation(ctx);
            state.mkmacro_open = Some(observation.open);
            state.mkmacro_draft_revision = Some(observation.draft_revision);
            state.mkmacro_selected_id = observation.selected_macro_id;
            state.mkmacro_selected_step_ids = Some(observation.selected_step_ids);
            state.mkmacro_table_focus_id = observation.table_focus_id.map(|id| format!("{id:?}"));
            state.mkmacro_table_focused = Some(observation.table_focused);
            state.mkmacro_document_macro_count = Some(observation.document_macro_count);
            state.mkmacro_active_runtime = Some(observation.runtime_active);
            state.mkmacro_recorder_active = Some(observation.recorder_active);
            state.mkmacro_action_editor_open =
                Some(app.mkmacro_dialog.action_editor.draft.is_some());
        }
        Owner::Dialog {
            kind: super::jump_mode::ui::NavigationDialogKind::FileSearch,
            ..
        } => {
            let search = &app.file_search_dialog;
            state.file_search_open = Some(search.open);
            state.file_search_query_char_count = Some(search.search_text.chars().count());
            state.file_search_status = Some(match search.current_status {
                crate::file_search::model::SearchStatus::Pending => "pending",
                crate::file_search::model::SearchStatus::Running => "running",
                crate::file_search::model::SearchStatus::Completed => "completed",
                crate::file_search::model::SearchStatus::Cancelled => "cancelled",
                crate::file_search::model::SearchStatus::Failed => "failed",
            });
            state.file_search_search_id = search.active_search_id.map(|id| id.0);
            state.file_search_form_generation = Some(search.jump_mode_target_generation().0);
            state.target_generation = state.file_search_form_generation;
        }
        Owner::Dialog { .. }
        | Owner::SnippetRemoval { .. }
        | Owner::ClipboardRemoval { .. }
        | Owner::MouseGestureBinding { .. }
        | Owner::MouseGestureDelete { .. }
        | Owner::XmlOverwriteConfirmation { .. }
        | Owner::SnippetPrompt { .. } => {}
        Owner::Root | Owner::Blocked => unreachable!("handled above"),
    }
    Some(state)
}

fn qa_action_observation(action: &crate::actions::Action) -> QaActionObservation {
    QaActionObservation {
        label: action.label.clone(),
        desc: action.desc.clone(),
        action: action.action.clone(),
        args: action.args.clone(),
    }
}

fn target_kind_name(kind: super::jump_mode::TargetKind) -> &'static str {
    match kind {
        super::jump_mode::TargetKind::Action => "action",
        super::jump_mode::TargetKind::Button => "button",
        super::jump_mode::TargetKind::TextEntry => "text_entry",
        super::jump_mode::TargetKind::Selectable => "selectable",
        super::jump_mode::TargetKind::Checkbox => "checkbox",
        super::jump_mode::TargetKind::Slider => "slider",
        super::jump_mode::TargetKind::Tab => "tab",
        super::jump_mode::TargetKind::ComboBox => "combo_box",
    }
}

fn target_surface_name(surface: &super::jump_mode::SurfaceId) -> &'static str {
    match surface {
        super::jump_mode::SurfaceId::Dashboard => "dashboard",
        super::jump_mode::SurfaceId::LauncherResults => "launcher_results",
        super::jump_mode::SurfaceId::Settings { .. } => "settings",
        super::jump_mode::SurfaceId::Dialog { .. } => "dialog",
        super::jump_mode::SurfaceId::Popup { .. } => "popup",
    }
}

fn take_matching_screenshot_event(
    ctx: &egui::Context,
    target_viewport_id: ViewportId,
    token: &UserData,
) -> Option<Arc<ColorImage>> {
    ctx.input_mut_for(target_viewport_id, |input| {
        let mut first_image = None;
        input.events.retain(|event| {
            if let Event::Screenshot {
                viewport_id,
                user_data,
                image,
            } = event
                && *viewport_id == target_viewport_id
                && user_data == token
            {
                if first_image.is_none() {
                    first_image = Some(Arc::clone(image));
                }
                false
            } else {
                true
            }
        });
        first_image
    })
}

fn take_unmodified_f11(ctx: &egui::Context) -> bool {
    ctx.input_mut(|input| {
        let mut pressed = false;
        input.events.retain(|event| {
            if let Event::Key {
                key: Key::F11,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = event
                && *modifiers == Modifiers::NONE
            {
                pressed = true;
                false
            } else {
                true
            }
        });
        pressed
    })
}

enum WriterJob {
    AwaitReply(CompletionReceipt),
    Save {
        receipt: CompletionReceipt,
        image: Arc<ColorImage>,
    },
    Failure(CompletionReceipt),
}

struct WriterOutcome {
    status: ReceiptStatus,
    error: Option<String>,
    receipt_written: bool,
    capture_index: u8,
    capture_count: u8,
}

fn spawn_writer(
    output_dir: PathBuf,
    ctx: egui::Context,
    reply_timeout: Duration,
) -> Result<(SyncSender<WriterJob>, Receiver<WriterOutcome>), String> {
    let (job_tx, job_rx) = mpsc::sync_channel(WRITER_QUEUE_CAPACITY);
    let (outcome_tx, outcome_rx) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("qa-framebuffer-writer".into())
        .spawn(move || {
            while let Ok(job) = job_rx.recv() {
                let job = match job {
                    WriterJob::AwaitReply(mut receipt) => {
                        match job_rx.recv_timeout(reply_timeout) {
                            Ok(reply_job @ WriterJob::Save { .. })
                            | Ok(reply_job @ WriterJob::Failure(_)) => reply_job,
                            Ok(WriterJob::AwaitReply(_)) => {
                                receipt.failure =
                                    Some("duplicate screenshot reply wait was rejected".into());
                                WriterJob::Failure(receipt)
                            }
                            Err(RecvTimeoutError::Timeout) => {
                                let deadline = if receipt.reply_wait_timeout_ms % 1_000 == 0 {
                                    format!(
                                        "{} seconds",
                                        receipt.reply_wait_timeout_ms / 1_000
                                    )
                                } else {
                                    format!("{} milliseconds", receipt.reply_wait_timeout_ms)
                                };
                                receipt.failure = Some(match receipt.target {
                                    CaptureTarget::Root => format!(
                                        "ROOT screenshot reply was not delivered within {deadline}"
                                    ),
                                    CaptureTarget::Qr => format!(
                                        "selected QR capture did not complete within {deadline}"
                                    ),
                                });
                                WriterJob::Failure(receipt)
                            }
                            Err(RecvTimeoutError::Disconnected) => {
                                receipt.failure = Some(match receipt.target {
                                    CaptureTarget::Root => {
                                        "capture owner exited before the ROOT screenshot reply arrived"
                                            .into()
                                    }
                                    CaptureTarget::Qr => {
                                        "capture owner exited before the selected QR child reply arrived"
                                            .into()
                                    }
                                });
                                WriterJob::Failure(receipt)
                            }
                        }
                    }
                    job => job,
                };
                let outcome = write_job(&output_dir, job);
                let continue_sequence = outcome.status == ReceiptStatus::Saved
                    && outcome.receipt_written
                    && outcome.capture_index < outcome.capture_count;
                if outcome_tx.send(outcome).is_err() {
                    break;
                }
                ctx.request_repaint_of(ViewportId::ROOT);
                if !continue_sequence {
                    break;
                }
            }
        })
        .map_err(|error| format!("failed to start bounded QA framebuffer writer: {error}"))?;
    Ok((job_tx, outcome_rx))
}

fn write_job(output_dir: &Path, job: WriterJob) -> WriterOutcome {
    let mut receipt = match job {
        WriterJob::AwaitReply(mut receipt) => {
            receipt.failure = Some("writer received an incomplete screenshot reply wait".into());
            receipt
        }
        WriterJob::Save { mut receipt, image } => {
            let (png_file_name, _) =
                capture_artifact_names(receipt.capture_index, receipt.capture_count);
            let image_dimensions = image_dimensions(&image);
            match image_dimensions.and_then(|(width, height)| encode_png(&image, width, height)) {
                Ok((width, height, png)) => {
                    let png_sha256 = sha256_bytes(&png);
                    let png_path = output_dir.join(&png_file_name);
                    match write_new_file(&png_path, &png) {
                        Ok(()) => {
                            let capture_evidence = receipt.captured.as_ref();
                            receipt.status = ReceiptStatus::Saved;
                            receipt.saved = Some(SaveEvidence {
                                completed_at_unix_ms: unix_millis(),
                                png_file: png_file_name,
                                png_sha256,
                                width,
                                height,
                                pixels_per_point: capture_evidence
                                    .and_then(|evidence| evidence.pixels_per_point),
                                native_pixels_per_point: capture_evidence
                                    .and_then(|evidence| evidence.native_pixels_per_point),
                                zoom_factor: capture_evidence
                                    .map_or(1.0, |evidence| evidence.zoom_factor),
                            });
                            receipt
                        }
                        Err(error) => {
                            receipt.failure = Some(format!(
                                "failed to create PNG exclusively at {}: {error}",
                                png_path.display()
                            ));
                            receipt
                        }
                    }
                }
                Err(error) => {
                    receipt.failure = Some(error);
                    receipt
                }
            }
        }
        WriterJob::Failure(receipt) => receipt,
    };

    let error = receipt.failure.clone();
    let status = receipt.status;
    let capture_index = receipt.capture_index;
    let capture_count = receipt.capture_count;
    let (_, receipt_file_name) = capture_artifact_names(capture_index, capture_count);
    receipt.failure = error.clone();
    let receipt_bytes = match serde_json::to_vec_pretty(&receipt) {
        Ok(bytes) => bytes,
        Err(serialize_error) => {
            return WriterOutcome {
                status: ReceiptStatus::Failed,
                error: Some(format!(
                    "failed to serialize completion receipt: {serialize_error}"
                )),
                receipt_written: false,
                capture_index,
                capture_count,
            };
        }
    };
    match write_new_file(&output_dir.join(receipt_file_name), &receipt_bytes) {
        Ok(()) => WriterOutcome {
            status,
            error,
            receipt_written: true,
            capture_index,
            capture_count,
        },
        Err(write_error) => WriterOutcome {
            status: ReceiptStatus::Failed,
            error: Some(format!(
                "failed to create completion receipt exclusively: {write_error}"
            )),
            receipt_written: false,
            capture_index,
            capture_count,
        },
    }
}

fn capture_artifact_names(capture_index: u8, capture_count: u8) -> (String, String) {
    if capture_count == 1 {
        (PNG_FILE_NAME.into(), RECEIPT_FILE_NAME.into())
    } else {
        (
            format!("framebuffer-{capture_index:03}.png"),
            format!("receipt-{capture_index:03}.json"),
        )
    }
}

fn image_dimensions(image: &ColorImage) -> Result<(u32, u32), String> {
    let [width, height] = image.size;
    let pixel_count = width
        .checked_mul(height)
        .ok_or_else(|| "screenshot dimensions overflow addressable memory".to_owned())?;
    if pixel_count != image.pixels.len() {
        return Err("screenshot pixel count does not match its dimensions".into());
    }
    let width =
        u32::try_from(width).map_err(|_| "screenshot width exceeds PNG limits".to_owned())?;
    let height =
        u32::try_from(height).map_err(|_| "screenshot height exceeds PNG limits".to_owned())?;
    if width == 0 || height == 0 {
        return Err("screenshot dimensions must be nonzero".into());
    }
    Ok((width, height))
}

fn encode_png(image: &ColorImage, width: u32, height: u32) -> Result<(u32, u32, Vec<u8>), String> {
    let byte_len = image
        .pixels
        .len()
        .checked_mul(4)
        .ok_or_else(|| "screenshot RGBA buffer size overflowed".to_owned())?;
    let mut rgba = Vec::with_capacity(byte_len);
    for pixel in &image.pixels {
        rgba.extend_from_slice(&pixel.to_srgba_unmultiplied());
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&rgba, width, height, image::ColorType::Rgba8)
        .map_err(|error| format!("failed to encode screenshot PNG: {error}"))?;
    Ok((width, height, png))
}

fn write_new_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc::TryRecvError;

    fn test_launcher_app(ctx: &egui::Context, data_dir: &Path) -> LauncherApp {
        let mut settings = crate::settings::Settings::default();
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        settings.enable_toasts = true;
        settings.show_error_toasts = true;
        settings.dashboard.config_path = Some(
            data_dir
                .join("dashboard.json")
                .to_string_lossy()
                .into_owned(),
        );
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test_at(data_dir),
            data_dir.join("actions.json").to_string_lossy().into_owned(),
            data_dir
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(true)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn test_request_for_target(
        target: CaptureTarget,
        trigger: CaptureTrigger,
        output_dir: PathBuf,
    ) -> QaFramebufferRequest {
        QaFramebufferRequest {
            manifest: RequestManifest {
                run_id: "test-run".into(),
                request_id: "test-request".into(),
                target,
                trigger,
                capture_count: 1,
                diagnostics: false,
                jump_mode_observation: None,
                output_dir: output_dir.clone(),
                fixture_manifest_sha256: "a".repeat(64),
                build_receipt_identity: "test-build".into(),
            },
            manifest_sha256: "b".repeat(64),
            output_dir,
            executable: ExecutableIdentity {
                path: PathBuf::from("test.exe"),
                sha256: "c".repeat(64),
            },
            process: ProcessIdentity {
                pid: 1,
                probe_initialized_at_unix_ms: 1,
            },
        }
    }

    fn phase2_fixture_json(
        surface: &str,
        plugins: &[&str],
        empty_context: bool,
    ) -> serde_json::Value {
        serde_json::json!({
            "schema_version": 4,
            "fixture_id": format!("jump-mode-m13-{surface}-v1"),
            "scenario": format!("phase2-{surface}"),
            "phase2_surface": surface,
            "qa_empty_paste_invocation_context": empty_context,
            "experimental_jump_mode_enabled": true,
            "enabled_plugins": plugins,
            "action_count": 0,
            "safety": {
                "profile_isolated_by_working_directory": true,
                "notes_path_supplied_by_ml_notes_dir": true,
                "clipboard_plugin_enabled": false,
                "mkmacro_plugin_enabled": surface == "mkmacro",
                "file_search_root_isolated": surface == "file-search",
                "file_search_fixture_file_count": if surface == "file-search" { 1 } else { 0 },
                "file_search_everything_enabled": false,
                "clipboard_modify_plugin_enabled": false,
                "macros_enabled": false,
                "network_plugin_enabled": false,
                "custom_actions_present": false,
                "paste_next_hotkey_enabled": false,
                "paste_cancel_hotkey_enabled": false,
                "qa_empty_paste_invocation_context": empty_context
            }
        })
    }

    #[test]
    fn phase2_fixture_capability_requires_exact_surface_identity_and_safe_allowlist() {
        let request = Phase2DialogRequest {
            surface: Phase2Surface::Paste,
            use_empty_paste_invocation_context: true,
        };
        let safe = phase2_fixture_json("paste", &["notes", "paste_sequence"], true);
        assert!(validate_phase2_fixture_capability(&safe, request).is_ok());

        let unsafe_allowlist =
            phase2_fixture_json("paste", &["notes", "paste_sequence", "clipboard"], true);
        assert!(validate_phase2_fixture_capability(&unsafe_allowlist, request).is_err());

        let mut stale_identity = safe.clone();
        stale_identity["fixture_id"] = serde_json::json!("other-profile");
        assert!(validate_phase2_fixture_capability(&stale_identity, request).is_err());

        let mut missing_isolation = safe;
        missing_isolation["safety"]["profile_isolated_by_working_directory"] =
            serde_json::json!(false);
        assert!(validate_phase2_fixture_capability(&missing_isolation, request).is_err());

        let file_search_request = Phase2DialogRequest {
            surface: Phase2Surface::FileSearch,
            use_empty_paste_invocation_context: false,
        };
        let file_search = phase2_fixture_json("file-search", &["notes", "file_search"], false);
        assert!(validate_phase2_fixture_capability(&file_search, file_search_request).is_ok());
        let mut global_search_enabled = file_search.clone();
        global_search_enabled["safety"]["file_search_everything_enabled"] = serde_json::json!(true);
        assert!(
            validate_phase2_fixture_capability(&global_search_enabled, file_search_request)
                .is_err()
        );
        let mut wrong_file_count = file_search;
        wrong_file_count["safety"]["file_search_fixture_file_count"] = serde_json::json!(2);
        assert!(
            validate_phase2_fixture_capability(&wrong_file_count, file_search_request).is_err()
        );
    }

    #[test]
    fn file_search_dialog_observation_reports_only_redacted_lifecycle_facts() {
        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let mut app = test_launcher_app(&ctx, workspace.root());
        app.file_search_dialog.open = true;
        app.file_search_dialog.search_text = "private-search-query".to_owned();
        app.file_search_dialog.current_status = crate::file_search::model::SearchStatus::Running;
        app.file_search_dialog.active_search_id = Some(crate::file_search::model::SearchId(77));
        app.file_search_dialog.custom_roots = vec!["private-root-path".to_owned()];
        let generation = app.file_search_dialog.jump_mode_target_generation().0;

        let state = qa_dialog_state(
            &app,
            &ctx,
            super::super::jump_mode::ui::NavigationOwner::Dialog {
                kind: super::super::jump_mode::ui::NavigationDialogKind::FileSearch,
                instance: 9,
            },
            &[],
        )
        .expect("the File Search dialog owner has a QA state");
        let serialized = serde_json::to_value(state).expect("File Search QA state serializes");
        assert_eq!(serialized["surface"], "file-search");
        assert_eq!(serialized["instance"], 9);
        assert_eq!(serialized["file_search_open"], true);
        assert_eq!(serialized["file_search_query_char_count"], 20);
        assert_eq!(serialized["file_search_status"], "running");
        assert_eq!(serialized["file_search_search_id"], 77);
        assert_eq!(serialized["file_search_form_generation"], generation);
        assert_eq!(serialized["target_generation"], generation);
        let text = serialized.to_string();
        assert!(!text.contains("private-search-query"));
        assert!(!text.contains("private-root-path"));
    }

    #[test]
    fn typed_paste_open_keeps_bound_empty_context_through_panel_ensure_open() {
        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let mut app = test_launcher_app(&ctx, workspace.root());
        app.paste_sequence_dialog
            .use_test_data_sources(workspace.root().join("paste_sequences.json"));

        let (mut probe, _jobs, _outcomes) =
            test_probe_for_target(CaptureTrigger::F11, CaptureTarget::Root);
        probe.jump_mode_observation_enabled = true;
        probe.phase2_request = Some(Phase2DialogRequest {
            surface: Phase2Surface::Paste,
            use_empty_paste_invocation_context: true,
        });
        app.qa_framebuffer = Some(probe);
        app.test_sequence_context_capture_forbidden = true;

        app.activate_action(
            crate::actions::Action {
                label: "Open Paste Sequences".into(),
                desc: "Paste Sequences".into(),
                action: "paste:open".into(),
                args: None,
            },
            None,
            crate::commands::ActivationSource::Enter,
        );

        assert_eq!(app.test_sequence_context_capture_calls, 0);
        let observation = app.paste_sequence_dialog.qa_observation();
        assert!(observation.open);
        assert!(observation.invocation_context_present);
        assert!(!observation.invocation_context_has_clipboard_text);
    }

    #[test]
    fn phase2_request_is_root_f11_and_empty_context_is_paste_only() {
        let decoded: JumpModeObservationRequest = serde_json::from_str(
            r#"{"wait_for_active_hints":false,"phase2":{"surface":"mkmacro"}}"#,
        )
        .expect("runner's mkmacro surface spelling is part of the request contract");
        assert_eq!(
            decoded.phase2.map(|request| request.surface),
            Some(Phase2Surface::MkMacro)
        );
        let file_search: JumpModeObservationRequest = serde_json::from_str(
            r#"{"wait_for_active_hints":false,"phase2":{"surface":"file-search"}}"#,
        )
        .expect("Phase 2 File Search uses the canonical navigation surface kind");
        assert_eq!(
            file_search.phase2.map(|request| request.surface),
            Some(Phase2Surface::FileSearch)
        );

        let manifest = RequestManifest {
            run_id: "phase2".into(),
            request_id: "paste".into(),
            target: CaptureTarget::Root,
            trigger: CaptureTrigger::F11,
            capture_count: 16,
            diagnostics: false,
            jump_mode_observation: Some(JumpModeObservationRequest {
                wait_for_active_hints: false,
                phase2: Some(Phase2DialogRequest {
                    surface: Phase2Surface::Paste,
                    use_empty_paste_invocation_context: true,
                }),
            }),
            output_dir: PathBuf::from("capture"),
            fixture_manifest_sha256: "a".repeat(64),
            build_receipt_identity: "test-build".into(),
        };
        assert!(validate_manifest(&manifest).is_ok());

        let mut invalid = manifest;
        invalid.jump_mode_observation.as_mut().unwrap().phase2 = Some(Phase2DialogRequest {
            surface: Phase2Surface::Settings,
            use_empty_paste_invocation_context: true,
        });
        assert!(validate_manifest(&invalid).is_err());
    }

    #[test]
    fn todo_dialog_observation_reports_registry_target_generation() {
        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().unwrap();
        let app = test_launcher_app(&ctx, directory.path());
        let expected_generation = app.todo_dialog.jump_mode_target_generation().0;
        let state = qa_dialog_state(
            &app,
            &ctx,
            super::super::jump_mode::ui::NavigationOwner::Todo { instance: 9 },
            &[],
        )
        .unwrap();

        assert_ne!(expected_generation, 0);
        assert_eq!(state.target_generation, Some(expected_generation));
    }

    #[test]
    fn settings_observation_separates_draft_weight_from_effective_runtime_weight() {
        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let mut app = test_launcher_app(&ctx, workspace.root());
        app.fuzzy_weight = 1.75;
        app.settings_editor.fuzzy_weight = 0.75;

        let state = qa_dialog_state(
            &app,
            &ctx,
            super::super::jump_mode::ui::NavigationOwner::Settings { instance: 12 },
            &[],
        )
        .unwrap();

        assert_eq!(state.settings_draft_fuzzy_weight, Some(0.75));
        assert_eq!(state.settings_committed_fuzzy_weight, Some(1.75));
    }

    #[test]
    fn timer_dialog_observation_reports_actual_sound_index() {
        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let mut app = test_launcher_app(&ctx, workspace.root());
        app.timer_dialog.open_timer();

        let state = qa_dialog_state(
            &app,
            &ctx,
            super::super::jump_mode::ui::NavigationOwner::Timer { instance: 12 },
            &[],
        )
        .unwrap();

        assert_eq!(state.timer_sound_index, Some(0));
    }

    #[test]
    fn mkmacro_observation_separates_selected_step_from_actual_table_focus() {
        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().unwrap();
        let mut app = test_launcher_app(&ctx, directory.path());
        app.mkmacro_dialog.create_macro();
        let macro_id = app.mkmacro_dialog.selected_macro_id.unwrap();
        let step_id = 42;
        app.mkmacro_dialog
            .selected_macro_mut()
            .unwrap()
            .steps
            .push(crate::mkmacro::MkStep {
                id: step_id,
                enabled: false,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: crate::mkmacro::MkErrorPolicy::Stop,
                metadata: Default::default(),
                action: crate::mkmacro::MkAction::Delay(crate::mkmacro::MkDelayPayload {
                    mode: crate::mkmacro::MkDelayMode::Fixed,
                    fixed_ms: 0,
                    minimum_ms: 0,
                    maximum_ms: 0,
                }),
            });
        app.mkmacro_dialog.selection.replace([step_id]);
        let table_id = app
            .mkmacro_dialog
            .qa_observation(&ctx)
            .table_focus_id
            .unwrap();
        ctx.memory_mut(|memory| memory.request_focus(table_id));

        let table_focused = qa_dialog_state(
            &app,
            &ctx,
            super::super::jump_mode::ui::NavigationOwner::MkMacro { instance: 11 },
            &[],
        )
        .unwrap();
        assert_eq!(table_focused.mkmacro_selected_id, Some(macro_id));
        assert_eq!(table_focused.mkmacro_selected_step_ids, Some(vec![step_id]));
        assert_eq!(
            table_focused.mkmacro_table_focus_id,
            Some(format!("{table_id:?}"))
        );
        assert_eq!(table_focused.mkmacro_table_focused, Some(true));

        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("other-native-widget")));
        let other_widget_focused = qa_dialog_state(
            &app,
            &ctx,
            super::super::jump_mode::ui::NavigationOwner::MkMacro { instance: 11 },
            &[],
        )
        .unwrap();
        assert_eq!(
            other_widget_focused.mkmacro_selected_step_ids,
            Some(vec![step_id])
        );
        assert_eq!(other_widget_focused.mkmacro_table_focused, Some(false));
    }

    fn eligible_surface() -> SurfaceEligibility {
        SurfaceEligibility {
            logical_root_visible: true,
            root_visibility_reported: Some(true),
            root_visible: true,
            ..Default::default()
        }
    }

    fn test_evidence(logic_frame_id: u64, ui_pass_id: u64, ui_pass_index: u32) -> FrameEvidence {
        FrameEvidence {
            viewport_id: "ROOT",
            logic_frame_id,
            ui_pass_id,
            ui_pass_index,
            evidence_stage: "test",
            completed_at_unix_ms: 1,
            query: "test query".into(),
            result_generation: 1,
            pixels_per_point: Some(1.0),
            native_pixels_per_point: Some(1.0),
            zoom_factor: 1.0,
            logical_root_visible: true,
            root_visibility_reported: Some(true),
            root_visible: true,
            viewport_focused: true,
            ocr_owner: false,
            tool_owner: false,
            modal_or_key_capture_owner: false,
            eligible: true,
            jump_mode_observation: None,
            qr_observation: None,
        }
    }

    fn test_image(color: egui::Color32) -> Arc<ColorImage> {
        Arc::new(ColorImage::new([1, 1], vec![color]))
    }

    fn test_probe(
        trigger: CaptureTrigger,
    ) -> (
        FramebufferProbe,
        Receiver<WriterJob>,
        SyncSender<WriterOutcome>,
    ) {
        test_probe_for_target(trigger, CaptureTarget::Root)
    }

    fn test_probe_for_target(
        trigger: CaptureTrigger,
        target: CaptureTarget,
    ) -> (
        FramebufferProbe,
        Receiver<WriterJob>,
        SyncSender<WriterOutcome>,
    ) {
        test_probe_with_capture_count(trigger, target, 1)
    }

    fn test_probe_with_capture_count(
        trigger: CaptureTrigger,
        target: CaptureTarget,
        capture_count: u8,
    ) -> (
        FramebufferProbe,
        Receiver<WriterJob>,
        SyncSender<WriterOutcome>,
    ) {
        let mut request = test_request_for_target(target, trigger, PathBuf::from("unused"));
        request.manifest.capture_count = capture_count;
        let token = UserData::new(ScreenshotToken(1));
        let (writer_tx, writer_rx) = mpsc::sync_channel(WRITER_QUEUE_CAPACITY);
        let (outcome_tx, outcome_rx) = mpsc::sync_channel(1);
        let mut probe = FramebufferProbe {
            output_dir: PathBuf::from("unused"),
            token,
            target,
            capture_count,
            diagnostics_enabled: false,
            jump_mode_observation_enabled: false,
            wait_for_active_hints: false,
            phase2_request: None,
            activation_count: 0,
            last_action: None,
            tab_processed_count: 0,
            last_tab_processed_logic_frame_id: None,
            enter_processed_count: 0,
            last_enter_processed_logic_frame_id: None,
            latest_jump_mode_observation: None,
            geometry_rebuild_count: 0,
            latest_root_list_diagnostic: None,
            phase: ProbePhase::Armed,
            logic_frame_id: 0,
            ui_pass_id: 0,
            current_ui_pass_id: None,
            current_ui_pass_index: 0,
            retained_pass: None,
            issue_logic_frame_id: None,
            capture_logic_frame_id: None,
            expected_reply_logic_frame_id: None,
            reply_wait_armed: false,
            latest_qr_observation: None,
            qr_acquired: false,
            qr_capture_parent_logic_frame_id: None,
            pending_reply: None,
            receipt: CompletionReceipt::new(&request, selected_reply_timeout(target)),
            writer_tx,
            writer_rx: outcome_rx,
        };
        if target == CaptureTarget::Qr {
            probe.arm_reply_wait().unwrap();
        }
        (probe, writer_rx, outcome_tx)
    }
    fn issue_frame_one(probe: &mut FramebufferProbe, ctx: &egui::Context) {
        let eligibility = eligible_surface();
        probe.logic_frame_id = 1;
        probe.phase = ProbePhase::Ready;
        ctx.begin_pass(egui::RawInput::default());
        probe.start_ui_pass(0);
        let issue_evidence = test_evidence(1, probe.current_ui_pass_id.unwrap(), 0);
        probe.issue_screenshot_if_ready(ctx, eligibility, true, issue_evidence.clone());
        probe.finish_ui_pass(eligibility, Some(issue_evidence));
        probe.finish_retained_ui_frame(ctx, eligibility, None);
        ctx.end_pass().drop_without_applying_deltas();
        assert_eq!(probe.phase, ProbePhase::Issued);
    }

    fn capture_frame_two(probe: &mut FramebufferProbe, ctx: &egui::Context) {
        let eligibility = eligible_surface();
        ctx.begin_pass(egui::RawInput::default());
        probe.begin_root_logic(ctx, Some(eligibility));
        assert_eq!(probe.logic_frame_id, 2);

        // An earlier pass can be discarded. The probe owns a marker for each
        // pass and uses the final retained pass as its capture evidence.
        probe.start_ui_pass(0);
        let mut discarded = eligibility;
        discarded.ocr_owner = true;
        probe.finish_ui_pass(
            discarded,
            Some(test_evidence(2, probe.current_ui_pass_id.unwrap(), 0)),
        );

        probe.start_ui_pass(1);
        let retained_evidence = test_evidence(2, probe.current_ui_pass_id.unwrap(), 1);
        probe.finish_ui_pass(eligibility, Some(retained_evidence));
        probe.finish_retained_ui_frame(ctx, eligibility, None);
        ctx.end_pass().drop_without_applying_deltas();
        assert_eq!(probe.phase, ProbePhase::AwaitingReply);
        assert_eq!(probe.capture_logic_frame_id, Some(2));
        assert_eq!(probe.expected_reply_logic_frame_id, Some(3));
    }

    fn prepare_save_attempt(probe: &mut FramebufferProbe) {
        let eligibility = eligible_surface();
        probe.phase = ProbePhase::AwaitingReply;
        probe.logic_frame_id = 3;
        probe.expected_reply_logic_frame_id = Some(3);
        probe.receipt.captured = Some(test_evidence(2, 2, 0));
        probe.pending_reply = Some(PendingScreenshotReply {
            image: test_image(egui::Color32::WHITE),
            evidence: ScreenshotReplyEvidence {
                viewport_id: "ROOT",
                logic_frame_id: 3,
                received_at_unix_ms: 3,
                width: 1,
                height: 1,
                qr_observation: None,
            },
        });
        probe.retained_pass = Some(RetainedPassMarker {
            logic_frame_id: 3,
            ui_pass_id: 3,
            ui_pass_index: 0,
            eligibility,
            evidence: Some(test_evidence(3, 3, 0)),
            root_list_diagnostic: None,
        });
    }

    fn deliver_reply_frame_three(
        probe: &mut FramebufferProbe,
        ctx: &egui::Context,
        final_pass_eligibility: SurfaceEligibility,
    ) {
        let reply = test_image(egui::Color32::WHITE);
        let raw_input = egui::RawInput {
            events: vec![Event::Screenshot {
                viewport_id: ViewportId::ROOT,
                user_data: probe.token.clone(),
                image: Arc::clone(&reply),
            }],
            ..Default::default()
        };
        ctx.begin_pass(raw_input);
        probe.begin_root_logic(ctx, Some(eligible_surface()));
        assert_eq!(probe.logic_frame_id, 3);
        assert_eq!(probe.phase, ProbePhase::AwaitingReply);
        assert!(probe.pending_reply.is_some());

        probe.start_ui_pass(0);
        probe.finish_ui_pass(
            final_pass_eligibility,
            Some(test_evidence(3, probe.current_ui_pass_id.unwrap(), 0)),
        );
        probe.finish_retained_ui_frame(ctx, eligible_surface(), None);
        let output = ctx.end_pass();
        output.drop_without_applying_deltas();
    }

    fn test_qr_observation(completed_frame_nr: u64) -> QrViewportObservation {
        QrViewportObservation {
            child_viewport_id: super::super::qr_dialog::viewport_id(),
            callback_class: egui::ViewportClass::Immediate,
            callback_frame_nr: completed_frame_nr,
            callback_pass_nr: completed_frame_nr,
            callback_pass_index: 0,
            visible: None,
            minimized: None,
            occluded: None,
            focused: Some(true),
            body_rendered: true,
            still_open: true,
            callback_observed_at_unix_ms: 1,
            completed_frame_nr: Some(completed_frame_nr),
            completed_pass_nr: Some(completed_frame_nr),
            renderer_returned_at_unix_ms: Some(2),
        }
    }

    fn start_qr_probe(probe: &mut FramebufferProbe, ctx: &egui::Context) {
        let eligibility = eligible_surface();
        ctx.begin_pass(egui::RawInput::default());
        probe.begin_root_logic(ctx, Some(eligibility));
        assert_eq!(probe.logic_frame_id, 1);
        probe.start_ui_pass(0);
        probe.finish_ui_pass(eligibility, None);
        probe.finish_retained_ui_frame(ctx, eligibility, None);
        ctx.end_pass().drop_without_applying_deltas();
        assert_eq!(probe.phase, ProbePhase::Ready);
    }

    fn issue_qr_screenshot(probe: &mut FramebufferProbe, ctx: &egui::Context) {
        let eligibility = eligible_surface();
        ctx.begin_pass(egui::RawInput::default());
        probe.begin_root_logic(ctx, Some(eligibility));
        assert_eq!(probe.logic_frame_id, 2);
        probe.start_ui_pass(0);
        let first_pass = probe.current_ui_pass_id.unwrap();
        probe.after_qr_show(
            ctx,
            eligibility,
            true,
            Some(test_qr_observation(20)),
            Some(test_evidence(2, first_pass, 0)),
        );
        assert_eq!(probe.phase, ProbePhase::Issued);
        let mut discarded = eligibility;
        discarded.modal_or_key_capture_owner = true;
        probe.finish_ui_pass(discarded, Some(test_evidence(2, first_pass, 0)));

        probe.start_ui_pass(1);
        let retained_pass = probe.current_ui_pass_id.unwrap();
        probe.after_qr_show(
            ctx,
            eligibility,
            true,
            Some(test_qr_observation(21)),
            Some(test_evidence(2, retained_pass, 1)),
        );
        probe.finish_ui_pass(eligibility, Some(test_evidence(2, retained_pass, 1)));
        probe.finish_retained_ui_frame(ctx, eligibility, None);
        ctx.end_pass().drop_without_applying_deltas();

        assert_eq!(probe.phase, ProbePhase::Issued);
        let baseline = probe.receipt.dispatch_boundary_baseline.as_ref().unwrap();
        assert_eq!(baseline.parent_logic_frame_id, 2);
        assert_eq!(baseline.completed_frame_nr, Some(21));
    }

    fn capture_and_reply_in_one_qr_parent_frame(
        probe: &mut FramebufferProbe,
        ctx: &egui::Context,
        final_eligibility: SurfaceEligibility,
    ) {
        let eligibility = eligible_surface();
        let viewport_id = CaptureTarget::Qr.viewport_id();
        ctx.begin_pass(egui::RawInput::default());
        probe.begin_root_logic(ctx, Some(eligibility));
        assert_eq!(probe.logic_frame_id, 3);
        probe.start_ui_pass(0);
        let ui_pass = probe.current_ui_pass_id.unwrap();
        probe.after_qr_show(
            ctx,
            eligibility,
            true,
            Some(test_qr_observation(22)),
            Some(test_evidence(3, ui_pass, 0)),
        );
        assert_eq!(probe.phase, ProbePhase::AwaitingReply);
        assert_eq!(probe.qr_capture_parent_logic_frame_id, Some(3));

        let image = test_image(egui::Color32::WHITE);
        ctx.input_mut_for(viewport_id, |input| {
            input.events.push(Event::Copy);
            input.events.push(Event::Screenshot {
                viewport_id,
                user_data: probe.token.clone(),
                image: Arc::clone(&image),
            });
        });
        probe.after_qr_show(
            ctx,
            eligibility,
            true,
            Some(test_qr_observation(23)),
            Some(test_evidence(3, ui_pass, 0)),
        );
        assert!(probe.pending_reply.is_some());
        assert!(ctx.input_for(viewport_id, |input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, Event::Copy))
        }));

        probe.finish_ui_pass(final_eligibility, None);
        probe.finish_retained_ui_frame(ctx, final_eligibility, None);
        ctx.end_pass().drop_without_applying_deltas();
    }

    #[test]
    fn qr_dispatch_uses_retained_issue_baseline_and_accepts_child_reply_in_capture_parent_frame() {
        let (mut probe, writer_rx, _outcome_tx) =
            test_probe_for_target(CaptureTrigger::Startup, CaptureTarget::Qr);
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::AwaitReply(_))));
        assert!(matches!(writer_rx.try_recv(), Err(TryRecvError::Empty)));

        let ctx = egui::Context::default();
        start_qr_probe(&mut probe, &ctx);
        issue_qr_screenshot(&mut probe, &ctx);
        assert!(matches!(writer_rx.try_recv(), Err(TryRecvError::Empty)));

        capture_and_reply_in_one_qr_parent_frame(&mut probe, &ctx, eligible_surface());
        assert_eq!(probe.phase, ProbePhase::Saving);
        let captured = probe.receipt.captured.as_ref().unwrap();
        assert_eq!(captured.logic_frame_id, 3);
        assert_eq!(
            captured
                .qr_observation
                .as_ref()
                .and_then(|observation| observation.completed_frame_nr),
            Some(22)
        );
        assert_eq!(
            probe
                .receipt
                .screenshot_received
                .as_ref()
                .unwrap()
                .qr_observation
                .as_ref()
                .and_then(|observation| observation.completed_frame_nr),
            Some(23)
        );
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::Save { .. })));
    }

    #[test]
    fn qr_buffered_reply_is_discarded_when_root_loses_eligibility_before_save() {
        let (mut probe, writer_rx, _outcome_tx) =
            test_probe_for_target(CaptureTrigger::Startup, CaptureTarget::Qr);
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::AwaitReply(_))));

        let ctx = egui::Context::default();
        start_qr_probe(&mut probe, &ctx);
        issue_qr_screenshot(&mut probe, &ctx);
        let mut ineligible = eligible_surface();
        ineligible.tool_owner = true;
        capture_and_reply_in_one_qr_parent_frame(&mut probe, &ctx, ineligible);

        assert_eq!(probe.phase, ProbePhase::Failed);
        assert!(probe.pending_reply.is_some());
        assert!(probe.receipt.saved.is_none());
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::Failure(_))));
    }

    #[test]
    fn qr_observation_requires_actual_child_and_treats_unknown_visibility_as_visible() {
        let observation = test_qr_observation(1);
        let evidence = qr_render_evidence(7, &observation);
        assert!(evidence.eligible);
        assert_eq!(evidence.callback_class, "immediate");

        let mut embedded = test_qr_observation(1);
        embedded.callback_class = egui::ViewportClass::EmbeddedWindow;
        assert!(!qr_render_evidence(7, &embedded).eligible);

        let mut wrong_child = test_qr_observation(1);
        wrong_child.child_viewport_id = ViewportId::from_hash_of("not QR");
        assert!(!qr_render_evidence(7, &wrong_child).eligible);

        let mut hidden = test_qr_observation(1);
        hidden.visible = Some(false);
        assert!(!qr_render_evidence(7, &hidden).eligible);

        let mut minimized = test_qr_observation(1);
        minimized.minimized = Some(true);
        assert!(!qr_render_evidence(7, &minimized).eligible);
    }

    #[test]
    fn qr_manifest_rejects_the_root_only_f11_trigger() {
        let manifest = RequestManifest {
            run_id: "test-run".into(),
            request_id: "test-request".into(),
            target: CaptureTarget::Qr,
            trigger: CaptureTrigger::F11,
            capture_count: 1,
            diagnostics: false,
            jump_mode_observation: None,
            output_dir: PathBuf::from("output"),
            fixture_manifest_sha256: "a".repeat(64),
            build_receipt_identity: "test-build".into(),
        };

        assert_eq!(
            validate_manifest(&manifest).unwrap_err(),
            "QR framebuffer capture supports the Startup trigger only"
        );
    }

    #[test]
    fn legacy_manifest_defaults_to_one_capture_and_diagnostics_off() {
        let manifest: RequestManifest = serde_json::from_value(serde_json::json!({
            "run_id": "legacy-run",
            "request_id": "legacy-request",
            "target": "ROOT",
            "trigger": "f11",
            "output_dir": "output",
            "fixture_manifest_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "build_receipt_identity": "test-build"
        }))
        .unwrap();

        assert_eq!(manifest.capture_count, 1);
        assert!(!manifest.diagnostics);
        assert!(manifest.jump_mode_observation.is_none());
        validate_manifest(&manifest).unwrap();
        assert_eq!(
            capture_artifact_names(1, 1),
            ("framebuffer.png".into(), "receipt.json".into())
        );
    }

    #[test]
    fn jump_mode_observation_is_root_only_and_opt_in() {
        let make_manifest = |target| RequestManifest {
            run_id: "query-probe".into(),
            request_id: "query-only".into(),
            target,
            trigger: CaptureTrigger::Startup,
            capture_count: 1,
            diagnostics: false,
            jump_mode_observation: Some(JumpModeObservationRequest {
                wait_for_active_hints: true,
                phase2: None,
            }),
            output_dir: PathBuf::from("capture"),
            fixture_manifest_sha256: "a".repeat(64),
            build_receipt_identity: "test-build".into(),
        };
        let root = make_manifest(CaptureTarget::Root);
        validate_manifest(&root).unwrap();

        let child = make_manifest(CaptureTarget::Qr);
        assert_eq!(
            validate_manifest(&child).unwrap_err(),
            "Jump Mode observation requires the ROOT capture target"
        );
    }

    #[test]
    fn armed_observer_errors_are_reported_without_changing_legacy_armed_failure_behavior() {
        let (mut legacy, _, _) = test_probe(CaptureTrigger::Startup);
        legacy.fail("legacy startup wait remains nonterminal");
        assert_eq!(legacy.phase, ProbePhase::Armed);

        let (mut observed, writer_rx, _) = test_probe(CaptureTrigger::Startup);
        observed.jump_mode_observation_enabled = true;
        observed.fail_jump_mode_observation("observation output failed");
        assert_eq!(observed.phase, ProbePhase::Failed);
        assert_eq!(observed.receipt.status, ReceiptStatus::Failed);
        assert_eq!(
            observed.receipt.failure.as_deref(),
            Some("observation output failed")
        );
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::Failure(_))));
    }

    #[test]
    fn activation_observation_counts_only_when_jump_probe_is_opted_in() {
        let action = crate::actions::Action {
            label: "Safe test action".into(),
            desc: String::new(),
            action: "test:noop".into(),
            args: Some("safe-argument".into()),
        };
        let (mut probe, _, _) = test_probe(CaptureTrigger::Startup);
        probe.record_activation(
            &action,
            Some("safe query"),
            crate::commands::ActivationSource::Enter,
        );
        assert_eq!(probe.activation_count, 0);
        assert!(probe.last_action.is_none());

        probe.jump_mode_observation_enabled = true;
        probe.record_activation(
            &action,
            Some("safe query"),
            crate::commands::ActivationSource::Enter,
        );
        assert_eq!(probe.activation_count, 1);
        let last_action = probe.last_action.as_ref().unwrap();
        assert_eq!(last_action.action_id, "test:noop");
        assert_eq!(last_action.query_override.as_deref(), Some("safe query"));
        assert_eq!(last_action.args.as_deref(), Some("safe-argument"));
        assert_eq!(last_action.source, "enter");
    }

    #[test]
    fn jump_mode_observation_tracks_processed_enter_frame_when_opted_in() {
        let (mut probe, _, _) = test_probe(CaptureTrigger::Startup);
        probe.logic_frame_id = 7;
        probe.record_enter_processed();
        assert_eq!(probe.enter_processed_count, 0);
        assert_eq!(probe.last_enter_processed_logic_frame_id, None);

        probe.jump_mode_observation_enabled = true;
        probe.record_enter_processed();
        assert_eq!(probe.enter_processed_count, 1);
        assert_eq!(probe.last_enter_processed_logic_frame_id, Some(7));

        probe.logic_frame_id = 8;
        probe.record_enter_processed();
        assert_eq!(probe.enter_processed_count, 2);
        assert_eq!(probe.last_enter_processed_logic_frame_id, Some(8));
    }

    #[test]
    fn jump_mode_observation_tracks_processed_tab_frame_when_opted_in() {
        let (mut probe, _, _) = test_probe(CaptureTrigger::Startup);
        probe.logic_frame_id = 7;
        probe.record_tab_processed();
        assert_eq!(probe.tab_processed_count, 0);
        assert_eq!(probe.last_tab_processed_logic_frame_id, None);

        probe.jump_mode_observation_enabled = true;
        probe.record_tab_processed();
        assert_eq!(probe.tab_processed_count, 1);
        assert_eq!(probe.last_tab_processed_logic_frame_id, Some(7));

        probe.logic_frame_id = 8;
        probe.record_tab_processed();
        assert_eq!(probe.tab_processed_count, 2);
        assert_eq!(probe.last_tab_processed_logic_frame_id, Some(8));
    }

    #[test]
    fn jump_mode_observation_resolves_selected_root_payload_from_typed_identity() {
        use super::super::jump_mode::{
            SemanticTargetId, SurfaceId, TargetGeneration, TargetId, TargetKind, TargetMetadata,
        };

        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().unwrap();
        let mut app = test_launcher_app(&ctx, directory.path());
        app.jump_mode_ui.set_enabled(true);
        app.query = "note list".into();
        app.results = vec![crate::actions::Action {
            label: "Jump QA Shared Note".into(),
            desc: "Open isolated fixture note".into(),
            action: "note:open:jump-qa-note-001".into(),
            args: Some("note-argument".into()),
        }];
        app.selected = Some(0);
        app.resolved_grid_layout = true;

        app.jump_mode_ui.begin_frame();
        app.jump_mode_ui.begin_ui_pass();
        let result_generation = app.root_list_geometry.result_generation();
        let generation = TargetGeneration(result_generation);
        let scope = app
            .jump_mode_ui
            .scope(
                SurfaceId::LauncherResults,
                generation,
                "launcher-results",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        let dashboard_scope = scope.with_surface(SurfaceId::Dashboard, generation);
        let target_id = TargetId::new(
            SurfaceId::LauncherResults,
            SemanticTargetId::RootResult(0),
            generation,
        );
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 560.0),
                )),
                focused: true,
                ..Default::default()
            },
            |ui| {
                let response = ui.button("Jump QA Shared Note");
                response.request_focus();
                assert!(matches!(
                    scope.register_metadata(
                        ui,
                        &response,
                        TargetMetadata::new(
                            target_id.clone(),
                            "Jump QA Shared Note",
                            TargetKind::Action,
                            0,
                        ),
                    ),
                    super::super::jump_mode::ui::RegistrationResult::Registered { .. }
                ));
                let dashboard_response = ui.button("Dashboard fixture target");
                let dashboard_target_id = TargetId::new(
                    SurfaceId::Dashboard,
                    SemanticTargetId::named("fixture-dashboard-target"),
                    generation,
                );
                assert!(matches!(
                    dashboard_scope.register_metadata(
                        ui,
                        &dashboard_response,
                        TargetMetadata::new(
                            dashboard_target_id,
                            "Dashboard fixture target",
                            TargetKind::Button,
                            1,
                        ),
                    ),
                    super::super::jump_mode::ui::RegistrationResult::Registered { .. }
                ));
            },
        )
        .drop_without_applying_deltas();
        app.jump_mode_ui.finish_ui_pass();

        let observation = build_jump_mode_observation(
            &app,
            &ctx,
            7,
            3,
            0,
            true,
            app.jump_mode_ui.pass_registry(),
            0,
            None,
            1234,
            "a".repeat(64),
            1,
            14,
            true,
        );

        assert_eq!(observation.result_generation, result_generation);
        assert_eq!(observation.result_count, 1);
        assert!(observation.grid_layout);
        assert_eq!(observation.selected_index, Some(0));
        let target_id_debug = format!("{target_id:?}");
        assert_eq!(
            observation.selected_target_id.as_deref(),
            Some(target_id_debug.as_str())
        );
        let root_target = observation
            .targets
            .iter()
            .find(|target| target.root_result_index == Some(0))
            .unwrap();
        assert_eq!(root_target.target_id, target_id_debug);
        assert_eq!(root_target.surface, "launcher_results");
        let root_action = root_target.root_action.as_ref().unwrap();
        assert_eq!(root_action.action, "note:open:jump-qa-note-001");
        assert_eq!(root_action.args.as_deref(), Some("note-argument"));
        assert_eq!(
            observation.selected_action.as_ref().unwrap().action,
            root_action.action
        );
        assert_eq!(observation.opened_note_slugs, Vec::<String>::new());
        assert_eq!(observation.capture_index, 1);
        assert_eq!(observation.capture_count, 14);
        assert!(observation.capture_armed);

        // Surface names come from the typed owner enum, not the opaque debug
        // representation of the target ID consumed by the native runner.
        let dashboard_target = observation
            .targets
            .iter()
            .find(|target| target.label == "Dashboard fixture target")
            .unwrap();
        assert_eq!(dashboard_target.surface, "dashboard");
        assert_eq!(dashboard_target.root_result_index, None);
        assert!(dashboard_target.root_action.is_none());
    }

    #[test]
    fn jump_mode_observer_recognizes_the_real_registered_query_text_entry_and_hint() {
        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().unwrap();
        let mut app = test_launcher_app(&ctx, directory.path());
        app.jump_mode_ui.set_enabled(true);

        let render_query_targets = |app: &mut LauncherApp, raw_input: egui::RawInput| {
            ctx.run_ui(raw_input, |ui| {
                app.jump_mode_ui.begin_frame();
                app.jump_mode_ui.handle_root_input(&ctx, true, true);
                app.jump_mode_ui.begin_ui_pass();
                let scope = app
                    .jump_mode_ui
                    .scope(
                        super::super::jump_mode::SurfaceId::LauncherResults,
                        super::super::jump_mode::TargetGeneration(1),
                        "launcher-results",
                        egui::ViewportId::ROOT,
                        true,
                    )
                    .unwrap();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app.query).id(egui::Id::new("query_input")),
                );
                response.request_focus();
                assert!(matches!(
                    scope.register(
                        ui,
                        &response,
                        "query_input",
                        "Search query",
                        super::super::jump_mode::TargetKind::TextEntry,
                        0,
                    ),
                    super::super::jump_mode::ui::RegistrationResult::Registered { .. }
                ));
                let response = ui.button("Safe target");
                assert!(matches!(
                    scope.register(
                        ui,
                        &response,
                        "safe-button",
                        "Safe target",
                        super::super::jump_mode::TargetKind::Button,
                        1,
                    ),
                    super::super::jump_mode::ui::RegistrationResult::Registered { .. }
                ));
            })
            .drop_without_applying_deltas();
            app.jump_mode_ui.finish_ui_pass();
            assert!(app.jump_mode_ui.prepare_pass_for_paint(&ctx, true, true));
        };
        let input = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 560.0),
            )),
            focused: true,
            events,
            ..Default::default()
        };

        render_query_targets(&mut app, input(Vec::new()));
        app.jump_mode_ui.complete_frame(&ctx, true, true);

        render_query_targets(
            &mut app,
            input(vec![
                Event::Key {
                    key: Key::Space,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::SHIFT,
                },
                Event::Key {
                    key: Key::Space,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: Modifiers::SHIFT,
                },
            ]),
        );
        assert!(app.jump_mode_ui.active_session().is_some());
        ctx.memory_mut(|memory| memory.surrender_focus(egui::Id::new("query_input")));
        let observation = build_jump_mode_observation(
            &app,
            &ctx,
            2,
            2,
            0,
            true,
            app.jump_mode_ui.pass_registry(),
            0,
            None,
            1234,
            "a".repeat(64),
            1,
            1,
            true,
        );

        assert!(observation.root_focused);
        assert!(observation.root_owner_allowed);
        assert!(observation.active_session);
        assert!(!observation.query_focused);
        assert!(observation.query_target_id.is_some());
        assert!(active_query_hint_snapshot(&observation));
        let query_target = observation
            .targets
            .iter()
            .find(|target| target.is_query_input)
            .unwrap();
        assert_eq!(query_target.kind, "text_entry");
        assert_ne!(query_target.target_id, "query_input");
        assert!(
            query_target
                .hint
                .as_deref()
                .is_some_and(|hint| !hint.is_empty())
        );
        assert_eq!(query_target.focused, observation.query_focused);

        app.jump_mode_ui.complete_frame(&ctx, true, true);
    }

    #[test]
    fn repeated_capture_manifest_is_bounded_to_root_f11() {
        let manifest = |target, trigger, capture_count| RequestManifest {
            run_id: "multi-run".into(),
            request_id: "multi-request".into(),
            target,
            trigger,
            capture_count,
            diagnostics: false,
            jump_mode_observation: None,
            output_dir: PathBuf::from("output"),
            fixture_manifest_sha256: "a".repeat(64),
            build_receipt_identity: "test-build".into(),
        };

        validate_manifest(&manifest(CaptureTarget::Root, CaptureTrigger::F11, 16)).unwrap();
        assert_eq!(
            validate_manifest(&manifest(CaptureTarget::Root, CaptureTrigger::F11, 17)).unwrap_err(),
            "manifest capture_count must be between 1 and 16"
        );
        assert_eq!(
            validate_manifest(&manifest(CaptureTarget::Root, CaptureTrigger::Startup, 2))
                .unwrap_err(),
            "multiple captures require the ROOT/F11 trigger"
        );
        assert_eq!(
            capture_artifact_names(3, 8),
            ("framebuffer-003.png".into(), "receipt-003.json".into())
        );
    }

    #[test]
    fn f11_pressed_while_saving_is_not_reused_for_the_next_capture() {
        let (mut probe, _writer_rx, outcome_tx) =
            test_probe_with_capture_count(CaptureTrigger::F11, CaptureTarget::Root, 2);
        probe.phase = ProbePhase::Saving;
        let prior_token = probe.token.clone();
        outcome_tx
            .send(WriterOutcome {
                status: ReceiptStatus::Saved,
                error: None,
                receipt_written: true,
                capture_index: 1,
                capture_count: 2,
            })
            .unwrap();

        let ctx = egui::Context::default();
        let f11_input = |pressed| {
            let mut input = egui::RawInput {
                events: vec![Event::Key {
                    key: Key::F11,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            };
            input.viewports.get_mut(&ViewportId::ROOT).unwrap().focused = Some(true);
            input
        };

        ctx.begin_pass(f11_input(true));
        probe.begin_root_logic(&ctx, Some(eligible_surface()));
        assert_eq!(probe.phase, ProbePhase::Armed);
        assert_eq!(probe.receipt.capture_index, 2);
        assert_ne!(probe.token, prior_token);
        assert!(ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, Event::Key { key: Key::F11, .. }))
        }));
        ctx.end_pass().drop_without_applying_deltas();

        ctx.begin_pass(f11_input(false));
        probe.begin_root_logic(&ctx, Some(eligible_surface()));
        assert_eq!(probe.phase, ProbePhase::Armed);
        ctx.end_pass().drop_without_applying_deltas();

        ctx.begin_pass(f11_input(true));
        probe.begin_root_logic(&ctx, Some(eligible_surface()));
        assert_eq!(probe.phase, ProbePhase::Ready);
        assert!(!ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, Event::Key { key: Key::F11, .. }))
        }));
        assert_eq!(
            probe.logic_frame_id, 3,
            "frame ids remain monotonic across captures"
        );
        ctx.end_pass().drop_without_applying_deltas();
    }

    #[test]
    fn qr_startup_wait_times_out_when_the_dialog_never_opens() {
        let output_dir = tempfile::tempdir().unwrap();
        let request = QaFramebufferRequest {
            manifest: RequestManifest {
                run_id: "unopened-qr-run".into(),
                request_id: "unopened-qr-request".into(),
                target: CaptureTarget::Qr,
                trigger: CaptureTrigger::Startup,
                capture_count: 1,
                diagnostics: false,
                jump_mode_observation: None,
                output_dir: output_dir.path().to_path_buf(),
                fixture_manifest_sha256: "a".repeat(64),
                build_receipt_identity: "test-build".into(),
            },
            manifest_sha256: "b".repeat(64),
            output_dir: output_dir.path().to_path_buf(),
            executable: ExecutableIdentity {
                path: PathBuf::from("test.exe"),
                sha256: "c".repeat(64),
            },
            process: ProcessIdentity {
                pid: 1,
                probe_initialized_at_unix_ms: 1,
            },
        };
        let probe = FramebufferProbe::new_with_reply_timeout(
            request,
            egui::Context::default(),
            Duration::from_millis(20),
        )
        .unwrap();
        assert_eq!(probe.phase, ProbePhase::Armed);

        let outcome = probe
            .writer_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert_eq!(outcome.status, ReceiptStatus::Failed);
        assert!(outcome.receipt_written);
        assert!(outcome.error.as_deref().is_some_and(|error| {
            error.starts_with("selected QR capture did not complete within")
        }));
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(output_dir.path().join(RECEIPT_FILE_NAME)).unwrap())
                .unwrap();
        assert_eq!(receipt["target"], "QR");
        assert_eq!(receipt["trigger"], "startup");
        assert_eq!(receipt["reply_wait_timeout_ms"], 20);
        assert!(!output_dir.path().join(PNG_FILE_NAME).exists());
    }

    #[test]
    fn selected_target_deadlines_are_recorded_in_the_receipt() {
        let root_request = test_request_for_target(
            CaptureTarget::Root,
            CaptureTrigger::Startup,
            PathBuf::from("unused"),
        );
        let qr_request = test_request_for_target(
            CaptureTarget::Qr,
            CaptureTrigger::Startup,
            PathBuf::from("unused"),
        );
        let root_receipt =
            CompletionReceipt::new(&root_request, selected_reply_timeout(CaptureTarget::Root));
        let qr_receipt =
            CompletionReceipt::new(&qr_request, selected_reply_timeout(CaptureTarget::Qr));

        assert_eq!(root_receipt.reply_wait_timeout_ms, 30_000);
        assert_eq!(qr_receipt.reply_wait_timeout_ms, 120_000);
        assert_eq!(
            serde_json::to_value(qr_receipt).unwrap()["reply_wait_timeout_ms"],
            120_000
        );
    }

    #[test]
    fn qr_armed_wrappers_cancel_hidden_and_modal_takeover_but_keep_root_f11_armed() {
        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let data_dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = test_launcher_app(&ctx, data_dir.path());

        let (qr_hidden_probe, qr_hidden_jobs, _qr_hidden_outcome) =
            test_probe_for_target(CaptureTrigger::Startup, CaptureTarget::Qr);
        app.qa_framebuffer = Some(qr_hidden_probe);
        app.qa_framebuffer_run_hidden_logic_tick();
        let qr_hidden_probe = app.qa_framebuffer.as_ref().unwrap();
        assert_eq!(qr_hidden_probe.phase, ProbePhase::Failed);
        assert!(
            qr_hidden_probe
                .receipt
                .failure
                .as_deref()
                .is_some_and(|failure| failure.contains("ROOT became hidden"))
        );
        assert!(matches!(
            qr_hidden_jobs.try_recv(),
            Ok(WriterJob::AwaitReply(_))
        ));
        assert!(matches!(
            qr_hidden_jobs.try_recv(),
            Ok(WriterJob::Failure(_))
        ));

        let (root_hidden_probe, root_hidden_jobs, _root_hidden_outcome) =
            test_probe_for_target(CaptureTrigger::F11, CaptureTarget::Root);
        app.qa_framebuffer = Some(root_hidden_probe);
        app.qa_framebuffer_run_hidden_logic_tick();
        assert_eq!(
            app.qa_framebuffer.as_ref().unwrap().phase,
            ProbePhase::Armed,
            "ROOT F11 capture remains armed until its pretrigger arrives"
        );
        assert!(matches!(
            root_hidden_jobs.try_recv(),
            Err(TryRecvError::Empty)
        ));

        app.confirm_modal
            .open_custom("QA capture wrapper test", "Modal owns launcher input");
        let (qr_modal_probe, qr_modal_jobs, _qr_modal_outcome) =
            test_probe_for_target(CaptureTrigger::Startup, CaptureTarget::Qr);
        app.qa_framebuffer = Some(qr_modal_probe);
        ctx.begin_pass(egui::RawInput::default());
        app.qa_framebuffer_revalidate_root_logic(&ctx, true);
        ctx.end_pass().drop_without_applying_deltas();
        let qr_modal_probe = app.qa_framebuffer.as_ref().unwrap();
        assert_eq!(qr_modal_probe.phase, ProbePhase::Failed);
        assert_eq!(
            qr_modal_probe.receipt.failure.as_deref(),
            Some("a modal or key-capture owner blocked the retained capture boundary")
        );
        assert!(matches!(
            qr_modal_jobs.try_recv(),
            Ok(WriterJob::AwaitReply(_))
        ));
        assert!(matches!(
            qr_modal_jobs.try_recv(),
            Ok(WriterJob::Failure(_))
        ));

        let (root_modal_probe, root_modal_jobs, _root_modal_outcome) =
            test_probe_for_target(CaptureTrigger::F11, CaptureTarget::Root);
        app.qa_framebuffer = Some(root_modal_probe);
        ctx.begin_pass(egui::RawInput::default());
        app.qa_framebuffer_revalidate_root_logic(&ctx, true);
        ctx.end_pass().drop_without_applying_deltas();
        assert_eq!(
            app.qa_framebuffer.as_ref().unwrap().phase,
            ProbePhase::Armed,
            "ROOT F11 capture remains armed while a modal owns input"
        );
        assert!(matches!(
            root_modal_jobs.try_recv(),
            Err(TryRecvError::Empty)
        ));
    }

    #[test]
    fn capture_schedule_uses_issue_capture_and_reply_frames() {
        let (mut probe, writer_rx, _outcome_tx) = test_probe(CaptureTrigger::Startup);
        let ctx = egui::Context::default();

        issue_frame_one(&mut probe, &ctx);
        assert!(matches!(writer_rx.try_recv(), Err(TryRecvError::Empty)));

        capture_frame_two(&mut probe, &ctx);
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::AwaitReply(_))));

        deliver_reply_frame_three(&mut probe, &ctx, eligible_surface());
        assert_eq!(probe.phase, ProbePhase::Saving);
        assert_eq!(probe.receipt.issued.as_ref().unwrap().logic_frame_id, 1);
        assert_eq!(probe.receipt.captured.as_ref().unwrap().logic_frame_id, 2);
        assert_eq!(
            probe.receipt.captured.as_ref().unwrap().ui_pass_index,
            1,
            "capture evidence must belong to the last retained pass"
        );
        assert_eq!(
            probe
                .receipt
                .screenshot_received
                .as_ref()
                .unwrap()
                .logic_frame_id,
            3
        );
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::Save { .. })));
    }

    #[test]
    fn root_geometry_diagnostics_are_opt_in_and_retained_with_the_capture_pass() {
        let (mut probe, _writer_rx, _outcome_tx) = test_probe(CaptureTrigger::Startup);
        let diagnostic = || RootListDiagnostic {
            mode: "list",
            result_count: 8,
            selected_index: Some(3),
            scroll_viewport: [1.0, 2.0, 30.0, 40.0],
            content_clip: [1.0, 2.0, 30.0, 40.0],
            content_origin: [1.0, 2.0],
            scroll_inner_rect: [0.0; 4],
            content_size: [0.0; 2],
            scroll_offset: [0.0; 2],
            total_content_height: 100.0,
            visible_rows: vec![VisibleRowDiagnostic {
                index: 3,
                top: 20.0,
                bottom: 40.0,
                visual_top: 20.0,
                visual_bottom: 41.0,
                intersects_viewport: true,
            }],
            result_generation: 7,
            cache_key: "test-key".into(),
            geometry_rebuild_count: 0,
            rebuilt_this_frame: false,
            font_revision: 2,
            style_identity: "test-style".into(),
            resolved_appearance: Some("dark".into()),
            overall_text_scale: Some(1.0),
            query_scale: 1.0,
            list_scale: 1.0,
            pixels_per_point: 1.0,
            native_pixels_per_point: Some(1.0),
            zoom_factor: 1.0,
        };

        probe.record_root_list_diagnostic(diagnostic(), true);
        assert!(probe.latest_root_list_diagnostic.is_none());
        assert_eq!(probe.geometry_rebuild_count, 0);

        probe.diagnostics_enabled = true;
        probe.start_ui_pass(0);
        probe.record_root_list_diagnostic(diagnostic(), true);
        probe.finish_ui_pass(eligible_surface(), None);
        let retained = probe.retained_pass.as_ref().unwrap();
        let retained_diagnostic = retained.root_list_diagnostic.as_ref().unwrap();
        assert_eq!(retained_diagnostic.result_count, 8);
        assert_eq!(retained_diagnostic.selected_index, Some(3));
        assert_eq!(retained_diagnostic.result_generation, 7);
        assert_eq!(retained_diagnostic.geometry_rebuild_count, 1);
        assert!(retained_diagnostic.rebuilt_this_frame);
        assert_eq!(retained_diagnostic.visible_rows[0].index, 3);

        probe.phase = ProbePhase::Saving;
        assert!(!probe.root_diagnostics_active());
        probe.record_root_list_diagnostic(diagnostic(), true);
        assert_eq!(probe.geometry_rebuild_count, 1);
        assert!(probe.latest_root_list_diagnostic.is_none());
        probe.phase = ProbePhase::Complete;
        assert!(!probe.root_diagnostics_active());
    }

    #[test]
    fn same_frame_tool_transition_cancels_buffered_reply_before_save() {
        let (mut probe, writer_rx, _outcome_tx) = test_probe(CaptureTrigger::Startup);
        let ctx = egui::Context::default();

        issue_frame_one(&mut probe, &ctx);
        capture_frame_two(&mut probe, &ctx);
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::AwaitReply(_))));

        let mut final_eligibility = eligible_surface();
        final_eligibility.tool_owner = true;
        deliver_reply_frame_three(&mut probe, &ctx, final_eligibility);

        assert_eq!(probe.phase, ProbePhase::Failed);
        assert!(probe.receipt.saved.is_none());
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::Failure(_))));
    }

    #[test]
    fn hidden_ocr_and_tool_ownership_cancel_an_active_request() {
        let ctx = egui::Context::default();
        for owner in [
            SurfaceEligibility {
                ocr_owner: true,
                ..eligible_surface()
            },
            SurfaceEligibility {
                tool_owner: true,
                ..eligible_surface()
            },
        ] {
            let (mut probe, writer_rx, _outcome_tx) = test_probe(CaptureTrigger::Startup);
            issue_frame_one(&mut probe, &ctx);
            capture_frame_two(&mut probe, &ctx);
            assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::AwaitReply(_))));

            probe.revalidate_root_logic(owner);
            assert_eq!(probe.phase, ProbePhase::Failed);
            assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::Failure(_))));
        }

        let (mut hidden, writer_rx, _outcome_tx) = test_probe(CaptureTrigger::Startup);
        issue_frame_one(&mut hidden, &ctx);
        capture_frame_two(&mut hidden, &ctx);
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::AwaitReply(_))));
        hidden.run_hidden_logic_tick();
        assert_eq!(hidden.phase, ProbePhase::Failed);
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::Failure(_))));
    }

    #[test]
    fn f11_trigger_requires_unmodified_key_input_in_a_focused_root() {
        let (mut unmodified, _writer_rx, _outcome_tx) = test_probe(CaptureTrigger::F11);
        let ctx = egui::Context::default();
        let mut raw_input = egui::RawInput {
            events: vec![Event::Key {
                key: Key::F11,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        };
        raw_input
            .viewports
            .get_mut(&ViewportId::ROOT)
            .unwrap()
            .focused = Some(true);
        ctx.begin_pass(raw_input);
        unmodified.begin_root_logic(&ctx, Some(eligible_surface()));
        assert_eq!(unmodified.phase, ProbePhase::Ready);
        assert!(!ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, Event::Key { key: Key::F11, .. }))
        }));
        ctx.end_pass().drop_without_applying_deltas();

        let (mut modified, _writer_rx, _outcome_tx) = test_probe(CaptureTrigger::F11);
        let mut raw_input = egui::RawInput {
            events: vec![Event::Key {
                key: Key::F11,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::CTRL,
            }],
            ..Default::default()
        };
        raw_input
            .viewports
            .get_mut(&ViewportId::ROOT)
            .unwrap()
            .focused = Some(true);
        ctx.begin_pass(raw_input);
        modified.begin_root_logic(&ctx, Some(eligible_surface()));
        assert_eq!(modified.phase, ProbePhase::Armed);
        assert!(ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, Event::Key { key: Key::F11, .. }))
        }));
        ctx.end_pass().drop_without_applying_deltas();
    }

    #[test]
    fn matching_screenshot_token_is_consumed_without_touching_foreign_events() {
        let ctx = egui::Context::default();
        let token = UserData::new(ScreenshotToken(1));
        let foreign_token = UserData::new(ScreenshotToken(2));
        let first_image = test_image(egui::Color32::WHITE);
        let duplicate_image = test_image(egui::Color32::BLACK);
        let foreign_image = test_image(egui::Color32::RED);
        let foreign_viewport = ViewportId::from_hash_of("foreign viewport");
        let raw_input = egui::RawInput {
            events: vec![
                Event::Screenshot {
                    viewport_id: ViewportId::ROOT,
                    user_data: token.clone(),
                    image: Arc::clone(&first_image),
                },
                Event::Screenshot {
                    viewport_id: ViewportId::ROOT,
                    user_data: token.clone(),
                    image: duplicate_image,
                },
                Event::Screenshot {
                    viewport_id: ViewportId::ROOT,
                    user_data: foreign_token,
                    image: Arc::clone(&foreign_image),
                },
                Event::Screenshot {
                    viewport_id: foreign_viewport,
                    user_data: token.clone(),
                    image: foreign_image,
                },
                Event::Copy,
            ],
            ..Default::default()
        };
        ctx.begin_pass(raw_input);
        let matched = take_matching_screenshot_event(&ctx, ViewportId::ROOT, &token).unwrap();
        assert!(Arc::ptr_eq(&matched, &first_image));
        let remaining = ctx.input(|input| input.events.clone());
        assert_eq!(remaining.len(), 3);
        assert!(
            matches!(remaining[0], Event::Screenshot { viewport_id, .. } if viewport_id == ViewportId::ROOT)
        );
        assert!(
            matches!(remaining[1], Event::Screenshot { viewport_id, .. } if viewport_id == foreign_viewport)
        );
        assert!(matches!(remaining[2], Event::Copy));
        let output = ctx.end_pass();
        output.drop_without_applying_deltas();
    }

    #[test]
    fn capture_modes_reject_performance_and_track_c_benchmark_runs() {
        assert!(reject_capture_mode(true, false).is_some());
        assert!(reject_capture_mode(false, true).is_some());
        assert!(reject_capture_mode(false, false).is_none());
    }

    #[test]
    fn unknown_occlusion_keeps_root_eligible_but_explicit_hidden_states_do_not() {
        let ctx = egui::Context::default();
        let mut visible_input = egui::RawInput::default();
        let viewport = visible_input.viewports.get_mut(&ViewportId::ROOT).unwrap();
        viewport.minimized = Some(false);
        viewport.occluded = None;
        ctx.begin_pass(visible_input);
        let reported = root_viewport_visibility_reported(&ctx);
        let visible = root_visibility_from_reported(reported);
        assert!(visible);
        assert_eq!(reported, None);
        assert!(
            SurfaceEligibility {
                logical_root_visible: true,
                root_visible: visible,
                ..Default::default()
            }
            .eligible()
        );
        ctx.end_pass().drop_without_applying_deltas();

        for (minimized, occluded) in [(Some(true), None), (Some(false), Some(true))] {
            let mut hidden_input = egui::RawInput::default();
            let viewport = hidden_input.viewports.get_mut(&ViewportId::ROOT).unwrap();
            viewport.minimized = minimized;
            viewport.occluded = occluded;
            ctx.begin_pass(hidden_input);
            let reported = root_viewport_visibility_reported(&ctx);
            let visible = root_visibility_from_reported(reported);
            assert!(!visible);
            assert_eq!(reported, Some(false));
            assert!(
                !SurfaceEligibility {
                    logical_root_visible: true,
                    root_visible: visible,
                    ..Default::default()
                }
                .eligible()
            );
            ctx.end_pass().drop_without_applying_deltas();
        }
    }

    #[test]
    fn bounded_worker_queue_holds_reply_wait_and_save_without_receiver_scheduling() {
        let (probe, writer_rx, _outcome_tx) = test_probe(CaptureTrigger::Startup);
        let receipt = probe.receipt.clone();
        assert!(
            probe
                .writer_tx
                .try_send(WriterJob::AwaitReply(receipt.clone()))
                .is_ok()
        );
        assert!(
            probe
                .writer_tx
                .try_send(WriterJob::Save {
                    receipt,
                    image: test_image(egui::Color32::WHITE),
                })
                .is_ok()
        );
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::AwaitReply(_))));
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::Save { .. })));
    }

    #[test]
    fn failed_save_handoff_enters_terminal_failure_for_full_or_disconnected_queue() {
        let (mut full, writer_rx, _outcome_tx) = test_probe(CaptureTrigger::Startup);
        let receipt = full.receipt.clone();
        full.writer_tx
            .try_send(WriterJob::AwaitReply(receipt.clone()))
            .unwrap();
        full.writer_tx
            .try_send(WriterJob::Failure(receipt))
            .unwrap();
        prepare_save_attempt(&mut full);
        full.finish_retained_ui_frame(&egui::Context::default(), eligible_surface(), None);
        assert_eq!(full.phase, ProbePhase::Failed);
        assert!(full.receipt.saved.is_none());
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::AwaitReply(_))));
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::Failure(_))));

        let (mut disconnected, writer_rx, _outcome_tx) = test_probe(CaptureTrigger::Startup);
        drop(writer_rx);
        prepare_save_attempt(&mut disconnected);
        disconnected.finish_retained_ui_frame(&egui::Context::default(), eligible_surface(), None);
        assert_eq!(disconnected.phase, ProbePhase::Failed);
        assert!(disconnected.receipt.saved.is_none());
    }

    #[test]
    fn writer_does_not_overwrite_existing_png() {
        let output_dir = std::env::temp_dir().join(format!(
            "multi-launcher-qa-framebuffer-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        fs::create_dir(&output_dir).unwrap();
        let png_path = output_dir.join(PNG_FILE_NAME);
        fs::write(&png_path, b"existing evidence").unwrap();

        let request = QaFramebufferRequest {
            manifest: RequestManifest {
                run_id: "test-run".into(),
                request_id: "test-request".into(),
                target: CaptureTarget::Root,
                trigger: CaptureTrigger::Startup,
                capture_count: 1,
                diagnostics: false,
                jump_mode_observation: None,
                output_dir: output_dir.clone(),
                fixture_manifest_sha256: "a".repeat(64),
                build_receipt_identity: "test-build".into(),
            },
            manifest_sha256: "b".repeat(64),
            output_dir: output_dir.clone(),
            executable: ExecutableIdentity {
                path: PathBuf::from("test.exe"),
                sha256: "c".repeat(64),
            },
            process: ProcessIdentity {
                pid: 1,
                probe_initialized_at_unix_ms: 1,
            },
        };
        let mut receipt =
            CompletionReceipt::new(&request, selected_reply_timeout(CaptureTarget::Root));
        receipt.captured = Some(test_evidence(2, 3, 0));
        let outcome = write_job(
            &output_dir,
            WriterJob::Save {
                receipt,
                image: test_image(egui::Color32::WHITE),
            },
        );

        assert_eq!(outcome.status, ReceiptStatus::Failed);
        assert!(outcome.receipt_written);
        assert_eq!(fs::read(&png_path).unwrap(), b"existing evidence");
        let _ = fs::remove_dir_all(output_dir);
    }

    #[test]
    fn repeated_root_captures_reuse_the_writer_with_numbered_artifacts() {
        let output_dir = tempfile::tempdir().unwrap();
        let request = test_request_for_target(
            CaptureTarget::Root,
            CaptureTrigger::F11,
            output_dir.path().to_path_buf(),
        );
        let (writer_tx, writer_rx) = spawn_writer(
            output_dir.path().to_path_buf(),
            egui::Context::default(),
            Duration::from_secs(2),
        )
        .unwrap();

        for capture_index in 1..=2 {
            let mut receipt = CompletionReceipt::new(&request, SCREENSHOT_REPLY_TIMEOUT);
            receipt.capture_index = capture_index;
            receipt.capture_count = 2;
            receipt.captured = Some(test_evidence(u64::from(capture_index), 1, 0));
            writer_tx
                .send(WriterJob::AwaitReply(receipt.clone()))
                .unwrap();
            writer_tx
                .send(WriterJob::Save {
                    receipt,
                    image: test_image(egui::Color32::WHITE),
                })
                .unwrap();

            let outcome = writer_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            assert_eq!(outcome.status, ReceiptStatus::Saved);
            assert!(outcome.receipt_written);
            assert_eq!(outcome.capture_index, capture_index);
            assert_eq!(outcome.capture_count, 2);
            let (png_name, receipt_name) = capture_artifact_names(capture_index, 2);
            assert!(output_dir.path().join(png_name).is_file());
            assert!(output_dir.path().join(receipt_name).is_file());
        }
    }
}
