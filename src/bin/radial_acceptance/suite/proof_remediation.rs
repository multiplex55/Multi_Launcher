//! The existing H15/L08 contracts use real tool, ROOT, input, and dispatch owners.
//! Suppressed Screen Draw input is deliberately separate from normal launcher
//! admission evidence, which remains subject to the ordinary Hotkey oracle.

use super::super::super::{
    PRIORITY_SMOKE_ACTION_LABEL, PRIORITY_SMOKE_QUERY, PRIORITY_SMOKE_TOOL_TITLE,
    PriorityConfiguredPrimaryEvidence, PriorityForegroundOwner, PriorityHookAdmissionEvidence,
    PriorityHookOwner, PriorityHookPrimaryEvidence, QUERY_CONFIGURED_ROOT_CLIENT_SIZE,
    QUERY_CONFIGURED_ROOT_POSITION, QueryPlacementEvidence, QueryPlacementToggleEvidence,
    ScreenDrawLauncherObservation, ScreenDrawParkingObservation, ScreenDrawParkingState,
    ScreenDrawPriorityEvidence, ScreenDrawRootIdentity, ScreenDrawToolbarMode,
    ScreenDrawToolbarObservation, ScreenDrawToolbarReceipt, ScreenDrawToolbarRole,
    ScreenDrawToolbarTarget, ScreenDrawToolbarWidget, query_cell_digest,
    validate_screen_draw_baseline_receipt, validate_screen_draw_recovery_owner,
    validate_screen_draw_toolbar_receipt,
};
use super::*;

fn failure(stage: FailureStage, message: impl Into<String>) -> CaseFailure {
    CaseFailure::new(stage, message.into())
}

fn presentation(window: &WindowSnapshot, displays: &[[i32; 4]]) -> QueryRootPresentationEvidence {
    QueryRootPresentationEvidence {
        hwnd: hwnd_id(window.hwnd),
        process_id: window.process_id,
        bounds: window.bounds,
        visible: window.visible,
        minimized: window.minimized,
        physically_visible: window.visible
            && !window.minimized
            && intersects_display_bounds(window.bounds, displays),
    }
}

fn settled_root(child: &NativeChild, visible: bool) -> Result<WindowSnapshot, CaseFailure> {
    let deadline = Instant::now() + ROOT_TIMEOUT;
    let mut previous = None;
    loop {
        let root = child.refresh_root().map_err(query_window_error)?;
        if root_is_physically_visible(child, &root)? == visible {
            if previous == Some(root.bounds) {
                return Ok(root);
            }
            previous = Some(root.bounds);
        } else {
            previous = None;
        }
        if Instant::now() >= deadline {
            return Err(failure(
                FailureStage::NativeRootState,
                "ROOT geometry did not settle in its required physical presentation",
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn moved_root_position(root: [i32; 4], displays: &[[i32; 4]]) -> Result<[i32; 2], String> {
    let width = root[2]
        .checked_sub(root[0])
        .filter(|width| *width > 0)
        .ok_or("invalid ROOT width")?;
    let height = root[3]
        .checked_sub(root[1])
        .filter(|height| *height > 0)
        .ok_or("invalid ROOT height")?;
    for display in displays {
        // Stay on the same monitor so moving the window cannot introduce a
        // different DPI lifetime into the restore-only geometry experiment.
        if root[0] < display[0]
            || root[1] < display[1]
            || root[2] > display[2]
            || root[3] > display[3]
        {
            continue;
        }
        for (dx, dy) in [(80, 80), (-80, -80), (80, -80), (-80, 80)] {
            let Some(left) = root[0].checked_add(dx) else {
                continue;
            };
            let Some(top) = root[1].checked_add(dy) else {
                continue;
            };
            let (Some(right), Some(bottom)) = (left.checked_add(width), top.checked_add(height))
            else {
                continue;
            };
            if left >= display[0]
                && top >= display[1]
                && right <= display[2]
                && bottom <= display[3]
            {
                return Ok([left, top]);
            }
        }
    }
    Err("no distinct fully onscreen ROOT position fits the actual current monitor".into())
}

const FIXTURE_NOTE_MARKER: &str = "# radial acceptance q11";
const MAX_NOTE_CLOSE_PHASES: usize = 24;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum NoteCloseMeasurement<T> {
    NotAttempted,
    Pending,
    Observed(T),
    Failed(NoteCloseError),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteCloseError {
    detail: String,
    original_bytes: usize,
    truncated: bool,
}

impl NoteCloseError {
    fn new(message: &str) -> Self {
        Self {
            detail: bounded_text(message, 512),
            original_bytes: message.len(),
            truncated: message.len() > 512,
        }
    }
}

impl<T> NoteCloseMeasurement<T> {
    fn failed(message: &str) -> Self {
        Self::Failed(NoteCloseError {
            detail: bounded_text(message, 512),
            original_bytes: message.len(),
            truncated: message.len() > 512,
        })
    }
}

impl<T> Default for NoteCloseMeasurement<T> {
    fn default() -> Self {
        Self::NotAttempted
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum NoteClosePhase {
    #[default]
    NotAttempted,
    SettlingRoot,
    LocatingEditor,
    NativeRootFocus,
    SemanticFocusRequest,
    WaitingForEditorFocus,
    Ready,
    Revalidating,
    EscapeInsertion,
    WaitingForNormalClose,
    DiscardAction,
    WaitingForDiscardClose,
    Complete,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteCloseRoot {
    hwnd: u64,
    process_id: u32,
    role: NoteCloseWindowRole,
    visible: bool,
    minimized: bool,
    bounds: [i32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum NoteCloseWindowRole {
    Root,
    Designer,
    OtherChild,
}

impl From<&WindowSnapshot> for NoteCloseRoot {
    fn from(root: &WindowSnapshot) -> Self {
        Self {
            hwnd: hwnd_id(root.hwnd),
            process_id: root.process_id,
            role: match root.role {
                WindowRole::Root => NoteCloseWindowRole::Root,
                WindowRole::Designer => NoteCloseWindowRole::Designer,
                WindowRole::OtherChild => NoteCloseWindowRole::OtherChild,
            },
            visible: root.visible,
            minimized: root.minimized,
            bounds: root.bounds,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteClosePhaseObservation {
    phase: NoteClosePhase,
    elapsed_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NoteCloseAttempt {
    phase: NoteClosePhase,
    failed_phase: Option<NoteClosePhase>,
    phases: Vec<NoteClosePhaseObservation>,
    phases_omitted: usize,
    observation_count: usize,
    elapsed_ms: u64,
    close_deadline_ms: Option<u64>,
    discard_deadline_ms: Option<u64>,
    deadline_expired: bool,
    expected_root: Option<NoteCloseRoot>,
    current_root: NoteCloseMeasurement<NoteCloseRoot>,
    admitted_editor: NoteCloseMeasurement<SemanticControlReadiness>,
    current_editor: NoteCloseMeasurement<SemanticControlReadiness>,
    marker_present: NoteCloseMeasurement<bool>,
    native_focus: NoteCloseMeasurement<()>,
    focus_request: NoteCloseMeasurement<()>,
    foreground: NoteCloseMeasurement<(u64, u32)>,
    escape_requested: bool,
    escape_inserted: NoteCloseMeasurement<u32>,
    discard_prompt: NoteCloseMeasurement<bool>,
    discard_action: NoteCloseMeasurement<()>,
    gui_request: NoteCloseMeasurement<NoteCloseGuiRequest>,
    gui_ownership: NoteCloseMeasurement<NoteCloseGuiResponse>,
    gui_nonce: Option<[u64; 2]>,
    gui_observation_error: Option<NoteCloseError>,
    note_absent: NoteCloseMeasurement<bool>,
    outcome: NoteCloseMeasurement<()>,
}

impl NoteCloseAttempt {
    fn enter(&mut self, phase: NoteClosePhase, elapsed: Duration) {
        self.phase = phase;
        self.elapsed_ms = duration_millis(elapsed);
        if self.phases.len() < MAX_NOTE_CLOSE_PHASES {
            self.phases.push(NoteClosePhaseObservation {
                phase,
                elapsed_ms: self.elapsed_ms,
            });
        } else {
            self.phases_omitted = self.phases_omitted.saturating_add(1);
        }
    }

    fn bound(&mut self) {
        fn message<T>(measurement: &mut NoteCloseMeasurement<T>) {
            if let NoteCloseMeasurement::Failed(error) = measurement {
                error.truncated |= error.detail.len() > 512;
                error.detail = bounded_text(&error.detail, 512);
            }
        }
        self.phases_omitted = self
            .phases_omitted
            .saturating_add(self.phases.len().saturating_sub(MAX_NOTE_CLOSE_PHASES));
        self.phases.truncate(MAX_NOTE_CLOSE_PHASES);
        message(&mut self.current_root);
        message(&mut self.admitted_editor);
        message(&mut self.current_editor);
        message(&mut self.marker_present);
        message(&mut self.native_focus);
        message(&mut self.focus_request);
        message(&mut self.foreground);
        message(&mut self.escape_inserted);
        message(&mut self.discard_prompt);
        message(&mut self.discard_action);
        message(&mut self.gui_request);
        message(&mut self.gui_ownership);
        if let Some(error) = &mut self.gui_observation_error {
            error.truncated |= error.detail.len() > 512;
            error.detail = bounded_text(&error.detail, 512);
        }
        message(&mut self.note_absent);
        message(&mut self.outcome);
    }
}

fn duration_millis(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

struct NoteEditorObservation {
    marker_present: bool,
    readiness: SemanticControlReadiness,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoteEditorPurpose {
    Focus,
    DiscardConfirmation,
}

struct NoteDiscardObservation {
    readiness: SemanticControlReadiness,
    is_button: bool,
    client_screen_bounds: [i32; 4],
}

/// The production adapter owns the COM elements and native handles. The shared
/// operation owns ordering/deadlines, so deterministic tests exercise that same
/// operation rather than a second model of its acknowledgement protocol.
trait FixtureNoteCloseBackend {
    type Editor;
    type Discard;
    fn now(&self) -> Duration;
    fn poll(&mut self);
    fn settled_root(&mut self) -> Result<WindowSnapshot, CaseFailure>;
    fn current_root(&mut self) -> Result<WindowSnapshot, CaseFailure>;
    fn find_editor(
        &mut self,
        root: &WindowSnapshot,
        purpose: NoteEditorPurpose,
    ) -> Result<Option<Self::Editor>, VisibleTextLookupError>;
    fn editor_observation(
        &mut self,
        root: &WindowSnapshot,
        admitted: &Self::Editor,
        purpose: NoteEditorPurpose,
    ) -> Result<NoteEditorObservation, VisibleTextLookupError>;
    fn find_discard(
        &mut self,
        root: &WindowSnapshot,
    ) -> Result<Option<Self::Discard>, VisibleTextLookupError>;
    fn note_ownership(
        &mut self,
        root: &WindowSnapshot,
        request: &NoteCloseGuiRequest,
        remaining: Duration,
    ) -> Result<NoteCloseGuiResponse, CaseFailure>;
    fn client_screen_bounds(&mut self, root: &WindowSnapshot) -> Result<[i32; 4], CaseFailure>;
    fn current_discard(
        &mut self,
        root: &WindowSnapshot,
        admitted: &Self::Discard,
    ) -> Result<(Self::Discard, NoteDiscardObservation), VisibleTextLookupError>;
    fn focus_root(&mut self, root: &WindowSnapshot) -> Result<(), CaseFailure>;
    fn request_editor_focus(&mut self, edit: &Self::Editor) -> Result<(), CaseFailure>;
    fn foreground(&mut self) -> (u64, u32);
    fn escape(&mut self, root: &WindowSnapshot) -> Result<u32, CaseFailure>;
    fn discard(
        &mut self,
        root: &WindowSnapshot,
        control: &Self::Discard,
    ) -> Result<(), CaseFailure>;
}

struct NativeFixtureNoteClose<'a> {
    child: &'a NativeChild,
    ui: &'a UiAutomation,
    trace_path: &'a Path,
    started: Instant,
}

impl FixtureNoteCloseBackend for NativeFixtureNoteClose<'_> {
    type Editor = SemanticControl;
    type Discard = SemanticControl;
    fn now(&self) -> Duration {
        self.started.elapsed()
    }
    fn poll(&mut self) {
        std::thread::sleep(WINDOW_POLL);
    }
    fn settled_root(&mut self) -> Result<WindowSnapshot, CaseFailure> {
        settled_root(self.child, true)
    }
    fn current_root(&mut self) -> Result<WindowSnapshot, CaseFailure> {
        self.child.refresh_root().map_err(query_window_error)
    }
    fn find_editor(
        &mut self,
        root: &WindowSnapshot,
        purpose: NoteEditorPurpose,
    ) -> Result<Option<SemanticControl>, VisibleTextLookupError> {
        match purpose {
            NoteEditorPurpose::Focus => self.ui.find_edit_with_value_fragment_classified(
                root.hwnd,
                self.child.process_id(),
                FIXTURE_NOTE_MARKER,
            ),
            NoteEditorPurpose::DiscardConfirmation => self
                .ui
                .find_edit_with_value_fragment_for_confirmation_classified(
                    root.hwnd,
                    self.child.process_id(),
                    FIXTURE_NOTE_MARKER,
                ),
        }
    }
    fn editor_observation(
        &mut self,
        root: &WindowSnapshot,
        admitted: &SemanticControl,
        purpose: NoteEditorPurpose,
    ) -> Result<NoteEditorObservation, VisibleTextLookupError> {
        // Inspect the admitted element even when the visible lookup filters it
        // out. A now-disabled/hidden target is not a successful note close.
        let admitted_status = self.ui.current_control_readiness(admitted, admitted)?;
        let current = self.find_editor(root, purpose)?;
        Ok(NoteEditorObservation {
            marker_present: current.is_some(),
            readiness: match current {
                Some(current) => self.ui.current_control_readiness(admitted, &current)?,
                None => admitted_status,
            },
        })
    }
    fn find_discard(
        &mut self,
        root: &WindowSnapshot,
    ) -> Result<Option<SemanticControl>, VisibleTextLookupError> {
        self.ui.find_visible_button_classified(
            root.hwnd,
            self.child.process_id(),
            "Discard Changes",
        )
    }
    fn note_ownership(
        &mut self,
        root: &WindowSnapshot,
        request: &NoteCloseGuiRequest,
        remaining: Duration,
    ) -> Result<NoteCloseGuiResponse, CaseFailure> {
        request_note_close_gui_observation(self.child, root, request, remaining)
    }
    fn current_discard(
        &mut self,
        root: &WindowSnapshot,
        admitted: &SemanticControl,
    ) -> Result<(SemanticControl, NoteDiscardObservation), VisibleTextLookupError> {
        let current = self.find_discard(root)?.ok_or_else(|| {
            VisibleTextLookupError::Other(
                "fixture confirmation disappeared before its click".into(),
            )
        })?;
        let observation = NoteDiscardObservation {
            readiness: self.ui.current_control_readiness(admitted, &current)?,
            is_button: self.ui.current_control_is_button(&current)?,
            client_screen_bounds: self
                .child
                .client_screen_bounds(root)
                .map_err(VisibleTextLookupError::Other)?,
        };
        Ok((current, observation))
    }
    fn client_screen_bounds(&mut self, root: &WindowSnapshot) -> Result<[i32; 4], CaseFailure> {
        self.child
            .client_screen_bounds(root)
            .map_err(query_window_error)
    }
    fn focus_root(&mut self, root: &WindowSnapshot) -> Result<(), CaseFailure> {
        self.child.focus_window(root).map_err(query_window_error)
    }
    fn request_editor_focus(&mut self, edit: &SemanticControl) -> Result<(), CaseFailure> {
        self.ui.focus(edit).map_err(query_uia_error)
    }
    fn foreground(&mut self) -> (u64, u32) {
        let (hwnd, pid) = capture_foreground();
        (hwnd_id(hwnd), pid)
    }
    fn escape(&mut self, root: &WindowSnapshot) -> Result<u32, CaseFailure> {
        send_escape_to_focused_window(self.child, root)
            .map_err(query_uia_error)
            .and_then(checked_note_escape_count)
    }
    fn discard(
        &mut self,
        root: &WindowSnapshot,
        control: &SemanticControl,
    ) -> Result<(), CaseFailure> {
        click_semantic_control(self.child, root, control, self.trace_path)
            .map(|_| ())
            .map_err(query_uia_error)
    }
}

fn checked_note_escape_count(inserted: usize) -> Result<u32, CaseFailure> {
    u32::try_from(inserted).map_err(|_| {
        failure(
            FailureStage::InputInjection,
            "fixture note Escape insertion count exceeds its diagnostic representation",
        )
    })
}

fn checked_note_root<B: FixtureNoteCloseBackend>(
    backend: &mut B,
    expected: &WindowSnapshot,
    attempt: &mut NoteCloseAttempt,
) -> Result<WindowSnapshot, CaseFailure> {
    let current = backend.current_root().inspect_err(|error| {
        attempt.current_root = NoteCloseMeasurement::failed(&error.message);
    })?;
    attempt.current_root = NoteCloseMeasurement::Observed(NoteCloseRoot::from(&current));
    if current.hwnd != expected.hwnd
        || hwnd_id(current.hwnd) == 0
        || current.process_id != expected.process_id
        || current.process_id == 0
        || current.role != WindowRole::Root
        || !current.visible
        || current.minimized
        || !current.is_nonzero()
    {
        return Err(failure(
            FailureStage::NativeRootState,
            "fixture note close lost its current visible owned ROOT",
        ));
    }
    Ok(current)
}

fn note_deadline<B: FixtureNoteCloseBackend>(
    backend: &B,
    deadline: Duration,
    attempt: &mut NoteCloseAttempt,
) -> Result<(), CaseFailure> {
    attempt.elapsed_ms = duration_millis(backend.now());
    if backend.now() >= deadline {
        attempt.deadline_expired = true;
        Err(failure(
            FailureStage::Cleanup,
            "fixture note close exceeded its existing normal UI deadline",
        ))
    } else {
        Ok(())
    }
}

fn note_editor_ready<B: FixtureNoteCloseBackend>(
    backend: &mut B,
    root: &WindowSnapshot,
    edit: &B::Editor,
    attempt: &mut NoteCloseAttempt,
) -> Result<bool, CaseFailure> {
    attempt.observation_count = attempt.observation_count.saturating_add(1);
    let observation = match backend.editor_observation(root, edit, NoteEditorPurpose::Focus) {
        Ok(observation) => observation,
        Err(VisibleTextLookupError::TransientElementUnavailable(message)) => {
            attempt.current_editor = NoteCloseMeasurement::failed(&message);
            return Ok(false);
        }
        Err(VisibleTextLookupError::Other(message)) => {
            attempt.current_editor = NoteCloseMeasurement::failed(&message);
            return Err(query_uia_error(message));
        }
    };
    let focused =
        record_note_editor_observation(root, observation, NoteEditorPurpose::Focus, attempt)?;
    let foreground = backend.foreground();
    attempt.foreground = NoteCloseMeasurement::Observed(foreground);
    if foreground != (hwnd_id(root.hwnd), root.process_id) {
        return Err(failure(
            FailureStage::NativeRootState,
            "fixture note close foreground does not match its owned ROOT",
        ));
    }
    Ok(focused)
}

fn record_note_editor_observation(
    root: &WindowSnapshot,
    observation: NoteEditorObservation,
    purpose: NoteEditorPurpose,
    attempt: &mut NoteCloseAttempt,
) -> Result<bool, CaseFailure> {
    attempt.marker_present = NoteCloseMeasurement::Observed(observation.marker_present);
    attempt.current_editor = NoteCloseMeasurement::Observed(observation.readiness.clone());
    let status = observation.readiness;
    if !observation.marker_present
        || !status.same_as_admitted
        || status.process_id != root.process_id
        || (purpose == NoteEditorPurpose::Focus && !status.enabled)
        || status.offscreen
        || status.bounds[2] <= status.bounds[0]
        || status.bounds[3] <= status.bounds[1]
    {
        return Err(failure(
            FailureStage::NativeRootState,
            "fixture note close target disappeared, changed identity, or is no longer ready",
        ));
    }
    Ok(status.has_keyboard_focus)
}

fn observe_note_ownership<B: FixtureNoteCloseBackend>(
    backend: &mut B,
    root: &WindowSnapshot,
    deadline: Duration,
    attempt: &mut NoteCloseAttempt,
) -> Result<NoteCloseGuiResponse, CaseFailure> {
    note_deadline(backend, deadline, attempt)?;
    let previous = match &attempt.gui_ownership {
        NoteCloseMeasurement::Observed(response) => Some(response),
        _ => None,
    };
    let nonce = *attempt.gui_nonce.get_or_insert_with(|| {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        [
            nanos as u64,
            ((nanos >> 64) as u64) ^ u64::from(std::process::id()),
        ]
    });
    let request = NoteCloseGuiRequest {
        schema_version: 1,
        fixture: NoteCloseFixture::Q11,
        request_id: NEXT_QUERY_OBSERVATION_REQUEST_ID.fetch_add(1, Ordering::Relaxed),
        run_nonce: nonce,
        expected_hwnd: hwnd_id(root.hwnd),
        expected_pid: root.process_id,
        expected_generation: previous
            .and_then(|response| response.root.as_ref())
            .map(|root| root.generation),
        after_frame_ordinal: previous.map_or(0, |response| response.observed_frame_ordinal),
    };
    attempt.gui_request = NoteCloseMeasurement::Observed(request.clone());
    let response = backend
        .note_ownership(root, &request, deadline.saturating_sub(backend.now()))
        .inspect_err(|error| {
            attempt.gui_ownership = NoteCloseMeasurement::failed(&error.message);
            attempt.gui_observation_error = Some(NoteCloseError::new(&error.message));
        })?;
    attempt.gui_ownership = NoteCloseMeasurement::Observed(response.clone());
    validate_note_close_gui_response(&request, &response).inspect_err(|error| {
        attempt.gui_observation_error = Some(NoteCloseError::new(&error.message));
    })?;
    let client = backend.client_screen_bounds(root)?;
    let snapshot = response
        .snapshot
        .as_ref()
        .ok_or_else(|| query_window_error("missing note-close client basis".into()))?;
    if client[2].checked_sub(client[0]) != Some(snapshot.client_size[0])
        || client[3].checked_sub(client[1]) != Some(snapshot.client_size[1])
    {
        return Err(query_window_error(
            "note-close GUI client basis differs from the current native client".into(),
        ));
    }
    note_deadline(backend, deadline, attempt)?;
    Ok(response)
}

fn sole_fixture(response: &NoteCloseGuiResponse) -> Result<&NoteCloseGuiNote, CaseFailure> {
    response
        .snapshot
        .as_ref()
        .filter(|snapshot| snapshot.open_note_count == 1)
        .and_then(|snapshot| snapshot.sole_note.as_ref())
        .filter(|note| {
            note.fixture_slug
                && note.fixture_marker
                && note.slug_digest == query_cell_digest("radial-acceptance-q11")
        })
        .ok_or_else(|| {
            query_window_error(
                "normal fixture close requires the sole canonical Q11 NotePanel".into(),
            )
        })
}

fn fixture_absent(response: &NoteCloseGuiResponse) -> bool {
    response.snapshot.as_ref().is_some_and(|snapshot| {
        snapshot.open_note_count == 0
            || (snapshot.open_note_count == 1
                && snapshot.sole_note.as_ref().is_some_and(|note| {
                    // Missing content markers do not close an existing
                    // canonical note. Both identity facts must establish
                    // a different note before claiming fixture absence.
                    !note.fixture_slug
                        && note.slug_digest != query_cell_digest("radial-acceptance-q11")
                }))
    })
}

fn match_note_discard(
    root: &WindowSnapshot,
    response: &NoteCloseGuiResponse,
    observation: &NoteDiscardObservation,
) -> Result<(), CaseFailure> {
    let note = sole_fixture(response)?;
    let widget = note
        .rendered_discard
        .as_ref()
        .filter(|_| note.pending_discard)
        .ok_or_else(|| {
            query_window_error("sole fixture has no current rendered normal confirmation".into())
        })?;
    let snapshot = response
        .snapshot
        .as_ref()
        .ok_or_else(|| query_window_error("fixture snapshot missing".into()))?;
    let contains = |outer: [i32; 4], inner: [i32; 4]| {
        inner[2] > inner[0]
            && inner[3] > inner[1]
            && outer[0] <= inner[0]
            && outer[1] <= inner[1]
            && outer[2] >= inner[2]
            && outer[3] >= inner[3]
    };
    let client = observation.client_screen_bounds;
    let width = client[2].checked_sub(client[0]);
    let height = client[3].checked_sub(client[1]);
    let physical = widget
        .bounds
        .into_iter()
        .enumerate()
        .map(|(index, edge)| edge.checked_add(client[index % 2]))
        .collect::<Option<Vec<_>>>();
    if width != Some(snapshot.client_size[0])
        || height != Some(snapshot.client_size[1])
        || !note.pending_discard
        || widget.widget_id == 0
        || widget.owner_slug_digest != note.slug_digest
        || !widget.enabled
        || !widget.visible
        || !widget.fully_visible
        || !contains(
            [0, 0, snapshot.client_size[0], snapshot.client_size[1]],
            widget.clip,
        )
        || !contains(widget.clip, widget.bounds)
        || !observation.is_button
        || !observation.readiness.same_as_admitted
        || observation.readiness.process_id != root.process_id
        || !observation.readiness.enabled
        || observation.readiness.offscreen
        || physical.as_deref() != Some(observation.readiness.bounds.as_slice())
    {
        return Err(query_window_error(
            "current UIA confirmation does not match the fixture's rendered widget and client"
                .into(),
        ));
    }
    Ok(())
}

fn finish_note_discard<B: FixtureNoteCloseBackend>(
    backend: &mut B,
    expected: &WindowSnapshot,
    edit: &B::Editor,
    discard: &B::Discard,
    close_deadline: Duration,
    attempt: &mut NoteCloseAttempt,
) -> Result<(), CaseFailure> {
    attempt.enter(NoteClosePhase::DiscardAction, backend.now());
    let root = checked_note_root(backend, expected, attempt)?;
    let response = observe_note_ownership(backend, &root, close_deadline, attempt)?;
    sole_fixture(&response)?;
    let root = checked_note_root(backend, expected, attempt)?;
    // The current marker-matching editor owns this confirmation even when the
    // modal disables it. Revalidate that same editor before the one click.
    attempt.observation_count = attempt.observation_count.saturating_add(1);
    let current = backend
        .editor_observation(&root, edit, NoteEditorPurpose::DiscardConfirmation)
        .map_err(|error| {
            let message = error.into_message();
            attempt.current_editor = NoteCloseMeasurement::failed(&message);
            query_uia_error(message)
        })?;
    record_note_editor_observation(
        &root,
        current,
        NoteEditorPurpose::DiscardConfirmation,
        attempt,
    )?;
    note_deadline(backend, close_deadline, attempt)?;
    let (current_discard, current_button) = backend
        .current_discard(&root, discard)
        .map_err(|error| query_uia_error(error.into_message()))?;
    match_note_discard(&root, &response, &current_button)?;
    note_deadline(backend, close_deadline, attempt)?;
    backend
        .discard(&root, &current_discard)
        .inspect_err(|error| {
            attempt.discard_action = NoteCloseMeasurement::failed(&error.message);
        })?;
    attempt.discard_action = NoteCloseMeasurement::Observed(());
    // This is the existing separate completion bound for the normal discard
    // action. Focus and Escape used the original close bound, not another one.
    let deadline = backend.now() + UIA_TIMEOUT;
    attempt.discard_deadline_ms = Some(duration_millis(deadline));
    attempt.enter(NoteClosePhase::WaitingForDiscardClose, backend.now());
    loop {
        note_deadline(backend, deadline, attempt)?;
        let root = checked_note_root(backend, expected, attempt)?;
        match backend.find_editor(&root, NoteEditorPurpose::DiscardConfirmation) {
            Ok(editor) => {
                let prompt = backend
                    .find_discard(&root)
                    .map_err(|error| query_uia_error(error.into_message()))?;
                attempt.discard_prompt = NoteCloseMeasurement::Observed(prompt.is_some());
                // A modal prompt can disable/filter the editor. Its absence is
                // not note absence until that actual prompt is gone too.
                let absent = editor.is_none() && prompt.is_none();
                attempt.note_absent = NoteCloseMeasurement::Observed(absent);
                if absent {
                    return Ok(());
                }
            }
            Err(VisibleTextLookupError::TransientElementUnavailable(message)) => {
                attempt.marker_present = NoteCloseMeasurement::failed(&message);
            }
            Err(VisibleTextLookupError::Other(message)) => return Err(query_uia_error(message)),
        }
        backend.poll();
    }
}

fn close_fixture_note_with<B: FixtureNoteCloseBackend>(
    backend: &mut B,
    attempt: &mut NoteCloseAttempt,
) -> Result<(), CaseFailure> {
    let result = (|| {
        attempt.enter(NoteClosePhase::SettlingRoot, backend.now());
        let expected = backend.settled_root().inspect_err(|error| {
            attempt.current_root = NoteCloseMeasurement::failed(&error.message);
        })?;
        attempt.expected_root = Some(NoteCloseRoot::from(&expected));
        let root = checked_note_root(backend, &expected, attempt)?;
        let deadline = backend.now() + UIA_TIMEOUT;
        attempt.close_deadline_ms = Some(duration_millis(deadline));
        attempt.enter(NoteClosePhase::LocatingEditor, backend.now());
        let ownership = observe_note_ownership(backend, &root, deadline, attempt)?;
        // Mandatory cleanup may encounter a real pending prompt. Its button
        // alone does not establish that the note belongs to this fixture.
        if let Some(discard) = backend
            .find_discard(&root)
            .map_err(|error| query_uia_error(error.into_message()))?
        {
            attempt.discard_prompt = NoteCloseMeasurement::Observed(true);
            let Some(edit) = backend
                .find_editor(&root, NoteEditorPurpose::DiscardConfirmation)
                .map_err(|error| query_uia_error(error.into_message()))?
            else {
                if !fixture_absent(&ownership) {
                    return Err(query_window_error(
                        "fixture note is not actually absent but has no current UIA editor".into(),
                    ));
                }
                attempt.marker_present = NoteCloseMeasurement::Observed(false);
                attempt.note_absent = NoteCloseMeasurement::Observed(true);
                return Ok(());
            };
            attempt.note_absent = NoteCloseMeasurement::Observed(false);
            sole_fixture(&ownership)?;
            let admitted = backend
                .editor_observation(&root, &edit, NoteEditorPurpose::DiscardConfirmation)
                .map_err(|error| {
                    let message = error.into_message();
                    attempt.admitted_editor = NoteCloseMeasurement::failed(&message);
                    query_uia_error(message)
                })?;
            attempt.admitted_editor = NoteCloseMeasurement::Observed(admitted.readiness.clone());
            record_note_editor_observation(
                &root,
                admitted,
                NoteEditorPurpose::DiscardConfirmation,
                attempt,
            )?;
            note_deadline(backend, deadline, attempt)?;
            return finish_note_discard(backend, &expected, &edit, &discard, deadline, attempt);
        }
        attempt.discard_prompt = NoteCloseMeasurement::Observed(false);
        let Some(edit) = backend
            .find_editor(&root, NoteEditorPurpose::Focus)
            .map_err(|error| query_uia_error(error.into_message()))?
        else {
            if !fixture_absent(&ownership) {
                return Err(query_window_error(
                    "fixture note is not actually absent but has no current UIA editor".into(),
                ));
            }
            attempt.marker_present = NoteCloseMeasurement::Observed(false);
            attempt.note_absent = NoteCloseMeasurement::Observed(true);
            return Ok(());
        };
        attempt.marker_present = NoteCloseMeasurement::Observed(true);
        attempt.note_absent = NoteCloseMeasurement::Observed(false);
        sole_fixture(&ownership)?;
        let admitted = backend
            .editor_observation(&root, &edit, NoteEditorPurpose::Focus)
            .map_err(|error| {
                let message = error.into_message();
                attempt.admitted_editor = NoteCloseMeasurement::failed(&message);
                query_uia_error(message)
            })?;
        attempt.admitted_editor = NoteCloseMeasurement::Observed(admitted.readiness.clone());
        record_note_editor_observation(&root, admitted, NoteEditorPurpose::Focus, attempt)?;
        note_deadline(backend, deadline, attempt)?;
        attempt.enter(NoteClosePhase::NativeRootFocus, backend.now());
        backend.focus_root(&root).inspect_err(|error| {
            attempt.native_focus = NoteCloseMeasurement::failed(&error.message);
        })?;
        attempt.native_focus = NoteCloseMeasurement::Observed(());
        note_deadline(backend, deadline, attempt)?;
        attempt.enter(NoteClosePhase::SemanticFocusRequest, backend.now());
        backend.request_editor_focus(&edit).inspect_err(|error| {
            attempt.focus_request = NoteCloseMeasurement::failed(&error.message);
        })?;
        attempt.focus_request = NoteCloseMeasurement::Observed(());
        attempt.current_editor = NoteCloseMeasurement::Pending;
        attempt.enter(NoteClosePhase::WaitingForEditorFocus, backend.now());
        loop {
            note_deadline(backend, deadline, attempt)?;
            let root = checked_note_root(backend, &expected, attempt)?;
            if note_editor_ready(backend, &root, &edit, attempt)? {
                attempt.enter(NoteClosePhase::Ready, backend.now());
                break;
            }
            backend.poll();
        }
        attempt.enter(NoteClosePhase::Revalidating, backend.now());
        let root = checked_note_root(backend, &expected, attempt)?;
        let ownership = observe_note_ownership(backend, &root, deadline, attempt)?;
        sole_fixture(&ownership)?;
        let root = checked_note_root(backend, &expected, attempt)?;
        if !note_editor_ready(backend, &root, &edit, attempt)? {
            return Err(failure(
                FailureStage::NativeRootState,
                "fixture note close lost processed editor focus before Escape",
            ));
        }
        note_deadline(backend, deadline, attempt)?;
        attempt.enter(NoteClosePhase::EscapeInsertion, backend.now());
        attempt.escape_requested = true;
        let inserted = backend.escape(&root).inspect_err(|error| {
            attempt.escape_inserted = NoteCloseMeasurement::failed(&error.message);
        })?;
        attempt.escape_inserted = NoteCloseMeasurement::Observed(inserted);
        if inserted != 2 {
            return Err(failure(
                FailureStage::InputInjection,
                "fixture note Escape did not insert its one checked down/up pair",
            ));
        }
        attempt.enter(NoteClosePhase::WaitingForNormalClose, backend.now());
        loop {
            note_deadline(backend, deadline, attempt)?;
            let root = checked_note_root(backend, &expected, attempt)?;
            if let Some(discard) = backend
                .find_discard(&root)
                .map_err(|error| query_uia_error(error.into_message()))?
            {
                attempt.discard_prompt = NoteCloseMeasurement::Observed(true);
                return finish_note_discard(backend, &expected, &edit, &discard, deadline, attempt);
            }
            attempt.discard_prompt = NoteCloseMeasurement::Observed(false);
            match backend.find_editor(&root, NoteEditorPurpose::DiscardConfirmation) {
                Ok(editor) => {
                    let absent = editor.is_none();
                    attempt.note_absent = NoteCloseMeasurement::Observed(absent);
                    if absent {
                        return Ok(());
                    }
                }
                Err(VisibleTextLookupError::TransientElementUnavailable(message)) => {
                    attempt.marker_present = NoteCloseMeasurement::failed(&message);
                }
                Err(VisibleTextLookupError::Other(message)) => {
                    return Err(query_uia_error(message));
                }
            }
            backend.poll();
        }
    })();
    match &result {
        Ok(()) => {
            attempt.outcome = NoteCloseMeasurement::Observed(());
            attempt.enter(NoteClosePhase::Complete, backend.now());
        }
        Err(error) => {
            attempt.failed_phase = Some(attempt.phase);
            attempt.outcome = NoteCloseMeasurement::failed(&error.message);
            attempt.enter(NoteClosePhase::Failed, backend.now());
        }
    }
    result
}

fn close_fixture_note(
    child: &NativeChild,
    ui: &UiAutomation,
    trace_path: &Path,
    attempt: &mut NoteCloseAttempt,
) -> Result<(), CaseFailure> {
    close_fixture_note_with(
        &mut NativeFixtureNoteClose {
            child,
            ui,
            trace_path,
            started: Instant::now(),
        },
        attempt,
    )
}

fn placement_toggle(
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    hotkey: AcceptanceHotkey,
    displays: &[[i32; 4]],
    visible: bool,
) -> Result<QueryPlacementToggleEvidence, CaseFailure> {
    let input = ensure_query_hotkey_root_visibility(child, anchor, trace_path, hotkey, visible)?;
    child
        .verify_acceptance_hotkey_released(hotkey)
        .map_err(query_uia_error)?;
    let tap = input.tap.as_ref().ok_or_else(|| {
        failure(
            FailureStage::GestureDecision,
            "L08 omitted its required explicit configured gesture",
        )
    })?;
    let deadline = Instant::now() + ROOT_TIMEOUT;
    loop {
        let lines = trace_lines(trace_path);
        let intent = lines
            .get(tap.visibility_event_ordinal.saturating_sub(1))
            .ok_or_else(|| {
                failure(
                    FailureStage::GestureDecision,
                    "L08 visibility receipt was evicted from the bounded trace",
                )
            })?;
        let revision = trace_field_value(intent, "revision")
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|revision| *revision > 0)
            .ok_or_else(|| {
                failure(
                    FailureStage::GestureDecision,
                    "L08 configured decision omitted its actual revision",
                )
            })?;
        let events = lines
            .iter()
            .enumerate()
            .skip(tap.visibility_event_ordinal)
            .filter_map(|(index, line)| {
                parse_hotkey_candidate_event(
                    line,
                    HotkeyCandidateStream::MainCandidate,
                    1,
                    HotkeyRunnerInputPurpose::LauncherChord,
                    index + 1,
                )
            })
            .collect::<Vec<_>>();
        for snapshot in events.iter().filter(|event| {
            event.kind == HotkeyTraceEventKind::NativeWindowSnapshot
                && event.visibility_revision == Some(revision)
                && event.invocation_id == Some(tap.invocation_id)
        }) {
            let Some(command) = events.iter().find(|event| {
                event.kind == HotkeyTraceEventKind::RootCommand
                    && event.visibility_revision == Some(revision)
                    && event.invocation_id == Some(tap.invocation_id)
                    && event.request_id == snapshot.request_id
                    && event.event_ordinal < snapshot.event_ordinal
            }) else {
                continue;
            };
            let current = child.refresh_root().map_err(query_window_error)?;
            let measured = presentation(&current, displays);
            let evidence = QueryPlacementToggleEvidence {
                input: input.clone(),
                visibility_revision: revision,
                command: command.clone(),
                snapshot: snapshot.clone(),
                presentation: measured,
            };
            if super::super::super::query_placement_toggle_is_valid(
                &evidence,
                hotkey,
                displays,
                hwnd_id(child.root().hwnd),
                child.process_id(),
                visible,
            ) {
                return Ok(evidence);
            }
        }
        if Instant::now() >= deadline {
            return Err(failure(
                FailureStage::NativeRootState,
                "L08 explicit gesture lacked a correlated native ROOT presentation receipt",
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

pub(super) fn run_l08_placement_case(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    anchor: &FocusAnchor,
    ui: &UiAutomation,
    trace_path: &Path,
    output: &Path,
    marker_path: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) {
    let diagnostic_identity = PrivateCaseDiagnosticIdentity::from_report(report);
    run_query_case_with_diagnostics(
        report,
        "L08",
        child,
        anchor,
        ui,
        trace_path,
        output,
        marker_path,
        hotkey,
        hold_threshold_ms,
        || {
            let mut operation = L08OperationDiagnostic::default();
            let result = run_l08_operation_with(
                &mut operation,
                || {
                    ensure_query_hotkey_root_visibility(child, anchor, trace_path, hotkey, true)
                        .map(|_| ())
                },
                |attempt| close_fixture_note(child, ui, trace_path, attempt),
                |operation| {
                    operation.phase = L08OperationPhase::ConfiguredBaseline;
                    let configured = settled_root(child, true)?;
                    operation.configured_root = Some(NoteCloseRoot::from(&configured));
                    let displays = native_display_bounds().map_err(query_window_error)?;
                    let dpi = child
                        .owned_window_dpi(&configured)
                        .map_err(query_window_error)?;
                    let configured_client_bounds = child
                        .client_bounds(&configured)
                        .map_err(query_window_error)?;
                    let target = moved_root_position(configured.bounds, &displays)
                        .map_err(query_window_error)?;
                    operation.phase = L08OperationPhase::MoveRoot;
                    operation.move_requested = Some(target);
                    let moved = child
                        .move_owned_root(&configured, target)
                        .map_err(query_window_error)?;
                    operation.moved_root = Some(NoteCloseRoot::from(&moved));
                    let moved_root = presentation(&settled_root(child, true)?, &displays);
                    if moved_root.bounds != moved.bounds
                        || child.owned_window_dpi(&moved).map_err(query_window_error)? != dpi
                    {
                        return Err(failure(
                            FailureStage::NativeRootState,
                            "L08 real move changed DPI or failed to settle at the requested geometry",
                        ));
                    }
                    let moved_client_bounds =
                        child.client_bounds(&moved).map_err(query_window_error)?;
                    operation.phase = L08OperationPhase::DispatchNewNote;
                    operation.note_query_requested = true;
                    let invocation = run_query_invocation(
                        child,
                        anchor,
                        ui,
                        trace_path,
                        output,
                        marker_path,
                        hotkey,
                        hold_threshold_ms,
                        "qa-note-new",
                        true,
                        Some(true),
                        |child, ui, _events| {
                            wait_note_editor(child, ui, "# radial acceptance q11", output)?;
                            Ok(QueryEffectResult {
                                effect_count: 1,
                                cancelled_confirmation_count: 0,
                                ui_ack: QueryEvidenceUiAck::NoteEditor,
                            })
                        },
                    )?;
                    operation.phase = L08OperationPhase::RestoreOnly;
                    let restored = settled_root(child, true)?;
                    operation.restored_root = Some(NoteCloseRoot::from(&restored));
                    let handoff_terminal_cursor = trace_lines(trace_path).len();
                    let restored_client_bounds =
                        child.client_bounds(&restored).map_err(query_window_error)?;
                    operation.phase = L08OperationPhase::ConfiguredHide;
                    let hide =
                        placement_toggle(child, anchor, trace_path, hotkey, &displays, false)?;
                    operation.hide_completed = true;
                    operation.phase = L08OperationPhase::ConfiguredShow;
                    let show =
                        placement_toggle(child, anchor, trace_path, hotkey, &displays, true)?;
                    operation.show_completed = true;
                    let shown_client_bounds = child
                        .client_bounds(&settled_root(child, true)?)
                        .map_err(query_window_error)?;
                    operation.phase = L08OperationPhase::PlacementMeasured;
                    Ok((
                        invocation,
                        QueryPlacementEvidence {
                            hotkey,
                            physical_displays: displays.clone(),
                            dpi,
                            configured_logical_position: QUERY_CONFIGURED_ROOT_POSITION,
                            configured_logical_client_size: QUERY_CONFIGURED_ROOT_CLIENT_SIZE,
                            configured_client_bounds,
                            configured_root: presentation(&configured, &displays),
                            moved_root,
                            moved_client_bounds,
                            handoff_terminal_cursor,
                            restored_root: presentation(&restored, &displays),
                            restored_client_bounds,
                            hide,
                            show,
                            shown_client_bounds,
                            note_absent_before: true,
                            note_closed_after: false,
                        },
                    ))
                },
            );
            let mut finished = finish_l08_with_cleanup(
                result,
                operation,
                |context| {
                    capture_l08_first_failure(
                        &diagnostic_identity,
                        context,
                        child,
                        ui,
                        trace_path,
                        output,
                    )
                },
                |attempt| {
                    ensure_query_hotkey_root_visibility(child, anchor, trace_path, hotkey, true)
                        .and_then(|_| close_fixture_note(child, ui, trace_path, attempt))
                },
            );
            if let Ok((_, proof)) = &mut finished.result {
                proof.note_closed_after = true;
            }
            finished
        },
    );
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum L08OperationPhase {
    #[default]
    NotAttempted,
    EnsureInitialRoot,
    InitialNoteClose,
    ConfiguredBaseline,
    MoveRoot,
    DispatchNewNote,
    RestoreOnly,
    ConfiguredHide,
    ConfiguredShow,
    PlacementMeasured,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct L08OperationDiagnostic {
    phase: L08OperationPhase,
    initial_note_close: NoteCloseAttempt,
    configured_root: Option<NoteCloseRoot>,
    move_requested: Option<[i32; 2]>,
    moved_root: Option<NoteCloseRoot>,
    note_query_requested: bool,
    restored_root: Option<NoteCloseRoot>,
    hide_completed: bool,
    show_completed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct L08CaseDiagnosticContext {
    original_operation: CaseOperationDiagnostic,
    operation: L08OperationDiagnostic,
    cleanup_note_close: NoteCloseAttempt,
    cleanup_result: Option<CaseOperationDiagnostic>,
}

impl L08CaseDiagnosticContext {
    pub(super) fn original_operation(&self) -> &CaseOperationDiagnostic {
        &self.original_operation
    }

    pub(super) fn bounded(&self) -> Self {
        let mut context = self.clone();
        context.original_operation.detail = bounded_text(&context.original_operation.detail, 512);
        context.operation.initial_note_close.bound();
        context.cleanup_note_close.bound();
        if let Some(cleanup) = &mut context.cleanup_result {
            cleanup.detail = bounded_text(&cleanup.detail, 512);
        }
        context
    }
}

fn run_l08_operation_with<R>(
    operation: &mut L08OperationDiagnostic,
    ensure_visible: impl FnOnce() -> Result<(), CaseFailure>,
    close_initial: impl FnOnce(&mut NoteCloseAttempt) -> Result<(), CaseFailure>,
    experiment: impl FnOnce(&mut L08OperationDiagnostic) -> Result<R, CaseFailure>,
) -> Result<R, CaseFailure> {
    operation.phase = L08OperationPhase::EnsureInitialRoot;
    ensure_visible()?;
    operation.phase = L08OperationPhase::InitialNoteClose;
    close_initial(&mut operation.initial_note_close)?;
    experiment(operation)
}

fn finish_l08_with_cleanup<R>(
    result: Result<R, CaseFailure>,
    operation: L08OperationDiagnostic,
    capture: impl FnOnce(&L08CaseDiagnosticContext) -> PrecleanupDiagnosticArtifacts,
    cleanup: impl FnOnce(&mut NoteCloseAttempt) -> Result<(), CaseFailure>,
) -> QueryCaseRun<R> {
    let original_operation = CaseOperationDiagnostic::from_result(
        &result
            .as_ref()
            .map(|_| "L08 placement operation completed".into())
            .map_err(Clone::clone),
    );
    let mut context = L08CaseDiagnosticContext {
        original_operation,
        operation,
        cleanup_note_close: NoteCloseAttempt::default(),
        cleanup_result: None,
    };
    // Capture the live operation state before teardown, including when the
    // eventual typed placement verdict is still awaiting cleanup/validation.
    // Only a failed final case attaches it; Some(empty) prevents substitution.
    let captured = capture(&context);
    let cleanup_result = cleanup(&mut context.cleanup_note_close);
    context.cleanup_result = Some(CaseOperationDiagnostic::from_result(
        &cleanup_result
            .as_ref()
            .map(|_| "normal fixture note cleanup completed".into())
            .map_err(Clone::clone),
    ));
    let mut result = match (result, cleanup_result) {
        (result, Ok(())) => result,
        (Ok(_), Err(error)) => Err(failure(FailureStage::Cleanup, error.message)),
        (Err(mut error), Err(cleanup)) => {
            error.message.push_str("; fixture note cleanup: ");
            error.message.push_str(&bounded_text(&cleanup.message, 512));
            Err(error)
        }
    };
    if let Err(error) = &mut result {
        if !captured.errors.is_empty() {
            error.message.push_str("; pre-note-cleanup capture: ");
            error
                .message
                .push_str(&bounded_text(&captured.errors.join("; "), 512));
        }
    }
    QueryCaseRun {
        result,
        diagnostics: Some(QueryCaseDiagnostics {
            l08: context,
            precleanup_artifacts: Some(captured.paths),
            capture_errors: captured.errors,
        }),
    }
}

fn priority_number<T: std::str::FromStr>(line: &str, field: &str) -> Result<T, String> {
    trace_field_value(line, field)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| format!("priority trace omitted or malformed {field}"))
}

fn priority_transition(line: &str) -> Result<HotkeyEdgeTransition, String> {
    match trace_field_value(line, "transition") {
        Some("Press") => Ok(HotkeyEdgeTransition::Press),
        Some("Release") => Ok(HotkeyEdgeTransition::Release),
        _ => Err("priority trace has an unknown primary transition".into()),
    }
}

fn priority_provenance(line: &str) -> Result<HotkeyInputProvenance, String> {
    match trace_field_value(line, "provenance") {
        Some("ExternalInjected") => Ok(HotkeyInputProvenance::ExternalInjected),
        Some("Physical") => Ok(HotkeyInputProvenance::Physical),
        Some("SelfInjected") => Ok(HotkeyInputProvenance::Owned),
        _ => Err("priority trace has an unknown input provenance".into()),
    }
}

fn parse_priority_primary(
    line: &str,
    ordinal: usize,
) -> Result<Option<PriorityHookPrimaryEvidence>, String> {
    if trace_field_value(line, "trace_event") != Some("hook_primary") {
        return Ok(None);
    }
    Ok(Some(PriorityHookPrimaryEvidence {
        event_ordinal: u32::try_from(ordinal).map_err(|_| "priority trace ordinal overflow")?,
        elapsed_ms: priority_number(line, "elapsed_ms")?,
        transition: priority_transition(line)?,
        provenance: priority_provenance(line)?,
        foreground_owner: match trace_field_value(line, "foreground_owner") {
            Some("Root") => PriorityForegroundOwner::Root,
            Some("PreviewInput") => PriorityForegroundOwner::PreviewInput,
            Some("PreviewVisual") => PriorityForegroundOwner::PreviewVisual,
            Some("Other") => PriorityForegroundOwner::Other,
            _ => return Err("priority trace has an unknown foreground owner".into()),
        },
    }))
}

fn parse_priority_admission(
    line: &str,
    ordinal: usize,
) -> Result<Option<PriorityHookAdmissionEvidence>, String> {
    if trace_field_value(line, "trace_event") != Some("hook_admission") {
        return Ok(None);
    }
    let boolean = |field: &str| {
        trace_field_value(line, field)
            .and_then(super::super::super::parse_trace_bool)
            .ok_or_else(|| format!("priority trace omitted or malformed {field}"))
    };
    Ok(Some(PriorityHookAdmissionEvidence {
        event_ordinal: u32::try_from(ordinal).map_err(|_| "priority trace ordinal overflow")?,
        elapsed_ms: priority_number(line, "elapsed_ms")?,
        transition: priority_transition(line)?,
        provenance: priority_provenance(line)?,
        owner: match trace_field_value(line, "owner") {
            Some("Launcher") => PriorityHookOwner::Launcher,
            Some("ScreenDrawRecovery") => PriorityHookOwner::ScreenDrawRecovery,
            Some("ExclusiveTool") => PriorityHookOwner::ExclusiveTool,
            _ => return Err("priority trace has an unknown admission owner".into()),
        },
        global_exclusive_owners: priority_number(line, "global_exclusive_owners")?,
        adapter_exclusive: boolean("adapter_exclusive")?,
        recovery: boolean("recovery")?,
        deadline_scheduled: boolean("deadline_scheduled")?,
        radial_intent: boolean("radial_intent")?,
    }))
}

fn parse_priority_configured_primary(
    line: &str,
    ordinal: usize,
) -> Result<Option<PriorityConfiguredPrimaryEvidence>, String> {
    if trace_field_value(line, "trace_event") != Some("configured_primary") {
        return Ok(None);
    }
    Ok(Some(PriorityConfiguredPrimaryEvidence {
        event_ordinal: u32::try_from(ordinal).map_err(|_| "priority trace ordinal overflow")?,
        elapsed_ms: priority_number(line, "elapsed_ms")?,
        transition: priority_transition(line)?,
        provenance: priority_provenance(line)?,
        modifiers_match: trace_field_value(line, "modifiers_match")
            .and_then(super::super::super::parse_trace_bool)
            .ok_or("priority configured primary omitted its modifier match")?,
        invocation_id: priority_number(line, "invocation_id")?,
        generation: priority_number(line, "generation")?,
    }))
}

struct PriorityInterval {
    primary_edges: Vec<PriorityHookPrimaryEvidence>,
    admissions: Vec<PriorityHookAdmissionEvidence>,
    configured_primary: Vec<PriorityConfiguredPrimaryEvidence>,
    candidate_events: Vec<HotkeyCandidateEventEvidence>,
    forbidden_event_count: usize,
}

fn validate_priority_recovery_event(
    line: &str,
    event: &HotkeyCandidateEventEvidence,
) -> Result<(), String> {
    use HotkeyTraceEventKind as Kind;

    // Other suites join partial trace records later. H15's closed recovery
    // interval requires a complete ROOT record before it contributes evidence.
    let complete = match event.kind {
        Kind::VisibilityIntent => event.visible.is_some() && event.visibility_source.is_some(),
        Kind::ScreenDrawRestoreFocusIntent => event.focus_intent.is_some(),
        Kind::RootCommand => {
            event.request_id.is_some()
                && event.terminal.is_some()
                && event
                    .command
                    .is_some_and(|command| command != HotkeyRootCommand::Other)
        }
        Kind::NativeWindowSnapshot => {
            event.request_id.is_some()
                && event.terminal.is_some()
                && event.visible.is_some()
                && event.minimized.is_some()
                && event.hwnd.is_some_and(|hwnd| hwnd != 0)
                && event.process_id.is_some_and(|process_id| process_id != 0)
                && event
                    .bounds
                    .is_some_and(|[left, top, right, bottom]| right > left && bottom > top)
        }
        Kind::NativeActivation => {
            event.request_id.is_some()
                && event.terminal.is_some()
                && event.hwnd.is_some_and(|hwnd| hwnd != 0)
                && matches!(
                    event.activation_edge,
                    Some(
                        HotkeyActivationEdge::RestoreRequested
                            | HotkeyActivationEdge::RestoreCompleted
                            | HotkeyActivationEdge::RestoreFailed
                    )
                )
        }
        _ => return Ok(()),
    };
    if !complete || event.visibility_revision.is_none() {
        return Err("priority interval has a malformed ROOT/recovery event".into());
    }
    let invocation = match (event.kind, trace_field_value(line, "invocation_id")) {
        (Kind::VisibilityIntent | Kind::ScreenDrawRestoreFocusIntent, Some("none")) => None,
        (_, Some(_)) => {
            let id = priority_number::<u64>(line, "invocation_id")?;
            (id != 0).then_some(id)
        }
        (_, None) => return Err("priority recovery event omitted invocation correlation".into()),
    };
    if invocation != event.invocation_id {
        return Err("priority recovery event has malformed invocation correlation".into());
    }
    match event.command {
        Some(HotkeyRootCommand::Position) if event.kind == Kind::RootCommand => {
            priority_number::<i32>(line, "requested_x")?;
            priority_number::<i32>(line, "requested_y")?;
        }
        Some(HotkeyRootCommand::Size) if event.kind == Kind::RootCommand => {
            let width = priority_number::<i32>(line, "requested_width")?;
            let height = priority_number::<i32>(line, "requested_height")?;
            if width <= 0 || height <= 0 {
                return Err("priority recovery size command has invalid dimensions".into());
            }
        }
        _ => {}
    }
    Ok(())
}

fn priority_interval(
    lines: &[String],
    cursor: usize,
    end: usize,
) -> Result<PriorityInterval, String> {
    if cursor == 0 || cursor >= end || end > lines.len() {
        return Err("priority trace span is incomplete".into());
    }
    let mut result = PriorityInterval {
        primary_edges: Vec::new(),
        admissions: Vec::new(),
        configured_primary: Vec::new(),
        candidate_events: Vec::new(),
        forbidden_event_count: 0,
    };
    for (index, line) in lines.iter().enumerate().take(end).skip(cursor) {
        if let Some(primary) = parse_priority_primary(line, index + 1)? {
            result.primary_edges.push(primary);
        }
        if let Some(admission) = parse_priority_admission(line, index + 1)? {
            result.admissions.push(admission);
        }
        let kind = trace_field_value(line, "trace_event");
        if let Some(configured) = parse_priority_configured_primary(line, index + 1)? {
            if result.configured_primary.len() == 1 {
                return Err("priority interval has an extra configured primary receipt".into());
            }
            result.configured_primary.push(configured);
            continue;
        }
        if matches!(
            kind,
            Some(
                "short_tap"
                    | "runtime_preparation"
                    | "radial_dispatch_requested"
                    | "radial_query_dispatch"
                    | "universal_action_execution"
                    | "radial_action"
                    | "session_request"
            )
        ) || (kind == Some("hook_deadline")
            && trace_field_value(line, "edge") == Some("Scheduled"))
            || (kind == Some("desired_visibility")
                && trace_field_value(line, "source") != Some("ScreenDrawRestore"))
            || (kind == Some("native_visibility")
                && trace_field_value(line, "visible") == Some("true"))
        {
            result.forbidden_event_count += 1;
        }
        if let Some(event) = parse_hotkey_candidate_event(
            line,
            HotkeyCandidateStream::MainCandidate,
            1,
            HotkeyRunnerInputPurpose::LauncherChord,
            index + 1,
        ) {
            validate_priority_recovery_event(line, &event)?;
            result.candidate_events.push(event);
        } else if matches!(
            kind,
            Some(
                "desired_visibility"
                    | "root_command"
                    | "native_window_snapshot"
                    | "native_activation"
                    | "screen_draw_restore_focus_intent"
            )
        ) {
            return Err("priority interval has a malformed ROOT/recovery event".into());
        }
        if result.primary_edges.len() > 2
            || result.admissions.len() > 2
            || result.candidate_events.len() > 128
        {
            return Err("priority interval exceeds its bounded evidence capacity".into());
        }
    }
    Ok(result)
}

fn toolbar_field(line: &str, key: &str) -> Result<String, String> {
    let prefix = format!("{key}=");
    let mut values = split_trace_tokens(line)
        .into_iter()
        .filter_map(|token| token.strip_prefix(&prefix));
    let value = values
        .next()
        .ok_or_else(|| format!("Screen Draw observation omitted {key}"))?;
    if values.next().is_some() {
        return Err(format!("Screen Draw observation duplicated {key}"));
    }
    trace_static_enum_value(value)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Screen Draw observation malformed {key}"))
}

fn toolbar_number<T: std::str::FromStr>(line: &str, key: &str) -> Result<T, String> {
    toolbar_field(line, key)?
        .parse()
        .map_err(|_| format!("Screen Draw observation invalid numeric {key}"))
}

fn toolbar_bool(line: &str, key: &str) -> Result<bool, String> {
    match toolbar_field(line, key)?.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("Screen Draw observation invalid boolean {key}")),
    }
}

fn toolbar_record_is_candidate(line: &str) -> bool {
    split_trace_tokens(line).into_iter().any(|token| {
        token
            .strip_prefix("trace_event=")
            .is_some_and(|value| value.trim_matches('"') == "screen_draw_toolbar")
    })
}

fn parse_screen_draw_toolbar(
    line: &str,
    ordinal: usize,
) -> Result<Option<ScreenDrawToolbarReceipt>, String> {
    if !toolbar_record_is_candidate(line) {
        return Ok(None);
    }
    if toolbar_field(line, "trace_event")? != "screen_draw_toolbar" {
        return Err("Screen Draw observation event conflict".into());
    }
    let mode = |key| match toolbar_field(line, key)?.as_str() {
        "Drawing" => Ok(ScreenDrawToolbarMode::Drawing),
        "Ghost" => Ok(ScreenDrawToolbarMode::Ghost),
        "Finish" => Ok(ScreenDrawToolbarMode::Finish),
        "Other" => Ok(ScreenDrawToolbarMode::Other),
        _ => Err("Screen Draw observation invalid mode".to_string()),
    };
    let widget = |prefix: &str| -> Result<ScreenDrawToolbarWidget, String> {
        let field = |suffix: &str| format!("{prefix}_{suffix}");
        let rect = |part: &str| -> Result<[i32; 4], String> {
            let key = |edge: &str| {
                if part.is_empty() {
                    field(edge)
                } else {
                    field(&format!("{part}_{edge}"))
                }
            };
            Ok([
                toolbar_number(line, &key("left"))?,
                toolbar_number(line, &key("top"))?,
                toolbar_number(line, &key("right"))?,
                toolbar_number(line, &key("bottom"))?,
            ])
        };
        Ok(ScreenDrawToolbarWidget {
            target: match toolbar_field(line, &field("target"))?.as_str() {
                "StateLabel" => ScreenDrawToolbarTarget::StateLabel,
                "ResumeDrawing" => ScreenDrawToolbarTarget::ResumeDrawing,
                _ => return Err("Screen Draw observation invalid semantic target".into()),
            },
            role: match toolbar_field(line, &field("role"))?.as_str() {
                "Label" => ScreenDrawToolbarRole::Label,
                "Button" => ScreenDrawToolbarRole::Button,
                _ => return Err("Screen Draw observation invalid role".into()),
            },
            widget_id: toolbar_number(line, &field("id"))?,
            enabled: toolbar_bool(line, &field("enabled"))?,
            bounds: rect("")?,
            clip: rect("clip")?,
            visible_bounds: rect("visible")?,
            fully_visible: toolbar_bool(line, &field("fully_visible"))?,
        })
    };
    let resume_present = toolbar_bool(line, "sd_resume_present")?;
    let resume = widget("sd_resume")?;
    // Absence has one exact producer representation, not a hidden response.
    if !resume_present
        && (resume.target != ScreenDrawToolbarTarget::ResumeDrawing
            || resume.role != ScreenDrawToolbarRole::Button
            || resume.widget_id != 0
            || resume.enabled
            || resume.bounds != [0; 4]
            || resume.clip != [0; 4]
            || resume.visible_bounds != [0; 4]
            || resume.fully_visible)
    {
        return Err("Screen Draw absent Resume has conflicting response facts".into());
    }
    let invocation_present = toolbar_bool(line, "sd_owner_invocation_present")?;
    let invocation: u64 = toolbar_number(line, "sd_owner_invocation")?;
    if invocation_present == (invocation == 0) {
        return Err("Screen Draw owner invocation presence conflicts with its value".into());
    }
    let root_present = toolbar_bool(line, "sd_root_present")?;
    let root = ScreenDrawRootIdentity {
        hwnd: toolbar_number(line, "sd_root_hwnd")?,
        process_id: toolbar_number(line, "sd_root_pid")?,
        generation: toolbar_number(line, "sd_root_generation")?,
    };
    if (root_present && (root.hwnd == 0 || root.process_id == 0 || root.generation == 0))
        || (!root_present && (root.hwnd != 0 || root.process_id != 0 || root.generation != 0))
    {
        return Err("Screen Draw ROOT presence conflicts with its identity".into());
    }
    let parking_present = toolbar_bool(line, "sd_parking_present")?;
    let parking_hwnd = toolbar_number::<u64>(line, "sd_transaction_hwnd")?;
    let parking_generation = toolbar_number::<u64>(line, "sd_transaction_generation")?;
    let parking_cycle = toolbar_number::<u64>(line, "sd_transaction_cycle")?;
    let parking_state = match toolbar_field(line, "sd_transaction_state")?.as_str() {
        "Active" => Some(ScreenDrawParkingState::Active),
        "Committed" => Some(ScreenDrawParkingState::Committed),
        "Restored" => Some(ScreenDrawParkingState::Restored),
        "None" => None,
        _ => return Err("Screen Draw observation invalid parking state".into()),
    };
    let parking = if parking_present {
        if parking_hwnd == 0 || parking_generation == 0 || parking_cycle == 0 {
            return Err("Screen Draw parking identity is incomplete".into());
        }
        Some(ScreenDrawParkingObservation {
            hwnd: parking_hwnd,
            generation: parking_generation,
            cycle: parking_cycle,
            state: parking_state.ok_or("Screen Draw present parking has no state")?,
        })
    } else {
        if parking_hwnd != 0
            || parking_generation != 0
            || parking_cycle != 0
            || parking_state.is_some()
        {
            return Err("Screen Draw absent parking has conflicting identity".into());
        }
        None
    };
    Ok(Some(ScreenDrawToolbarReceipt {
        event_ordinal: u32::try_from(ordinal)
            .map_err(|_| "Screen Draw observation ordinal overflow")?,
        trace_sequence: toolbar_number(line, "trace_sequence")?,
        elapsed_ms: toolbar_number(line, "elapsed_ms")?,
        observation: ScreenDrawToolbarObservation {
            hwnd: toolbar_number(line, "sd_hwnd")?,
            process_id: toolbar_number(line, "sd_pid")?,
            generation: toolbar_number(line, "sd_generation")?,
            lifetime: toolbar_number(line, "sd_lifetime")?,
            frame_nr: toolbar_number(line, "sd_frame")?,
            state: mode("sd_state")?,
            runtime_mode: mode("sd_runtime")?,
            client_size: [
                toolbar_number(line, "sd_client_width")?,
                toolbar_number(line, "sd_client_height")?,
            ],
            launcher: ScreenDrawLauncherObservation {
                visibility_revision: toolbar_number(line, "sd_owner_revision")?,
                invocation_id: invocation_present.then_some(invocation),
                focus_intent: match toolbar_field(line, "sd_owner_focus")?.as_str() {
                    "ActivateRoot" => multi_launcher::visibility::RootFocusIntent::ActivateRoot,
                    "PreserveForeground" => {
                        multi_launcher::visibility::RootFocusIntent::PreserveForeground
                    }
                    _ => return Err("Screen Draw observation invalid launcher focus intent".into()),
                },
                visible: toolbar_bool(line, "sd_owner_visible")?,
                root: root_present.then_some(root),
                parking,
            },
            label: widget("sd_label")?,
            resume: resume_present.then_some(resume),
        },
    }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum ToolbarObservationPhase {
    Drawing,
    Recovery,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolbarObservationDiagnostic {
    phase: Option<ToolbarObservationPhase>,
    boundary_ordinal: usize,
    last_trace_ordinal: usize,
    last_admissible: Option<ScreenDrawToolbarReceipt>,
    rejected: Option<String>,
}

fn toolbar_observation_boundary(
    lines: &[String],
    cursor: usize,
    phase: ToolbarObservationPhase,
) -> Result<Option<(u32, u64)>, String> {
    let mut boundary = None;
    for (index, line) in lines.iter().enumerate().skip(cursor) {
        let candidate = match phase {
            ToolbarObservationPhase::Drawing => {
                if trace_field_value(line, "trace_event") == Some("root_result_pointer")
                    && trace_field_value(line, "clicked") == Some("true")
                    && trace_field_value(line, "pointer_released") == Some("true")
                {
                    Some((
                        u32::try_from(index + 1).map_err(|_| "entry ordinal overflow")?,
                        toolbar_number(line, "trace_sequence")?,
                    ))
                } else {
                    None
                }
            }
            ToolbarObservationPhase::Recovery => parse_priority_admission(line, index + 1)?
                .filter(|admission| {
                    admission.transition == HotkeyEdgeTransition::Press
                        && admission.owner == PriorityHookOwner::ScreenDrawRecovery
                        && admission.recovery
                        && !admission.deadline_scheduled
                        && !admission.radial_intent
                })
                .map(|admission| (admission.event_ordinal, 0)),
        };
        if let Some(candidate) = candidate {
            if boundary.replace(candidate).is_some() {
                return Err("toolbar readiness boundary is not unique".into());
            }
        }
    }
    Ok(boundary)
}

fn admit_toolbar_observation_from_trace(
    lines: &[String],
    boundary: (u32, u64),
    mode: ScreenDrawToolbarMode,
    tool: &QueryRootPresentationEvidence,
    client: [i32; 4],
    displays: &[[i32; 4]],
    prior: Option<&ScreenDrawToolbarReceipt>,
) -> Result<ScreenDrawToolbarReceipt, String> {
    if lines
        .iter()
        .any(|line| trace_field_value(line, "trace_event") == Some("budget_exhausted"))
    {
        return Err("toolbar observation exhausted the ordinary trace budget".into());
    }
    let (index, line) = lines
        .iter()
        .enumerate()
        .skip(boundary.0 as usize)
        .rev()
        .find(|(_, line)| toolbar_record_is_candidate(line))
        .ok_or("no actual toolbar frame follows the boundary")?;
    let receipt = parse_screen_draw_toolbar(line, index + 1)?
        .ok_or("selected frame is not a toolbar observation")?;
    for (prior_index, prior_line) in lines
        .iter()
        .enumerate()
        .skip(boundary.0 as usize)
        .take(index - boundary.0 as usize)
    {
        if let Some(previous) = parse_screen_draw_toolbar(prior_line, prior_index + 1)? {
            let frame = &receipt.observation;
            let other = &previous.observation;
            if (
                other.hwnd,
                other.process_id,
                other.generation,
                other.lifetime,
                other.frame_nr,
            ) == (
                frame.hwnd,
                frame.process_id,
                frame.generation,
                frame.lifetime,
                frame.frame_nr,
            ) {
                return Err("toolbar observation frame publication is ambiguous".into());
            }
        }
    }
    validate_screen_draw_toolbar_receipt(
        &receipt,
        mode,
        tool,
        client,
        displays,
        boundary.0,
        u32::try_from(lines.len()).map_err(|_| "toolbar trace ordinal overflow")?,
        prior,
    )?;
    if receipt.trace_sequence <= boundary.1 {
        return Err("toolbar receipt precedes entry publication".into());
    }
    Ok(receipt)
}

fn wait_screen_draw_observation(
    child: &NativeChild,
    trace_path: &Path,
    cursor: usize,
    phase: ToolbarObservationPhase,
    prior: Option<&ScreenDrawToolbarReceipt>,
    displays: &[[i32; 4]],
    timeout: Duration,
    diagnostic: &mut ToolbarObservationDiagnostic,
) -> Result<(WindowSnapshot, [i32; 4], ScreenDrawToolbarReceipt), CaseFailure> {
    diagnostic.phase = Some(phase);
    diagnostic.boundary_ordinal = cursor;
    diagnostic.last_admissible = prior.cloned();
    diagnostic.rejected = None;
    let deadline = Instant::now() + timeout;
    loop {
        let lines = trace_lines(trace_path);
        diagnostic.last_trace_ordinal = lines.len();
        if lines
            .iter()
            .any(|line| trace_field_value(line, "trace_event") == Some("budget_exhausted"))
        {
            return Err(failure(
                FailureStage::NativeRootState,
                "H15 toolbar observation exhausted the ordinary trace budget",
            ));
        }
        if let Some((boundary, sequence)) =
            toolbar_observation_boundary(&lines, cursor, phase).map_err(query_uia_error)?
        {
            diagnostic.boundary_ordinal = boundary as usize;
            if let Some(tool) = screen_draw_toolbar(child).map_err(query_window_error)? {
                let client = child.client_bounds(&tool).map_err(query_window_error)?;
                let mode = match phase {
                    ToolbarObservationPhase::Drawing => ScreenDrawToolbarMode::Drawing,
                    ToolbarObservationPhase::Recovery => ScreenDrawToolbarMode::Ghost,
                };
                match admit_toolbar_observation_from_trace(
                    &lines,
                    (boundary, sequence),
                    mode,
                    &presentation(&tool, displays),
                    client,
                    displays,
                    prior,
                ) {
                    Ok(receipt) => {
                        diagnostic.last_admissible = Some(receipt.clone());
                        return Ok((tool, client, receipt));
                    }
                    Err(error) => diagnostic.rejected = Some(bounded_text(&error, 256)),
                }
            } else {
                diagnostic.rejected = Some("no current owned toolbar exists".into());
            }
        } else {
            diagnostic.rejected = Some("entry/recovery boundary has not been published".into());
        }
        if Instant::now() >= deadline {
            return Err(failure(
                FailureStage::NativeRootState,
                format!(
                    "H15 {phase:?} toolbar observation failed: {}",
                    diagnostic
                        .rejected
                        .as_deref()
                        .unwrap_or("no admissible actual frame")
                ),
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn admit_prechord_toolbar_baseline(
    lines: &[String],
    entry_cursor: usize,
    entry: (u32, u64),
    ready: &ScreenDrawToolbarReceipt,
    tool: &QueryRootPresentationEvidence,
    client: [i32; 4],
    root_hwnd: u64,
    process_id: u32,
    displays: &[[i32; 4]],
) -> Result<ScreenDrawToolbarReceipt, String> {
    if toolbar_observation_boundary(lines, entry_cursor, ToolbarObservationPhase::Drawing)?
        != Some(entry)
    {
        return Err("H15 current pre-chord frame has no unique owned entry boundary".into());
    }
    let baseline = admit_toolbar_observation_from_trace(
        lines,
        entry,
        ScreenDrawToolbarMode::Drawing,
        tool,
        client,
        displays,
        None,
    )?;
    validate_screen_draw_baseline_receipt(
        &baseline,
        ready,
        tool,
        client,
        root_hwnd,
        process_id,
        displays,
        entry.0,
        entry.1,
        u32::try_from(lines.len()).map_err(|_| "pre-chord trace ordinal overflow")?,
    )?;
    Ok(baseline)
}

fn screen_draw_toolbar(child: &NativeChild) -> Result<Option<WindowSnapshot>, String> {
    let mut tools = child.windows().into_iter().filter(|window| {
        window.process_id == child.process_id()
            && window.role == WindowRole::OtherChild
            && window_title(window.hwnd) == PRIORITY_SMOKE_TOOL_TITLE
    });
    let tool = tools.next();
    if tools.next().is_some() {
        return Err("Screen Draw published multiple owned toolbar HWNDs".into());
    }
    Ok(tool)
}

fn close_screen_draw_tool(child: &NativeChild) -> Result<(), CaseFailure> {
    if let Some(tool) = screen_draw_toolbar(child).map_err(query_window_error)? {
        request_window_close(child, &tool).map_err(query_window_error)?;
        let deadline = Instant::now() + ROOT_TIMEOUT;
        loop {
            if screen_draw_toolbar(child)
                .map_err(query_window_error)?
                .is_none()
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err(failure(
                    FailureStage::Cleanup,
                    "Screen Draw toolbar remained after its normal owned Close request",
                ));
            }
            std::thread::sleep(WINDOW_POLL);
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum H15AttemptPhase {
    ToolEntry,
    ObserverStart,
    ObserverPreflight,
    ChordSend,
    ChordWait,
    ChordValidation,
    RecoveryWait,
    Close,
    NextGesture,
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct H15OperationDiagnostic {
    checked_at_us: Option<u64>,
    succeeded: bool,
    error: Option<String>,
    error_chars: Option<usize>,
}

impl H15OperationDiagnostic {
    fn measured(epoch: Instant, result: &Result<(), String>) -> Self {
        Self {
            checked_at_us: h15_relative_us(epoch),
            succeeded: result.is_ok(),
            error: result.as_ref().err().map(|error| bounded_text(error, 256)),
            error_chars: result.as_ref().err().map(|error| error.chars().count()),
        }
    }
}

fn h15_relative_us(epoch: Instant) -> Option<u64> {
    Instant::now()
        .checked_duration_since(epoch)
        .and_then(|duration| u64::try_from(duration.as_micros()).ok())
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct H15ChordPredicates {
    exact_injected_pairs: bool,
    exact_injected_sequence: bool,
    no_foreign_edges: bool,
    exact_configured_key_set: bool,
    checked_at_us: Option<u64>,
}

impl H15ChordPredicates {
    fn passed(&self) -> bool {
        self.exact_injected_pairs
            && self.exact_injected_sequence
            && self.no_foreign_edges
            && self.exact_configured_key_set
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct H15NativeChordDiagnostic {
    down: Option<NativeInputDiagnostic>,
    up: Option<NativeInputDiagnostic>,
    observed_vks: Option<Vec<u32>>,
    total_observed_vks: Option<usize>,
    send: Option<H15OperationDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct H15AttemptDiagnostic {
    phase: H15AttemptPhase,
    expected_edges: Vec<(u32, bool)>,
    expected_vks: Vec<u32>,
    observer_start: Option<H15OperationDiagnostic>,
    pump_roundtrip: Option<H15OperationDiagnostic>,
    quiet_ms: Option<u64>,
    quiet_timestamp_overflow: bool,
    quiet_matching_edges: Option<usize>,
    baseline_trace_cursor: Option<usize>,
    baseline_revision: Option<u64>,
    baseline_invocation: Option<u64>,
    baseline_observation: Option<ScreenDrawToolbarReceipt>,
    baseline_established: bool,
    input: Option<H15NativeChordDiagnostic>,
    observation: Option<ChordObservationDiagnostic>,
    observation_checked_at_us: Option<u64>,
    pending_after_stop: Option<PendingChordDiagnostic>,
    pending_after_stop_checked_at_us: Option<u64>,
    wait_started_at_us: Option<u64>,
    wait_finished_at_us: Option<u64>,
    predicates: Option<H15ChordPredicates>,
    key_release_check: Option<H15OperationDiagnostic>,
    observer_stop: Option<H15OperationDiagnostic>,
    primary_stage: Option<FailureStage>,
}

impl H15AttemptDiagnostic {
    fn new(hotkey: AcceptanceHotkey) -> Self {
        Self {
            phase: H15AttemptPhase::ToolEntry,
            expected_edges: expected_hotkey_edges(hotkey, 1),
            expected_vks: hotkey_observer_keys(hotkey),
            observer_start: None,
            pump_roundtrip: None,
            quiet_ms: None,
            quiet_timestamp_overflow: false,
            quiet_matching_edges: None,
            baseline_trace_cursor: None,
            baseline_revision: None,
            baseline_invocation: None,
            baseline_observation: None,
            baseline_established: false,
            input: None,
            observation: None,
            observation_checked_at_us: None,
            pending_after_stop: None,
            pending_after_stop_checked_at_us: None,
            wait_started_at_us: None,
            wait_finished_at_us: None,
            predicates: None,
            key_release_check: None,
            observer_stop: None,
            primary_stage: None,
        }
    }
}

fn send_h15_chord_attempt(
    epoch: Instant,
    attempt: &mut H15AttemptDiagnostic,
    send: impl FnOnce(
        &mut dyn FnMut(&NativeInputEdgeEvidence),
        &mut dyn FnMut(&NativeInputEdgeEvidence),
    ) -> Result<AcceptanceHotkeyTapEvidence, String>,
) -> Result<AcceptanceHotkeyTapEvidence, CaseFailure> {
    attempt.phase = H15AttemptPhase::ChordSend;
    let native_diagnostic = std::cell::RefCell::new(H15NativeChordDiagnostic::default());
    let sent_input = send(
        &mut |edge| native_diagnostic.borrow_mut().down = Some(edge.diagnostic()),
        &mut |edge| native_diagnostic.borrow_mut().up = Some(edge.diagnostic()),
    );
    let mut native_diagnostic = native_diagnostic.into_inner();
    native_diagnostic.send = Some(H15OperationDiagnostic::measured(
        epoch,
        &sent_input.as_ref().map(|_| ()).map_err(Clone::clone),
    ));
    if let Ok(input) = &sent_input {
        native_diagnostic.observed_vks = Some(input.observed_vks.iter().copied().take(8).collect());
        native_diagnostic.total_observed_vks = Some(input.observed_vks.len());
    }
    attempt.input = Some(native_diagnostic);
    sent_input.map_err(query_uia_error)
}

fn validate_h15_chord_attempt(
    hotkey: AcceptanceHotkey,
    input: &AcceptanceHotkeyTapEvidence,
    observation: &RunnerChordObservation,
    epoch: Instant,
    attempt: &mut H15AttemptDiagnostic,
) -> Result<(), CaseFailure> {
    attempt.phase = H15AttemptPhase::ChordValidation;
    // Retain the actual measurement before any acceptance predicate rejects it.
    let input_diagnostic = attempt
        .input
        .get_or_insert_with(H15NativeChordDiagnostic::default);
    input_diagnostic.down = Some(input.down.diagnostic());
    input_diagnostic.up = Some(input.up.diagnostic());
    input_diagnostic.observed_vks = Some(input.observed_vks.iter().copied().take(8).collect());
    input_diagnostic.total_observed_vks = Some(input.observed_vks.len());
    attempt.observation = Some(observation.diagnostic(epoch));
    attempt.observation_checked_at_us = h15_relative_us(epoch);
    let predicates = H15ChordPredicates {
        exact_injected_pairs: observation.exact_injected_pairs(1),
        exact_injected_sequence: observation
            .exact_injected_sequence(&expected_hotkey_edges(hotkey, 1)),
        no_foreign_edges: observation.foreign_edges.is_empty(),
        exact_configured_key_set: input.observed_vks == hotkey_observer_keys(hotkey),
        checked_at_us: h15_relative_us(epoch),
    };
    let passed = predicates.passed();
    attempt.predicates = Some(predicates);
    if passed {
        Ok(())
    } else {
        attempt.primary_stage = Some(FailureStage::HookAdmission);
        Err(failure(
            FailureStage::HookAdmission,
            "H15 recovery input was not one exact owned configured chord",
        ))
    }
}

fn finish_h15_observer<T>(
    sent: Result<T, CaseFailure>,
    release: impl FnOnce() -> Result<(), String>,
    stop: impl FnOnce() -> Result<(), String>,
    epoch: Instant,
    attempt: &mut H15AttemptDiagnostic,
) -> Result<T, CaseFailure> {
    let released = release();
    attempt.key_release_check = Some(H15OperationDiagnostic::measured(epoch, &released));
    let stopped = stop();
    attempt.observer_stop = Some(H15OperationDiagnostic::measured(epoch, &stopped));
    let sent = match (sent, released) {
        (result, Ok(())) => result,
        (Ok(_), Err(error)) => Err(query_uia_error(error)),
        (Err(mut error), Err(release)) => {
            error.message.push_str(&format!(
                "; key release check: {}",
                bounded_text(&release, 256)
            ));
            Err(error)
        }
    };
    let result = match (sent, stopped) {
        (result, Ok(())) => result,
        (Ok(_), Err(error)) => Err(query_uia_error(error)),
        (Err(mut error), Err(stop)) => {
            error
                .message
                .push_str(&format!("; observer cleanup: {}", bounded_text(&stop, 256)));
            Err(error)
        }
    };
    if let Err(error) = &result {
        attempt.primary_stage = Some(error.stage);
    }
    result
}

#[derive(Default)]
struct H15PrecloseArtifacts {
    paths: Vec<PathBuf>,
    errors: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct H15CleanupDiagnostic {
    preclose_capture_attempted: bool,
    preclose_capture_errors: Vec<String>,
    close: Option<H15OperationDiagnostic>,
}

/// Measured context moves with the case into the one append/qualification
/// owner. It is diagnostic data, never an alternate accepted H15 proof.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct H15CaseDiagnosticContext {
    original_operation: CaseOperationDiagnostic,
    attempt: H15AttemptDiagnostic,
    toolbar: ToolbarObservationDiagnostic,
    cleanup: H15CleanupDiagnostic,
}

impl H15CaseDiagnosticContext {
    pub(super) fn original_operation(&self) -> &CaseOperationDiagnostic {
        &self.original_operation
    }
}

fn finish_h15_with_cleanup(
    result: Result<String, CaseFailure>,
    capture: impl FnOnce(&CaseFailure) -> H15PrecloseArtifacts,
    close: impl FnOnce() -> Result<(), CaseFailure>,
) -> (Result<String, CaseFailure>, Option<Vec<PathBuf>>) {
    let captured = result.as_ref().err().map(capture);
    // The current tool/native owner is still alive during capture; close is
    // unconditional, including when capture or serialization failed.
    let cleanup = close();
    let mut result = match (result, cleanup) {
        (result, Ok(())) => result,
        (Ok(_), Err(error)) => Err(failure(
            FailureStage::Cleanup,
            format!("Screen Draw cleanup: {}", error.message),
        )),
        (Err(error), Err(cleanup)) => Err(failure(
            error.stage,
            format!(
                "{}; Screen Draw cleanup: {}",
                error.message, cleanup.message
            ),
        )),
    };
    if let (Err(error), Some(captured)) = (&mut result, &captured) {
        if !captured.errors.is_empty() {
            error.message.push_str("; pre-close capture diagnostics: ");
            error
                .message
                .push_str(&bounded_text(&captured.errors.join("; "), 512));
        }
    }
    (result, captured.map(|captured| captured.paths))
}

fn capture_h15_preclose(
    child: &NativeChild,
    trace_path: &Path,
    output: &Path,
    diagnostic: &ToolbarObservationDiagnostic,
    attempt: &H15AttemptDiagnostic,
    primary: &CaseFailure,
) -> H15PrecloseArtifacts {
    let mut captured = H15PrecloseArtifacts::default();
    let tool = screen_draw_toolbar(child);
    let native_client = tool
        .as_ref()
        .ok()
        .and_then(|tool| tool.as_ref())
        .map(|tool| child.client_bounds(tool));
    let windows = child.windows();
    let inventory = h15_preclose_inventory(
        diagnostic,
        attempt,
        primary,
        child.process_id(),
        &tool,
        native_client.as_ref(),
        &windows,
    );
    persist_h15_preclose_piece(
        &mut captured,
        output,
        "observation.json",
        serde_json::to_vec_pretty(&inventory)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                if bytes.len() <= MAX_PRIVATE_LOG_BYTES {
                    Ok(bytes)
                } else {
                    Err("pre-close observation exceeds its bound".into())
                }
            }),
    );
    persist_h15_preclose_piece(
        &mut captured,
        output,
        "trace.log",
        fs::read_to_string(trace_path)
            .map(|trace| safe_trace_excerpt(&trace).into_bytes())
            .map_err(|error| error.to_string()),
    );
    persist_h15_preclose_piece(
        &mut captured,
        output,
        "private.log",
        read_bounded_log_tail(child.log_path()),
    );
    if let Ok(Some(tool)) = tool {
        let destination = output.join("case-H15-preclose-toolbar.png");
        match capture_window_screenshot(&tool, &destination) {
            Ok(()) => captured.paths.push(destination),
            Err(error) => captured.errors.push(bounded_text(
                &format!("pre-close owned toolbar crop: {error}"),
                256,
            )),
        }
    }
    captured
}

fn h15_preclose_inventory(
    diagnostic: &ToolbarObservationDiagnostic,
    attempt: &H15AttemptDiagnostic,
    primary: &CaseFailure,
    child_process_id: u32,
    tool: &Result<Option<WindowSnapshot>, String>,
    native_client: Option<&Result<[i32; 4], String>>,
    windows: &[WindowSnapshot],
) -> serde_json::Value {
    // No current tool is a real diagnostic state. Never substitute ROOT.
    serde_json::json!({
        "observation": diagnostic,
        "observation_role": "toolbar_polling_history",
        "attempted_chord": attempt,
        "failed_phase": attempt.phase,
        "primary_stage": primary.stage,
        "primary_failure": bounded_text(&primary.message,512),
        "tool": tool.as_ref().ok().and_then(|tool|tool.as_ref()).map(WindowRecord::from),
        "tool_discovery_error":tool.as_ref().err().map(|error|bounded_text(error,256)),
        "native_client":native_client.and_then(|client|client.as_ref().ok()),
        "native_client_error":native_client.and_then(|client|client.as_ref().err()).map(|error|bounded_text(error,256)),
        "child_process_id": child_process_id,
        "windows":windows.iter().take(32).map(WindowRecord::from).collect::<Vec<_>>(),
        "windows_truncated":windows.len()>32,
        "phase":"before-owned-close",
    })
}

fn persist_h15_preclose_piece(
    captured: &mut H15PrecloseArtifacts,
    output: &Path,
    suffix: &str,
    bytes: Result<Vec<u8>, String>,
) {
    match bytes {
        Ok(bytes) => {
            let limit = if suffix == "trace.log" {
                MAX_TRACE_BYTES
            } else {
                MAX_PRIVATE_LOG_BYTES
            };
            if !matches!(suffix, "observation.json" | "trace.log" | "private.log")
                || bytes.len() > limit
            {
                captured.errors.push(
                    "pre-close diagnostic has invalid path or exceeds its existing bound".into(),
                );
                return;
            }
            let destination = output.join(format!("case-H15-preclose-{suffix}"));
            match fs::write(&destination, bytes) {
                Ok(()) => captured.paths.push(destination),
                Err(error) => captured.errors.push(bounded_text(
                    &format!("save pre-close {suffix}: {error}"),
                    256,
                )),
            }
        }
        Err(error) => captured.errors.push(bounded_text(&error, 256)),
    }
}

pub(super) fn run_h15_priority_case(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hotkey: AcceptanceHotkey,
    hold_threshold_ms: u64,
) {
    let started = Instant::now();
    begin_hotkey_evidence_capture("H15", trace_path);
    let mut observation_diagnostic = ToolbarObservationDiagnostic::default();
    let mut attempt = H15AttemptDiagnostic::new(hotkey);
    let result = (|| {
        child
            .verify_acceptance_hotkey_released(hotkey)
            .map_err(query_uia_error)?;
        ensure_hotkey_root_visibility(child, anchor, hotkey, true)?;
        if child.designer().is_some()
            || screen_draw_toolbar(child)
                .map_err(query_window_error)?
                .is_some()
        {
            return Err(failure(
                FailureStage::WindowDiscovery,
                "H15 must start without an existing Designer or Screen Draw tool",
            ));
        }
        let ui = UiAutomation::new().map_err(query_uia_error)?;
        let displays = native_display_bounds().map_err(query_window_error)?;
        let root = settled_root(child, true)?;
        child.focus_window(&root).map_err(query_window_error)?;
        let edit = ui
            .find_first_edit(root.hwnd, child.process_id())
            .map_err(query_uia_error)?
            .ok_or_else(|| {
                failure(
                    FailureStage::NativeRootState,
                    "H15 ROOT omitted its normal query edit",
                )
            })?;
        replace_text(child, &root, &edit, &ui, PRIORITY_SMOKE_QUERY).map_err(query_uia_error)?;
        if !ui
            .wait_edit_value(&edit, PRIORITY_SMOKE_QUERY, UIA_TIMEOUT)
            .map_err(query_uia_error)?
        {
            return Err(failure(
                FailureStage::NativeRootState,
                "H15 query edit did not acknowledge the normal app-prefixed fixture query",
            ));
        }
        let control = ui
            .wait_named_containing(
                root.hwnd,
                child.process_id(),
                PRIORITY_SMOKE_ACTION_LABEL,
                UIA_TIMEOUT,
            )
            .map_err(query_uia_error)?;
        let entry_cursor = trace_lines(trace_path).len();
        let click =
            click_semantic_control(child, &root, &control, trace_path).map_err(query_uia_error)?;
        let (tool, ready_client_bounds, ready_observation) = wait_screen_draw_observation(
            child,
            trace_path,
            entry_cursor,
            ToolbarObservationPhase::Drawing,
            None,
            &displays,
            HOTKEY_FIXTURE_STARTUP_TIMEOUT,
            &mut observation_diagnostic,
        )?;
        let entries = trace_lines(trace_path);
        let clicks = entries
            .iter()
            .enumerate()
            .skip(entry_cursor)
            .filter(|(_, line)| {
                trace_field_value(line, "trace_event") == Some("root_result_pointer")
                    && trace_field_value(line, "clicked") == Some("true")
                    && trace_field_value(line, "pointer_released") == Some("true")
            })
            .map(|(index, _)| index + 1)
            .collect::<Vec<_>>();
        if clicks.len() != 1
            || root_is_physically_visible(
                child,
                &child.refresh_root().map_err(query_window_error)?,
            )?
        {
            return Err(failure(
                FailureStage::GestureDecision,
                "H15 did not prove one real normal result activation and capture-safe parked ROOT",
            ));
        }
        let entry_clicked_sequence =
            toolbar_number::<u64>(&entries[clicks[0] - 1], "trace_sequence")
                .map_err(query_uia_error)?;
        if !wait_query_trace_quiet(
            trace_path,
            entry_cursor,
            Duration::from_millis(250),
            Duration::from_secs(2),
        ) {
            return Err(failure(
                FailureStage::NativeRootState,
                "H15 tool entry did not settle its ROOT/effect trace",
            ));
        }
        child.focus_window(&tool).map_err(query_window_error)?;
        focus_is_validated(tool.hwnd, child.process_id()).map_err(query_uia_error)?;
        attempt.phase = H15AttemptPhase::ObserverStart;
        let observer_start = RunnerHookObserver::start();
        attempt.observer_start = Some(H15OperationDiagnostic::measured(
            started,
            &observer_start.as_ref().map(|_| ()).map_err(Clone::clone),
        ));
        let mut observer = observer_start.map_err(query_uia_error)?;
        let sent = (|| {
            attempt.phase = H15AttemptPhase::ObserverPreflight;
            let probe = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
            let roundtrip = observer.pump_roundtrip(probe, Duration::from_secs(1));
            attempt.pump_roundtrip = Some(H15OperationDiagnostic::measured(started, &roundtrip));
            roundtrip.map_err(query_uia_error)?;
            let quiet = observer
                .wait_for_key_quiet(
                    &hotkey_observer_keys(hotkey),
                    Duration::from_millis(75),
                    Duration::from_secs(1),
                )
                .map_err(query_uia_error)?;
            attempt.quiet_ms = u64::try_from(quiet.quiet_ms).ok();
            attempt.quiet_timestamp_overflow = attempt.quiet_ms.is_none();
            attempt.quiet_matching_edges = Some(quiet.matching_edges);
            let lines = trace_lines(trace_path);
            let cursor = lines.len();
            let baseline_tool = screen_draw_toolbar(child)
                .map_err(query_window_error)?
                .ok_or_else(|| {
                    failure(
                        FailureStage::WindowDiscovery,
                        "H15 pre-chord toolbar is absent",
                    )
                })?;
            let baseline_client = child
                .client_bounds(&baseline_tool)
                .map_err(query_window_error)?;
            let baseline_observation = admit_prechord_toolbar_baseline(
                &lines,
                entry_cursor,
                (
                    u32::try_from(clicks[0])
                        .map_err(|_| query_uia_error("entry ordinal overflow".into()))?,
                    entry_clicked_sequence,
                ),
                &ready_observation,
                &presentation(&baseline_tool, &displays),
                baseline_client,
                hwnd_id(root.hwnd),
                child.process_id(),
                &displays,
            )
            .map_err(query_uia_error)?;
            let current_root = child.refresh_root().map_err(query_window_error)?;
            if current_root.hwnd != root.hwnd
                || current_root.process_id != root.process_id
                || root_is_physically_visible(child, &current_root)?
            {
                return Err(failure(
                    FailureStage::NativeRootState,
                    "H15 current pre-chord ROOT is not capture-safe parked",
                ));
            }
            let revision = baseline_observation
                .observation
                .launcher
                .visibility_revision;
            let invocation = baseline_observation.observation.launcher.invocation_id;
            attempt.baseline_trace_cursor = Some(cursor);
            attempt.baseline_revision = Some(revision);
            attempt.baseline_invocation = invocation;
            attempt.baseline_observation = Some(baseline_observation.clone());
            attempt.baseline_established = true;
            let input = send_h15_chord_attempt(started, &mut attempt, |down, up| {
                child.send_acceptance_hotkey_observed(
                    tool.hwnd,
                    child.process_id(),
                    hotkey,
                    Duration::from_millis(25),
                    down,
                    up,
                )
            })?;
            attempt.phase = H15AttemptPhase::ChordWait;
            attempt.wait_started_at_us = h15_relative_us(started);
            let observation = observer.wait_for_chord_burst(
                &hotkey_observer_keys(hotkey),
                1,
                Duration::from_secs(3),
            );
            attempt.wait_finished_at_us = h15_relative_us(started);
            validate_h15_chord_attempt(hotkey, &input, &observation, started, &mut attempt)?;
            Ok((
                cursor,
                revision,
                invocation,
                baseline_observation,
                baseline_tool,
                baseline_client,
                quiet,
                input,
                observation,
            ))
        })();
        let finished = finish_h15_observer(
            sent,
            || child.verify_acceptance_hotkey_released(hotkey).map(|_| ()),
            || observer.stop_and_report(),
            started,
            &mut attempt,
        );
        if attempt.input.is_some() && attempt.observation.is_none() {
            attempt.pending_after_stop = observer
                .pending_chord_diagnostic_after_stop(&hotkey_observer_keys(hotkey), started);
            attempt.pending_after_stop_checked_at_us = h15_relative_us(started);
        }
        let (
            cursor,
            revision,
            invocation,
            baseline_observation,
            baseline_tool,
            baseline_client_bounds,
            quiet,
            input,
            observation,
        ) = finished?;
        attempt.phase = H15AttemptPhase::RecoveryWait;
        let (recovered_tool, recovered_client_bounds, recovered_observation) =
            wait_screen_draw_observation(
                child,
                trace_path,
                cursor,
                ToolbarObservationPhase::Recovery,
                Some(&baseline_observation),
                &displays,
                UIA_TIMEOUT,
                &mut observation_diagnostic,
            )?;
        let recovered_root = settled_root(child, true)?;
        let recovery_lines = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
            has_trace(events, "desired_visibility", &["source=ScreenDrawRestore"])
                && has_trace(events, "screen_draw_restore_focus_intent", &[])
                && has_trace(events, "native_window_snapshot", &[])
                && has_trace(events, "native_activation", &["edge=RestoreCompleted"])
        });
        if recovery_lines.is_empty() {
            return Err(failure(
                FailureStage::NativeRootState,
                "H15 recovery did not publish its real ROOT/native trace",
            ));
        }
        let lines = trace_lines(trace_path);
        let end = lines.len();
        let interval = priority_interval(&lines, cursor, end).map_err(query_uia_error)?;
        let restorations = build_hotkey_follow_on_restorations(&interval.candidate_events);
        if restorations.len() != 1 {
            return Err(failure(
                FailureStage::GestureDecision,
                "H15 recovery has no unique actual restoration owner",
            ));
        }
        validate_screen_draw_recovery_owner(
            &baseline_observation,
            &recovered_observation,
            restorations[0].visibility_revision,
            restorations[0].invocation_id,
            restorations[0].focus_intent,
        )
        .map_err(query_uia_error)?;
        let runtime_window_count = child
            .windows()
            .iter()
            .filter(|window| window.class_name == "MultiLauncherRadialHost" && window.visible)
            .count();
        attempt.phase = H15AttemptPhase::Close;
        close_screen_draw_tool(child)?;
        child
            .verify_acceptance_hotkey_released(hotkey)
            .map_err(query_uia_error)?;
        if !wait_query_trace_quiet(trace_path, end, Duration::from_millis(250), ROOT_TIMEOUT) {
            return Err(failure(
                FailureStage::Cleanup,
                "H15 normal tool close did not settle its ROOT/effect lifecycle",
            ));
        }
        settled_root(child, true)?;
        // The ordinary packet owns only the post-close admitted gesture. The
        // real suppressed gesture has its separate, positively owned proof.
        begin_hotkey_evidence_capture("H15", trace_path);
        let close_terminal_cursor = trace_lines(trace_path).len();
        attempt.phase = H15AttemptPhase::NextGesture;
        let next = run_hotkey_burst_attempt(
            child,
            anchor,
            trace_path,
            hotkey,
            1,
            true,
            hold_threshold_ms,
        )?;
        if next.final_visible {
            return Err(failure(
                FailureStage::GestureDecision,
                "H15 next normal gesture did not hide visible ROOT",
            ));
        }
        let after = trace_lines(trace_path);
        let next_admissions = after
            .iter()
            .enumerate()
            .skip(close_terminal_cursor)
            .map(|(index, line)| parse_priority_admission(line, index + 1))
            .collect::<Result<Vec<_>, _>>()
            .map_err(query_uia_error)?
            .into_iter()
            .flatten()
            .filter(|admission| admission.transition == HotkeyEdgeTransition::Press)
            .collect::<Vec<_>>();
        if next_admissions.len() != 1 {
            return Err(failure(
                FailureStage::HookAdmission,
                "H15 next cycle omitted its unique real launcher admission",
            ));
        }
        let priority = ScreenDrawPriorityEvidence {
            entry_root: presentation(&root, &displays),
            entry_query_digest: query_cell_digest(PRIORITY_SMOKE_QUERY),
            entry_label_digest: query_cell_digest(PRIORITY_SMOKE_ACTION_LABEL),
            entry_action_digest: query_cell_digest("screen_draw:start"),
            entry_control_bounds: control.bounds,
            entry_down_inserted: click.down.inserted,
            entry_up_inserted: click.up.inserted,
            entry_clicked_ordinal: u32::try_from(clicks[0]).map_err(|_| {
                failure(FailureStage::GestureDecision, "H15 entry ordinal overflow")
            })?,
            entry_clicked_sequence,
            tool_title_digest: query_cell_digest(PRIORITY_SMOKE_TOOL_TITLE),
            ready_tool: presentation(&tool, &displays),
            drawing_ui_ack: true,
            ready_observation,
            ready_client_bounds,
            baseline_observation,
            baseline_tool: presentation(&baseline_tool, &displays),
            baseline_client_bounds,
            trace_cursor: cursor,
            trace_end: end,
            baseline_visibility_revision: revision,
            baseline_invocation_id: invocation,
            quiet_preflight_ms: u64::try_from(quiet.quiet_ms).unwrap_or(u64::MAX),
            observer_default_desktop: observation.desktop == "Default",
            input_down_inserted: input.down.inserted,
            input_up_inserted: input.up.inserted,
            input_down_foreground_hwnd: input.down.foreground_hwnd,
            input_down_foreground_pid: input.down.foreground_pid,
            input_up_foreground_hwnd: input.up.foreground_hwnd,
            input_up_foreground_pid: input.up.foreground_pid,
            input_down_default_desktop: native_input_desktop_is_default(&input.down.input_desktop),
            input_up_default_desktop: native_input_desktop_is_default(&input.up.input_desktop),
            runner_edges: query_setup_observed_edges(&observation.ordered_edges),
            foreign_edges: query_setup_observed_edges(&observation.foreign_edges),
            primary_edges: interval.primary_edges,
            admissions: interval.admissions,
            configured_primary: interval.configured_primary,
            candidate_events: interval.candidate_events,
            restorations,
            forbidden_event_count: interval.forbidden_event_count,
            runtime_window_count,
            recovered_root: presentation(&recovered_root, &displays),
            recovered_tool: presentation(&recovered_tool, &displays),
            ghost_ui_ack: true,
            recovered_observation,
            recovered_client_bounds,
            keys_released: true,
            close_requested: true,
            tool_closed: true,
            close_terminal_cursor,
            next_admission: next_admissions[0].clone(),
        };
        ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| {
            if let Some(capture) = slot.borrow_mut().as_mut() {
                capture.screen_draw_priority = Some(priority);
            }
        });
        attempt.phase = H15AttemptPhase::Complete;
        Ok(format!(
            "evidence:v1; hotkey={}; {}",
            hotkey.as_str(),
            super::super::super::required_case_evidence("H15")
                .unwrap_or_default()
                .join("; ")
        ))
    })();
    if let Err(error) = &result {
        attempt.primary_stage = Some(error.stage);
    }
    let original_operation = CaseOperationDiagnostic::from_result(&result);
    let cleanup_diagnostic = RefCell::new(H15CleanupDiagnostic::default());
    let (result, precleanup_artifacts) = finish_h15_with_cleanup(
        result,
        |error| {
            let captured = capture_h15_preclose(
                child,
                trace_path,
                output,
                &observation_diagnostic,
                &attempt,
                error,
            );
            let mut diagnostic = cleanup_diagnostic.borrow_mut();
            diagnostic.preclose_capture_attempted = true;
            diagnostic.preclose_capture_errors = captured.errors.clone();
            captured
        },
        || {
            let closed = close_screen_draw_tool(child);
            cleanup_diagnostic.borrow_mut().close = Some(H15OperationDiagnostic::measured(
                started,
                &closed
                    .as_ref()
                    .map(|_| ())
                    .map_err(|error| error.message.clone()),
            ));
            closed
        },
    );
    append_case_with_diagnostic_context(
        report,
        "H15",
        expected("H15"),
        started,
        result,
        Some(child),
        output,
        trace_path,
        // Even a later validator rejection is after the owned close attempt.
        // Preserve the absence of a pre-close capture rather than substituting
        // a post-close ROOT crop for the disposed toolbar.
        Some(precleanup_artifacts.unwrap_or_default()),
        Some(H15CaseDiagnosticContext {
            original_operation,
            attempt,
            toolbar: observation_diagnostic,
            cleanup: cleanup_diagnostic.into_inner(),
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::super::tests::test_acceptance_report;
    use super::*;

    #[derive(Clone, Copy, Debug)]
    enum NoteReadinessFault {
        WrongProcess,
        ReplacedEditor,
        MissingMarker,
        Disabled,
        Offscreen,
        EmptyBounds,
        OtherEditorFocused,
        PropertyError,
        TransientProperty,
    }

    #[derive(Clone, Copy, Debug)]
    enum NoteOwnershipFault {
        CoexistentNote,
        WrongSlug,
        WrongDigest,
        MissingMarker,
        WrongNonce,
        WrongRequest,
        StaleFrame,
        WrongHwnd,
        WrongPid,
        WrongGeneration,
        Failed,
        MissingWidget,
        WrongWidgetOwner,
        DisabledWidget,
        HiddenWidget,
        ClippedWidget,
        WrongWidgetBounds,
        WrongClient,
        TransportError,
    }

    struct ScriptedNoteClose {
        root: WindowSnapshot,
        events: Vec<String>,
        clock: Duration,
        poll_step: Duration,
        polls: usize,
        focus_after_polls: usize,
        focus_requested: bool,
        root_focus_calls: usize,
        focus_calls: usize,
        editor_observations: usize,
        fault: Option<(usize, NoteReadinessFault)>,
        root_calls: usize,
        root_fault: Option<(usize, WindowSnapshot)>,
        root_error_at: Option<usize>,
        lookup_calls: usize,
        lookup_purposes: Vec<NoteEditorPurpose>,
        lookup_error_at: Option<usize>,
        root_focus_error: bool,
        focus_error: bool,
        foreground_override: Option<(u64, u32)>,
        escape_calls: usize,
        escape_error: bool,
        inserted: u32,
        native_inserted: Option<usize>,
        escaped_at: Option<usize>,
        close_after_polls: usize,
        present: bool,
        editor_disabled: bool,
        dirty: bool,
        prompt: bool,
        discard_calls: usize,
        discard_error: bool,
        discarded_at: Option<usize>,
        discard_after_polls: usize,
        ownership_calls: usize,
        ownership_delay: Duration,
        ownership_frame: u64,
        ownership_fault: Option<(usize, NoteOwnershipFault)>,
        ownership_snapshot: Option<NoteCloseGuiSnapshot>,
        client_override: Option<[i32; 4]>,
        button_fault: Option<NoteReadinessFault>,
        button_role_wrong: bool,
    }

    impl Default for ScriptedNoteClose {
        fn default() -> Self {
            Self {
                root: WindowSnapshot {
                    hwnd: HWND(42usize as *mut std::ffi::c_void),
                    process_id: 202,
                    role: WindowRole::Root,
                    class_name: "owned test ROOT".into(),
                    visible: true,
                    minimized: false,
                    bounds: [100, 200, 1000, 850],
                },
                events: Vec::new(),
                clock: Duration::ZERO,
                poll_step: WINDOW_POLL,
                polls: 0,
                focus_after_polls: 0,
                focus_requested: false,
                root_focus_calls: 0,
                focus_calls: 0,
                editor_observations: 0,
                fault: None,
                root_calls: 0,
                root_fault: None,
                root_error_at: None,
                lookup_calls: 0,
                lookup_purposes: Vec::new(),
                lookup_error_at: None,
                root_focus_error: false,
                focus_error: false,
                foreground_override: None,
                escape_calls: 0,
                escape_error: false,
                inserted: 2,
                native_inserted: None,
                escaped_at: None,
                close_after_polls: 2,
                present: true,
                editor_disabled: false,
                dirty: false,
                prompt: false,
                discard_calls: 0,
                discard_error: false,
                discarded_at: None,
                discard_after_polls: 2,
                ownership_calls: 0,
                ownership_delay: Duration::ZERO,
                ownership_frame: 0,
                ownership_fault: None,
                ownership_snapshot: None,
                client_override: None,
                button_fault: None,
                button_role_wrong: false,
            }
        }
    }

    impl FixtureNoteCloseBackend for ScriptedNoteClose {
        type Editor = u32;
        type Discard = u32;
        fn now(&self) -> Duration {
            self.clock
        }
        fn poll(&mut self) {
            self.events.push("poll".into());
            self.clock += self.poll_step;
            self.polls += 1;
            if let Some(at) = self.discarded_at {
                if self.polls - at >= self.discard_after_polls {
                    self.present = false;
                    self.prompt = false;
                }
            } else if let Some(at) = self.escaped_at {
                if self.polls - at >= self.close_after_polls {
                    if self.dirty {
                        self.prompt = true;
                    } else {
                        self.present = false;
                    }
                }
            }
        }
        fn settled_root(&mut self) -> Result<WindowSnapshot, CaseFailure> {
            self.events.push("settled_root:42:202".into());
            Ok(self.root.clone())
        }
        fn current_root(&mut self) -> Result<WindowSnapshot, CaseFailure> {
            self.root_calls += 1;
            self.events.push("current_root:42:202".into());
            if self.root_error_at == Some(self.root_calls) {
                return Err(query_window_error("ROOT disappeared".into()));
            }
            Ok(self
                .root_fault
                .as_ref()
                .filter(|(at, _)| *at == self.root_calls)
                .map_or_else(|| self.root.clone(), |(_, root)| root.clone()))
        }
        fn find_editor(
            &mut self,
            root: &WindowSnapshot,
            purpose: NoteEditorPurpose,
        ) -> Result<Option<u32>, VisibleTextLookupError> {
            assert_eq!(hwnd_id(root.hwnd), 42);
            assert_eq!(root.process_id, 202);
            self.lookup_calls += 1;
            self.lookup_purposes.push(purpose);
            self.events.push(format!("marker_present:{}", self.present));
            if self.lookup_error_at == Some(self.lookup_calls) {
                return Err(VisibleTextLookupError::Other(
                    "multiple owned matching editors".into(),
                ));
            }
            Ok((self.present
                && (purpose == NoteEditorPurpose::DiscardConfirmation || !self.editor_disabled))
                .then_some(7))
        }
        fn editor_observation(
            &mut self,
            root: &WindowSnapshot,
            admitted: &u32,
            _purpose: NoteEditorPurpose,
        ) -> Result<NoteEditorObservation, VisibleTextLookupError> {
            assert_eq!(*admitted, 7);
            assert_eq!(hwnd_id(root.hwnd), 42);
            self.editor_observations += 1;
            let mut observation = NoteEditorObservation {
                marker_present: self.present,
                readiness: SemanticControlReadiness {
                    process_id: 202,
                    bounds: [140, 300, 980, 800],
                    enabled: !self.editor_disabled,
                    offscreen: false,
                    has_keyboard_focus: self.focus_requested
                        && self.polls >= self.focus_after_polls,
                    same_as_admitted: true,
                },
            };
            if let Some((at, fault)) = self.fault {
                if at == self.editor_observations {
                    match fault {
                        NoteReadinessFault::WrongProcess => observation.readiness.process_id = 999,
                        NoteReadinessFault::ReplacedEditor => {
                            observation.readiness.same_as_admitted = false
                        }
                        NoteReadinessFault::MissingMarker => observation.marker_present = false,
                        NoteReadinessFault::Disabled => observation.readiness.enabled = false,
                        NoteReadinessFault::Offscreen => observation.readiness.offscreen = true,
                        NoteReadinessFault::EmptyBounds => {
                            observation.readiness.bounds[2] = observation.readiness.bounds[0]
                        }
                        NoteReadinessFault::OtherEditorFocused => {
                            observation.readiness.has_keyboard_focus = false
                        }
                        NoteReadinessFault::PropertyError => {
                            return Err(VisibleTextLookupError::Other(
                                "current focus property failed".into(),
                            ));
                        }
                        NoteReadinessFault::TransientProperty => {
                            return Err(VisibleTextLookupError::TransientElementUnavailable(
                                "current element temporarily unavailable".into(),
                            ));
                        }
                    }
                }
            }
            self.events.push(format!(
                "editor:7:focus={}",
                observation.readiness.has_keyboard_focus
            ));
            Ok(observation)
        }
        fn find_discard(
            &mut self,
            root: &WindowSnapshot,
        ) -> Result<Option<u32>, VisibleTextLookupError> {
            assert_eq!(hwnd_id(root.hwnd), 42);
            self.events.push(format!("discard_prompt:{}", self.prompt));
            Ok(self.prompt.then_some(9))
        }
        fn client_screen_bounds(&mut self, root: &WindowSnapshot) -> Result<[i32; 4], CaseFailure> {
            assert_eq!((hwnd_id(root.hwnd), root.process_id), (42, 202));
            Ok(self.client_override.unwrap_or(root.bounds))
        }
        fn note_ownership(
            &mut self,
            root: &WindowSnapshot,
            request: &NoteCloseGuiRequest,
            remaining: Duration,
        ) -> Result<NoteCloseGuiResponse, CaseFailure> {
            assert_eq!((hwnd_id(root.hwnd), root.process_id), (42, 202));
            assert!(remaining <= UIA_TIMEOUT);
            self.ownership_calls += 1;
            self.ownership_frame += 1;
            self.clock += self.ownership_delay;
            self.events
                .push(format!("completed_note_frame:{}", self.ownership_frame));
            let slug = if self.present {
                "radial-acceptance-q11"
            } else {
                "unrelated-note"
            };
            let count = usize::from(self.present || self.prompt);
            let mut response = NoteCloseGuiResponse {
                schema_version: 1,
                fixture: NoteCloseFixture::Q11,
                request_id: request.request_id,
                run_nonce: request.run_nonce,
                status: NoteCloseGuiStatus::Captured,
                error: None,
                observed_frame_ordinal: self.ownership_frame,
                root: Some(NoteCloseGuiRoot {
                    hwnd: 42,
                    process_id: 202,
                    generation: 3,
                }),
                snapshot: Some(NoteCloseGuiSnapshot {
                    client_size: [900, 650],
                    open_note_count: count,
                    sole_note: (count == 1).then(|| NoteCloseGuiNote {
                        slug_digest: query_cell_digest(slug),
                        fixture_slug: self.present,
                        fixture_marker: self.present,
                        pending_discard: self.prompt,
                        rendered_discard: self.prompt.then(|| NoteCloseGuiDiscard {
                            owner_slug_digest: query_cell_digest(slug),
                            widget_id: 9,
                            role: NoteCloseGuiWidgetRole::DiscardChanges,
                            enabled: true,
                            visible: true,
                            fully_visible: true,
                            bounds: [40, 100, 170, 120],
                            clip: [0, 0, 900, 650],
                        }),
                    }),
                }),
            };
            // GUI note existence and native marker-editor lookup are separate
            // observations. A canonical note can exist without that marker.
            if let Some(snapshot) = &self.ownership_snapshot {
                response.snapshot = Some(snapshot.clone());
            }
            if let Some((at, fault)) = self.ownership_fault {
                if at == self.ownership_calls {
                    let snapshot = response.snapshot.as_mut().unwrap();
                    match fault {
                        NoteOwnershipFault::CoexistentNote => {
                            snapshot.open_note_count = 2;
                            snapshot.sole_note = None;
                        }
                        NoteOwnershipFault::WrongSlug => {
                            snapshot.sole_note.as_mut().unwrap().fixture_slug = false
                        }
                        NoteOwnershipFault::WrongDigest => {
                            snapshot.sole_note.as_mut().unwrap().slug_digest ^= 1
                        }
                        NoteOwnershipFault::MissingMarker => {
                            snapshot.sole_note.as_mut().unwrap().fixture_marker = false
                        }
                        NoteOwnershipFault::WrongNonce => response.run_nonce[0] ^= 1,
                        NoteOwnershipFault::WrongRequest => response.request_id += 1,
                        NoteOwnershipFault::StaleFrame => {
                            response.observed_frame_ordinal = request.after_frame_ordinal
                        }
                        NoteOwnershipFault::WrongHwnd => response.root.as_mut().unwrap().hwnd = 43,
                        NoteOwnershipFault::WrongPid => {
                            response.root.as_mut().unwrap().process_id = 999
                        }
                        NoteOwnershipFault::WrongGeneration => {
                            response.root.as_mut().unwrap().generation = 99
                        }
                        NoteOwnershipFault::Failed => {
                            response.status = NoteCloseGuiStatus::Failed;
                            response.error = Some(NoteCloseGuiError::WrongRoot);
                        }
                        NoteOwnershipFault::MissingWidget => {
                            snapshot.sole_note.as_mut().unwrap().rendered_discard = None
                        }
                        NoteOwnershipFault::WrongWidgetOwner => {
                            snapshot
                                .sole_note
                                .as_mut()
                                .unwrap()
                                .rendered_discard
                                .as_mut()
                                .unwrap()
                                .owner_slug_digest ^= 1
                        }
                        NoteOwnershipFault::DisabledWidget => {
                            snapshot
                                .sole_note
                                .as_mut()
                                .unwrap()
                                .rendered_discard
                                .as_mut()
                                .unwrap()
                                .enabled = false
                        }
                        NoteOwnershipFault::HiddenWidget => {
                            snapshot
                                .sole_note
                                .as_mut()
                                .unwrap()
                                .rendered_discard
                                .as_mut()
                                .unwrap()
                                .visible = false
                        }
                        NoteOwnershipFault::ClippedWidget => {
                            snapshot
                                .sole_note
                                .as_mut()
                                .unwrap()
                                .rendered_discard
                                .as_mut()
                                .unwrap()
                                .fully_visible = false
                        }
                        NoteOwnershipFault::WrongWidgetBounds => {
                            snapshot
                                .sole_note
                                .as_mut()
                                .unwrap()
                                .rendered_discard
                                .as_mut()
                                .unwrap()
                                .bounds[0] += 1
                        }
                        NoteOwnershipFault::WrongClient => snapshot.client_size[0] += 1,
                        NoteOwnershipFault::TransportError => {
                            return Err(query_window_error(
                                "fixture snapshot transport failed".into(),
                            ));
                        }
                    }
                }
            }
            Ok(response)
        }
        fn current_discard(
            &mut self,
            root: &WindowSnapshot,
            admitted: &u32,
        ) -> Result<(u32, NoteDiscardObservation), VisibleTextLookupError> {
            assert_eq!(*admitted, 9);
            assert!(self.prompt);
            let mut readiness = SemanticControlReadiness {
                process_id: 202,
                bounds: [140, 300, 270, 320],
                enabled: true,
                offscreen: false,
                has_keyboard_focus: false,
                same_as_admitted: true,
            };
            if let Some(fault) = self.button_fault {
                match fault {
                    NoteReadinessFault::WrongProcess => readiness.process_id = 999,
                    NoteReadinessFault::ReplacedEditor => readiness.same_as_admitted = false,
                    NoteReadinessFault::Disabled => readiness.enabled = false,
                    NoteReadinessFault::Offscreen => readiness.offscreen = true,
                    NoteReadinessFault::EmptyBounds => readiness.bounds[2] = readiness.bounds[0],
                    _ => {
                        return Err(VisibleTextLookupError::Other(
                            "confirmation current property unavailable".into(),
                        ));
                    }
                }
            }
            self.events.push("fresh_discard_button:9".into());
            Ok((
                9,
                NoteDiscardObservation {
                    readiness,
                    is_button: !self.button_role_wrong,
                    client_screen_bounds: self
                        .client_screen_bounds(root)
                        .map_err(|error| VisibleTextLookupError::Other(error.message))?,
                },
            ))
        }
        fn focus_root(&mut self, root: &WindowSnapshot) -> Result<(), CaseFailure> {
            assert_eq!(hwnd_id(root.hwnd), 42);
            self.root_focus_calls += 1;
            self.events.push("focus_root:42".into());
            if self.root_focus_error {
                Err(query_window_error("native focus request failed".into()))
            } else {
                Ok(())
            }
        }
        fn request_editor_focus(&mut self, edit: &u32) -> Result<(), CaseFailure> {
            assert_eq!(*edit, 7);
            self.focus_calls += 1;
            self.focus_requested = true;
            self.events.push("SetFocus-admitted:7".into());
            if self.focus_error {
                Err(query_uia_error("semantic focus request failed".into()))
            } else {
                Ok(())
            }
        }
        fn foreground(&mut self) -> (u64, u32) {
            self.foreground_override.unwrap_or((42, 202))
        }
        fn escape(&mut self, root: &WindowSnapshot) -> Result<u32, CaseFailure> {
            assert_eq!(hwnd_id(root.hwnd), 42);
            assert_eq!(root.process_id, 202);
            self.escape_calls += 1;
            self.events.push("Escape-pair-insertion:42".into());
            self.escaped_at = Some(self.polls);
            if self.escape_error {
                Err(query_uia_error("checked SendInput failed".into()))
            } else {
                checked_note_escape_count(self.native_inserted.unwrap_or(self.inserted as usize))
            }
        }
        fn discard(&mut self, root: &WindowSnapshot, control: &u32) -> Result<(), CaseFailure> {
            assert_eq!((hwnd_id(root.hwnd), *control), (42, 9));
            self.discard_calls += 1;
            self.events.push("Discard Changes:9".into());
            self.discarded_at = Some(self.polls);
            if self.discard_error {
                Err(query_uia_error("normal discard input failed".into()))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn l08_note_close_waits_for_processed_same_editor_focus_before_one_escape() {
        let mut backend = ScriptedNoteClose {
            focus_after_polls: 3,
            ..Default::default()
        };
        let mut attempt = NoteCloseAttempt::default();
        close_fixture_note_with(&mut backend, &mut attempt).unwrap();
        let request = backend
            .events
            .iter()
            .position(|event| event == "SetFocus-admitted:7")
            .unwrap();
        let processed = backend
            .events
            .iter()
            .position(|event| event == "editor:7:focus=true")
            .unwrap();
        let escape = backend
            .events
            .iter()
            .position(|event| event == "Escape-pair-insertion:42")
            .unwrap();
        let absence = backend
            .events
            .iter()
            .position(|event| event == "marker_present:false")
            .unwrap();
        assert!(request < processed && processed < escape && escape < absence);
        assert_eq!(
            backend.events[request..processed]
                .iter()
                .filter(|event| event.as_str() == "poll")
                .count(),
            3
        );
        assert_eq!(
            (
                backend.root_focus_calls,
                backend.focus_calls,
                backend.escape_calls,
                backend.discard_calls
            ),
            (1, 1, 1, 0)
        );
        assert_eq!(attempt.escape_inserted, NoteCloseMeasurement::Observed(2));
        assert_eq!(attempt.note_absent, NoteCloseMeasurement::Observed(true));
        assert_eq!(attempt.phase, NoteClosePhase::Complete);
        assert!(attempt.elapsed_ms >= 5 * duration_millis(WINDOW_POLL));
        assert_eq!(attempt.close_deadline_ms, Some(5000));
        assert!(matches!(
            attempt.admitted_editor,
            NoteCloseMeasurement::Observed(SemanticControlReadiness {
                has_keyboard_focus: false,
                ..
            })
        ));
    }

    #[test]
    fn l08_note_close_already_focused_clean_note_waits_for_normal_absence() {
        let mut backend = ScriptedNoteClose {
            focus_requested: true,
            ..Default::default()
        };
        let mut attempt = NoteCloseAttempt::default();
        close_fixture_note_with(&mut backend, &mut attempt).unwrap();
        assert_eq!(
            (
                backend.focus_calls,
                backend.escape_calls,
                backend.discard_calls
            ),
            (1, 1, 0)
        );
        assert!(
            attempt
                .phases
                .iter()
                .any(|phase| phase.phase == NoteClosePhase::WaitingForNormalClose)
        );
        assert_eq!(attempt.discard_deadline_ms, None);
        let mut no_note = ScriptedNoteClose {
            present: false,
            ..Default::default()
        };
        let mut absent = NoteCloseAttempt::default();
        close_fixture_note_with(&mut no_note, &mut absent).unwrap();
        assert_eq!(
            (
                no_note.root_focus_calls,
                no_note.focus_calls,
                no_note.escape_calls
            ),
            (0, 0, 0)
        );
        assert_eq!(absent.escape_inserted, NoteCloseMeasurement::NotAttempted);
        assert_eq!(absent.note_absent, NoteCloseMeasurement::Observed(true));
    }

    #[test]
    fn l08_note_close_dirty_note_waits_for_one_normal_discard_and_absence() {
        let mut backend = ScriptedNoteClose {
            dirty: true,
            ..Default::default()
        };
        let mut attempt = NoteCloseAttempt::default();
        close_fixture_note_with(&mut backend, &mut attempt).unwrap();
        let escape = backend
            .events
            .iter()
            .position(|event| event == "Escape-pair-insertion:42")
            .unwrap();
        let prompt = backend
            .events
            .iter()
            .position(|event| event == "discard_prompt:true")
            .unwrap();
        let discard = backend
            .events
            .iter()
            .position(|event| event == "Discard Changes:9")
            .unwrap();
        let absence = backend
            .events
            .iter()
            .position(|event| event == "marker_present:false")
            .unwrap();
        assert!(escape < prompt && prompt < discard && discard < absence);
        assert_eq!((backend.escape_calls, backend.discard_calls), (1, 1));
        assert_eq!(attempt.discard_action, NoteCloseMeasurement::Observed(()));
        assert_eq!(attempt.note_absent, NoteCloseMeasurement::Observed(true));
        assert_eq!(attempt.outcome, NoteCloseMeasurement::Observed(()));
    }

    #[test]
    fn l08_note_close_already_pending_discard_uses_no_new_focus_or_escape() {
        let mut backend = ScriptedNoteClose {
            prompt: true,
            dirty: true,
            ..Default::default()
        };
        let mut attempt = NoteCloseAttempt::default();
        close_fixture_note_with(&mut backend, &mut attempt).unwrap();
        assert_eq!(
            (
                backend.root_focus_calls,
                backend.focus_calls,
                backend.escape_calls,
                backend.discard_calls
            ),
            (0, 0, 0, 1)
        );
        assert_eq!(attempt.focus_request, NoteCloseMeasurement::NotAttempted);
        assert_eq!(attempt.escape_inserted, NoteCloseMeasurement::NotAttempted);
        assert!(attempt.discard_deadline_ms.is_some());
        assert_eq!(attempt.note_absent, NoteCloseMeasurement::Observed(true));
    }

    #[test]
    fn l08_note_close_escape_count_adapter_preserves_native_counts_and_refuses_overflow() {
        for inserted in [0usize, 1, 2, 3] {
            assert_eq!(
                checked_note_escape_count(inserted).unwrap(),
                inserted as u32
            );
            let mut backend = ScriptedNoteClose {
                native_inserted: Some(inserted),
                ..Default::default()
            };
            let mut attempt = NoteCloseAttempt::default();
            let result = close_fixture_note_with(&mut backend, &mut attempt);
            assert_eq!(backend.escape_calls, 1);
            assert_eq!(
                attempt.escape_inserted,
                NoteCloseMeasurement::Observed(inserted as u32)
            );
            if inserted == 2 {
                result.unwrap();
                assert_eq!(attempt.note_absent, NoteCloseMeasurement::Observed(true));
            } else {
                assert_eq!(result.unwrap_err().stage, FailureStage::InputInjection);
                assert_eq!(attempt.failed_phase, Some(NoteClosePhase::EscapeInsertion));
                assert_eq!(attempt.note_absent, NoteCloseMeasurement::Observed(false));
                assert_eq!(backend.discard_calls, 0);
            }
        }
        assert_eq!(
            checked_note_escape_count(u32::MAX as usize).unwrap(),
            u32::MAX
        );
        if let Ok(overflow) = usize::try_from(u64::from(u32::MAX) + 1) {
            let mut backend = ScriptedNoteClose {
                native_inserted: Some(overflow),
                ..Default::default()
            };
            let mut attempt = NoteCloseAttempt::default();
            assert_eq!(
                close_fixture_note_with(&mut backend, &mut attempt)
                    .unwrap_err()
                    .stage,
                FailureStage::InputInjection
            );
            assert!(attempt.escape_requested);
            assert_eq!(backend.escape_calls, 1);
            assert!(matches!(
                attempt.escape_inserted,
                NoteCloseMeasurement::Failed(_)
            ));
            assert_eq!(attempt.note_absent, NoteCloseMeasurement::Observed(false));
            assert_eq!(backend.discard_calls, 0);
        }
    }

    #[test]
    fn l08_note_close_unrelated_pending_discard_without_fixture_sends_no_input() {
        let mut backend = ScriptedNoteClose {
            present: false,
            prompt: true,
            dirty: true,
            ..Default::default()
        };
        let mut attempt = NoteCloseAttempt::default();
        close_fixture_note_with(&mut backend, &mut attempt).unwrap();
        assert_eq!(
            (
                backend.root_focus_calls,
                backend.focus_calls,
                backend.escape_calls,
                backend.discard_calls
            ),
            (0, 0, 0, 0)
        );
        assert!(backend.prompt && backend.dirty && !backend.present);
        assert_eq!(backend.editor_observations, 0);
        assert_eq!(
            backend.lookup_purposes,
            vec![NoteEditorPurpose::DiscardConfirmation]
        );
        assert_eq!(
            attempt.marker_present,
            NoteCloseMeasurement::Observed(false)
        );
        assert_eq!(attempt.discard_prompt, NoteCloseMeasurement::Observed(true));
        assert_eq!(attempt.admitted_editor, NoteCloseMeasurement::NotAttempted);
        assert_eq!(attempt.current_editor, NoteCloseMeasurement::NotAttempted);
        assert_eq!(attempt.native_focus, NoteCloseMeasurement::NotAttempted);
        assert_eq!(attempt.focus_request, NoteCloseMeasurement::NotAttempted);
        assert!(!attempt.escape_requested);
        assert_eq!(attempt.escape_inserted, NoteCloseMeasurement::NotAttempted);
        assert_eq!(attempt.discard_action, NoteCloseMeasurement::NotAttempted);
        assert_eq!(attempt.discard_deadline_ms, None);
        assert_eq!(attempt.note_absent, NoteCloseMeasurement::Observed(true));
    }

    #[test]
    fn l08_note_close_pending_discard_disabled_fixture_owns_one_confirmation() {
        let mut backend = ScriptedNoteClose {
            prompt: true,
            dirty: true,
            editor_disabled: true,
            ..Default::default()
        };
        let mut attempt = NoteCloseAttempt::default();
        close_fixture_note_with(&mut backend, &mut attempt).unwrap();
        let discard = backend
            .events
            .iter()
            .position(|event| event == "Discard Changes:9")
            .unwrap();
        assert_eq!(
            backend.events[..discard]
                .iter()
                .filter(|event| event.as_str() == "editor:7:focus=false")
                .count(),
            2
        );
        assert_eq!(
            (
                backend.root_focus_calls,
                backend.focus_calls,
                backend.escape_calls,
                backend.discard_calls
            ),
            (0, 0, 0, 1)
        );
        assert!(
            backend
                .lookup_purposes
                .iter()
                .all(|purpose| *purpose == NoteEditorPurpose::DiscardConfirmation)
        );
        let NoteCloseMeasurement::Observed(editor) = &attempt.admitted_editor else {
            panic!("the disabled fixture editor must actually own this prompt");
        };
        assert_eq!(editor.process_id, 202);
        assert!(editor.same_as_admitted && !editor.offscreen);
        assert!(!editor.enabled && !editor.has_keyboard_focus);
        assert_eq!(attempt.current_editor, attempt.admitted_editor);
        assert_eq!(attempt.marker_present, NoteCloseMeasurement::Observed(true));
        assert_eq!(attempt.focus_request, NoteCloseMeasurement::NotAttempted);
        assert_eq!(attempt.escape_inserted, NoteCloseMeasurement::NotAttempted);
        assert_eq!(attempt.discard_action, NoteCloseMeasurement::Observed(()));
        assert!(attempt.discard_deadline_ms.is_some());
        assert_eq!(attempt.note_absent, NoteCloseMeasurement::Observed(true));
        assert_eq!(attempt.outcome, NoteCloseMeasurement::Observed(()));
    }

    #[test]
    fn l08_note_close_pending_discard_rejects_changed_editor_and_current_root_before_input() {
        for at in [1, 2] {
            for fault in [
                NoteReadinessFault::WrongProcess,
                NoteReadinessFault::ReplacedEditor,
                NoteReadinessFault::MissingMarker,
                NoteReadinessFault::Offscreen,
                NoteReadinessFault::EmptyBounds,
                NoteReadinessFault::PropertyError,
                NoteReadinessFault::TransientProperty,
            ] {
                let mut backend = ScriptedNoteClose {
                    prompt: true,
                    dirty: true,
                    editor_disabled: true,
                    fault: Some((at, fault)),
                    ..Default::default()
                };
                let mut attempt = NoteCloseAttempt::default();
                assert!(
                    close_fixture_note_with(&mut backend, &mut attempt).is_err(),
                    "{at}:{fault:?}"
                );
                assert_eq!(
                    (
                        backend.root_focus_calls,
                        backend.focus_calls,
                        backend.escape_calls,
                        backend.discard_calls
                    ),
                    (0, 0, 0, 0),
                    "{at}:{fault:?}"
                );
                assert!(backend.present && backend.prompt);
                assert_eq!(attempt.note_absent, NoteCloseMeasurement::Observed(false));
                assert_eq!(attempt.discard_action, NoteCloseMeasurement::NotAttempted);
                assert_eq!(attempt.escape_inserted, NoteCloseMeasurement::NotAttempted);
                assert_eq!(
                    attempt.failed_phase,
                    Some(if at == 1 {
                        NoteClosePhase::LocatingEditor
                    } else {
                        NoteClosePhase::DiscardAction
                    })
                );
            }
        }
        for kind in 0..7 {
            let mut backend = ScriptedNoteClose {
                prompt: true,
                dirty: true,
                ..Default::default()
            };
            if kind == 6 {
                backend.lookup_error_at = Some(1);
            } else {
                let mut wrong = backend.root.clone();
                match kind {
                    0 => wrong.hwnd = HWND(43usize as *mut std::ffi::c_void),
                    1 => wrong.process_id = 999,
                    2 => wrong.visible = false,
                    3 => wrong.minimized = true,
                    4 => wrong.role = WindowRole::OtherChild,
                    _ => wrong.bounds[3] = wrong.bounds[1],
                }
                backend.root_fault = Some((2, wrong));
            }
            let mut attempt = NoteCloseAttempt::default();
            assert!(
                close_fixture_note_with(&mut backend, &mut attempt).is_err(),
                "{kind}"
            );
            assert_eq!(
                (
                    backend.root_focus_calls,
                    backend.focus_calls,
                    backend.escape_calls,
                    backend.discard_calls
                ),
                (0, 0, 0, 0)
            );
            assert_eq!(attempt.discard_action, NoteCloseMeasurement::NotAttempted);
            assert_eq!(attempt.escape_inserted, NoteCloseMeasurement::NotAttempted);
            if kind == 6 {
                assert_eq!(attempt.admitted_editor, NoteCloseMeasurement::NotAttempted);
                assert_eq!(attempt.note_absent, NoteCloseMeasurement::NotAttempted);
            } else {
                assert!(matches!(
                    attempt.admitted_editor,
                    NoteCloseMeasurement::Observed(_)
                ));
                assert_eq!(attempt.note_absent, NoteCloseMeasurement::Observed(false));
            }
        }
    }

    #[test]
    fn l08_note_close_post_escape_discard_keeps_original_fixture_identity() {
        for fault in [
            NoteReadinessFault::ReplacedEditor,
            NoteReadinessFault::MissingMarker,
        ] {
            let mut backend = ScriptedNoteClose {
                dirty: true,
                fault: Some((4, fault)),
                ..Default::default()
            };
            let mut attempt = NoteCloseAttempt::default();
            let error = close_fixture_note_with(&mut backend, &mut attempt).unwrap_err();
            assert_eq!(error.stage, FailureStage::NativeRootState);
            assert_eq!(attempt.failed_phase, Some(NoteClosePhase::DiscardAction));
            assert_eq!(
                (
                    backend.root_focus_calls,
                    backend.focus_calls,
                    backend.escape_calls,
                    backend.discard_calls
                ),
                (1, 1, 1, 0)
            );
            assert!(backend.present && backend.prompt);
            assert_eq!(attempt.escape_inserted, NoteCloseMeasurement::Observed(2));
            assert_eq!(attempt.discard_prompt, NoteCloseMeasurement::Observed(true));
            assert_eq!(attempt.discard_action, NoteCloseMeasurement::NotAttempted);
            assert_eq!(attempt.note_absent, NoteCloseMeasurement::Observed(false));
            assert!(matches!(attempt.outcome, NoteCloseMeasurement::Failed(_)));
        }
    }

    #[test]
    fn l08_note_close_readiness_identity_and_visibility_failures_insert_no_escape() {
        for fault in [
            NoteReadinessFault::WrongProcess,
            NoteReadinessFault::ReplacedEditor,
            NoteReadinessFault::MissingMarker,
            NoteReadinessFault::Disabled,
            NoteReadinessFault::Offscreen,
            NoteReadinessFault::EmptyBounds,
        ] {
            let mut backend = ScriptedNoteClose {
                fault: Some((2, fault)),
                ..Default::default()
            };
            let mut attempt = NoteCloseAttempt::default();
            let error = close_fixture_note_with(&mut backend, &mut attempt).unwrap_err();
            assert_eq!(error.stage, FailureStage::NativeRootState, "{fault:?}");
            assert_eq!(
                attempt.failed_phase,
                Some(NoteClosePhase::WaitingForEditorFocus)
            );
            assert_eq!(
                (
                    backend.root_focus_calls,
                    backend.focus_calls,
                    backend.escape_calls
                ),
                (1, 1, 0)
            );
            assert!(!attempt.escape_requested);
            assert_eq!(attempt.note_absent, NoteCloseMeasurement::Observed(false));
        }
        for kind in 0..6 {
            let mut backend = ScriptedNoteClose::default();
            let mut wrong = backend.root.clone();
            match kind {
                0 => wrong.hwnd = HWND(43usize as *mut std::ffi::c_void),
                1 => wrong.process_id = 999,
                2 => wrong.visible = false,
                3 => wrong.minimized = true,
                4 => wrong.role = WindowRole::OtherChild,
                _ => wrong.bounds[3] = wrong.bounds[1],
            }
            backend.root_fault = Some((2, wrong));
            let mut attempt = NoteCloseAttempt::default();
            assert!(close_fixture_note_with(&mut backend, &mut attempt).is_err());
            assert_eq!(backend.escape_calls, 0);
            assert!(matches!(
                attempt.current_root,
                NoteCloseMeasurement::Observed(_)
            ));
        }
    }

    #[test]
    fn l08_note_close_lookup_property_focus_and_foreground_errors_do_not_inject() {
        for kind in 0..6 {
            let mut backend = ScriptedNoteClose::default();
            match kind {
                0 => backend.lookup_error_at = Some(1),
                1 => backend.fault = Some((2, NoteReadinessFault::PropertyError)),
                2 => backend.root_focus_error = true,
                3 => backend.focus_error = true,
                4 => backend.foreground_override = Some((42, 999)),
                _ => backend.root_error_at = Some(2),
            }
            let mut attempt = NoteCloseAttempt::default();
            assert!(close_fixture_note_with(&mut backend, &mut attempt).is_err());
            assert_eq!(backend.escape_calls, 0);
            assert!(backend.focus_calls <= 1 && backend.root_focus_calls <= 1);
            assert!(matches!(attempt.outcome, NoteCloseMeasurement::Failed(_)));
        }
        let mut transient = ScriptedNoteClose {
            fault: Some((2, NoteReadinessFault::TransientProperty)),
            ..Default::default()
        };
        close_fixture_note_with(&mut transient, &mut NoteCloseAttempt::default()).unwrap();
        assert_eq!(transient.escape_calls, 1);
        assert_eq!(transient.focus_calls, 1);
    }

    #[test]
    fn l08_note_close_final_revalidation_refuses_lost_ack_or_replaced_target() {
        for fault in [
            NoteReadinessFault::OtherEditorFocused,
            NoteReadinessFault::ReplacedEditor,
            NoteReadinessFault::MissingMarker,
            NoteReadinessFault::PropertyError,
        ] {
            let mut backend = ScriptedNoteClose {
                fault: Some((3, fault)),
                ..Default::default()
            };
            let mut attempt = NoteCloseAttempt::default();
            assert!(close_fixture_note_with(&mut backend, &mut attempt).is_err());
            assert!(
                attempt
                    .phases
                    .iter()
                    .any(|phase| phase.phase == NoteClosePhase::Ready)
            );
            assert_eq!(attempt.failed_phase, Some(NoteClosePhase::Revalidating));
            assert_eq!((backend.focus_calls, backend.escape_calls), (1, 0));
        }
    }

    #[test]
    fn l08_note_close_focus_and_close_consume_one_existing_deadline() {
        let mut backend = ScriptedNoteClose {
            focus_after_polls: 4,
            poll_step: Duration::from_secs(1),
            close_after_polls: 2,
            ..Default::default()
        };
        let mut attempt = NoteCloseAttempt::default();
        let error = close_fixture_note_with(&mut backend, &mut attempt).unwrap_err();
        assert_eq!(error.stage, FailureStage::Cleanup);
        assert_eq!(attempt.close_deadline_ms, Some(5000));
        assert_eq!(attempt.elapsed_ms, 5000);
        assert!(attempt.deadline_expired);
        assert_eq!(
            attempt.failed_phase,
            Some(NoteClosePhase::WaitingForNormalClose)
        );
        assert_eq!((backend.focus_calls, backend.escape_calls), (1, 1));
        assert!(
            backend.present,
            "insertion did not acknowledge note absence"
        );
        let mut timeout = ScriptedNoteClose {
            focus_after_polls: usize::MAX,
            poll_step: Duration::from_secs(1),
            ..Default::default()
        };
        let mut pending = NoteCloseAttempt::default();
        assert!(close_fixture_note_with(&mut timeout, &mut pending).is_err());
        assert_eq!(timeout.escape_calls, 0);
        assert_eq!(
            pending.failed_phase,
            Some(NoteClosePhase::WaitingForEditorFocus)
        );
        assert_eq!(pending.elapsed_ms, 5000);
        assert_eq!(pending.escape_inserted, NoteCloseMeasurement::NotAttempted);
    }

    #[test]
    fn l08_note_close_discard_keeps_its_existing_separate_completion_bound() {
        let mut backend = ScriptedNoteClose {
            focus_after_polls: 3,
            poll_step: Duration::from_secs(1),
            close_after_polls: 1,
            dirty: true,
            discard_after_polls: 3,
            ..Default::default()
        };
        let mut attempt = NoteCloseAttempt::default();
        close_fixture_note_with(&mut backend, &mut attempt).unwrap();
        assert_eq!(attempt.close_deadline_ms, Some(5000));
        assert_eq!(attempt.discard_deadline_ms, Some(9000));
        assert_eq!(attempt.elapsed_ms, 7000);
        assert_eq!(
            (
                backend.focus_calls,
                backend.escape_calls,
                backend.discard_calls
            ),
            (1, 1, 1)
        );
        let mut stuck = ScriptedNoteClose {
            prompt: true,
            discard_after_polls: usize::MAX,
            poll_step: Duration::from_secs(1),
            ..Default::default()
        };
        let mut pending = NoteCloseAttempt::default();
        assert!(close_fixture_note_with(&mut stuck, &mut pending).is_err());
        assert_eq!(
            pending.failed_phase,
            Some(NoteClosePhase::WaitingForDiscardClose)
        );
        assert_eq!(stuck.discard_calls, 1);
        assert_eq!(pending.note_absent, NoteCloseMeasurement::Observed(false));
    }

    #[test]
    fn l08_note_close_input_failures_do_not_claim_delivery_absence_or_retry() {
        for inserted in [0, 1, 3] {
            let mut backend = ScriptedNoteClose {
                inserted,
                ..Default::default()
            };
            let mut attempt = NoteCloseAttempt::default();
            assert!(close_fixture_note_with(&mut backend, &mut attempt).is_err());
            assert_eq!(backend.escape_calls, 1);
            assert_eq!(
                attempt.escape_inserted,
                NoteCloseMeasurement::Observed(inserted)
            );
            assert_eq!(attempt.failed_phase, Some(NoteClosePhase::EscapeInsertion));
            assert_eq!(attempt.note_absent, NoteCloseMeasurement::Observed(false));
        }
        let mut backend = ScriptedNoteClose {
            escape_error: true,
            ..Default::default()
        };
        let mut attempt = NoteCloseAttempt::default();
        assert!(close_fixture_note_with(&mut backend, &mut attempt).is_err());
        assert_eq!(backend.escape_calls, 1);
        assert!(matches!(
            attempt.escape_inserted,
            NoteCloseMeasurement::Failed(_)
        ));
        let mut discard = ScriptedNoteClose {
            prompt: true,
            discard_error: true,
            ..Default::default()
        };
        assert!(close_fixture_note_with(&mut discard, &mut NoteCloseAttempt::default()).is_err());
        assert_eq!((discard.escape_calls, discard.discard_calls), (0, 1));
    }

    #[test]
    fn l08_note_close_coexistent_notes_refuse_close_and_unrelated_confirmation() {
        for pending in [false, true] {
            let mut backend = ScriptedNoteClose {
                prompt: pending,
                dirty: pending,
                ownership_fault: Some((1, NoteOwnershipFault::CoexistentNote)),
                ..Default::default()
            };
            let mut attempt = NoteCloseAttempt::default();
            let error = close_fixture_note_with(&mut backend, &mut attempt).unwrap_err();
            assert!(error.message.contains("sole canonical Q11"));
            assert_eq!(
                (
                    backend.root_focus_calls,
                    backend.focus_calls,
                    backend.escape_calls,
                    backend.discard_calls
                ),
                (0, 0, 0, 0)
            );
            let NoteCloseMeasurement::Observed(receipt) = &attempt.gui_ownership else {
                panic!("retain the actual ambiguous GUI frame");
            };
            assert_eq!(receipt.snapshot.as_ref().unwrap().open_note_count, 2);
            assert!(receipt.snapshot.as_ref().unwrap().sole_note.is_none());
            assert_eq!(attempt.admitted_editor, NoteCloseMeasurement::NotAttempted);
            assert!(matches!(attempt.outcome, NoteCloseMeasurement::Failed(_)));
        }
    }

    #[test]
    fn l08_note_close_gui_owner_replay_and_canonical_rejections_send_no_input() {
        for fault in [
            NoteOwnershipFault::WrongSlug,
            NoteOwnershipFault::WrongDigest,
            NoteOwnershipFault::MissingMarker,
            NoteOwnershipFault::WrongNonce,
            NoteOwnershipFault::WrongRequest,
            NoteOwnershipFault::StaleFrame,
            NoteOwnershipFault::WrongHwnd,
            NoteOwnershipFault::WrongPid,
            NoteOwnershipFault::Failed,
            NoteOwnershipFault::WrongClient,
            NoteOwnershipFault::TransportError,
        ] {
            let mut backend = ScriptedNoteClose {
                ownership_fault: Some((1, fault)),
                ..Default::default()
            };
            let mut attempt = NoteCloseAttempt::default();
            assert!(
                close_fixture_note_with(&mut backend, &mut attempt).is_err(),
                "{fault:?}"
            );
            assert_eq!(
                (
                    backend.root_focus_calls,
                    backend.focus_calls,
                    backend.escape_calls,
                    backend.discard_calls
                ),
                (0, 0, 0, 0),
                "{fault:?}"
            );
            assert_eq!(attempt.admitted_editor, NoteCloseMeasurement::NotAttempted);
            assert_eq!(attempt.failed_phase, Some(NoteClosePhase::LocatingEditor));
            assert!(matches!(attempt.outcome, NoteCloseMeasurement::Failed(_)));
        }
    }

    #[test]
    fn l08_note_close_fresh_discard_widget_and_native_button_revalidation_are_required() {
        for fault in [
            NoteOwnershipFault::MissingWidget,
            NoteOwnershipFault::WrongWidgetOwner,
            NoteOwnershipFault::DisabledWidget,
            NoteOwnershipFault::HiddenWidget,
            NoteOwnershipFault::ClippedWidget,
            NoteOwnershipFault::WrongWidgetBounds,
            NoteOwnershipFault::WrongGeneration,
        ] {
            let mut backend = ScriptedNoteClose {
                prompt: true,
                dirty: true,
                ownership_fault: Some((2, fault)),
                ..Default::default()
            };
            let mut attempt = NoteCloseAttempt::default();
            assert!(
                close_fixture_note_with(&mut backend, &mut attempt).is_err(),
                "{fault:?}"
            );
            assert_eq!(backend.ownership_calls, 2);
            assert_eq!(
                (
                    backend.root_focus_calls,
                    backend.focus_calls,
                    backend.escape_calls,
                    backend.discard_calls
                ),
                (0, 0, 0, 0)
            );
            assert_eq!(attempt.failed_phase, Some(NoteClosePhase::DiscardAction));
        }
        for fault in [
            NoteReadinessFault::WrongProcess,
            NoteReadinessFault::ReplacedEditor,
            NoteReadinessFault::Disabled,
            NoteReadinessFault::Offscreen,
            NoteReadinessFault::EmptyBounds,
            NoteReadinessFault::PropertyError,
        ] {
            let mut backend = ScriptedNoteClose {
                prompt: true,
                dirty: true,
                button_fault: Some(fault),
                ..Default::default()
            };
            assert!(
                close_fixture_note_with(&mut backend, &mut NoteCloseAttempt::default()).is_err(),
                "{fault:?}"
            );
            assert_eq!(
                (
                    backend.root_focus_calls,
                    backend.focus_calls,
                    backend.escape_calls,
                    backend.discard_calls
                ),
                (0, 0, 0, 0)
            );
        }
        let mut wrong_role = ScriptedNoteClose {
            prompt: true,
            button_role_wrong: true,
            ..Default::default()
        };
        assert!(
            close_fixture_note_with(&mut wrong_role, &mut NoteCloseAttempt::default()).is_err()
        );
        assert_eq!(wrong_role.discard_calls, 0);
    }

    #[test]
    fn l08_note_close_later_coexistence_or_lifetime_change_refuses_escape_and_postescape_discard() {
        for fault in [
            NoteOwnershipFault::CoexistentNote,
            NoteOwnershipFault::WrongGeneration,
        ] {
            let mut before_escape = ScriptedNoteClose {
                ownership_fault: Some((2, fault)),
                ..Default::default()
            };
            let mut attempt = NoteCloseAttempt::default();
            assert!(close_fixture_note_with(&mut before_escape, &mut attempt).is_err());
            assert_eq!(
                (
                    before_escape.root_focus_calls,
                    before_escape.focus_calls,
                    before_escape.escape_calls,
                    before_escape.discard_calls
                ),
                (1, 1, 0, 0)
            );
            assert_eq!(attempt.failed_phase, Some(NoteClosePhase::Revalidating));
            let mut after_escape = ScriptedNoteClose {
                dirty: true,
                ownership_fault: Some((3, fault)),
                ..Default::default()
            };
            let mut attempt = NoteCloseAttempt::default();
            assert!(close_fixture_note_with(&mut after_escape, &mut attempt).is_err());
            assert_eq!(after_escape.ownership_calls, 3);
            assert_eq!(
                (after_escape.escape_calls, after_escape.discard_calls),
                (1, 0)
            );
            assert_eq!(attempt.escape_inserted, NoteCloseMeasurement::Observed(2));
            assert_eq!(attempt.failed_phase, Some(NoteClosePhase::DiscardAction));
        }
        let mut pending = ScriptedNoteClose {
            prompt: true,
            ..Default::default()
        };
        let mut attempt = NoteCloseAttempt::default();
        close_fixture_note_with(&mut pending, &mut attempt).unwrap();
        let NoteCloseMeasurement::Observed(request) = &attempt.gui_request else {
            panic!("missing fresh request");
        };
        let NoteCloseMeasurement::Observed(response) = &attempt.gui_ownership else {
            panic!("missing fresh frame");
        };
        assert_eq!(request.expected_generation, Some(3));
        assert_eq!(request.after_frame_ordinal, 1);
        assert_eq!(response.observed_frame_ordinal, 2);
        let fresh_button = pending
            .events
            .iter()
            .position(|event| event == "fresh_discard_button:9")
            .unwrap();
        let action = pending
            .events
            .iter()
            .position(|event| event == "Discard Changes:9")
            .unwrap();
        assert!(fresh_button < action);
        assert_eq!((pending.escape_calls, pending.discard_calls), (0, 1));
    }

    #[test]
    fn l08_note_close_ownership_snapshot_time_consumes_original_deadline() {
        for pending in [false, true] {
            let mut backend = ScriptedNoteClose {
                prompt: pending,
                dirty: pending,
                ownership_delay: Duration::from_secs(3),
                ..Default::default()
            };
            let mut attempt = NoteCloseAttempt::default();
            let error = close_fixture_note_with(&mut backend, &mut attempt).unwrap_err();
            assert_eq!(backend.ownership_calls, 2);
            assert_eq!(attempt.close_deadline_ms, Some(5000));
            assert_eq!(attempt.elapsed_ms, 6000);
            assert_eq!((backend.escape_calls, backend.discard_calls), (0, 0));
            assert!(error.message.contains("deadline"));
            assert_eq!(attempt.discard_deadline_ms, None);
        }
    }

    #[test]
    fn l08_note_close_ambiguity_receipt_survives_precleanup_capture_and_cleanup_failure() {
        let mut operation = L08OperationDiagnostic::default();
        let mut backend = ScriptedNoteClose {
            prompt: true,
            ownership_fault: Some((1, NoteOwnershipFault::CoexistentNote)),
            ..Default::default()
        };
        let result: Result<(), CaseFailure> = run_l08_operation_with(
            &mut operation,
            || Ok(()),
            |attempt| close_fixture_note_with(&mut backend, attempt),
            |_| panic!("placement must not run after an ambiguous note"),
        );
        let primary = result.as_ref().unwrap_err().message.clone();
        let captures = std::cell::Cell::new(0);
        let cleanup_calls = std::cell::Cell::new(0);
        let run = finish_l08_with_cleanup(
            result,
            operation,
            |context| {
                captures.set(captures.get() + 1);
                assert_eq!(cleanup_calls.get(), 0);
                let NoteCloseMeasurement::Observed(receipt) =
                    &context.operation.initial_note_close.gui_ownership
                else {
                    panic!("actual source receipt absent");
                };
                assert_eq!(receipt.snapshot.as_ref().unwrap().open_note_count, 2);
                let private = serde_json::to_vec(context).unwrap();
                assert!(private.len() <= 64 * 1024);
                let value: serde_json::Value = serde_json::from_slice(&private).unwrap();
                assert_eq!(
                    value["operation"]["initial_note_close"]["gui_ownership"]["Observed"]["snapshot"]
                        ["open_note_count"],
                    2
                );
                PrecleanupDiagnosticArtifacts::default()
            },
            |attempt| {
                cleanup_calls.set(cleanup_calls.get() + 1);
                backend.ownership_fault = Some((2, NoteOwnershipFault::CoexistentNote));
                close_fixture_note_with(&mut backend, attempt)
            },
        );
        assert_eq!((captures.get(), cleanup_calls.get()), (1, 1));
        assert!(run.result.unwrap_err().message.starts_with(&primary));
        assert!(run.diagnostics.unwrap().precleanup_artifacts.is_some());
        assert_eq!(
            (
                backend.root_focus_calls,
                backend.focus_calls,
                backend.escape_calls,
                backend.discard_calls
            ),
            (0, 0, 0, 0)
        );
    }

    fn note_close_canonical_without_marker_snapshot(pending: bool) -> NoteCloseGuiSnapshot {
        // The real NotePanel producer retains the canonical slug after content
        // mutation. Native marker lookup independently returns no editor.
        serde_json::from_value(serde_json::json!({
            "client_size": [900, 650], "open_note_count": 1,
            "sole_note": {
                "slug_digest": query_cell_digest("radial-acceptance-q11"),
                "fixture_slug": true, "fixture_marker": false,
                "pending_discard": pending, "rendered_discard": null,
            }
        }))
        .unwrap()
    }

    #[test]
    fn l08_note_close_open_canonical_without_marker_and_native_editor_refuses_both_routes() {
        for pending in [false, true] {
            for identity in ["canonical", "wrong_digest", "contradictory_slug"] {
                let mut snapshot = note_close_canonical_without_marker_snapshot(pending);
                match identity {
                    "wrong_digest" => snapshot.sole_note.as_mut().unwrap().slug_digest ^= 1,
                    "contradictory_slug" => {
                        snapshot.sole_note.as_mut().unwrap().fixture_slug = false
                    }
                    _ => {}
                }
                let mut backend = ScriptedNoteClose {
                    present: false,
                    prompt: pending,
                    ownership_snapshot: Some(snapshot.clone()),
                    ..Default::default()
                };
                let mut attempt = NoteCloseAttempt::default();
                let error = close_fixture_note_with(&mut backend, &mut attempt).unwrap_err();
                assert!(
                    error.message.contains("not actually absent"),
                    "{identity}, pending={pending}"
                );
                assert_eq!(backend.lookup_calls, 1);
                assert_eq!(
                    backend.lookup_purposes,
                    [if pending {
                        NoteEditorPurpose::DiscardConfirmation
                    } else {
                        NoteEditorPurpose::Focus
                    }]
                );
                assert!(
                    backend
                        .events
                        .iter()
                        .any(|event| event == "marker_present:false")
                );
                assert_eq!(
                    (
                        backend.root_focus_calls,
                        backend.focus_calls,
                        backend.escape_calls,
                        backend.discard_calls
                    ),
                    (0, 0, 0, 0)
                );
                let NoteCloseMeasurement::Observed(receipt) = &attempt.gui_ownership else {
                    panic!("retain the real-shaped canonical existence receipt");
                };
                assert_eq!(receipt.snapshot, Some(snapshot));
                assert_eq!(attempt.note_absent, NoteCloseMeasurement::NotAttempted);
                assert_eq!(attempt.admitted_editor, NoteCloseMeasurement::NotAttempted);
                assert_eq!(attempt.native_focus, NoteCloseMeasurement::NotAttempted);
                assert_eq!(attempt.focus_request, NoteCloseMeasurement::NotAttempted);
                assert!(!attempt.escape_requested);
                assert_eq!(attempt.escape_inserted, NoteCloseMeasurement::NotAttempted);
                assert_eq!(attempt.discard_action, NoteCloseMeasurement::NotAttempted);
                assert_eq!(attempt.phase, NoteClosePhase::Failed);
                assert_eq!(attempt.failed_phase, Some(NoteClosePhase::LocatingEditor));
                assert!(matches!(attempt.outcome, NoteCloseMeasurement::Failed(_)));
            }
        }
    }

    #[test]
    fn l08_note_close_absence_requires_zero_notes_or_consistent_noncanonical_owner() {
        let zero = NoteCloseGuiSnapshot {
            client_size: [900, 650],
            open_note_count: 0,
            sole_note: None,
        };
        for (snapshot, pending) in
            std::iter::once((zero, false)).chain([false, true].map(|pending| {
                let mut snapshot = note_close_canonical_without_marker_snapshot(pending);
                let note = snapshot.sole_note.as_mut().unwrap();
                note.fixture_slug = false;
                note.slug_digest = query_cell_digest("another-note");
                (snapshot, pending)
            }))
        {
            let mut backend = ScriptedNoteClose {
                present: false,
                prompt: pending,
                ownership_snapshot: Some(snapshot),
                ..Default::default()
            };
            let mut attempt = NoteCloseAttempt::default();
            close_fixture_note_with(&mut backend, &mut attempt).unwrap();
            assert_eq!(
                (
                    backend.root_focus_calls,
                    backend.focus_calls,
                    backend.escape_calls,
                    backend.discard_calls
                ),
                (0, 0, 0, 0)
            );
            assert_eq!(attempt.note_absent, NoteCloseMeasurement::Observed(true));
            assert_eq!(attempt.phase, NoteClosePhase::Complete);
            assert_eq!(attempt.outcome, NoteCloseMeasurement::Observed(()));
        }
        let mut ambiguous = ScriptedNoteClose {
            present: false,
            ownership_snapshot: Some(NoteCloseGuiSnapshot {
                client_size: [900, 650],
                open_note_count: 2,
                sole_note: None,
            }),
            ..Default::default()
        };
        let mut attempt = NoteCloseAttempt::default();
        assert!(close_fixture_note_with(&mut ambiguous, &mut attempt).is_err());
        assert_eq!(attempt.phase, NoteClosePhase::Failed);
        assert_ne!(attempt.note_absent, NoteCloseMeasurement::Observed(true));
        assert_eq!(
            (
                ambiguous.root_focus_calls,
                ambiguous.focus_calls,
                ambiguous.escape_calls,
                ambiguous.discard_calls
            ),
            (0, 0, 0, 0)
        );
    }

    #[test]
    fn l08_note_close_markerless_canonical_failure_prevents_placement_and_survives_mandatory_cleanup()
     {
        for pending in [false, true] {
            let mut backend = ScriptedNoteClose {
                present: false,
                prompt: pending,
                ownership_snapshot: Some(note_close_canonical_without_marker_snapshot(pending)),
                ..Default::default()
            };
            let mut operation = L08OperationDiagnostic::default();
            let moved = std::cell::Cell::new(0);
            let dispatched = std::cell::Cell::new(0);
            let result: Result<(QueryInvocationEvidence, QueryPlacementEvidence), CaseFailure> =
                run_l08_operation_with(
                    &mut operation,
                    || Ok(()),
                    |attempt| close_fixture_note_with(&mut backend, attempt),
                    |_| {
                        moved.set(moved.get() + 1);
                        dispatched.set(dispatched.get() + 1);
                        panic!("no placement/new-note dispatch after failed initial close");
                    },
                );
            let primary = result.as_ref().unwrap_err().message.clone();
            assert_eq!((moved.get(), dispatched.get()), (0, 0));
            let captures = std::cell::Cell::new(0);
            let cleanup_calls = std::cell::Cell::new(0);
            let finished = finish_l08_with_cleanup(
                result,
                operation,
                |context| {
                    captures.set(captures.get() + 1);
                    assert_eq!(cleanup_calls.get(), 0);
                    assert_eq!(context.operation.phase, L08OperationPhase::InitialNoteClose);
                    let initial = &context.operation.initial_note_close;
                    assert_eq!(initial.phase, NoteClosePhase::Failed);
                    assert_ne!(initial.note_absent, NoteCloseMeasurement::Observed(true));
                    let NoteCloseMeasurement::Observed(receipt) = &initial.gui_ownership else {
                        panic!("missing actual source receipt before teardown");
                    };
                    let note = receipt
                        .snapshot
                        .as_ref()
                        .unwrap()
                        .sole_note
                        .as_ref()
                        .unwrap();
                    assert!(note.fixture_slug && !note.fixture_marker);
                    assert_eq!(note.slug_digest, query_cell_digest("radial-acceptance-q11"));
                    let bytes = serde_json::to_vec(context).unwrap();
                    assert!(bytes.len() <= 64 * 1024);
                    let captured: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(
                        captured["operation"]["initial_note_close"]["gui_ownership"]["Observed"]["snapshot"]
                            ["open_note_count"],
                        1
                    );
                    PrecleanupDiagnosticArtifacts {
                        errors: vec!["bounded deterministic precleanup capture error".into()],
                        ..Default::default()
                    }
                },
                |attempt| {
                    cleanup_calls.set(cleanup_calls.get() + 1);
                    close_fixture_note_with(&mut backend, attempt)
                },
            );
            assert_eq!((captures.get(), cleanup_calls.get()), (1, 1));
            assert_eq!((moved.get(), dispatched.get()), (0, 0));
            let error = finished.result.as_ref().unwrap_err();
            assert!(error.message.starts_with(&primary));
            assert!(error.message.contains("fixture note cleanup:"));
            assert!(error.message.contains("pre-note-cleanup capture:"));
            assert!(
                finished.result.as_ref().ok().is_none(),
                "no proof can mark note_closed_after on failure"
            );
            let diagnostics = finished.diagnostics.unwrap();
            assert_eq!(diagnostics.precleanup_artifacts, Some(Vec::new()));
            assert_eq!(
                diagnostics.l08.cleanup_note_close.phase,
                NoteClosePhase::Failed
            );
            assert_ne!(
                diagnostics.l08.cleanup_note_close.note_absent,
                NoteCloseMeasurement::Observed(true)
            );
            assert!(matches!(
                diagnostics.l08.cleanup_note_close.outcome,
                NoteCloseMeasurement::Failed(_)
            ));
            assert_eq!(
                (
                    backend.root_focus_calls,
                    backend.focus_calls,
                    backend.escape_calls,
                    backend.discard_calls
                ),
                (0, 0, 0, 0)
            );
        }
    }

    fn l08_first_failure_fixture(
        report: &AcceptanceReport,
        context: &L08CaseDiagnosticContext,
    ) -> L08FirstFailureDiagnostic {
        let state = L08PrecleanupState::from_context(context);
        L08FirstFailureDiagnostic {
            qualification: FailedHotkeyQualification::UNQUALIFIED,
            state,
            status: state.status(),
            case_id: "L08",
            phase: "before-note-cleanup",
            identity: PrivateCaseDiagnosticIdentity::from_report(report),
            context: context.clone(),
            root: None,
            root_error: Some("no native capture in this deterministic fixture".into()),
            root_error_original_bytes: Some(
                "no native capture in this deterministic fixture".len(),
            ),
            windows: Vec::new(),
            windows_observed: 0,
            windows_omitted: 0,
            private_tree: L08PrivateObservation::NotAttempted,
            safe_trace: L08PrivateObservation::NotAttempted,
            root_crop: L08PrivateObservation::NotAttempted,
            capture_errors: Vec::new(),
            capture_errors_omitted: 0,
        }
    }

    fn l08_completed_placement_fixture(
        hotkey: AcceptanceHotkey,
    ) -> (QueryInvocationEvidence, QueryPlacementEvidence) {
        let configured = QueryRootPresentationEvidence {
            hwnd: 42,
            process_id: 202,
            bounds: [240, 180, 1156, 869],
            visible: true,
            minimized: false,
            physically_visible: true,
        };
        let moved = QueryRootPresentationEvidence {
            bounds: [320, 260, 1236, 949],
            ..configured.clone()
        };
        let mut invocation = crate::tests::query_invocation_for_test(
            "qa-note-new",
            QueryEvidenceMode::ExactCommand,
            QueryEvidenceState::Ready,
            QueryEvidenceRequirement::LauncherUi,
            QueryEvidenceOutcome::Executed,
            QueryEvidenceRootPolicy::Legacy,
            1,
            0,
            QueryEvidenceUiAck::NoteEditor,
            true,
            120,
        );
        invocation.root_observations = vec![moved.clone(), moved.clone()];
        invocation.setup_visibility = QuerySetupVisibilityEvidence {
            visible_before: true,
            desired_visible: true,
            tap: None,
        };
        let toggle = |visible, cursor: usize, invocation_id, revision| {
            let command = HotkeyCandidateEventEvidence {
                stream: HotkeyCandidateStream::MainCandidate,
                input_group_id: 1,
                input_purpose: HotkeyRunnerInputPurpose::LauncherChord,
                event_ordinal: (cursor + 5) as u32,
                elapsed_ms: (cursor + 5) as u64,
                kind: HotkeyTraceEventKind::RootCommand,
                invocation_id: Some(invocation_id),
                visibility_revision: Some(revision),
                request_id: Some(revision),
                visible: None,
                minimized: None,
                bounds: None,
                hwnd: None,
                process_id: None,
                command: Some(if visible {
                    HotkeyRootCommand::Show
                } else {
                    HotkeyRootCommand::Position
                }),
                visibility_source: None,
                modifiers_match: None,
                provenance: None,
                terminal: None,
                activation_edge: None,
                focus_intent: None,
                radial_action_stage: None,
            };
            let presentation = if visible {
                configured.clone()
            } else {
                QueryRootPresentationEvidence {
                    bounds: [-10_000, -10_000, -9084, -9311],
                    physically_visible: false,
                    ..moved.clone()
                }
            };
            let mut snapshot = command.clone();
            snapshot.kind = HotkeyTraceEventKind::NativeWindowSnapshot;
            snapshot.command = None;
            snapshot.event_ordinal += 1;
            snapshot.elapsed_ms += 1;
            snapshot.hwnd = Some(42);
            snapshot.process_id = Some(202);
            snapshot.bounds = Some(presentation.bounds);
            snapshot.visible = Some(true);
            snapshot.minimized = Some(false);
            let edges = crate::configured_chord_edges(hotkey, 1);
            QueryPlacementToggleEvidence {
                input: QuerySetupVisibilityEvidence {
                    visible_before: !visible,
                    desired_visible: visible,
                    tap: Some(QuerySetupTapEvidence {
                        input_down_inserted: edges.len() / 2,
                        input_up_inserted: edges.len() / 2,
                        quiet_preflight_ms: 80,
                        quiet_matching_edges: 0,
                        observer_default_desktop: true,
                        observed_edges: edges
                            .into_iter()
                            .enumerate()
                            .map(|(index, (virtual_key, down))| QuerySetupObservedEdge {
                                runner_relative_us: index as u64 * 100,
                                virtual_key,
                                down,
                                injected: true,
                                runner_cookie_matched: true,
                            })
                            .collect(),
                        foreign_edges: Vec::new(),
                        trace_cursor: cursor,
                        invocation_id,
                        generation: 10,
                        press_event_ordinal: cursor + 1,
                        release_event_ordinal: cursor + 2,
                        short_tap_event_ordinal: cursor + 3,
                        visibility_event_ordinal: cursor + 4,
                        visibility_visible: visible,
                    }),
                },
                visibility_revision: revision,
                command,
                snapshot,
                presentation,
            }
        };
        (
            invocation,
            QueryPlacementEvidence {
                hotkey,
                physical_displays: vec![[0, 0, 1920, 1080]],
                dpi: 96,
                configured_logical_position: QUERY_CONFIGURED_ROOT_POSITION,
                configured_logical_client_size: QUERY_CONFIGURED_ROOT_CLIENT_SIZE,
                configured_client_bounds: [0, 0, 900, 650],
                configured_root: configured.clone(),
                moved_root: moved.clone(),
                moved_client_bounds: [0, 0, 900, 650],
                handoff_terminal_cursor: 20,
                restored_root: moved.clone(),
                restored_client_bounds: [0, 0, 900, 650],
                hide: toggle(false, 30, 200, 20),
                show: toggle(true, 40, 201, 21),
                shown_client_bounds: [0, 0, 900, 650],
                note_absent_before: true,
                note_closed_after: true,
            },
        )
    }

    #[test]
    fn l08_postcleanup_placement_rejection_retains_awaiting_precleanup_measurements() {
        for hotkey in [AcceptanceHotkey::F11, AcceptanceHotkey::ShiftAltWinEnd] {
            let directory = tempfile::tempdir().unwrap();
            let mut report = test_acceptance_report(hotkey);
            let (invocation, mut proof) = l08_completed_placement_fixture(hotkey);
            assert!(
                qualify_query_case_result("L08", Ok((invocation.clone(), proof.clone())))
                    .0
                    .is_ok()
            );
            // The operation returns normally, but restore-only geometry fails
            // the unchanged placement oracle only after mandatory cleanup.
            proof.restored_root.bounds[0] += 1;
            proof.note_closed_after = false;
            let order = RefCell::new(Vec::new());
            let mut operation = L08OperationDiagnostic::default();
            let result = run_l08_operation_with(
                &mut operation,
                || Ok(()),
                |attempt| {
                    close_fixture_note_with(
                        &mut ScriptedNoteClose {
                            present: false,
                            ..Default::default()
                        },
                        attempt,
                    )
                },
                |operation| {
                    operation.phase = L08OperationPhase::PlacementMeasured;
                    operation.note_query_requested = true;
                    operation.hide_completed = true;
                    operation.show_completed = true;
                    Ok((invocation, proof))
                },
            );
            let mut completed = finish_l08_with_cleanup(
                result,
                operation,
                |context| {
                    order.borrow_mut().push("precleanup");
                    assert!(context.original_operation.succeeded);
                    assert!(context.cleanup_result.is_none());
                    let bytes = bounded_l08_diagnostic_bytes(&mut l08_first_failure_fixture(
                        &report, context,
                    ))
                    .unwrap();
                    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(value["state"], "AwaitingCleanupAndValidation");
                    assert!(value["status"].is_null());
                    assert_eq!(value["qualification"], "UNQUALIFIED");
                    let mut captured = PrecleanupDiagnosticArtifacts::default();
                    retain_l08_capture_piece(
                        &mut captured,
                        directory.path(),
                        PrivateCaseDiagnosticFile::L08FirstFailure,
                        Ok(bytes),
                    );
                    assert!(captured.errors.is_empty());
                    captured
                },
                |attempt| {
                    order.borrow_mut().push("cleanup");
                    close_fixture_note_with(&mut ScriptedNoteClose::default(), attempt)
                },
            );
            completed.result.as_mut().unwrap().1.note_closed_after = true;
            assert!(
                completed
                    .diagnostics
                    .as_ref()
                    .unwrap()
                    .l08
                    .cleanup_result
                    .as_ref()
                    .unwrap()
                    .succeeded
            );
            let first = directory
                .path()
                .join("case-L08-before-note-cleanup-unqualified.json");
            let original = fs::read(&first).unwrap();
            let (result, evidence) = qualify_query_case_result("L08", completed.result);
            order.borrow_mut().push("validation");
            let primary = result.as_ref().unwrap_err().clone();
            assert_eq!(primary.stage, FailureStage::GestureDecision);
            assert_eq!(
                primary.message,
                "L08 Query evidence does not prove its case contract"
            );
            assert!(evidence.is_none());
            append_query_case_result_with_diagnostics(
                &mut report,
                "L08",
                Instant::now(),
                result,
                evidence,
                None,
                directory.path(),
                &directory.path().join("trace.log"),
                &directory.path().join("markers.json"),
                hotkey,
                200,
                completed.diagnostics,
            );
            assert_eq!(*order.borrow(), ["precleanup", "cleanup", "validation"]);
            assert_eq!(report.cases[0].status, CaseStatus::Failed);
            assert_eq!(report.cases[0].failure_stage, Some(primary.stage));
            assert_eq!(report.cases[0].observed, primary.message);
            assert_eq!(report.cases[0].artifacts.len(), 2);
            assert!(report.query_evidence.is_empty());
            assert_eq!(fs::read(first).unwrap(), original);
            let cleanup: serde_json::Value = serde_json::from_slice(
                &fs::read(
                    directory
                        .path()
                        .join("case-L08-after-note-cleanup-unqualified.json"),
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(cleanup["context"]["original_operation"]["succeeded"], true);
            assert_eq!(cleanup["context"]["cleanup_result"]["succeeded"], true);
            assert_eq!(cleanup["case_result"]["succeeded"], false);
            assert_eq!(cleanup["case_result"]["detail"], primary.message);
            assert!(!directory.path().join("case-L08.png").exists());
        }
    }

    #[test]
    fn l08_successful_validation_does_not_attach_snapshot_or_fail_on_capture_error() {
        for capture_fails in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let mut report = test_acceptance_report(AcceptanceHotkey::F11);
            let completed = finish_l08_with_cleanup(
                Ok(l08_completed_placement_fixture(AcceptanceHotkey::F11)),
                L08OperationDiagnostic::default(),
                |context| {
                    let mut captured = PrecleanupDiagnosticArtifacts::default();
                    let bytes = if capture_fails {
                        Err("precleanup capture unavailable".into())
                    } else {
                        bounded_l08_diagnostic_bytes(&mut l08_first_failure_fixture(
                            &report, context,
                        ))
                    };
                    retain_l08_capture_piece(
                        &mut captured,
                        directory.path(),
                        PrivateCaseDiagnosticFile::L08FirstFailure,
                        bytes,
                    );
                    captured
                },
                |attempt| {
                    close_fixture_note_with(
                        &mut ScriptedNoteClose {
                            present: false,
                            ..Default::default()
                        },
                        attempt,
                    )
                },
            );
            let (result, evidence) = qualify_query_case_result("L08", completed.result);
            assert!(result.is_ok());
            assert!(evidence.is_some());
            append_query_case_result_with_diagnostics(
                &mut report,
                "L08",
                Instant::now(),
                result,
                evidence,
                None,
                directory.path(),
                &directory.path().join("trace.log"),
                &directory.path().join("markers.json"),
                AcceptanceHotkey::F11,
                200,
                completed.diagnostics,
            );
            assert_eq!(report.cases[0].status, CaseStatus::Passed);
            assert!(report.cases[0].artifacts.is_empty());
            assert!(report.artifacts.is_empty());
            assert_eq!(report.query_evidence.len(), 1);
            assert!(
                !directory
                    .path()
                    .join("case-L08-after-note-cleanup-unqualified.json")
                    .exists()
            );
        }
    }

    #[test]
    fn l08_initial_close_failure_is_persisted_before_cleanup_and_prevents_move_dispatch() {
        let directory = tempfile::tempdir().unwrap();
        let mut report = test_acceptance_report(AcceptanceHotkey::F11);
        let mut operation = L08OperationDiagnostic::default();
        let backend = RefCell::new(ScriptedNoteClose {
            focus_after_polls: usize::MAX,
            poll_step: Duration::from_secs(1),
            ..Default::default()
        });
        let order = RefCell::new(Vec::new());
        let result: Result<String, CaseFailure> = run_l08_operation_with(
            &mut operation,
            || {
                order.borrow_mut().push("ensure_initial");
                Ok(())
            },
            |attempt| {
                order.borrow_mut().push("initial_close");
                close_fixture_note_with(&mut *backend.borrow_mut(), attempt)
            },
            |_| {
                order.borrow_mut().push("move_and_dispatch");
                Ok("unreachable placement".into())
            },
        );
        let primary = result.as_ref().unwrap_err().clone();
        let original_bytes = RefCell::new(Vec::new());
        let completed = finish_l08_with_cleanup(
            result,
            operation,
            |context| {
                order.borrow_mut().push("capture_and_persist");
                assert_eq!(context.operation.phase, L08OperationPhase::InitialNoteClose);
                assert_eq!(context.operation.move_requested, None);
                assert!(!context.operation.note_query_requested);
                assert_eq!(context.cleanup_result, None);
                assert_eq!(
                    context.cleanup_note_close.phase,
                    NoteClosePhase::NotAttempted
                );
                let mut captured = PrecleanupDiagnosticArtifacts::default();
                let bytes =
                    bounded_l08_diagnostic_bytes(&mut l08_first_failure_fixture(&report, context))
                        .unwrap();
                *original_bytes.borrow_mut() = bytes.clone();
                retain_l08_capture_piece(
                    &mut captured,
                    directory.path(),
                    PrivateCaseDiagnosticFile::L08FirstFailure,
                    Ok(bytes),
                );
                assert_eq!(captured.paths.len(), 1);
                captured
            },
            |attempt| {
                order.borrow_mut().push("ensure_cleanup_and_close");
                let before = directory
                    .path()
                    .join("case-L08-before-note-cleanup-unqualified.json");
                assert_eq!(fs::read(&before).unwrap(), *original_bytes.borrow());
                let mut backend = backend.borrow_mut();
                backend.focus_after_polls = 0;
                backend.close_after_polls = 1;
                close_fixture_note_with(&mut *backend, attempt)
            },
        );
        assert_eq!(
            *order.borrow(),
            [
                "ensure_initial",
                "initial_close",
                "capture_and_persist",
                "ensure_cleanup_and_close"
            ]
        );
        assert_eq!(completed.result.as_ref().unwrap_err().stage, primary.stage);
        assert_eq!(
            completed.result.as_ref().unwrap_err().message,
            primary.message
        );
        let context = &completed.diagnostics.as_ref().unwrap().l08;
        assert!(context.cleanup_result.as_ref().unwrap().succeeded);
        assert_eq!(
            context.cleanup_note_close.outcome,
            NoteCloseMeasurement::Observed(())
        );
        assert!(!context.original_operation.succeeded);
        assert_eq!(
            backend.borrow().escape_calls,
            1,
            "only the mandatory cleanup inserted Escape"
        );
        append_query_case_result_with_diagnostics(
            &mut report,
            "L08",
            Instant::now(),
            completed.result,
            None,
            None,
            directory.path(),
            &directory.path().join("after-cleanup-trace.log"),
            &directory.path().join("markers.json"),
            AcceptanceHotkey::F11,
            200,
            completed.diagnostics,
        );
        assert_eq!(report.cases[0].status, CaseStatus::Failed);
        assert_eq!(report.cases[0].failure_stage, Some(primary.stage));
        assert!(report.query_evidence.is_empty());
        let first = directory
            .path()
            .join("case-L08-before-note-cleanup-unqualified.json");
        assert_eq!(fs::read(&first).unwrap(), *original_bytes.borrow());
        assert_eq!(report.cases[0].artifacts.len(), 2);
        assert!(
            report.cases[0]
                .artifacts
                .iter()
                .all(|path| Path::new(path).is_file())
        );
        assert!(!directory.path().join("case-L08.png").exists());
        assert!(!directory.path().join("case-L08-windows.json").exists());
        let value: serde_json::Value = serde_json::from_slice(&fs::read(first).unwrap()).unwrap();
        assert_eq!(value["qualification"], "UNQUALIFIED");
        assert_eq!(value["phase"], "before-note-cleanup");
        assert_eq!(
            value["status"],
            serde_json::to_value(CaseStatus::Failed).unwrap()
        );
        assert!(value["context"]["cleanup_result"].is_null());
        assert!(value["context"]["operation"]["moved_root"].is_null());
        assert!(serde_json::from_value::<QueryCaseEvidence>(value).is_err());
    }

    #[test]
    fn l08_operation_capture_and_cleanup_errors_preserve_primary_and_run_teardown_once() {
        for operation_fails in [false, true] {
            for cleanup_fails in [false, true] {
                for capture_fails in [false, true] {
                    let order = RefCell::new(Vec::new());
                    let result = if operation_fails {
                        Err(failure(
                            FailureStage::NativeRootState,
                            "primary owned editor failure",
                        ))
                    } else {
                        Ok("placement completed")
                    };
                    let completed = finish_l08_with_cleanup(
                        result,
                        L08OperationDiagnostic::default(),
                        |context| {
                            order.borrow_mut().push("capture");
                            assert_eq!(context.original_operation.succeeded, !operation_fails);
                            assert_eq!(context.cleanup_result, None);
                            if capture_fails {
                                let mut captured = PrecleanupDiagnosticArtifacts::default();
                                retain_l08_capture_piece(
                                    &mut captured,
                                    Path::new("unused"),
                                    PrivateCaseDiagnosticFile::L08FirstFailure,
                                    Err("serialize first-failure diagnostic failed".into()),
                                );
                                captured
                            } else {
                                PrecleanupDiagnosticArtifacts::default()
                            }
                        },
                        |attempt| {
                            order.borrow_mut().push("cleanup");
                            let mut backend = ScriptedNoteClose {
                                root_focus_error: cleanup_fails,
                                ..Default::default()
                            };
                            close_fixture_note_with(&mut backend, attempt)
                        },
                    );
                    assert_eq!(
                        order
                            .borrow()
                            .iter()
                            .filter(|phase| **phase == "cleanup")
                            .count(),
                        1
                    );
                    assert_eq!(order.borrow().first().copied(), Some("capture"));
                    let diagnostic = completed.diagnostics.as_ref().unwrap();
                    assert!(diagnostic.precleanup_artifacts.is_some());
                    assert_eq!(diagnostic.capture_errors.is_empty(), !capture_fails);
                    assert_eq!(
                        diagnostic.l08.cleanup_result.as_ref().unwrap().succeeded,
                        !cleanup_fails
                    );
                    match &completed.result {
                        Ok(_) => assert!(!operation_fails && !cleanup_fails),
                        Err(error) if operation_fails => {
                            assert_eq!(error.stage, FailureStage::NativeRootState);
                            assert!(error.message.starts_with("primary owned editor failure"));
                            assert_eq!(
                                error.message.contains("fixture note cleanup:"),
                                cleanup_fails
                            );
                            assert_eq!(
                                error.message.contains("pre-note-cleanup capture:"),
                                capture_fails
                            );
                        }
                        Err(error) => {
                            assert!(cleanup_fails);
                            assert_eq!(error.stage, FailureStage::Cleanup);
                            assert_eq!(
                                error.message.contains("pre-note-cleanup capture:"),
                                capture_fails
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn l08_private_receipt_bounds_partial_measurements_and_keeps_values_out_of_report() {
        let directory = tempfile::tempdir().unwrap();
        let mut report = test_acceptance_report(AcceptanceHotkey::F11);
        let completed = finish_l08_with_cleanup::<String>(
            Err(failure(
                FailureStage::WindowDiscovery,
                "actual ROOT preflight failure",
            )),
            L08OperationDiagnostic::default(),
            |context| {
                let mut captured = PrecleanupDiagnosticArtifacts::default();
                let private = b"private candidate-owned editor value: SECRET-NOTE-ONLY".to_vec();
                let tree = retain_l08_capture_piece(
                    &mut captured,
                    directory.path(),
                    PrivateCaseDiagnosticFile::L08Tree,
                    Ok(private),
                );
                let safe_trace = safe_trace_excerpt(
                    r#"trace_event="root_command" command="show" invocation_id=124 request_id=126 hwnd=42 process_id=202 value="SECRET-NOTE-ONLY""#,
                );
                assert_eq!(
                    trace_field_value(&safe_trace, "trace_event"),
                    Some("root_command")
                );
                assert_eq!(trace_field_value(&safe_trace, "invocation_id"), Some("124"));
                assert!(!safe_trace.contains("SECRET-NOTE-ONLY"));
                let trace = retain_l08_capture_piece(
                    &mut captured,
                    directory.path(),
                    PrivateCaseDiagnosticFile::L08Trace,
                    Ok(safe_trace.into_bytes()),
                );
                let mut diagnostic = l08_first_failure_fixture(&report, context);
                diagnostic.private_tree = tree;
                diagnostic.safe_trace = trace;
                diagnostic.capture_errors = vec!["bounded capture detail ".repeat(10000); 100];
                diagnostic.context.operation.initial_note_close.phases = vec![
                        NoteClosePhaseObservation {
                            phase: NoteClosePhase::WaitingForEditorFocus,
                            elapsed_ms: 3
                        };
                        1000
                    ];
                diagnostic.context.original_operation.detail =
                    "bounded primary detail ".repeat(10000);
                let bytes = bounded_l08_diagnostic_bytes(&mut diagnostic).unwrap();
                assert!(bytes.len() <= MAX_PRIVATE_LOG_BYTES);
                assert_eq!(diagnostic.capture_errors.len(), 8);
                assert_eq!(diagnostic.capture_errors_omitted, 92);
                assert_eq!(
                    diagnostic.context.operation.initial_note_close.phases.len(),
                    MAX_NOTE_CLOSE_PHASES
                );
                assert_eq!(
                    diagnostic
                        .context
                        .operation
                        .initial_note_close
                        .phases_omitted,
                    1000 - MAX_NOTE_CLOSE_PHASES
                );
                let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    value["context"]["operation"]["initial_note_close"]["focus_request"],
                    "NotAttempted"
                );
                assert_eq!(
                    value["context"]["operation"]["initial_note_close"]["escape_inserted"],
                    "NotAttempted"
                );
                assert_eq!(value["root_crop"], "NotAttempted");
                assert_eq!(
                    value["private_tree"]["Retained"]["file_name"],
                    "case-L08-before-note-cleanup-private-tree.log"
                );
                assert!(!String::from_utf8_lossy(&bytes).contains("SECRET-NOTE-ONLY"));
                retain_l08_capture_piece(
                    &mut captured,
                    directory.path(),
                    PrivateCaseDiagnosticFile::L08FirstFailure,
                    Ok(bytes),
                );
                captured
            },
            |attempt| {
                close_fixture_note_with(
                    &mut ScriptedNoteClose {
                        present: false,
                        ..Default::default()
                    },
                    attempt,
                )
            },
        );
        append_query_case_result_with_diagnostics(
            &mut report,
            "L08",
            Instant::now(),
            completed.result,
            None,
            None,
            directory.path(),
            &directory.path().join("trace.log"),
            &directory.path().join("markers.json"),
            AcceptanceHotkey::F11,
            200,
            completed.diagnostics,
        );
        let public = serde_json::to_string(&report).unwrap();
        assert!(!public.contains("SECRET-NOTE-ONLY"));
        assert!(report.query_evidence.is_empty());
        assert!(
            report
                .cases
                .iter()
                .all(|case| case.status == CaseStatus::Failed)
        );
    }

    #[test]
    fn l08_shared_private_writer_rejects_bad_case_size_directory_and_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        assert!(
            persist_private_case_diagnostic(
                directory.path(),
                PrivateCaseDiagnosticFile::Hotkey("H15/../L08"),
                b"{}"
            )
            .is_err()
        );
        assert!(
            persist_private_case_diagnostic(
                directory.path(),
                PrivateCaseDiagnosticFile::L08FirstFailure,
                b""
            )
            .is_err()
        );
        assert!(
            persist_private_case_diagnostic(
                directory.path(),
                PrivateCaseDiagnosticFile::L08FirstFailure,
                &vec![b'x'; MAX_PRIVATE_LOG_BYTES + 1]
            )
            .is_err()
        );
        let path = persist_private_case_diagnostic(
            directory.path(),
            PrivateCaseDiagnosticFile::L08FirstFailure,
            &vec![b'x'; MAX_PRIVATE_LOG_BYTES],
        )
        .unwrap();
        let original = fs::read(&path).unwrap();
        assert_eq!(original.len(), MAX_PRIVATE_LOG_BYTES);
        assert!(
            persist_private_case_diagnostic(
                directory.path(),
                PrivateCaseDiagnosticFile::L08FirstFailure,
                b"overwrite"
            )
            .is_err()
        );
        assert_eq!(fs::read(path).unwrap(), original);
        let not_a_directory = directory.path().join("regular-file");
        fs::write(&not_a_directory, b"owned file").unwrap();
        assert!(
            persist_private_case_diagnostic(
                &not_a_directory,
                PrivateCaseDiagnosticFile::L08Cleanup,
                b"{}"
            )
            .is_err()
        );
        fs::create_dir(
            directory
                .path()
                .join("case-L08-after-note-cleanup-unqualified.json"),
        )
        .unwrap();
        assert!(
            persist_private_case_diagnostic(
                directory.path(),
                PrivateCaseDiagnosticFile::L08Cleanup,
                b"{}"
            )
            .is_err()
        );
        assert!(
            persist_failed_hotkey_diagnostic(directory.path(), "H15", b"{}")
                .unwrap()
                .ends_with("case-H15-unqualified.json")
        );
    }

    #[test]
    fn l08_empty_first_capture_preserves_failed_status_and_suppresses_postcleanup_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let mut report = test_acceptance_report(AcceptanceHotkey::F11);
        let completed = finish_l08_with_cleanup::<String>(
            Err(failure(
                FailureStage::NativeRootState,
                "current note focus failed",
            )),
            L08OperationDiagnostic::default(),
            |_| PrecleanupDiagnosticArtifacts {
                paths: Vec::new(),
                errors: vec!["first capture unavailable".into()],
            },
            |attempt| {
                close_fixture_note_with(
                    &mut ScriptedNoteClose {
                        present: false,
                        ..Default::default()
                    },
                    attempt,
                )
            },
        );
        assert_eq!(
            completed.diagnostics.as_ref().unwrap().precleanup_artifacts,
            Some(Vec::new())
        );
        append_query_case_result_with_diagnostics(
            &mut report,
            "L08",
            Instant::now(),
            completed.result,
            None,
            None,
            directory.path(),
            &directory.path().join("nonexistent-trace.log"),
            &directory.path().join("markers.json"),
            AcceptanceHotkey::F11,
            200,
            completed.diagnostics,
        );
        assert_eq!(report.cases[0].status, CaseStatus::Failed);
        assert_eq!(
            report.cases[0].failure_stage,
            Some(FailureStage::NativeRootState)
        );
        assert!(
            report.cases[0]
                .observed
                .starts_with("current note focus failed")
        );
        assert!(
            report.cases[0]
                .observed
                .contains("first capture unavailable")
        );
        assert_eq!(
            report.cases[0].artifacts.len(),
            1,
            "only separately labeled cleanup facts were retained"
        );
        assert!(
            report.cases[0].artifacts[0].ends_with("case-L08-after-note-cleanup-unqualified.json")
        );
        assert!(report.query_evidence.is_empty());
        assert!(!directory.path().join("case-L08.png").exists());
        assert!(!directory.path().join("case-L08-windows.json").exists());
        assert!(!directory.path().join("case-L08-trace.log").exists());
    }

    #[test]
    fn l08_diagnostic_attachment_keeps_existing_aggregate_artifact_capacity_strict() {
        let directory = tempfile::tempdir().unwrap();
        let mut report = test_acceptance_report(AcceptanceHotkey::F11);
        for _ in 0..crate::MAX_ARTIFACTS {
            report.push_artifact("previous-private-artifact.log");
        }
        let completed = finish_l08_with_cleanup::<String>(
            Err(failure(
                FailureStage::NativeRootState,
                "original L08 failed",
            )),
            L08OperationDiagnostic::default(),
            |_| {
                let mut captured = PrecleanupDiagnosticArtifacts::default();
                retain_l08_capture_piece(
                    &mut captured,
                    directory.path(),
                    PrivateCaseDiagnosticFile::L08FirstFailure,
                    Ok(b"{\"qualification\":\"UNQUALIFIED\"}".to_vec()),
                );
                captured
            },
            |attempt| {
                close_fixture_note_with(
                    &mut ScriptedNoteClose {
                        present: false,
                        ..Default::default()
                    },
                    attempt,
                )
            },
        );
        append_query_case_result_with_diagnostics(
            &mut report,
            "L08",
            Instant::now(),
            completed.result,
            None,
            None,
            directory.path(),
            &directory.path().join("trace.log"),
            &directory.path().join("markers.json"),
            AcceptanceHotkey::F11,
            200,
            completed.diagnostics,
        );
        assert!(report.capacity_saturated);
        assert_eq!(report.artifacts.len(), crate::MAX_ARTIFACTS);
        assert_eq!(report.cases[0].status, CaseStatus::Failed);
        assert!(report.query_evidence.is_empty());
        assert!(report.cases[0].observed.starts_with("original L08 failed"));
    }

    fn h15_chord_fixture() -> (Instant, AcceptanceHotkeyTapEvidence, RunnerChordObservation) {
        let epoch = Instant::now();
        let hotkey = AcceptanceHotkey::ShiftAltWinEnd;
        let keys = hotkey_observer_keys(hotkey);
        let edge = |at, inserted, cleanup: &str| NativeInputEdgeEvidence {
            inserted,
            at_unix_ms: at,
            foreground_hwnd: 1002,
            foreground_pid: 202,
            input_desktop: "thread=Default;active=Default".into(),
            cleanup_status: cleanup.into(),
            keyboard_input: Some(KeyboardInputEvidence {
                vk: 0xA0,
                scan: 0,
                flags: 0,
                extra_info: ACCEPTANCE_RUNNER_INPUT_COOKIE,
                async_state_before: 0,
                async_state_after: -32768,
            }),
        };
        let input = AcceptanceHotkeyTapEvidence {
            down: edge(1700000000000, 4, "not_required_for_down_edge"),
            up: edge(1700000000025, 4, "async_state_clear_after_owned_release"),
            observed_vks: keys.clone(),
        };
        let mut input = input;
        input.up.keyboard_input = Some(KeyboardInputEvidence {
            vk: 0x23,
            scan: 0,
            flags: 3,
            extra_info: ACCEPTANCE_RUNNER_INPUT_COOKIE,
            async_state_before: -32768,
            async_state_after: 0,
        });
        let observation = RunnerChordObservation {
            desktop: "thread=Default;active=Default".into(),
            keys: keys
                .iter()
                .map(|vk| RunnerChordKeyObservation {
                    vk: *vk,
                    down: 1,
                    up: 1,
                    injected_down: 1,
                    injected_up: 1,
                })
                .collect(),
            ordered_edges: expected_hotkey_edges(hotkey, 1)
                .into_iter()
                .enumerate()
                .map(|(index, (vk, down))| RunnerChordEdge {
                    vk,
                    down,
                    injected: true,
                    extra_info: ACCEPTANCE_RUNNER_INPUT_COOKIE,
                    at: epoch + Duration::from_micros(index as u64 * 100),
                })
                .collect(),
            foreign_edges: Vec::new(),
        };
        (epoch, input, observation)
    }

    fn completed_h15_context(packet: &HotkeyCaseEvidence) -> (Instant, H15CaseDiagnosticContext) {
        let hotkey = AcceptanceHotkey::ShiftAltWinEnd;
        let (epoch, input, observation) = h15_chord_fixture();
        let mut attempt = H15AttemptDiagnostic::new(hotkey);
        send_h15_chord_attempt(epoch, &mut attempt, |down, up| {
            down(&input.down);
            up(&input.up);
            Ok(input.clone())
        })
        .unwrap();
        validate_h15_chord_attempt(hotkey, &input, &observation, epoch, &mut attempt).unwrap();
        finish_h15_observer(Ok(()), || Ok(()), || Ok(()), epoch, &mut attempt).unwrap();
        let proof = packet.screen_draw_priority.as_ref().unwrap();
        attempt.baseline_trace_cursor = Some(proof.trace_cursor);
        attempt.baseline_revision = Some(proof.baseline_visibility_revision);
        attempt.baseline_invocation = proof.baseline_invocation_id;
        attempt.baseline_observation = Some(proof.baseline_observation.clone());
        attempt.baseline_established = true;
        attempt.phase = H15AttemptPhase::Complete;
        (
            epoch,
            H15CaseDiagnosticContext {
                original_operation: CaseOperationDiagnostic::from_result(&Ok(
                    "actual completed operation fixture".into(),
                )),
                attempt,
                toolbar: ToolbarObservationDiagnostic {
                    phase: Some(ToolbarObservationPhase::Recovery),
                    boundary_ordinal: proof.trace_cursor,
                    last_trace_ordinal: proof.trace_end,
                    last_admissible: Some(proof.recovered_observation.clone()),
                    rejected: None,
                },
                cleanup: H15CleanupDiagnostic {
                    preclose_capture_attempted: false,
                    preclose_capture_errors: Vec::new(),
                    close: Some(H15OperationDiagnostic::measured(epoch, &Ok(()))),
                },
            },
        )
    }

    fn install_completed_packet_capture(packet: HotkeyCaseEvidence, trace: &Path) {
        begin_hotkey_evidence_capture("H15", trace);
        ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let capture = slot.as_mut().unwrap();
            capture.runner_edges = packet.runner_edges;
            capture.physical_displays = packet.physical_displays;
            capture.screen_draw_priority = packet.screen_draw_priority;
            let identity = &packet.root_identities[0];
            capture.segments.push(HotkeyCaptureSegment {
                stream: identity.stream,
                path: trace.to_owned(),
                cursor: 0,
                end: None,
                materialized_events: Some(packet.candidate_events),
                input_group_id: 1,
                purpose: HotkeyRunnerInputPurpose::LauncherChord,
                root_hwnd: identity.hwnd,
                root_process_id: identity.process_id,
            });
        });
    }

    #[test]
    fn h15_append_final_rejection_retains_completed_actual_context_as_unqualified_after_close() {
        let directory = tempfile::tempdir().unwrap();
        let trace = directory.path().join("live-trace.log");
        fs::write(&trace, "").unwrap();
        let hotkey = AcceptanceHotkey::ShiftAltWinEnd;
        let mut packet = crate::tests::h15_current_owner_packet(hotkey);
        crate::validate_hotkey_evidence_packet_with_context(&packet, hotkey, 350).unwrap();
        let (epoch, context) = completed_h15_context(&packet);
        let expected_context = context.clone();
        packet
            .screen_draw_priority
            .as_mut()
            .unwrap()
            .baseline_visibility_revision = 114;
        install_completed_packet_capture(packet, &trace);
        let mut report = super::super::tests::test_acceptance_report(hotkey);
        append_case_with_diagnostic_context(
            &mut report,
            "H15",
            expected("H15"),
            epoch,
            Ok("completed real entry/chord/close/next-gesture fixture".into()),
            None,
            directory.path(),
            &trace,
            Some(Vec::new()),
            Some(context),
        );
        assert_eq!(report.cases[0].status, CaseStatus::Failed);
        assert_eq!(
            report.cases[0].failure_stage,
            Some(FailureStage::GestureDecision)
        );
        assert!(report.cases[0].observed.contains("baseline scalars"));
        assert!(report.hotkey_evidence.is_empty());
        assert_eq!(report.artifacts.len(), 1);
        let bytes = fs::read(directory.path().join("case-H15-unqualified.json")).unwrap();
        assert!(bytes.len() <= MAX_PRIVATE_LOG_BYTES);
        let diagnostic: FailedHotkeyDiagnostic = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            diagnostic.qualification,
            FailedHotkeyQualification::UNQUALIFIED
        );
        assert_eq!(diagnostic.status, CaseStatus::Failed);
        assert_eq!(diagnostic.failure_stage, FailureStage::GestureDecision);
        assert_eq!(
            diagnostic.capture_boundary,
            FailedHotkeyCaptureBoundary::AfterOwnedCleanupAttempt
        );
        assert!(diagnostic.capture_at_case_relative_us.is_some());
        assert!(diagnostic.original_operation.succeeded);
        assert!(!diagnostic.packet_projection_truncated);
        assert!(diagnostic.omitted.is_none());
        assert!(diagnostic.validation_reason.contains("baseline scalars"));
        assert_eq!(diagnostic.h15.unwrap(), expected_context);
        let proof = diagnostic
            .rejected_packet
            .unwrap()
            .screen_draw_priority
            .unwrap();
        assert_eq!(proof.baseline_visibility_revision, 114);
        assert_eq!(
            proof
                .baseline_observation
                .observation
                .launcher
                .visibility_revision,
            115
        );
        assert_eq!(
            proof
                .recovered_observation
                .observation
                .launcher
                .visibility_revision,
            116
        );
        assert_eq!(
            proof
                .baseline_observation
                .observation
                .launcher
                .invocation_id,
            None
        );
        assert_eq!(
            proof
                .baseline_observation
                .observation
                .launcher
                .root
                .unwrap()
                .hwnd,
            1001
        );
        assert_eq!(
            proof
                .baseline_observation
                .observation
                .launcher
                .parking
                .unwrap()
                .cycle,
            1
        );
        assert!(proof.tool_closed && proof.close_requested && proof.keys_released);
        assert_eq!(proof.runner_edges.len(), 8);
        assert!(
            proof
                .runner_edges
                .iter()
                .all(|edge| edge.runner_cookie_matched && edge.injected)
        );
        assert!(serde_json::from_slice::<HotkeyCaseEvidence>(&bytes).is_err());
        let mut forged = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
        forged["qualification"] = "QUALIFIED".into();
        assert!(serde_json::from_value::<FailedHotkeyDiagnostic>(forged).is_err());
        assert!(!directory.path().join("case-H15.png").exists());
        assert!(!directory.path().join("case-H15-windows.json").exists());
        assert!(
            !directory
                .path()
                .join("case-H15-preclose-observation.json")
                .exists()
        );
        assert!(finish_hotkey_evidence_capture("H15").is_none());
        crate::revalidate_final_r0(&mut report, true);
        assert!(
            report
                .cases
                .iter()
                .any(|case| case.id == "R0" && case.status == CaseStatus::Failed)
        );
        assert!(!crate::aggregate_native_cases_passed(
            true,
            Some(&report),
            true
        ));
    }

    #[test]
    fn h15_append_operation_failure_keeps_foreign_cookie_predicates_primary_and_preclose_cleanup_facts()
     {
        let directory = tempfile::tempdir().unwrap();
        let trace = directory.path().join("live-trace.log");
        fs::write(&trace, "").unwrap();
        let hotkey = AcceptanceHotkey::ShiftAltWinEnd;
        let packet = crate::tests::h15_current_owner_packet(hotkey);
        let (epoch, input, mut observation) = h15_chord_fixture();
        let mut foreign = observation.ordered_edges[0];
        foreign.extra_info = multi_launcher::hotkey::launcher_invocation::MULTI_LAUNCHER_INJECT_TAG;
        observation.foreign_edges.push(foreign);
        let mut attempt = H15AttemptDiagnostic::new(hotkey);
        send_h15_chord_attempt(epoch, &mut attempt, |down, up| {
            down(&input.down);
            up(&input.up);
            Ok(input.clone())
        })
        .unwrap();
        let error = validate_h15_chord_attempt(hotkey, &input, &observation, epoch, &mut attempt)
            .unwrap_err();
        let error =
            finish_h15_observer::<()>(Err(error), || Ok(()), || Ok(()), epoch, &mut attempt)
                .unwrap_err();
        assert_eq!(error.stage, FailureStage::HookAdmission);
        let captured = RefCell::new(H15CleanupDiagnostic::default());
        let original_operation =
            CaseOperationDiagnostic::from_result(&Err(failure(error.stage, error.message.clone())));
        let (result, paths) = finish_h15_with_cleanup(
            Err(error),
            |_| {
                captured.borrow_mut().preclose_capture_attempted = true;
                captured
                    .borrow_mut()
                    .preclose_capture_errors
                    .push("fixture capture failure".into());
                H15PrecloseArtifacts {
                    paths: Vec::new(),
                    errors: vec!["fixture capture failure".into()],
                }
            },
            || {
                let closed = Err("fixture owned-close failure".to_owned());
                captured.borrow_mut().close =
                    Some(H15OperationDiagnostic::measured(epoch, &closed));
                Err(failure(
                    FailureStage::Cleanup,
                    "fixture owned-close failure",
                ))
            },
        );
        let expected_attempt = attempt.clone();
        let mut packet = packet;
        packet
            .screen_draw_priority
            .as_mut()
            .unwrap()
            .baseline_visibility_revision = 114;
        install_completed_packet_capture(packet, &trace);
        let mut report = super::super::tests::test_acceptance_report(hotkey);
        append_case_with_diagnostic_context(
            &mut report,
            "H15",
            expected("H15"),
            epoch,
            result,
            None,
            directory.path(),
            &trace,
            paths,
            Some(H15CaseDiagnosticContext {
                original_operation,
                attempt,
                toolbar: ToolbarObservationDiagnostic::default(),
                cleanup: captured.into_inner(),
            }),
        );
        assert_eq!(report.cases[0].status, CaseStatus::Failed);
        assert_eq!(
            report.cases[0].failure_stage,
            Some(FailureStage::HookAdmission)
        );
        assert!(report.cases[0].observed.contains("Screen Draw cleanup"));
        assert!(
            report.cases[0]
                .observed
                .contains("pre-close capture diagnostics")
        );
        assert!(report.cases[0].observed.contains("typed H evidence"));
        let diagnostic: FailedHotkeyDiagnostic = serde_json::from_slice(
            &fs::read(directory.path().join("case-H15-unqualified.json")).unwrap(),
        )
        .unwrap();
        assert!(!diagnostic.original_operation.succeeded);
        assert_eq!(
            diagnostic.original_operation.stage,
            Some(FailureStage::HookAdmission)
        );
        let retained = diagnostic.h15.unwrap();
        assert_eq!(retained.attempt, expected_attempt);
        assert!(retained.cleanup.preclose_capture_attempted);
        assert_eq!(
            retained.cleanup.preclose_capture_errors,
            ["fixture capture failure"]
        );
        assert!(!retained.cleanup.close.unwrap().succeeded);
        let predicates = retained.attempt.predicates.unwrap();
        assert!(
            predicates.exact_injected_pairs
                && predicates.exact_injected_sequence
                && predicates.exact_configured_key_set
        );
        assert!(!predicates.no_foreign_edges);
        let observed = retained.attempt.observation.unwrap();
        assert_eq!(observed.foreign_edges.len(), 1);
        assert_eq!(
            observed.foreign_edges[0].cookie,
            multi_launcher::hotkey::launcher_invocation::MULTI_LAUNCHER_INJECT_TAG as u64
        );
        assert_eq!(
            observed.foreign_edges[0].cookie_owner,
            DiagnosticInputCookie::MultiLauncherInjected
        );
        assert_eq!(observed.ordered_edges.len(), 8);
        assert!(report.hotkey_evidence.is_empty());
    }

    #[test]
    fn h15_append_qualified_completed_packet_has_one_proof_and_no_failed_diagnostic() {
        let directory = tempfile::tempdir().unwrap();
        let trace = directory.path().join("live-trace.log");
        fs::write(&trace, "").unwrap();
        let hotkey = AcceptanceHotkey::ShiftAltWinEnd;
        let packet = crate::tests::h15_current_owner_packet(hotkey);
        let (epoch, context) = completed_h15_context(&packet);
        install_completed_packet_capture(packet, &trace);
        let mut report = super::super::tests::test_acceptance_report(hotkey);
        append_case_with_diagnostic_context(
            &mut report,
            "H15",
            expected("H15"),
            epoch,
            Ok("completed".into()),
            None,
            directory.path(),
            &trace,
            Some(Vec::new()),
            Some(context),
        );
        assert_eq!(report.cases[0].status, CaseStatus::Passed);
        assert_eq!(report.hotkey_evidence.len(), 1);
        crate::validate_hotkey_evidence_packet_with_context(
            &report.hotkey_evidence[0],
            hotkey,
            350,
        )
        .unwrap();
        assert!(report.artifacts.is_empty());
        assert!(!directory.path().join("case-H15-unqualified.json").exists());
        assert!(finish_hotkey_evidence_capture("H15").is_none());
    }

    #[test]
    fn h15_append_oversized_rejected_packet_records_actual_omissions_under_private_cap() {
        let directory = tempfile::tempdir().unwrap();
        let trace = directory.path().join("live-trace.log");
        fs::write(&trace, "").unwrap();
        let hotkey = AcceptanceHotkey::ShiftAltWinEnd;
        let mut packet = crate::tests::h15_current_owner_packet(hotkey);
        let (epoch, context) = completed_h15_context(&packet);
        let template = packet.candidate_events.last().unwrap().clone();
        for index in 0..1500 {
            let mut event = template.clone();
            event.event_ordinal = 200 + index;
            event.elapsed_ms = 200 + u64::from(index);
            packet.candidate_events.push(event);
        }
        install_completed_packet_capture(packet, &trace);
        let mut report = super::super::tests::test_acceptance_report(hotkey);
        append_case_with_diagnostic_context(
            &mut report,
            "H15",
            expected("H15"),
            epoch,
            Ok("completed".into()),
            None,
            directory.path(),
            &trace,
            Some(Vec::new()),
            Some(context.clone()),
        );
        let bytes = fs::read(directory.path().join("case-H15-unqualified.json")).unwrap();
        assert!(bytes.len() <= MAX_PRIVATE_LOG_BYTES);
        let diagnostic: FailedHotkeyDiagnostic = serde_json::from_slice(&bytes).unwrap();
        assert!(diagnostic.packet_projection_truncated);
        assert_eq!(diagnostic.collected.unwrap().candidate_events, 1506);
        assert_eq!(diagnostic.omitted.unwrap().candidate_events, 1490);
        assert_eq!(
            diagnostic.rejected_packet.unwrap().candidate_events.len(),
            16
        );
        assert_eq!(diagnostic.h15.unwrap(), context);
        assert!(report.hotkey_evidence.is_empty());
        assert_eq!(report.cases[0].status, CaseStatus::Failed);
    }

    #[test]
    fn h15_append_diagnostic_create_failure_stays_secondary_and_never_overwrites_existing_artifact()
    {
        let directory = tempfile::tempdir().unwrap();
        let trace = directory.path().join("live-trace.log");
        fs::write(&trace, "").unwrap();
        let destination = directory.path().join("case-H15-unqualified.json");
        fs::write(&destination, "existing private artifact").unwrap();
        let hotkey = AcceptanceHotkey::ShiftAltWinEnd;
        let mut packet = crate::tests::h15_current_owner_packet(hotkey);
        let (epoch, context) = completed_h15_context(&packet);
        packet
            .screen_draw_priority
            .as_mut()
            .unwrap()
            .baseline_visibility_revision = 114;
        install_completed_packet_capture(packet, &trace);
        let mut report = super::super::tests::test_acceptance_report(hotkey);
        append_case_with_diagnostic_context(
            &mut report,
            "H15",
            expected("H15"),
            epoch,
            Ok("completed".into()),
            None,
            directory.path(),
            &trace,
            Some(Vec::new()),
            Some(context),
        );
        assert_eq!(
            report.cases[0].failure_stage,
            Some(FailureStage::GestureDecision)
        );
        assert!(
            report.cases[0]
                .observed
                .starts_with("typed H evidence packet failed validation")
        );
        assert!(
            report.cases[0]
                .observed
                .contains("unqualified diagnostic capture")
        );
        assert_eq!(
            fs::read_to_string(destination).unwrap(),
            "existing private artifact"
        );
        assert!(report.artifacts.is_empty());
        assert!(report.hotkey_evidence.is_empty());
        assert!(persist_failed_hotkey_diagnostic(directory.path(), "H15/foreign", b"{}").is_err());
        assert!(persist_failed_hotkey_diagnostic(directory.path(), "H15", b"").is_err());
        assert!(
            persist_failed_hotkey_diagnostic(
                directory.path(),
                "H15",
                &vec![b' '; MAX_PRIVATE_LOG_BYTES + 1]
            )
            .is_err()
        );
    }

    #[test]
    fn h15_attempt_retains_all_four_independent_failed_predicates_before_hook_admission_return() {
        let directory = tempfile::tempdir().unwrap();
        let trace = directory.path().join("fixture-trace.log");
        fs::write(&trace, "").unwrap();
        for broken in 0..4 {
            begin_hotkey_evidence_capture("H15", &trace);
            let (epoch, mut input, mut observation) = h15_chord_fixture();
            match broken {
                0 => observation.keys[0].down = 2,
                1 => observation.ordered_edges.swap(0, 1),
                2 => observation.foreign_edges.push(RunnerChordEdge {
                    vk: 0x23,
                    down: true,
                    injected: true,
                    extra_info:
                        multi_launcher::hotkey::launcher_invocation::MULTI_LAUNCHER_INJECT_TAG,
                    at: epoch + Duration::from_micros(350),
                }),
                3 => input.observed_vks.pop().map(|_| ()).unwrap(),
                _ => unreachable!(),
            }
            let mut attempt = H15AttemptDiagnostic::new(AcceptanceHotkey::ShiftAltWinEnd);
            let error = validate_h15_chord_attempt(
                AcceptanceHotkey::ShiftAltWinEnd,
                &input,
                &observation,
                epoch,
                &mut attempt,
            )
            .unwrap_err();
            assert_eq!(error.stage, FailureStage::HookAdmission);
            assert_eq!(
                error.message,
                "H15 recovery input was not one exact owned configured chord"
            );
            assert_eq!(attempt.phase, H15AttemptPhase::ChordValidation);
            let measured = attempt.predicates.as_ref().unwrap();
            let results = [
                measured.exact_injected_pairs,
                measured.exact_injected_sequence,
                measured.no_foreign_edges,
                measured.exact_configured_key_set,
            ];
            for (index, passed) in results.into_iter().enumerate() {
                assert_eq!(passed, index != broken);
            }
            assert!(measured.checked_at_us.is_some());
            assert_eq!(
                attempt
                    .input
                    .as_ref()
                    .unwrap()
                    .down
                    .as_ref()
                    .unwrap()
                    .inserted,
                4
            );
            assert_eq!(
                attempt
                    .input
                    .as_ref()
                    .unwrap()
                    .up
                    .as_ref()
                    .unwrap()
                    .cleanup_status,
                "async_state_clear_after_owned_release"
            );
            assert_eq!(attempt.observation.as_ref().unwrap().total_ordered_edges, 8);
            let round_trip: H15AttemptDiagnostic =
                serde_json::from_slice(&serde_json::to_vec(&attempt).unwrap()).unwrap();
            assert_eq!(round_trip, attempt);
            ACTIVE_HOTKEY_EVIDENCE_CAPTURE.with(|slot| {
                assert!(
                    slot.borrow()
                        .as_ref()
                        .unwrap()
                        .screen_draw_priority
                        .is_none()
                )
            });
        }
    }

    #[test]
    fn h15_attempt_foreign_matching_app_untagged_and_other_cookies_remain_distinct_and_rejected() {
        for (cookie, expected_owner) in [
            (
                multi_launcher::hotkey::launcher_invocation::MULTI_LAUNCHER_INJECT_TAG,
                DiagnosticInputCookie::MultiLauncherInjected,
            ),
            (0, DiagnosticInputCookie::Untagged),
            (0x1234, DiagnosticInputCookie::Other),
        ] {
            let (epoch, input, mut observation) = h15_chord_fixture();
            let mut counts = hotkey_observer_keys(AcceptanceHotkey::ShiftAltWinEnd)
                .into_iter()
                .map(|vk| (vk, [1; 4]))
                .collect();
            record_runner_chord_edge(
                RunnerHookEdge {
                    vk: 0x23,
                    down: true,
                    injected: true,
                    extra_info: cookie,
                    at: epoch + Duration::from_micros(350),
                },
                &mut counts,
                &mut observation.ordered_edges,
                &mut observation.foreign_edges,
            );
            assert_eq!(observation.ordered_edges.len(), 8);
            assert_eq!(observation.foreign_edges.len(), 1);
            let mut attempt = H15AttemptDiagnostic::new(AcceptanceHotkey::ShiftAltWinEnd);
            assert_eq!(
                validate_h15_chord_attempt(
                    AcceptanceHotkey::ShiftAltWinEnd,
                    &input,
                    &observation,
                    epoch,
                    &mut attempt
                )
                .unwrap_err()
                .stage,
                FailureStage::HookAdmission
            );
            let observed = attempt.observation.unwrap();
            assert_eq!(observed.foreign_edges[0].cookie_owner, expected_owner);
            assert_eq!(observed.foreign_edges[0].cookie, cookie as u64);
            assert_eq!(observed.foreign_edges[0].relative_us, Some(350));
            assert!(
                observed
                    .ordered_edges
                    .iter()
                    .all(|edge| edge.cookie_owner == DiagnosticInputCookie::RunnerOwned)
            );
        }
    }

    #[test]
    fn h15_partial_send_wait_and_observer_cleanup_keep_measurements_absent_and_primary_failure_owned()
     {
        let (epoch, input, observation) = h15_chord_fixture();
        for partial in [false, true] {
            let mut attempt = H15AttemptDiagnostic::new(AcceptanceHotkey::ShiftAltWinEnd);
            let sent = send_h15_chord_attempt(epoch, &mut attempt, |down, up| {
                if partial {
                    down(&input.down);
                    up(&input.up);
                }
                Err("actual insertion owner rejected the attempt; owned release attempted".into())
            });
            let primary_stage = sent.as_ref().err().unwrap().stage;
            let calls = std::cell::RefCell::new(Vec::new());
            let result = finish_h15_observer(
                sent,
                || {
                    calls.borrow_mut().push("release check");
                    Err("owned release acknowledgement missing".into())
                },
                || {
                    calls.borrow_mut().push("observer stop");
                    Err("unhook acknowledgement missing".into())
                },
                epoch,
                &mut attempt,
            );
            assert_eq!(*calls.borrow(), ["release check", "observer stop"]);
            let error = result.unwrap_err();
            assert_eq!(error.stage, primary_stage);
            assert!(error.message.starts_with("actual insertion owner rejected"));
            assert!(
                error.message.contains("key release check")
                    && error.message.contains("observer cleanup")
            );
            assert_eq!(attempt.phase, H15AttemptPhase::ChordSend);
            assert!(attempt.observation.is_none() && attempt.predicates.is_none());
            let native = attempt.input.as_ref().unwrap();
            assert_eq!(native.down.is_some(), partial);
            assert_eq!(native.up.is_some(), partial);
            assert!(native.observed_vks.is_none());
            assert!(!attempt.key_release_check.as_ref().unwrap().succeeded);
            assert!(!attempt.observer_stop.as_ref().unwrap().succeeded);
            assert!(
                attempt.key_release_check.as_ref().unwrap().checked_at_us
                    <= attempt.observer_stop.as_ref().unwrap().checked_at_us
            );
        }
        // A bounded wait can return an incomplete measured observation. It is
        // retained as measured, rather than fabricated into a complete chord.
        let mut incomplete = observation;
        incomplete.ordered_edges.clear();
        for key in &mut incomplete.keys {
            key.down = 0;
            key.up = 0;
            key.injected_down = 0;
            key.injected_up = 0;
        }
        let mut attempt = H15AttemptDiagnostic::new(AcceptanceHotkey::ShiftAltWinEnd);
        let sent = validate_h15_chord_attempt(
            AcceptanceHotkey::ShiftAltWinEnd,
            &input,
            &incomplete,
            epoch,
            &mut attempt,
        );
        let error = finish_h15_observer(
            sent,
            || Ok(()),
            || Err("stop failed".into()),
            epoch,
            &mut attempt,
        )
        .unwrap_err();
        assert_eq!(error.stage, FailureStage::HookAdmission);
        assert_eq!(attempt.observation.as_ref().unwrap().total_ordered_edges, 0);
        assert!(!attempt.predicates.as_ref().unwrap().exact_injected_pairs);
        assert!(!attempt.predicates.as_ref().unwrap().exact_injected_sequence);
        assert!(attempt.key_release_check.unwrap().succeeded);
    }

    #[test]
    fn h15_actual_owned_release_failure_keeps_partial_native_receipts_and_stops_the_observer() {
        let (epoch, input, _observation) = h15_chord_fixture();
        let mut guard = OwnedKeyboardReleaseGuard::new();
        guard.owned = acceptance_hotkey_keys(AcceptanceHotkey::ShiftAltWinEnd);
        let mut attempt = H15AttemptDiagnostic::new(AcceptanceHotkey::ShiftAltWinEnd);
        let calls = std::cell::RefCell::new(Vec::new());
        let sent = send_h15_chord_attempt(epoch, &mut attempt, |down, up| {
            calls.borrow_mut().push("down measured");
            down(&input.down);
            guard.release_with(
                |events, _| {
                    assert_eq!(events.len(), 4);
                    let mut prefix = input.up.clone();
                    prefix.inserted = 1;
                    prefix.cleanup_status =
                        "release_prefix_inserted;owned_guard_cleanup_pending".into();
                    calls.borrow_mut().push("up prefix measured");
                    up(&prefix);
                    Err((1, "owned key-up prefix failed".into()))
                },
                |_| panic!("partial release must enter cleanup before successful verification"),
                |owned| {
                    calls.borrow_mut().push("owned cleanup");
                    assert_eq!(owned.len(), 3);
                    let releases = owned.iter().rev().copied().collect::<Vec<_>>();
                    assert_eq!(
                        retire_inserted_keyboard_ups(owned, &releases, releases.len()),
                        3
                    );
                    "remaining three owned key-ups inserted".into()
                },
                |_| Vec::new(),
            )?;
            unreachable!("the actual guard must retain its failed release result")
        });
        assert!(guard.owned.is_empty());
        let primary = sent.as_ref().err().unwrap().stage;
        let result = finish_h15_observer(
            sent,
            || {
                calls.borrow_mut().push("release check");
                assert!(guard.owned.is_empty());
                Ok(())
            },
            || {
                calls.borrow_mut().push("observer stop");
                Ok(())
            },
            epoch,
            &mut attempt,
        );
        let error = result.unwrap_err();
        assert_eq!(error.stage, primary);
        assert!(error.message.contains("owned key-up prefix failed"));
        assert!(error.message.contains("release_inserted=1/4"));
        assert_eq!(
            *calls.borrow(),
            [
                "down measured",
                "up prefix measured",
                "owned cleanup",
                "release check",
                "observer stop"
            ]
        );
        let native = attempt.input.unwrap();
        assert_eq!(native.down.unwrap().inserted, 4);
        assert_eq!(native.up.unwrap().inserted, 1);
        assert!(!native.send.unwrap().succeeded);
        assert!(
            attempt.key_release_check.unwrap().succeeded
                && attempt.observer_stop.unwrap().succeeded
        );
        assert!(attempt.observation.is_none() && attempt.predicates.is_none());
    }

    #[test]
    fn h15_failed_chord_preclose_receipt_is_phase_owned_persisted_before_close_and_keeps_cleanup_separate()
     {
        let (epoch, input, mut observation) = h15_chord_fixture();
        observation.foreign_edges.push(RunnerChordEdge {
            vk: 0x23,
            down: true,
            injected: true,
            extra_info: multi_launcher::hotkey::launcher_invocation::MULTI_LAUNCHER_INJECT_TAG,
            at: epoch + Duration::from_micros(350),
        });
        let mut attempt = H15AttemptDiagnostic::new(AcceptanceHotkey::ShiftAltWinEnd);
        let sent = validate_h15_chord_attempt(
            AcceptanceHotkey::ShiftAltWinEnd,
            &input,
            &observation,
            epoch,
            &mut attempt,
        );
        let failed = finish_h15_observer(sent, || Ok(()), || Ok(()), epoch, &mut attempt)
            .map(|_| String::new());
        let history = ToolbarObservationDiagnostic {
            phase: Some(ToolbarObservationPhase::Drawing),
            rejected: Some("earlier Drawing poll was not yet ready".into()),
            ..Default::default()
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("case-H15-preclose-observation.json");
        let (result, artifacts) = finish_h15_with_cleanup(
            failed,
            |primary| {
                let inventory =
                    h15_preclose_inventory(&history, &attempt, primary, 202, &Ok(None), None, &[]);
                let mut captured = H15PrecloseArtifacts::default();
                persist_h15_preclose_piece(
                    &mut captured,
                    directory.path(),
                    "observation.json",
                    Ok(serde_json::to_vec_pretty(&inventory).unwrap()),
                );
                captured
            },
            || {
                assert!(
                    path.is_file(),
                    "attempt must be persisted while the owned session is alive"
                );
                let value: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                let retained: H15AttemptDiagnostic =
                    serde_json::from_value(value["attempted_chord"].clone()).unwrap();
                assert_eq!(retained, attempt);
                assert_eq!(value["failed_phase"], "chord_validation");
                assert_eq!(value["observation_role"], "toolbar_polling_history");
                assert_eq!(value["observation"]["phase"], "Drawing");
                assert!(!retained.predicates.unwrap().no_foreign_edges);
                Err(failure(FailureStage::Cleanup, "owned close failed"))
            },
        );
        assert_eq!(artifacts.unwrap(), [path]);
        let error = result.unwrap_err();
        assert_eq!(error.stage, FailureStage::HookAdmission);
        assert!(
            error
                .message
                .starts_with("H15 recovery input was not one exact owned configured chord")
        );
        assert!(
            error
                .message
                .contains("Screen Draw cleanup: owned close failed")
        );
    }

    #[test]
    fn h15_attempt_bounds_total_json_and_collections_without_losing_truncation_or_clock_failures() {
        let (epoch, mut input, mut observation) = h15_chord_fixture();
        input.down.at_unix_ms = u128::MAX;
        input.down.input_desktop = "\u{1f642}".repeat(100_000);
        input.up.cleanup_status = "\u{03b2}".repeat(100_000);
        input.observed_vks = vec![0x23; 100_000];
        observation.desktop = "\u{1f642}".repeat(100_000);
        observation.keys = vec![observation.keys[0].clone(); 100_000];
        observation.ordered_edges = vec![observation.ordered_edges[0]; 100_000];
        observation.foreign_edges = vec![
            RunnerChordEdge {
                vk: 0x23,
                down: true,
                injected: true,
                extra_info: 0,
                at: epoch - Duration::from_micros(1)
            };
            100_000
        ];
        let mut attempt = H15AttemptDiagnostic::new(AcceptanceHotkey::ShiftAltWinEnd);
        let error = validate_h15_chord_attempt(
            AcceptanceHotkey::ShiftAltWinEnd,
            &input,
            &observation,
            epoch,
            &mut attempt,
        )
        .unwrap_err();
        let measured = attempt.observation.as_ref().unwrap();
        assert_eq!(
            (
                measured.keys.len(),
                measured.ordered_edges.len(),
                measured.foreign_edges.len()
            ),
            (8, 32, 32)
        );
        assert_eq!(
            (
                measured.total_keys,
                measured.total_ordered_edges,
                measured.total_foreign_edges
            ),
            (100_000, 100_000, 100_000)
        );
        assert!(
            measured
                .foreign_edges
                .iter()
                .all(|edge| edge.before_attempt && edge.relative_us.is_none())
        );
        let native = attempt.input.as_ref().unwrap();
        assert_eq!(native.observed_vks.as_ref().unwrap().len(), 8);
        assert_eq!(native.total_observed_vks, Some(100_000));
        assert!(
            native.down.as_ref().unwrap().timestamp_overflow
                && native.down.as_ref().unwrap().at_unix_ms.is_none()
        );
        assert_eq!(native.down.as_ref().unwrap().input_desktop_chars, 100_000);
        assert_eq!(
            native.down.as_ref().unwrap().input_desktop.chars().count(),
            128
        );
        assert_eq!(native.up.as_ref().unwrap().cleanup_status_chars, 100_000);
        assert_eq!(measured.desktop_chars, 100_000);
        let value = h15_preclose_inventory(
            &ToolbarObservationDiagnostic::default(),
            &attempt,
            &error,
            202,
            &Ok(None),
            None,
            &[],
        );
        let bytes = serde_json::to_vec_pretty(&value).unwrap();
        assert!(bytes.len() < MAX_PRIVATE_LOG_BYTES);
        let wire: H15AttemptDiagnostic =
            serde_json::from_value(value["attempted_chord"].clone()).unwrap();
        assert_eq!(wire, attempt);
        for bad in ["-1", "1.5", "18446744073709551616"] {
            let bad = serde_json::to_string(&attempt).unwrap().replacen(
                "\"at_unix_ms\":null",
                &format!("\"at_unix_ms\":{bad}"),
                1,
            );
            assert!(serde_json::from_str::<H15AttemptDiagnostic>(&bad).is_err());
        }
    }

    #[test]
    fn screen_draw_restore_causal_trace_sanitizer_retains_finite_identity_and_rejects_private_values()
     {
        let raw = "WARN target trace_event=\"screen_draw_restore_decision\" elapsed_ms=52 trace_sequence=43 sd_restore_cause=LauncherRecovery sd_recovery_intent=11 sd_recovery_admission=7 sd_recovery_epoch=1 sd_restore_generation=3 sd_restore_lifecycle=2 sd_parking_cycle=1 sd_restore_start_revision=115 sd_restore_current_revision=116 sd_restore_outcome=Published private_label=secret";
        let safe = safe_trace_excerpt(raw);
        assert_eq!(
            trace_field_value(&safe, "trace_event"),
            Some("screen_draw_restore_decision")
        );
        for (key, value) in [
            ("sd_restore_cause", "LauncherRecovery"),
            ("sd_recovery_intent", "11"),
            ("sd_recovery_admission", "7"),
            ("sd_restore_current_revision", "116"),
            ("sd_restore_outcome", "Published"),
        ] {
            assert_eq!(trace_field_value(&safe, key), Some(value));
        }
        assert!(!safe.contains("secret"));
        for key in [
            "sd_restore_cause",
            "sd_restore_outcome",
            "sd_recovery_intent",
        ] {
            let current = trace_field_value(raw, key).unwrap();
            let changed = raw.replace(
                &format!("{key}={current}"),
                &format!("{key}=private_unbounded_label"),
            );
            let safe = safe_trace_excerpt(&changed);
            assert!(!safe.contains("private_unbounded_label"));
            assert!(trace_field_value(&safe, key).is_none());
        }
    }

    fn toolbar_frame(mode: ScreenDrawToolbarMode, frame_nr: u64) -> ScreenDrawToolbarObservation {
        let label = ScreenDrawToolbarWidget {
            target: ScreenDrawToolbarTarget::StateLabel,
            role: ScreenDrawToolbarRole::Label,
            widget_id: 701,
            enabled: true,
            bounds: [8, 35, 60, 50],
            clip: [0, 0, 500, 200],
            visible_bounds: [8, 35, 60, 50],
            fully_visible: true,
        };
        let resume = ScreenDrawToolbarWidget {
            target: ScreenDrawToolbarTarget::ResumeDrawing,
            role: ScreenDrawToolbarRole::Button,
            widget_id: 702,
            enabled: true,
            bounds: [8, 60, 130, 80],
            clip: [0, 0, 500, 200],
            visible_bounds: [8, 60, 130, 80],
            fully_visible: true,
        };
        ScreenDrawToolbarObservation {
            hwnd: 1002,
            process_id: 202,
            generation: 7,
            lifetime: 11,
            frame_nr,
            state: mode,
            runtime_mode: mode,
            client_size: [500, 200],
            launcher: ScreenDrawLauncherObservation {
                visibility_revision: if mode == ScreenDrawToolbarMode::Drawing {
                    5
                } else {
                    6
                },
                invocation_id: Some(5),
                focus_intent: multi_launcher::visibility::RootFocusIntent::ActivateRoot,
                visible: mode != ScreenDrawToolbarMode::Drawing,
                root: Some(ScreenDrawRootIdentity {
                    hwnd: 1001,
                    process_id: 202,
                    generation: 3,
                }),
                parking: Some(ScreenDrawParkingObservation {
                    hwnd: 1001,
                    generation: 7,
                    cycle: 1,
                    state: if mode == ScreenDrawToolbarMode::Drawing {
                        ScreenDrawParkingState::Committed
                    } else {
                        ScreenDrawParkingState::Restored
                    },
                }),
            },
            label,
            resume: matches!(
                mode,
                ScreenDrawToolbarMode::Ghost | ScreenDrawToolbarMode::Finish
            )
            .then_some(resume),
        }
    }

    fn toolbar_record(frame: ScreenDrawToolbarObservation, sequence: u64) -> String {
        multi_launcher::radial::acceptance_trace::screen_draw_toolbar_trace_record(
            frame,
            sequence,
            sequence + 20,
        )
    }

    const HISTORICAL_PRECHORD_VISIBILITY: &str = "trace_event=\"desired_visibility\" elapsed_ms=1 trace_sequence=1 revision=114 invocation_id=124 visible=false source=ToggleBatch";

    fn current_prechord_fixture(
        invocation: Option<u64>,
    ) -> (Vec<String>, ScreenDrawToolbarReceipt) {
        let mut frame = toolbar_frame(ScreenDrawToolbarMode::Drawing, 1);
        frame.launcher.visibility_revision = 115;
        frame.launcher.invocation_id = invocation;
        let raw = format!(
            "{HISTORICAL_PRECHORD_VISIBILITY}\ntrace_event=\"root_result_pointer\" elapsed_ms=2 trace_sequence=2 clicked=true pointer_released=true\n{}\n{}",
            toolbar_record(frame, 3),
            toolbar_record(
                ScreenDrawToolbarObservation {
                    frame_nr: 2,
                    ..frame
                },
                4
            ),
        );
        let lines = safe_trace_excerpt(&raw)
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let ready = parse_screen_draw_toolbar(&lines[2], 3).unwrap().unwrap();
        (lines, ready)
    }

    #[test]
    fn toolbar_observation_current_owner_sanitizer_round_trips_finite_values_and_full_integer_bounds()
     {
        use multi_launcher::visibility::RootFocusIntent;
        for invocation in [None, Some(u64::MAX)] {
            for focus in [
                RootFocusIntent::ActivateRoot,
                RootFocusIntent::PreserveForeground,
            ] {
                for state in [
                    ScreenDrawParkingState::Active,
                    ScreenDrawParkingState::Committed,
                    ScreenDrawParkingState::Restored,
                ] {
                    let mut frame = toolbar_frame(ScreenDrawToolbarMode::Drawing, 1);
                    frame.launcher.visibility_revision = u64::MAX;
                    frame.launcher.invocation_id = invocation;
                    frame.launcher.focus_intent = focus;
                    frame.launcher.visible = state == ScreenDrawParkingState::Restored;
                    frame.launcher.root = Some(ScreenDrawRootIdentity {
                        hwnd: u64::MAX,
                        process_id: u32::MAX,
                        generation: u64::MAX,
                    });
                    frame.launcher.parking = Some(ScreenDrawParkingObservation {
                        hwnd: u64::MAX,
                        generation: u64::MAX,
                        cycle: u64::MAX,
                        state,
                    });
                    let raw = toolbar_record(frame, 3);
                    let sanitized = safe_trace_excerpt(&format!(
                        "{raw} label=private_payload capture=private_payload"
                    ));
                    let parsed = parse_screen_draw_toolbar(&sanitized, 3).unwrap().unwrap();
                    assert_eq!(parsed.observation, frame);
                    assert_eq!(parsed.observation.launcher.invocation_id, invocation);
                    assert!(!sanitized.contains("private_payload"));
                }
            }
        }
        let mut absent = toolbar_frame(ScreenDrawToolbarMode::Drawing, 1);
        absent.launcher.invocation_id = None;
        absent.launcher.root = None;
        absent.launcher.parking = None;
        let sanitized = safe_trace_excerpt(&toolbar_record(absent, 3));
        let parsed = parse_screen_draw_toolbar(&sanitized, 3).unwrap().unwrap();
        assert_eq!(parsed.observation, absent);
        for token in [
            "sd_owner_invocation_present=false",
            "sd_owner_invocation=0",
            "sd_root_present=false",
            "sd_root_hwnd=0",
            "sd_root_pid=0",
            "sd_root_generation=0",
            "sd_parking_present=false",
            "sd_transaction_hwnd=0",
            "sd_transaction_generation=0",
            "sd_transaction_cycle=0",
            "sd_transaction_state=None",
        ] {
            assert!(sanitized.split_whitespace().any(|field| field == token));
        }
    }

    #[test]
    fn toolbar_observation_current_owner_sanitizer_rejects_payload_malformed_and_overflow_fields() {
        let raw = toolbar_record(toolbar_frame(ScreenDrawToolbarMode::Drawing, 1), 3);
        let sanitized = safe_trace_excerpt(&raw);
        assert!(parse_screen_draw_toolbar(&sanitized, 3).unwrap().is_some());
        let reject = |key: &str, value: &str| {
            let prefix = format!("{key}=");
            let original = raw
                .split_whitespace()
                .find(|token| token.starts_with(&prefix))
                .unwrap();
            let changed = raw.replacen(original, &format!("{key}={value}"), 1);
            assert_ne!(changed, raw);
            let sanitized = safe_trace_excerpt(&changed);
            assert!(
                !sanitized
                    .split_whitespace()
                    .any(|token| token.starts_with(&prefix)),
                "invalid {key}={value} must not enter the excerpt"
            );
            assert!(parse_screen_draw_toolbar(&sanitized, 3).is_err());
            assert!(!sanitized.contains("private_payload"));
        };
        for key in [
            "sd_owner_revision",
            "sd_owner_invocation",
            "sd_root_hwnd",
            "sd_root_generation",
            "sd_transaction_hwnd",
            "sd_transaction_generation",
            "sd_transaction_cycle",
        ] {
            for value in [
                "18446744073709551616",
                "-1",
                "1.5",
                "1e0",
                "private_payload",
            ] {
                reject(key, value);
            }
        }
        for value in ["4294967296", "-1", "1.5", "1e0", "private_payload"] {
            reject("sd_root_pid", value);
        }
        for key in [
            "sd_owner_invocation_present",
            "sd_owner_visible",
            "sd_root_present",
            "sd_parking_present",
        ] {
            for value in ["True", "1", "private_payload"] {
                reject(key, value);
            }
        }
        for key in ["sd_owner_focus", "sd_transaction_state"] {
            for value in ["Other", "private_payload"] {
                reject(key, value);
            }
        }
    }

    #[test]
    fn h15_current_baseline_same_owner_refresh_and_newer_revision_change_are_admitted() {
        use multi_launcher::visibility::RootFocusIntent;
        let (mut lines, ready) = current_prechord_fixture(None);
        let admit = |lines: &[String]| {
            admit_prechord_toolbar_baseline(
                lines,
                0,
                (2, 2),
                &ready,
                &toolbar_presentation(),
                [0, 0, 500, 200],
                1001,
                202,
                &[[-1920, -1080, 3840, 2160]],
            )
        };
        let refresh = admit(&lines).unwrap();
        assert_eq!(refresh.event_ordinal, 4);
        assert_eq!(refresh.observation.launcher, ready.observation.launcher);
        let mut changed = ready.observation;
        changed.frame_nr = 3;
        changed.launcher.visibility_revision = 116;
        changed.launcher.invocation_id = Some(124);
        changed.launcher.focus_intent = RootFocusIntent::PreserveForeground;
        lines.push(safe_trace_excerpt(&toolbar_record(changed, 5)));
        let current = admit(&lines).unwrap();
        assert_eq!(current.event_ordinal, 5);
        assert_eq!(current.trace_sequence, 5);
        assert_eq!(current.observation.launcher, changed.launcher);
        assert_eq!(
            current.observation.launcher.root,
            refresh.observation.launcher.root
        );
        assert_eq!(
            current.observation.launcher.parking,
            refresh.observation.launcher.parking
        );
    }

    #[test]
    fn h15_current_baseline_same_revision_invocation_focus_and_visibility_conflicts_reject_live() {
        let changes: [fn(&mut ScreenDrawToolbarObservation); 3] = [
            |frame| frame.launcher.invocation_id = Some(124),
            |frame| {
                frame.launcher.focus_intent =
                    multi_launcher::visibility::RootFocusIntent::PreserveForeground
            },
            |frame| frame.launcher.visible = true,
        ];
        for (index, change) in changes.into_iter().enumerate() {
            let (mut lines, ready) = current_prechord_fixture(None);
            let admit = |lines: &[String]| {
                admit_prechord_toolbar_baseline(
                    lines,
                    0,
                    (2, 2),
                    &ready,
                    &toolbar_presentation(),
                    [0, 0, 500, 200],
                    1001,
                    202,
                    &[[-1920, -1080, 3840, 2160]],
                )
            };
            assert!(admit(&lines).is_ok());
            let mut contradicted = ready.observation;
            contradicted.frame_nr = 3;
            change(&mut contradicted);
            lines.push(safe_trace_excerpt(&toolbar_record(contradicted, 5)));
            let parsed = parse_screen_draw_toolbar(&lines[4], 5).unwrap().unwrap();
            assert_eq!(parsed.observation, contradicted);
            assert_eq!(
                parsed.observation.launcher.visibility_revision,
                ready.observation.launcher.visibility_revision
            );
            let error = admit(&lines).unwrap_err();
            if index < 2 {
                assert!(error.contains("H15 baseline"), "{error}");
            }
        }
    }

    #[test]
    fn h15_current_baseline_selects_latest_real_sanitized_frame_instead_of_historical_visibility_owner()
     {
        for invocation in [None, Some(125)] {
            let (lines, ready) = current_prechord_fixture(invocation);
            let tool = toolbar_presentation();
            let baseline = admit_prechord_toolbar_baseline(
                &lines,
                0,
                (2, 2),
                &ready,
                &tool,
                [0, 0, 500, 200],
                1001,
                202,
                &[[-1920, -1080, 3840, 2160]],
            )
            .unwrap();
            assert_eq!(baseline.event_ordinal, 4);
            assert_eq!(baseline.trace_sequence, 4);
            assert_eq!(baseline.observation.launcher.visibility_revision, 115);
            assert_eq!(baseline.observation.launcher.invocation_id, invocation);
            assert_eq!(
                baseline.observation.launcher.parking.unwrap().state,
                ScreenDrawParkingState::Committed
            );
            assert!(HISTORICAL_PRECHORD_VISIBILITY.contains("revision=114 invocation_id=124"));
            assert_eq!(
                trace_field_value(HISTORICAL_PRECHORD_VISIBILITY, "trace_event"),
                Some("desired_visibility")
            );
            assert_eq!(
                trace_field_value(HISTORICAL_PRECHORD_VISIBILITY, "revision"),
                Some("114")
            );
            assert_eq!(
                trace_field_value(HISTORICAL_PRECHORD_VISIBILITY, "invocation_id"),
                Some("124")
            );
            assert_eq!(
                trace_field_value(&lines[0], "trace_event"),
                Some("desired_visibility")
            );
            assert_eq!(trace_field_value(&lines[0], "invocation_id"), Some("124"));
            let mut recovered = toolbar_frame(ScreenDrawToolbarMode::Ghost, 3);
            recovered.launcher.visibility_revision = 116;
            recovered.launcher.invocation_id = invocation;
            let recovered =
                parse_screen_draw_toolbar(&safe_trace_excerpt(&toolbar_record(recovered, 6)), 6)
                    .unwrap()
                    .unwrap();
            validate_screen_draw_toolbar_receipt(
                &recovered,
                ScreenDrawToolbarMode::Ghost,
                &tool,
                [0, 0, 500, 200],
                &[[-1920, -1080, 3840, 2160]],
                5,
                6,
                Some(&baseline),
            )
            .unwrap();
            validate_screen_draw_recovery_owner(
                &baseline,
                &recovered,
                116,
                invocation,
                HotkeyRootFocusIntent::ActivateRoot,
            )
            .unwrap();
            let decoded: ScreenDrawToolbarReceipt =
                serde_json::from_slice(&serde_json::to_vec(&baseline).unwrap()).unwrap();
            assert_eq!(decoded, baseline);
        }
    }

    #[test]
    fn h15_current_baseline_latest_stale_ambiguous_foreign_or_postrecovery_frame_never_falls_back()
    {
        let (original, ready) = current_prechord_fixture(None);
        let tool = toolbar_presentation();
        let admit = |lines: &[String]| {
            admit_prechord_toolbar_baseline(
                lines,
                0,
                (2, 2),
                &ready,
                &tool,
                [0, 0, 500, 200],
                1001,
                202,
                &[[-1920, -1080, 3840, 2160]],
            )
        };
        assert!(admit(&original).is_ok());
        for (from, to) in [
            ("sd_owner_revision=115", "sd_owner_revision=114"),
            (
                "sd_owner_invocation_present=false",
                "sd_owner_invocation_present=true",
            ),
            ("sd_owner_visible=false", "sd_owner_visible=true"),
            ("sd_root_generation=3", "sd_root_generation=4"),
            ("sd_root_pid=202", "sd_root_pid=203"),
            ("sd_root_hwnd=1001", "sd_root_hwnd=1003"),
            ("sd_transaction_generation=7", "sd_transaction_generation=8"),
            ("sd_transaction_cycle=1", "sd_transaction_cycle=2"),
            (
                "sd_transaction_state=Committed",
                "sd_transaction_state=Active",
            ),
            ("sd_frame=2", "sd_frame=1"),
            ("trace_sequence=4", "trace_sequence=2"),
            ("sd_hwnd=1002", "sd_hwnd=1003"),
        ] {
            let mut lines = original.clone();
            assert!(lines[3].contains(from));
            lines[3] = lines[3].replace(from, to);
            assert!(
                admit(&lines).is_err(),
                "{from}->{to} admitted an older frame"
            );
        }
        let mut lines = original.clone();
        lines.push(lines[3].clone());
        assert!(admit(&lines).is_err());
        let mut lines = original.clone();
        lines.push(lines[1].clone());
        assert!(admit(&lines).is_err());
        let mut lines = original.clone();
        lines.push(safe_trace_excerpt(&toolbar_record(
            toolbar_frame(ScreenDrawToolbarMode::Ghost, 3),
            5,
        )));
        assert!(admit(&lines).is_err());
        assert!(admit(&original[..2]).is_err());
    }

    #[test]
    fn toolbar_observation_current_owner_parser_requires_all_exact_producer_fields_after_sanitization()
     {
        let (lines, _) = current_prechord_fixture(None);
        let good = &lines[3];
        for key in [
            "sd_owner_revision",
            "sd_owner_invocation_present",
            "sd_owner_invocation",
            "sd_owner_focus",
            "sd_owner_visible",
            "sd_root_present",
            "sd_root_hwnd",
            "sd_root_pid",
            "sd_root_generation",
            "sd_parking_present",
            "sd_transaction_hwnd",
            "sd_transaction_generation",
            "sd_transaction_cycle",
            "sd_transaction_state",
        ] {
            let token = good
                .split_whitespace()
                .find(|token| token.starts_with(&format!("{key}=")))
                .unwrap();
            assert!(
                parse_screen_draw_toolbar(&good.replace(&format!(" {token}"), ""), 4).is_err(),
                "missing {key}"
            );
            assert!(
                parse_screen_draw_toolbar(&format!("{good} {token}"), 4).is_err(),
                "duplicate {key}"
            );
        }
        for (from, to) in [
            ("sd_owner_invocation=0", "sd_owner_invocation=124"),
            (
                "sd_owner_invocation_present=false",
                "sd_owner_invocation_present=true",
            ),
            (
                "sd_owner_revision=115",
                "sd_owner_revision=18446744073709551616",
            ),
            ("sd_owner_revision=115", "sd_owner_revision=-1"),
            ("sd_owner_revision=115", "sd_owner_revision=115.5"),
            ("sd_owner_visible=false", "sd_owner_visible=1"),
            ("sd_owner_focus=ActivateRoot", "sd_owner_focus=Other"),
            ("sd_root_present=true", "sd_root_present=false"),
            ("sd_parking_present=true", "sd_parking_present=false"),
            (
                "sd_transaction_state=Committed",
                "sd_transaction_state=None",
            ),
            (
                "sd_transaction_state=Committed",
                "sd_transaction_state=Other",
            ),
        ] {
            assert!(good.contains(from));
            assert!(
                parse_screen_draw_toolbar(&good.replace(from, to), 4).is_err(),
                "{from}->{to}"
            );
        }
        let raw = toolbar_record(toolbar_frame(ScreenDrawToolbarMode::Drawing, 1), 3);
        let sanitized = safe_trace_excerpt(&format!(
            "{raw} label=private_payload capture=private_payload"
        ));
        let parsed = parse_screen_draw_toolbar(&sanitized, 3).unwrap().unwrap();
        assert_eq!(parsed.observation.launcher.invocation_id, Some(5));
        assert!(!sanitized.contains("private_payload"));
    }

    fn toolbar_presentation() -> QueryRootPresentationEvidence {
        QueryRootPresentationEvidence {
            hwnd: 1002,
            process_id: 202,
            bounds: [200, 200, 700, 400],
            visible: true,
            minimized: false,
            physically_visible: true,
        }
    }

    #[test]
    fn h15_toolbar_fractional_client_live_and_persisted_admission_keeps_exact_native_pixels() {
        let scale = 1.2_f32;
        let logical = [317.0_f32 / scale, 840.0_f32 / scale];
        assert_eq!(
            logical.map(|dimension| (f64::from(dimension) * f64::from(scale)).ceil() as i32),
            [318, 841],
            "outward widget rounding cannot reconstruct the exact native client"
        );
        let displays = [[0, 0, 1920, 1080]];
        let client = [0, 0, 317, 840];
        let mut tool = toolbar_presentation();
        tool.bounds = [200, 100, 540, 1000];
        let client_frame = |mode, number| {
            let mut frame = toolbar_frame(mode, number);
            frame.client_size = [317, 840];
            frame.label.clip = client;
            if let Some(resume) = &mut frame.resume {
                resume.clip = client;
            }
            frame
        };
        let drawing = client_frame(ScreenDrawToolbarMode::Drawing, 1);
        let ghost = client_frame(ScreenDrawToolbarMode::Ghost, 2);
        let mut lines = vec![
            "trace_event=unrelated elapsed_ms=20".to_owned(),
            safe_trace_excerpt(&toolbar_record(drawing, 21)),
        ];
        let ready = admit_toolbar_observation_from_trace(
            &lines,
            (1, 20),
            ScreenDrawToolbarMode::Drawing,
            &tool,
            client,
            &displays,
            None,
        )
        .unwrap();
        lines.push(safe_trace_excerpt(ADMISSION));
        lines.push(safe_trace_excerpt(&toolbar_record(ghost, 31)));
        let live = admit_toolbar_observation_from_trace(
            &lines,
            (3, 0),
            ScreenDrawToolbarMode::Ghost,
            &tool,
            client,
            &displays,
            Some(&ready),
        )
        .unwrap();
        let persisted: ScreenDrawToolbarReceipt =
            serde_json::from_slice(&serde_json::to_vec(&live).unwrap()).unwrap();
        assert_eq!(live, persisted);
        for receipt in [&live, &persisted] {
            assert_eq!(receipt.observation, ghost);
            assert!(
                receipt
                    .observation
                    .has_visible_controls_for_mode(ScreenDrawToolbarMode::Ghost)
            );
            validate_screen_draw_toolbar_receipt(
                receipt,
                ScreenDrawToolbarMode::Ghost,
                &tool,
                client,
                &displays,
                3,
                4,
                Some(&ready),
            )
            .unwrap();
            for wrong_native in [[0, 0, 318, 840], [0, 0, 317, 841]] {
                assert!(
                    validate_screen_draw_toolbar_receipt(
                        receipt,
                        ScreenDrawToolbarMode::Ghost,
                        &tool,
                        wrong_native,
                        &displays,
                        3,
                        4,
                        Some(&ready),
                    )
                    .is_err()
                );
            }
        }
        let mutations: &[fn(&mut ScreenDrawToolbarObservation)] = &[
            |frame| frame.client_size[0] = 318,
            |frame| frame.client_size[1] = 841,
            |frame| frame.label.clip[2] = 318,
            |frame| frame.resume.as_mut().unwrap().clip[3] = 841,
            |frame| frame.resume.as_mut().unwrap().fully_visible = false,
        ];
        for mutate in mutations {
            let mut wrong = ghost;
            mutate(&mut wrong);
            lines[3] = safe_trace_excerpt(&toolbar_record(wrong, 31));
            assert!(
                admit_toolbar_observation_from_trace(
                    &lines,
                    (3, 0),
                    ScreenDrawToolbarMode::Ghost,
                    &tool,
                    client,
                    &displays,
                    Some(&ready),
                )
                .is_err()
            );
            let parsed = parse_screen_draw_toolbar(&lines[3], 4).unwrap().unwrap();
            let persisted: ScreenDrawToolbarReceipt =
                serde_json::from_slice(&serde_json::to_vec(&parsed).unwrap()).unwrap();
            assert!(
                validate_screen_draw_toolbar_receipt(
                    &persisted,
                    ScreenDrawToolbarMode::Ghost,
                    &tool,
                    client,
                    &displays,
                    3,
                    4,
                    Some(&ready),
                )
                .is_err()
            );
        }
    }

    #[test]
    fn h15_toolbar_actual_serializer_sanitizer_parser_round_trip_keeps_only_finite_observations() {
        for mode in [
            ScreenDrawToolbarMode::Drawing,
            ScreenDrawToolbarMode::Ghost,
            ScreenDrawToolbarMode::Finish,
        ] {
            let frame = toolbar_frame(mode, 3);
            let raw = format!(
                "WARN multi_launcher.radial_acceptance: {} private_label=\"private capture\" sd_private=secret",
                toolbar_record(frame, 41)
            );
            let sanitized = safe_trace_excerpt(&raw);
            assert!(sanitized.starts_with("trace_event=screen_draw_toolbar "));
            assert!(!sanitized.contains("private") && !sanitized.contains("secret"));
            let parsed = parse_screen_draw_toolbar(&sanitized, 11).unwrap().unwrap();
            assert_eq!(parsed.observation, frame);
            assert_eq!(parsed.event_ordinal, 11);
            assert_eq!(parsed.trace_sequence, 41);
            assert_eq!(parsed.elapsed_ms, 61);
            let wire = serde_json::to_vec(&parsed).unwrap();
            assert_eq!(
                serde_json::from_slice::<ScreenDrawToolbarReceipt>(&wire).unwrap(),
                parsed
            );
        }
    }

    #[test]
    fn h15_toolbar_parser_rejects_missing_duplicate_malformed_and_overflow_producer_fields() {
        let good = toolbar_record(toolbar_frame(ScreenDrawToolbarMode::Ghost, 2), 31);
        for (key, replacement) in [
            ("sd_hwnd", "18446744073709551616"),
            ("sd_pid", "4294967296"),
            ("sd_lifetime", "-1"),
            ("sd_frame", "1.5"),
            ("sd_client_width", "2147483648"),
            ("sd_label_enabled", "yes"),
            ("sd_state", "\"Ghost"),
            ("sd_resume_role", "Imaginary"),
            ("sd_resume_target", "GhostButton"),
        ] {
            let old = toolbar_field(&good, key).unwrap();
            let bad = good.replace(&format!("{key}={old}"), &format!("{key}={replacement}"));
            assert!(
                parse_screen_draw_toolbar(&bad, 11).is_err(),
                "invalid {key} accepted"
            );
            assert!(
                !matches!(
                    parse_screen_draw_toolbar(&safe_trace_excerpt(&bad), 11),
                    Ok(Some(_))
                ),
                "invalid sanitized {key} accepted"
            );
        }
        for key in [
            "sd_hwnd",
            "sd_generation",
            "sd_label_id",
            "sd_resume_present",
            "sd_resume_clip_top",
            "trace_sequence",
            "elapsed_ms",
        ] {
            let value = toolbar_field(&good, key).unwrap();
            assert!(
                parse_screen_draw_toolbar(&good.replace(&format!(" {key}={value}"), ""), 11)
                    .is_err()
            );
            assert!(parse_screen_draw_toolbar(&format!("{good} {key}={value}"), 11).is_err());
        }
        assert!(
            parse_screen_draw_toolbar(&format!("{good} trace_event=\"screen_draw_toolbar\""), 11)
                .is_err()
        );
        assert!(parse_screen_draw_toolbar(&format!("trace_event=Other {good}"), 11).is_err());
        let mut absent = toolbar_frame(ScreenDrawToolbarMode::Drawing, 2);
        absent.resume = None;
        let forged = toolbar_record(absent, 31).replace("sd_resume_id=0", "sd_resume_id=702");
        assert!(parse_screen_draw_toolbar(&forged, 11).is_err());
    }

    #[test]
    fn h15_toolbar_live_admission_requires_current_boundary_same_lifetime_and_actual_ghost_frame() {
        let displays = [[0, 0, 1920, 1080]];
        let client = [0, 0, 500, 200];
        let tool = toolbar_presentation();
        let mut lines = vec![
            "trace_event=unrelated elapsed_ms=20".to_owned(),
            toolbar_record(toolbar_frame(ScreenDrawToolbarMode::Drawing, 1), 21),
        ];
        let ready = admit_toolbar_observation_from_trace(
            &lines,
            (1, 20),
            ScreenDrawToolbarMode::Drawing,
            &tool,
            client,
            &displays,
            None,
        )
        .unwrap();
        lines.push(ADMISSION.into());
        lines.push(toolbar_record(
            toolbar_frame(ScreenDrawToolbarMode::Ghost, 2),
            31,
        ));
        let recovered = admit_toolbar_observation_from_trace(
            &lines,
            (3, 0),
            ScreenDrawToolbarMode::Ghost,
            &tool,
            client,
            &displays,
            Some(&ready),
        )
        .unwrap();
        assert_eq!(
            recovered.observation.label.widget_id,
            ready.observation.label.widget_id
        );
        assert!(
            recovered.event_ordinal > ready.event_ordinal
                && recovered.trace_sequence > ready.trace_sequence
        );
        for mode in [
            ScreenDrawToolbarMode::Drawing,
            ScreenDrawToolbarMode::Finish,
        ] {
            lines[3] = toolbar_record(toolbar_frame(mode, 2), 31);
            assert!(
                admit_toolbar_observation_from_trace(
                    &lines,
                    (3, 0),
                    ScreenDrawToolbarMode::Ghost,
                    &tool,
                    client,
                    &displays,
                    Some(&ready)
                )
                .is_err(),
                "Ghost button/Finish Resume accepted as recovered Ghost"
            );
        }
        lines[3] = toolbar_record(toolbar_frame(ScreenDrawToolbarMode::Ghost, 2), 31);
        assert!(
            admit_toolbar_observation_from_trace(
                &lines,
                (4, 0),
                ScreenDrawToolbarMode::Ghost,
                &tool,
                client,
                &displays,
                Some(&ready)
            )
            .is_err()
        );
        let mut stale = toolbar_frame(ScreenDrawToolbarMode::Ghost, 3);
        stale.lifetime += 1;
        lines.push(toolbar_record(stale, 32));
        assert!(
            admit_toolbar_observation_from_trace(
                &lines,
                (3, 0),
                ScreenDrawToolbarMode::Ghost,
                &tool,
                client,
                &displays,
                Some(&ready)
            )
            .is_err(),
            "newest wrong lifetime must not fall back to historic valid frame"
        );
        lines.pop();
        lines.push("trace_event=budget_exhausted trace_sequence=32 elapsed_ms=62".into());
        assert!(
            admit_toolbar_observation_from_trace(
                &lines,
                (3, 0),
                ScreenDrawToolbarMode::Ghost,
                &tool,
                client,
                &displays,
                Some(&ready)
            )
            .is_err()
        );
    }

    #[test]
    fn h15_toolbar_live_and_persisted_admission_rejects_owner_client_control_and_frame_mutations() {
        let displays = [[0, 0, 1920, 1080]];
        let tool = toolbar_presentation();
        let client = [0, 0, 500, 200];
        let ready = parse_screen_draw_toolbar(
            &toolbar_record(toolbar_frame(ScreenDrawToolbarMode::Drawing, 1), 21),
            2,
        )
        .unwrap()
        .unwrap();
        let mutations: &[fn(&mut ScreenDrawToolbarObservation)] = &[
            |f| f.hwnd = 0,
            |f| f.hwnd += 1,
            |f| f.process_id = 0,
            |f| f.process_id += 1,
            |f| f.generation = 0,
            |f| f.generation += 1,
            |f| f.lifetime = 0,
            |f| f.lifetime += 1,
            |f| f.frame_nr = 0,
            |f| f.frame_nr = 1,
            |f| f.state = ScreenDrawToolbarMode::Finish,
            |f| f.runtime_mode = ScreenDrawToolbarMode::Drawing,
            |f| f.resume = None,
            |f| f.label.widget_id = 0,
            |f| f.label.widget_id += 1,
            |f| f.label.target = ScreenDrawToolbarTarget::ResumeDrawing,
            |f| f.label.role = ScreenDrawToolbarRole::Button,
            |f| f.label.enabled = false,
            |f| f.resume.as_mut().unwrap().enabled = false,
            |f| f.resume.as_mut().unwrap().fully_visible = false,
            |f| f.resume.as_mut().unwrap().visible_bounds = [0; 4],
            |f| f.resume.as_mut().unwrap().clip = [0, 0, 50, 30],
            |f| f.resume.as_mut().unwrap().bounds = [i32::MIN, 0, i32::MAX, 100],
            |f| f.client_size = [0, 200],
            |f| f.label.clip[2] = 501,
        ];
        for (index, mutate) in mutations.iter().enumerate() {
            let mut frame = toolbar_frame(ScreenDrawToolbarMode::Ghost, 2);
            mutate(&mut frame);
            let raw = toolbar_record(frame, 31);
            let parsed = parse_screen_draw_toolbar(&safe_trace_excerpt(&raw), 4)
                .unwrap()
                .unwrap();
            let persisted: ScreenDrawToolbarReceipt =
                serde_json::from_slice(&serde_json::to_vec(&parsed).unwrap()).unwrap();
            for receipt in [&parsed, &persisted] {
                assert!(
                    validate_screen_draw_toolbar_receipt(
                        receipt,
                        ScreenDrawToolbarMode::Ghost,
                        &tool,
                        client,
                        &displays,
                        3,
                        4,
                        Some(&ready)
                    )
                    .is_err(),
                    "mutation {index} accepted by shared contract"
                );
            }
        }
        let good = parse_screen_draw_toolbar(
            &toolbar_record(toolbar_frame(ScreenDrawToolbarMode::Ghost, 2), 31),
            4,
        )
        .unwrap()
        .unwrap();
        for client in [[0, 0, 499, 200], [1, 0, 501, 200], [0, 0, i32::MAX, 200]] {
            assert!(
                validate_screen_draw_toolbar_receipt(
                    &good,
                    ScreenDrawToolbarMode::Ghost,
                    &tool,
                    client,
                    &displays,
                    3,
                    4,
                    Some(&ready)
                )
                .is_err()
            );
        }
        let mut hidden = tool;
        hidden.visible = false;
        assert!(
            validate_screen_draw_toolbar_receipt(
                &good,
                ScreenDrawToolbarMode::Ghost,
                &hidden,
                [0, 0, 500, 200],
                &displays,
                3,
                4,
                Some(&ready)
            )
            .is_err()
        );
    }

    #[test]
    fn h15_toolbar_boundary_requires_real_unique_entry_click_and_recovery_admission() {
        let click="trace_event=\"root_result_pointer\" clicked=true pointer_released=true trace_sequence=21".to_owned();
        let lines = vec!["trace_event=trace_ready".into(), click.clone()];
        assert_eq!(
            toolbar_observation_boundary(&lines, 1, ToolbarObservationPhase::Drawing).unwrap(),
            Some((2, 21))
        );
        assert!(
            toolbar_observation_boundary(&lines, 2, ToolbarObservationPhase::Drawing)
                .unwrap()
                .is_none()
        );
        let mut duplicate = lines.clone();
        duplicate.push(click);
        assert!(
            toolbar_observation_boundary(&duplicate, 1, ToolbarObservationPhase::Drawing).is_err()
        );
        let lines = vec![ADMISSION.to_string()];
        assert_eq!(
            toolbar_observation_boundary(&lines, 0, ToolbarObservationPhase::Recovery).unwrap(),
            Some((1, 0))
        );
        let false_owner = vec![ADMISSION.replace("ScreenDrawRecovery", "Launcher")];
        assert!(
            toolbar_observation_boundary(&false_owner, 0, ToolbarObservationPhase::Recovery)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn h15_failure_capture_runs_before_close_retains_actual_files_and_preserves_primary_error() {
        let directory = tempfile::tempdir().unwrap();
        let alive = std::cell::Cell::new(true);
        let order = std::cell::RefCell::new(Vec::new());
        let original = failure(
            FailureStage::NativeRootState,
            "original Ghost frame predicate",
        );
        let (result, paths) = finish_h15_with_cleanup(
            Err(original),
            |primary| {
                assert!(alive.get());
                assert_eq!(primary.message, "original Ghost frame predicate");
                order.borrow_mut().push("capture");
                let mut captured = H15PrecloseArtifacts::default();
                persist_h15_preclose_piece(
                    &mut captured,
                    directory.path(),
                    "observation.json",
                    Ok(b"{\"tool\":1002,\"phase\":\"before-owned-close\"}".to_vec()),
                );
                persist_h15_preclose_piece(
                    &mut captured,
                    directory.path(),
                    "trace.log",
                    Err("screenshot/source capture failure".into()),
                );
                captured
            },
            || {
                assert!(alive.replace(false));
                order.borrow_mut().push("close");
                assert!(
                    directory
                        .path()
                        .join("case-H15-preclose-observation.json")
                        .exists()
                );
                Err(failure(FailureStage::Cleanup, "owned close failed"))
            },
        );
        let error = result.unwrap_err();
        assert_eq!(error.stage, FailureStage::NativeRootState);
        assert!(error.message.starts_with("original Ghost frame predicate"));
        assert!(
            error
                .message
                .contains("Screen Draw cleanup: owned close failed")
        );
        assert!(
            error
                .message
                .contains("pre-close capture diagnostics: screenshot/source capture failure")
        );
        assert_eq!(*order.borrow(), ["capture", "close"]);
        let paths = paths.unwrap();
        assert_eq!(paths.len(), 1);
        assert!(
            fs::read_to_string(&paths[0])
                .unwrap()
                .contains("before-owned-close")
        );
    }

    #[test]
    fn h15_failure_capture_bounds_paths_diagnostics_and_success_cleanup_failure() {
        let directory = tempfile::tempdir().unwrap();
        let mut captured = H15PrecloseArtifacts::default();
        persist_h15_preclose_piece(&mut captured, directory.path(), "../escape", Ok(vec![1]));
        persist_h15_preclose_piece(
            &mut captured,
            directory.path(),
            "observation.json",
            Ok(vec![1; MAX_PRIVATE_LOG_BYTES + 1]),
        );
        persist_h15_preclose_piece(
            &mut captured,
            directory.path(),
            "private.log",
            Err("x".repeat(1000)),
        );
        assert!(captured.paths.is_empty());
        assert_eq!(captured.errors.len(), 3);
        assert!(captured.errors.iter().all(|error| error.len() <= 256));
        let (result, paths) = finish_h15_with_cleanup(
            Ok("qualified pre-cleanup flow".into()),
            |_| panic!("success is not a failed observation"),
            || {
                Err(failure(
                    FailureStage::WindowDiscovery,
                    "owned toolbar could not close",
                ))
            },
        );
        let error = result.unwrap_err();
        assert_eq!(error.stage, FailureStage::Cleanup);
        assert!(error.message.starts_with("Screen Draw cleanup:"));
        assert!(paths.is_none());
    }

    #[test]
    fn h15_preclose_inventory_reports_missing_tool_without_substituting_restored_root() {
        let root = WindowSnapshot {
            hwnd: HWND(1001usize as *mut _),
            process_id: 202,
            role: WindowRole::Root,
            class_name: "ROOT".into(),
            visible: true,
            minimized: false,
            bounds: [100, 100, 900, 700],
        };
        let diagnostic = ToolbarObservationDiagnostic {
            phase: Some(ToolbarObservationPhase::Recovery),
            boundary_ordinal: 23,
            last_trace_ordinal: 32,
            last_admissible: Some(
                parse_screen_draw_toolbar(
                    &toolbar_record(toolbar_frame(ScreenDrawToolbarMode::Drawing, 1), 21),
                    11,
                )
                .unwrap()
                .unwrap(),
            ),
            rejected: Some("no current owned toolbar exists".into()),
        };
        let primary = failure(FailureStage::NativeRootState, "recovery toolbar is absent");
        let value = h15_preclose_inventory(
            &diagnostic,
            &H15AttemptDiagnostic::new(AcceptanceHotkey::ShiftAltWinEnd),
            &primary,
            202,
            &Ok(None),
            None,
            &[root.clone()],
        );
        assert!(value["tool"].is_null() && value["native_client"].is_null());
        assert_eq!(value["windows"][0]["role"], "root");
        assert_eq!(
            value["observation"]["rejected"],
            "no current owned toolbar exists"
        );
        assert_eq!(value["primary_failure"], "recovery toolbar is absent");
        assert_eq!(value["phase"], "before-owned-close");
        let value = h15_preclose_inventory(
            &diagnostic,
            &H15AttemptDiagnostic::new(AcceptanceHotkey::ShiftAltWinEnd),
            &primary,
            202,
            &Ok(None),
            None,
            &vec![root; 33],
        );
        assert_eq!(value["windows"].as_array().unwrap().len(), 32);
        assert_eq!(value["windows_truncated"], true);
    }

    const PRIMARY: &str = "WARN multi_launcher.radial_acceptance: radial acceptance trace trace_event=\"hook_primary\" elapsed_ms=50 transition=Press provenance=ExternalInjected foreground_owner=Other";
    const CONFIGURED: &str = "WARN multi_launcher.radial_acceptance: radial acceptance trace trace_event=\"configured_primary\" elapsed_ms=50 transition=Press provenance=ExternalInjected modifiers_match=true invocation_id=7 generation=1";
    const ADMISSION: &str = "WARN multi_launcher.radial_acceptance: radial acceptance trace trace_event=\"hook_admission\" elapsed_ms=51 transition=Press provenance=ExternalInjected owner=ScreenDrawRecovery global_exclusive_owners=1 adapter_exclusive=false recovery=true deadline_scheduled=false radial_intent=false";

    #[test]
    fn h15_priority_parser_uses_real_producer_owner_provenance_and_boolean_fields() {
        let primary = parse_priority_primary(PRIMARY, 21).unwrap().unwrap();
        assert_eq!(primary.event_ordinal, 21);
        assert_eq!(primary.elapsed_ms, 50);
        assert_eq!(primary.transition, HotkeyEdgeTransition::Press);
        assert_eq!(primary.provenance, HotkeyInputProvenance::ExternalInjected);
        assert_eq!(primary.foreground_owner, PriorityForegroundOwner::Other);
        let admission = parse_priority_admission(ADMISSION, 22).unwrap().unwrap();
        assert_eq!(admission.owner, PriorityHookOwner::ScreenDrawRecovery);
        assert_eq!(admission.global_exclusive_owners, 1);
        assert!(admission.recovery);
        assert!(!admission.deadline_scheduled && !admission.radial_intent);
        let self_injected = PRIMARY.replace("ExternalInjected", "SelfInjected");
        assert_eq!(
            parse_priority_primary(&self_injected, 21)
                .unwrap()
                .unwrap()
                .provenance,
            HotkeyInputProvenance::Owned
        );
        assert!(parse_priority_primary(ADMISSION, 22).unwrap().is_none());
        assert!(parse_priority_admission(PRIMARY, 21).unwrap().is_none());
        for malformed in [
            ADMISSION.replace("ScreenDrawRecovery", "ClaimedOwner"),
            ADMISSION.replace("ExternalInjected", "LauncherInjected"),
            ADMISSION.replace(" recovery=true", ""),
            ADMISSION.replace("recovery=true", "recovery=unknown"),
            ADMISSION.replace("global_exclusive_owners=1", "global_exclusive_owners=-1"),
            ADMISSION.replace("elapsed_ms=51", "elapsed_ms=missing"),
            ADMISSION.replace("transition=Press", "transition=Repeat"),
        ] {
            assert!(
                parse_priority_admission(&malformed, 22).is_err(),
                "{malformed}"
            );
        }
    }

    #[test]
    fn h15_priority_interval_preserves_owned_edge_order_and_rejects_malformed_or_excess_receipts() {
        let lines = vec!["trace_event=\"trace_ready\"".into(), PRIMARY.into(), CONFIGURED.into(), ADMISSION.into(),
            "trace_event=\"desired_visibility\" elapsed_ms=52 visible=true revision=6 source=ScreenDrawRestore invocation_id=5".into(),
            "trace_event=\"screen_draw_restore_focus_intent\" elapsed_ms=53 revision=6 invocation_id=5 focus_intent=ActivateRoot".into(),
            PRIMARY.replace("elapsed_ms=50", "elapsed_ms=58").replace("Press", "Release").replace("Other", "Root"),
            ADMISSION.replace("elapsed_ms=51", "elapsed_ms=59").replace("Press", "Release").replace("recovery=true", "recovery=false")];
        let interval = priority_interval(&lines, 1, lines.len()).unwrap();
        assert_eq!(interval.primary_edges.len(), 2);
        assert_eq!(interval.admissions.len(), 2);
        assert_eq!(interval.candidate_events.len(), 2);
        assert_eq!(interval.candidate_events[0].event_ordinal, 5);
        assert_eq!(interval.configured_primary.len(), 1);
        let configured = &interval.configured_primary[0];
        assert_eq!(configured.invocation_id, 7);
        assert_eq!(configured.generation, 1);
        assert!(configured.modifiers_match);
        assert!(interval.primary_edges[0].event_ordinal < configured.event_ordinal);
        assert!(configured.event_ordinal < interval.admissions[0].event_ordinal);
        assert_eq!(
            interval.candidate_events[0].visibility_source,
            Some(HotkeyVisibilitySource::ScreenDrawRestore)
        );
        assert_eq!(interval.forbidden_event_count, 0);
        for forbidden in [
            "trace_event=\"short_tap\" elapsed_ms=60 invocation_id=7 terminal=true",
            "trace_event=\"radial_query_dispatch\" elapsed_ms=60 outcome=executed",
            "trace_event=\"radial_dispatch_requested\" elapsed_ms=60",
            "trace_event=\"universal_action_execution\" elapsed_ms=60",
            "trace_event=\"hook_deadline\" elapsed_ms=60 edge=Scheduled",
            "trace_event=\"desired_visibility\" elapsed_ms=60 visible=false revision=7 source=ToggleBatch invocation_id=9",
        ] {
            let mut changed = lines.clone();
            changed.push(forbidden.into());
            assert_eq!(
                priority_interval(&changed, 1, changed.len())
                    .unwrap()
                    .forbidden_event_count,
                1
            );
        }
        let mut malformed = lines.clone();
        malformed.push("trace_event=\"root_command\" elapsed_ms=60".into());
        assert!(priority_interval(&malformed, 1, malformed.len()).is_err());
        let mut excess = lines.clone();
        excess.push(PRIMARY.into());
        assert!(priority_interval(&excess, 1, excess.len()).is_err());
        assert!(priority_interval(&lines, 0, lines.len()).is_err());
        assert!(priority_interval(&lines, 1, lines.len() + 1).is_err());
        for extra in [
            CONFIGURED.to_owned(),
            CONFIGURED.replace("Press", "Release"),
        ] {
            let mut excess = lines.clone();
            excess.push(extra);
            assert!(priority_interval(&excess, 1, excess.len()).is_err());
        }
        for malformed in [
            CONFIGURED.replace(" generation=1", ""),
            CONFIGURED.replace("generation=1", "generation=wrong"),
            CONFIGURED.replace("invocation_id=7", "invocation_id=none"),
            CONFIGURED.replace("modifiers_match=true", "modifiers_match=unknown"),
            CONFIGURED.replace("ExternalInjected", "unknown"),
        ] {
            assert!(
                parse_priority_configured_primary(&malformed, 3).is_err(),
                "{malformed}"
            );
        }
    }

    fn recovery_receipt_interval(line: &str) -> Result<PriorityInterval, String> {
        let lines = vec!["trace_event=\"trace_ready\"".into(), line.into()];
        priority_interval(&lines, 1, lines.len())
    }

    const RECOVERY_CORRELATION: &str = "request_id=13 request_kind=None session_id=0 generation=13 terminal=false visibility_revision=6 invocation_id=5";

    #[test]
    fn h15_recovery_receipts_require_complete_producer_fields_for_each_kind() {
        let prefix = "WARN multi_launcher.radial_acceptance: radial acceptance trace";
        let common = [
            ("request_id", "13"),
            ("terminal", "false"),
            ("visibility_revision", "6"),
            ("invocation_id", "5"),
        ];
        let mut command = common.to_vec();
        command.push(("command", "Show"));
        let mut snapshot = common.to_vec();
        snapshot.extend([
            ("hwnd", "101"),
            ("process_id", "202"),
            ("left", "100"),
            ("top", "100"),
            ("right", "1000"),
            ("bottom", "750"),
            ("visible", "true"),
            ("minimized", "false"),
        ]);
        let mut activation = common.to_vec();
        activation.extend([("hwnd", "101"), ("edge", "RestoreCompleted")]);
        let cases = [
            (
                format!(
                    "{prefix} trace_event=\"desired_visibility\" elapsed_ms=60 visible=true revision=6 source=ScreenDrawRestore invocation_id=5"
                ),
                vec![
                    ("visible", "true"),
                    ("revision", "6"),
                    ("source", "ScreenDrawRestore"),
                    ("invocation_id", "5"),
                ],
            ),
            (
                format!(
                    "{prefix} trace_event=\"screen_draw_restore_focus_intent\" elapsed_ms=60 revision=6 invocation_id=5 focus_intent=ActivateRoot"
                ),
                vec![
                    ("revision", "6"),
                    ("invocation_id", "5"),
                    ("focus_intent", "ActivateRoot"),
                ],
            ),
            (
                format!(
                    "{prefix} trace_event=\"root_command\" elapsed_ms=60 command=Show {RECOVERY_CORRELATION}"
                ),
                command,
            ),
            (
                format!(
                    "{prefix} trace_event=\"native_window_snapshot\" elapsed_ms=60 hwnd=101 process_id=202 left=100 top=100 right=1000 bottom=750 visible=true minimized=false {RECOVERY_CORRELATION}"
                ),
                snapshot,
            ),
            (
                format!(
                    "{prefix} trace_event=\"native_activation\" elapsed_ms=60 edge=RestoreCompleted hwnd=101 {RECOVERY_CORRELATION}"
                ),
                activation,
            ),
        ];
        for (line, mut fields) in cases {
            let interval = recovery_receipt_interval(&line).unwrap();
            assert_eq!(interval.candidate_events.len(), 1);
            assert_eq!(interval.candidate_events[0].invocation_id, Some(5));
            assert_eq!(interval.candidate_events[0].visibility_revision, Some(6));
            assert_eq!(interval.candidate_events[0].event_ordinal, 2);
            fields.push(("elapsed_ms", "60"));
            for (field, value) in fields {
                let original = format!(" {field}={value}");
                assert!(line.contains(&original));
                for malformed in [
                    line.replacen(&original, "", 1),
                    line.replacen(&original, &format!(" {field}=unknown"), 1),
                ] {
                    assert!(
                        recovery_receipt_interval(&malformed).is_err(),
                        "{field}: {malformed}"
                    );
                }
            }
        }
        let partial = "trace_event=\"root_command\" elapsed_ms=60";
        assert!(
            parse_hotkey_candidate_event(
                partial,
                HotkeyCandidateStream::MainCandidate,
                1,
                HotkeyRunnerInputPurpose::LauncherChord,
                2
            )
            .is_some()
        );
        assert!(recovery_receipt_interval(partial).is_err());
    }

    #[test]
    fn h15_recovery_receipts_preserve_explicit_none_and_reject_invalid_identity_or_payload() {
        let root = format!(
            "trace_event=\"root_command\" elapsed_ms=60 command=Show {RECOVERY_CORRELATION}"
        );
        let snapshot = format!(
            "trace_event=\"native_window_snapshot\" elapsed_ms=60 hwnd=101 process_id=202 left=-100 top=-200 right=800 bottom=450 visible=true minimized=false {RECOVERY_CORRELATION}"
        );
        let activation = format!(
            "trace_event=\"native_activation\" elapsed_ms=60 edge=RestoreCompleted hwnd=101 {RECOVERY_CORRELATION}"
        );
        for line in [&root, &snapshot, &activation] {
            let zero_invocation = line.replace("invocation_id=5", "invocation_id=0");
            let interval = recovery_receipt_interval(&zero_invocation).unwrap();
            assert_eq!(interval.candidate_events[0].invocation_id, None);
            for malformed in [
                line.replace("invocation_id=5", "invocation_id=none"),
                line.replace("invocation_id=5", "invocation_id=-1"),
                line.replace("request_id=13", "request_id=0"),
                line.replace("visibility_revision=6", "visibility_revision=0"),
            ] {
                assert!(
                    recovery_receipt_interval(&malformed).is_err(),
                    "{malformed}"
                );
            }
        }
        for line in [
            "trace_event=\"desired_visibility\" elapsed_ms=60 visible=true revision=6 source=ScreenDrawRestore invocation_id=none",
            "trace_event=\"screen_draw_restore_focus_intent\" elapsed_ms=60 revision=6 invocation_id=none focus_intent=PreserveForeground",
        ] {
            assert_eq!(
                recovery_receipt_interval(line).unwrap().candidate_events[0].invocation_id,
                None
            );
        }
        for command in [
            "Show",
            "Minimize",
            "Focus",
            "ParkingBoundary",
            "position requested_x=-120 requested_y=140",
            "size requested_width=900 requested_height=650",
        ] {
            let line = root.replace("command=Show", &format!("command={command}"));
            assert!(recovery_receipt_interval(&line).is_ok(), "{line}");
        }
        for command in [
            "unknown",
            "position requested_x=1",
            "position requested_x=wrong requested_y=1",
            "position requested_x=2147483648 requested_y=1",
            "size requested_width=900",
            "size requested_width=0 requested_height=650",
            "size requested_width=900 requested_height=-1",
        ] {
            let line = root.replace("command=Show", &format!("command={command}"));
            assert!(recovery_receipt_interval(&line).is_err(), "{line}");
        }
        for (before, after) in [
            ("hwnd=101", "hwnd=0"),
            ("process_id=202", "process_id=0"),
            ("right=800", "right=-100"),
            ("bottom=450", "bottom=-201"),
        ] {
            assert!(recovery_receipt_interval(&snapshot.replace(before, after)).is_err());
        }
        for edge in ["RestoreRequested", "RestoreCompleted", "RestoreFailed"] {
            assert!(
                recovery_receipt_interval(&activation.replace("RestoreCompleted", edge)).is_ok()
            );
        }
        assert!(
            recovery_receipt_interval(&activation.replace("RestoreCompleted", "Superseded"))
                .is_err()
        );
    }

    #[test]
    fn h15_native_down_and_owned_release_use_actual_thread_and_active_desktop_receipts() {
        let edge = |foreground_hwnd, desktop: &str| NativeInputEdgeEvidence {
            inserted: 1,
            at_unix_ms: 1,
            foreground_hwnd,
            foreground_pid: 202,
            input_desktop: desktop.into(),
            cleanup_status: "release_inserted;async_state_to_be_verified_by_owner".into(),
            keyboard_input: None,
        };
        let input = AcceptanceHotkeyTapEvidence {
            down: edge(1002, "thread=Default;active=Default"),
            up: edge(1001, "thread=Default;active=Default"),
            observed_vks: vec![0x7a],
        };
        assert!(native_input_desktop_is_default(&input.down.input_desktop));
        assert!(native_input_desktop_is_default(&input.up.input_desktop));
        assert_eq!(input.down.foreground_hwnd, 1002);
        assert_eq!(input.up.foreground_hwnd, 1001);
        assert_eq!(input.down.foreground_pid, input.up.foreground_pid);
        for wrong in [
            "Default",
            "thread=Default;active=Secure",
            "thread=Secure;active=Default",
            "thread=Default",
            "active=Default",
            "thread=Default;active=Default;unknown=true",
        ] {
            let down = edge(1002, wrong);
            let release = edge(1001, wrong);
            assert!(!native_input_desktop_is_default(&down.input_desktop));
            assert!(!native_input_desktop_is_default(&release.input_desktop));
        }
    }

    #[test]
    fn l08_moved_root_position_requires_distinct_same_monitor_measured_geometry() {
        assert_eq!(
            moved_root_position([240, 180, 1156, 869], &[[0, 0, 1920, 1080]]).unwrap(),
            [320, 260]
        );
        assert_eq!(
            moved_root_position([-1680, 180, -764, 869], &[[-1920, 0, 0, 1080]]).unwrap(),
            [-1600, 260]
        );
        for (root, displays) in [
            ([0, 0, 900, 650], vec![[0, 0, 900, 650]]),
            ([240, 180, 1156, 869], vec![[-1920, 0, 0, 1080]]),
            ([100, 100, 50, 50], vec![[0, 0, 1920, 1080]]),
            (
                [i32::MIN, 0, i32::MAX, 650],
                vec![[i32::MIN, 0, i32::MAX, 1080]],
            ),
        ] {
            assert!(moved_root_position(root, &displays).is_err());
        }
    }

    #[test]
    fn query_ordinary_observation_follows_actual_preserved_dispatch_not_native_sampler_policy() {
        let actual = "trace_event=\"radial_query_dispatch\" elapsed_ms=12 invocation_id=5 session_digest=6 cell_digest=7 session_generation=2 config_revision=3 mode=execute_first query_digest=6 selected_digest=8 interaction_requirement=external root_policy=preserve outcome=executed";
        assert!(query_operation_requires_ordinary_observation(&[
            actual.into()
        ]));
        for changed in [
            actual.replace("root_policy=preserve", "root_policy=legacy"),
            actual.replace("outcome=executed", "outcome=confirmation_required"),
            actual.replace("selected_digest=8", "selected_digest=none"),
            actual.replace("radial_query_dispatch", "radial_query_resolution"),
        ] {
            assert!(!query_operation_requires_ordinary_observation(&[changed]));
        }
    }
}
