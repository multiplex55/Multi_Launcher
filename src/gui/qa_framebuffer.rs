use super::LauncherApp;
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
const SCREENSHOT_REPLY_TIMEOUT: Duration = Duration::from_secs(30);
const WRITER_QUEUE_CAPACITY: usize = 2;

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
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestManifest {
    run_id: String,
    request_id: String,
    target: CaptureTarget,
    trigger: CaptureTrigger,
    output_dir: PathBuf,
    fixture_manifest_sha256: String,
    build_receipt_identity: String,
}

/// A validated, single-use capture request prepared by the production GUI
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
}

#[derive(Debug, PartialEq, Eq)]
struct ScreenshotToken(u64);

struct PendingScreenshotReply {
    image: Arc<ColorImage>,
    evidence: ScreenshotReplyEvidence,
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
}

#[derive(Clone, Debug, Serialize)]
struct ScreenshotReplyEvidence {
    viewport_id: &'static str,
    logic_frame_id: u64,
    received_at_unix_ms: u64,
    width: u64,
    height: u64,
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
    run_id: String,
    request_id: String,
    target: CaptureTarget,
    trigger: CaptureTrigger,
    fixture_manifest_sha256: String,
    build_receipt_identity: String,
    manifest_sha256: String,
    executable: ExecutableIdentity,
    process: ProcessIdentity,
    capture_semantics: &'static str,
    issued: Option<FrameEvidence>,
    captured: Option<FrameEvidence>,
    screenshot_received: Option<ScreenshotReplyEvidence>,
    saved: Option<SaveEvidence>,
    failure: Option<String>,
}

impl CompletionReceipt {
    fn new(request: &QaFramebufferRequest) -> Self {
        Self {
            schema_version: 1,
            status: ReceiptStatus::Failed,
            run_id: request.manifest.run_id.clone(),
            request_id: request.manifest.request_id.clone(),
            target: request.manifest.target,
            trigger: request.manifest.trigger,
            fixture_manifest_sha256: request.manifest.fixture_manifest_sha256.clone(),
            build_receipt_identity: request.manifest.build_receipt_identity.clone(),
            manifest_sha256: request.manifest_sha256.clone(),
            executable: request.executable.clone(),
            process: request.process.clone(),
            capture_semantics: "ROOT framebuffer pixels; CPU UI completion before GPU presentation",
            issued: None,
            captured: None,
            screenshot_received: None,
            saved: None,
            failure: None,
        }
    }
}

pub(super) struct FramebufferProbe {
    token: UserData,
    phase: ProbePhase,
    logic_frame_id: u64,
    ui_pass_id: u64,
    current_ui_pass_id: Option<u64>,
    current_ui_pass_index: u32,
    retained_pass: Option<RetainedPassMarker>,
    issue_logic_frame_id: Option<u64>,
    capture_logic_frame_id: Option<u64>,
    expected_reply_logic_frame_id: Option<u64>,
    pending_reply: Option<PendingScreenshotReply>,
    receipt: CompletionReceipt,
    writer_tx: SyncSender<WriterJob>,
    writer_rx: Receiver<WriterOutcome>,
}

impl FramebufferProbe {
    fn new(request: QaFramebufferRequest, ctx: egui::Context) -> Result<Self, String> {
        let (writer_tx, writer_rx) = spawn_writer(request.output_dir.clone(), ctx)?;
        let token = UserData::new(ScreenshotToken(1));
        let receipt = CompletionReceipt::new(&request);
        Ok(Self {
            token,
            phase: ProbePhase::Armed,
            logic_frame_id: 0,
            ui_pass_id: 0,
            current_ui_pass_id: None,
            current_ui_pass_index: 0,
            retained_pass: None,
            issue_logic_frame_id: None,
            capture_logic_frame_id: None,
            expected_reply_logic_frame_id: None,
            pending_reply: None,
            receipt,
            writer_tx,
            writer_rx,
        })
    }

    fn begin_root_logic(&mut self, ctx: &egui::Context, eligibility: Option<SurfaceEligibility>) {
        if !self.needs_root_logic() {
            return;
        }
        self.logic_frame_id = self.logic_frame_id.saturating_add(1);
        self.retained_pass = None;
        self.current_ui_pass_id = None;

        if matches!(self.phase, ProbePhase::Issued | ProbePhase::AwaitingReply)
            && let Some(image) = take_matching_screenshot_event(ctx, &self.token)
            && self.pending_reply.is_none()
        {
            let [width, height] = image.size;
            let evidence = ScreenshotReplyEvidence {
                viewport_id: "ROOT",
                logic_frame_id: self.logic_frame_id,
                received_at_unix_ms: unix_millis(),
                width: u64::try_from(width).unwrap_or(u64::MAX),
                height: u64::try_from(height).unwrap_or(u64::MAX),
            };
            self.receipt.screenshot_received = Some(evidence.clone());
            self.pending_reply = Some(PendingScreenshotReply { image, evidence });
        }

        if matches!(self.phase, ProbePhase::AwaitingReply | ProbePhase::Saving) {
            self.poll_writer_outcome();
        }
        if !self.needs_root_logic() {
            return;
        }

        if let Some(eligibility) = eligibility
            && self.receipt.trigger == CaptureTrigger::F11
            && self.phase == ProbePhase::Armed
            && eligibility.eligible()
            && ctx.input(|input| input.viewport().focused == Some(true))
            && take_unmodified_f11(ctx)
        {
            self.phase = ProbePhase::Ready;
        }

        if self.phase_is_active()
            && let Some(eligibility) = eligibility
            && !eligibility.eligible()
        {
            self.fail(eligibility.failure_reason());
        }
    }

    fn revalidate_root_logic(&mut self, eligibility: SurfaceEligibility) {
        if self.phase_is_active() && !eligibility.eligible() {
            self.fail(eligibility.failure_reason());
        }
    }

    fn run_hidden_logic_tick(&mut self) {
        if self.phase == ProbePhase::Saving {
            self.poll_writer_outcome();
        } else if self.phase_is_active() {
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
        });
    }

    fn issue_screenshot_if_ready(
        &mut self,
        ctx: &egui::Context,
        eligibility: SurfaceEligibility,
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
        self.receipt.issued = Some(evidence);
        self.issue_logic_frame_id = Some(self.logic_frame_id);
        self.phase = ProbePhase::Issued;
        ctx.send_viewport_cmd_to(
            ViewportId::ROOT,
            ViewportCommand::Screenshot(self.token.clone()),
        );
        // Glow services a screenshot command after the next paint and returns
        // its event on the following input. One repaint requests that paint.
        ctx.request_repaint_of(ViewportId::ROOT);
    }

    fn finish_retained_ui_frame(
        &mut self,
        ctx: &egui::Context,
        final_eligibility: SurfaceEligibility,
    ) {
        let Some(mut marker) = self.retained_pass.take() else {
            if self.phase_is_active() {
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
            evidence.eligible = marker.eligibility.eligible();
        }

        match self.phase {
            ProbePhase::Armed if self.receipt.trigger == CaptureTrigger::Startup => {
                if marker.eligibility.eligible() {
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
                    }
                } else if marker.logic_frame_id > issue_frame {
                    if !marker.eligibility.eligible() {
                        self.fail(marker.eligibility.failure_reason());
                    } else {
                        let Some(mut captured) = marker.evidence.clone() else {
                            self.fail("retained capture frame omitted its provenance snapshot");
                            return;
                        };
                        debug_assert_eq!(captured.ui_pass_id, marker.ui_pass_id);
                        debug_assert_eq!(captured.ui_pass_index, marker.ui_pass_index);
                        captured.evidence_stage = "expected_capture_frame_cpu_ui_completion";
                        self.receipt.captured = Some(captured);
                        self.capture_logic_frame_id = Some(marker.logic_frame_id);
                        self.expected_reply_logic_frame_id =
                            Some(marker.logic_frame_id.saturating_add(1));
                        self.phase = ProbePhase::AwaitingReply;
                        match self
                            .writer_tx
                            .try_send(WriterJob::AwaitReply(self.receipt.clone()))
                        {
                            Ok(()) => {
                                // The capture frame is N+1; one bounded repaint
                                // requests the reply input on N+2.
                                ctx.request_repaint_of(ViewportId::ROOT);
                            }
                            Err(error) => {
                                self.fail(format!("failed to arm screenshot reply wait: {error}"));
                            }
                        }
                    }
                }
            }
            ProbePhase::AwaitingReply => {
                if !marker.eligibility.eligible() {
                    self.fail(marker.eligibility.failure_reason());
                } else if let (Some(expected_reply), Some(reply)) = (
                    self.expected_reply_logic_frame_id,
                    self.pending_reply.take(),
                ) {
                    if self.logic_frame_id >= expected_reply
                        && reply.evidence.logic_frame_id >= expected_reply
                    {
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
                    } else {
                        self.pending_reply = Some(reply);
                    }
                }
            }
        }
    }

    fn phase_is_active(&self) -> bool {
        matches!(
            self.phase,
            ProbePhase::Ready | ProbePhase::Issued | ProbePhase::AwaitingReply
        )
    }

    fn needs_retained_marker(&self) -> bool {
        matches!(
            self.phase,
            ProbePhase::Armed | ProbePhase::Ready | ProbePhase::Issued | ProbePhase::AwaitingReply
        )
    }

    fn needs_input_eligibility(&self) -> bool {
        self.phase == ProbePhase::Armed || self.phase_is_active()
    }

    fn needs_root_logic(&self) -> bool {
        self.needs_input_eligibility() || self.phase == ProbePhase::Saving
    }

    fn fail(&mut self, reason: impl Into<String>) {
        if !self.phase_is_active() {
            return;
        }
        self.receipt.status = ReceiptStatus::Failed;
        self.receipt.failure = Some(reason.into());
        self.phase = ProbePhase::Failed;
        let _ = self
            .writer_tx
            .try_send(WriterJob::Failure(self.receipt.clone()));
    }

    fn poll_writer_outcome(&mut self) {
        match self.writer_rx.try_recv() {
            Ok(outcome) => {
                self.phase = if outcome.status == ReceiptStatus::Saved && outcome.receipt_written {
                    ProbePhase::Complete
                } else {
                    ProbePhase::Failed
                };
                self.receipt.status = outcome.status;
                if let Some(error) = outcome.error {
                    self.receipt.failure = Some(error);
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
}

impl LauncherApp {
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
            .is_some_and(FramebufferProbe::phase_is_active)
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
            .filter(|probe| probe.phase_is_active() || probe.phase == ProbePhase::Saving)
        {
            probe.run_hidden_logic_tick();
        }
    }

    pub(super) fn qa_framebuffer_start_ui_pass(&mut self, pass_index: u32) {
        if let Some(probe) = self.qa_framebuffer.as_mut() {
            probe.start_ui_pass(pass_index);
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
            .filter(|probe| probe.phase == ProbePhase::Ready)
            .and_then(|probe| {
                probe
                    .current_ui_pass_id
                    .map(|pass_id| (probe.logic_frame_id, pass_id, probe.current_ui_pass_index))
            })
        else {
            return;
        };
        let eligibility = surface_eligibility(self, ctx, logical_root_visible);
        let evidence = frame_evidence(
            self,
            ctx,
            logic_frame_id,
            ui_pass_id,
            ui_pass_index,
            "screenshot_issued",
            logical_root_visible,
            eligibility,
        );
        if let Some(probe) = self.qa_framebuffer.as_mut() {
            probe.issue_screenshot_if_ready(ctx, eligibility, evidence);
        }
    }

    pub(super) fn qa_framebuffer_finish_ui_pass(
        &mut self,
        ctx: &egui::Context,
        logical_root_visible: bool,
    ) {
        let Some((logic_frame_id, ui_pass_id, ui_pass_index, should_record_evidence)) = self
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
                    )
                })
            })
        else {
            return;
        };
        let eligibility = surface_eligibility(self, ctx, logical_root_visible);
        let evidence = should_record_evidence.then(|| {
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
        if let Some(probe) = self.qa_framebuffer.as_mut() {
            probe.finish_retained_ui_frame(&ctx, eligibility);
        }
    }
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
    }
}

fn take_matching_screenshot_event(
    ctx: &egui::Context,
    token: &UserData,
) -> Option<Arc<ColorImage>> {
    ctx.input_mut(|input| {
        let mut first_image = None;
        input.events.retain(|event| {
            if let Event::Screenshot {
                viewport_id,
                user_data,
                image,
            } = event
                && *viewport_id == ViewportId::ROOT
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
}

fn spawn_writer(
    output_dir: PathBuf,
    ctx: egui::Context,
) -> Result<(SyncSender<WriterJob>, Receiver<WriterOutcome>), String> {
    let (job_tx, job_rx) = mpsc::sync_channel(WRITER_QUEUE_CAPACITY);
    let (outcome_tx, outcome_rx) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("qa-framebuffer-writer".into())
        .spawn(move || {
            if let Ok(job) = job_rx.recv() {
                let job = match job {
                    WriterJob::AwaitReply(mut receipt) => {
                        match job_rx.recv_timeout(SCREENSHOT_REPLY_TIMEOUT) {
                            Ok(reply_job @ WriterJob::Save { .. })
                            | Ok(reply_job @ WriterJob::Failure(_)) => reply_job,
                            Ok(WriterJob::AwaitReply(_)) => {
                                receipt.failure =
                                    Some("duplicate screenshot reply wait was rejected".into());
                                WriterJob::Failure(receipt)
                            }
                            Err(RecvTimeoutError::Timeout) => {
                                receipt.failure = Some(format!(
                                    "ROOT screenshot reply was not delivered within {} seconds",
                                    SCREENSHOT_REPLY_TIMEOUT.as_secs()
                                ));
                                WriterJob::Failure(receipt)
                            }
                            Err(RecvTimeoutError::Disconnected) => {
                                receipt.failure = Some(
                                    "capture owner exited before the ROOT screenshot reply arrived"
                                        .into(),
                                );
                                WriterJob::Failure(receipt)
                            }
                        }
                    }
                    job => job,
                };
                let outcome = write_job(&output_dir, job);
                let _ = outcome_tx.send(outcome);
                ctx.request_repaint_of(ViewportId::ROOT);
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
            let image_dimensions = image_dimensions(&image);
            match image_dimensions.and_then(|(width, height)| encode_png(&image, width, height)) {
                Ok((width, height, png)) => {
                    let png_sha256 = sha256_bytes(&png);
                    let png_path = output_dir.join(PNG_FILE_NAME);
                    match write_new_file(&png_path, &png) {
                        Ok(()) => {
                            let capture_evidence = receipt.captured.as_ref();
                            receipt.status = ReceiptStatus::Saved;
                            receipt.saved = Some(SaveEvidence {
                                completed_at_unix_ms: unix_millis(),
                                png_file: PNG_FILE_NAME.into(),
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
            };
        }
    };
    match write_new_file(&output_dir.join(RECEIPT_FILE_NAME), &receipt_bytes) {
        Ok(()) => WriterOutcome {
            status,
            error,
            receipt_written: true,
        },
        Err(write_error) => WriterOutcome {
            status: ReceiptStatus::Failed,
            error: Some(format!(
                "failed to create completion receipt exclusively: {write_error}"
            )),
            receipt_written: false,
        },
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
    use std::sync::mpsc::TryRecvError;

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
        let request = QaFramebufferRequest {
            manifest: RequestManifest {
                run_id: "test-run".into(),
                request_id: "test-request".into(),
                target: CaptureTarget::Root,
                trigger,
                output_dir: PathBuf::from("unused"),
                fixture_manifest_sha256: "a".repeat(64),
                build_receipt_identity: "test-build".into(),
            },
            manifest_sha256: "b".repeat(64),
            output_dir: PathBuf::from("unused"),
            executable: ExecutableIdentity {
                path: PathBuf::from("test.exe"),
                sha256: "c".repeat(64),
            },
            process: ProcessIdentity {
                pid: 1,
                probe_initialized_at_unix_ms: 1,
            },
        };
        let token = UserData::new(ScreenshotToken(1));
        let (writer_tx, writer_rx) = mpsc::sync_channel(WRITER_QUEUE_CAPACITY);
        let (outcome_tx, outcome_rx) = mpsc::sync_channel(1);
        let probe = FramebufferProbe {
            token,
            phase: ProbePhase::Armed,
            logic_frame_id: 0,
            ui_pass_id: 0,
            current_ui_pass_id: None,
            current_ui_pass_index: 0,
            retained_pass: None,
            issue_logic_frame_id: None,
            capture_logic_frame_id: None,
            expected_reply_logic_frame_id: None,
            pending_reply: None,
            receipt: CompletionReceipt::new(&request),
            writer_tx,
            writer_rx: outcome_rx,
        };
        (probe, writer_rx, outcome_tx)
    }

    fn issue_frame_one(probe: &mut FramebufferProbe, ctx: &egui::Context) {
        let eligibility = eligible_surface();
        probe.logic_frame_id = 1;
        probe.phase = ProbePhase::Ready;
        ctx.begin_pass(egui::RawInput::default());
        probe.start_ui_pass(0);
        let issue_evidence = test_evidence(1, probe.current_ui_pass_id.unwrap(), 0);
        probe.issue_screenshot_if_ready(ctx, eligibility, issue_evidence.clone());
        probe.finish_ui_pass(eligibility, Some(issue_evidence));
        probe.finish_retained_ui_frame(ctx, eligibility);
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
        probe.finish_retained_ui_frame(ctx, eligibility);
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
            },
        });
        probe.retained_pass = Some(RetainedPassMarker {
            logic_frame_id: 3,
            ui_pass_id: 3,
            ui_pass_index: 0,
            eligibility,
            evidence: Some(test_evidence(3, 3, 0)),
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
        probe.finish_retained_ui_frame(ctx, eligible_surface());
        let output = ctx.end_pass();
        output.drop_without_applying_deltas();
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
        let matched = take_matching_screenshot_event(&ctx, &token).unwrap();
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
        full.finish_retained_ui_frame(&egui::Context::default(), eligible_surface());
        assert_eq!(full.phase, ProbePhase::Failed);
        assert!(full.receipt.saved.is_none());
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::AwaitReply(_))));
        assert!(matches!(writer_rx.try_recv(), Ok(WriterJob::Failure(_))));

        let (mut disconnected, writer_rx, _outcome_tx) = test_probe(CaptureTrigger::Startup);
        drop(writer_rx);
        prepare_save_attempt(&mut disconnected);
        disconnected.finish_retained_ui_frame(&egui::Context::default(), eligible_surface());
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
        let mut receipt = CompletionReceipt::new(&request);
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
}
