//! Transactional, typed action editing.
//!
//! The editor owns a complete `MkStep` clone.  No document field is borrowed by
//! the modal, which makes closing/cancelling it a genuinely lossless operation.
pub use super::image_authoring_destination::ConditionOperationDestination;
use super::variable_catalog::{VariableCatalog, VariableDescriptor, VariableValueType};
use super::{
    MkMacroDialog,
    key_capture::{CapturedChord, captured_chord, key_name},
};
use crate::mkmacro::variables::{MkPoint, MkValue};
use crate::mkmacro::*;
use eframe::egui;
use std::sync::Arc;

type NotificationPreview = Arc<dyn Fn(&ResolvedNotification) -> Result<(), String> + Send + Sync>;
type SoundPreview = Arc<dyn Fn(&str) + Send + Sync>;

pub struct ActionEditorState {
    pub draft: Option<MkStep>,
    /// `None` means insert a new row; otherwise replace this stable step id.
    pub editing_id: Option<u64>,
    /// Captured when the editor opens, so applying cannot accidentally use a
    /// selection which changed underneath the modal.
    pub insertion: Option<InsertionIntent>,
    pub capture_keys: bool,
    pub capture_message: Option<String>,
    pub editor: Option<super::action_catalog::EditorKind>,
    position_capture: Option<PositionCaptureState>,
    pub(crate) draft_generation: u64,
    /// Set by defensive asynchronous completion application, independently of document dirty state.
    pub(crate) draft_changed: bool,
    pub image_search: Option<super::image_search_editor::ImageSearchEditorState>,
    /// Typed requests collected by the widget and executed only when Apply confirms the draft.
    pub add_smooth_move: bool,
    pub add_activate_before: bool,
    pub image_authoring: super::image_authoring_job::ImageAuthoringJob,
    /// Cloneable operation client borrowed from the dialog-wide visual-overlay service.
    /// The editor owns only the operation IDs it starts, not the native service itself.
    pub visual_overlay: super::visual_capture_workflow::SharedVisualOverlayController,
    /// Installed by the owning launcher integration because it alone owns the
    /// launcher and dialog native-window visibility boundary.
    pub visual_capture: Option<super::visual_capture_workflow::VisualCaptureWorkflow>,
    overlay_diagnostic: Option<(super::visual_overlay::OperationId, String)>,
    active_point_pick: Option<super::visual_overlay::OperationId>,
    pending_visual_region: Option<PendingVisualRegionOperation>,
    picker: NativePositionPicker,
    notification_preview: NotificationPreview,
    sound_preview: SoundPreview,
    /// Rebuilt from the live document on every editor frame. The stable id is
    /// resolved against that document, so row moves cannot leave a stale scope.
    variable_catalog: VariableCatalog,
    variable_consumer_index: usize,
    variable_consumer_id: Option<u64>,
}

#[derive(Clone)]
enum PreviewRequest {
    Notification(ResolvedNotification),
    Sound(String),
}

fn production_notification_preview(notification: &ResolvedNotification) -> Result<(), String> {
    #[cfg(windows)]
    {
        use crate::mkmacro::NotificationBackend;
        crate::mkmacro::notifications::WindowsNotificationBackend::new()
            .notify(notification)
            .map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = notification;
        crate::mkmacro::notifications::initialize_desktop_notifications()
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertionIntent {
    Plain { after_step_id: Option<u64> },
    Wrap { step_ids: Vec<u64> },
    EditExisting { step_id: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VisualRegionDestination {
    CaptureScreenshotRegion,
    ImageActionSearchRegion,
    ImageActionReferenceAsset,
    ConditionSearchRegion(ConditionOperationDestination),
    WaitForVisualChangeRegion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpectedVisualAction {
    CaptureScreenshot,
    ImageFind,
    ImageClick,
    Condition,
    WaitForVisualChange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingVisualRegionOperation {
    destination: VisualRegionDestination,
    macro_id: u64,
    step_id: Option<u64>,
    draft_generation: u64,
    expected_action: ExpectedVisualAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionOperationResult {
    Asset(u64),
    Rectangle(ScreenRect),
    Matcher(MkWindowMatcher),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PositionCaptureSlot {
    MoveTarget,
    ClickTarget,
    DragFrom,
    DragTo,
    PixelPosition,
    PixelColor,
}
#[derive(Clone, Debug)]
struct PositionCaptureState {
    target_step_id: Option<u64>,
    draft_generation: u64,
    slot: PositionCaptureSlot,
    awaiting_release: bool,
    last_screen_position: Option<MkPoint>,
    #[cfg(windows)]
    foreground_window: isize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PositionCaptureEvent {
    PointerMoved(MkPoint),
    LeftPressed,
    LeftReleased,
    Enter,
    Escape,
}
pub trait PositionPicker {
    /// Polls once and returns immediately. Confirming clicks are deliberately
    /// allowed through to the underlying application (the launcher never hooks
    /// or suppresses input).
    fn poll_event(&mut self) -> Result<Option<PositionCaptureEvent>, String>;
}

#[derive(Default)]
struct NativePositionPicker {
    #[cfg(windows)]
    last_position: Option<MkPoint>,
    #[cfg(windows)]
    left_down: bool,
    #[cfg(windows)]
    enter_down: bool,
    #[cfg(windows)]
    escape_down: bool,
}

trait RegionPreviewBoundary {
    fn preview_desktop(
        &self,
        monitors: Vec<MonitorDescriptor>,
    ) -> super::visual_overlay::OperationId;
    fn highlight_monitor(&self, monitor: MonitorDescriptor) -> super::visual_overlay::OperationId;
    fn preview_rectangle(&self, rect: ScreenRect) -> super::visual_overlay::OperationId;
    fn highlight_window(
        &self,
        rect: ScreenRect,
        kind: super::visual_overlay::WindowAreaKind,
    ) -> super::visual_overlay::OperationId;
}
impl RegionPreviewBoundary for super::visual_capture_workflow::SharedVisualOverlayController {
    fn preview_desktop(
        &self,
        monitors: Vec<MonitorDescriptor>,
    ) -> super::visual_overlay::OperationId {
        self.preview_desktop(monitors)
    }
    fn highlight_monitor(&self, monitor: MonitorDescriptor) -> super::visual_overlay::OperationId {
        self.highlight_monitor(monitor)
    }
    fn preview_rectangle(&self, rect: ScreenRect) -> super::visual_overlay::OperationId {
        self.preview_rectangle(rect)
    }
    fn highlight_window(
        &self,
        rect: ScreenRect,
        kind: super::visual_overlay::WindowAreaKind,
    ) -> super::visual_overlay::OperationId {
        self.highlight_window(rect, kind)
    }
}

fn dispatch_region_preview(
    region: &SearchRegion,
    monitors: &Result<Vec<MonitorDescriptor>, String>,
    resolve_window: &dyn Fn(&MkWindowMatcher, bool) -> ExecResult<ScreenRect>,
    preview: &dyn RegionPreviewBoundary,
) -> Result<(super::visual_overlay::OperationId, String), String> {
    use super::visual_overlay::WindowAreaKind;
    let monitor_list = || {
        monitors
            .as_ref()
            .map_err(|e| format!("Monitor discovery failed: {e}"))
    };
    match region {
        SearchRegion::Desktop => {
            let descriptors = monitor_list()?;
            if descriptors.is_empty() {
                return Err("No monitors are currently available".into());
            }
            Ok((
                preview.preview_desktop(descriptors.clone()),
                "Unable to preview desktop monitors".into(),
            ))
        }
        SearchRegion::Monitor { index } => {
            let descriptor = monitor_list()?
                .iter()
                .find(|m| m.index == *index)
                .cloned()
                .ok_or_else(|| format!("Monitor index {index} no longer exists"))?;
            Ok((
                preview.highlight_monitor(descriptor),
                format!("Unable to highlight monitor {index}"),
            ))
        }
        SearchRegion::Rectangle { rect } => {
            if rect.is_empty() {
                return Err("Rectangle is invalid: width and height must be positive".into());
            }
            Ok((
                preview.preview_rectangle(*rect),
                format!("Unable to preview rectangle {rect:?}"),
            ))
        }
        SearchRegion::Window { matcher } | SearchRegion::ClientArea { matcher } => {
            let client = matches!(region, SearchRegion::ClientArea { .. });
            let rect = resolve_window(matcher, client).map_err(|e| match e.kind {
                DiagnosticKind::TargetNotFound => "Window target was not found".into(),
                DiagnosticKind::AmbiguousTarget => "Window matcher is ambiguous".into(),
                _ => format!("Unable to resolve window target: {e}"),
            })?;
            let kind = if client {
                WindowAreaKind::ClientArea
            } else {
                WindowAreaKind::WholeWindow
            };
            Ok((
                preview.highlight_window(rect, kind),
                format!(
                    "Unable to preview {}",
                    if client {
                        "client area"
                    } else {
                        "whole window"
                    }
                ),
            ))
        }
    }
}

impl ActionEditorState {
    pub fn new(
        visual_overlay: super::visual_capture_workflow::SharedVisualOverlayController,
    ) -> Self {
        Self {
            draft: None,
            editing_id: None,
            insertion: None,
            capture_keys: false,
            capture_message: None,
            editor: None,
            position_capture: None,
            draft_generation: 0,
            draft_changed: false,
            image_search: None,
            add_smooth_move: false,
            add_activate_before: false,
            image_authoring: Default::default(),
            visual_overlay,
            visual_capture: None,
            overlay_diagnostic: None,
            active_point_pick: None,
            pending_visual_region: None,
            picker: Default::default(),
            notification_preview: Arc::new(production_notification_preview),
            sound_preview: Arc::new(crate::sound::play_sound),
            variable_catalog: VariableCatalog::default(),
            variable_consumer_index: 0,
            variable_consumer_id: None,
        }
    }

    fn refresh_variable_catalog(&mut self, steps: &[MkStep]) {
        let index = if let Some(id) = self.editing_id {
            steps
                .iter()
                .position(|step| step.id == id)
                .unwrap_or(steps.len())
        } else {
            match self.insertion.as_ref() {
                Some(InsertionIntent::Plain {
                    after_step_id: Some(id),
                }) => steps
                    .iter()
                    .position(|step| step.id == *id)
                    .map_or(steps.len(), |index| index + 1),
                Some(InsertionIntent::Wrap { step_ids }) => step_ids
                    .iter()
                    .filter_map(|id| steps.iter().position(|step| step.id == *id))
                    .min()
                    .unwrap_or(steps.len()),
                _ => 0,
            }
        };
        self.variable_consumer_index = index;
        self.variable_consumer_id = self.editing_id;
        self.variable_catalog = VariableCatalog::before_step(steps, index);
    }
    fn cancel_owned_passive_overlay(&mut self) {
        if let Some(operation_id) = self.active_point_pick.take() {
            self.visual_overlay.cancel_operation(operation_id);
        }
        if let Some((operation_id, _)) = self.overlay_diagnostic.take() {
            self.visual_overlay.cancel_operation(operation_id);
        }
    }
    fn cancel_visual_capture(&mut self) {
        self.pending_visual_region = None;
        if let Some(workflow) = &mut self.visual_capture {
            workflow.cancel();
            while workflow.active() {
                workflow.tick();
            }
            let _ = workflow.take_completed();
        }
    }
    fn preview_region(&mut self, region: SearchRegion) {
        let monitors = if matches!(region, SearchRegion::Desktop | SearchRegion::Monitor { .. }) {
            crate::mkmacro::monitor_descriptors().map_err(|e| e.to_string())
        } else {
            Ok(vec![])
        };
        match dispatch_region_preview(
            &region,
            &monitors,
            &crate::mkmacro::resolve_window_screen_rect,
            &self.visual_overlay,
        ) {
            Ok((id, context)) => {
                self.overlay_diagnostic = Some((id, context));
                self.capture_message = None;
            }
            Err(message) => self.capture_message = Some(message),
        }
    }
    pub fn condition_destination(
        &self,
        macro_id: u64,
        request: super::condition_editor::ConditionImageRequest,
    ) -> ConditionOperationDestination {
        ConditionOperationDestination {
            macro_id,
            step_id: self.editing_id,
            draft_generation: self.draft_generation,
            path: request.path,
            operation: request.operation,
        }
    }
    /// Applies only to the exact still-live condition and compatible field selected when work began.
    pub fn apply_condition_result(
        &mut self,
        destination: &ConditionOperationDestination,
        result: ConditionOperationResult,
        current_macro_id: Option<u64>,
    ) -> bool {
        if current_macro_id != Some(destination.macro_id)
            || self.draft_generation != destination.draft_generation
            || self.editing_id != destination.step_id
        {
            return false;
        }
        let Some(step) = self.draft.as_mut() else {
            return false;
        };
        let root = match &mut step.action {
            MkAction::If(c)
            | MkAction::WhileStart { condition: c }
            | MkAction::WaitUntil { condition: c, .. } => c,
            _ => return false,
        };
        let Some(MkCondition::ImageSearch { search, .. }) =
            super::condition_editor::resolve_condition_mut(root, &destination.path)
        else {
            return false;
        };
        let applied = match (&destination.operation, result) {
            (
                super::condition_editor::ConditionImageOperation::ImportPng
                | super::condition_editor::ConditionImageOperation::CaptureRectangle,
                ConditionOperationResult::Asset(id),
            ) => {
                search.asset_id = id;
                true
            }
            (
                super::condition_editor::ConditionImageOperation::PickRectangle,
                ConditionOperationResult::Rectangle(rect),
            ) if matches!(search.region, SearchRegion::Rectangle { .. }) => {
                search.region = SearchRegion::Rectangle { rect };
                true
            }
            (
                super::condition_editor::ConditionImageOperation::PickWindow,
                ConditionOperationResult::Matcher(matcher),
            ) => match &mut search.region {
                SearchRegion::Window { matcher: m } | SearchRegion::ClientArea { matcher: m } => {
                    *m = matcher;
                    true
                }
                _ => false,
            },
            _ => false,
        };
        if applied {
            self.draft_changed = true;
        }
        applied
    }
    fn start_import_from_selected_path(
        &mut self,
        store: std::sync::Arc<MkMacroStore>,
        macro_id: u64,
        path: std::path::PathBuf,
    ) -> anyhow::Result<()> {
        self.start_import_from_selected_path_with_executor(
            store,
            macro_id,
            path,
            &super::image_authoring_job::ThreadExecutor,
        )
    }

    fn start_import_from_selected_path_with_executor(
        &mut self,
        store: std::sync::Arc<MkMacroStore>,
        macro_id: u64,
        path: std::path::PathBuf,
        executor: &dyn super::image_authoring_job::ImageAuthoringExecutor,
    ) -> anyhow::Result<()> {
        let token = super::image_authoring_job::DraftToken {
            macro_id,
            draft_generation: self.draft_generation,
        };
        let previous = self
            .draft
            .as_mut()
            .and_then(image_payload_mut)
            .ok_or_else(|| anyhow::anyhow!("no image-action draft is open"))?
            .asset_id;
        self.image_authoring
            .start_with_executor(
                store,
                token,
                super::image_authoring_job::ImageAuthoringDestination::ImageActionReference,
                previous,
                path,
                executor,
            )
            .map_err(anyhow::Error::msg)
    }

    fn start_condition_import_from_selected_path(
        &mut self,
        store: Arc<MkMacroStore>,
        destination: ConditionOperationDestination,
        path: std::path::PathBuf,
    ) -> anyhow::Result<()> {
        self.start_condition_import_from_selected_path_with_executor(
            store,
            destination,
            path,
            &super::image_authoring_job::ThreadExecutor,
        )
    }

    fn start_condition_import_from_selected_path_with_executor(
        &mut self,
        store: Arc<MkMacroStore>,
        destination: ConditionOperationDestination,
        path: std::path::PathBuf,
        executor: &dyn super::image_authoring_job::ImageAuthoringExecutor,
    ) -> anyhow::Result<()> {
        let previous = self
            .draft
            .as_ref()
            .and_then(|step| match &step.action {
                MkAction::If(c)
                | MkAction::WhileStart { condition: c }
                | MkAction::WaitUntil { condition: c, .. } => {
                    super::condition_editor::resolve_condition(c, &destination.path)
                }
                _ => None,
            })
            .and_then(|c| match c {
                MkCondition::ImageSearch { search, .. } => Some(search.asset_id),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("the selected image condition no longer exists"))?;
        let token = super::image_authoring_job::DraftToken {
            macro_id: destination.macro_id,
            draft_generation: destination.draft_generation,
        };
        self.image_authoring
            .start_with_executor(
                store,
                token,
                super::image_authoring_job::ImageAuthoringDestination::ConditionImage(destination),
                previous,
                path,
                executor,
            )
            .map_err(anyhow::Error::msg)
    }
    /// Starts the one user-drawn desktop rectangle operation owned by the action
    /// editor. Callers must choose the semantic purpose at the point where they
    /// translate their typed UI request; the workflow never guesses it from the
    /// draft, selected region, or button text.
    pub(crate) fn request_rectangle_selection(
        &mut self,
        macro_id: u64,
        purpose: super::visual_overlay::RectanglePurpose,
        destination: VisualRegionDestination,
    ) -> anyhow::Result<()> {
        self.pending_visual_region = None;
        if let Some(workflow) = &mut self.visual_capture {
            if workflow.active() {
                workflow.cancel();
                while workflow.active() {
                    workflow.tick();
                }
                let _ = workflow.take_completed();
            }
        }
        if self.image_authoring.is_importing() || self.position_capture.is_some() {
            anyhow::bail!("an authoring operation is already active");
        }
        let generation = self.draft_generation;
        let expected_action = match self.draft.as_ref().map(|step| &step.action) {
            Some(MkAction::ImageFind(_)) => ExpectedVisualAction::ImageFind,
            Some(MkAction::ImageClick(_)) => ExpectedVisualAction::ImageClick,
            Some(MkAction::If(_))
            | Some(MkAction::WhileStart { .. })
            | Some(MkAction::WaitUntil { .. }) => ExpectedVisualAction::Condition,
            Some(MkAction::WaitForVisualChange(_)) => ExpectedVisualAction::WaitForVisualChange,
            Some(MkAction::CaptureScreenshot(_)) => ExpectedVisualAction::CaptureScreenshot,
            _ => anyhow::bail!("draft does not support rectangle selection"),
        };
        let compatible = matches!(
            (&destination, expected_action, purpose),
            (
                VisualRegionDestination::ImageActionSearchRegion,
                ExpectedVisualAction::ImageFind | ExpectedVisualAction::ImageClick,
                super::visual_overlay::RectanglePurpose::SearchRegion
            ) | (
                VisualRegionDestination::ImageActionReferenceAsset,
                ExpectedVisualAction::ImageFind | ExpectedVisualAction::ImageClick,
                super::visual_overlay::RectanglePurpose::ReferenceImageCapture
            ) | (
                VisualRegionDestination::ConditionSearchRegion(_),
                ExpectedVisualAction::Condition,
                _
            ) | (
                VisualRegionDestination::WaitForVisualChangeRegion,
                ExpectedVisualAction::WaitForVisualChange,
                super::visual_overlay::RectanglePurpose::SearchRegion
            ) | (
                VisualRegionDestination::CaptureScreenshotRegion,
                ExpectedVisualAction::CaptureScreenshot,
                super::visual_overlay::RectanglePurpose::SearchRegion
            )
        );
        if !compatible {
            anyhow::bail!("rectangle purpose and destination do not match the draft");
        }
        let workflow = self
            .visual_capture
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("visual capture integration is unavailable"))?;
        workflow
            .begin(
                super::visual_capture_workflow::DraftToken {
                    macro_id,
                    draft_generation: generation,
                },
                purpose,
            )
            .map_err(anyhow::Error::msg)?;
        self.pending_visual_region = Some(PendingVisualRegionOperation {
            destination,
            macro_id,
            step_id: self.editing_id,
            draft_generation: generation,
            expected_action,
        });
        Ok(())
    }

    pub fn apply_window_matcher(
        &mut self,
        request: &super::window_picker::MatcherEditRequest,
        matcher: MkWindowMatcher,
        current_macro_id: Option<u64>,
    ) -> bool {
        let super::window_picker::MatcherDestination::Action {
            macro_id,
            draft_generation,
            path,
        } = &request.destination;
        if Some(*macro_id) != current_macro_id || *draft_generation != self.draft_generation {
            return false;
        }
        let Some(step) = self.draft.as_mut() else {
            return false;
        };
        if matches!(path, super::window_picker::MatcherPath::VisualRegion) {
            if !matches!(
                step.action,
                MkAction::ImageFind(_)
                    | MkAction::ImageClick(_)
                    | MkAction::WaitForVisualChange(_)
                    | MkAction::CaptureScreenshot(_)
            ) {
                return false;
            }
            let Some(image) = self.image_search.as_mut() else {
                return false;
            };
            if image.kind == super::image_search_editor::SearchRegionKind::ClientArea {
                image.client_matcher = matcher;
            } else if image.kind == super::image_search_editor::SearchRegionKind::Window {
                image.window_matcher = matcher;
            } else {
                return false;
            }
            return true;
        }
        let Some(target) = matcher_at_path(&mut step.action, path) else {
            return false;
        };
        if *target != matcher {
            *target = matcher;
        }
        true
    }
    pub fn begin_new(&mut self, action: MkAction) {
        let editor = super::action_catalog::editor_for_action(&action);
        self.begin_new_with_editor(action, editor);
    }
    pub fn begin_new_with_editor(
        &mut self,
        action: MkAction,
        editor: super::action_catalog::EditorKind,
    ) {
        if self.draft.is_some() {
            return;
        }
        self.cancel_owned_passive_overlay();
        self.cancel_visual_capture();
        self.image_authoring = Default::default();
        self.stop_position_capture();
        self.draft_generation = self.draft_generation.wrapping_add(1);
        self.editing_id = None;
        self.draft_changed = false;
        self.add_smooth_move = false;
        self.add_activate_before = false;
        // The dialog supplies the precise insertion intent immediately after
        // this call. This fallback keeps programmatic callers insertion-safe.
        self.insertion = Some(InsertionIntent::Plain {
            after_step_id: None,
        });
        self.editor = Some(editor);
        self.image_search = match &action {
            MkAction::ImageFind(p) | MkAction::ImageClick(p) => {
                Some(super::image_search_editor::ImageSearchEditorState::from_region(&p.region))
            }
            MkAction::WaitForVisualChange(p) => {
                Some(super::image_search_editor::ImageSearchEditorState::from_region(&p.region))
            }
            MkAction::CaptureScreenshot(p) => {
                Some(super::image_search_editor::ImageSearchEditorState::from_region(&p.region))
            }
            _ => None,
        };
        self.draft = Some(MkStep {
            id: 0,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action,
        });
    }
    pub fn begin_edit(&mut self, step: &MkStep) {
        self.cancel_owned_passive_overlay();
        self.cancel_visual_capture();
        self.image_authoring = Default::default();
        self.stop_position_capture();
        self.draft_generation = self.draft_generation.wrapping_add(1);
        self.editing_id = Some(step.id);
        self.draft_changed = false;
        self.add_smooth_move = false;
        self.add_activate_before = false;
        self.insertion = Some(InsertionIntent::EditExisting { step_id: step.id });
        self.draft = Some(step.clone());
        self.image_search = match &step.action {
            MkAction::ImageFind(p) | MkAction::ImageClick(p) => {
                Some(super::image_search_editor::ImageSearchEditorState::from_region(&p.region))
            }
            MkAction::WaitForVisualChange(p) => {
                Some(super::image_search_editor::ImageSearchEditorState::from_region(&p.region))
            }
            MkAction::CaptureScreenshot(p) => {
                Some(super::image_search_editor::ImageSearchEditorState::from_region(&p.region))
            }
            _ => None,
        };
        self.editor = Some(super::action_catalog::editor_for_action(&step.action));
    }
    pub fn cancel(&mut self) {
        self.pending_visual_region = None;
        self.image_authoring = Default::default();
        if let Some(workflow) = &mut self.visual_capture {
            workflow.cancel();
            // Cancellation synchronously releases the active overlay before the draft is discarded.
            while workflow.active() {
                workflow.tick();
            }
        }
        self.cancel_owned_passive_overlay();
        self.stop_position_capture();
        self.draft = None;
        self.editing_id = None;
        self.insertion = None;
        self.capture_keys = false;
        self.editor = None;
        self.image_search = None;
    }

    pub fn tick_visual_capture(&mut self, current_macro_id: Option<u64>) {
        let Some(workflow) = &mut self.visual_capture else {
            return;
        };
        workflow.tick();
        let Some(outcome) = workflow.take_completed() else {
            return;
        };
        self.apply_visual_capture_outcome(current_macro_id, outcome);
    }
    fn apply_visual_capture_outcome(
        &mut self,
        current_macro_id: Option<u64>,
        outcome: super::visual_capture_workflow::WorkflowOutcome,
    ) {
        use super::visual_capture_workflow::WorkflowOutcome;
        match outcome {
            WorkflowOutcome::Region { token, rect } => {
                let token_matches = self.pending_visual_region.as_ref().is_some_and(|pending| {
                    pending.macro_id == token.macro_id
                        && pending.draft_generation == token.draft_generation
                });
                if !token_matches {
                    return;
                }
                let pending = self.pending_visual_region.take().unwrap();
                let valid = current_macro_id == Some(pending.macro_id)
                    && self.draft_generation == pending.draft_generation
                    && self.editing_id == pending.step_id
                    && self
                        .draft
                        .as_ref()
                        .is_some_and(|step| match pending.expected_action {
                            ExpectedVisualAction::ImageFind => {
                                matches!(step.action, MkAction::ImageFind(_))
                            }
                            ExpectedVisualAction::ImageClick => {
                                matches!(step.action, MkAction::ImageClick(_))
                            }
                            ExpectedVisualAction::Condition => matches!(
                                step.action,
                                MkAction::If(_)
                                    | MkAction::WhileStart { .. }
                                    | MkAction::WaitUntil { .. }
                            ),
                            ExpectedVisualAction::WaitForVisualChange => {
                                matches!(step.action, MkAction::WaitForVisualChange(_))
                            }
                            ExpectedVisualAction::CaptureScreenshot => {
                                matches!(step.action, MkAction::CaptureScreenshot(_))
                            }
                        });
                if !valid {
                    return;
                }
                let applied = match pending.destination {
                    VisualRegionDestination::ImageActionSearchRegion => {
                        if let Some(image) = self.image_search.as_mut() {
                            image.rectangle = rect;
                            image.kind = super::image_search_editor::SearchRegionKind::Rectangle;
                            true
                        } else {
                            false
                        }
                    }
                    VisualRegionDestination::ConditionSearchRegion(destination) => self
                        .apply_condition_result(
                            &destination,
                            ConditionOperationResult::Rectangle(rect),
                            current_macro_id,
                        ),
                    VisualRegionDestination::WaitForVisualChangeRegion => {
                        if let (Some(step), Some(image)) = (&mut self.draft, &mut self.image_search)
                            && let MkAction::WaitForVisualChange(payload) = &mut step.action
                        {
                            let region = SearchRegion::Rectangle { rect };
                            payload.region = region.clone();
                            image.rectangle = rect;
                            image.kind = super::image_search_editor::SearchRegionKind::Rectangle;
                            true
                        } else {
                            false
                        }
                    }
                    VisualRegionDestination::CaptureScreenshotRegion => {
                        if let (Some(step), Some(region_editor)) =
                            (&mut self.draft, &mut self.image_search)
                            && let MkAction::CaptureScreenshot(payload) = &mut step.action
                        {
                            payload.region = SearchRegion::Rectangle { rect };
                            region_editor.rectangle = rect;
                            region_editor.kind =
                                super::image_search_editor::SearchRegionKind::Rectangle;
                            true
                        } else {
                            false
                        }
                    }
                    VisualRegionDestination::ImageActionReferenceAsset => false,
                };
                if applied {
                    self.draft_changed = true;
                }
            }
            WorkflowOutcome::Asset { token, asset_id } => {
                if self.pending_visual_region.as_ref().is_some_and(|pending| {
                    pending.macro_id == token.macro_id
                        && pending.draft_generation == token.draft_generation
                        && current_macro_id == Some(token.macro_id)
                        && self.draft_generation == token.draft_generation
                        && self.editing_id == pending.step_id
                }) {
                    let pending = self.pending_visual_region.take().unwrap();
                    match pending.destination {
                        VisualRegionDestination::ImageActionReferenceAsset => {
                            if let Some(payload) = self.draft.as_mut().and_then(image_payload_mut) {
                                payload.asset_id = asset_id;
                                self.draft_changed = true;
                            }
                        }
                        VisualRegionDestination::ConditionSearchRegion(destination) => {
                            self.apply_condition_result(
                                &destination,
                                ConditionOperationResult::Asset(asset_id),
                                current_macro_id,
                            );
                        }
                        _ => {}
                    }
                }
            }
            WorkflowOutcome::Failed(message) => {
                self.pending_visual_region = None;
                self.capture_message = Some(message)
            }
            WorkflowOutcome::Cancelled => {
                self.pending_visual_region = None;
                self.capture_message = Some("Visual capture cancelled".into())
            }
        }
    }
    fn sync_image_region_to_draft(&mut self) {
        if let (Some(step), Some(image)) = (&mut self.draft, &self.image_search) {
            let region = image.selected_region();
            match &mut step.action {
                MkAction::ImageFind(p) | MkAction::ImageClick(p) => p.region = region,
                MkAction::WaitForVisualChange(p) => p.region = region,
                MkAction::CaptureScreenshot(p) => p.region = region,
                _ => {}
            }
        }
    }
    fn poll_visual_overlay(&mut self, current_macro_id: Option<u64>) {
        for event in self.visual_overlay.poll() {
            match event {
                super::visual_overlay::VisualOverlayEvent::PointConfirmed {
                    operation_id,
                    request,
                    point,
                } => {
                    if self.active_point_pick == Some(operation_id) {
                        self.active_point_pick = None;
                        self.apply_point_confirmation(&request, point, current_macro_id);
                    }
                }
                super::visual_overlay::VisualOverlayEvent::Cancelled { operation_id }
                    if self.active_point_pick == Some(operation_id) =>
                {
                    self.active_point_pick = None;
                }
                super::visual_overlay::VisualOverlayEvent::Error {
                    operation_id,
                    ref error,
                } if self.active_point_pick == Some(operation_id) => {
                    self.active_point_pick = None;
                    self.capture_message = Some(format!("Unable to pick position: {error}"));
                }
                event => apply_overlay_diagnostic(
                    &mut self.capture_message,
                    &self.overlay_diagnostic,
                    event,
                ),
            }
        }
    }
    fn apply_point_confirmation(
        &mut self,
        request: &super::visual_overlay::VisualPointRequest,
        point: MkPoint,
        current_macro_id: Option<u64>,
    ) -> bool {
        if current_macro_id != Some(request.macro_id)
            || self.draft_generation != request.draft_generation
            || self.editing_id != request.step_id
            || !matches!(
                request.destination,
                super::visual_overlay::VisualPointDestination::SetVariablePoint
            )
        {
            return false;
        }
        let Some(MkStep {
            action:
                MkAction::SetVariable {
                    value: MkValue::Point(target),
                    ..
                },
            ..
        }) = self.draft.as_mut()
        else {
            return false;
        };
        *target = point;
        self.draft_changed = true;
        true
    }
    fn request_point_pick(&mut self, macro_id: u64) {
        if self.active_point_pick.is_some() {
            return;
        }
        if !matches!(
            self.draft.as_ref().map(|s| &s.action),
            Some(MkAction::SetVariable {
                value: MkValue::Point(_),
                ..
            })
        ) {
            return;
        }
        let id = self
            .visual_overlay
            .begin_point_pick(super::visual_overlay::VisualPointRequest {
                macro_id,
                draft_generation: self.draft_generation,
                step_id: self.editing_id,
                destination: super::visual_overlay::VisualPointDestination::SetVariablePoint,
            });
        self.active_point_pick = Some(id);
        self.capture_message = None;
    }
    pub fn apply(&mut self, dialog: &mut MkMacroDialog) -> Option<u64> {
        if self.image_authoring.is_importing() {
            return None;
        }
        self.image_authoring = Default::default();
        self.pending_visual_region = None;
        if let Some(workflow) = &mut self.visual_capture {
            workflow.cancel();
            while workflow.active() {
                workflow.tick();
            }
        }
        self.cancel_owned_passive_overlay();
        self.stop_position_capture();
        self.sync_image_region_to_draft();
        let mut step = self.draft.take()?;
        normalize_optional_outputs(&mut step.action);
        let edit = self.editing_id.take();
        let intent = self.insertion.take().unwrap_or_else(|| match edit {
            Some(step_id) => InsertionIntent::EditExisting { step_id },
            None => InsertionIntent::Plain {
                after_step_id: None,
            },
        });
        self.editor = None;
        // New block openers are inserted together with their mandatory closing
        // marker, while the configured step settings remain on the opener.
        if !matches!(intent, InsertionIntent::EditExisting { .. })
            && matches!(
                &step.action,
                MkAction::If(_) | MkAction::RepeatStart { .. } | MkAction::WhileStart { .. }
            )
        {
            return match super::action_catalog::apply_structural(dialog, step, intent) {
                Ok(id) => Some(id),
                Err(error) => {
                    dialog.command_error = Some(error);
                    None
                }
            };
        }
        let m = dialog.selected_macro_mut()?;
        let index = if let InsertionIntent::EditExisting { step_id: id } = intent {
            let i = m.steps.iter().position(|s| s.id == id)?;
            step.id = id;
            m.steps[i] = step;
            i
        } else {
            step.id = 0;
            let after_step_id = match intent {
                InsertionIntent::Plain { after_step_id } => after_step_id,
                InsertionIntent::Wrap { .. } | InsertionIntent::EditExisting { .. } => None,
            };
            let i = after_step_id
                .and_then(|id| m.steps.iter().position(|s| s.id == id))
                .map_or(m.steps.len(), |i| i + 1);
            m.steps.insert(i, step);
            i
        };
        crate::mkmacro::repair_ids(&mut dialog.draft);
        let id = dialog.selected_macro()?.steps[index].id;
        dialog.selection.ids.clear();
        dialog.selection.ids.insert(id);
        dialog.mark_dirty();
        Some(id)
    }

    fn poll_image_authoring(
        &mut self,
        current_macro_id: Option<u64>,
    ) -> Option<crate::mkmacro::StagedImageAsset> {
        let Some((active_token, active_destination, previous_asset_id, source, completion)) =
            self.image_authoring.try_take()
        else {
            return None;
        };
        // The receiver itself identifies the active job; this extra comparison makes
        // it impossible for a queued completion to retire a subsequently started job.
        if completion.token != active_token || completion.destination != active_destination {
            return None;
        }
        self.image_authoring = Default::default();
        match completion.result {
            Ok(staged) => {
                let applied = match &active_destination {
                    super::image_authoring_job::ImageAuthoringDestination::ImageActionReference => {
                        if current_macro_id != Some(active_token.macro_id)
                            || self.draft_generation != active_token.draft_generation
                        {
                            false
                        } else if let Some(payload) =
                            self.draft.as_mut().and_then(image_payload_mut)
                        {
                            if payload.asset_id != previous_asset_id {
                                false
                            } else {
                                payload.asset_id = staged.asset_id;
                                true
                            }
                        } else {
                            false
                        }
                    }
                    super::image_authoring_job::ImageAuthoringDestination::ConditionImage(
                        destination,
                    ) => {
                        if !matches!(
                            destination.operation,
                            super::condition_editor::ConditionImageOperation::ImportPng
                        ) {
                            false
                        } else if current_macro_id != Some(destination.macro_id)
                            || self.draft_generation != destination.draft_generation
                            || self.editing_id != destination.step_id
                        {
                            false
                        } else {
                            let node =
                                self.draft.as_mut().and_then(|step| match &mut step.action {
                                    MkAction::If(c)
                                    | MkAction::WhileStart { condition: c }
                                    | MkAction::WaitUntil { condition: c, .. } => {
                                        super::condition_editor::resolve_condition_mut(
                                            c,
                                            &destination.path,
                                        )
                                    }
                                    _ => None,
                                });
                            match node {
                                Some(MkCondition::ImageSearch { search, .. })
                                    if search.asset_id == previous_asset_id =>
                                {
                                    search.asset_id = staged.asset_id;
                                    true
                                }
                                _ => false,
                            }
                        }
                    }
                };
                if applied {
                    self.draft_changed = true;
                    self.capture_message = None;
                    return Some(staged);
                }
            }
            Err(error) => {
                let name = source
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("selected PNG");
                self.capture_message = Some(format!("{error} ({name}, {active_destination:?})"));
            }
        }
        None
    }
    fn stop_position_capture(&mut self) {
        self.position_capture = None;
        self.capture_message = None;
    }

    fn start_position_capture(&mut self, slot: PositionCaptureSlot) -> Result<(), String> {
        #[cfg(not(windows))]
        {
            let _ = slot;
            return Err("Position capture is available only on Windows".into());
        }
        #[cfg(windows)]
        {
            use ::windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
            self.position_capture = Some(PositionCaptureState {
                target_step_id: self.editing_id,
                draft_generation: self.draft_generation,
                slot,
                awaiting_release: true,
                last_screen_position: None,
                foreground_window: unsafe { GetForegroundWindow() }.0 as isize,
            });
            self.capture_message = None;
            Ok(())
        }
    }

    fn target_mut(&mut self, slot: PositionCaptureSlot) -> Option<&mut MkCoordinateTarget> {
        let action = &mut self.draft.as_mut()?.action;
        match (slot, action) {
            (PositionCaptureSlot::MoveTarget, MkAction::MouseMove(p)) => Some(&mut p.target),
            (PositionCaptureSlot::ClickTarget, MkAction::MouseClick(p)) => Some(&mut p.target),
            (PositionCaptureSlot::DragFrom, MkAction::MouseDrag(p)) => Some(&mut p.from),
            (PositionCaptureSlot::DragTo, MkAction::MouseDrag(p)) => Some(&mut p.to),
            (PositionCaptureSlot::PixelPosition, MkAction::PixelCheck { target, .. }) => {
                Some(target)
            }
            _ => None,
        }
    }

    fn process_position_event(&mut self, event: PositionCaptureEvent) {
        let Some(mut capture) = self.position_capture.take() else {
            return;
        };
        if capture.target_step_id != self.editing_id
            || capture.draft_generation != self.draft_generation
        {
            self.capture_message =
                Some("Position capture cancelled because the edited action changed".into());
            return;
        }
        match event {
            PositionCaptureEvent::PointerMoved(p) => {
                capture.last_screen_position = Some(p);
                self.position_capture = Some(capture);
            }
            PositionCaptureEvent::LeftReleased if capture.awaiting_release => {
                capture.awaiting_release = false;
                self.position_capture = Some(capture);
            }
            PositionCaptureEvent::LeftPressed if !capture.awaiting_release => {
                self.confirm_position(capture)
            }
            PositionCaptureEvent::Enter => self.confirm_position(capture),
            PositionCaptureEvent::Escape => {
                self.capture_message = Some("Position capture cancelled".into());
            }
            _ => self.position_capture = Some(capture),
        }
    }

    fn confirm_position(&mut self, capture: PositionCaptureState) {
        self.confirm_position_with(capture, read_live_desktop_pixel);
    }

    fn confirm_position_with(
        &mut self,
        capture: PositionCaptureState,
        read_pixel: impl FnOnce(MkPoint) -> Result<[u8; 3], String>,
    ) {
        let Some(screen) = capture.last_screen_position else {
            self.capture_message = Some("Unable to read the current pointer position".into());
            return;
        };
        if capture.slot == PositionCaptureSlot::PixelColor {
            let result = read_pixel(screen).map(|rgb| {
                if let Some(MkStep {
                    action: MkAction::PixelCheck { color, .. },
                    ..
                }) = self.draft.as_mut()
                {
                    *color = crate::mkmacro::screen::format_rgb(rgb);
                }
            });
            self.capture_message = result.err();
            return;
        }
        let needs_matched = matches!(
            self.target_mut(capture.slot),
            Some(MkCoordinateTarget::WindowClient { .. })
        );
        let window = if needs_matched {
            picked_window_at(screen)
        } else {
            picked_foreground_window(&capture)
        };
        let result = match self.target_mut(capture.slot) {
            Some(
                target @ (MkCoordinateTarget::ActiveWindow { .. }
                | MkCoordinateTarget::WindowClient { .. }),
            ) => window.and_then(|w| apply_picked_position(target, screen, Some(&w))),
            Some(target) => apply_picked_position(target, screen, None),
            None => Err("The captured field no longer exists".into()),
        };
        self.capture_message = result.err();
    }
    /// Applies a platform-independent captured chord. Modifier-only captures are invalid.
    pub fn set_captured_keys(&mut self, mut keys: Vec<MkKey>) -> bool {
        let Some(step) = &mut self.draft else {
            return false;
        };
        keys.dedup();
        let Some(primary) = keys.iter().rposition(|k| !is_modifier(k)) else {
            return false;
        };
        let key = keys.remove(primary);
        step.action = match step.action {
            MkAction::KeyDown(_) => MkAction::KeyDown(key),
            MkAction::KeyUp(_) => MkAction::KeyUp(key),
            _ if keys.is_empty() => MkAction::KeyPress(key),
            _ => {
                keys.push(key);
                MkAction::Hotkey(keys)
            }
        };
        self.capture_keys = false;
        true
    }
}

fn normalize_optional_outputs(action: &mut MkAction) {
    match action {
        MkAction::ImageFind(payload) | MkAction::ImageClick(payload) => payload.outputs.normalize(),
        MkAction::FindPixel(payload) => payload.outputs.normalize(),
        MkAction::CaptureScreenshot(payload) => {
            payload.path_output = payload.path_output.take().and_then(|name| {
                let name = name.trim();
                (!name.is_empty()).then(|| name.to_owned())
            });
        }
        _ => {}
    }
}

fn apply_overlay_diagnostic(
    message: &mut Option<String>,
    current: &Option<(super::visual_overlay::OperationId, String)>,
    event: super::visual_overlay::VisualOverlayEvent,
) {
    if let super::visual_overlay::VisualOverlayEvent::Error {
        operation_id,
        error,
    } = event
        && current.as_ref().map(|v| v.0) == Some(operation_id)
    {
        *message = Some(format!("{}: {error}", current.as_ref().unwrap().1));
    }
}

impl Drop for ActionEditorState {
    fn drop(&mut self) {
        if let Some(workflow) = &mut self.visual_capture {
            workflow.cancel();
            while workflow.active() {
                workflow.tick();
            }
        }
        self.cancel_owned_passive_overlay();
    }
}

#[cfg(windows)]
fn read_live_desktop_pixel(point: MkPoint) -> Result<[u8; 3], String> {
    let rgba = WindowsScreenCaptureBackend::system()
        .read_pixel(point)
        .map_err(|e| e.to_string())?;
    Ok([rgba[0], rgba[1], rgba[2]])
}
#[cfg(not(windows))]
fn read_live_desktop_pixel(_: MkPoint) -> Result<[u8; 3], String> {
    Err("Desktop color picking is available only on Windows".into())
}

fn is_modifier(k: &MkKey) -> bool {
    matches!(
        k,
        MkKey::Control
            | MkKey::LeftControl
            | MkKey::RightControl
            | MkKey::Alt
            | MkKey::LeftAlt
            | MkKey::RightAlt
            | MkKey::Shift
            | MkKey::LeftShift
            | MkKey::RightShift
            | MkKey::Meta
            | MkKey::LeftMeta
            | MkKey::RightMeta
    )
}

#[cfg(windows)]
impl PositionPicker for NativePositionPicker {
    fn poll_event(&mut self) -> Result<Option<PositionCaptureEvent>, String> {
        use ::windows::Win32::{
            Foundation::POINT,
            UI::{
                Input::KeyboardAndMouse::{
                    GetAsyncKeyState, VIRTUAL_KEY, VK_ESCAPE, VK_LBUTTON, VK_RETURN,
                },
                WindowsAndMessaging::GetCursorPos,
            },
        };
        let mut raw = POINT::default();
        unsafe { GetCursorPos(&mut raw) }.map_err(|e| format!("GetCursorPos failed: {e}"))?;
        let point = MkPoint { x: raw.x, y: raw.y };
        if self.last_position != Some(point) {
            self.last_position = Some(point);
            return Ok(Some(PositionCaptureEvent::PointerMoved(point)));
        }
        let down = |key: VIRTUAL_KEY| unsafe { GetAsyncKeyState(key.0 as i32) } < 0;
        let left = down(VK_LBUTTON);
        let enter = down(VK_RETURN);
        let escape = down(VK_ESCAPE);
        let event = if escape && !self.escape_down {
            Some(PositionCaptureEvent::Escape)
        } else if enter && !self.enter_down {
            Some(PositionCaptureEvent::Enter)
        } else if left != self.left_down {
            Some(if left {
                PositionCaptureEvent::LeftPressed
            } else {
                PositionCaptureEvent::LeftReleased
            })
        } else {
            None
        };
        self.left_down = left;
        self.enter_down = enter;
        self.escape_down = escape;
        Ok(event)
    }
}
#[cfg(not(windows))]
impl PositionPicker for NativePositionPicker {
    fn poll_event(&mut self) -> Result<Option<PositionCaptureEvent>, String> {
        Err("Position capture is available only on Windows".into())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct PickedWindow {
    pub matcher: MkWindowMatcher,
    pub client_origin: MkPoint,
}

#[cfg(windows)]
fn picked_foreground_window(c: &PositionCaptureState) -> Result<PickedWindow, String> {
    use ::windows::Win32::{
        Foundation::{HWND, POINT},
        Graphics::Gdi::ClientToScreen,
        UI::WindowsAndMessaging::GetForegroundWindow,
    };
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() || hwnd != HWND(c.foreground_window as *mut core::ffi::c_void) {
        return Err("The active window changed or disappeared during capture".into());
    }
    let mut origin = POINT::default();
    if !unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool() {
        return Err(format!(
            "Could not determine the active window client origin: {}",
            ::windows::core::Error::from_win32()
        ));
    }
    Ok(PickedWindow {
        matcher: MkWindowMatcher {
            title: None,
            title_regex: None,
            process: None,
            class: None,
        },
        client_origin: MkPoint {
            x: origin.x,
            y: origin.y,
        },
    })
}
#[cfg(not(windows))]
fn picked_foreground_window(_: &PositionCaptureState) -> Result<PickedWindow, String> {
    Err("Active-window capture is available only on Windows".into())
}
#[cfg(windows)]
fn picked_window_at(screen: MkPoint) -> Result<PickedWindow, String> {
    use ::windows::Win32::{
        Foundation::{HWND, POINT},
        Graphics::Gdi::ClientToScreen,
        UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, IsWindow, WindowFromPoint},
    };
    let child = unsafe {
        WindowFromPoint(POINT {
            x: screen.x,
            y: screen.y,
        })
    };
    let root = unsafe { GetAncestor(child, GA_ROOT) };
    if child.0.is_null() || root.0.is_null() || !unsafe { IsWindow(root) }.as_bool() {
        return Err("The pointed window disappeared; move the pointer and retry".into());
    }
    let handle = root.0 as usize;
    let candidate = crate::multi_manager::win::enumerate_top_level_windows()
        .map_err(|e| format!("Could not read the pointed window identity: {e}"))?
        .into_iter()
        .find(|w| w.hwnd == handle)
        .ok_or("The pointed window disappeared before its identity could be read")?;
    let mut origin = POINT::default();
    if !unsafe { ClientToScreen(HWND(root.0), &mut origin) }.as_bool() {
        return Err("Could not determine the pointed window client origin".into());
    }
    Ok(PickedWindow {
        matcher: MkWindowMatcher {
            title: Some(candidate.title),
            title_regex: None,
            process: Some(candidate.executable),
            class: None,
        },
        client_origin: MkPoint {
            x: origin.x,
            y: origin.y,
        },
    })
}
#[cfg(not(windows))]
fn picked_window_at(_: MkPoint) -> Result<PickedWindow, String> {
    Err("Matched-window capture is available only on Windows".into())
}
pub trait WindowPicker {
    fn pick_window(&mut self) -> Result<Option<PickedWindow>, String>;
}

pub fn apply_picked_position(
    target: &mut MkCoordinateTarget,
    screen: MkPoint,
    window: Option<&PickedWindow>,
) -> Result<(), String> {
    match target {
        MkCoordinateTarget::Screen { point } => *point = screen,
        MkCoordinateTarget::ActiveWindow { point } => {
            let w = window.ok_or("No matching active window is available")?;
            *point = translated_client_point(screen, w.client_origin)?;
        }
        MkCoordinateTarget::WindowClient { matcher, point } => {
            let w = window.ok_or("No pointed window is available")?;
            let candidate = crate::mkmacro::windows::WindowCandidate {
                handle: 0,
                title: w.matcher.title.clone().unwrap_or_default(),
                executable: w.matcher.process.clone().unwrap_or_default(),
                process_path: String::new(),
                class_name: w.matcher.class.clone().unwrap_or_default(),
            };
            if !crate::mkmacro::windows::candidate_matches(matcher, &candidate)
                .map_err(|e| e.to_string())?
            {
                return Err("The pointed window does not match this target. Use Choose Window… to deliberately replace the matcher, then pick the position again".into());
            }
            *point = translated_client_point(screen, w.client_origin)?;
        }
        _ => return Err("This target does not store a fixed position".into()),
    }
    Ok(())
}
fn translated_client_point(screen: MkPoint, origin: MkPoint) -> Result<MkPoint, String> {
    Ok(MkPoint {
        x: screen
            .x
            .checked_sub(origin.x)
            .ok_or("Client-relative X coordinate overflow")?,
        y: screen
            .y
            .checked_sub(origin.y)
            .ok_or("Client-relative Y coordinate overflow")?,
    })
}
pub fn apply_picked_window(payload: &mut MkWindowPayload, picked: &PickedWindow) {
    payload.matcher = picked.matcher.clone();
}

fn optional_field(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) {
    let v = value.get_or_insert_with(String::new);
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(v);
    });
    if value.as_ref().is_some_and(|x| x.is_empty()) {
        *value = None;
    }
}
pub(super) fn matcher_ui(ui: &mut egui::Ui, m: &mut MkWindowMatcher) -> bool {
    optional_field(ui, "Executable", &mut m.process);
    optional_field(ui, "Title contains", &mut m.title);
    optional_field(ui, "Title regex", &mut m.title_regex);
    optional_field(ui, "Class", &mut m.class);
    ui.button("Choose Window…").clicked()
}
#[derive(Default)]
pub(super) struct TargetUiOutcome {
    pick_position: bool,
    pick_matcher: bool,
}

const RUNTIME_SCOPE_TOOLTIP: &str = "Runtime outputs are available when the producing and consuming steps execute in the same macro run.";

#[derive(Clone, Debug, PartialEq, Eq)]
struct VariablePickerModel {
    suggestions: Vec<VariableDescriptor>,
}

impl VariablePickerModel {
    fn new(catalog: &VariableCatalog, allowed: impl Fn(VariableValueType) -> bool) -> Self {
        // Filtering follows effective definitions, rather than filtering the
        // history first. Thus a later String correctly shadows an earlier Point.
        Self {
            suggestions: catalog
                .effective_variables()
                .iter()
                .filter(|descriptor| allowed(descriptor.value_type))
                .cloned()
                .collect(),
        }
    }

    fn select(&self, index: usize, buffer: &mut String) -> bool {
        let Some(suggestion) = self.suggestions.get(index) else {
            return false;
        };
        *buffer = suggestion.name.clone();
        true
    }
}

fn variable_type_label(value_type: VariableValueType) -> &'static str {
    match value_type {
        VariableValueType::String => "String",
        VariableValueType::Number => "Number",
        VariableValueType::Boolean => "Boolean",
        VariableValueType::Point => "Point",
        VariableValueType::Unknown => "Unknown",
    }
}

fn variable_detail_text(descriptor: &VariableDescriptor) -> String {
    let mut lines = vec![format!(
        "Produced by {} at step {} (stable ID {}).",
        descriptor.source_action_label, descriptor.source_step_number, descriptor.source_step_id
    )];
    for reason in &descriptor.uncertainty_reasons {
        lines.push(reason.help_text().to_owned());
    }
    if let Some(help) = descriptor.help_text
        && !lines.iter().any(|line| line == help)
    {
        lines.push(help.to_owned());
    }
    lines.push(RUNTIME_SCOPE_TOOLTIP.to_owned());
    lines.join("\n")
}

/// Editable variable consumer with a deliberately optional suggestion popup.
/// The predicate keeps this reusable for consumers accepting multiple types.
fn variable_picker_ui(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash,
    buffer: &mut String,
    catalog: &VariableCatalog,
    allowed: impl Fn(VariableValueType) -> bool,
) {
    let model = VariablePickerModel::new(catalog, allowed);
    let popup_id = ui.make_persistent_id(("variable_picker_popup", id_source));
    let highlighted_id = popup_id.with("highlighted");
    let mut anchor = None;
    ui.horizontal(|ui| {
        ui.label("Variable");
        ui.text_edit_singleline(buffer)
            .on_hover_text(RUNTIME_SCOPE_TOOLTIP);
        let response = ui.button("Suggestions ▾");
        if response.clicked() {
            ui.memory_mut(|memory| memory.toggle_popup(popup_id));
        }
        anchor = Some(response);
    });
    let anchor = anchor.expect("picker button is always rendered");
    let mut close = false;
    let mut chosen = None;
    let popup_open = ui.memory(|memory| memory.is_popup_open(popup_id));
    if popup_open
        && egui::popup::popup_below_widget(ui, popup_id, &anchor, |ui| {
            ui.set_min_width(430.0);
            if model.suggestions.is_empty() {
                ui.weak("No compatible variables are available before this step.");
                return;
            }
            let mut highlighted = ui
                .data(|data| data.get_temp::<usize>(highlighted_id))
                .unwrap_or(0)
                .min(model.suggestions.len() - 1);
            // Do not consume arrows while the editable field owns keyboard focus.
            if !ui.ctx().wants_keyboard_input() {
                if ui.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
                    highlighted = (highlighted + 1).min(model.suggestions.len() - 1);
                }
                if ui.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
                    highlighted = highlighted.saturating_sub(1);
                }
            }
            if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                close = true;
            } else if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                chosen = Some(highlighted);
            }
            for (index, descriptor) in model.suggestions.iter().enumerate() {
                let warning = descriptor
                    .warning_marker()
                    .map_or(String::new(), |marker| format!("{marker} "));
                let label = format!(
                    "{warning}{} · {} · {} · step {}",
                    descriptor.name,
                    variable_type_label(descriptor.value_type),
                    descriptor.source_action_label,
                    descriptor.source_step_number
                );
                let response = ui.selectable_label(index == highlighted, label);
                response
                    .clone()
                    .on_hover_text(variable_detail_text(descriptor));
                if response.hovered() {
                    highlighted = index;
                }
                if response.clicked() {
                    chosen = Some(index);
                }
            }
            ui.data_mut(|data| data.insert_temp(highlighted_id, highlighted));
        })
        .is_none()
    {
        close = true;
    }
    if let Some(index) = chosen {
        model.select(index, buffer);
        close = true;
    }
    if close {
        ui.memory_mut(|memory| memory.close_popup());
    }
}

/// Produces one consistent, stable description wherever an image asset is shown.
pub(crate) fn image_asset_label(asset_id: u64, assets: &[MkImageAsset]) -> String {
    let Some(asset) = assets.iter().find(|asset| asset.id == asset_id) else {
        return format!("Missing asset · ID {asset_id}");
    };
    let filename = std::path::Path::new(&asset.relative_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .trim();
    let friendly = asset.name.trim();
    let mut parts = Vec::new();
    if !friendly.is_empty() {
        parts.push(friendly.to_owned());
    }
    if !filename.is_empty() && filename != friendly {
        parts.push(filename.to_owned());
    }
    if parts.is_empty() {
        parts.push("Image asset".to_owned());
    }
    parts.push(format!("ID {}", asset.id));
    parts.join(" · ")
}

pub(crate) fn select_image_asset(asset_id: &mut u64, selected: u64) {
    *asset_id = selected;
}

pub(crate) fn change_target_kind(
    target: &mut MkCoordinateTarget,
    next: usize,
    assets: &[MkImageAsset],
) {
    *target = match next {
        0 => MkCoordinateTarget::Screen {
            point: MkPoint { x: 0, y: 0 },
        },
        1 => MkCoordinateTarget::ActiveWindow {
            point: MkPoint { x: 0, y: 0 },
        },
        2 => MkCoordinateTarget::WindowClient {
            matcher: MkWindowMatcher::default(),
            point: MkPoint { x: 0, y: 0 },
        },
        3 => MkCoordinateTarget::Variable {
            name: String::new(),
        },
        4 => MkCoordinateTarget::Image {
            asset_id: assets.first().map_or(0, |a| a.id),
            offset: MkPoint { x: 0, y: 0 },
        },
        _ => MkCoordinateTarget::Pixel {
            search_id: 0,
            offset: MkPoint { x: 0, y: 0 },
        },
    };
}

/// Immutable document state shared by every coordinate-target editor.
///
/// Keeping this separate from the mutable target makes it impossible for the
/// widget to silently repair (or replace) a dangling asset reference.
#[derive(Clone, Copy)]
pub(super) struct TargetEditorContext<'a> {
    pub macro_id: u64,
    pub assets: &'a [MkImageAsset],
    pub store: &'a MkMacroStore,
}

impl<'a> TargetEditorContext<'a> {
    pub(super) fn resolve_asset(&self, asset_id: u64) -> Option<&'a MkImageAsset> {
        (asset_id != 0)
            .then(|| self.assets.iter().find(|asset| asset.id == asset_id))
            .flatten()
    }
}

pub(super) fn target_ui(
    ui: &mut egui::Ui,
    target: &mut MkCoordinateTarget,
    context: &TargetEditorContext<'_>,
) -> TargetUiOutcome {
    target_ui_with_variables(ui, target, context, None, 0)
}

fn target_ui_with_variables(
    ui: &mut egui::Ui,
    target: &mut MkCoordinateTarget,
    context: &TargetEditorContext<'_>,
    variable_catalog: Option<&VariableCatalog>,
    picker_id: u64,
) -> TargetUiOutcome {
    let assets = context.assets;
    let kind = match target {
        MkCoordinateTarget::Screen { .. } => 0,
        MkCoordinateTarget::ActiveWindow { .. } => 1,
        MkCoordinateTarget::WindowClient { .. } => 2,
        MkCoordinateTarget::Variable { .. } => 3,
        MkCoordinateTarget::Image { .. } => 4,
        MkCoordinateTarget::Pixel { .. } => 5,
    };
    let mut next = kind;
    egui::ComboBox::from_label("Target")
        .selected_text(
            [
                "Screen",
                "Active Window",
                "Matched Window",
                "Variable",
                "Image Result",
                "Pixel Result",
            ][kind],
        )
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut next, 0, "Screen");
            ui.selectable_value(&mut next, 1, "Active Window");
            ui.selectable_value(&mut next, 2, "Matched Window");
            ui.selectable_value(&mut next, 3, "Variable");
            ui.add_enabled_ui(!assets.is_empty(), |ui| {
                ui.selectable_value(&mut next, 4, "Image Result");
            });
            ui.selectable_value(&mut next, 5, "Pixel Result");
        });
    if next != kind {
        change_target_kind(target, next, assets);
    }
    match target {
        MkCoordinateTarget::Screen { point } | MkCoordinateTarget::ActiveWindow { point } => {
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut point.x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut point.y));
            });
            #[cfg(windows)]
            let picked = ui.button("Pick Position").clicked();
            #[cfg(not(windows))]
            let picked = {
                ui.add_enabled(false, egui::Button::new("Pick Position (Windows only)"));
                false
            };
            return TargetUiOutcome {
                pick_position: picked,
                pick_matcher: false,
            };
        }
        MkCoordinateTarget::WindowClient { matcher, point } => {
            ui.heading("Window");
            let choose = matcher_ui(ui, matcher);
            ui.heading("Position");
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut point.x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut point.y));
            });
            #[cfg(windows)]
            let position = ui.button("Pick Position").clicked();
            #[cfg(not(windows))]
            let position = {
                ui.add_enabled(false, egui::Button::new("Pick Position (Windows only)"));
                false
            };
            return TargetUiOutcome {
                pick_position: position,
                pick_matcher: choose,
            };
        }
        MkCoordinateTarget::Variable { name } => {
            if let Some(catalog) = variable_catalog {
                variable_picker_ui(ui, picker_id, name, catalog, |value_type| {
                    value_type == VariableValueType::Point
                });
                if let Some(warning) =
                    catalog.warning_for_expected_type(name, VariableValueType::Point)
                {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("⚠ {}", warning.message_for_consumer("Mouse Move")),
                    );
                }
            } else {
                ui.horizontal(|ui| {
                    ui.label("Variable");
                    ui.text_edit_singleline(name);
                });
            }
        }
        MkCoordinateTarget::Image { asset_id, offset } => {
            if assets.is_empty() {
                ui.add_enabled(
                    false,
                    egui::Label::new("No image results are available in this context."),
                );
            } else {
                egui::ComboBox::from_label("Result from")
                    .selected_text(image_asset_label(*asset_id, assets))
                    .show_ui(ui, |ui| {
                        for asset in assets {
                            let label = image_asset_label(asset.id, assets);
                            if ui.selectable_label(*asset_id == asset.id, label).clicked() {
                                select_image_asset(asset_id, asset.id);
                            }
                        }
                    });
            }
            ui.label("Reference/result image:");
            if *asset_id == 0 {
                ui.weak("No image result selected");
            } else if context.resolve_asset(*asset_id).is_some() {
                super::image_preview::show_thumbnail(
                    ui,
                    context.store,
                    context.macro_id,
                    *asset_id,
                    super::image_preview::TARGET_THUMBNAIL_BOUND,
                );
                ui.weak("Visual reference for the selected image-search result; Mouse Move does not run a new search.");
            } else {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("Missing asset · ID {}", *asset_id),
                );
            }
            ui.heading("Offset");
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut offset.x));
            });
            ui.horizontal(|ui| {
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut offset.y));
            });
        }
        MkCoordinateTarget::Pixel { search_id, offset } => {
            ui.horizontal(|ui| {
                ui.label("Search ID");
                ui.add(egui::DragValue::new(search_id));
            });
            ui.heading("Offset");
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut offset.x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut offset.y));
            });
        }
    }
    TargetUiOutcome::default()
}

/// A shared typed editor. Switching type constructs a fresh value rather than
/// interpreting text belonging to the previous type.
pub(super) fn value_ui(ui: &mut egui::Ui, value: &mut MkValue) {
    let kind = match value {
        MkValue::String(_) => 0,
        MkValue::Number(_) => 1,
        MkValue::Boolean(_) => 2,
        MkValue::Point(_) => 3,
        MkValue::Null => 4,
    };
    let mut next = kind;
    egui::ComboBox::from_label("Type")
        .selected_text(["String", "Number", "Boolean", "Point", "Null"][kind])
        .show_ui(ui, |ui| {
            for (index, label) in ["String", "Number", "Boolean", "Point", "Null"]
                .iter()
                .enumerate()
            {
                ui.selectable_value(&mut next, index, *label);
            }
        });
    if next != kind {
        *value = match next {
            0 => MkValue::String(String::new()),
            1 => MkValue::Number(0.0),
            2 => MkValue::Boolean(false),
            3 => MkValue::Point(MkPoint { x: 0, y: 0 }),
            _ => MkValue::Null,
        };
    }
    match value {
        MkValue::String(text) => {
            ui.horizontal(|ui| {
                ui.label("Value");
                ui.text_edit_singleline(text);
            });
        }
        MkValue::Number(number) => {
            ui.horizontal(|ui| {
                ui.label("Value");
                ui.add(egui::DragValue::new(number));
            });
        }
        MkValue::Boolean(boolean) => {
            ui.checkbox(boolean, "Value");
        }
        MkValue::Point(point) => {
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut point.x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut point.y));
            });
        }
        MkValue::Null => {
            ui.small("Null has no value.");
        }
    }
}

fn action_ui(
    ui: &mut egui::Ui,
    step: &mut MkStep,
    capture: &mut bool,
    image_context: super::image_asset_picker::ImageAssetUiContext<'_>,
    draft_generation: u64,
    variable_catalog: &VariableCatalog,
    point_pick_active: bool,
) -> (
    Option<PositionCaptureSlot>,
    Option<super::window_picker::MatcherPath>,
    Option<super::launcher_action_picker::PickerPurpose>,
    Option<ImageAuthoringRequest>,
    Option<super::condition_editor::ConditionImageRequest>,
    Option<PreviewRequest>,
    bool,
) {
    let target_context = TargetEditorContext {
        macro_id: image_context.macro_id,
        assets: image_context.assets,
        store: image_context.store,
    };
    let mut pick = None;
    let mut window_pick = None;
    let mut launcher_pick = None;
    let image_request = None;
    let mut condition_image_request = None;
    let mut preview_request = None;
    let mut point_pick = false;
    let draft_id = step.id;
    match &mut step.action {
        MkAction::KeyPress(k) | MkAction::KeyDown(k) | MkAction::KeyUp(k) => {
            ui.label(format!("Captured: {}", key_name(k)));
            if ui.button("Capture next key or chord").clicked() {
                *capture = true;
            }
        }
        MkAction::Hotkey(keys) => {
            ui.label(format!(
                "Captured: {}",
                keys.iter().map(key_name).collect::<Vec<_>>().join(" + ")
            ));
            if ui.button("Capture next key or chord").clicked() {
                *capture = true;
            }
        }
        MkAction::Text(p) => {
            ui.add(
                egui::TextEdit::multiline(&mut p.text)
                    .desired_rows(10)
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                ui.radio_value(&mut p.mode, MkTextMode::Type, "Type");
                ui.radio_value(&mut p.mode, MkTextMode::Paste, "Paste");
            });
            if p.mode == MkTextMode::Paste {
                ui.small("Temporarily replaces clipboard text during playback and attempts to restore it. Non-text clipboard contents cannot be preserved.");
            }
        }
        MkAction::Notify(p) => {
            ui.heading("Notify");
            ui.label("Title");
            ui.text_edit_singleline(&mut p.title);
            if p.title.trim().is_empty() {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "Notification title cannot be empty",
                );
            }
            ui.label("Description");
            ui.add(egui::TextEdit::multiline(&mut p.description).desired_rows(4));
            egui::ComboBox::from_id_source(("notify_kind", draft_id, draft_generation))
                .selected_text(p.kind.label())
                .show_ui(ui, |ui| {
                    for (kind, label) in [
                        (MkNotificationKind::Information, "Information"),
                        (MkNotificationKind::Success, "Success"),
                        (MkNotificationKind::Warning, "Warning"),
                        (MkNotificationKind::Error, "Error"),
                    ] {
                        ui.selectable_value(&mut p.kind, kind, label);
                    }
                });
            egui::ComboBox::from_id_source(("notify_duration", draft_id, draft_generation))
                .selected_text(match p.duration {
                    MkNotificationDuration::Short => "Short",
                    MkNotificationDuration::Long => "Long",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut p.duration, MkNotificationDuration::Short, "Short");
                    ui.selectable_value(&mut p.duration, MkNotificationDuration::Long, "Long");
                });
            ui.checkbox(&mut p.show_symbol, "Show symbol");
            if ui.button("Preview Notification").clicked() {
                preview_request = Some(PreviewRequest::Notification(ResolvedNotification {
                    title: p.title.clone(),
                    description: p.description.clone(),
                    kind: p.kind,
                    duration: p.duration,
                    show_symbol: p.show_symbol,
                }));
            }
        }
        MkAction::PlaySound(p) => {
            ui.heading("Play Sound");
            egui::ComboBox::from_id_source(("play_sound", draft_id, draft_generation))
                .selected_text(&p.sound)
                .show_ui(ui, |ui| {
                    for name in crate::sound::SOUND_NAMES
                        .iter()
                        .copied()
                        .filter(|name| *name != "None")
                    {
                        ui.selectable_value(&mut p.sound, name.to_owned(), name);
                    }
                });
            if !play_sound_is_supported(&p.sound) {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Unsupported sound: {}", p.sound),
                );
            }
            if ui.button("Preview Sound").clicked() && play_sound_is_supported(&p.sound) {
                preview_request = Some(PreviewRequest::Sound(p.sound.clone()));
            }
        }
        MkAction::MouseMove(p) => {
            let response = target_ui_with_variables(
                ui,
                &mut p.target,
                &target_context,
                Some(variable_catalog),
                draft_id,
            );
            if response.pick_position {
                pick = Some(PositionCaptureSlot::MoveTarget);
            }
            if response.pick_matcher {
                window_pick = Some(super::window_picker::MatcherPath::Coordinate(
                    super::window_picker::CoordinateMatcherPath::MoveTarget,
                ));
            }
            let mut smooth = p.duration_ms != 0;
            ui.horizontal(|ui| {
                ui.label("Movement");
                ui.radio_value(&mut smooth, false, "Instant");
                ui.radio_value(&mut smooth, true, "Smooth");
            });
            if smooth {
                if p.duration_ms == 0 {
                    p.duration_ms = 250;
                }
                ui.add(
                    egui::DragValue::new(&mut p.duration_ms)
                        .clamp_range(1..=86_400_000)
                        .suffix(" ms"),
                );
            } else {
                p.duration_ms = 0;
            }
        }
        MkAction::MouseDrag(p) => {
            ui.label("Start");
            let response = target_ui(ui, &mut p.from, &target_context);
            if response.pick_position {
                pick = Some(PositionCaptureSlot::DragFrom);
            }
            if response.pick_matcher {
                window_pick = Some(super::window_picker::MatcherPath::Coordinate(
                    super::window_picker::CoordinateMatcherPath::DragFrom,
                ));
            }
            ui.label("Destination");
            let response = target_ui(ui, &mut p.to, &target_context);
            if response.pick_position {
                pick = Some(PositionCaptureSlot::DragTo);
            }
            if response.pick_matcher {
                window_pick = Some(super::window_picker::MatcherPath::Coordinate(
                    super::window_picker::CoordinateMatcherPath::DragTo,
                ));
            }
            egui::ComboBox::from_label("Button")
                .selected_text(format!("{:?}", p.button))
                .show_ui(ui, |ui| {
                    for b in [
                        MkMouseButton::Left,
                        MkMouseButton::Right,
                        MkMouseButton::Middle,
                        MkMouseButton::X1,
                        MkMouseButton::X2,
                    ] {
                        ui.selectable_value(&mut p.button, b.clone(), format!("{b:?}"));
                    }
                });
            ui.horizontal(|ui| {
                ui.label("Duration (ms)");
                ui.add(egui::DragValue::new(&mut p.duration_ms));
            });
        }
        MkAction::MouseClick(p) => {
            let response = target_ui(ui, &mut p.target, &target_context);
            if response.pick_position {
                pick = Some(PositionCaptureSlot::ClickTarget);
            }
            if response.pick_matcher {
                window_pick = Some(super::window_picker::MatcherPath::Coordinate(
                    super::window_picker::CoordinateMatcherPath::ClickTarget,
                ));
            }
            egui::ComboBox::from_label("Button")
                .selected_text(format!("{:?}", p.button))
                .show_ui(ui, |ui| {
                    for b in [
                        MkMouseButton::Left,
                        MkMouseButton::Right,
                        MkMouseButton::Middle,
                        MkMouseButton::X1,
                        MkMouseButton::X2,
                    ] {
                        ui.selectable_value(&mut p.button, b.clone(), format!("{b:?}"));
                    }
                });
            ui.horizontal(|ui| {
                ui.label("Click count");
                ui.add(egui::DragValue::new(&mut p.clicks).clamp_range(1..=1_000_000));
            });
        }
        MkAction::MouseDown(button) | MkAction::MouseUp(button) => {
            egui::ComboBox::from_label("Button")
                .selected_text(format!("{button:?}"))
                .show_ui(ui, |ui| {
                    for b in [
                        MkMouseButton::Left,
                        MkMouseButton::Right,
                        MkMouseButton::Middle,
                        MkMouseButton::X1,
                        MkMouseButton::X2,
                    ] {
                        ui.selectable_value(button, b.clone(), format!("{b:?}"));
                    }
                });
        }
        MkAction::MouseScroll { axis, i32_delta } => {
            const WHEEL: i32 = 120;
            let mut direction = match (*axis, i32_delta.is_negative()) {
                (MkMouseScrollAxis::Vertical, false) => 0,
                (MkMouseScrollAxis::Vertical, true) => 1,
                (MkMouseScrollAxis::Horizontal, true) => 2,
                (MkMouseScrollAxis::Horizontal, false) => 3,
            };
            let before_direction = direction;
            ui.label("Direction");
            ui.horizontal_wrapped(|ui| {
                ui.radio_value(&mut direction, 0, "Vertical Up");
                ui.radio_value(&mut direction, 1, "Vertical Down");
                ui.radio_value(&mut direction, 2, "Horizontal Left");
                ui.radio_value(&mut direction, 3, "Horizontal Right");
            });
            let (selected_axis, positive) = match direction {
                0 => (MkMouseScrollAxis::Vertical, true),
                1 => (MkMouseScrollAxis::Vertical, false),
                // WM_MOUSEHWHEEL and MOUSEEVENTF_HWHEEL define positive as right.
                2 => (MkMouseScrollAxis::Horizontal, false),
                _ => (MkMouseScrollAxis::Horizontal, true),
            };
            if direction != before_direction {
                apply_scroll_direction(axis, i32_delta, selected_axis, positive);
            }
            if *i32_delta % WHEEL != 0 {
                ui.label(format!("Legacy raw wheel delta: {i32_delta}"));
                ui.small("The raw delta is preserved. Select a direction above to normalize it to one whole notch.");
            } else {
                let mut notches = (*i32_delta / WHEEL).unsigned_abs().max(1);
                let before = notches;
                ui.add(
                    egui::DragValue::new(&mut notches)
                        .clamp_range(1..=(i32::MAX as u32 / WHEEL as u32))
                        .prefix("Notches "),
                );
                if before != notches || *i32_delta == 0 {
                    let magnitude = (notches as i32)
                        .checked_mul(WHEEL)
                        .unwrap_or(i32::MAX / WHEEL * WHEEL);
                    *i32_delta = if positive { magnitude } else { -magnitude };
                }
            }
        }
        MkAction::Delay { milliseconds } => {
            ui.horizontal(|ui| {
                ui.label("Action duration (ms)");
                ui.add(egui::DragValue::new(milliseconds).clamp_range(0..=86_400_000));
            });
        }
        MkAction::Process(p) => {
            ui.horizontal(|ui| {
                ui.label("Program");
                ui.text_edit_singleline(&mut p.program);
                if ui.button("Browse…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Executable", &["exe", "bat", "cmd", "com"])
                        .pick_file()
                {
                    super::launcher_action_picker::apply_chosen_program_path(p, path);
                }
                if ui.button("Choose From Launcher…").clicked() {
                    launcher_pick = Some(super::launcher_action_picker::PickerPurpose::Process);
                }
            });
            let mut args = p.arguments.join(" ");
            ui.horizontal(|ui| {
                ui.label("Arguments");
                ui.text_edit_singleline(&mut args);
            });
            p.arguments = shlex::split(&args).unwrap_or_else(|| vec![args]);
            let wd = p.working_directory.get_or_insert_with(String::new);
            ui.horizontal(|ui| {
                ui.label("Working directory");
                ui.text_edit_singleline(wd);
                if ui.button("Browse…").clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                {
                    *wd = path.to_string_lossy().into_owned();
                }
            });
            if wd.trim().is_empty() {
                p.working_directory = None;
            }
            ui.checkbox(&mut p.wait, "Wait for completion");
        }
        MkAction::WindowActivate(p) | MkAction::WindowWait(p) => {
            if matcher_ui(ui, &mut p.matcher) {
                window_pick = Some(super::window_picker::MatcherPath::Action);
            }
            if let Some(w) = &mut p.wait {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Timeout (ms)");
                        ui.add(egui::DragValue::new(&mut w.timeout_ms).clamp_range(0..=86_400_000));
                    });
                    ui.small("0 = wait forever");
                    ui.horizontal(|ui| {
                        ui.label("Poll (ms)");
                        ui.add(
                            egui::DragValue::new(&mut w.poll_interval_ms)
                                .clamp_range(1..=86_400_000),
                        );
                    });
                });
            }
        }
        MkAction::WindowClose(m) => {
            if matcher_ui(ui, m) {
                window_pick = Some(super::window_picker::MatcherPath::Action);
            }
        }
        MkAction::WindowMoveResize(p) => {
            if matcher_ui(ui, &mut p.matcher) {
                window_pick = Some(super::window_picker::MatcherPath::Action);
            }
            let mut moving = p.x.is_some() && p.y.is_some();
            if ui.checkbox(&mut moving, "Move").changed() {
                if moving {
                    p.x = Some(0);
                    p.y = Some(0);
                } else {
                    p.x = None;
                    p.y = None;
                }
            }
            if moving {
                ui.horizontal(|ui| {
                    ui.label("X");
                    ui.add(egui::DragValue::new(p.x.as_mut().unwrap()));
                    ui.label("Y");
                    ui.add(egui::DragValue::new(p.y.as_mut().unwrap()));
                });
            }
            let mut resizing = p.width.is_some() && p.height.is_some();
            if ui.checkbox(&mut resizing, "Resize").changed() {
                if resizing {
                    p.width = Some(1200);
                    p.height = Some(800);
                } else {
                    p.width = None;
                    p.height = None;
                }
            }
            if resizing {
                ui.horizontal(|ui| {
                    ui.label("Width");
                    ui.add(
                        egui::DragValue::new(p.width.as_mut().unwrap()).clamp_range(1..=u32::MAX),
                    );
                    ui.label("Height");
                    ui.add(
                        egui::DragValue::new(p.height.as_mut().unwrap()).clamp_range(1..=u32::MAX),
                    );
                });
            }
            if !moving && !resizing {
                ui.colored_label(ui.visuals().error_fg_color, "Enable Move or Resize");
            }
        }
        MkAction::WindowState { matcher, state } => {
            if matcher_ui(ui, matcher) {
                window_pick = Some(super::window_picker::MatcherPath::Action);
            }
            egui::ComboBox::from_label("Window state")
                .selected_text(format!("{state:?}"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(state, MkWindowState::Minimize, "Minimize");
                    ui.selectable_value(state, MkWindowState::Maximize, "Maximize");
                    ui.selectable_value(state, MkWindowState::Restore, "Restore");
                });
        }
        MkAction::SetVariable { name, value } => {
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(name);
            });
            value_ui(ui, value);
            if matches!(value, MkValue::Point(_)) {
                point_pick = ui
                    .add_enabled(!point_pick_active, egui::Button::new("Pick Position"))
                    .clicked();
            }
        }
        MkAction::UnsetVariable { name } => {
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(name);
            });
        }
        MkAction::PromptInput(p) => {
            ui.label("Dialog title");
            ui.text_edit_singleline(&mut p.title)
                .on_hover_text("Title of the independent input dialog");
            ui.label("Prompt text");
            ui.text_edit_multiline(&mut p.prompt);
            ui.label("Default value");
            ui.text_edit_singleline(&mut p.default_value);
            ui.label("Destination variable");
            ui.text_edit_singleline(&mut p.variable);
            ui.checkbox(&mut p.copy_to_clipboard, "Copy result to clipboard");
            if let Err(reason) = crate::mkmacro::variables::validate_variable_name(&p.variable) {
                ui.colored_label(ui.visuals().error_fg_color, reason);
            }
            ui.small("During playback, Cancel or closing the prompt stops the macro.");
        }
        MkAction::RepeatStart { count } => {
            ui.horizontal(|ui| {
                ui.label("Repeat count");
                ui.add(egui::DragValue::new(count).clamp_range(1..=1_000_000));
            });
        }
        MkAction::If(condition) | MkAction::WhileStart { condition } => {
            if let Some(request) =
                super::condition_editor::condition_ui_with_context(ui, condition, image_context)
            {
                match request {
                    super::condition_editor::ConditionEditorRequest::WindowMatcher { path } => {
                        window_pick =
                            Some(super::window_picker::MatcherPath::Condition(path.indexes()));
                    }
                    super::condition_editor::ConditionEditorRequest::Image(request) => {
                        condition_image_request = Some(request);
                    }
                }
            }
        }
        MkAction::WaitUntil { condition, wait } => {
            if let Some(request) =
                super::condition_editor::condition_ui_with_context(ui, condition, image_context)
            {
                match request {
                    super::condition_editor::ConditionEditorRequest::WindowMatcher { path } => {
                        window_pick =
                            Some(super::window_picker::MatcherPath::Condition(path.indexes()));
                    }
                    super::condition_editor::ConditionEditorRequest::Image(request) => {
                        condition_image_request = Some(request);
                    }
                }
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Timeout (ms)");
                    ui.add(egui::DragValue::new(&mut wait.timeout_ms).clamp_range(0..=86_400_000));
                });
                ui.small("0 = wait forever");
                ui.horizontal(|ui| {
                    ui.label("Poll (ms)");
                    ui.add(
                        egui::DragValue::new(&mut wait.poll_interval_ms)
                            .clamp_range(1..=86_400_000),
                    );
                });
            });
        }
        MkAction::LauncherCommand { command, args } => {
            ui.horizontal(|ui| {
                ui.label("Canonical action");
                ui.text_edit_singleline(command);
            });
            let a = args.get_or_insert_with(String::new);
            ui.horizontal(|ui| {
                ui.label("Arguments");
                ui.text_edit_singleline(a);
            });
            if a.is_empty() {
                *args = None;
            }
            ui.small(
                "Search uses canonical launcher action values; display labels are not persisted.",
            );
            if ui.button("Choose Launcher Action…").clicked() {
                launcher_pick = Some(super::launcher_action_picker::PickerPurpose::LauncherCommand);
            }
        }
        MkAction::ImageFind(_) | MkAction::ImageClick(_) => {}
        MkAction::WaitForVisualChange(p) => {
            ui.heading("Wait for Visual Change");
            ui.horizontal(|ui| {
                ui.label("Change threshold (%)");
                ui.add(
                    egui::DragValue::new(&mut p.change_threshold_percent).clamp_range(0.01..=100.0),
                );
            });
            ui.small("Changed-pixel percentage: a pixel counts when any RGBA channel exceeds the tolerance.");
            ui.horizontal(|ui| {
                ui.label("Per-channel tolerance");
                let tolerance = p.per_pixel_tolerance.get_or_insert(8);
                ui.add(egui::DragValue::new(tolerance));
                if ui.button("Disable tolerance").clicked() {
                    p.per_pixel_tolerance = None;
                }
            });
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Timeout (ms)");
                    ui.add(egui::DragValue::new(&mut p.timeout_ms).clamp_range(0..=86_400_000));
                });
                ui.small("0 = wait forever");
                ui.horizontal(|ui| {
                    ui.label("Poll (ms)");
                    ui.add(
                        egui::DragValue::new(&mut p.poll_interval_ms).clamp_range(1..=86_400_000),
                    );
                });
            });
            ui.horizontal(|ui| {
                ui.label("Consecutive changed frames");
                ui.add(
                    egui::DragValue::new(p.consecutive_changed_frames.get_or_insert(2))
                        .clamp_range(1..=1000),
                );
            });
        }
        MkAction::CaptureScreenshot(p) => {
            ui.heading("Capture Screenshot");
            egui::ComboBox::from_label("Destination")
                .selected_text(format!("{:?}", p.destination))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut p.destination, MkScreenshotDestination::File, "File");
                    ui.selectable_value(
                        &mut p.destination,
                        MkScreenshotDestination::Clipboard,
                        "Clipboard",
                    );
                    ui.selectable_value(
                        &mut p.destination,
                        MkScreenshotDestination::Both,
                        "File + Clipboard",
                    );
                });
            if p.destination.produces_file() {
                ui.horizontal(|ui| {
                    ui.label("Path template");
                    ui.text_edit_singleline(p.path.get_or_insert_default());
                });
                egui::ComboBox::from_label("Format")
                    .selected_text(format!("{:?}", p.format))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut p.format, MkScreenshotFormat::Png, "PNG");
                        ui.selectable_value(&mut p.format, MkScreenshotFormat::Jpeg, "JPEG");
                        ui.selectable_value(&mut p.format, MkScreenshotFormat::Bmp, "BMP");
                    });
                egui::ComboBox::from_label("If file exists")
                    .selected_text(format!("{:?}", p.collision))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut p.collision, MkFileCollisionPolicy::Error, "Fail");
                        ui.selectable_value(
                            &mut p.collision,
                            MkFileCollisionPolicy::Overwrite,
                            "Overwrite",
                        );
                        ui.selectable_value(
                            &mut p.collision,
                            MkFileCollisionPolicy::Unique,
                            "Create unique name",
                        );
                    });
                let mut enabled = p.path_output.is_some();
                ui.checkbox(&mut enabled, "Store written path");
                if enabled {
                    ui.text_edit_singleline(
                        p.path_output
                            .get_or_insert_with(|| "screenshot_path".into()),
                    );
                } else {
                    p.path_output = None;
                }
            } else {
                p.path = None;
                p.path_output = None;
                ui.add_enabled(
                    false,
                    egui::TextEdit::singleline(&mut String::new())
                        .hint_text("File path (not used)"),
                );
            }
        }
        MkAction::FindPixel(p) => {
            ui.heading("Find Pixel Color");
            ui.horizontal(|ui| {
                ui.label("Color");
                ui.text_edit_singleline(&mut p.color);
                if let Ok(rgb) = crate::mkmacro::screen::parse_rgb(&p.color) {
                    let mut picked = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                    if ui.color_edit_button_srgba(&mut picked).changed() {
                        p.color = crate::mkmacro::screen::format_rgb([
                            picked.r(),
                            picked.g(),
                            picked.b(),
                        ]);
                    }
                }
                ui.label("Tolerance");
                ui.add(egui::DragValue::new(&mut p.tolerance));
            });
            ui.small("Tolerance is the maximum absolute difference in each RGB channel.");
            ui.horizontal(|ui| {
                ui.label("Search ID");
                ui.add(egui::DragValue::new(&mut p.search_id));
            });
            ui.horizontal(|ui| {
                ui.label("Timeout (ms)");
                ui.add(egui::DragValue::new(&mut p.wait.timeout_ms));
                ui.label("Poll (ms)");
                ui.add(
                    egui::DragValue::new(&mut p.wait.poll_interval_ms).clamp_range(1..=86_400_000),
                );
            });
            egui::ComboBox::from_label("If missing")
                .selected_text(match p.not_found_policy {
                    MkImageNotFoundPolicy::Continue => "Continue",
                    MkImageNotFoundPolicy::Fail => "Fail action",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut p.not_found_policy,
                        MkImageNotFoundPolicy::Continue,
                        "Continue",
                    );
                    ui.selectable_value(
                        &mut p.not_found_policy,
                        MkImageNotFoundPolicy::Fail,
                        "Fail action",
                    );
                });
            let mut region_kind = match p.region {
                SearchRegion::Desktop => 0,
                SearchRegion::Monitor { .. } => 1,
                SearchRegion::Rectangle { .. } => 2,
                SearchRegion::Window { .. } => 3,
                SearchRegion::ClientArea { .. } => 4,
            };
            egui::ComboBox::from_label("Region")
                .selected_text(
                    ["Desktop", "Monitor", "Rectangle", "Window", "Client Area"][region_kind],
                )
                .show_ui(ui, |ui| {
                    for (i, label) in ["Desktop", "Monitor", "Rectangle", "Window", "Client Area"]
                        .into_iter()
                        .enumerate()
                    {
                        ui.selectable_value(&mut region_kind, i, label);
                    }
                });
            let current_kind = match p.region {
                SearchRegion::Desktop => 0,
                SearchRegion::Monitor { .. } => 1,
                SearchRegion::Rectangle { .. } => 2,
                SearchRegion::Window { .. } => 3,
                SearchRegion::ClientArea { .. } => 4,
            };
            if region_kind != current_kind {
                p.region = match region_kind {
                    0 => SearchRegion::Desktop,
                    1 => SearchRegion::Monitor { index: 0 },
                    2 => SearchRegion::Rectangle {
                        rect: ScreenRect::new(0, 0, 800, 500),
                    },
                    3 => SearchRegion::Window {
                        matcher: MkWindowMatcher::default(),
                    },
                    _ => SearchRegion::ClientArea {
                        matcher: MkWindowMatcher::default(),
                    },
                };
            }
            match &mut p.region {
                SearchRegion::Monitor { index } => {
                    ui.horizontal(|ui| {
                        ui.label("Monitor index");
                        ui.add(egui::DragValue::new(index));
                    });
                }
                SearchRegion::Rectangle { rect } => {
                    ui.horizontal(|ui| {
                        ui.label("X");
                        ui.add(egui::DragValue::new(&mut rect.x));
                        ui.label("Y");
                        ui.add(egui::DragValue::new(&mut rect.y));
                        ui.label("Width");
                        ui.add(egui::DragValue::new(&mut rect.width));
                        ui.label("Height");
                        ui.add(egui::DragValue::new(&mut rect.height));
                    });
                }
                SearchRegion::Window { matcher } | SearchRegion::ClientArea { matcher } => {
                    matcher_ui(ui, matcher);
                }
                SearchRegion::Desktop => {}
            }
            for (label, output) in [
                ("Found output", &mut p.outputs.found),
                ("Point output", &mut p.outputs.point),
                ("X output", &mut p.outputs.x),
                ("Y output", &mut p.outputs.y),
            ] {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.text_edit_singleline(output.get_or_insert_with(String::new));
                });
                if output.as_ref().is_some_and(|x| x.is_empty()) {
                    *output = None;
                }
            }
        }
        MkAction::PixelCheck {
            target,
            color,
            tolerance,
        } => {
            ui.heading("Coordinate");
            let response = target_ui(ui, target, &target_context);
            if response.pick_position {
                pick = Some(PositionCaptureSlot::PixelPosition);
            }
            if response.pick_matcher {
                window_pick = Some(super::window_picker::MatcherPath::Coordinate(
                    super::window_picker::CoordinateMatcherPath::PixelPosition,
                ));
            }
            ui.separator();
            ui.heading("Color");
            ui.horizontal(|ui| {
                ui.label("Color");
                let response = ui.text_edit_singleline(color);
                if response.lost_focus() {
                    if let Ok(rgb) = crate::mkmacro::screen::parse_rgb(color) {
                        *color = crate::mkmacro::screen::format_rgb(rgb);
                    }
                }
                match crate::mkmacro::screen::parse_rgb(color) {
                    Ok(rgb) => {
                        let swatch = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(28.0, 20.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 3.0, swatch);
                    }
                    Err(error) => {
                        ui.colored_label(egui::Color32::RED, error.to_string());
                    }
                }
                if ui.button("Pick Color").clicked() {
                    pick = Some(PositionCaptureSlot::PixelColor);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Tolerance");
                ui.add(egui::DragValue::new(tolerance).clamp_range(0..=255));
            });
        }
        _ => {
            ui.label(
                "This legacy action is unavailable for editing; its saved payload is preserved.",
            );
        }
    }
    (
        pick,
        window_pick,
        launcher_pick,
        image_request,
        condition_image_request,
        preview_request,
        point_pick,
    )
}

fn play_sound_is_supported(sound: &str) -> bool {
    sound != "None" && crate::sound::SOUND_NAMES.contains(&sound)
}

fn dispatch_preview(state: &mut ActionEditorState, request: PreviewRequest) {
    match request {
        PreviewRequest::Notification(notification) => {
            state.capture_message = (state.notification_preview)(&notification).err();
        }
        PreviewRequest::Sound(sound) => (state.sound_preview)(&sound),
    }
}

fn apply_scroll_direction(
    axis: &mut MkMouseScrollAxis,
    delta: &mut i32,
    selected_axis: MkMouseScrollAxis,
    positive: bool,
) {
    const WHEEL: i32 = 120;
    *axis = selected_axis;
    let magnitude = if *delta % WHEEL == 0 {
        delta.unsigned_abs().max(WHEEL as u32) as i32
    } else {
        WHEEL
    };
    *delta = if positive { magnitude } else { -magnitude };
}

#[cfg(test)]
mod scroll_editor_tests {
    use super::*;

    #[test]
    fn all_directions_normalize_legacy_raw_deltas() {
        for (selected_axis, positive, expected) in [
            (MkMouseScrollAxis::Vertical, true, 120),
            (MkMouseScrollAxis::Vertical, false, -120),
            (MkMouseScrollAxis::Horizontal, false, -120),
            (MkMouseScrollAxis::Horizontal, true, 120),
        ] {
            let mut axis = MkMouseScrollAxis::Vertical;
            let mut delta = 37;
            apply_scroll_direction(&mut axis, &mut delta, selected_axis, positive);
            assert_eq!((axis, delta), (selected_axis, expected));
        }
    }

    #[test]
    fn whole_notch_magnitude_is_retained_when_direction_changes() {
        let mut axis = MkMouseScrollAxis::Vertical;
        let mut delta = -360;
        apply_scroll_direction(&mut axis, &mut delta, MkMouseScrollAxis::Horizontal, true);
        assert_eq!((axis, delta), (MkMouseScrollAxis::Horizontal, 360));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImageAuthoringRequest {
    Import,
    CaptureRectangle,
    PickRectangle,
}

impl ImageAuthoringRequest {
    fn rectangle_purpose(self) -> Option<super::visual_overlay::RectanglePurpose> {
        use super::visual_overlay::RectanglePurpose;
        match self {
            Self::Import => None,
            Self::CaptureRectangle => Some(RectanglePurpose::ReferenceImageCapture),
            Self::PickRectangle => Some(RectanglePurpose::SearchRegion),
        }
    }
}

fn image_payload(step: &MkStep) -> Option<&MkImagePayload> {
    match &step.action {
        MkAction::ImageFind(p) | MkAction::ImageClick(p) => Some(p),
        _ => None,
    }
}

/// Inserts an independent smooth move after the stable image-search step.
pub(crate) fn insert_smooth_move_after(
    dialog: &mut MkMacroDialog,
    anchor_id: u64,
    payload: &MkImagePayload,
) -> bool {
    let Some(macro_) = dialog.selected_macro_mut() else {
        return false;
    };
    let Some(index) = macro_.steps.iter().position(|step| step.id == anchor_id) else {
        return false;
    };
    macro_.steps.insert(
        index + 1,
        MkStep {
            id: 0,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action: MkAction::MouseMove(MkMouseMovePayload {
                target: MkCoordinateTarget::Image {
                    asset_id: payload.asset_id,
                    offset: MkPoint { x: 0, y: 0 },
                },
                duration_ms: 500,
            }),
        },
    );
    repair_and_report_change(dialog);
    true
}

pub(crate) fn activation_matcher(region: &SearchRegion) -> Option<MkWindowMatcher> {
    match region {
        SearchRegion::Window { matcher } | SearchRegion::ClientArea { matcher } => {
            Some(matcher.clone())
        }
        SearchRegion::Desktop | SearchRegion::Monitor { .. } | SearchRegion::Rectangle { .. } => {
            None
        }
    }
}

/// Inserts a normal activation action before the stable search row.
pub(crate) fn insert_activate_window_before(
    dialog: &mut MkMacroDialog,
    anchor_id: u64,
    payload: &MkImagePayload,
) -> bool {
    let Some(matcher) = activation_matcher(&payload.region) else {
        return false;
    };
    let Some(macro_) = dialog.selected_macro_mut() else {
        return false;
    };
    let Some(index) = macro_.steps.iter().position(|step| step.id == anchor_id) else {
        return false;
    };
    macro_.steps.insert(
        index,
        MkStep {
            id: 0,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action: MkAction::WindowActivate(MkWindowPayload {
                matcher,
                wait: None,
            }),
        },
    );
    repair_and_report_change(dialog);
    true
}

fn repair_and_report_change(dialog: &mut MkMacroDialog) {
    crate::mkmacro::repair_ids(&mut dialog.draft);
    dialog.mark_dirty();
}

fn ensure_image_asset_catalog_entry(dialog: &mut MkMacroDialog, asset_id: u64) {
    if asset_id == 0 {
        return;
    }
    let Some(macro_) = dialog.selected_macro_mut() else {
        return;
    };
    if !macro_.image_assets.iter().any(|asset| asset.id == asset_id) {
        macro_.image_assets.push(MkImageAsset {
            id: asset_id,
            name: String::new(),
            relative_path: format!("mkmacro_assets/{}/{}.png", macro_.id, asset_id),
        });
        dialog.mark_dirty();
    }
}

/// Non-egui completion reducer used by the frame loop and authoring tests.
/// A staged file becomes visible in the live macro asset browser only when the
/// editor still owns the exact destination that initiated the operation.
fn reduce_image_authoring_completion(dialog: &mut MkMacroDialog) {
    let selected_macro_id = dialog.selected_macro_id;
    let Some(staged) = dialog.action_editor.poll_image_authoring(selected_macro_id) else {
        return;
    };
    let Some(macro_) = dialog.selected_macro_mut() else {
        return;
    };
    if !macro_
        .image_assets
        .iter()
        .any(|asset| asset.id == staged.asset_id)
    {
        macro_.image_assets.push(MkImageAsset {
            id: staged.asset_id,
            name: String::new(),
            relative_path: staged
                .managed_reference
                .to_string_lossy()
                .replace('\\', "/"),
        });
        dialog.mark_dirty();
    }
}

fn image_payload_mut(step: &mut MkStep) -> Option<&mut MkImagePayload> {
    match &mut step.action {
        MkAction::ImageFind(p) | MkAction::ImageClick(p) => Some(p),
        _ => None,
    }
}
fn image_output_names_valid(action: &MkAction) -> bool {
    let MkAction::ImageFind(p) = action else {
        return true;
    };
    let names: Vec<_> = [
        &p.outputs.found,
        &p.outputs.point,
        &p.outputs.x,
        &p.outputs.y,
    ]
    .into_iter()
    .flatten()
    .collect();
    names
        .iter()
        .all(|name| crate::mkmacro::variables::validate_variable_name(name).is_ok())
        && !names
            .iter()
            .enumerate()
            .any(|(i, name)| names[..i].contains(name))
}

fn apply_capture(
    store: &MkMacroStore,
    macro_id: u64,
    payload: &mut MkImagePayload,
    image: &image::RgbaImage,
) -> anyhow::Result<()> {
    let staged = ImageAssetAuthoringService::new(store).stage_rgba(macro_id, image)?;
    payload.asset_id = staged.asset_id;
    Ok(())
}

fn condition_at_path<'a>(
    mut c: &'a mut MkCondition,
    path: &[usize],
) -> Option<&'a mut MkWindowMatcher> {
    for &index in path {
        c = match c {
            MkCondition::All { conditions } | MkCondition::Any { conditions } => {
                conditions.get_mut(index)?
            }
            MkCondition::Not { condition } if index == 0 => condition,
            _ => return None,
        };
    }
    match c {
        MkCondition::WindowExists { matcher } | MkCondition::WindowActive { matcher } => {
            Some(matcher)
        }
        _ => None,
    }
}

fn matcher_at_path<'a>(
    action: &'a mut MkAction,
    path: &super::window_picker::MatcherPath,
) -> Option<&'a mut MkWindowMatcher> {
    use super::window_picker::MatcherPath;
    match path {
        MatcherPath::Action => match action {
            MkAction::WindowActivate(p) | MkAction::WindowWait(p) => Some(&mut p.matcher),
            MkAction::WindowClose(m) => Some(m),
            MkAction::WindowMoveResize(p) => Some(&mut p.matcher),
            MkAction::WindowState { matcher, .. } => Some(matcher),
            _ => None,
        },
        MatcherPath::Condition(path) => match action {
            MkAction::If(c)
            | MkAction::WhileStart { condition: c }
            | MkAction::WaitUntil { condition: c, .. } => condition_at_path(c, path),
            _ => None,
        },
        MatcherPath::VisualRegion => match action {
            MkAction::ImageFind(p) | MkAction::ImageClick(p) => match &mut p.region {
                SearchRegion::Window { matcher } | SearchRegion::ClientArea { matcher } => {
                    Some(matcher)
                }
                _ => None,
            },
            MkAction::WaitForVisualChange(p) => match &mut p.region {
                SearchRegion::Window { matcher } | SearchRegion::ClientArea { matcher } => {
                    Some(matcher)
                }
                _ => None,
            },
            _ => None,
        },
        MatcherPath::Coordinate(path) => {
            use super::window_picker::CoordinateMatcherPath::*;
            let target = match (path, action) {
                (MoveTarget, MkAction::MouseMove(p)) => &mut p.target,
                (ClickTarget, MkAction::MouseClick(p)) => &mut p.target,
                (DragFrom, MkAction::MouseDrag(p)) => &mut p.from,
                (DragTo, MkAction::MouseDrag(p)) => &mut p.to,
                (PixelPosition, MkAction::PixelCheck { target, .. }) => target,
                _ => return None,
            };
            match target {
                MkCoordinateTarget::WindowClient { matcher, .. } => Some(matcher),
                _ => None,
            }
        }
    }
}

pub(super) fn show(ctx: &egui::Context, d: &mut MkMacroDialog) {
    if d.action_editor.draft.is_none() {
        return;
    }
    let mut open = true;
    d.action_editor.tick_visual_capture(d.selected_macro_id);
    reduce_image_authoring_completion(d);
    let importing = d.action_editor.image_authoring.is_importing();
    if importing {
        ctx.request_repaint();
    }
    let overlay_was_active = d.action_editor.visual_overlay.operation_id().is_some();
    d.action_editor.poll_visual_overlay(d.selected_macro_id);
    if overlay_was_active || d.action_editor.visual_overlay.operation_id().is_some() {
        ctx.request_repaint();
    }
    let workflow_active = d
        .action_editor
        .visual_capture
        .as_ref()
        .is_some_and(|w| w.active());
    if workflow_active {
        ctx.request_repaint();
    }
    let mut apply = false;
    let mut cancel = false;
    let mut captured = None;
    if d.action_editor.position_capture.is_some() {
        ctx.request_repaint();
        match d.action_editor.picker.poll_event() {
            Ok(Some(event)) => d.action_editor.process_position_event(event),
            Ok(None) => {}
            Err(error) => {
                d.action_editor.stop_position_capture();
                d.action_editor.capture_message = Some(error);
            }
        }
    }
    let mut pick_request = None;
    let mut image_request = None;
    let mut wait_visual_region_request = false;
    let mut screenshot_region_request = false;
    let mut condition_image_request = None;
    let mut preview_request = None;
    let mut point_pick_request = false;
    // Execute after egui releases the mutable draft borrow held by `step`.
    let mut region_preview_request = None;
    let image_assets = d
        .selected_macro()
        .map(|m| m.image_assets.clone())
        .unwrap_or_default();
    let live_steps = d
        .selected_macro()
        .map(|macro_| macro_.steps.clone())
        .unwrap_or_default();
    d.action_editor.refresh_variable_catalog(&live_steps);
    egui::Window::new("Action Editor")
        .open(&mut open)
        .collapsible(false)
        .default_size(egui::vec2(640.0, 720.0))
        .resizable(true)
        .show(ctx, |ui| {
          egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let state = &mut d.action_editor;
            let draft_generation = state.draft_generation;
            let point_pick_active = state.active_point_pick.is_some();
            let step = state.draft.as_mut().unwrap();
            let editor = state.editor.expect("action editor draft has no editor strategy");
            assert!(super::action_catalog::editor_route_recognizes(&step.action, editor), "action editor strategy does not match draft action");
            let action_before = step.action.clone();
            let image_context = super::image_asset_picker::ImageAssetUiContext {
                macro_id: d.selected_macro_id.unwrap_or(0),
                assets: &image_assets,
                store: &d.store,
            };
            let (position, mut window, launcher, image, condition_image, preview, pick_point)=action_ui(
                ui,
                step,
                &mut state.capture_keys,
                image_context,
                draft_generation,
                &state.variable_catalog,
                point_pick_active,
            );
            if step.action != action_before { state.draft_changed = true; }
            pick_request = position;
            image_request = image;
            condition_image_request = condition_image;
            preview_request = preview;
            point_pick_request = pick_point;
            if matches!(
                step.action,
                MkAction::WaitForVisualChange(_) | MkAction::CaptureScreenshot(_)
            ) {
                ui.separator(); ui.heading("Region");
                let is_screenshot = matches!(step.action, MkAction::CaptureScreenshot(_));
                let region_state = state.image_search.as_mut().expect("visual region requires editor state");
                let before = region_state.selected_region();
                if let Some(request) = super::image_search_controls::show_search_region_fields(ui, region_state) {
                    use super::image_search_controls::SearchRegionRequest as R;
                    match request {
                        R::SelectRectangle => {
                            image_request = Some(ImageAuthoringRequest::PickRectangle);
                            if is_screenshot {
                                screenshot_region_request = true;
                            } else {
                                wait_visual_region_request = true;
                            }
                        }
                        R::PickWindow => window = Some(super::window_picker::MatcherPath::VisualRegion),
                        R::RefreshMonitors => region_state.refresh_monitors(),
                        R::IdentifyMonitors => {},
                        R::PreviewRegion => {
                            region_preview_request = Some(region_state.selected_region());
                        }
                    }
                }
                let selected = region_state.selected_region();
                if selected != before {
                    match &mut step.action {
                        MkAction::WaitForVisualChange(payload) => payload.region = selected,
                        MkAction::CaptureScreenshot(payload) => payload.region = selected,
                        _ => unreachable!(),
                    }
                    state.draft_changed = true;
                }
            }
            let find_action = matches!(step.action, MkAction::ImageFind(_));
            if let Some(payload) = image_payload_mut(step) {
                let request = super::image_search_editor::show(ui, payload, state.image_search.as_mut().expect("image action requires image editor state"), &d.store, d.selected_macro_id.unwrap_or(0), importing, find_action, image_assets.iter().any(|asset| asset.id == payload.asset_id));
                use super::image_search_editor::ImageEditorRequest::*;
                match request {
                    Some(ImportPng) => image_request = Some(ImageAuthoringRequest::Import),
                    Some(CaptureRectangle) => image_request = Some(ImageAuthoringRequest::CaptureRectangle),
                    Some(SelectRegion) => image_request = Some(ImageAuthoringRequest::PickRectangle),
                    Some(PickWindow { .. }) => window = Some(super::window_picker::MatcherPath::VisualRegion),
                    Some(PreviewRegion) | Some(HighlightMonitor) | Some(HighlightWindow { .. }) => {
                        region_preview_request =
                            Some(state.image_search.as_ref().unwrap().selected_region());
                    }
                    Some(IdentifyMonitors) => {
                        let image = state.image_search.as_mut().unwrap();
                        image.refresh_monitors();
                        match &image.monitors {
                            Ok(monitors) if !monitors.is_empty() => {
                                let id = state.visual_overlay.identify_monitors(monitors.clone());
                                state.overlay_diagnostic =
                                    Some((id, "Unable to identify monitors".into()));
                                if state.visual_overlay.operation_id() == Some(id) {
                                    state.capture_message = None;
                                }
                            }
                            Ok(_) => state.capture_message = Some("No monitors are currently available".into()),
                            Err(error) => state.capture_message = Some(format!("Monitor information unavailable: {error}")),
                        }
                    }
                    Some(AddSmoothMouseMove) => state.add_smooth_move = true,
                    Some(AddActivateWindowBefore) => state.add_activate_before = true,
                    None => {}
                }
            }
            if let Some(path)=window {
                let original = if matches!(path, super::window_picker::MatcherPath::VisualRegion) {
                    let image=state.image_search.as_ref().expect("image picker requires image state");
                    if image.kind == super::image_search_editor::SearchRegionKind::ClientArea { image.client_matcher.clone() } else { image.window_matcher.clone() }
                } else { matcher_at_path(&mut step.action, &path).expect("picker path must resolve").clone() };
                let macro_id=d.selected_macro_id.unwrap_or(0);
                d.window_picker.open(super::window_picker::MatcherEditRequest { destination: super::window_picker::MatcherDestination::Action { macro_id, draft_generation: state.draft_generation, path }, original });
            }
            if let Some(purpose) = launcher {
                let request = super::launcher_action_picker::LauncherActionRequest {
                    purpose,
                    macro_id: d.selected_macro_id.unwrap_or(0),
                    step_id: state.editing_id,
                    draft_generation: state.draft_generation,
                };
                d.launcher_action_picker.open(request);
            }
            if state.position_capture.is_some() {
                ui.colored_label(egui::Color32::YELLOW, "Move the mouse to the desired location. Left-click or press Enter to capture. Escape cancels.");
            }
            if let Some(message) = &state.capture_message { ui.colored_label(egui::Color32::RED, message); }
            if state.capture_keys {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "Press the next real key or chord. Escape cancels capture.",
                );
                if let Some(result) = ui.input(captured_chord) {
                    match result {
                        CapturedChord::Cancelled => state.capture_keys = false,
                        CapturedChord::Keys(keys) => captured = Some(keys),
                    }
                }
            }
            ui.separator();
            ui.heading("Step settings");
            ui.horizontal(|ui| {
                ui.label("Repeat");
                ui.add(egui::DragValue::new(&mut step.repeat).clamp_range(1..=1_000_000));
                ui.label("Delay after (ms)");
                ui.add(egui::DragValue::new(&mut step.delay_after_ms).clamp_range(0..=86_400_000));
            });
            egui::ComboBox::from_label("On error")
                .selected_text(match step.on_error {
                    MkErrorPolicy::Stop => "Stop",
                    MkErrorPolicy::Continue => "Continue",
                    MkErrorPolicy::Retry(_) => "Retry",
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(matches!(step.on_error, MkErrorPolicy::Stop), "Stop")
                        .clicked()
                    {
                        step.on_error = MkErrorPolicy::Stop
                    }
                    if ui
                        .selectable_label(
                            matches!(step.on_error, MkErrorPolicy::Continue),
                            "Continue",
                        )
                        .clicked()
                    {
                        step.on_error = MkErrorPolicy::Continue
                    }
                    if ui
                        .selectable_label(matches!(step.on_error, MkErrorPolicy::Retry(_)), "Retry")
                        .clicked()
                    {
                        step.on_error = MkErrorPolicy::Retry(MkRetry {
                            attempts: 3,
                            delay_ms: 100,
                        })
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                let valid = !matches!(&step.action, MkAction::PromptInput(p) if crate::mkmacro::variables::validate_variable_name(&p.variable).is_err())
                    && !matches!(&step.action, MkAction::Notify(p) if p.title.trim().is_empty())
                    && !matches!(&step.action, MkAction::PlaySound(p) if !play_sound_is_supported(&p.sound))
                    && image_output_names_valid(&step.action)
                    && !matches!(
                        super::action_catalog::draft_validation_contract(&step.action),
                        super::action_catalog::DraftValidationContract::AwaitingRequiredAsset
                    )
                    && state.image_search.as_ref().and_then(|image| image.validation_error()).is_none();
                apply = ui.add_enabled(valid && !workflow_active && !importing, egui::Button::new("Apply")).clicked();
                cancel = ui.button(if workflow_active { "Cancel visual capture" } else { "Cancel" }).on_hover_text("Cancel editing; during playback Cancel stops the macro").clicked();
            });
          });
        });
    if let Some(request) = preview_request {
        dispatch_preview(&mut d.action_editor, request);
    }
    if point_pick_request {
        d.action_editor
            .request_point_pick(d.selected_macro_id.unwrap_or(0));
    }
    if let Some(region) = region_preview_request {
        d.action_editor.preview_region(region);
    }
    if (workflow_active || importing) && image_request.is_some() {
        d.action_editor.capture_message = Some(
            if importing {
                "Wait for the active reference image import to finish"
            } else {
                "Finish or cancel the active visual capture first"
            }
            .into(),
        );
    } else if let Some(request) = image_request {
        let macro_id = d.selected_macro_id.unwrap_or(0);
        let result = match request {
            ImageAuthoringRequest::Import => rfd::FileDialog::new()
                .add_filter("PNG image", &["png"])
                .pick_file()
                .map(|path| {
                    d.action_editor
                        .start_import_from_selected_path(d.store.clone(), macro_id, path)
                })
                .transpose()
                .map(|_| ()),
            request @ (ImageAuthoringRequest::CaptureRectangle
            | ImageAuthoringRequest::PickRectangle) => {
                let destination = if wait_visual_region_request {
                    VisualRegionDestination::WaitForVisualChangeRegion
                } else if screenshot_region_request {
                    VisualRegionDestination::CaptureScreenshotRegion
                } else if matches!(request, ImageAuthoringRequest::CaptureRectangle) {
                    VisualRegionDestination::ImageActionReferenceAsset
                } else {
                    VisualRegionDestination::ImageActionSearchRegion
                };
                d.action_editor.request_rectangle_selection(
                    macro_id,
                    request
                        .rectangle_purpose()
                        .expect("rectangle request has a purpose"),
                    destination,
                )
            }
        };
        if let Err(error) = result {
            d.action_editor.capture_message = Some(format!("Reference image: {error:#}"));
        }
    }
    if let Some(ref request) = condition_image_request
        && matches!(
            request.operation,
            super::condition_editor::ConditionImageOperation::ImportPng
        )
    {
        if workflow_active || importing {
            d.action_editor.capture_message =
                Some("Finish or cancel the active authoring operation first".into());
        } else {
            let destination = d
                .action_editor
                .condition_destination(d.selected_macro_id.unwrap_or(0), request.clone());
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG image", &["png"])
                .pick_file()
                && let Err(error) = d.action_editor.start_condition_import_from_selected_path(
                    d.store.clone(),
                    destination,
                    path,
                )
            {
                d.action_editor.capture_message = Some(format!("Condition image: {error:#}"));
            }
        }
    }
    if let Some(ref request) = condition_image_request
        && let Some(purpose) = request.operation.rectangle_purpose()
    {
        let macro_id = d.selected_macro_id.unwrap_or(0);
        if workflow_active || importing {
            d.action_editor.capture_message =
                Some("Finish or cancel the active authoring operation first".into());
        } else {
            let destination = d
                .action_editor
                .condition_destination(macro_id, request.clone());
            if let Err(error) = d.action_editor.request_rectangle_selection(
                macro_id,
                purpose,
                VisualRegionDestination::ConditionSearchRegion(destination),
            ) {
                d.action_editor.capture_message = Some(format!("Condition image: {error:#}"));
            }
        }
    }
    if let Some(ref request) = condition_image_request
        && matches!(
            request.operation,
            super::condition_editor::ConditionImageOperation::PreviewRectangle
                | super::condition_editor::ConditionImageOperation::HighlightMonitor
                | super::condition_editor::ConditionImageOperation::HighlightWindow
        )
    {
        let region = d
            .action_editor
            .draft
            .as_ref()
            .and_then(|step| match &step.action {
                MkAction::If(c)
                | MkAction::WhileStart { condition: c }
                | MkAction::WaitUntil { condition: c, .. } => {
                    super::condition_editor::resolve_condition(c, &request.path)
                }
                _ => None,
            })
            .and_then(|condition| match condition {
                MkCondition::ImageSearch { search, .. } => Some(search.region.clone()),
                _ => None,
            });
        match region {
            Some(region) => d.action_editor.preview_region(region),
            None => {
                d.action_editor.capture_message =
                    Some("Image-search condition is no longer available".into())
            }
        }
    }
    if let Some(slot) = pick_request {
        if let Err(error) = d.action_editor.start_position_capture(slot) {
            d.action_editor.capture_message = Some(error);
        }
    }
    if let Some(keys) = captured {
        d.action_editor.set_captured_keys(keys);
    }
    if apply {
        if let Some(payload) = d.action_editor.draft.as_mut().and_then(image_payload_mut) {
            payload.outputs.normalize();
        }
        if let Some(MkStep {
            action: MkAction::PixelCheck { color, .. },
            ..
        }) = d.action_editor.draft.as_mut()
        {
            match crate::mkmacro::screen::parse_rgb(color) {
                Ok(rgb) => *color = crate::mkmacro::screen::format_rgb(rgb),
                Err(error) => {
                    d.action_editor.capture_message = Some(error.to_string());
                    return;
                }
            }
        }
        let mut state = d.take_action_editor();
        let smooth = state.add_smooth_move;
        let activate = state.add_activate_before;
        let shortcut_payload = state.draft.as_ref().and_then(image_payload).cloned();
        let anchor = state.apply(d);
        if let (Some(anchor), Some(payload)) = (anchor, shortcut_payload.as_ref()) {
            if activate {
                insert_activate_window_before(d, anchor, payload);
            }
            if smooth {
                insert_smooth_move_after(d, anchor, payload);
            }
            ensure_image_asset_catalog_entry(d, payload.asset_id);
        }
        d.action_editor = state;
        d.window_picker
            .cancel("Window picker closed because the action editor was applied");
        d.launcher_action_picker.cancel();
    } else if cancel || !open {
        d.action_editor.cancel();
        d.window_picker
            .cancel("Window picker closed because the action editor was cancelled");
        d.launcher_action_picker.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::super::visual_overlay::{
        OperationId, RectanglePurpose, VisualOverlayCommand, VisualOverlayEvent,
    };
    use super::*;
    use image::RgbaImage;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};

    fn variable_step(id: u64, name: &str, value: MkValue) -> MkStep {
        MkStep {
            id,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action: MkAction::SetVariable {
                name: name.into(),
                value,
            },
        }
    }

    #[test]
    fn variable_picker_model_filters_effective_points_and_preserves_manual_text() {
        let steps = vec![
            variable_step(1, "point", MkValue::Point(MkPoint { x: 1, y: 2 })),
            variable_step(2, "text", MkValue::String("hello".into())),
            variable_step(3, "shadowed", MkValue::Point(MkPoint { x: 3, y: 4 })),
            variable_step(4, "shadowed", MkValue::Number(42.0)),
            variable_step(5, "future", MkValue::Point(MkPoint { x: 5, y: 6 })),
        ];
        let catalog = VariableCatalog::before_step(&steps, 4);
        let model = VariablePickerModel::new(&catalog, |kind| kind == VariableValueType::Point);
        assert_eq!(
            model
                .suggestions
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            vec!["point"]
        );

        let mut manual = "unknown_custom_name".to_owned();
        assert_eq!(manual, "unknown_custom_name");
        assert!(!model.select(99, &mut manual));
        assert_eq!(manual, "unknown_custom_name");
        assert!(model.select(0, &mut manual));
        assert_eq!(manual, "point");
    }

    #[test]
    fn editor_catalog_re_resolves_stable_consumer_after_reorder() {
        let producer = variable_step(1, "point", MkValue::Point(MkPoint { x: 1, y: 2 }));
        let consumer = variable_step(2, "unused", MkValue::Null);
        let mut editor = test_editor();
        editor.begin_edit(&consumer);
        editor.refresh_variable_catalog(&[producer.clone(), consumer.clone()]);
        assert_eq!(editor.variable_consumer_index, 1);
        assert_eq!(editor.variable_consumer_id, Some(2));
        assert_eq!(editor.variable_catalog.effective_variables().len(), 1);

        editor.refresh_variable_catalog(&[consumer, producer]);
        assert_eq!(editor.variable_consumer_index, 0);
        assert!(editor.variable_catalog.effective_variables().is_empty());
    }

    #[test]
    fn mouse_move_editor_uses_the_catalog_point_filter() {
        let marker = |id, action| MkStep {
            id,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action,
        };
        let steps = vec![
            variable_step(401, "p", MkValue::Point(MkPoint { x: 1, y: 2 })),
            marker(
                402,
                MkAction::PromptInput(MkPromptInputPayload {
                    variable: "text".into(),
                    ..Default::default()
                }),
            ),
            marker(
                403,
                MkAction::ImageFind(MkImagePayload {
                    asset_id: 1,
                    wait: MkWaitOptions::default(),
                    region: SearchRegion::Desktop,
                    tolerance: 0,
                    alpha: AlphaPolicy::Compare,
                    return_point: ReturnPoint::Center,
                    not_found_policy: MkImageNotFoundPolicy::Continue,
                    outputs: MkImageOutputs {
                        found: Some("was_found".into()),
                        point: Some("found_point".into()),
                        x: None,
                        y: None,
                    },
                }),
            ),
            marker(
                404,
                MkAction::MouseMove(MkMouseMovePayload {
                    target: MkCoordinateTarget::Variable { name: "p".into() },
                    duration_ms: 0,
                }),
            ),
        ];
        let pure_catalog = VariableCatalog::before_step(&steps, 3);
        let pure = VariablePickerModel::new(&pure_catalog, |kind| kind == VariableValueType::Point);

        let mut editor = test_editor();
        editor.begin_edit(&steps[3]);
        editor.refresh_variable_catalog(&steps);
        let integrated = VariablePickerModel::new(&editor.variable_catalog, |kind| {
            kind == VariableValueType::Point
        });

        assert_eq!(integrated, pure);
        assert_eq!(
            integrated
                .suggestions
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            vec!["p", "found_point"]
        );
        assert_eq!(
            integrated.suggestions[1].availability,
            super::super::variable_catalog::VariableAvailability::PossiblyUnavailable
        );
        assert!(
            integrated.suggestions[1]
                .help_text
                .unwrap()
                .contains("Null")
        );
    }

    #[test]
    fn variable_picker_details_distinguish_nullable_and_structural_warnings() {
        use super::super::variable_catalog::{VariableAvailability, VariableUncertaintyReason};
        let marker = |id, action| MkStep {
            id,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action,
        };
        let condition = MkCondition::Variable {
            name: "guard".into(),
            op: MkCompareOp::Eq,
            value: MkValue::Boolean(true),
        };
        let conditional_steps = vec![
            marker(77, MkAction::If(condition)),
            variable_step(
                78,
                "conditional_point",
                MkValue::Point(MkPoint { x: 1, y: 2 }),
            ),
            marker(79, MkAction::EndIf),
        ];
        let catalog = VariableCatalog::before_step(&conditional_steps, usize::MAX);
        let model = VariablePickerModel::new(&catalog, |kind| kind == VariableValueType::Point);
        assert_eq!(model.suggestions.len(), 1);
        assert_eq!(
            model.suggestions[0].availability,
            VariableAvailability::PossiblyUnavailable
        );
        assert!(matches!(
            model.suggestions[0].uncertainty_reasons.as_slice(),
            [VariableUncertaintyReason::ProducedInside(MkBlockKind::If)]
        ));

        let descriptor = VariableDescriptor {
            name: "image_point".into(),
            value_type: VariableValueType::Point,
            source_step_id: 77,
            source_step_index: 2,
            source_step_number: 3,
            source_action_label: "Find Image",
            availability: VariableAvailability::PossiblyUnavailable,
            uncertainty_reasons: vec![
                VariableUncertaintyReason::ProducedInside(MkBlockKind::If),
                VariableUncertaintyReason::MayBeNullIfNotFound,
            ],
            help_text: Some("May be Null if the image is not found"),
        };
        assert_eq!(descriptor.warning_marker(), Some("⚠"));
        let details = variable_detail_text(&descriptor);
        assert!(details.contains("Find Image at step 3 (stable ID 77)"));
        assert!(details.contains("Produced inside If"));
        assert!(details.contains("May be Null if the image is not found"));
        assert!(details.contains(RUNTIME_SCOPE_TOOLTIP));
    }

    #[test]
    fn target_context_resolves_only_nonzero_current_macro_assets() {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let assets = vec![MkImageAsset {
            id: 14,
            name: "needle".into(),
            relative_path: "mkmacro_assets/7/14.png".into(),
        }];
        let context = TargetEditorContext {
            macro_id: 7,
            assets: &assets,
            store: &store,
        };

        assert_eq!(context.resolve_asset(14).map(|asset| asset.id), Some(14));
        assert!(context.resolve_asset(0).is_none());
        assert!(context.resolve_asset(99).is_none());
    }

    struct TestDesktop;
    impl ScreenCaptureBackend for TestDesktop {
        fn virtual_desktop(&self) -> ExecResult<ScreenRect> {
            Ok(ScreenRect::new(-100, -100, 1000, 1000))
        }
        fn region_bounds(&self, _: &SearchRegion) -> ExecResult<ScreenRect> {
            self.virtual_desktop()
        }
        fn capture_rect(&self, rect: ScreenRect, _: &dyn Fn() -> bool) -> ExecResult<RgbaImage> {
            Ok(RgbaImage::new(rect.width, rect.height))
        }
    }
    struct TestAssets;
    impl super::super::visual_capture_workflow::AssetStoreAdapter for TestAssets {
        fn write_png_asset(&mut self, _: u64, _: &RgbaImage) -> Result<u64, String> {
            Ok(91)
        }
    }

    fn install_real_service_workflow(editor: &mut ActionEditorState) {
        use super::super::visual_capture_workflow::{
            ScreenCaptureAdapter, VisualCaptureWorkflow, VisualOverlayRectangleAdapter,
        };
        let desktop: Arc<dyn ScreenCaptureBackend> = Arc::new(TestDesktop);
        editor.visual_capture = Some(VisualCaptureWorkflow::new(
            Box::new(VisualOverlayRectangleAdapter::new(
                editor.visual_overlay.clone(),
                desktop.clone(),
            )),
            Box::new(ScreenCaptureAdapter(desktop)),
            Box::new(TestAssets),
        ));
    }

    fn test_editor() -> ActionEditorState {
        ActionEditorState::new(
            super::super::visual_capture_workflow::SharedVisualOverlayController::new(
                super::super::visual_overlay::VisualOverlayController::default(),
            ),
        )
    }

    fn image_action() -> MkAction {
        MkAction::ImageFind(MkImagePayload {
            asset_id: 1,
            region: SearchRegion::Rectangle {
                rect: ScreenRect::new(1, 2, 30, 40),
            },
            wait: MkWaitOptions::default(),
            tolerance: 0,
            alpha: AlphaPolicy::Ignore,
            return_point: ReturnPoint::TopLeft,
            not_found_policy: MkImageNotFoundPolicy::Fail,
            outputs: MkImageOutputs::default(),
        })
    }

    #[test]
    fn optional_outputs_normalize_at_apply_boundary_and_survive_persistence() {
        for (input, expected) in [
            ("point", Some("point")),
            ("  point  ", Some("point")),
            ("", None),
            ("   ", None),
        ] {
            let mut action = image_action();
            let MkAction::ImageFind(payload) = &mut action else {
                unreachable!()
            };
            payload.outputs = MkImageOutputs {
                found: Some(" found ".into()),
                point: Some(input.into()),
                x: Some(" x ".into()),
                y: Some(" y ".into()),
            };
            normalize_optional_outputs(&mut action);
            let saved = serde_json::to_vec(&action).unwrap();
            let loaded: MkAction = serde_json::from_slice(&saved).unwrap();
            let MkAction::ImageFind(payload) = loaded else {
                unreachable!()
            };
            assert_eq!(payload.outputs.point.as_deref(), expected);
            assert_eq!(payload.outputs.found.as_deref(), Some("found"));
            assert_eq!(payload.outputs.x.as_deref(), Some("x"));
            assert_eq!(payload.outputs.y.as_deref(), Some("y"));
        }

        let mut screenshot = screenshot_action(SearchRegion::Desktop);
        let MkAction::CaptureScreenshot(payload) = &mut screenshot else {
            unreachable!()
        };
        payload.path_output = Some(" saved_path ".into());
        normalize_optional_outputs(&mut screenshot);
        let MkAction::CaptureScreenshot(payload) = screenshot else {
            unreachable!()
        };
        assert_eq!(payload.path_output.as_deref(), Some("saved_path"));
    }

    fn screenshot_action(region: SearchRegion) -> MkAction {
        MkAction::CaptureScreenshot(MkScreenshotPayload {
            region,
            destination: MkScreenshotDestination::Clipboard,
            path: None,
            format: MkScreenshotFormat::Png,
            collision: MkFileCollisionPolicy::Error,
            path_output: None,
        })
    }

    #[derive(Default)]
    struct PickerState {
        next_id: u64,
        events: std::collections::VecDeque<super::super::visual_capture_workflow::SelectionEvent>,
        cancelled: Vec<u64>,
    }
    struct FakeRectanglePicker(Arc<Mutex<PickerState>>);
    impl super::super::visual_capture_workflow::RectangleOverlay for FakeRectanglePicker {
        fn begin(
            &mut self,
            _: super::super::visual_overlay::RectanglePurpose,
        ) -> Result<u64, String> {
            let mut state = self.0.lock().unwrap();
            state.next_id += 1;
            Ok(state.next_id)
        }
        fn poll(&mut self) -> super::super::visual_capture_workflow::SelectionEvent {
            self.0
                .lock()
                .unwrap()
                .events
                .pop_front()
                .unwrap_or(super::super::visual_capture_workflow::SelectionEvent::Pending)
        }
        fn cancel(&mut self, operation_id: u64) {
            self.0.lock().unwrap().cancelled.push(operation_id);
        }
    }
    struct UnusedCapture;
    impl super::super::visual_capture_workflow::CaptureAdapter for UnusedCapture {
        fn capture_rect(&mut self, _: ScreenRect) -> Result<RgbaImage, String> {
            panic!("search-region selection must not capture a screenshot")
        }
    }
    struct UnusedAssetStore;
    impl super::super::visual_capture_workflow::AssetStoreAdapter for UnusedAssetStore {
        fn write_png_asset(&mut self, _: u64, _: &RgbaImage) -> Result<u64, String> {
            panic!("search-region selection must not import an asset")
        }
    }
    fn install_fake_picker(editor: &mut ActionEditorState) -> Arc<Mutex<PickerState>> {
        let state = Arc::new(Mutex::new(PickerState::default()));
        editor.visual_capture = Some(
            super::super::visual_capture_workflow::VisualCaptureWorkflow::new(
                Box::new(FakeRectanglePicker(state.clone())),
                Box::new(UnusedCapture),
                Box::new(UnusedAssetStore),
            ),
        );
        state
    }
    fn select_screenshot_region(editor: &mut ActionEditorState, macro_id: u64) {
        editor
            .request_rectangle_selection(
                macro_id,
                super::super::visual_overlay::RectanglePurpose::SearchRegion,
                VisualRegionDestination::CaptureScreenshotRegion,
            )
            .unwrap();
    }

    #[test]
    fn screenshot_picker_completion_changes_only_the_region_and_cancellation_is_retryable() {
        let rectangle_a = ScreenRect::new(-900, 20, 301, 207);
        let rectangle_b = ScreenRect::new(45, -330, 640, 480);
        let mut action = screenshot_action(SearchRegion::Rectangle { rect: rectangle_a });
        let MkAction::CaptureScreenshot(payload) = &mut action else {
            unreachable!()
        };
        payload.destination = MkScreenshotDestination::File;
        payload.path = Some("captures/example.jpg".into());
        payload.format = MkScreenshotFormat::Jpeg;
        payload.collision = MkFileCollisionPolicy::Unique;
        payload.path_output = Some("screenshot_path".into());

        let mut editor = test_editor();
        editor.begin_edit(&MkStep {
            id: 808,
            enabled: false,
            repeat: 3,
            delay_after_ms: 91,
            on_error: MkErrorPolicy::Continue,
            action,
        });
        let picker = install_fake_picker(&mut editor);
        let original = editor.draft.clone().unwrap();
        select_screenshot_region(&mut editor, 17);
        let operation_id = match editor.visual_capture.as_ref().unwrap().state() {
            super::super::visual_capture_workflow::WorkflowState::Selecting {
                operation_id,
                ..
            } => *operation_id,
            state => panic!("unexpected picker state: {state:?}"),
        };
        let target = editor.pending_visual_region.clone().unwrap();
        assert_eq!(target.step_id, Some(808));
        assert_eq!(target.draft_generation, editor.draft_generation);
        picker.lock().unwrap().events.push_back(
            super::super::visual_capture_workflow::SelectionEvent::Confirmed {
                operation_id,
                rect: rectangle_b,
            },
        );
        editor.tick_visual_capture(Some(17));
        let mut expected = original.clone();
        let MkAction::CaptureScreenshot(expected_payload) = &mut expected.action else {
            unreachable!()
        };
        expected_payload.region = SearchRegion::Rectangle { rect: rectangle_b };
        assert_eq!(editor.draft.as_ref(), Some(&expected));
        assert!(editor.pending_visual_region.is_none());

        // A second, independent operation is cancelled by the picker.  It must
        // release ownership, preserve A, and remain available for an immediate retry.
        editor.draft = Some(original.clone());
        editor.capture_message = None;
        select_screenshot_region(&mut editor, 17);
        let cancelled_id = picker.lock().unwrap().next_id;
        picker.lock().unwrap().events.push_back(
            super::super::visual_capture_workflow::SelectionEvent::Cancelled {
                operation_id: cancelled_id,
            },
        );
        editor.tick_visual_capture(Some(17));
        assert_eq!(editor.draft.as_ref(), Some(&original));
        assert!(editor.pending_visual_region.is_none());
        assert!(!editor.image_authoring.is_importing());
        assert_eq!(
            editor.capture_message.as_deref(),
            Some("Visual capture cancelled")
        );
        select_screenshot_region(&mut editor, 17);
        assert!(editor.pending_visual_region.is_some());
    }

    #[test]
    fn late_screenshot_picker_result_cannot_mutate_a_new_target_or_closed_editor() {
        let rectangle_a = ScreenRect::new(-30, -20, 10, 11);
        let rectangle_b = ScreenRect::new(500, 400, 30, 20);
        for close_editor in [false, true] {
            let mut editor = test_editor();
            let step_a = MkStep {
                id: 1,
                enabled: true,
                repeat: 1,
                delay_after_ms: 0,
                on_error: MkErrorPolicy::Stop,
                action: screenshot_action(SearchRegion::Rectangle { rect: rectangle_a }),
            };
            editor.begin_edit(&step_a);
            let picker = install_fake_picker(&mut editor);
            select_screenshot_region(&mut editor, 17);
            let old_id = picker.lock().unwrap().next_id;
            let old_token = super::super::visual_capture_workflow::DraftToken {
                macro_id: 17,
                draft_generation: editor.draft_generation,
            };
            if close_editor {
                editor.cancel();
                editor.apply_visual_capture_outcome(
                    Some(17),
                    super::super::visual_capture_workflow::WorkflowOutcome::Region {
                        token: old_token,
                        rect: rectangle_b,
                    },
                );
                assert!(editor.draft.is_none());
            } else {
                let step_b = MkStep {
                    id: 2,
                    action: MkAction::Delay { milliseconds: 77 },
                    ..step_a.clone()
                };
                editor.begin_edit(&step_b);
                let current = editor.draft.clone();
                editor.apply_visual_capture_outcome(
                    Some(17),
                    super::super::visual_capture_workflow::WorkflowOutcome::Region {
                        token: old_token,
                        rect: rectangle_b,
                    },
                );
                assert_eq!(editor.draft, current);
                assert_eq!(
                    step_a.action,
                    screenshot_action(SearchRegion::Rectangle { rect: rectangle_a })
                );
            }
            assert!(picker.lock().unwrap().cancelled.contains(&old_id));
        }
    }

    #[test]
    fn screenshot_rectangle_completion_is_typed_dirty_and_single_use() {
        let mut editor = test_editor();
        editor.begin_new(screenshot_action(SearchRegion::Desktop));
        let pending = PendingVisualRegionOperation {
            destination: VisualRegionDestination::CaptureScreenshotRegion,
            macro_id: 17,
            step_id: None,
            draft_generation: editor.draft_generation,
            expected_action: ExpectedVisualAction::CaptureScreenshot,
        };
        editor.pending_visual_region = Some(pending.clone());
        let token = super::super::visual_capture_workflow::DraftToken {
            macro_id: 17,
            draft_generation: editor.draft_generation,
        };
        let rect = ScreenRect::new(-1920, -80, 3840, 1080);
        editor.apply_visual_capture_outcome(
            Some(17),
            super::super::visual_capture_workflow::WorkflowOutcome::Region {
                token: token.clone(),
                rect,
            },
        );
        assert!(editor.draft_changed);
        assert!(matches!(
            &editor.draft.as_ref().unwrap().action,
            MkAction::CaptureScreenshot(p) if p.region == (SearchRegion::Rectangle { rect })
        ));

        // A duplicate completion has no pending ownership and cannot overwrite the payload.
        editor.apply_visual_capture_outcome(
            Some(17),
            super::super::visual_capture_workflow::WorkflowOutcome::Region {
                token,
                rect: ScreenRect::new(1, 2, 3, 4),
            },
        );
        assert!(matches!(
            &editor.draft.as_ref().unwrap().action,
            MkAction::CaptureScreenshot(p) if p.region == (SearchRegion::Rectangle { rect })
        ));

        // Even a matching snapshot is incompatible after the action variant changes.
        editor.pending_visual_region = Some(pending);
        editor.draft.as_mut().unwrap().action = image_action();
        editor.apply_visual_capture_outcome(
            Some(17),
            super::super::visual_capture_workflow::WorkflowOutcome::Region {
                token: super::super::visual_capture_workflow::DraftToken {
                    macro_id: 17,
                    draft_generation: editor.draft_generation,
                },
                rect: ScreenRect::new(-5, -6, 7, 8),
            },
        );
        assert!(matches!(
            editor.draft.as_ref().unwrap().action,
            MkAction::ImageFind(_)
        ));
    }

    fn shared_dialog() -> (
        tempfile::TempDir,
        MkMacroDialog,
        super::super::visual_capture_workflow::TestOverlayServiceFixture,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let fixture =
            super::super::visual_capture_workflow::SharedVisualOverlayController::test_fixture();
        let mut dialog = MkMacroDialog::new(Arc::new(store));
        dialog.visual_overlay = fixture.controller.clone();
        dialog.action_editor = ActionEditorState::new(dialog.visual_overlay.clone());
        dialog.create_macro();
        (dir, dialog, fixture)
    }

    fn begin_owned_preview(editor: &mut ActionEditorState) -> OperationId {
        editor.begin_new(image_action());
        editor.preview_region(SearchRegion::Rectangle {
            rect: ScreenRect::new(4, 5, 6, 7),
        });
        editor.visual_overlay.operation_id().unwrap()
    }

    fn assert_transaction_reuses_service(apply: bool) {
        let (_dir, mut dialog, fixture) = shared_dialog();
        let mut editor = ActionEditorState::new(dialog.visual_overlay_controller());
        let owned = begin_owned_preview(&mut editor);
        fixture.observer.wait_for_commands(1);
        if apply {
            assert!(editor.apply(&mut dialog).is_some());
        } else {
            editor.cancel();
        }
        fixture.observer.wait_for_commands(2);
        assert!(
            matches!(fixture.observer.commands.lock().unwrap()[1], VisualOverlayCommand::Cancel { expected_operation_id: Some(id) } if id == owned)
        );
        let fresh = dialog
            .visual_overlay_controller()
            .preview_rectangle(ScreenRect::new(8, 9, 10, 11));
        fixture.observer.wait_for_commands(3);
        assert!(
            matches!(fixture.observer.commands.lock().unwrap()[2], VisualOverlayCommand::PreviewRectangle { operation_id, .. } if operation_id == fresh)
        );
        assert_eq!(fixture.observer.joins.load(Ordering::SeqCst), 0);
        assert!(!dialog.visual_overlay_controller().poll().iter().any(|e| matches!(e, VisualOverlayEvent::Error { error, .. } if error.message.contains("shut down"))));
    }

    fn assert_plain_transaction_then_reference_capture(apply: bool, image: MkAction) {
        let (_dir, mut dialog, fixture) = shared_dialog();
        // This ordinary action never owns an overlay. On the pre-ownership-fix baseline,
        // applying/cancelling it dropped the service-owning editor and the assertion below
        // observed `visual overlay service is shut down` instead of BeginRectanglePick.
        dialog
            .action_editor
            .begin_new(MkAction::Delay { milliseconds: 5 });
        let mut plain = dialog.take_action_editor();
        if apply {
            assert!(plain.apply(&mut dialog).is_some());
        } else {
            plain.cancel();
        }
        dialog.action_editor = plain;

        dialog.action_editor.begin_new(image);
        install_real_service_workflow(&mut dialog.action_editor);
        let macro_id = dialog.selected_macro_id.unwrap();
        dialog
            .action_editor
            .request_rectangle_selection(
                macro_id,
                RectanglePurpose::ReferenceImageCapture,
                VisualRegionDestination::ImageActionReferenceAsset,
            )
            .unwrap();
        let operation_id = dialog
            .action_editor
            .visual_capture
            .as_ref()
            .and_then(|w| match w.state() {
                super::super::visual_capture_workflow::WorkflowState::Selecting {
                    operation_id,
                    ..
                } => Some(*operation_id),
                _ => None,
            })
            .unwrap();
        fixture.observer.wait_for_commands(1);
        assert!(matches!(fixture.observer.commands.lock().unwrap().last(),
            Some(VisualOverlayCommand::BeginRectanglePick { operation_id: id, purpose: RectanglePurpose::ReferenceImageCapture, .. }) if *id == operation_id));
        assert_ne!(operation_id, 0);
        assert_eq!(fixture.observer.starts.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.observer.shutdowns.load(Ordering::SeqCst), 0);
        assert_eq!(fixture.observer.joins.load(Ordering::SeqCst), 0);
        assert!(!dialog.visual_overlay_controller().poll().iter().any(|event|
            matches!(event, VisualOverlayEvent::Error { error, .. } if error.message.contains("visual overlay service is shut down"))));

        let original = dialog.action_editor.draft.clone();
        fixture.observer.cancel_rectangle(operation_id);
        for _ in 0..100 {
            dialog.action_editor.tick_visual_capture(Some(macro_id));
            if !dialog
                .action_editor
                .visual_capture
                .as_ref()
                .unwrap()
                .active()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(dialog.action_editor.draft, original);
        assert_eq!(fixture.observer.starts.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.observer.shutdowns.load(Ordering::SeqCst), 0);
        assert_eq!(fixture.observer.joins.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn apply_then_capture_uses_the_original_dialog_worker() {
        assert_plain_transaction_then_reference_capture(true, image_action());
    }

    #[test]
    fn cancel_then_capture_uses_the_original_dialog_worker() {
        let MkAction::ImageFind(payload) = image_action() else {
            unreachable!()
        };
        assert_plain_transaction_then_reference_capture(false, MkAction::ImageClick(payload));
    }

    #[test]
    fn cancel_cleans_up_only_its_operation_and_leaves_dialog_service_reusable() {
        assert_transaction_reuses_service(false);
    }

    #[test]
    fn apply_cleans_up_only_its_operation_and_leaves_dialog_service_reusable() {
        assert_transaction_reuses_service(true);
    }

    #[test]
    fn dropping_editor_cancels_at_most_once_and_final_dialog_handle_owns_join() {
        let (_dir, dialog, fixture) = shared_dialog();
        let mut editor = ActionEditorState::new(dialog.visual_overlay_controller());
        let owned = begin_owned_preview(&mut editor);
        fixture.observer.wait_for_commands(1);
        drop(editor);
        fixture.observer.wait_for_commands(2);
        let cancel_count = fixture.observer.commands.lock().unwrap().iter().filter(|c| matches!(c, VisualOverlayCommand::Cancel { expected_operation_id: Some(id) } if *id == owned)).count();
        assert_eq!(cancel_count, 1);
        dialog
            .visual_overlay_controller()
            .preview_rectangle(ScreenRect::new(9, 9, 2, 2));
        fixture.observer.wait_for_commands(3);
        assert_eq!(fixture.observer.joins.load(Ordering::SeqCst), 0);
        drop(dialog);
        drop(fixture.controller);
        assert_eq!(fixture.observer.joins.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn repeated_action_transactions_never_poison_shared_overlay_client() {
        let (_dir, mut dialog, fixture) = shared_dialog();
        for (index, apply) in [true, false, true, false].into_iter().enumerate() {
            let mut editor = ActionEditorState::new(dialog.visual_overlay_controller());
            let _ = begin_owned_preview(&mut editor);
            if index == 2 {
                editor.begin_edit(&MkStep {
                    id: 55,
                    enabled: true,
                    repeat: 1,
                    delay_after_ms: 0,
                    on_error: Default::default(),
                    action: MkAction::ImageClick(match image_action() {
                        MkAction::ImageFind(p) => p,
                        _ => unreachable!(),
                    }),
                });
            }
            if apply {
                let _ = editor.apply(&mut dialog);
            } else {
                editor.cancel();
            }
            let id = dialog
                .visual_overlay_controller()
                .preview_rectangle(ScreenRect::new(index as i32, 0, 2, 2));
            fixture.observer.wait_for_commands((index + 1) * 3);
            assert_eq!(dialog.visual_overlay_controller().operation_id(), Some(id));
            assert_eq!(fixture.observer.joins.load(Ordering::SeqCst), 0);
        }
        assert_eq!(fixture.observer.starts.load(Ordering::SeqCst), 1);
    }

    #[derive(Debug, PartialEq)]
    enum PreviewCall {
        Desktop(Vec<MonitorDescriptor>),
        Monitor(MonitorDescriptor),
        Rectangle(ScreenRect),
        Window(ScreenRect, super::super::visual_overlay::WindowAreaKind),
    }
    #[derive(Default)]
    struct FakePreview(Mutex<Vec<PreviewCall>>);
    impl RegionPreviewBoundary for FakePreview {
        fn preview_desktop(&self, v: Vec<MonitorDescriptor>) -> u64 {
            self.0.lock().unwrap().push(PreviewCall::Desktop(v));
            1
        }
        fn highlight_monitor(&self, v: MonitorDescriptor) -> u64 {
            self.0.lock().unwrap().push(PreviewCall::Monitor(v));
            2
        }
        fn preview_rectangle(&self, v: ScreenRect) -> u64 {
            self.0.lock().unwrap().push(PreviewCall::Rectangle(v));
            3
        }
        fn highlight_window(
            &self,
            r: ScreenRect,
            k: super::super::visual_overlay::WindowAreaKind,
        ) -> u64 {
            self.0.lock().unwrap().push(PreviewCall::Window(r, k));
            4
        }
    }
    fn monitor(index: usize, rect: ScreenRect) -> MonitorDescriptor {
        MonitorDescriptor {
            index,
            bounds: rect,
            primary: index == 0,
        }
    }
    fn resolve(rect: ScreenRect) -> impl Fn(&MkWindowMatcher, bool) -> ExecResult<ScreenRect> {
        move |_, _| Ok(rect)
    }

    #[test]
    fn region_preview_dispatches_each_authored_variant_without_desktop_union() {
        use super::super::visual_overlay::WindowAreaKind;
        let monitors = Ok(vec![
            monitor(0, ScreenRect::new(-100, 0, 100, 80)),
            monitor(7, ScreenRect::new(50, 20, 70, 60)),
        ]);
        let fake = FakePreview::default();
        let outer = ScreenRect::new(-701, -503, 411, 307);
        let client = ScreenRect::new(-680, -460, 350, 220);
        let resolver =
            |_: &MkWindowMatcher, client_area: bool| Ok(if client_area { client } else { outer });
        let (desktop_id, _) =
            dispatch_region_preview(&SearchRegion::Desktop, &monitors, &resolver, &fake).unwrap();
        let (monitor_id, _) = dispatch_region_preview(
            &SearchRegion::Monitor { index: 7 },
            &monitors,
            &resolver,
            &fake,
        )
        .unwrap();
        let signed = ScreenRect::new(-42, -9, 31, 27);
        let (rectangle_id, _) = dispatch_region_preview(
            &SearchRegion::Rectangle { rect: signed },
            &monitors,
            &resolver,
            &fake,
        )
        .unwrap();
        let (window_id, _) = dispatch_region_preview(
            &SearchRegion::Window {
                matcher: Default::default(),
            },
            &monitors,
            &resolver,
            &fake,
        )
        .unwrap();
        let (client_id, _) = dispatch_region_preview(
            &SearchRegion::ClientArea {
                matcher: Default::default(),
            },
            &monitors,
            &resolver,
            &fake,
        )
        .unwrap();
        assert_eq!(
            [desktop_id, monitor_id, rectangle_id, window_id, client_id],
            [1, 2, 3, 4, 4]
        );
        assert_eq!(
            *fake.0.lock().unwrap(),
            vec![
                PreviewCall::Desktop(monitors.unwrap()),
                PreviewCall::Monitor(monitor(7, ScreenRect::new(50, 20, 70, 60))),
                PreviewCall::Rectangle(signed),
                PreviewCall::Window(outer, WindowAreaKind::WholeWindow),
                PreviewCall::Window(client, WindowAreaKind::ClientArea)
            ]
        );
    }

    #[test]
    fn invalid_region_resolution_is_visible_and_never_starts_preview() {
        let cases: Vec<(
            SearchRegion,
            Result<Vec<MonitorDescriptor>, String>,
            Box<dyn Fn(&MkWindowMatcher, bool) -> ExecResult<ScreenRect>>,
            &str,
        )> = vec![
            (
                SearchRegion::Rectangle {
                    rect: ScreenRect::new(1, 2, 0, 4),
                },
                Ok(vec![]),
                Box::new(resolve(ScreenRect::new(0, 0, 1, 1))),
                "invalid",
            ),
            (
                SearchRegion::Desktop,
                Ok(vec![]),
                Box::new(resolve(ScreenRect::new(0, 0, 1, 1))),
                "No monitors",
            ),
            (
                SearchRegion::Desktop,
                Err("adapter offline".into()),
                Box::new(resolve(ScreenRect::new(0, 0, 1, 1))),
                "adapter offline",
            ),
            (
                SearchRegion::Monitor { index: 9 },
                Ok(vec![monitor(1, ScreenRect::new(0, 0, 1, 1))]),
                Box::new(resolve(ScreenRect::new(0, 0, 1, 1))),
                "no longer exists",
            ),
            (
                SearchRegion::Window {
                    matcher: Default::default(),
                },
                Ok(vec![]),
                Box::new(|_, _| {
                    Err(ExecutionDiagnostic::new(
                        DiagnosticKind::TargetNotFound,
                        "x",
                    ))
                }),
                "not found",
            ),
            (
                SearchRegion::ClientArea {
                    matcher: Default::default(),
                },
                Ok(vec![]),
                Box::new(|_, _| {
                    Err(ExecutionDiagnostic::new(
                        DiagnosticKind::AmbiguousTarget,
                        "x",
                    ))
                }),
                "ambiguous",
            ),
        ];
        for (region, monitors, resolver, expected) in cases {
            let fake = FakePreview::default();
            let error =
                dispatch_region_preview(&region, &monitors, resolver.as_ref(), &fake).unwrap_err();
            assert!(error.contains(expected), "{error:?}");
            assert!(fake.0.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn stale_overlay_error_cannot_replace_current_preview_status() {
        use super::super::visual_overlay::{
            OverlayErrorKind, VisualOverlayError, VisualOverlayEvent,
        };
        let current = Some((22, "Unable to preview current rectangle".into()));
        let mut message = None;
        apply_overlay_diagnostic(
            &mut message,
            &current,
            VisualOverlayEvent::Error {
                operation_id: 21,
                error: VisualOverlayError {
                    kind: OverlayErrorKind::Platform,
                    message: "old failure".into(),
                },
            },
        );
        assert_eq!(message, None);
        apply_overlay_diagnostic(
            &mut message,
            &current,
            VisualOverlayEvent::Error {
                operation_id: 22,
                error: VisualOverlayError {
                    kind: OverlayErrorKind::Platform,
                    message: "current failure".into(),
                },
            },
        );
        assert_eq!(
            message.as_deref(),
            Some("Unable to preview current rectangle: current failure")
        );
    }

    #[test]
    fn every_typed_rectangle_request_has_one_explicit_purpose() {
        use super::super::condition_editor::ConditionImageOperation;
        use super::super::visual_overlay::RectanglePurpose;

        assert_eq!(
            ImageAuthoringRequest::CaptureRectangle.rectangle_purpose(),
            Some(RectanglePurpose::ReferenceImageCapture)
        );
        assert_eq!(
            ImageAuthoringRequest::PickRectangle.rectangle_purpose(),
            Some(RectanglePurpose::SearchRegion)
        );
        assert_eq!(ImageAuthoringRequest::Import.rectangle_purpose(), None);
        assert_eq!(
            ConditionImageOperation::CaptureRectangle.rectangle_purpose(),
            Some(RectanglePurpose::ReferenceImageCapture)
        );
        assert_eq!(
            ConditionImageOperation::PickRectangle.rectangle_purpose(),
            Some(RectanglePurpose::SearchRegion)
        );
        for operation in [
            ConditionImageOperation::ImportPng,
            ConditionImageOperation::PreviewRectangle,
            ConditionImageOperation::HighlightMonitor,
            ConditionImageOperation::PickWindow,
            ConditionImageOperation::HighlightWindow,
        ] {
            assert_eq!(operation.rectangle_purpose(), None);
        }
    }
    use image::{GenericImageView, Rgba};
    use std::sync::mpsc;

    #[derive(Default)]
    struct HeldExecutor(std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>);
    impl super::super::image_authoring_job::ImageAuthoringExecutor for HeldExecutor {
        fn execute(&self, work: Box<dyn FnOnce() + Send>) {
            assert!(self.0.lock().unwrap().replace(work).is_none());
        }
    }
    impl HeldExecutor {
        fn release(&self) {
            self.0.lock().unwrap().take().unwrap()();
        }
    }
    fn step(a: MkAction) -> MkStep {
        MkStep {
            id: 7,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action: a,
        }
    }

    fn condition_search(asset_id: u64) -> MkCondition {
        MkCondition::ImageSearch {
            search: MkImageSearchCondition {
                asset_id,
                region: SearchRegion::Rectangle {
                    rect: ScreenRect::new(11, 12, 130, 140),
                },
                tolerance: 17,
                alpha: AlphaPolicy::Ignore,
                return_point: ReturnPoint::TopLeft,
            },
            found: false,
        }
    }

    fn condition_import_dialog() -> (tempfile::TempDir, MkMacroDialog, HeldExecutor) {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        // Reserve IDs 1..=8 in storage so the real importer deterministically returns 9.
        for id in 1..=8 {
            store
                .write_png_asset(
                    4,
                    id,
                    &RgbaImage::from_pixel(1, 1, Rgba([id as u8, 0, 0, 255])),
                )
                .unwrap();
        }
        let source = dir.path().join("condition.png");
        RgbaImage::from_pixel(2, 2, Rgba([9, 8, 7, 255]))
            .save(&source)
            .unwrap();
        let mut dialog = MkMacroDialog::new(Arc::new(store));
        let wait = MkWaitOptions {
            timeout_ms: 12_345,
            poll_interval_ms: 321,
        };
        let selected = step(MkAction::WaitUntil {
            condition: condition_search(4),
            wait,
        });
        dialog.draft.macros = vec![
            MkMacro {
                id: 4,
                name: "Current".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                playback: Default::default(),
                steps: vec![selected.clone()],
                image_assets: vec![asset(4, "Original", "mkmacro_assets/4/4.png")],
            },
            MkMacro {
                id: 40,
                name: "Other".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                playback: Default::default(),
                steps: vec![],
                image_assets: vec![asset(77, "Other", "mkmacro_assets/40/77.png")],
            },
        ];
        dialog.set_selected_macro(Some(4));
        dialog.action_editor.begin_edit(&selected);
        let request = super::super::condition_editor::ConditionImageRequest {
            path: super::super::image_authoring_destination::ConditionPath::root(),
            operation: super::super::condition_editor::ConditionImageOperation::ImportPng,
        };
        let destination = dialog.action_editor.condition_destination(4, request);
        let executor = HeldExecutor::default();
        dialog
            .action_editor
            .start_condition_import_from_selected_path_with_executor(
                dialog.store.clone(),
                destination.clone(),
                source,
                &executor,
            )
            .unwrap();
        let super::super::image_authoring_job::ImageAuthoringJob::Importing {
            token,
            destination: actual,
            previous_asset_id,
            ..
        } = &dialog.action_editor.image_authoring
        else {
            panic!("condition import did not start")
        };
        assert_eq!(token.macro_id, 4);
        assert_eq!(token.draft_generation, destination.draft_generation);
        assert_eq!(previous_asset_id, &4);
        assert_eq!(
            actual,
            &super::super::image_authoring_job::ImageAuthoringDestination::ConditionImage(
                destination
            )
        );
        (dir, dialog, executor)
    }

    #[test]
    fn condition_png_completion_is_transactional_integrated_and_single_use() {
        let (_dir, mut dialog, executor) = condition_import_dialog();
        let original = dialog.action_editor.draft.clone().unwrap();
        let other_assets = dialog.draft.macros[1].image_assets.clone();
        executor.release();
        reduce_image_authoring_completion(&mut dialog);

        let edited = dialog.action_editor.draft.as_ref().unwrap();
        let (condition, wait) = match &edited.action {
            MkAction::WaitUntil { condition, wait } => (condition, wait),
            other => panic!("selected step changed action: {other:?}"),
        };
        let MkCondition::ImageSearch { search, found } = condition else {
            panic!("condition type changed")
        };
        assert_eq!(search.asset_id, 9);
        assert!(!found);
        let MkAction::WaitUntil {
            condition: original_condition,
            wait: original_wait,
        } = &original.action
        else {
            unreachable!()
        };
        assert_eq!(wait, original_wait);
        let MkCondition::ImageSearch {
            search: original_search,
            found: original_found,
        } = original_condition
        else {
            unreachable!()
        };
        assert_eq!(search.region, original_search.region);
        assert_eq!(search.tolerance, original_search.tolerance);
        assert_eq!(search.alpha, original_search.alpha);
        assert_eq!(search.return_point, original_search.return_point);
        assert_eq!(found, original_found);
        let current = dialog.selected_macro().unwrap();
        assert_eq!(current.image_assets.iter().filter(|a| a.id == 9).count(), 1);
        assert_eq!(dialog.draft.macros[1].image_assets, other_assets);
        assert_eq!(
            super::super::image_asset_picker::filtered_assets(&current.image_assets, "9")
                .iter()
                .map(|asset| asset.id)
                .collect::<Vec<_>>(),
            vec![9]
        );

        // Polling the consumed operation again cannot duplicate it or overwrite a later edit.
        if let MkAction::WaitUntil { condition, .. } =
            &mut dialog.action_editor.draft.as_mut().unwrap().action
        {
            *condition = MkCondition::WindowActive {
                matcher: MkWindowMatcher::default(),
            };
        }
        reduce_image_authoring_completion(&mut dialog);
        assert!(matches!(
            dialog.action_editor.draft.as_ref().unwrap().action,
            MkAction::WaitUntil {
                condition: MkCondition::WindowActive { .. },
                ..
            }
        ));
        assert_eq!(
            dialog
                .selected_macro()
                .unwrap()
                .image_assets
                .iter()
                .filter(|a| a.id == 9)
                .count(),
            1
        );
    }

    #[test]
    fn condition_png_completion_rejects_changed_condition_step_and_closed_editor() {
        let (_dir, mut dialog, executor) = condition_import_dialog();
        if let MkAction::WaitUntil { condition, .. } =
            &mut dialog.action_editor.draft.as_mut().unwrap().action
        {
            *condition = MkCondition::WindowExists {
                matcher: MkWindowMatcher {
                    title: Some("replacement".into()),
                    ..Default::default()
                },
            };
        }
        executor.release();
        reduce_image_authoring_completion(&mut dialog);
        assert!(matches!(
            dialog.action_editor.draft.as_ref().unwrap().action,
            MkAction::WaitUntil {
                condition: MkCondition::WindowExists { .. },
                ..
            }
        ));
        assert!(
            !dialog
                .selected_macro()
                .unwrap()
                .image_assets
                .iter()
                .any(|a| a.id == 9)
        );

        let (_dir, mut dialog, executor) = condition_import_dialog();
        let step_b = step(MkAction::Delay { milliseconds: 88 });
        dialog.action_editor.begin_edit(&step_b);
        executor.release();
        reduce_image_authoring_completion(&mut dialog);
        assert!(matches!(
            dialog.action_editor.draft.as_ref().unwrap().action,
            MkAction::Delay { milliseconds: 88 }
        ));
        assert!(matches!(
            dialog.selected_macro().unwrap().steps[0].action,
            MkAction::WaitUntil { .. }
        ));
        assert!(
            !dialog
                .selected_macro()
                .unwrap()
                .image_assets
                .iter()
                .any(|a| a.id == 9)
        );

        let (_dir, mut dialog, executor) = condition_import_dialog();
        dialog.action_editor.cancel();
        executor.release();
        reduce_image_authoring_completion(&mut dialog);
        assert!(dialog.action_editor.draft.is_none());
        assert!(
            !dialog
                .selected_macro()
                .unwrap()
                .image_assets
                .iter()
                .any(|a| a.id == 9)
        );
    }

    #[test]
    fn failed_condition_png_completion_preserves_asset_and_reports_error() {
        let (_dir, mut dialog, executor) = condition_import_dialog();
        let source = match &mut dialog.action_editor.image_authoring {
            super::super::image_authoring_job::ImageAuthoringJob::Importing { source, .. } => {
                source
            }
            _ => unreachable!(),
        };
        std::fs::write(source, b"representative corrupt PNG").unwrap();
        executor.release();
        reduce_image_authoring_completion(&mut dialog);
        let MkAction::WaitUntil {
            condition: MkCondition::ImageSearch { search, .. },
            ..
        } = &dialog.action_editor.draft.as_ref().unwrap().action
        else {
            unreachable!()
        };
        assert_eq!(search.asset_id, 4);
        assert_eq!(dialog.selected_macro().unwrap().image_assets.len(), 1);
        assert!(
            dialog
                .action_editor
                .capture_message
                .as_deref()
                .unwrap()
                .contains("Reference image")
        );
    }

    fn pending_wait(editor: &ActionEditorState, macro_id: u64) -> PendingVisualRegionOperation {
        PendingVisualRegionOperation {
            destination: VisualRegionDestination::WaitForVisualChangeRegion,
            macro_id,
            step_id: editor.editing_id,
            draft_generation: editor.draft_generation,
            expected_action: ExpectedVisualAction::WaitForVisualChange,
        }
    }

    #[test]
    fn wait_visual_change_region_results_are_transactional_and_stale_safe() {
        use super::super::visual_capture_workflow::{DraftToken, WorkflowOutcome};
        let a = ScreenRect::new(1, 2, 30, 40);
        let b = ScreenRect::new(50, 60, 70, 80);
        let mut editor = test_editor();
        let mut payload = WaitForVisualChange::default();
        payload.region = SearchRegion::Rectangle { rect: a };
        editor.begin_edit(&step(MkAction::WaitForVisualChange(payload)));
        let token = DraftToken {
            macro_id: 11,
            draft_generation: editor.draft_generation,
        };

        editor.pending_visual_region = Some(pending_wait(&editor, 11));
        editor.apply_visual_capture_outcome(Some(11), WorkflowOutcome::Cancelled);
        assert_eq!(editor.image_search.as_ref().unwrap().rectangle, a);

        editor.pending_visual_region = Some(pending_wait(&editor, 11));
        editor.apply_visual_capture_outcome(Some(11), WorkflowOutcome::Region { token, rect: b });
        assert_eq!(editor.image_search.as_ref().unwrap().rectangle, b);
        assert!(editor.draft_changed);
        assert!(matches!(
            &editor.draft.as_ref().unwrap().action,
            MkAction::WaitForVisualChange(p) if p.region == SearchRegion::Rectangle { rect: b }
        ));

        // No explicit destination means no image-editor fallback.
        editor.apply_visual_capture_outcome(Some(11), WorkflowOutcome::Region { token, rect: a });
        assert_eq!(editor.image_search.as_ref().unwrap().rectangle, b);

        editor.pending_visual_region = Some(pending_wait(&editor, 11));
        editor.cancel();
        editor.apply_visual_capture_outcome(Some(11), WorkflowOutcome::Region { token, rect: a });
        assert!(editor.draft.is_none());
    }

    #[test]
    fn wait_visual_change_rejects_wrong_identity_generation_and_action_shape() {
        use super::super::visual_capture_workflow::{DraftToken, WorkflowOutcome};
        let original = ScreenRect::new(1, 2, 3, 4);
        let selected = ScreenRect::new(8, 9, 10, 11);
        let mut editor = test_editor();
        let mut payload = WaitForVisualChange::default();
        payload.region = SearchRegion::Rectangle { rect: original };
        editor.begin_edit(&step(MkAction::WaitForVisualChange(payload.clone())));
        let generation = editor.draft_generation;
        for (current_macro, token_macro, token_generation) in [
            (Some(12), 11, generation),
            (Some(11), 12, generation),
            (Some(11), 11, generation.wrapping_add(1)),
        ] {
            editor.pending_visual_region = Some(pending_wait(&editor, 11));
            editor.apply_visual_capture_outcome(
                current_macro,
                WorkflowOutcome::Region {
                    token: DraftToken {
                        macro_id: token_macro,
                        draft_generation: token_generation,
                    },
                    rect: selected,
                },
            );
            assert_eq!(editor.image_search.as_ref().unwrap().rectangle, original);
        }
        editor.pending_visual_region = Some(pending_wait(&editor, 11));
        editor.draft.as_mut().unwrap().action = MkAction::Text(MkTextPayload {
            text: "replacement".into(),
            mode: MkTextMode::Type,
        });
        editor.apply_visual_capture_outcome(
            Some(11),
            WorkflowOutcome::Region {
                token: DraftToken {
                    macro_id: 11,
                    draft_generation: generation,
                },
                rect: selected,
            },
        );
        assert!(matches!(
            editor.draft.as_ref().unwrap().action,
            MkAction::Text(_)
        ));
    }

    fn asset(id: u64, name: &str, path: &str) -> MkImageAsset {
        MkImageAsset {
            id,
            name: name.into(),
            relative_path: path.into(),
        }
    }

    #[test]
    fn asset_labels_cover_names_paths_duplicates_missing_and_id_fallback() {
        assert_eq!(
            image_asset_label(10, &[asset(10, "Save Button", "images/save_button.png")]),
            "Save Button · save_button.png · ID 10"
        );
        assert_eq!(
            image_asset_label(10, &[asset(10, "", "images/save_button.png")]),
            "save_button.png · ID 10"
        );
        assert_eq!(
            image_asset_label(
                10,
                &[asset(10, "save_button.png", "images/save_button.png")]
            ),
            "save_button.png · ID 10"
        );
        assert_eq!(image_asset_label(44, &[]), "Missing asset · ID 44");
        assert_eq!(
            image_asset_label(10, &[asset(10, "", "")]),
            "Image asset · ID 10"
        );
    }

    #[test]
    fn selecting_asset_preserves_offset_and_image_kind_uses_first_asset() {
        let mut target = MkCoordinateTarget::Image {
            asset_id: 5,
            offset: MkPoint { x: 12, y: -7 },
        };
        if let MkCoordinateTarget::Image { asset_id, .. } = &mut target {
            select_image_asset(asset_id, 10);
        }
        assert_eq!(
            target,
            MkCoordinateTarget::Image {
                asset_id: 10,
                offset: MkPoint { x: 12, y: -7 }
            }
        );
        change_target_kind(&mut target, 4, &[asset(22, "", "22.png")]);
        assert_eq!(
            target,
            MkCoordinateTarget::Image {
                asset_id: 22,
                offset: MkPoint { x: 0, y: 0 }
            }
        );
    }

    #[test]
    fn shortcut_region_and_payload_helpers_are_exact() {
        let matcher = MkWindowMatcher {
            title: Some("Title".into()),
            process: Some("app.exe".into()),
            title_regex: Some("T.*".into()),
            class: Some("Class".into()),
        };
        for region in [
            SearchRegion::Window {
                matcher: matcher.clone(),
            },
            SearchRegion::ClientArea {
                matcher: matcher.clone(),
            },
        ] {
            assert_eq!(activation_matcher(&region), Some(matcher.clone()));
        }
        assert!(activation_matcher(&SearchRegion::Desktop).is_none());
        assert!(activation_matcher(&SearchRegion::Monitor { index: 1 }).is_none());
        assert!(
            activation_matcher(&SearchRegion::Rectangle {
                rect: ScreenRect::new(0, 0, 1, 1)
            })
            .is_none()
        );
    }

    #[test]
    fn image_asset_updates_are_transactional() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let mut payload = MkImagePayload {
            asset_id: 9,
            wait: MkWaitOptions {
                timeout_ms: 5_000,
                poll_interval_ms: 100,
            },
            region: SearchRegion::Desktop,
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
            not_found_policy: MkImageNotFoundPolicy::Fail,
            outputs: MkImageOutputs::default(),
        };
        let bad = dir.path().join("bad.png");
        std::fs::write(&bad, b"not png").unwrap();
        assert!(
            ImageAssetAuthoringService::new(&store)
                .import_png(4, &bad)
                .is_err()
        );
        assert_eq!(payload.asset_id, 9);

        let image = RgbaImage::from_pixel(3, 2, Rgba([1, 2, 3, 255]));
        apply_capture(&store, 4, &mut payload, &image).unwrap();
        assert_eq!(payload.asset_id, 1);
        assert_eq!(
            image::open(store.asset_path(4, 1).unwrap())
                .unwrap()
                .width(),
            3
        );
    }

    fn image_payload(asset_id: u64, region: SearchRegion) -> MkImagePayload {
        MkImagePayload {
            asset_id,
            wait: MkWaitOptions {
                timeout_ms: 5_000,
                poll_interval_ms: 100,
            },
            region,
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
            not_found_policy: MkImageNotFoundPolicy::Fail,
            outputs: MkImageOutputs::default(),
        }
    }

    #[test]
    fn selected_path_import_is_pending_then_atomically_applied() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let store = std::sync::Arc::new(store);
        store
            .write_png_asset(4, 1, &RgbaImage::from_pixel(1, 1, Rgba([9, 9, 9, 255])))
            .unwrap();
        let source = dir.path().join("selected.png");
        RgbaImage::from_pixel(2, 3, Rgba([1, 2, 3, 255]))
            .save(&source)
            .unwrap();
        let region = SearchRegion::Rectangle {
            rect: ScreenRect::new(3, 4, 5, 6),
        };
        let mut editor = test_editor();
        editor.draft_generation = 7;
        editor.draft = Some(step(MkAction::ImageFind(image_payload(1, region.clone()))));
        let executor = HeldExecutor::default();
        editor
            .start_import_from_selected_path_with_executor(store.clone(), 4, source, &executor)
            .unwrap();
        assert_eq!(draft_image_payload(&editor).asset_id, 1);
        editor.poll_image_authoring(Some(4));
        assert_eq!(draft_image_payload(&editor).asset_id, 1);
        executor.release();
        editor.poll_image_authoring(Some(4));
        assert_eq!(draft_image_payload(&editor).asset_id, 2);
        assert_eq!(draft_image_payload(&editor).region, region);
        assert_eq!(
            image::open(store.asset_path(4, 2).unwrap())
                .unwrap()
                .dimensions(),
            (2, 3)
        );
        assert!(!editor.image_authoring.is_importing());
    }

    #[test]
    fn corrupt_selected_path_preserves_payload_and_stages_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let store = std::sync::Arc::new(store);
        let source = dir.path().join("corrupt.png");
        std::fs::write(&source, b"not a png").unwrap();
        let region = SearchRegion::Rectangle {
            rect: ScreenRect::new(8, 9, 10, 11),
        };
        let mut editor = test_editor();
        editor.draft_generation = 1;
        editor.draft = Some(step(MkAction::ImageFind(image_payload(41, region.clone()))));
        let executor = HeldExecutor::default();
        editor
            .start_import_from_selected_path_with_executor(store.clone(), 4, source, &executor)
            .unwrap();
        executor.release();
        editor.poll_image_authoring(Some(4));
        assert_eq!(draft_image_payload(&editor).asset_id, 41);
        assert_eq!(draft_image_payload(&editor).region, region);
        assert!(
            editor
                .capture_message
                .as_deref()
                .unwrap()
                .contains("Reference image")
        );
        assert!(store.asset_ids(4).unwrap().is_empty());
        assert!(!editor.image_authoring.is_importing());
    }

    fn importing_editor(
        token: super::super::image_authoring_job::DraftToken,
        region: SearchRegion,
    ) -> (
        ActionEditorState,
        mpsc::Sender<super::super::image_authoring_job::ImageAuthoringCompletion>,
    ) {
        let mut editor = test_editor();
        editor.draft_generation = token.draft_generation;
        editor.draft = Some(step(MkAction::ImageFind(image_payload(9, region))));
        let (sender, completion) = mpsc::channel();
        editor.image_authoring = super::super::image_authoring_job::ImageAuthoringJob::Importing {
            token,
            destination:
                super::super::image_authoring_job::ImageAuthoringDestination::ImageActionReference,
            previous_asset_id: 9,
            source: "chosen.png".into(),
            completion,
        };
        (editor, sender)
    }

    #[test]
    fn image_import_completion_changes_only_asset_and_failure_preserves_draft() {
        use super::super::image_authoring_job::{DraftToken, ImageAuthoringCompletion};
        let token = DraftToken {
            macro_id: 4,
            draft_generation: 2,
        };
        let region = SearchRegion::Rectangle {
            rect: ScreenRect::new(1, 2, 30, 40),
        };
        let (mut editor, sender) = importing_editor(token, region.clone());
        sender
            .send(ImageAuthoringCompletion {
                token,
                destination: super::super::image_authoring_job::ImageAuthoringDestination::ImageActionReference,
                result: Ok(crate::mkmacro::StagedImageAsset {
                    asset_id: 10,
                    managed_reference: "mkmacro_assets/4/10.png".into(),
                }),
            })
            .unwrap();
        editor.poll_image_authoring(Some(4));
        let payload = editor.draft.as_mut().and_then(image_payload_mut).unwrap();
        assert_eq!(payload.asset_id, 10);
        assert_eq!(payload.region, region);
        assert!(!editor.image_authoring.is_importing());

        let (mut editor, sender) = importing_editor(token, region.clone());
        sender
            .send(ImageAuthoringCompletion {
                token,
                destination: super::super::image_authoring_job::ImageAuthoringDestination::ImageActionReference,
                result: Err("Reference image: corrupt PNG".into()),
            })
            .unwrap();
        editor.poll_image_authoring(Some(4));
        let payload = editor.draft.as_mut().and_then(image_payload_mut).unwrap();
        assert_eq!(payload.asset_id, 9);
        assert_eq!(payload.region, region);
        assert!(
            editor
                .capture_message
                .as_deref()
                .unwrap()
                .contains("corrupt")
        );
    }

    #[test]
    fn stale_image_import_completions_never_touch_another_draft() {
        use super::super::image_authoring_job::{DraftToken, ImageAuthoringCompletion};
        let token = DraftToken {
            macro_id: 4,
            draft_generation: 2,
        };
        for (macro_id, generation) in [(5, 2), (4, 3)] {
            let (mut editor, sender) = importing_editor(token, SearchRegion::Desktop);
            editor.draft_generation = generation;
            sender
                .send(ImageAuthoringCompletion {
                    token,
                    destination: super::super::image_authoring_job::ImageAuthoringDestination::ImageActionReference,
                    result: Ok(crate::mkmacro::StagedImageAsset {
                        asset_id: 55,
                        managed_reference: "unused.png".into(),
                    }),
                })
                .unwrap();
            editor.poll_image_authoring(Some(macro_id));
            assert_eq!(
                editor
                    .draft
                    .as_mut()
                    .and_then(image_payload_mut)
                    .unwrap()
                    .asset_id,
                9
            );
        }
        let (mut editor, sender) = importing_editor(token, SearchRegion::Desktop);
        editor.cancel();
        sender
            .send(ImageAuthoringCompletion {
                token,
                destination: super::super::image_authoring_job::ImageAuthoringDestination::ImageActionReference,
                result: Ok(crate::mkmacro::StagedImageAsset {
                    asset_id: 55,
                    managed_reference: "unused.png".into(),
                }),
            })
            .unwrap_err();
        editor.poll_image_authoring(Some(4));
        assert!(editor.draft.is_none());
    }

    fn draft_image_payload(editor: &ActionEditorState) -> &MkImagePayload {
        match &editor.draft.as_ref().unwrap().action {
            MkAction::ImageFind(payload) | MkAction::ImageClick(payload) => payload,
            _ => panic!("expected image action"),
        }
    }

    #[test]
    fn visual_results_mutate_only_their_purpose_specific_draft_state() {
        use super::super::image_search_editor::SearchRegionKind;
        use super::super::visual_capture_workflow::{DraftToken, WorkflowOutcome};

        let original_region = SearchRegion::Window {
            matcher: MkWindowMatcher {
                title: Some("unchanged".into()),
                ..Default::default()
            },
        };
        let mut editor = test_editor();
        editor.begin_edit(&step(MkAction::ImageFind(image_payload(
            4,
            original_region.clone(),
        ))));
        let token = DraftToken {
            macro_id: 11,
            draft_generation: editor.draft_generation,
        };
        let before_editor = format!("{:?}", editor.image_search.as_ref().unwrap());
        editor.pending_visual_region = Some(PendingVisualRegionOperation {
            destination: VisualRegionDestination::ImageActionReferenceAsset,
            macro_id: 11,
            step_id: editor.editing_id,
            draft_generation: editor.draft_generation,
            expected_action: ExpectedVisualAction::ImageFind,
        });
        editor
            .apply_visual_capture_outcome(Some(11), WorkflowOutcome::Asset { token, asset_id: 5 });
        assert_eq!(draft_image_payload(&editor).asset_id, 5);
        assert_eq!(
            format!("{:?}", editor.image_search.as_ref().unwrap()),
            before_editor
        );
        assert_eq!(draft_image_payload(&editor).region, original_region);

        let picked = ScreenRect::new(-123, 45, 67, 89);
        editor.pending_visual_region = Some(PendingVisualRegionOperation {
            destination: VisualRegionDestination::ImageActionSearchRegion,
            macro_id: 11,
            step_id: editor.editing_id,
            draft_generation: editor.draft_generation,
            expected_action: ExpectedVisualAction::ImageFind,
        });
        editor.apply_visual_capture_outcome(
            Some(11),
            WorkflowOutcome::Region {
                token,
                rect: picked,
            },
        );
        let image = editor.image_search.as_ref().unwrap();
        assert_eq!(image.rectangle, picked);
        assert_eq!(image.kind, SearchRegionKind::Rectangle);
        assert_eq!(draft_image_payload(&editor).asset_id, 5);
        assert_eq!(draft_image_payload(&editor).region, original_region);

        editor.sync_image_region_to_draft();
        assert_eq!(
            draft_image_payload(&editor).region,
            SearchRegion::Rectangle { rect: picked }
        );
    }

    #[test]
    fn stale_cancelled_and_failed_visual_results_preserve_the_draft() {
        use super::super::visual_capture_workflow::{DraftToken, WorkflowOutcome};

        let region = SearchRegion::Monitor { index: 2 };
        let mut editor = test_editor();
        editor.begin_edit(&step(MkAction::ImageClick(image_payload(4, region))));
        let generation = editor.draft_generation;
        let before_draft = serde_json::to_vec(editor.draft.as_ref().unwrap()).unwrap();
        let before_editor = format!("{:?}", editor.image_search.as_ref().unwrap());
        for outcome in [
            WorkflowOutcome::Asset {
                token: DraftToken {
                    macro_id: 12,
                    draft_generation: generation,
                },
                asset_id: 5,
            },
            WorkflowOutcome::Region {
                token: DraftToken {
                    macro_id: 11,
                    draft_generation: generation.wrapping_add(1),
                },
                rect: ScreenRect::new(1, 2, 3, 4),
            },
            WorkflowOutcome::Cancelled,
            WorkflowOutcome::Failed("PNG staging failed".into()),
        ] {
            editor.apply_visual_capture_outcome(Some(11), outcome);
            assert_eq!(
                serde_json::to_vec(editor.draft.as_ref().unwrap()).unwrap(),
                before_draft
            );
            assert_eq!(
                format!("{:?}", editor.image_search.as_ref().unwrap()),
                before_editor
            );
        }
        assert_eq!(draft_image_payload(&editor).asset_id, 4);
    }
    fn capture(slot: PositionCaptureSlot) -> PositionCaptureState {
        PositionCaptureState {
            target_step_id: Some(7),
            draft_generation: 1,
            slot,
            awaiting_release: true,
            last_screen_position: None,
            #[cfg(windows)]
            foreground_window: 0,
        }
    }

    #[test]
    fn window_picker_routes_root_nested_and_image_matchers() {
        let replacement = MkWindowMatcher {
            process: Some("picked.exe".into()),
            title: Some("Picked".into()),
            title_regex: None,
            class: None,
        };
        for (action, path) in [
            (
                MkAction::WindowClose(MkWindowMatcher::default()),
                super::super::window_picker::MatcherPath::Action,
            ),
            (
                MkAction::WhileStart {
                    condition: MkCondition::All {
                        conditions: vec![MkCondition::WindowExists {
                            matcher: MkWindowMatcher::default(),
                        }],
                    },
                },
                super::super::window_picker::MatcherPath::Condition(vec![0]),
            ),
            (
                MkAction::ImageFind(MkImagePayload {
                    asset_id: 1,
                    tolerance: 0,
                    alpha: AlphaPolicy::Compare,
                    region: SearchRegion::Window {
                        matcher: MkWindowMatcher::default(),
                    },
                    return_point: ReturnPoint::Center,
                    not_found_policy: MkImageNotFoundPolicy::Fail,
                    outputs: MkImageOutputs::default(),
                    wait: MkWaitOptions {
                        timeout_ms: 0,
                        poll_interval_ms: 1,
                    },
                }),
                super::super::window_picker::MatcherPath::VisualRegion,
            ),
        ] {
            let mut editor = test_editor();
            editor.begin_edit(&step(action));
            let request = super::super::window_picker::MatcherEditRequest {
                destination: super::super::window_picker::MatcherDestination::Action {
                    macro_id: 5,
                    draft_generation: editor.draft_generation,
                    path: path.clone(),
                },
                original: MkWindowMatcher::default(),
            };
            assert!(editor.apply_window_matcher(&request, replacement.clone(), Some(5)));
            if matches!(path, super::super::window_picker::MatcherPath::VisualRegion) {
                // Image regions intentionally remain in the UI-only per-mode
                // cache until Apply; picker results must not mutate the
                // serialized action draft behind the editor's back.
                assert_eq!(
                    editor.image_search.as_ref().unwrap().window_matcher,
                    replacement
                );
                assert!(matches!(
                    &editor.draft.as_ref().unwrap().action,
                    MkAction::ImageFind(p)
                        if matches!(&p.region, SearchRegion::Window { matcher } if matcher == &MkWindowMatcher::default())
                ));
            } else {
                assert_eq!(
                    matcher_at_path(&mut editor.draft.as_mut().unwrap().action, &path),
                    Some(&mut replacement.clone())
                );
            }
            assert!(
                !editor.apply_window_matcher(&request, MkWindowMatcher::default(), Some(6)),
                "another macro must not be modified"
            );
            let mut stale = request.clone();
            let super::super::window_picker::MatcherDestination::Action {
                draft_generation, ..
            } = &mut stale.destination;
            *draft_generation = draft_generation.wrapping_add(1);
            assert!(
                !editor.apply_window_matcher(&stale, MkWindowMatcher::default(), Some(5)),
                "a stale editor generation must not be modified"
            );
        }
    }

    #[test]
    fn both_image_actions_use_the_same_typed_image_editor_state() {
        let payload = MkImagePayload {
            asset_id: 4,
            tolerance: 12,
            alpha: AlphaPolicy::Ignore,
            region: SearchRegion::Rectangle {
                rect: ScreenRect::new(-10, 20, 30, 40),
            },
            return_point: ReturnPoint::TopLeft,
            not_found_policy: MkImageNotFoundPolicy::Fail,
            outputs: MkImageOutputs::default(),
            wait: MkWaitOptions {
                timeout_ms: 2_000,
                poll_interval_ms: 25,
            },
        };
        for action in [
            MkAction::ImageFind(payload.clone()),
            MkAction::ImageClick(payload.clone()),
        ] {
            let mut editor = test_editor();
            editor.begin_edit(&step(action));
            let image = editor.image_search.as_ref().expect("shared image editor");
            assert_eq!(
                image.kind,
                super::super::image_search_editor::SearchRegionKind::Rectangle
            );
            assert_eq!(image.rectangle, ScreenRect::new(-10, 20, 30, 40));
            assert_eq!(image.selected_region(), payload.region);
        }
    }

    #[test]
    fn condition_window_picker_routes_every_host_kind_and_preserves_siblings_on_apply_or_cancel() {
        fn matcher(title: &str) -> MkWindowMatcher {
            MkWindowMatcher {
                title: Some(title.into()),
                ..Default::default()
            }
        }
        for active in [false, true] {
            for host in ["if", "while", "wait"] {
                let target = if active {
                    MkCondition::WindowActive {
                        matcher: matcher("original"),
                    }
                } else {
                    MkCondition::WindowExists {
                        matcher: matcher("original"),
                    }
                };
                // Exercise all recursive containers and retain two sentinels to
                // prove MatcherPath::Condition mutates only the requested leaf.
                let condition = MkCondition::All {
                    conditions: vec![
                        MkCondition::WindowExists {
                            matcher: matcher("all-sibling"),
                        },
                        MkCondition::Any {
                            conditions: vec![
                                MkCondition::WindowActive {
                                    matcher: matcher("any-sibling"),
                                },
                                MkCondition::Not {
                                    condition: Box::new(target),
                                },
                            ],
                        },
                    ],
                };
                let picker_path =
                    super::super::condition_editor::first_window_picker_path(match &condition {
                        MkCondition::All { conditions } => &conditions[1],
                        _ => unreachable!(),
                    })
                    .unwrap();
                let mut full_path = vec![1];
                full_path.extend(picker_path);
                assert_eq!(full_path, vec![1, 0], "condition_ui recursive picker path");
                // Select the second Any child (the Not branch), exactly as a
                // picker request emitted from that rendered control does.
                let path = super::super::window_picker::MatcherPath::Condition(vec![1, 1, 0]);
                let action = match host {
                    "if" => MkAction::If(condition),
                    "while" => MkAction::WhileStart { condition },
                    _ => MkAction::WaitUntil {
                        condition,
                        wait: MkWaitOptions {
                            timeout_ms: 1000,
                            poll_interval_ms: 50,
                        },
                    },
                };
                let mut editor = test_editor();
                editor.begin_edit(&step(action));
                let before_cancel = editor.draft.as_ref().unwrap().action.clone();
                // Cancellation never calls apply_window_matcher.
                assert_eq!(
                    editor.draft.as_ref().unwrap().action,
                    before_cancel,
                    "{host}: picker cancellation"
                );
                let request = super::super::window_picker::MatcherEditRequest {
                    destination: super::super::window_picker::MatcherDestination::Action {
                        macro_id: 9,
                        draft_generation: editor.draft_generation,
                        path: path.clone(),
                    },
                    original: matcher("original"),
                };
                assert!(editor.apply_window_matcher(&request, matcher("chosen"), Some(9)));
                assert_eq!(
                    matcher_at_path(&mut editor.draft.as_mut().unwrap().action, &path)
                        .unwrap()
                        .title
                        .as_deref(),
                    Some("chosen")
                );
                let serialized =
                    serde_json::to_string(&editor.draft.as_ref().unwrap().action).unwrap();
                assert!(
                    serialized.contains("all-sibling") && serialized.contains("any-sibling"),
                    "{host}: sibling changed"
                );
            }
        }
    }

    #[test]
    fn capture_is_frame_driven_armed_and_one_shot() {
        let mut e = test_editor();
        e.begin_edit(&step(MkAction::MouseMove(MkMouseMovePayload {
            target: MkCoordinateTarget::Screen {
                point: MkPoint { x: 1, y: 2 },
            },
            duration_ms: 0,
        })));
        e.draft_generation = 1;
        e.position_capture = Some(capture(PositionCaptureSlot::MoveTarget));
        e.process_position_event(PositionCaptureEvent::PointerMoved(MkPoint {
            x: -300,
            y: 44,
        }));
        e.process_position_event(PositionCaptureEvent::LeftPressed);
        assert!(e.position_capture.is_some(), "initiating press is ignored");
        e.process_position_event(PositionCaptureEvent::LeftReleased);
        assert!(!e.position_capture.as_ref().unwrap().awaiting_release);
        e.process_position_event(PositionCaptureEvent::LeftPressed);
        assert!(e.position_capture.is_none());
        let MkAction::MouseMove(p) = &e.draft.as_ref().unwrap().action else {
            panic!()
        };
        assert_eq!(
            p.target,
            MkCoordinateTarget::Screen {
                point: MkPoint { x: -300, y: 44 }
            }
        );
        e.process_position_event(PositionCaptureEvent::LeftPressed);
        let MkAction::MouseMove(p) = &e.draft.as_ref().unwrap().action else {
            panic!()
        };
        assert_eq!(
            p.target,
            MkCoordinateTarget::Screen {
                point: MkPoint { x: -300, y: 44 }
            }
        );
    }

    #[test]
    fn escape_and_editor_lifecycle_clear_capture_without_mutation() {
        let source = step(MkAction::MouseClick(MkMousePayload {
            target: MkCoordinateTarget::Screen {
                point: MkPoint { x: 9, y: 8 },
            },
            button: MkMouseButton::Left,
            clicks: 1,
        }));
        let mut e = test_editor();
        e.begin_edit(&source);
        e.draft_generation = 1;
        e.position_capture = Some(capture(PositionCaptureSlot::ClickTarget));
        e.process_position_event(PositionCaptureEvent::Escape);
        assert!(e.position_capture.is_none());
        assert_eq!(e.draft.as_ref().unwrap().action, source.action);
        e.position_capture = Some(capture(PositionCaptureSlot::ClickTarget));
        e.cancel();
        assert!(e.position_capture.is_none());
    }

    #[test]
    fn desktop_color_pick_updates_only_after_confirmation_and_escape_preserves_draft() {
        let source = step(MkAction::PixelCheck {
            target: MkCoordinateTarget::Screen {
                point: MkPoint { x: -8, y: 4 },
            },
            color: "#112233".into(),
            tolerance: 0,
        });
        let mut editor = test_editor();
        editor.begin_edit(&source);
        editor.draft_generation = 1;
        let mut pending = capture(PositionCaptureSlot::PixelColor);
        pending.last_screen_position = Some(MkPoint { x: -8, y: 4 });
        editor.confirm_position_with(pending, |point| {
            assert_eq!(point, MkPoint { x: -8, y: 4 });
            Ok([0xab, 0, 0xff])
        });
        let MkAction::PixelCheck { color, .. } = &editor.draft.as_ref().unwrap().action else {
            panic!()
        };
        assert_eq!(color, "#AB00FF");

        editor.position_capture = Some(capture(PositionCaptureSlot::PixelColor));
        editor.process_position_event(PositionCaptureEvent::Escape);
        let MkAction::PixelCheck { color, .. } = &editor.draft.as_ref().unwrap().action else {
            panic!()
        };
        assert_eq!(color, "#AB00FF");
    }

    #[test]
    fn drag_slots_are_independent() {
        let drag = MkMouseDragPayload {
            from: MkCoordinateTarget::Screen {
                point: MkPoint { x: 1, y: 2 },
            },
            to: MkCoordinateTarget::Screen {
                point: MkPoint { x: 3, y: 4 },
            },
            button: MkMouseButton::X2,
            duration_ms: 10,
        };
        let mut e = test_editor();
        e.begin_edit(&step(MkAction::MouseDrag(drag)));
        e.draft_generation = 1;
        let mut c = capture(PositionCaptureSlot::DragFrom);
        c.awaiting_release = false;
        c.last_screen_position = Some(MkPoint { x: -5, y: -6 });
        e.position_capture = Some(c);
        e.process_position_event(PositionCaptureEvent::Enter);
        let MkAction::MouseDrag(p) = &e.draft.as_ref().unwrap().action else {
            panic!()
        };
        assert_eq!(
            p.from,
            MkCoordinateTarget::Screen {
                point: MkPoint { x: -5, y: -6 }
            }
        );
        assert_eq!(
            p.to,
            MkCoordinateTarget::Screen {
                point: MkPoint { x: 3, y: 4 }
            }
        );
    }
    #[test]
    fn capture_chooses_press_hotkey_down_and_up() {
        let mut e = test_editor();
        e.begin_edit(&step(MkAction::KeyPress(MkKey::Enter)));
        assert!(e.set_captured_keys(vec![MkKey::Character("A".into())]));
        assert!(matches!(
            e.draft.as_ref().unwrap().action,
            MkAction::KeyPress(_)
        ));
        e.set_captured_keys(vec![MkKey::Control, MkKey::Character("K".into())]);
        assert!(matches!(
            e.draft.as_ref().unwrap().action,
            MkAction::Hotkey(_)
        ));
        e.begin_edit(&step(MkAction::KeyDown(MkKey::Enter)));
        e.set_captured_keys(vec![MkKey::Control, MkKey::Character("Q".into())]);
        assert!(matches!(
            e.draft.as_ref().unwrap().action,
            MkAction::KeyDown(MkKey::Character(_))
        ));
        e.begin_edit(&step(MkAction::KeyUp(MkKey::Enter)));
        e.set_captured_keys(vec![MkKey::Character("Q".into())]);
        assert!(matches!(
            e.draft.as_ref().unwrap().action,
            MkAction::KeyUp(_)
        ));
    }
    #[test]
    fn cancel_does_not_touch_source() {
        let source = step(MkAction::Delay { milliseconds: 12 });
        let bytes = serde_json::to_vec(&source).unwrap();
        let mut e = test_editor();
        e.begin_edit(&source);
        if let Some(MkStep {
            action: MkAction::Delay { milliseconds },
            ..
        }) = e.draft.as_mut()
        {
            *milliseconds = 99
        }
        e.cancel();
        assert_eq!(serde_json::to_vec(&source).unwrap(), bytes);
    }
    #[test]
    fn screen_and_window_relative_position_are_pure() {
        let mut t = MkCoordinateTarget::Screen {
            point: MkPoint { x: 0, y: 0 },
        };
        apply_picked_position(&mut t, MkPoint { x: -20, y: 40 }, None).unwrap();
        assert_eq!(
            t,
            MkCoordinateTarget::Screen {
                point: MkPoint { x: -20, y: 40 }
            }
        );
        let w = PickedWindow {
            matcher: MkWindowMatcher {
                title: Some("Editor".into()),
                title_regex: None,
                process: Some("app.exe".into()),
                class: Some("Class".into()),
            },
            client_origin: MkPoint { x: -100, y: 10 },
        };
        let mut t = MkCoordinateTarget::ActiveWindow {
            point: MkPoint { x: 0, y: 0 },
        };
        apply_picked_position(&mut t, MkPoint { x: -20, y: 40 }, Some(&w)).unwrap();
        assert_eq!(
            t,
            MkCoordinateTarget::ActiveWindow {
                point: MkPoint { x: 80, y: 30 }
            }
        );
    }
    #[test]
    fn picked_window_stores_only_durable_matcher() {
        let mut p = MkWindowPayload {
            matcher: MkWindowMatcher {
                title: None,
                title_regex: None,
                process: None,
                class: None,
            },
            wait: None,
        };
        let picked = PickedWindow {
            matcher: MkWindowMatcher {
                title: Some("Document".into()),
                title_regex: None,
                process: Some("editor.exe".into()),
                class: Some("EditorWindow".into()),
            },
            client_origin: MkPoint { x: 1, y: 2 },
        };
        apply_picked_window(&mut p, &picked);
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("editor.exe"));
        assert!(!json.to_lowercase().contains("hwnd"));
    }

    /// Full-editor regressions for the independence of reference-image capture and
    /// search-region authoring. These deliberately install the real workflow in an
    /// `ActionEditorState`; workflow outcomes are never applied directly.
    mod visual_authoring_independence {
        use super::*;
        use crate::gui::mkmacro_dialog::{
            image_search_editor::{ImageSearchEditorState, SearchRegionKind},
            visual_capture_workflow::{
                AssetStoreAdapter, CaptureAdapter, RectangleOverlay, SelectionEvent,
                VisualCaptureWorkflow,
            },
            visual_overlay::{OperationId, RectanglePurpose},
        };
        use std::sync::{Arc, Mutex};

        const MACRO_ID: u64 = 73;
        const OPERATION_ID: OperationId = 19;
        const ORIGINAL: ScreenRect = ScreenRect {
            x: 100,
            y: 100,
            width: 800,
            height: 600,
        };

        #[derive(Default)]
        struct FakeState {
            purposes: Vec<RectanglePurpose>,
            selection: Option<SelectionEvent>,
            capture_result: Option<Result<RgbaImage, String>>,
            stage_result: Option<Result<u64, String>>,
            capture_rects: Vec<ScreenRect>,
            staged_dimensions: Vec<(u32, u32)>,
        }
        struct Overlay(Arc<Mutex<FakeState>>);
        impl RectangleOverlay for Overlay {
            fn begin(&mut self, purpose: RectanglePurpose) -> Result<OperationId, String> {
                self.0.lock().unwrap().purposes.push(purpose);
                Ok(OPERATION_ID)
            }
            fn poll(&mut self) -> SelectionEvent {
                self.0
                    .lock()
                    .unwrap()
                    .selection
                    .take()
                    .unwrap_or(SelectionEvent::Pending)
            }
            fn cancel(&mut self, _expected_operation_id: OperationId) {}
        }
        struct Capture(Arc<Mutex<FakeState>>);
        impl CaptureAdapter for Capture {
            fn capture_rect(&mut self, rect: ScreenRect) -> Result<RgbaImage, String> {
                let mut state = self.0.lock().unwrap();
                state.capture_rects.push(rect);
                state
                    .capture_result
                    .take()
                    .unwrap_or_else(|| Ok(RgbaImage::new(rect.width, rect.height)))
            }
        }
        struct Assets(Arc<Mutex<FakeState>>);
        impl AssetStoreAdapter for Assets {
            fn write_png_asset(&mut self, _: u64, image: &RgbaImage) -> Result<u64, String> {
                let mut state = self.0.lock().unwrap();
                state.staged_dimensions.push(image.dimensions());
                state.stage_result.take().unwrap_or(Ok(8))
            }
        }
        struct Fixture {
            editor: ActionEditorState,
            fake: Arc<Mutex<FakeState>>,
        }
        impl Fixture {
            fn new() -> Self {
                let fake = Arc::new(Mutex::new(FakeState::default()));
                let workflow = VisualCaptureWorkflow::new(
                    Box::new(Overlay(fake.clone())),
                    Box::new(Capture(fake.clone())),
                    Box::new(Assets(fake.clone())),
                );
                let payload = MkImagePayload {
                    asset_id: 7,
                    region: SearchRegion::Rectangle { rect: ORIGINAL },
                    wait: MkWaitOptions {
                        timeout_ms: 1_234,
                        poll_interval_ms: 56,
                    },
                    tolerance: 17,
                    alpha: AlphaPolicy::Ignore,
                    return_point: ReturnPoint::TopLeft,
                    not_found_policy: MkImageNotFoundPolicy::Fail,
                    outputs: MkImageOutputs::default(),
                };
                let mut editor = test_editor();
                editor.begin_edit(&step(MkAction::ImageClick(payload)));
                editor.visual_capture = Some(workflow);
                assert_eq!(
                    editor.draft_generation, 1,
                    "fixture must use a known draft generation"
                );
                assert_eq!(
                    editor.image_search.as_ref().unwrap().kind,
                    SearchRegionKind::Rectangle
                );
                assert_eq!(editor.image_search.as_ref().unwrap().rectangle, ORIGINAL);
                Self { editor, fake }
            }
            fn begin_selecting(&mut self, purpose: RectanglePurpose) {
                let destination = match purpose {
                    RectanglePurpose::SearchRegion => {
                        VisualRegionDestination::ImageActionSearchRegion
                    }
                    RectanglePurpose::ReferenceImageCapture => {
                        VisualRegionDestination::ImageActionReferenceAsset
                    }
                };
                self.editor
                    .request_rectangle_selection(MACRO_ID, purpose, destination)
                    .unwrap();
                assert_eq!(self.fake.lock().unwrap().purposes, [purpose]);
            }
            fn selection(&mut self, event: SelectionEvent) {
                self.fake.lock().unwrap().selection = Some(event);
            }
            fn tick(&mut self) {
                self.editor.tick_visual_capture(Some(MACRO_ID));
            }
            fn draft_payload(&self) -> &MkImagePayload {
                match &self.editor.draft.as_ref().unwrap().action {
                    MkAction::ImageClick(payload) => payload,
                    _ => panic!("visual fixture must remain an Image Click draft"),
                }
            }
            fn snapshots(&self) -> (Vec<u8>, ImageSearchEditorState) {
                (
                    serde_json::to_vec(self.editor.draft.as_ref().unwrap()).unwrap(),
                    self.editor.image_search.clone().unwrap(),
                )
            }
        }

        #[test]
        fn pick_region_changes_only_search_geometry() {
            let mut f = Fixture::new();
            let unrelated = f.draft_payload().clone();
            let selected = ScreenRect::new(200, 200, 400, 300);
            f.begin_selecting(RectanglePurpose::SearchRegion);
            f.selection(SelectionEvent::Confirmed {
                operation_id: OPERATION_ID,
                rect: selected,
            });
            f.tick();
            f.tick();
            assert_eq!(
                f.draft_payload().asset_id,
                7,
                "Pick Region must not replace the reference asset"
            );
            assert_eq!(
                f.editor.image_search.as_ref().unwrap().rectangle,
                selected,
                "Pick Region must update the editable search rectangle"
            );
            assert_eq!(
                f.draft_payload(),
                &unrelated,
                "Pick Region must not mutate unrelated payload fields before normal synchronization"
            );
            f.editor.sync_image_region_to_draft();
            let mut synchronized = unrelated;
            synchronized.region = SearchRegion::Rectangle { rect: selected };
            assert_eq!(
                f.draft_payload(),
                &synchronized,
                "Pick Region synchronization must change only the search rectangle"
            );
            let state = f.fake.lock().unwrap();
            assert!(
                state.capture_rects.is_empty(),
                "Pick Region must not capture the screen"
            );
            assert!(
                state.staged_dimensions.is_empty(),
                "Pick Region must not stage a reference asset"
            );
        }

        #[test]
        fn capture_changes_only_reference_asset() {
            let mut f = Fixture::new();
            let unrelated = f.draft_payload().clone();
            let selected = ScreenRect::new(321, 654, 40, 30);
            f.begin_selecting(RectanglePurpose::ReferenceImageCapture);
            f.selection(SelectionEvent::Confirmed {
                operation_id: OPERATION_ID,
                rect: selected,
            });
            f.tick();
            f.tick();
            f.tick();
            assert_eq!(
                f.draft_payload().asset_id,
                8,
                "Capture must install the newly staged reference asset"
            );
            let mut captured = unrelated.clone();
            captured.asset_id = 8;
            assert_eq!(
                f.draft_payload(),
                &captured,
                "Capture must change only the reference asset ID"
            );
            assert_eq!(
                f.draft_payload().region,
                unrelated.region,
                "Capture must not overwrite the search rectangle in the draft"
            );
            assert_eq!(
                f.editor.image_search.as_ref().unwrap().rectangle,
                ORIGINAL,
                "Capture must not overwrite the editable search rectangle"
            );
            let state = f.fake.lock().unwrap();
            assert_eq!(
                state.capture_rects,
                [selected],
                "Capture must use the exact nonzero-origin selection"
            );
            assert_eq!(
                state.staged_dimensions,
                [(40, 30)],
                "Capture must stage the exact 40x30 image"
            );
        }

        fn assert_capture_cancel(active_selection: bool) {
            let mut f = Fixture::new();
            let (draft, image) = f.snapshots();
            f.begin_selecting(RectanglePurpose::ReferenceImageCapture);
            if active_selection {
                f.selection(SelectionEvent::Cancelled {
                    operation_id: OPERATION_ID,
                });
                f.tick();
            } else {
                f.editor.visual_capture.as_mut().unwrap().cancel();
            }
            f.tick();
            assert_eq!(
                serde_json::to_vec(f.editor.draft.as_ref().unwrap()).unwrap(),
                draft,
                "Capture cancellation must preserve the complete draft byte-for-byte"
            );
            assert_eq!(
                f.editor.image_search.as_ref().unwrap(),
                &image,
                "Capture cancellation must preserve complete image-search editor state"
            );
            let state = f.fake.lock().unwrap();
            assert!(
                state.capture_rects.is_empty(),
                "Cancelled Capture must not call screen capture"
            );
            assert!(
                state.staged_dimensions.is_empty(),
                "Cancelled Capture must not stage an asset"
            );
        }
        #[test]
        fn capture_cancel_before_drag_confirmation_is_lossless() {
            assert_capture_cancel(false);
        }
        #[test]
        fn capture_cancel_during_active_selection_is_lossless() {
            assert_capture_cancel(true);
        }

        fn assert_capture_failure(stage_failure: bool) {
            let mut f = Fixture::new();
            let unrelated = f.draft_payload().clone();
            if stage_failure {
                f.fake.lock().unwrap().stage_result =
                    Some(Err("PNG staging failed visibly".into()));
            } else {
                f.fake.lock().unwrap().capture_result =
                    Some(Err("screen capture failed visibly".into()));
            }
            f.begin_selecting(RectanglePurpose::ReferenceImageCapture);
            f.selection(SelectionEvent::Confirmed {
                operation_id: OPERATION_ID,
                rect: ScreenRect::new(9, 11, 40, 30),
            });
            f.tick();
            f.tick();
            f.tick();
            assert_eq!(
                f.draft_payload(),
                &unrelated,
                "Failed Capture must not apply any asset or unrelated payload outcome"
            );
            assert_eq!(
                f.editor.image_search.as_ref().unwrap().rectangle,
                ORIGINAL,
                "Failed Capture must not overwrite the search rectangle"
            );
            assert!(
                f.editor
                    .capture_message
                    .as_deref()
                    .is_some_and(|m| m.contains("failed visibly")),
                "Capture failure must be visible to the user"
            );
            let state = f.fake.lock().unwrap();
            assert_eq!(
                state.capture_rects.len(),
                1,
                "Both downstream failures occur after one exact capture attempt"
            );
            assert_eq!(
                state.staged_dimensions.len(),
                usize::from(stage_failure),
                "Screen-capture failure must not stage; staging failure must receive the captured image"
            );
        }
        #[test]
        fn screen_capture_failure_preserves_independent_fields() {
            assert_capture_failure(false);
        }
        #[test]
        fn png_staging_failure_preserves_independent_fields() {
            assert_capture_failure(true);
        }

        #[test]
        fn stale_draft_token_ignores_completed_capture() {
            let mut f = Fixture::new();
            let snapshots = f.snapshots();
            f.begin_selecting(RectanglePurpose::ReferenceImageCapture);
            f.selection(SelectionEvent::Confirmed {
                operation_id: OPERATION_ID,
                rect: ScreenRect::new(44, 55, 40, 30),
            });
            f.tick();
            f.editor.draft_generation = f.editor.draft_generation.wrapping_add(1);
            f.tick();
            f.tick();
            assert_eq!(
                serde_json::to_vec(f.editor.draft.as_ref().unwrap()).unwrap(),
                snapshots.0,
                "A stale Capture result must not replace the reference asset"
            );
            assert_eq!(
                f.editor.image_search.as_ref().unwrap(),
                &snapshots.1,
                "A stale Capture result must not overwrite the search rectangle"
            );
        }
    }

    #[cfg(test)]
    mod visual_region_picker_tests {
        use super::*;
        use crate::gui::mkmacro_dialog::{
            image_search_controls::SearchRegionKind,
            window_picker::{MatcherDestination, MatcherEditRequest, MatcherPath},
        };

        #[test]
        fn wait_visual_change_picker_requires_live_compatible_draft() {
            let mut editor = test_editor();
            editor.begin_new(MkAction::WaitForVisualChange(WaitForVisualChange::default()));
            let generation = editor.draft_generation;
            let request = MatcherEditRequest {
                destination: MatcherDestination::Action {
                    macro_id: 7,
                    draft_generation: generation,
                    path: MatcherPath::VisualRegion,
                },
                original: MkWindowMatcher::default(),
            };
            let matcher = MkWindowMatcher {
                title: Some("picked".into()),
                ..Default::default()
            };
            assert!(!editor.apply_window_matcher(&request, matcher.clone(), Some(7)));
            editor.image_search.as_mut().unwrap().kind = SearchRegionKind::ClientArea;
            assert!(editor.apply_window_matcher(&request, matcher.clone(), Some(7)));
            assert_eq!(
                editor.image_search.as_ref().unwrap().client_matcher,
                matcher
            );
            assert!(
                editor
                    .image_search
                    .as_ref()
                    .unwrap()
                    .window_matcher
                    .title
                    .is_none()
            );
            editor.draft_generation += 1;
            assert!(!editor.apply_window_matcher(&request, MkWindowMatcher::default(), Some(7)));
        }
    }

    mod point_picker_routing_tests {
        use super::*;
        use crate::gui::mkmacro_dialog::visual_overlay::{
            VisualPointDestination, VisualPointRequest,
        };

        fn setup() -> (ActionEditorState, VisualPointRequest) {
            let mut editor = test_editor();
            let step = MkStep {
                id: 55,
                enabled: true,
                repeat: 1,
                delay_after_ms: 0,
                on_error: MkErrorPolicy::Stop,
                action: MkAction::SetVariable {
                    name: "p".into(),
                    value: MkValue::Point(MkPoint { x: 1, y: 2 }),
                },
            };
            editor.begin_edit(&step);
            let request = VisualPointRequest {
                macro_id: 7,
                draft_generation: editor.draft_generation,
                step_id: Some(55),
                destination: VisualPointDestination::SetVariablePoint,
            };
            (editor, request)
        }
        fn value(editor: &ActionEditorState) -> Option<MkPoint> {
            match &editor.draft.as_ref()?.action {
                MkAction::SetVariable {
                    value: MkValue::Point(p),
                    ..
                } => Some(*p),
                _ => None,
            }
        }

        #[test]
        fn matching_confirmation_updates_both_axes_atomically() {
            let (mut editor, request) = setup();
            assert!(editor.apply_point_confirmation(&request, MkPoint { x: -8, y: 99 }, Some(7)));
            assert_eq!(value(&editor), Some(MkPoint { x: -8, y: 99 }));
        }

        #[test]
        fn stale_identity_and_changed_draft_are_ignored() {
            for mutation in 0..5 {
                let (mut editor, mut request) = setup();
                match mutation {
                    0 => request.macro_id = 8,
                    1 => request.draft_generation += 1,
                    2 => request.step_id = Some(56),
                    3 => {
                        editor.draft.as_mut().unwrap().action =
                            MkAction::UnsetVariable { name: "p".into() }
                    }
                    _ => {
                        if let MkAction::SetVariable { value, .. } =
                            &mut editor.draft.as_mut().unwrap().action
                        {
                            *value = MkValue::Number(1.0)
                        }
                    }
                }
                assert!(!editor.apply_point_confirmation(
                    &request,
                    MkPoint { x: 10, y: 20 },
                    Some(7)
                ));
            }
        }

        #[test]
        fn cancellation_preserves_original_point() {
            let (editor, _) = setup();
            assert_eq!(value(&editor), Some(MkPoint { x: 1, y: 2 }));
        }
    }
}
