use crate::diagram::{
    DiagramDocument, DiagramScene, DiagramSession, EdgeId, FlowDirection, GraphOperation, GroupId,
    MAX_DIAGRAM_GROUPS, MAX_DIAGRAM_NODES, NodeId, NodePosition, NodeShape, Position, PreparedText,
    PreviewState, SceneMetrics, ScenePoint, SceneRect, SessionStatus, TextHorizontalAlignment,
    build_scene,
};
use eframe::egui;
use std::collections::BTreeSet;

const DEFAULT_VIEWPORT_SIZE: [f32; 2] = [1180.0, 760.0];
const MIN_VIEWPORT_SIZE: [f32; 2] = [480.0, 360.0];
const CAMERA_MIN_ZOOM: f32 = 0.025;
const CAMERA_MAX_ZOOM: f32 = 8.0;
const GRID_SPACING: f32 = 10.0;
const DRAG_NODE_SEPARATION: f64 = 120.0;
const VISUAL_TWO_COLUMN_MIN_WIDTH: f32 = 460.0;
const SPLIT_TWO_COLUMN_MIN_WIDTH: f32 = 860.0;

pub(super) fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("diagram_studio_viewport")
}

fn viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Diagram Studio")
        .with_inner_size(DEFAULT_VIEWPORT_SIZE)
        .with_min_inner_size(MIN_VIEWPORT_SIZE)
        .with_resizable(true)
        .with_visible(true)
}

#[cfg(feature = "qa-framebuffer-capture")]
#[derive(Clone)]
pub(super) struct DiagramViewportObservation {
    pub child_viewport_id: egui::ViewportId,
    pub callback_class: egui::ViewportClass,
    pub callback_frame_nr: u64,
    pub callback_pass_nr: u64,
    pub callback_pass_index: usize,
    pub visible: Option<bool>,
    pub minimized: Option<bool>,
    pub occluded: Option<bool>,
    pub focused: Option<bool>,
    pub body_rendered: bool,
    pub still_open: bool,
    pub application_input_allowed: bool,
    pub child_modal_open: bool,
    pub pixels_per_point: f32,
    pub native_pixels_per_point: Option<f32>,
    pub zoom_factor: f32,
    pub session_revision: u64,
    pub rendered_scene_revision: Option<u64>,
    pub status: SessionStatus,
    pub callback_observed_at_unix_ms: u64,
    pub completed_frame_nr: Option<u64>,
    pub completed_pass_nr: Option<u64>,
    pub renderer_returned_at_unix_ms: Option<u64>,
}

#[cfg(feature = "qa-framebuffer-capture")]
impl DiagramViewportObservation {
    fn is_immediate_target(&self) -> bool {
        self.callback_class == egui::ViewportClass::Immediate
            && self.child_viewport_id == viewport_id()
            && self.body_rendered
            && self.still_open
    }
}

#[cfg(feature = "qa-framebuffer-capture")]
fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ViewMode {
    Source,
    Visual,
    #[default]
    Split,
}

impl ViewMode {
    const ALL: [(Self, &'static str); 3] = [
        (Self::Source, "Source"),
        (Self::Visual, "Visual"),
        (Self::Split, "Split"),
    ];
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DiagramElement {
    Node(NodeId),
    Edge(EdgeId),
    Group(GroupId),
}

#[derive(Clone, Debug)]
enum PropertyFields {
    Node {
        label: String,
        shape: NodeShape,
        group: Option<GroupId>,
    },
    Edge {
        label: Option<String>,
        from: NodeId,
        to: NodeId,
    },
    Group {
        label: String,
    },
}

#[derive(Clone, Debug)]
struct PropertyDraft {
    target: DiagramElement,
    basis_revision: u64,
    fields: PropertyFields,
}

#[derive(Clone, Copy, Debug)]
struct Camera {
    center: ScenePoint,
    zoom: f32,
    fit_next: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            center: ScenePoint::default(),
            zoom: 1.0,
            fit_next: true,
        }
    }
}

#[derive(Clone, Debug)]
struct DragState {
    active_node: NodeId,
    pointer_origin: egui::Pos2,
    initial_positions: Vec<NodePosition>,
}

struct RevisionCache {
    revision: u64,
    status: SessionStatus,
    scene: Option<DiagramScene>,
    scene_error: Option<String>,
}

enum PendingMutation {
    ReplaceSource(String),
    Transaction {
        operations: Vec<GraphOperation>,
        expected_revision: u64,
        select_after: Option<DiagramElement>,
    },
    Undo,
    Redo,
}

struct PendingRootEffects {
    root_frame_nr: u64,
    root_pass_nr: u64,
    child_viewport_id: Option<egui::ViewportId>,
    viewport_callback_observed: bool,
    close_dialog: bool,
    close_viewport: bool,
    os_close_requested: bool,
    focus_viewport: bool,
    canvas_focus_requested: Option<egui::Id>,
    source_focus_requested: Option<egui::Id>,
    mutation: Option<PendingMutation>,
}

pub(crate) struct DiagramStudioDialog {
    pub(crate) open: bool,
    viewport_bootstrap_pending: bool,
    focus_viewport: bool,
    focus_source: bool,
    close_pending: bool,
    pending_root_effects: Option<PendingRootEffects>,
    deferred_canvas_focus: Option<(egui::ViewportId, egui::Id)>,
    deferred_source_focus: Option<(egui::ViewportId, egui::Id)>,
    session: DiagramSession,
    view_mode: ViewMode,
    cache: Option<RevisionCache>,
    camera: Camera,
    selection: Option<DiagramElement>,
    selected_nodes: BTreeSet<NodeId>,
    property_draft: Option<PropertyDraft>,
    drag: Option<DragState>,
    message: Option<String>,
    #[cfg(test)]
    test_source_editor_id: Option<egui::Id>,
    #[cfg(test)]
    test_source_editor_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_canvas_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_canvas_id: Option<egui::Id>,
    #[cfg(test)]
    test_action_rects: Vec<(String, egui::Rect, egui::Rect)>,
    #[cfg(test)]
    test_body_rendered: bool,
    #[cfg(test)]
    test_callback_id: Option<egui::ViewportId>,
    #[cfg(test)]
    test_callback_class: Option<egui::ViewportClass>,
    #[cfg(test)]
    test_application_input_allowed: bool,
    #[cfg(test)]
    test_child_focused: bool,
    #[cfg(test)]
    test_request_child_discard: bool,
    #[cfg(test)]
    test_child_discard_requested: bool,
    #[cfg(test)]
    test_pending_mutation: Option<PendingMutation>,
}

impl Default for DiagramStudioDialog {
    fn default() -> Self {
        Self {
            open: false,
            viewport_bootstrap_pending: false,
            focus_viewport: false,
            focus_source: false,
            close_pending: false,
            pending_root_effects: None,
            deferred_canvas_focus: None,
            deferred_source_focus: None,
            session: DiagramSession::new(),
            view_mode: ViewMode::Split,
            cache: None,
            camera: Camera::default(),
            selection: None,
            selected_nodes: BTreeSet::new(),
            property_draft: None,
            drag: None,
            message: None,
            #[cfg(test)]
            test_source_editor_id: None,
            #[cfg(test)]
            test_source_editor_rect: None,
            #[cfg(test)]
            test_canvas_rect: None,
            #[cfg(test)]
            test_canvas_id: None,
            #[cfg(test)]
            test_action_rects: Vec::new(),
            #[cfg(test)]
            test_body_rendered: false,
            #[cfg(test)]
            test_callback_id: None,
            #[cfg(test)]
            test_callback_class: None,
            #[cfg(test)]
            test_application_input_allowed: false,
            #[cfg(test)]
            test_child_focused: false,
            #[cfg(test)]
            test_request_child_discard: false,
            #[cfg(test)]
            test_child_discard_requested: false,
            #[cfg(test)]
            test_pending_mutation: None,
        }
    }
}

impl DiagramStudioDialog {
    /// Explicit opener; repeated user opens focus the existing child.
    pub(crate) fn open(&mut self) {
        if !self.open {
            self.viewport_bootstrap_pending = true;
        }
        self.open = true;
        self.close_pending = false;
        self.focus_viewport = true;
        self.focus_source = true;
        self.deferred_canvas_focus = None;
        self.deferred_source_focus = None;
    }

    /// Pinned-panel maintenance may show the child but does not steal focus.
    pub(crate) fn ensure_open(&mut self) {
        if !self.open {
            self.viewport_bootstrap_pending = true;
        }
        self.open = true;
        self.close_pending = false;
    }

    pub(crate) fn needs_viewport_bootstrap(&self) -> bool {
        self.viewport_bootstrap_pending
    }

    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        self.viewport_bootstrap_pending || self.close_pending
    }

    pub(crate) fn request_close(&mut self) {
        if self.open {
            self.open = false;
            self.viewport_bootstrap_pending = false;
            self.focus_viewport = false;
            self.focus_source = false;
            self.close_pending = true;
            self.deferred_canvas_focus = None;
            self.deferred_source_focus = None;
            self.cancel_transients();
        }
    }

    fn cancel_transients(&mut self) {
        self.drag = None;
        self.property_draft = None;
    }

    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        application_input_allowed: bool,
        root_input_allowed: bool,
    ) {
        #[cfg(feature = "qa-framebuffer-capture")]
        self.show_impl(
            ctx,
            first_ui_pass,
            application_input_allowed,
            root_input_allowed,
            None,
        );
        #[cfg(not(feature = "qa-framebuffer-capture"))]
        self.show_impl(
            ctx,
            first_ui_pass,
            application_input_allowed,
            root_input_allowed,
        );
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(super) fn show_with_observation(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        application_input_allowed: bool,
        root_input_allowed: bool,
    ) -> Option<DiagramViewportObservation> {
        let mut observation = None;
        self.show_impl(
            ctx,
            first_ui_pass,
            application_input_allowed,
            root_input_allowed,
            Some(&mut observation),
        );
        if let Some(observation) = observation.as_mut() {
            if observation.is_immediate_target() {
                let viewport_id = observation.child_viewport_id;
                observation.completed_frame_nr = Some(ctx.cumulative_frame_nr_for(viewport_id));
                observation.completed_pass_nr = Some(ctx.cumulative_pass_nr_for(viewport_id));
            }
            observation.renderer_returned_at_unix_ms = Some(unix_millis());
        }
        observation
    }

    fn show_impl(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        application_input_allowed: bool,
        root_input_allowed: bool,
        #[cfg(feature = "qa-framebuffer-capture")] mut observation_slot: Option<
            &mut Option<DiagramViewportObservation>,
        >,
    ) {
        self.refresh_cache();
        if !self.open {
            if self.close_pending && first_ui_pass && !ctx.will_discard() {
                self.pending_root_effects = Some(PendingRootEffects {
                    root_frame_nr: ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT),
                    root_pass_nr: ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT),
                    child_viewport_id: None,
                    viewport_callback_observed: false,
                    close_dialog: false,
                    close_viewport: true,
                    os_close_requested: false,
                    focus_viewport: false,
                    canvas_focus_requested: None,
                    source_focus_requested: None,
                    mutation: None,
                });
            }
            return;
        }

        let mut pending = None;
        let mut mutation_authorized = false;
        let mut child_pass_accepted = false;
        let mut close_requested = false;
        let mut os_close_requested = false;
        let mut focus_requested = false;
        let mut canvas_focus_requested = None;
        let mut source_focus_requested = None;
        let mut body_rendered = false;
        let mut child_modal_open = false;
        let mut child_focused = false;
        let mut callback_class = egui::ViewportClass::Root;
        let mut child_viewport_id = None;
        self.test_begin_frame();
        ctx.show_viewport_immediate(viewport_id(), viewport_builder(), |child, class| {
            callback_class = class;
            let independent = class == egui::ViewportClass::Immediate;
            let child_context = child.ctx().clone();
            child_viewport_id = Some(child_context.viewport_id());
            let current_pass = child.current_pass_index() == 0;
            let modal_open = child.memory(|memory| memory.top_modal_layer().is_some());
            let (focused, close_requested_by_os) = child.input(|input| {
                (
                    input.focused,
                    independent && input.viewport().close_requested(),
                )
            });
            child_focused = focused;
            child_modal_open = modal_open;
            let pass_authorized =
                first_ui_pass && current_pass && !ctx.will_discard() && !child.will_discard();
            let surface_input_allowed = if independent {
                application_input_allowed && child_context.viewport_id() == viewport_id()
            } else {
                root_input_allowed && child_context.viewport_id() == ctx.viewport_id()
            };
            let child_input_allowed = surface_input_allowed && focused && !modal_open;
            let actions_allowed = pass_authorized && child_input_allowed;
            let source_focus_allowed = pass_authorized && surface_input_allowed && !modal_open;
            focus_requested = independent
                && self.focus_viewport
                && pass_authorized
                && application_input_allowed
                && !modal_open;

            let keyboard_close = if actions_allowed {
                child.input_mut(|input| {
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                        || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                })
            } else {
                false
            };
            os_close_requested = close_requested_by_os;
            if pass_authorized && (close_requested_by_os || keyboard_close) {
                close_requested = true;
            }

            if independent {
                if !close_requested {
                    egui::CentralPanel::default().show(child, |ui| {
                        body_rendered = ui.is_visible() && !ui.is_sizing_pass();
                        self.draw_body(
                            ui,
                            actions_allowed,
                            &mut pending,
                            &mut canvas_focus_requested,
                            &mut source_focus_requested,
                            source_focus_allowed,
                        );
                    });
                }
            } else {
                body_rendered = child.is_visible() && !child.is_sizing_pass();
                self.draw_body(
                    child,
                    actions_allowed,
                    &mut pending,
                    &mut canvas_focus_requested,
                    &mut source_focus_requested,
                    source_focus_allowed,
                );
            }
            if body_rendered
                && pass_authorized
                && surface_input_allowed
                && !modal_open
                && !close_requested
            {
                if let Some((target_viewport, id)) = self.deferred_canvas_focus
                    && target_viewport == child_context.viewport_id()
                {
                    self.deferred_canvas_focus = None;
                    child.memory_mut(|memory| memory.request_focus(id));
                }
                if let Some((target_viewport, id)) = self.deferred_source_focus
                    && target_viewport == child_context.viewport_id()
                {
                    self.deferred_source_focus = None;
                    child.memory_mut(|memory| memory.request_focus(id));
                }
            }
            #[cfg(test)]
            if std::mem::take(&mut self.test_request_child_discard) {
                child.request_discard("Diagram Studio test child pass discard");
                self.test_child_discard_requested = true;
            }
            mutation_authorized = actions_allowed && !child.will_discard() && !ctx.will_discard();
            child_pass_accepted = pass_authorized && !child.will_discard() && !ctx.will_discard();
            #[cfg(test)]
            {
                self.test_callback_id = Some(child_context.viewport_id());
                self.test_callback_class = Some(class);
                self.test_application_input_allowed = application_input_allowed;
                self.test_child_focused = focused;
                self.test_body_rendered = body_rendered;
            }
            #[cfg(feature = "qa-framebuffer-capture")]
            if let Some(slot) = observation_slot.as_deref_mut() {
                let (visible, minimized, occluded, focused) = child.input(|input| {
                    let viewport = input.viewport();
                    (
                        viewport.visible(),
                        viewport.minimized,
                        viewport.occluded,
                        viewport.focused,
                    )
                });
                let cached = self.cache.as_ref();
                let status = cached
                    .map(|cache| cache.status.clone())
                    .unwrap_or_else(|| self.session.status());
                *slot = Some(DiagramViewportObservation {
                    child_viewport_id: child_context.viewport_id(),
                    callback_class: class,
                    callback_frame_nr: child.cumulative_frame_nr(),
                    callback_pass_nr: child.cumulative_pass_nr(),
                    callback_pass_index: child.current_pass_index(),
                    visible,
                    minimized,
                    occluded,
                    focused,
                    body_rendered,
                    still_open: self.open,
                    application_input_allowed,
                    child_modal_open,
                    pixels_per_point: child.pixels_per_point(),
                    native_pixels_per_point: child.native_pixels_per_point(),
                    zoom_factor: child.zoom_factor(),
                    session_revision: self.session.revision(),
                    rendered_scene_revision: cached
                        .and_then(|cache| cache.scene.as_ref().map(|_| cache.revision)),
                    status,
                    callback_observed_at_unix_ms: unix_millis(),
                    completed_frame_nr: None,
                    completed_pass_nr: None,
                    renderer_returned_at_unix_ms: None,
                });
            }
        });

        if first_ui_pass {
            let viewport_callback_observed = child_viewport_id.is_some();
            self.pending_root_effects = Some(PendingRootEffects {
                root_frame_nr: ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT),
                root_pass_nr: ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT),
                child_viewport_id: child_pass_accepted.then_some(child_viewport_id).flatten(),
                viewport_callback_observed,
                close_dialog: close_requested && child_pass_accepted,
                close_viewport: close_requested
                    && child_pass_accepted
                    && callback_class == egui::ViewportClass::Immediate,
                os_close_requested,
                focus_viewport: focus_requested && child_pass_accepted,
                canvas_focus_requested: child_pass_accepted
                    .then_some(canvas_focus_requested)
                    .flatten(),
                source_focus_requested: child_pass_accepted
                    .then_some(source_focus_requested)
                    .flatten(),
                mutation: (mutation_authorized && child_pass_accepted)
                    .then_some(pending)
                    .flatten(),
            });
        }
        #[cfg(test)]
        {
            self.test_body_rendered = body_rendered;
            self.test_child_focused = child_focused;
            let _ = child_modal_open;
        }
        let _ = (body_rendered, child_focused, child_modal_open);
    }

    pub(crate) fn on_ui_frame_complete(&mut self, ctx: &egui::Context) {
        let Some(effects) = self.pending_root_effects.take() else {
            return;
        };
        let retained_pass = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT)
            == effects.root_frame_nr.saturating_add(1)
            && ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT)
                == effects.root_pass_nr.saturating_add(1);
        if !retained_pass {
            if effects.os_close_requested {
                ctx.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::CancelClose);
            }
            return;
        }

        let bootstrap_completed =
            self.viewport_bootstrap_pending && effects.viewport_callback_observed;
        if bootstrap_completed {
            self.viewport_bootstrap_pending = false;
        }
        let child_viewport_id = effects.child_viewport_id;
        let repaint_child = effects.focus_viewport
            || effects.canvas_focus_requested.is_some()
            || effects.source_focus_requested.is_some()
            || effects.mutation.is_some()
            || effects.close_dialog
            || effects.close_viewport
            || effects.os_close_requested;
        let repaint_root = bootstrap_completed || repaint_child;

        if let Some(target_viewport) = child_viewport_id.clone() {
            if let Some(id) = effects.canvas_focus_requested {
                self.deferred_canvas_focus = Some((target_viewport, id));
            }
            if let Some(id) = effects.source_focus_requested {
                self.deferred_source_focus = Some((target_viewport, id));
                self.focus_source = false;
            }
        }
        if effects.focus_viewport {
            ctx.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::Focus);
            self.focus_viewport = false;
        }
        if effects.close_dialog {
            self.request_close();
            self.close_pending = false;
        }
        if effects.close_viewport {
            ctx.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::Close);
            self.close_pending = false;
        } else if effects.os_close_requested && !effects.close_dialog {
            ctx.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::CancelClose);
        }
        if let Some(mutation) = effects.mutation {
            self.apply_pending(mutation);
        }
        if repaint_child {
            if let Some(target_viewport) = child_viewport_id {
                ctx.request_repaint_of(target_viewport);
            }
        }
        if repaint_root {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    fn test_begin_frame(&mut self) {
        #[cfg(test)]
        {
            self.test_source_editor_rect = None;
            self.test_canvas_rect = None;
            self.test_canvas_id = None;
            self.test_action_rects.clear();
            self.test_body_rendered = false;
        }
    }

    fn refresh_cache(&mut self) {
        let revision = self.session.revision();
        if self
            .cache
            .as_ref()
            .is_some_and(|cache| cache.revision == revision)
        {
            return;
        }
        let status = self.session.status();
        let (scene, scene_error) = self.session.preview_graph().map_or((None, None), |graph| {
            match build_scene(graph, SceneMetrics::default()) {
                Ok(scene) => (Some(scene), None),
                Err(error) => (None, Some(error.to_string())),
            }
        });
        self.cache = Some(RevisionCache {
            revision,
            status,
            scene,
            scene_error,
        });
        self.revalidate_selection();
        self.drag = None;
    }

    fn revalidate_selection(&mut self) {
        let Some(graph) = self.session.preview_graph() else {
            self.selection = None;
            self.selected_nodes.clear();
            return;
        };
        self.selected_nodes
            .retain(|id| graph.nodes.iter().any(|node| &node.id == id));
        let selection_exists = self
            .selection
            .as_ref()
            .is_some_and(|selected| match selected {
                DiagramElement::Node(id) => graph.nodes.iter().any(|node| &node.id == id),
                DiagramElement::Edge(id) => graph.edges.iter().any(|edge| edge.id == *id),
                DiagramElement::Group(id) => graph.groups.iter().any(|group| &group.id == id),
            });
        if !selection_exists {
            self.selection = None;
            self.selected_nodes.clear();
            self.property_draft = None;
        }
    }

    fn apply_pending(&mut self, pending: PendingMutation) {
        let result = match pending {
            PendingMutation::ReplaceSource(source) => self
                .session
                .replace_source(source)
                .map(|changed| (changed, None)),
            PendingMutation::Transaction {
                operations,
                expected_revision,
                select_after,
            } => {
                if self.session.revision() != expected_revision {
                    self.message = Some(
                        "The diagram changed while these properties were open. Reload them before applying.".into(),
                    );
                    return;
                }
                match self.session.apply_transaction(&operations) {
                    Ok(changed) => Ok((changed, select_after)),
                    Err(error) => Err(error),
                }
            }
            PendingMutation::Undo => self.session.undo().map(|changed| (changed, None)),
            PendingMutation::Redo => self.session.redo().map(|changed| (changed, None)),
        };
        match result {
            Ok((changed, select_after)) => {
                self.message = None;
                if changed {
                    if let Some(selection) = select_after {
                        self.set_selection(selection, false);
                    }
                    self.refresh_cache();
                }
            }
            Err(error) => self.message = Some(error.to_string()),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_replace_source(&mut self, source: &str) {
        let _ = self.session.replace_source(source);
        self.refresh_cache();
    }

    #[cfg(test)]
    pub(crate) fn test_request_child_discard(&mut self) {
        self.test_request_child_discard = true;
    }

    #[cfg(test)]
    fn test_canvas_has_focus(&self) -> bool {
        self.test_canvas_id.is_some_and(|id| {
            super::render::immediate_viewport_test_focused_id()
                .is_some_and(|(viewport, focused)| viewport == viewport_id() && focused == Some(id))
        })
    }

    #[cfg(test)]
    pub(crate) fn test_stage_add_node(&mut self) {
        let id = NodeId::new("FromUi").expect("test node ID is valid");
        self.test_pending_mutation = Some(PendingMutation::Transaction {
            operations: vec![GraphOperation::AddNode {
                id: id.clone(),
                label: "From UI".into(),
                shape: NodeShape::Rectangle,
                position: Position::new(900.0, 900.0).expect("test position is valid"),
                group: None,
            }],
            expected_revision: self.session.revision(),
            select_after: Some(DiagramElement::Node(id)),
        });
    }

    #[cfg(test)]
    pub(crate) fn test_session(&self) -> &DiagramSession {
        &self.session
    }

    #[cfg(test)]
    pub(crate) fn test_last_callback(
        &self,
    ) -> (
        Option<egui::ViewportId>,
        Option<egui::ViewportClass>,
        bool,
        bool,
    ) {
        (
            self.test_callback_id,
            self.test_callback_class,
            self.test_body_rendered,
            self.test_application_input_allowed,
        )
    }

    #[cfg(test)]
    pub(crate) fn test_source_editor_has_focus(&self, _ctx: &egui::Context) -> bool {
        self.test_source_editor_id.is_some_and(|id| {
            super::render::immediate_viewport_test_focused_id()
                .is_some_and(|(viewport, focused)| viewport == viewport_id() && focused == Some(id))
        })
    }

    #[cfg(test)]
    pub(crate) fn test_source_focus_pending(&self) -> bool {
        self.focus_source
    }

    #[cfg(test)]
    pub(crate) fn test_move_child_focus_to(&mut self, id: egui::Id) {
        self.deferred_canvas_focus = Some((viewport_id(), id));
    }

    fn set_selection(&mut self, element: DiagramElement, extend: bool) {
        match &element {
            DiagramElement::Node(id) if extend => {
                if !self.selected_nodes.insert(id.clone()) {
                    self.selected_nodes.remove(id);
                }
                self.selection = if self.selected_nodes.contains(id) {
                    Some(element)
                } else {
                    self.selected_nodes
                        .last()
                        .cloned()
                        .map(DiagramElement::Node)
                };
            }
            DiagramElement::Node(id) => {
                self.selected_nodes.clear();
                self.selected_nodes.insert(id.clone());
                self.selection = Some(element);
            }
            _ => {
                self.selected_nodes.clear();
                self.selection = Some(element);
            }
        }
        self.property_draft = None;
    }

    fn draw_body(
        &mut self,
        ui: &mut egui::Ui,
        actions_allowed: bool,
        pending: &mut Option<PendingMutation>,
        canvas_focus_requested: &mut Option<egui::Id>,
        source_focus_requested: &mut Option<egui::Id>,
        source_focus_allowed: bool,
    ) {
        self.refresh_cache();
        #[cfg(test)]
        if pending.is_none() {
            *pending = self.test_pending_mutation.take();
        }
        #[cfg(test)]
        {
            self.test_body_rendered = true;
        }
        let Self {
            session,
            view_mode,
            cache,
            camera,
            selection,
            selected_nodes,
            property_draft,
            drag,
            message,
            focus_source,
            #[cfg(test)]
            test_source_editor_id,
            #[cfg(test)]
            test_source_editor_rect,
            #[cfg(test)]
            test_canvas_rect,
            #[cfg(test)]
            test_canvas_id,
            #[cfg(test)]
            test_action_rects,
            ..
        } = self;
        let revision = session.revision();
        let cache = cache.as_ref().expect("cache refreshed before drawing");
        let status = &cache.status;
        let graph = session.preview_graph();
        let visual_mutations_allowed = actions_allowed && status.preview == PreviewState::Current;

        if actions_allowed
            && pending.is_none()
            && let Some(action) = take_history_shortcut(ui)
        {
            *pending = Some(action);
        }

        egui::ScrollArea::vertical()
            .id_salt("diagram-studio-body-scroll")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading("Diagram Studio");
                    ui.separator();
                    for (mode, label) in ViewMode::ALL {
                        let response = ui.selectable_label(*view_mode == mode, label);
                        if actions_allowed && response.clicked() {
                            *view_mode = mode;
                        }
                    }
                    ui.separator();
                    let undo = ui.add_enabled(
                        actions_allowed && session.can_undo(),
                        egui::Button::new("Undo"),
                    );
                    if undo.clicked() && pending.is_none() {
                        *pending = Some(PendingMutation::Undo);
                    }
                    let redo = ui.add_enabled(
                        actions_allowed && session.can_redo(),
                        egui::Button::new("Redo"),
                    );
                    if redo.clicked() && pending.is_none() {
                        *pending = Some(PendingMutation::Redo);
                    }
                    if ui
                        .add_enabled(actions_allowed, egui::Button::new("Fit"))
                        .clicked()
                    {
                        camera.fit_next = true;
                    }
                    if ui
                        .add_enabled(actions_allowed, egui::Button::new("Reset view"))
                        .clicked()
                    {
                        *camera = Camera::default();
                        camera.fit_next = false;
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    let current = status.preview == PreviewState::Current;
                    let add_node = ui.add_enabled(
                        visual_mutations_allowed
                            && graph.is_some_and(|graph| graph.nodes.len() < MAX_DIAGRAM_NODES),
                        egui::Button::new("Add node"),
                    );
                    #[cfg(test)]
                    test_action_rects.push(("add_node".into(), add_node.rect, ui.clip_rect()));
                    if add_node.clicked() && pending.is_none() {
                        if let Some(graph) = graph {
                            match next_node(graph, camera.center) {
                                Ok((id, position)) => {
                                    *pending = Some(PendingMutation::Transaction {
                                        operations: vec![GraphOperation::AddNode {
                                            id: id.clone(),
                                            label: "New node".into(),
                                            shape: NodeShape::Rectangle,
                                            position,
                                            group: None,
                                        }],
                                        expected_revision: revision,
                                        select_after: Some(DiagramElement::Node(id)),
                                    });
                                }
                                Err(error) => *message = Some(error),
                            }
                        }
                    }
                    if ui
                        .add_enabled(
                            visual_mutations_allowed
                                && graph.is_some_and(|graph| {
                                    graph.nodes.len() >= 2
                                        && graph.edges.len() < crate::diagram::MAX_DIAGRAM_EDGES
                                }),
                            egui::Button::new("Add edge"),
                        )
                        .clicked()
                        && pending.is_none()
                        && let Some(graph) = graph
                    {
                        let from = graph.nodes.first().map(|node| node.id.clone());
                        let to = graph.nodes.get(1).map(|node| node.id.clone());
                        if let (Some(from), Some(to)) = (from, to) {
                            *pending = Some(PendingMutation::Transaction {
                                operations: vec![GraphOperation::AddEdge {
                                    from,
                                    to,
                                    label: None,
                                }],
                                expected_revision: revision,
                                select_after: None,
                            });
                        }
                    }
                    if ui
                        .add_enabled(
                            visual_mutations_allowed
                                && graph
                                    .is_some_and(|graph| graph.groups.len() < MAX_DIAGRAM_GROUPS),
                            egui::Button::new("Add group"),
                        )
                        .clicked()
                        && pending.is_none()
                        && let Some(graph) = graph
                    {
                        match next_group_id(graph) {
                            Ok(id) => {
                                *pending = Some(PendingMutation::Transaction {
                                    operations: vec![GraphOperation::AddGroup {
                                        id: id.clone(),
                                        label: "New group".into(),
                                    }],
                                    expected_revision: revision,
                                    select_after: Some(DiagramElement::Group(id)),
                                });
                            }
                            Err(error) => *message = Some(error),
                        }
                    }
                    let can_remove = visual_mutations_allowed
                        && (selected_nodes.len() > 0
                            || selection.as_ref().is_some_and(|selected| {
                                !matches!(selected, DiagramElement::Node(_))
                            }));
                    if ui
                        .add_enabled(can_remove, egui::Button::new("Remove selection"))
                        .clicked()
                        && pending.is_none()
                    {
                        if graph.is_some() {
                            let operations = removal_operations(selection.as_ref(), selected_nodes);
                            if !operations.is_empty() {
                                *pending = Some(PendingMutation::Transaction {
                                    operations,
                                    expected_revision: revision,
                                    select_after: None,
                                });
                            }
                        }
                    }
                    direction_control(ui, graph, current, actions_allowed, revision, pending);
                });

                if status.preview == PreviewState::Stale {
                    let diagnostic = status
                        .diagnostic
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "The source has changed.".into());
                    ui.colored_label(
                        egui::Color32::from_rgb(190, 104, 18),
                        format!("STALE PREVIEW — {diagnostic} Visual changes are disabled."),
                    );
                } else if status.preview == PreviewState::NoPreview {
                    let diagnostic = status
                        .diagnostic
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "No valid preview is available.".into());
                    ui.colored_label(egui::Color32::from_rgb(185, 65, 65), diagnostic);
                }
                if let Some(error) = message.as_ref() {
                    ui.colored_label(egui::Color32::from_rgb(185, 65, 65), error);
                }
                if let Some(error) = cache.scene_error.as_ref() {
                    ui.colored_label(egui::Color32::from_rgb(185, 65, 65), error);
                }
                ui.separator();

                match *view_mode {
                    ViewMode::Source => source_editor(
                        ui,
                        session,
                        actions_allowed,
                        pending,
                        *focus_source && source_focus_allowed,
                        source_focus_requested,
                        #[cfg(test)]
                        test_source_editor_id,
                        #[cfg(test)]
                        test_source_editor_rect,
                    ),
                    ViewMode::Visual => visual_workspace(
                        ui,
                        graph,
                        cache.scene.as_ref(),
                        status.preview == PreviewState::Current,
                        revision,
                        camera,
                        selection,
                        selected_nodes,
                        property_draft,
                        drag,
                        actions_allowed,
                        pending,
                        message,
                        canvas_focus_requested,
                        #[cfg(test)]
                        test_canvas_rect,
                        #[cfg(test)]
                        test_canvas_id,
                    ),
                    ViewMode::Split => {
                        #[cfg(test)]
                        let mut source_id = None;
                        #[cfg(test)]
                        let mut source_rect = None;
                        #[cfg(test)]
                        let mut canvas_rect = None;
                        #[cfg(test)]
                        let mut canvas_id = None;
                        if ui.available_width() < SPLIT_TWO_COLUMN_MIN_WIDTH {
                            source_editor(
                                ui,
                                session,
                                actions_allowed,
                                pending,
                                *focus_source && source_focus_allowed,
                                source_focus_requested,
                                #[cfg(test)]
                                &mut source_id,
                                #[cfg(test)]
                                &mut source_rect,
                            );
                            ui.separator();
                            visual_workspace(
                                ui,
                                graph,
                                cache.scene.as_ref(),
                                status.preview == PreviewState::Current,
                                revision,
                                camera,
                                selection,
                                selected_nodes,
                                property_draft,
                                drag,
                                actions_allowed,
                                pending,
                                message,
                                canvas_focus_requested,
                                #[cfg(test)]
                                &mut canvas_rect,
                                #[cfg(test)]
                                &mut canvas_id,
                            );
                        } else {
                            ui.columns(2, |columns| {
                                source_editor(
                                    &mut columns[0],
                                    session,
                                    actions_allowed,
                                    pending,
                                    *focus_source && source_focus_allowed,
                                    source_focus_requested,
                                    #[cfg(test)]
                                    &mut source_id,
                                    #[cfg(test)]
                                    &mut source_rect,
                                );
                                visual_workspace(
                                    &mut columns[1],
                                    graph,
                                    cache.scene.as_ref(),
                                    status.preview == PreviewState::Current,
                                    revision,
                                    camera,
                                    selection,
                                    selected_nodes,
                                    property_draft,
                                    drag,
                                    actions_allowed,
                                    pending,
                                    message,
                                    canvas_focus_requested,
                                    #[cfg(test)]
                                    &mut canvas_rect,
                                    #[cfg(test)]
                                    &mut canvas_id,
                                );
                            });
                        }
                        #[cfg(test)]
                        {
                            *test_source_editor_id = source_id;
                            *test_source_editor_rect = source_rect;
                            *test_canvas_rect = canvas_rect;
                            *test_canvas_id = canvas_id;
                        }
                    }
                }
            });
    }
}

fn source_editor(
    ui: &mut egui::Ui,
    session: &DiagramSession,
    actions_allowed: bool,
    pending: &mut Option<PendingMutation>,
    focus_source: bool,
    source_focus_requested: &mut Option<egui::Id>,
    #[cfg(test)] test_editor_id: &mut Option<egui::Id>,
    #[cfg(test)] test_editor_rect: &mut Option<egui::Rect>,
) {
    ui.label("Mermaid source (raw text is retained exactly)");
    let editor_id = ui.make_persistent_id("diagram-studio-source-editor");
    if focus_source {
        *source_focus_requested = Some(editor_id);
    }
    clear_text_edit_undoer(ui.ctx(), editor_id);
    let mut source = session.draft().to_owned();
    let response = ui.add_enabled(
        actions_allowed,
        egui::TextEdit::multiline(&mut source)
            .id(editor_id)
            .desired_width(f32::INFINITY)
            .desired_rows(14)
            .code_editor(),
    );
    #[cfg(test)]
    {
        *test_editor_id = Some(editor_id);
        *test_editor_rect = Some(response.rect);
    }
    if response.changed() && actions_allowed && pending.is_none() {
        *pending = Some(PendingMutation::ReplaceSource(source));
    }
    ui.small("Supported syntax is documented in the Diagram Studio help section.");
}

fn clear_text_edit_undoer(ctx: &egui::Context, id: egui::Id) {
    if let Some(mut state) = egui::widgets::text_edit::TextEditState::load(ctx, id) {
        state.clear_undoer();
        state.store(ctx, id);
    }
}

fn direction_control(
    ui: &mut egui::Ui,
    graph: Option<&DiagramDocument>,
    graph_is_current: bool,
    actions_allowed: bool,
    revision: u64,
    pending: &mut Option<PendingMutation>,
) {
    let Some(graph) = graph else {
        ui.add_enabled_ui(false, |ui| {
            ui.label("Direction unavailable");
        });
        return;
    };
    let mut direction = graph.direction;
    ui.add_enabled_ui(actions_allowed && graph_is_current, |ui| {
        egui::ComboBox::from_id_salt("diagram-studio-direction")
            .selected_text(direction.as_mermaid())
            .show_ui(ui, |ui| {
                for option in [
                    FlowDirection::TopDown,
                    FlowDirection::TopBottom,
                    FlowDirection::LeftRight,
                ] {
                    ui.selectable_value(&mut direction, option, option.as_mermaid());
                }
            });
    });
    if direction != graph.direction && pending.is_none() {
        *pending = Some(PendingMutation::Transaction {
            operations: vec![GraphOperation::SetDirection(direction)],
            expected_revision: revision,
            select_after: None,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn visual_workspace(
    ui: &mut egui::Ui,
    graph: Option<&DiagramDocument>,
    scene: Option<&DiagramScene>,
    graph_is_current: bool,
    revision: u64,
    camera: &mut Camera,
    selection: &mut Option<DiagramElement>,
    selected_nodes: &mut BTreeSet<NodeId>,
    property_draft: &mut Option<PropertyDraft>,
    drag: &mut Option<DragState>,
    actions_allowed: bool,
    pending: &mut Option<PendingMutation>,
    message: &mut Option<String>,
    canvas_focus_requested: &mut Option<egui::Id>,
    #[cfg(test)] test_canvas_rect: &mut Option<egui::Rect>,
    #[cfg(test)] test_canvas_id: &mut Option<egui::Id>,
) {
    if ui.available_width() < VISUAL_TWO_COLUMN_MIN_WIDTH {
        draw_inspector(
            ui,
            graph,
            graph_is_current,
            revision,
            selection,
            selected_nodes,
            property_draft,
            actions_allowed,
            pending,
        );
        ui.separator();
        let canvas_rect = ui
            .scope_builder(canvas_scope_builder(), |ui| {
                draw_canvas(
                    ui,
                    graph,
                    scene,
                    graph_is_current,
                    revision,
                    camera,
                    selection,
                    selected_nodes,
                    drag,
                    actions_allowed,
                    pending,
                    message,
                    canvas_focus_requested,
                    #[cfg(test)]
                    test_canvas_id,
                )
            })
            .inner;
        #[cfg(test)]
        {
            *test_canvas_rect = Some(canvas_rect);
        }
    } else {
        ui.columns(2, |columns| {
            draw_inspector(
                &mut columns[0],
                graph,
                graph_is_current,
                revision,
                selection,
                selected_nodes,
                property_draft,
                actions_allowed,
                pending,
            );
            let canvas_rect = columns[1]
                .scope_builder(canvas_scope_builder(), |ui| {
                    draw_canvas(
                        ui,
                        graph,
                        scene,
                        graph_is_current,
                        revision,
                        camera,
                        selection,
                        selected_nodes,
                        drag,
                        actions_allowed,
                        pending,
                        message,
                        canvas_focus_requested,
                        #[cfg(test)]
                        test_canvas_id,
                    )
                })
                .inner;
            #[cfg(test)]
            {
                *test_canvas_rect = Some(canvas_rect);
            }
        });
    }
}

fn canvas_scope_builder() -> egui::UiBuilder {
    egui::UiBuilder::new()
        .id(egui::Id::new("diagram-studio-canvas-scope"))
        .layout(egui::Layout::top_down(egui::Align::Min))
}

#[allow(clippy::too_many_arguments)]
fn draw_inspector(
    ui: &mut egui::Ui,
    graph: Option<&DiagramDocument>,
    graph_is_current: bool,
    revision: u64,
    selection: &mut Option<DiagramElement>,
    selected_nodes: &mut BTreeSet<NodeId>,
    property_draft: &mut Option<PropertyDraft>,
    actions_allowed: bool,
    pending: &mut Option<PendingMutation>,
) {
    ui.set_min_width(180.0);
    ui.heading("Structure");
    ui.small("Click list entries or nodes to inspect them.");
    if let Some(graph) = graph {
        ui.collapsing(format!("Nodes ({})", graph.nodes.len()), |ui| {
            for node in &graph.nodes {
                let selected = selected_nodes.contains(&node.id);
                let response = ui.selectable_label(
                    selected,
                    format!("{}  {}", node.id, node.label.replace('\n', " ")),
                );
                if actions_allowed && response.clicked() {
                    select_element(
                        selection,
                        selected_nodes,
                        DiagramElement::Node(node.id.clone()),
                        response
                            .ctx
                            .input(|input| input.modifiers.shift || input.modifiers.command),
                    );
                }
            }
        });
        ui.collapsing(format!("Edges ({})", graph.edges.len()), |ui| {
            for edge in &graph.edges {
                let element = DiagramElement::Edge(edge.id);
                let label = edge.label.as_deref().unwrap_or("(unlabeled)");
                let response = ui.selectable_label(
                    selection.as_ref() == Some(&element),
                    format!("{} → {}  {label}", edge.from, edge.to),
                );
                if actions_allowed && response.clicked() {
                    select_element(selection, selected_nodes, element, false);
                }
            }
        });
        ui.collapsing(format!("Groups ({})", graph.groups.len()), |ui| {
            for group in &graph.groups {
                let element = DiagramElement::Group(group.id.clone());
                let response = ui.selectable_label(
                    selection.as_ref() == Some(&element),
                    format!("{}  {}", group.id, group.label),
                );
                if actions_allowed && response.clicked() {
                    select_element(selection, selected_nodes, element, false);
                }
            }
        });
    } else {
        ui.label("No valid graph preview.");
    }
    ui.separator();
    ui.heading("Properties");
    draw_properties(
        ui,
        graph,
        graph_is_current,
        revision,
        selection,
        property_draft,
        actions_allowed,
        pending,
    );
}

fn select_element(
    selection: &mut Option<DiagramElement>,
    selected_nodes: &mut BTreeSet<NodeId>,
    element: DiagramElement,
    extend: bool,
) {
    match &element {
        DiagramElement::Node(id) if extend => {
            if !selected_nodes.insert(id.clone()) {
                selected_nodes.remove(id);
            }
            *selection = if selected_nodes.contains(id) {
                Some(element)
            } else {
                selected_nodes.last().cloned().map(DiagramElement::Node)
            };
        }
        DiagramElement::Node(id) => {
            selected_nodes.clear();
            selected_nodes.insert(id.clone());
            *selection = Some(element);
        }
        _ => {
            selected_nodes.clear();
            *selection = Some(element);
        }
    }
}

fn select_canvas_element(
    selection: &mut Option<DiagramElement>,
    selected_nodes: &mut BTreeSet<NodeId>,
    element: DiagramElement,
    extend: bool,
    canvas_id: egui::Id,
    canvas_focus_requested: &mut Option<egui::Id>,
) {
    select_element(selection, selected_nodes, element, extend);
    *canvas_focus_requested = Some(canvas_id);
}

fn draw_properties(
    ui: &mut egui::Ui,
    graph: Option<&DiagramDocument>,
    graph_is_current: bool,
    revision: u64,
    selection: &Option<DiagramElement>,
    property_draft: &mut Option<PropertyDraft>,
    actions_allowed: bool,
    pending: &mut Option<PendingMutation>,
) {
    let (Some(graph), Some(selection)) = (graph, selection.as_ref()) else {
        *property_draft = None;
        ui.label("Select a node, edge, or group.");
        return;
    };
    if property_draft
        .as_ref()
        .is_none_or(|draft| draft.target != *selection)
    {
        *property_draft = property_draft_for(graph, selection, revision);
    }
    let stale_before_reload = property_draft
        .as_ref()
        .is_some_and(|draft| draft.basis_revision != revision);
    if stale_before_reload {
        ui.colored_label(
            egui::Color32::from_rgb(190, 104, 18),
            "The source or history changed. Reload these properties before applying.",
        );
        if ui.button("Reload properties").clicked() && actions_allowed {
            *property_draft = property_draft_for(graph, selection, revision);
        }
    }
    let Some(draft) = property_draft.as_mut() else {
        ui.label("This selection is no longer in the preview.");
        return;
    };
    let stale_basis = draft.basis_revision != revision;
    let can_apply = actions_allowed && graph_is_current && !stale_basis;
    let mut operations = Vec::new();
    match &mut draft.fields {
        PropertyFields::Node {
            label,
            shape,
            group,
        } => {
            ui.label(format!("Node {}", selection_label(selection)));
            let label_id =
                ui.make_persistent_id(("diagram-node-label", selection_label(selection)));
            clear_text_edit_undoer(ui.ctx(), label_id);
            ui.add_enabled_ui(actions_allowed && !stale_basis, |ui| {
                ui.label("Label");
                ui.add(egui::TextEdit::singleline(label).id(label_id));
                ui.label("Shape");
                egui::ComboBox::from_id_salt(("diagram-node-shape", selection_label(selection)))
                    .selected_text(shape_label(*shape))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(shape, NodeShape::Rectangle, "Rectangle");
                        ui.selectable_value(shape, NodeShape::Rounded, "Rounded");
                        ui.selectable_value(shape, NodeShape::Diamond, "Diamond");
                    });
                ui.label("Group");
                egui::ComboBox::from_id_salt(("diagram-node-group", selection_label(selection)))
                    .selected_text(group.as_ref().map_or("None", GroupId::as_str))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(group, None, "None");
                        for candidate in &graph.groups {
                            ui.selectable_value(
                                group,
                                Some(candidate.id.clone()),
                                candidate.id.as_str(),
                            );
                        }
                    });
            });
            if let DiagramElement::Node(id) = selection {
                operations.extend([
                    GraphOperation::SetNodeLabel {
                        id: id.clone(),
                        label: label.clone(),
                    },
                    GraphOperation::SetNodeShape {
                        id: id.clone(),
                        shape: *shape,
                    },
                    GraphOperation::SetNodeGroup {
                        id: id.clone(),
                        group: group.clone(),
                    },
                ]);
            }
        }
        PropertyFields::Edge { label, from, to } => {
            ui.label(format!("Edge {}", selection_label(selection)));
            let label_id =
                ui.make_persistent_id(("diagram-edge-label", selection_label(selection)));
            clear_text_edit_undoer(ui.ctx(), label_id);
            let mut text = label.clone().unwrap_or_default();
            ui.add_enabled_ui(actions_allowed && !stale_basis, |ui| {
                ui.label("Label (blank removes label)");
                let response = ui.add(egui::TextEdit::singleline(&mut text).id(label_id));
                if response.changed() {
                    *label = (!text.is_empty()).then(|| text.clone());
                }
                if !graph.nodes.is_empty() {
                    ui.label("From");
                    egui::ComboBox::from_id_salt(("diagram-edge-from", selection_label(selection)))
                        .selected_text(from.as_str())
                        .show_ui(ui, |ui| {
                            for node in &graph.nodes {
                                ui.selectable_value(from, node.id.clone(), node.id.as_str());
                            }
                        });
                    ui.label("To");
                    egui::ComboBox::from_id_salt(("diagram-edge-to", selection_label(selection)))
                        .selected_text(to.as_str())
                        .show_ui(ui, |ui| {
                            for node in &graph.nodes {
                                ui.selectable_value(to, node.id.clone(), node.id.as_str());
                            }
                        });
                }
            });
            if let DiagramElement::Edge(id) = selection {
                operations.extend([
                    GraphOperation::SetEdgeLabel {
                        id: *id,
                        label: label.clone(),
                    },
                    GraphOperation::ReconnectEdge {
                        id: *id,
                        from: from.clone(),
                        to: to.clone(),
                    },
                ]);
            }
        }
        PropertyFields::Group { label } => {
            ui.label(format!("Group {}", selection_label(selection)));
            let label_id =
                ui.make_persistent_id(("diagram-group-label", selection_label(selection)));
            clear_text_edit_undoer(ui.ctx(), label_id);
            ui.add_enabled_ui(actions_allowed && !stale_basis, |ui| {
                ui.label("Label");
                ui.add(egui::TextEdit::singleline(label).id(label_id));
            });
            if let DiagramElement::Group(id) = selection {
                operations.push(GraphOperation::SetGroupLabel {
                    id: id.clone(),
                    label: label.clone(),
                });
            }
        }
    }
    if ui
        .add_enabled(can_apply, egui::Button::new("Apply properties"))
        .clicked()
        && pending.is_none()
        && !operations.is_empty()
    {
        *pending = Some(PendingMutation::Transaction {
            operations,
            expected_revision: draft.basis_revision,
            select_after: Some(draft.target.clone()),
        });
    }
}

fn property_draft_for(
    graph: &DiagramDocument,
    selection: &DiagramElement,
    revision: u64,
) -> Option<PropertyDraft> {
    let fields = match selection {
        DiagramElement::Node(id) => {
            let node = graph.nodes.iter().find(|node| &node.id == id)?;
            PropertyFields::Node {
                label: node.label.clone(),
                shape: node.shape,
                group: node.group.clone(),
            }
        }
        DiagramElement::Edge(id) => {
            let edge = graph.edges.iter().find(|edge| edge.id == *id)?;
            PropertyFields::Edge {
                label: edge.label.clone(),
                from: edge.from.clone(),
                to: edge.to.clone(),
            }
        }
        DiagramElement::Group(id) => {
            let group = graph.groups.iter().find(|group| &group.id == id)?;
            PropertyFields::Group {
                label: group.label.clone(),
            }
        }
    };
    Some(PropertyDraft {
        target: selection.clone(),
        basis_revision: revision,
        fields,
    })
}

fn selection_label(selection: &DiagramElement) -> String {
    match selection {
        DiagramElement::Node(id) => id.as_str().to_owned(),
        DiagramElement::Edge(id) => id.get().to_string(),
        DiagramElement::Group(id) => id.as_str().to_owned(),
    }
}

fn shape_label(shape: NodeShape) -> &'static str {
    match shape {
        NodeShape::Rectangle => "Rectangle",
        NodeShape::Rounded => "Rounded",
        NodeShape::Diamond => "Diamond",
    }
}

fn next_node(
    graph: &DiagramDocument,
    camera_center: ScenePoint,
) -> Result<(NodeId, Position), String> {
    let id = (1..=MAX_DIAGRAM_NODES + 1)
        .find_map(|index| {
            let candidate = NodeId::new(format!("Node{index}")).ok()?;
            (!graph.nodes.iter().any(|node| node.id == candidate)).then_some(candidate)
        })
        .ok_or_else(|| "Could not allocate a unique node ID.".to_string())?;
    let positions = graph
        .nodes
        .iter()
        .map(|node| node.position)
        .collect::<Vec<_>>();
    let center_x = snap_world(f64::from(camera_center.x));
    let center_y = snap_world(f64::from(camera_center.y));
    let position = (0_i32..=32)
        .flat_map(|radius| {
            (-radius..=radius).flat_map(move |dy| {
                (-radius..=radius)
                    .filter_map(move |dx| (dx.abs().max(dy.abs()) == radius).then_some((dx, dy)))
            })
        })
        .find_map(|(dx, dy)| {
            let x = center_x + dx as f32 * 180.0;
            let y = center_y + dy as f32 * 180.0;
            let candidate = Position::new(x, y).ok()?;
            positions
                .iter()
                .all(|position| {
                    f64::from(position.x - candidate.x).hypot(f64::from(position.y - candidate.y))
                        >= DRAG_NODE_SEPARATION
                })
                .then_some(candidate)
        })
        .ok_or_else(|| {
            "No separated bounded node position is available near the current view.".to_string()
        })?;
    Ok((id, position))
}

fn next_group_id(graph: &DiagramDocument) -> Result<GroupId, String> {
    (1..=MAX_DIAGRAM_GROUPS + 1)
        .find_map(|index| {
            let candidate = GroupId::new(format!("Group{index}")).ok()?;
            (!graph.groups.iter().any(|group| group.id == candidate)
                && !graph
                    .nodes
                    .iter()
                    .any(|node| node.id.as_str() == candidate.as_str()))
            .then_some(candidate)
        })
        .ok_or_else(|| "Could not allocate a unique group ID.".into())
}

fn removal_operations(
    selection: Option<&DiagramElement>,
    selected_nodes: &BTreeSet<NodeId>,
) -> Vec<GraphOperation> {
    if !selected_nodes.is_empty() {
        return selected_nodes
            .iter()
            .map(|id| GraphOperation::RemoveNode { id: id.clone() })
            .collect();
    }
    match selection {
        Some(DiagramElement::Node(id)) => vec![GraphOperation::RemoveNode { id: id.clone() }],
        Some(DiagramElement::Edge(id)) => vec![GraphOperation::RemoveEdge { id: *id }],
        Some(DiagramElement::Group(id)) => vec![GraphOperation::RemoveGroup { id: id.clone() }],
        None => Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_canvas(
    ui: &mut egui::Ui,
    graph: Option<&DiagramDocument>,
    scene: Option<&DiagramScene>,
    graph_is_current: bool,
    revision: u64,
    camera: &mut Camera,
    selection: &mut Option<DiagramElement>,
    selected_nodes: &mut BTreeSet<NodeId>,
    drag: &mut Option<DragState>,
    actions_allowed: bool,
    pending: &mut Option<PendingMutation>,
    message: &mut Option<String>,
    canvas_focus_requested: &mut Option<egui::Id>,
    #[cfg(test)] test_canvas_id: &mut Option<egui::Id>,
) -> egui::Rect {
    let width = ui.available_width().max(220.0);
    let height = ui.available_height().max(300.0);
    let (canvas_response, painter) =
        ui.allocate_painter(egui::vec2(width, height), egui::Sense::click_and_drag());
    let canvas_rect = canvas_response.rect;
    let canvas_id = canvas_response.id;
    #[cfg(test)]
    {
        *test_canvas_id = Some(canvas_id);
    }
    let painter = painter.with_clip_rect(painter.clip_rect().intersect(canvas_rect));
    painter.rect_filled(canvas_rect, 0.0, egui::Color32::from_rgb(248, 249, 251));

    let pointer = ui.ctx().input(|input| input.pointer.interact_pos());
    if actions_allowed
        && let Some(pointer) = pointer.filter(|pointer| canvas_rect.contains(*pointer))
    {
        let scroll = ui.ctx().input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > 0.01 {
            camera.zoom =
                (camera.zoom * (scroll * 0.001).exp()).clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
        }
        let _ = pointer;
    }
    if actions_allowed && canvas_response.dragged_by(egui::PointerButton::Middle) {
        let delta = canvas_response.drag_delta();
        camera.center.x -= f64::from(delta.x / camera.zoom);
        camera.center.y -= f64::from(delta.y / camera.zoom);
    }

    let mut entity_selected = false;
    if let (Some(scene), Some(bounds)) = (scene, scene.and_then(|scene| scene.bounds)) {
        if camera.fit_next {
            fit_camera(camera, bounds, canvas_rect);
        }
        let drag_delta = drag.as_ref().and_then(|drag| {
            pointer.map(|pointer| ScenePoint {
                x: f64::from(pointer.x - drag.pointer_origin.x) / f64::from(camera.zoom),
                y: f64::from(pointer.y - drag.pointer_origin.y) / f64::from(camera.zoom),
            })
        });
        for group in &scene.groups {
            let group_rect = world_rect_to_screen(group.bounds, canvas_rect, *camera);
            let selected = selection == &Some(DiagramElement::Group(group.id.clone()));
            painter.rect_stroke(
                group_rect,
                10.0,
                egui::Stroke::new(
                    if selected { 2.0 } else { 1.0 },
                    if selected {
                        egui::Color32::from_rgb(55, 109, 189)
                    } else {
                        egui::Color32::from_rgb(150, 160, 176)
                    },
                ),
                egui::StrokeKind::Inside,
            );
            let label_rect = world_rect_to_screen(group.label_bounds, canvas_rect, *camera);
            draw_prepared_text(
                &painter,
                label_rect,
                &group.label,
                group.label_alignment,
                egui::Color32::from_rgb(85, 94, 106),
                scene.metrics,
                camera.zoom,
            );
            let header_hit = label_rect.expand(4.0).intersect(canvas_rect);
            let response = ui.interact(
                header_hit,
                egui::Id::new(("diagram-group", group.id.as_str())),
                egui::Sense::click(),
            );
            if actions_allowed && response.clicked() {
                entity_selected = true;
                select_canvas_element(
                    selection,
                    selected_nodes,
                    DiagramElement::Group(group.id.clone()),
                    false,
                    canvas_id,
                    canvas_focus_requested,
                );
            }
        }

        for edge in &scene.edges {
            let points = cubic_points(edge.path, canvas_rect, *camera, 20);
            if points.len() >= 2 {
                let selected = selection == &Some(DiagramElement::Edge(edge.id));
                let color = if selected {
                    egui::Color32::from_rgb(44, 105, 190)
                } else {
                    egui::Color32::from_rgb(85, 92, 103)
                };
                painter.add(egui::Shape::line(
                    points.clone(),
                    egui::Stroke::new(if selected { 2.5 } else { 1.6 }, color),
                ));
                let tip = world_to_screen(edge.arrowhead.tip, canvas_rect, *camera);
                let left = world_to_screen(edge.arrowhead.left, canvas_rect, *camera);
                let right = world_to_screen(edge.arrowhead.right, canvas_rect, *camera);
                painter.line_segment([tip, left], egui::Stroke::new(1.6, color));
                painter.line_segment([tip, right], egui::Stroke::new(1.6, color));
                if let (Some(label), Some(label_bounds), Some(alignment)) =
                    (edge.label.as_ref(), edge.label_bounds, edge.label_alignment)
                {
                    draw_prepared_text(
                        &painter,
                        world_rect_to_screen(label_bounds, canvas_rect, *camera),
                        label,
                        alignment,
                        color,
                        scene.metrics,
                        camera.zoom,
                    );
                }
                if let Some(hit_rect) = points_bounds(&points) {
                    let response = ui.interact(
                        hit_rect.expand(8.0).intersect(canvas_rect),
                        egui::Id::new(("diagram-edge", edge.id.get())),
                        egui::Sense::click(),
                    );
                    if actions_allowed && response.clicked() {
                        entity_selected = true;
                        select_canvas_element(
                            selection,
                            selected_nodes,
                            DiagramElement::Edge(edge.id),
                            false,
                            canvas_id,
                            canvas_focus_requested,
                        );
                    }
                }
            }
        }

        if let Some(graph) = graph {
            for node in &scene.nodes {
                let is_being_dragged = drag
                    .as_ref()
                    .is_some_and(|drag| drag.initial_positions.iter().any(|p| p.id == node.id));
                let translated = if is_being_dragged {
                    drag_delta.map_or(node.bounds, |delta| translate_rect(node.bounds, delta))
                } else {
                    node.bounds
                };
                let node_rect = world_rect_to_screen(translated, canvas_rect, *camera);
                let selected = selected_nodes.contains(&node.id);
                let fill = if selected {
                    egui::Color32::from_rgb(222, 235, 252)
                } else {
                    egui::Color32::WHITE
                };
                let stroke = if selected {
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(50, 108, 190))
                } else {
                    egui::Stroke::new(1.2, egui::Color32::from_rgb(81, 91, 106))
                };
                match node.shape {
                    NodeShape::Rectangle => {
                        painter.rect_filled(node_rect, 2.0, fill);
                        painter.rect_stroke(node_rect, 2.0, stroke, egui::StrokeKind::Inside);
                    }
                    NodeShape::Rounded => {
                        painter.rect_filled(node_rect, 12.0, fill);
                        painter.rect_stroke(node_rect, 12.0, stroke, egui::StrokeKind::Inside);
                    }
                    NodeShape::Diamond => {
                        let center = node_rect.center();
                        let vertices = vec![
                            egui::pos2(center.x, node_rect.top()),
                            egui::pos2(node_rect.right(), center.y),
                            egui::pos2(center.x, node_rect.bottom()),
                            egui::pos2(node_rect.left(), center.y),
                        ];
                        painter.add(egui::Shape::convex_polygon(vertices, fill, stroke));
                    }
                }
                let label_bounds = if is_being_dragged {
                    drag_delta.map_or(node.label_bounds, |delta| {
                        translate_rect(node.label_bounds, delta)
                    })
                } else {
                    node.label_bounds
                };
                let label_rect = world_rect_to_screen(label_bounds, canvas_rect, *camera);
                draw_prepared_text(
                    &painter,
                    label_rect,
                    &node.label,
                    node.label_alignment,
                    egui::Color32::from_rgb(34, 39, 47),
                    scene.metrics,
                    camera.zoom,
                );
                let response = ui.interact(
                    node_rect,
                    egui::Id::new(("diagram-node", node.id.as_str())),
                    egui::Sense::click_and_drag(),
                );
                if actions_allowed && response.clicked() {
                    entity_selected = true;
                    let extend = ui
                        .ctx()
                        .input(|input| input.modifiers.shift || input.modifiers.command);
                    select_canvas_element(
                        selection,
                        selected_nodes,
                        DiagramElement::Node(node.id.clone()),
                        extend,
                        canvas_id,
                        canvas_focus_requested,
                    );
                }
                if actions_allowed && response.drag_started() {
                    entity_selected = true;
                    let extend = ui
                        .ctx()
                        .input(|input| input.modifiers.shift || input.modifiers.command);
                    if !selected_nodes.contains(&node.id) {
                        select_canvas_element(
                            selection,
                            selected_nodes,
                            DiagramElement::Node(node.id.clone()),
                            extend,
                            canvas_id,
                            canvas_focus_requested,
                        );
                    } else {
                        *canvas_focus_requested = Some(canvas_id);
                    }
                    let initial_positions = graph
                        .nodes
                        .iter()
                        .filter(|candidate| selected_nodes.contains(&candidate.id))
                        .map(|candidate| NodePosition {
                            id: candidate.id.clone(),
                            position: candidate.position,
                        })
                        .collect::<Vec<_>>();
                    if let Some(pointer) = response.interact_pointer_pos() {
                        *drag = Some(DragState {
                            active_node: node.id.clone(),
                            pointer_origin: pointer,
                            initial_positions,
                        });
                    }
                }
                let drag_stopped = drag
                    .as_ref()
                    .is_some_and(|drag| drag.active_node == node.id)
                    && response.drag_stopped();
                if drag_stopped {
                    if actions_allowed
                        && graph_is_current
                        && pending.is_none()
                        && let Some(drag_state) = drag.as_ref()
                    {
                        let pointer = response
                            .interact_pointer_pos()
                            .or_else(|| ui.ctx().input(|input| input.pointer.interact_pos()));
                        if let Some(pointer) = pointer {
                            let delta = ScenePoint {
                                x: f64::from(pointer.x - drag_state.pointer_origin.x)
                                    / f64::from(camera.zoom),
                                y: f64::from(pointer.y - drag_state.pointer_origin.y)
                                    / f64::from(camera.zoom),
                            };
                            match moved_positions(&drag_state.initial_positions, delta) {
                                Ok(positions) => {
                                    *pending = Some(PendingMutation::Transaction {
                                        operations: vec![GraphOperation::MoveNodes { positions }],
                                        expected_revision: revision,
                                        select_after: None,
                                    });
                                }
                                Err(error) => *message = Some(error),
                            }
                        }
                    }
                    *drag = None;
                }
            }
        }
    } else {
        camera.fit_next = false;
        let message = if graph.is_none() {
            "No preview is available. Fix the Mermaid source to create a graph."
        } else {
            "The graph has no drawable nodes yet. Add a node to begin."
        };
        painter.text(
            canvas_rect.center(),
            egui::Align2::CENTER_CENTER,
            message,
            egui::FontId::proportional(14.0),
            egui::Color32::from_rgb(88, 95, 105),
        );
    }

    if actions_allowed && canvas_response.clicked() {
        *canvas_focus_requested = Some(canvas_id);
        if !canvas_response.drag_started() && !entity_selected {
            *selection = None;
            selected_nodes.clear();
        }
    }
    if actions_allowed {
        canvas_shortcuts(
            ui,
            canvas_id,
            graph,
            revision,
            selection,
            selected_nodes,
            pending,
            message,
        );
    }
    let _ = graph_is_current;
    canvas_rect
}

fn fit_camera(camera: &mut Camera, bounds: SceneRect, canvas_rect: egui::Rect) {
    let usable = canvas_rect.size() - egui::vec2(48.0, 48.0);
    let width = bounds.width().max(1.0) as f32;
    let height = bounds.height().max(1.0) as f32;
    camera.center = bounds.center();
    camera.zoom = (usable.x / width)
        .min(usable.y / height)
        .clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
    camera.fit_next = false;
}

fn world_to_screen(point: ScenePoint, canvas: egui::Rect, camera: Camera) -> egui::Pos2 {
    egui::pos2(
        canvas.center().x + ((point.x - camera.center.x) as f32 * camera.zoom),
        canvas.center().y + ((point.y - camera.center.y) as f32 * camera.zoom),
    )
}

fn world_rect_to_screen(rect: SceneRect, canvas: egui::Rect, camera: Camera) -> egui::Rect {
    egui::Rect::from_min_max(
        world_to_screen(rect.min, canvas, camera),
        world_to_screen(rect.max, canvas, camera),
    )
}

fn translate_rect(rect: SceneRect, delta: ScenePoint) -> SceneRect {
    SceneRect {
        min: ScenePoint {
            x: rect.min.x + delta.x,
            y: rect.min.y + delta.y,
        },
        max: ScenePoint {
            x: rect.max.x + delta.x,
            y: rect.max.y + delta.y,
        },
    }
}

fn cubic_points(
    curve: crate::diagram::CubicBezier,
    canvas: egui::Rect,
    camera: Camera,
    segments: usize,
) -> Vec<egui::Pos2> {
    let mut points = Vec::with_capacity(segments.saturating_add(1));
    for index in 0..=segments {
        let t = index as f64 / segments.max(1) as f64;
        let inverse = 1.0 - t;
        let point = ScenePoint {
            x: curve.start.x * inverse.powi(3)
                + 3.0 * curve.control1.x * inverse.powi(2) * t
                + 3.0 * curve.control2.x * inverse * t.powi(2)
                + curve.end.x * t.powi(3),
            y: curve.start.y * inverse.powi(3)
                + 3.0 * curve.control1.y * inverse.powi(2) * t
                + 3.0 * curve.control2.y * inverse * t.powi(2)
                + curve.end.y * t.powi(3),
        };
        points.push(world_to_screen(point, canvas, camera));
    }
    points
}

fn points_bounds(points: &[egui::Pos2]) -> Option<egui::Rect> {
    let first = *points.first()?;
    let mut min = first;
    let mut max = first;
    for point in points.iter().skip(1) {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    Some(egui::Rect::from_min_max(min, max))
}

fn draw_prepared_text(
    painter: &egui::Painter,
    bounds: egui::Rect,
    text: &PreparedText,
    alignment: TextHorizontalAlignment,
    color: egui::Color32,
    metrics: SceneMetrics,
    zoom: f32,
) {
    let inherited_clip = painter.clip_rect();
    for (index, line) in text.lines.iter().enumerate() {
        let width = (line.reserved_width as f32 * zoom).max(0.0);
        let height = (text.line_height as f32 * zoom).max(0.0);
        let x = match alignment {
            TextHorizontalAlignment::Center => bounds.center().x - width / 2.0,
            TextHorizontalAlignment::Leading => bounds.left(),
        };
        let y = bounds.top() + index as f32 * height;
        let line_clip = prepared_text_line_clip(
            egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(width, height)),
            bounds,
            inherited_clip,
        );
        if !line_clip.is_positive() {
            continue;
        }
        let font_size = ((metrics.text.max_scalar_advance as f32 * zoom) * 0.78).max(1.0);
        let galley = painter.layout_no_wrap(
            line.text.clone(),
            egui::FontId::proportional(font_size),
            color,
        );
        painter
            .with_clip_rect(line_clip)
            .galley(egui::pos2(x, y), galley, color);
    }
}

fn prepared_text_line_clip(
    line: egui::Rect,
    bounds: egui::Rect,
    inherited_clip: egui::Rect,
) -> egui::Rect {
    line.intersect(bounds).intersect(inherited_clip)
}

fn moved_positions(
    original: &[NodePosition],
    delta: ScenePoint,
) -> Result<Vec<NodePosition>, String> {
    let delta_x = snap_delta(delta.x);
    let delta_y = snap_delta(delta.y);
    original
        .iter()
        .map(|target| {
            let position = Position::new(target.position.x + delta_x, target.position.y + delta_y)
                .map_err(|error| {
                    format!("Move would leave the supported coordinate range: {error}")
                })?;
            Ok(NodePosition {
                id: target.id.clone(),
                position,
            })
        })
        .collect()
}

fn snap_delta(value: f64) -> f32 {
    ((value / f64::from(GRID_SPACING)).round() * f64::from(GRID_SPACING)) as f32
}

fn snap_world(value: f64) -> f32 {
    let snapped = (value / f64::from(GRID_SPACING)).round() * f64::from(GRID_SPACING);
    snapped.clamp(
        -f64::from(crate::diagram::MAX_POSITION_ABS),
        f64::from(crate::diagram::MAX_POSITION_ABS),
    ) as f32
}

fn take_history_shortcut(ui: &mut egui::Ui) -> Option<PendingMutation> {
    ui.input_mut(|input| {
        if input.consume_key(
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::Z,
        ) {
            Some(PendingMutation::Redo)
        } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y) {
            Some(PendingMutation::Redo)
        } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z) {
            Some(PendingMutation::Undo)
        } else {
            None
        }
    })
}

fn canvas_shortcuts(
    ui: &mut egui::Ui,
    canvas_id: egui::Id,
    graph: Option<&DiagramDocument>,
    revision: u64,
    selection: &mut Option<DiagramElement>,
    selected_nodes: &mut BTreeSet<NodeId>,
    pending: &mut Option<PendingMutation>,
    message: &mut Option<String>,
) {
    let canvas_focused = ui.memory(|memory| memory.has_focus(canvas_id));
    if !canvas_focused {
        return;
    }
    enum Action {
        SelectAll,
        Remove,
        Move(i32, i32),
    }
    let action = ui.input_mut(|input| {
        if input.consume_key(egui::Modifiers::COMMAND, egui::Key::A) {
            return Some(Action::SelectAll);
        }
        if input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
            || input.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)
        {
            return Some(Action::Remove);
        }
        let modifiers = input.modifiers;
        let distance = if modifiers.shift { 40 } else { 10 };
        if input.consume_key(modifiers, egui::Key::ArrowLeft) {
            return Some(Action::Move(-distance, 0));
        }
        if input.consume_key(modifiers, egui::Key::ArrowRight) {
            return Some(Action::Move(distance, 0));
        }
        if input.consume_key(modifiers, egui::Key::ArrowUp) {
            return Some(Action::Move(0, -distance));
        }
        if input.consume_key(modifiers, egui::Key::ArrowDown) {
            return Some(Action::Move(0, distance));
        }
        None
    });
    match action {
        Some(Action::SelectAll) => {
            if let Some(graph) = graph {
                selected_nodes.clear();
                selected_nodes.extend(graph.nodes.iter().map(|node| node.id.clone()));
                *selection = selected_nodes.last().cloned().map(DiagramElement::Node);
            }
        }
        Some(Action::Remove) if pending.is_none() => {
            if graph.is_some() {
                let operations = removal_operations(selection.as_ref(), selected_nodes);
                if !operations.is_empty() {
                    *pending = Some(PendingMutation::Transaction {
                        operations,
                        expected_revision: revision,
                        select_after: None,
                    });
                }
            }
        }
        Some(Action::Move(dx, dy)) if pending.is_none() && !selected_nodes.is_empty() => {
            if let Some(graph) = graph {
                let initial = graph
                    .nodes
                    .iter()
                    .filter(|node| selected_nodes.contains(&node.id))
                    .map(|node| NodePosition {
                        id: node.id.clone(),
                        position: node.position,
                    })
                    .collect::<Vec<_>>();
                if !initial.is_empty() {
                    match moved_positions(
                        &initial,
                        ScenePoint {
                            x: f64::from(dx),
                            y: f64::from(dy),
                        },
                    ) {
                        Ok(positions) => {
                            *pending = Some(PendingMutation::Transaction {
                                operations: vec![GraphOperation::MoveNodes { positions }],
                                expected_revision: revision,
                                select_after: None,
                            });
                        }
                        Err(error) => *message = Some(error),
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    const TEST_VIEWPORT_SIZE: egui::Vec2 = egui::vec2(1200.0, 900.0);

    fn frame(dialog: &mut DiagramStudioDialog, ctx: &egui::Context) {
        super::super::render::with_immediate_viewport_renderer(ctx, || {
            let raw_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    TEST_VIEWPORT_SIZE,
                )),
                focused: true,
                ..Default::default()
            };
            let output = ctx.run_ui(raw_input, |ui| {
                let first_pass = ui.ctx().current_pass_index() == 0;
                dialog.show(ui.ctx(), first_pass, true, true);
            });
            output.drop_without_applying_deltas();
            dialog.on_ui_frame_complete(ctx);
        });
    }

    fn frame_with_child_events(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) {
        super::super::render::with_immediate_viewport_test_events(events, || frame(dialog, ctx));
    }

    fn embedded_frame_with_events(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        root_input_allowed: bool,
    ) {
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                TEST_VIEWPORT_SIZE,
            )),
            events,
            focused: true,
            ..Default::default()
        };
        let output = ctx.run_ui(raw_input, |ui| {
            let first_pass = ui.ctx().current_pass_index() == 0;
            dialog.show(ui.ctx(), first_pass, true, root_input_allowed);
        });
        output.drop_without_applying_deltas();
        dialog.on_ui_frame_complete(ctx);
    }

    fn frame_with_root_discard(dialog: &mut DiagramStudioDialog, ctx: &egui::Context) {
        super::super::render::with_immediate_viewport_renderer(ctx, || {
            let raw_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    TEST_VIEWPORT_SIZE,
                )),
                focused: true,
                ..Default::default()
            };
            let output = ctx.run_ui(raw_input, |ui| {
                let first_pass = ui.ctx().current_pass_index() == 0;
                dialog.show(ui.ctx(), first_pass, true, true);
                if first_pass {
                    ui.ctx()
                        .request_discard("Diagram Studio root pass test discard");
                }
            });
            output.drop_without_applying_deltas();
            dialog.on_ui_frame_complete(ctx);
        });
    }

    fn initialized_dialog() -> DiagramStudioDialog {
        let mut dialog = DiagramStudioDialog::default();
        dialog.test_replace_source("flowchart TD\n    A[\"Alpha\"]");
        dialog.open();
        dialog
    }

    fn current_node_count(dialog: &DiagramStudioDialog) -> usize {
        dialog
            .test_session()
            .current_graph()
            .expect("test source remains valid")
            .nodes
            .len()
    }

    fn action_center(dialog: &DiagramStudioDialog, name: &str) -> egui::Pos2 {
        dialog
            .test_action_rects
            .iter()
            .find(|(action, _, _)| action == name)
            .map(|(_, rect, clip)| {
                assert!(clip.contains_rect(*rect));
                rect.center()
            })
            .expect("embedded visual toolbar exposes Add node")
    }

    #[test]
    fn diagram_studio_non_discarded_child_pass_commits_visual_transaction() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let revision = dialog.test_session().revision();
        dialog.test_stage_add_node();
        frame(&mut dialog, &ctx);

        assert!(matches!(
            dialog.test_last_callback().1,
            Some(egui::ViewportClass::Immediate)
        ));
        assert_eq!(dialog.test_session().revision(), revision + 1);
        assert_eq!(current_node_count(&dialog), 2);
    }

    #[test]
    fn diagram_studio_child_discard_does_not_commit_visual_transaction() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let revision = dialog.test_session().revision();
        dialog.test_stage_add_node();
        dialog.test_request_child_discard();
        frame(&mut dialog, &ctx);

        assert!(dialog.test_child_discard_requested);
        assert!(matches!(
            dialog.test_last_callback().1,
            Some(egui::ViewportClass::Immediate)
        ));
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(current_node_count(&dialog), 1);
    }

    #[test]
    fn diagram_studio_root_discard_after_show_drops_staged_visual_transaction() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let revision = dialog.test_session().revision();
        dialog.test_stage_add_node();

        frame_with_root_discard(&mut dialog, &ctx);

        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(current_node_count(&dialog), 1);
    }

    #[test]
    fn diagram_studio_root_discard_does_not_publish_child_focus_intent() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();

        frame_with_root_discard(&mut dialog, &ctx);

        assert!(dialog.focus_source);
        assert!(dialog.needs_viewport_bootstrap());
        assert!(dialog.deferred_source_focus.is_none());
        frame(&mut dialog, &ctx);
        assert!(!dialog.needs_viewport_bootstrap());
        assert!(dialog.deferred_source_focus.is_some());
        frame(&mut dialog, &ctx);
        assert!(dialog.test_source_editor_has_focus(&ctx));
    }

    #[test]
    fn diagram_studio_embedded_fallback_uses_parent_surface_without_claiming_child_identity() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(true);

        // The first embedded Window pass only measures its initial geometry.
        // Settle it with parent input disabled before testing the visible UI.
        for _ in 0..3 {
            embedded_frame_with_events(&mut dialog, &ctx, Vec::new(), false);
        }

        let (callback_id, callback_class, body_rendered, _) = dialog.test_last_callback();
        assert_eq!(callback_id, Some(egui::ViewportId::ROOT));
        assert!(matches!(
            callback_class,
            Some(egui::ViewportClass::EmbeddedWindow)
        ));
        assert!(body_rendered);
        assert!(dialog.test_child_focused);
        assert!(!dialog.needs_viewport_bootstrap());
        assert_eq!(current_node_count(&dialog), 1);

        let click = action_center(&dialog, "add_node");
        let pointer_events = |pressed| {
            vec![
                egui::Event::PointerMoved(click),
                egui::Event::PointerButton {
                    pos: click,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let revision = dialog.test_session().revision();
        embedded_frame_with_events(&mut dialog, &ctx, pointer_events(true), false);
        embedded_frame_with_events(&mut dialog, &ctx, pointer_events(false), false);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(current_node_count(&dialog), 1);

        // Re-publish enabled hit records under the authorized, focused parent.
        embedded_frame_with_events(&mut dialog, &ctx, Vec::new(), true);
        assert!(dialog.test_child_focused);
        let click = action_center(&dialog, "add_node");
        let pointer_events = |pressed| {
            vec![
                egui::Event::PointerMoved(click),
                egui::Event::PointerButton {
                    pos: click,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        embedded_frame_with_events(&mut dialog, &ctx, pointer_events(true), true);
        embedded_frame_with_events(&mut dialog, &ctx, pointer_events(false), true);
        let (callback_id, callback_class, body_rendered, _) = dialog.test_last_callback();
        assert_eq!(callback_id, Some(egui::ViewportId::ROOT));
        assert!(matches!(
            callback_class,
            Some(egui::ViewportClass::EmbeddedWindow)
        ));
        assert!(body_rendered);
        assert_eq!(dialog.test_session().revision(), revision + 1);
        assert_eq!(current_node_count(&dialog), 2);
    }

    #[test]
    fn diagram_studio_close_pending_waits_for_an_accepted_root_pass() {
        let mut dialog = DiagramStudioDialog::default();
        dialog.close_pending = true;
        let ctx = egui::Context::default();
        let raw_input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                TEST_VIEWPORT_SIZE,
            )),
            focused: true,
            ..Default::default()
        };

        let output = ctx.run_ui(raw_input(), |ui| dialog.show(ui.ctx(), false, true, true));
        output.drop_without_applying_deltas();
        dialog.on_ui_frame_complete(&ctx);
        assert!(dialog.close_pending);

        let output = ctx.run_ui(raw_input(), |ui| {
            dialog.show(ui.ctx(), ui.ctx().current_pass_index() == 0, true, true);
            if ui.ctx().current_pass_index() == 0 {
                ui.ctx()
                    .request_discard("Diagram Studio close root discard test");
            }
        });
        output.drop_without_applying_deltas();
        dialog.on_ui_frame_complete(&ctx);
        assert!(dialog.close_pending);

        let output = ctx.run_ui(raw_input(), |ui| dialog.show(ui.ctx(), true, true, true));
        output.drop_without_applying_deltas();
        dialog.on_ui_frame_complete(&ctx);
        assert!(!dialog.close_pending);
    }

    #[test]
    fn diagram_studio_new_nodes_start_at_the_view_then_search_outward() {
        let graph = DiagramDocument::default();
        let center = ScenePoint {
            x: 240.0,
            y: -120.0,
        };
        let (_, first) = next_node(&graph, center).unwrap();
        assert_eq!(first, Position::new(240.0, -120.0).unwrap());

        let mut occupied = graph;
        occupied.nodes.push(crate::diagram::DiagramNode {
            id: NodeId::new("Existing").unwrap(),
            label: "Existing".into(),
            shape: NodeShape::Rectangle,
            position: first,
            group: None,
        });
        let (_, second) = next_node(&occupied, center).unwrap();
        assert_ne!(second, first);
        assert!(
            f64::from(second.x - first.x).hypot(f64::from(second.y - first.y))
                >= DRAG_NODE_SEPARATION
        );
    }

    #[test]
    fn diagram_studio_minimum_width_stacks_split_and_keeps_visual_controls_reachable() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        let size = egui::vec2(480.0, 360.0);
        dialog.test_begin_frame();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                focused: true,
                ..Default::default()
            },
            |ui| {
                let mut pending = None;
                let mut canvas_focus = None;
                let mut source_focus = None;
                dialog.draw_body(
                    ui,
                    true,
                    &mut pending,
                    &mut canvas_focus,
                    &mut source_focus,
                    true,
                );
            },
        );
        output.drop_without_applying_deltas();

        let canvas = dialog
            .test_canvas_rect
            .expect("Split draws its visual canvas");
        assert!(
            canvas.width() >= 220.0,
            "canvas geometry is too narrow: {canvas:?}"
        );
        assert!(canvas.left() >= 0.0 && canvas.right() <= size.x);
        let (_, add_node, clip) = dialog
            .test_action_rects
            .iter()
            .find(|(name, _, _)| name == "add_node")
            .expect("the visual toolbar exposes Add node");
        assert!(clip.left() <= add_node.left() && clip.right() >= add_node.right());
        assert!(add_node.left() >= 0.0 && add_node.right() <= size.x);
    }

    #[test]
    fn diagram_studio_default_split_keeps_canvas_visible_beside_inspector() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        let size = egui::vec2(1180.0, 760.0);
        dialog.test_begin_frame();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                focused: true,
                ..Default::default()
            },
            |ui| {
                let mut pending = None;
                let mut canvas_focus = None;
                let mut source_focus = None;
                dialog.draw_body(
                    ui,
                    true,
                    &mut pending,
                    &mut canvas_focus,
                    &mut source_focus,
                    true,
                );
            },
        );
        output.drop_without_applying_deltas();

        let canvas = dialog
            .test_canvas_rect
            .expect("default Split mode renders a canvas");
        let visible = canvas.intersect(egui::Rect::from_min_size(egui::Pos2::ZERO, size));
        assert!(canvas.width() >= 220.0, "canvas is too narrow: {canvas:?}");
        assert!(
            visible.height() >= 220.0,
            "the default window shows a useful canvas region: canvas={canvas:?}, visible={visible:?}"
        );
    }

    #[test]
    fn diagram_studio_prepared_text_respects_canvas_and_parent_clips() {
        let parent = egui::Rect::from_min_max(egui::pos2(10.0, 10.0), egui::pos2(100.0, 100.0));
        let canvas = egui::Rect::from_min_max(egui::pos2(50.0, 0.0), egui::pos2(120.0, 80.0));
        let line = egui::Rect::from_min_max(egui::pos2(0.0, 40.0), egui::pos2(90.0, 90.0));
        assert_eq!(
            prepared_text_line_clip(line, canvas, parent),
            egui::Rect::from_min_max(egui::pos2(50.0, 40.0), egui::pos2(90.0, 80.0))
        );
    }

    #[test]
    fn diagram_studio_group_moves_preserve_spacing_and_ten_unit_nudges() {
        let initial = [
            NodePosition {
                id: NodeId::new("A").unwrap(),
                position: Position::new(13.0, 21.0).unwrap(),
            },
            NodePosition {
                id: NodeId::new("B").unwrap(),
                position: Position::new(190.0, 21.0).unwrap(),
            },
        ];
        let moved = moved_positions(&initial, ScenePoint { x: 10.0, y: 0.0 }).unwrap();
        assert_eq!(moved[0].position, Position::new(23.0, 21.0).unwrap());
        assert_eq!(moved[1].position, Position::new(200.0, 21.0).unwrap());
        assert_eq!(
            moved[1].position.x - moved[0].position.x,
            initial[1].position.x - initial[0].position.x
        );
    }

    #[test]
    fn diagram_studio_canvas_selection_focus_enables_canvas_delete_shortcut() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let scene = dialog
            .cache
            .as_ref()
            .and_then(|cache| cache.scene.as_ref())
            .expect("valid source produces a canvas scene");
        let node = scene.nodes.first().expect("fixture contains a node");
        let canvas = dialog.test_canvas_rect.expect("canvas was rendered");
        let click = world_rect_to_screen(node.bounds, canvas, dialog.camera).center();

        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(click),
                egui::Event::PointerButton {
                    pos: click,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(click),
                egui::Event::PointerButton {
                    pos: click,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(matches!(dialog.selection, Some(DiagramElement::Node(_))));
        frame(&mut dialog, &ctx);
        assert!(dialog.test_canvas_has_focus());

        let revision = dialog.test_session().revision();
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(dialog.test_session().revision(), revision + 1);
        assert_eq!(current_node_count(&dialog), 0);
    }

    #[test]
    fn diagram_studio_invalid_source_retains_last_good_preview_and_blocks_visual_edits() {
        let mut dialog = initialized_dialog();
        dialog.test_replace_source("flowchart TD\n    A -->");
        let revision = dialog.session.revision();
        let invalid_source = dialog.session.draft().to_owned();
        let last_good_nodes = dialog.session.preview_graph().unwrap().nodes.len();
        assert_eq!(dialog.session.status().preview, PreviewState::Stale);

        let id = NodeId::new("Blocked").unwrap();
        dialog.apply_pending(PendingMutation::Transaction {
            operations: vec![GraphOperation::AddNode {
                id,
                label: "Blocked".into(),
                shape: NodeShape::Rectangle,
                position: Position::new(900.0, 900.0).unwrap(),
                group: None,
            }],
            expected_revision: revision,
            select_after: None,
        });

        assert_eq!(dialog.session.revision(), revision);
        assert_eq!(dialog.session.draft(), invalid_source);
        assert_eq!(
            dialog.session.preview_graph().unwrap().nodes.len(),
            last_good_nodes
        );
        assert!(
            dialog
                .message
                .as_deref()
                .is_some_and(|message| message.contains("invalid"))
        );
    }

    #[test]
    fn diagram_studio_stale_property_basis_cannot_overwrite_new_source() {
        let mut dialog = initialized_dialog();
        let old_revision = dialog.test_session().revision();
        dialog.test_replace_source("flowchart TD\n    A[\"Updated source\"]");
        let updated_revision = dialog.test_session().revision();

        dialog.apply_pending(PendingMutation::Transaction {
            operations: vec![GraphOperation::SetNodeLabel {
                id: NodeId::new("A").unwrap(),
                label: "Stale property draft".into(),
            }],
            expected_revision: old_revision,
            select_after: Some(DiagramElement::Node(NodeId::new("A").unwrap())),
        });

        let graph = dialog
            .test_session()
            .current_graph()
            .expect("updated source remains valid");
        assert_eq!(graph.nodes[0].label, "Updated source");
        assert_eq!(dialog.test_session().revision(), updated_revision);
        assert!(dialog
            .message
            .as_deref()
            .is_some_and(|message| message.contains("changed while these properties were open")));
    }
}
