use super::PendingCellDrop;
use super::canvas::{
    CanvasPoint, CanvasTransform, DesignerMode, DragPayload, PlacementDraft,
    ProjectedCellProvenance, ProjectedSelection, VisitedMenuPath,
};
use crate::radial::authoring::StableSelection;
use crate::radial::authoring::{AuthoringClient, AuthoringSessionId, RadialAuthoringSession};
use crate::radial::compositor::CompositorCache;
use crate::radial::geometry::{
    HitShape, LogicalPoint, LogicalRect, PhysicalPoint, PhysicalRect, ScaleFactor, cascade_layout,
    layout_document_menu, shape_center,
};
use crate::radial::model::{
    CellContent, CellId, InvocationId, MenuDefinition, MenuId, RadialDocument, RingId, SessionId,
    SkinId,
};
use crate::radial::preparation::{
    PreparedFrameInput, PreparedPlacement, PreviewPlacement, ensure_preview_center_back,
};
use crate::radial::render::{build_scene_prepared_selected, build_scene_prepared_selected_tooltip};
use crate::radial::session::{CellRole, FrameId, SessionEvent, SessionIntent, SessionReducer};
use crate::radial::tooltip::{
    TooltipHoverState, TooltipIdentity, TooltipPreferences, monotonic_ms,
};
use eframe::egui;
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PreviewPreset {
    #[default]
    Current,
    OneRing,
    MultiRing,
    Submenu,
    LongLabels,
    HighDpi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DesignDragDisposition {
    Authored,
    Center,
    ReadOnly,
    Pan,
}

fn design_drag_disposition(
    hovered: Option<&ProjectedCellProvenance>,
    center_hit: bool,
) -> DesignDragDisposition {
    match hovered {
        Some(ProjectedCellProvenance::Authored { .. }) => DesignDragDisposition::Authored,
        Some(ProjectedCellProvenance::Dynamic { .. }) | Some(ProjectedCellProvenance::Control) => {
            DesignDragDisposition::ReadOnly
        }
        Some(ProjectedCellProvenance::Center) if center_hit => DesignDragDisposition::Center,
        Some(_) => DesignDragDisposition::Pan,
        None if center_hit => DesignDragDisposition::Center,
        None => DesignDragDisposition::Pan,
    }
}

fn empty_authored_slots(menu: &MenuDefinition) -> Vec<(RingId, CellId)> {
    menu.rings
        .iter()
        .flat_map(|ring| {
            ring.cells
                .iter()
                .filter(|cell| matches!(&cell.content, CellContent::Spacer))
                .map(|cell| (ring.id.clone(), cell.id.clone()))
        })
        .collect()
}

/// A Preview/Test dispatch is always consumed locally, but retaining its
/// typed projection identity keeps the read-only surface honest: a generated
/// result can be inspected as the exact authored source/result that produced
/// it without pretending that its synthetic CellId is part of the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InterceptedDynamicDispatch {
    pub(super) generated_cell_id: CellId,
    pub(super) provenance: crate::radial::bindings::ProjectedDynamicProvenance,
}

pub(super) struct EmbeddedPreview {
    reducer: Option<SessionReducer>,
    root: Option<MenuId>,
    document_generation: u64,
    selection_token: String,
    frame_token: String,
    failed_frame_token: Option<String>,
    preparation_notice: Option<super::ResourceNotice>,
    compositor: CompositorCache,
    texture: Option<egui::TextureHandle>,
    pub(super) intercepted_dispatches: usize,
    pub(super) last_intercepted_dynamic_dispatch: Option<InterceptedDynamicDispatch>,
    pub(super) simulated_drags: usize,
    drag_cell: Option<CellId>,
    navigation_frames: BTreeMap<FrameId, (String, std::sync::Arc<PreparedFrameInput>)>,
    pending_frame_id: Option<FrameId>,
    pending_frame_token: Option<String>,
    frozen_center: Option<PhysicalPoint>,
    frozen_scale: Option<ScaleFactor>,
    tooltip_preferences: TooltipPreferences,
    tooltip_hover: TooltipHoverState,
    hovered_cell: Option<CellId>,
    #[cfg(test)]
    preparation_attempts: usize,
}

impl Default for EmbeddedPreview {
    fn default() -> Self {
        Self {
            reducer: None,
            root: None,
            document_generation: 0,
            selection_token: String::new(),
            frame_token: String::new(),
            failed_frame_token: None,
            preparation_notice: None,
            compositor: CompositorCache::default(),
            texture: None,
            intercepted_dispatches: 0,
            last_intercepted_dynamic_dispatch: None,
            simulated_drags: 0,
            drag_cell: None,
            navigation_frames: BTreeMap::new(),
            pending_frame_id: None,
            pending_frame_token: None,
            frozen_center: None,
            frozen_scale: None,
            tooltip_preferences: TooltipPreferences::default(),
            tooltip_hover: TooltipHoverState::default(),
            hovered_cell: None,
            #[cfg(test)]
            preparation_attempts: 0,
        }
    }
}

impl EmbeddedPreview {
    pub(super) fn cancel_tooltip(&mut self) {
        self.tooltip_hover.cancel();
        self.hovered_cell = None;
    }

    /// Drop all prepared navigation state when the owning viewport closes.
    /// A late service reply is generation/session checked by authoring, but
    /// clearing this local cache as well prevents a hidden viewport from
    /// resurrecting an old prepared frame on a subsequent open.
    pub(super) fn dispose(&mut self) {
        self.cancel_tooltip();
        self.reducer = None;
        self.root = None;
        self.document_generation = 0;
        self.selection_token.clear();
        self.frame_token.clear();
        self.failed_frame_token = None;
        self.preparation_notice = None;
        self.last_intercepted_dynamic_dispatch = None;
        self.navigation_frames.clear();
        self.pending_frame_id = None;
        self.pending_frame_token = None;
        self.frozen_center = None;
        self.frozen_scale = None;
        self.texture = None;
        self.drag_cell = None;
    }

    pub(super) fn prepared_frame(
        &self,
        session: &RadialAuthoringSession,
    ) -> Option<std::sync::Arc<PreparedFrameInput>> {
        let frame_id = self.reducer.as_ref()?.state.stack.last()?.frame_id;
        let cached = self
            .navigation_frames
            .get(&frame_id)
            .filter(|(token, input)| {
                token == &self.frame_token
                    && self
                        .reducer
                        .as_ref()
                        .and_then(|reducer| reducer.state.stack.last())
                        .is_some_and(|frame| frame.page == input.page)
            })
            .map(|(_, input)| std::sync::Arc::clone(input));
        cached.or_else(|| {
            (frame_id == FrameId(1))
                .then(|| {
                    session
                        .embedded_preview
                        .as_ref()
                        .filter(|(token, _)| token == &self.frame_token)
                        .map(|(_, input)| std::sync::Arc::clone(input))
                })
                .flatten()
        })
    }

    pub(super) fn sync_preparation(
        &mut self,
        session: &mut RadialAuthoringSession,
        client: Option<&AuthoringClient>,
        preset: PreviewPreset,
        selection: Option<&StableSelection>,
        tooltip_preferences: TooltipPreferences,
    ) {
        if self.tooltip_preferences != tooltip_preferences {
            self.tooltip_preferences = tooltip_preferences;
            self.cancel_tooltip();
        }
        let selected_menu = match selection {
            Some(
                StableSelection::Menu(id)
                | StableSelection::Ring { menu_id: id, .. }
                | StableSelection::Cell { menu_id: id, .. },
            ) => Some(id.clone()),
            _ => None,
        };
        let selected_skin = match selection {
            Some(StableSelection::Skin(id)) => Some(id.clone()),
            _ => None,
        };
        let root = selected_menu
            .or_else(|| self.root.clone())
            .unwrap_or_else(|| session.draft.default_menu_id.clone());
        let document =
            representative_document(&session.draft, preset, &root, selected_skin.as_ref());
        let token = format!("{root:?}:{selected_skin:?}:{preset:?}");
        if self.reducer.is_none()
            || self.document_generation != session.generation.0
            || self.selection_token != token
        {
            self.reset(&document, &root, session.generation.0);
            self.selection_token = token;
        }
        let menu_id = self
            .current_menu()
            .cloned()
            .filter(|id| document.menus.iter().any(|menu| &menu.id == id))
            .unwrap_or(root);
        let selected = self
            .reducer
            .as_ref()
            .and_then(|reducer| reducer.state.selected.clone());
        let page = self
            .reducer
            .as_ref()
            .and_then(|reducer| reducer.state.stack.last())
            .map_or(0, |frame| frame.page);
        let current_frame = self
            .reducer
            .as_ref()
            .and_then(|reducer| reducer.state.stack.last())
            .cloned();
        let frame_token = format!(
            "{}:{menu_id}:{selected:?}:{selected_skin:?}:{page}:{preset:?}:assets={}:tooltips={:?}",
            session.generation.0,
            session.pending_assets.preview_identity(),
            tooltip_preferences,
        );
        if let (Some(pending_frame_id), Some(pending_token)) = (
            self.pending_frame_id.take(),
            self.pending_frame_token.take(),
        ) && let Some(input) = session
            .embedded_preview
            .as_ref()
            .filter(|(reply_token, _)| reply_token == &pending_token)
            .map(|(_, input)| std::sync::Arc::clone(input))
        {
            let mut input = (*input).clone();
            if pending_frame_id == FrameId(1) {
                self.frozen_center = Some(input.layout.origin);
                self.frozen_scale = Some(input.layout.scale_factor);
            } else if let Some(frame) = current_frame
                .as_ref()
                .filter(|frame| frame.frame_id == pending_frame_id)
            {
                let menu = document.menus.iter().find(|menu| menu.id == frame.menu_id);
                if let Some(menu) = menu {
                    ensure_preview_center_back(&mut input.layout, menu);
                    let previous_placement = self
                        .navigation_frames
                        .get(&pending_frame_id)
                        .map(|(_, previous)| previous.placement);
                    let parent_menu = frame
                        .parent_frame_id
                        .and_then(|parent_id| {
                            self.reducer
                                .as_ref()?
                                .state
                                .stack
                                .iter()
                                .find(|candidate| candidate.frame_id == parent_id)
                        })
                        .and_then(|parent| {
                            document.menus.iter().find(|menu| menu.id == parent.menu_id)
                        });
                    let cascade_requested = parent_menu.is_some_and(|parent| {
                        parent.submenu_presentation
                            == crate::radial::model::SubmenuPresentation::Cascade
                    });
                    let should_cascade = cascade_requested
                        && previous_placement != Some(PreparedPlacement::SameCenterFallback)
                        && (previous_placement == Some(PreparedPlacement::Cascade)
                            || input.placement == PreparedPlacement::Cascade);
                    if should_cascade
                        && let Some(parent_frame_id) = frame.parent_frame_id
                        && let Some((_, parent)) = self.navigation_frames.get(&parent_frame_id)
                    {
                        input.resources =
                            crate::radial::authoring::native_preview::merge_preview_resources(
                                &parent.resources,
                                &input.resources,
                            );
                        input.layout = cascade_layout(&parent.layout, input.layout);
                        input.placement = PreparedPlacement::Cascade;
                    } else if cascade_requested
                        && (input.placement == PreparedPlacement::SameCenterFallback
                            || previous_placement == Some(PreparedPlacement::SameCenterFallback))
                    {
                        input.placement = PreparedPlacement::SameCenterFallback;
                    }
                    let selected = frame.selected.as_ref();
                    input.scene = build_scene_prepared_selected(
                        &input.layout,
                        session.generation.0,
                        &input.resources,
                        selected,
                    );
                    if let Some(current) = self
                        .reducer
                        .as_mut()
                        .and_then(|reducer| reducer.state.stack.last_mut())
                        .filter(|current| current.frame_id == pending_frame_id)
                    {
                        current.origin = input.layout.origin;
                        current.page = input.page;
                        current.page_count = input.page_count;
                        current.scale_factor = input.layout.scale_factor.get();
                    }
                }
            }
            let input = std::sync::Arc::new(input);
            self.navigation_frames.insert(
                pending_frame_id,
                (pending_token.clone(), std::sync::Arc::clone(&input)),
            );
            session.embedded_preview = Some((pending_token, input));
        }
        if let Some(frame) = current_frame.as_ref()
            && let Some((cached_token, cached)) = self.navigation_frames.get(&frame.frame_id)
            && cached_token == &frame_token
            && cached.page == page
        {
            self.frame_token = frame_token.clone();
            session.embedded_preview = Some((frame_token, std::sync::Arc::clone(cached)));
            if let Some(current) = self
                .reducer
                .as_mut()
                .and_then(|reducer| reducer.state.stack.last_mut())
            {
                current.page = cached.page;
                current.page_count = cached.page_count;
                current.origin = cached.layout.origin;
                current.scale_factor = cached.layout.scale_factor.get();
            }
            self.failed_frame_token = None;
            self.preparation_notice = None;
            return;
        }
        let synthetic_center = self
            .frozen_center
            .unwrap_or(PhysicalPoint { x: 240.0, y: 240.0 });
        self.frame_token = frame_token.clone();
        if current_frame.as_ref().map(|frame| frame.frame_id) == Some(FrameId(1))
            && let Some((token, input)) = session.embedded_preview.as_ref()
            && token == &frame_token
        {
            if let Some(frame) = self
                .reducer
                .as_mut()
                .and_then(|reducer| reducer.state.stack.last_mut())
            {
                frame.page = input.page;
                frame.page_count = input.page_count;
            }
            self.failed_frame_token = None;
            self.preparation_notice = None;
            return;
        }
        if self.failed_frame_token.as_deref() == Some(frame_token.as_str()) {
            return;
        }
        if self.failed_frame_token.is_some() {
            self.failed_frame_token = None;
            self.preparation_notice = None;
        }
        if session.pending_request.is_some() {
            return;
        }
        let Some(client) = client else { return };
        let scale = if preset == PreviewPreset::HighDpi {
            2.0
        } else {
            1.0
        };
        let scale = self.frozen_scale.map_or_else(
            || ScaleFactor::new(scale).unwrap(),
            |scale| {
                if preset == PreviewPreset::HighDpi {
                    ScaleFactor::new(2.0).unwrap()
                } else {
                    scale
                }
            },
        );
        let (anchor, placement) = self.preview_placement(
            &document,
            current_frame.as_ref(),
            &menu_id,
            synthetic_center,
            scale,
        );
        #[cfg(test)]
        {
            self.preparation_attempts += 1;
        }
        let mut request = session.request_embedded_preview_placed(
            std::sync::Arc::new(document),
            menu_id,
            selected,
            anchor,
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 480.0, y: 480.0 },
            },
            scale,
            frame_token.clone(),
            page,
            selected_skin,
            placement,
        );
        if let Ok(crate::radial::authoring::AuthoringRequest::PrepareEmbeddedPreview {
            projection,
            ..
        }) = &mut request
        {
            projection.tooltip_preferences = tooltip_preferences;
        }
        match request {
            Ok(request) => {
                let correlation = (request.id(), request.generation(), request.editor_session());
                self.pending_frame_id = current_frame.as_ref().map(|frame| frame.frame_id);
                self.pending_frame_token = Some(frame_token.clone());
                if let Err(error) = client.send(request) {
                    session.cancel_pending_request(correlation.0, correlation.1, correlation.2);
                    self.fail_preparation(
                        frame_token,
                        format!("preview service unavailable: {}", authoring_error(&error)),
                    );
                }
            }
            Err(error) => self.fail_preparation(
                frame_token,
                format!("preview preparation rejected: {}", authoring_error(&error)),
            ),
        }
    }

    fn preview_placement(
        &self,
        document: &RadialDocument,
        frame: Option<&crate::radial::session::MenuFrame>,
        menu_id: &MenuId,
        same_center: PhysicalPoint,
        scale: ScaleFactor,
    ) -> (PhysicalPoint, PreviewPlacement) {
        let Some(frame) = frame.filter(|frame| frame.parent_frame_id.is_some()) else {
            return (same_center, PreviewPlacement::FlexibleRoot);
        };
        if self.navigation_frames.contains_key(&frame.frame_id) {
            return (frame.origin, PreviewPlacement::FixedCenter);
        }
        let Some(parent_frame_id) = frame.parent_frame_id else {
            return (same_center, PreviewPlacement::FixedCenter);
        };
        let parent_menu = frame
            .parent_frame_id
            .and_then(|parent_id| {
                self.reducer
                    .as_ref()?
                    .state
                    .stack
                    .iter()
                    .find(|candidate| candidate.frame_id == parent_id)
            })
            .and_then(|parent| document.menus.iter().find(|menu| menu.id == parent.menu_id));
        let Some(parent_menu) = parent_menu else {
            return (same_center, PreviewPlacement::FixedCenter);
        };
        let parent_layout = self
            .navigation_frames
            .get(&parent_frame_id)
            .map(|(_, input)| &input.layout);
        let current_parent_center = parent_layout.map_or(same_center, |layout| layout.origin);
        if parent_menu.submenu_presentation == crate::radial::model::SubmenuPresentation::SameCenter
        {
            return (current_parent_center, PreviewPlacement::FixedCenter);
        }
        let cascade_center = parent_layout.and_then(|layout| {
            let submenu_cell = parent_menu
                .rings
                .iter()
                .flat_map(|ring| &ring.cells)
                .find(|cell| matches!(&cell.content, CellContent::Submenu { menu_id: target } if target == menu_id))?;
            let cell = layout.cells.iter().find(|cell| cell.cell_id == submenu_cell.id)?;
            Some(shape_center(&cell.shape, scale))
        });
        match cascade_center {
            Some(center) => (
                center,
                PreviewPlacement::Cascade {
                    fallback_center: current_parent_center,
                },
            ),
            None => (current_parent_center, PreviewPlacement::FixedCenter),
        }
    }

    fn fail_preparation(&mut self, fingerprint: String, message: String) {
        self.cancel_tooltip();
        self.failed_frame_token = Some(fingerprint);
        self.preparation_notice = Some(super::ResourceNotice::error(message));
        if let Some(reducer) = self.reducer.as_mut()
            && reducer.state.stack.len() > 1
        {
            let frame = reducer.state.stack.last().cloned();
            let pointer_baseline = frame.map_or(LogicalPoint { x: 240.0, y: 240.0 }, |frame| {
                let scale = frame.scale_factor.max(f64::EPSILON);
                LogicalPoint {
                    x: (frame.origin.x / scale) as f32,
                    y: (frame.origin.y / scale) as f32,
                }
            });
            reducer.reduce(SessionEvent::Back {
                geometry_generation: self.document_generation,
                pointer_baseline,
            });
            self.pending_frame_id = None;
            self.pending_frame_token = None;
        }
    }

    fn retry_preparation(&mut self) {
        self.failed_frame_token = None;
        self.preparation_notice = None;
    }

    pub(super) fn reset(&mut self, document: &RadialDocument, menu_id: &MenuId, generation: u64) {
        self.navigation_frames.clear();
        self.pending_frame_id = None;
        self.pending_frame_token = None;
        self.frozen_center = None;
        self.frozen_scale = None;
        self.cancel_tooltip();
        self.last_intercepted_dynamic_dispatch = None;
        let Some(menu) = document.menus.iter().find(|menu| &menu.id == menu_id) else {
            self.reducer = None;
            return;
        };
        self.root = Some(menu_id.clone());
        self.document_generation = generation;
        self.reducer = Some(SessionReducer::new(
            SessionId::new("editor-preview"),
            document.revision,
            menu_id.clone(),
            PhysicalPoint { x: 240.0, y: 240.0 },
            generation,
            menu.interaction,
            InvocationId(0),
            LogicalPoint { x: 240.0, y: 240.0 },
        ));
    }

    pub(super) fn current_menu(&self) -> Option<&MenuId> {
        self.reducer
            .as_ref()?
            .state
            .stack
            .last()
            .map(|frame| &frame.menu_id)
    }

    pub(super) fn back(&mut self) {
        self.cancel_tooltip();
        if let Some(reducer) = self.reducer.as_mut() {
            let frame = reducer.state.stack.last().cloned();
            let pointer_baseline = frame.map_or(LogicalPoint { x: 240.0, y: 240.0 }, |frame| {
                let scale = frame.scale_factor.max(f64::EPSILON);
                LogicalPoint {
                    x: (frame.origin.x / scale) as f32,
                    y: (frame.origin.y / scale) as f32,
                }
            });
            let _ = reducer.reduce(SessionEvent::Back {
                geometry_generation: self.document_generation,
                pointer_baseline,
            });
        }
    }

    pub(super) fn activate(
        &mut self,
        document: &RadialDocument,
        prepared: Option<&PreparedFrameInput>,
        cell_id: &CellId,
    ) {
        self.cancel_tooltip();
        if cell_id.as_str() == "__center"
            && self
                .reducer
                .as_ref()
                .is_some_and(|reducer| reducer.state.stack.len() > 1)
        {
            self.back();
            return;
        }
        let Some(menu_id) = self.current_menu().cloned() else {
            return;
        };
        let authored_cell = document
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| {
                menu.rings
                    .iter()
                    .flat_map(|ring| &ring.cells)
                    .find(|cell| &cell.id == cell_id)
            });
        // A projected dynamic result is intentionally absent from the
        // authored document. Use the prepared typed map for its action
        // semantics, while authored membership wins if an ID happens to
        // collide with a generated-looking value.
        let generated_provenance = authored_cell
            .is_none()
            .then(|| prepared.and_then(|input| input.provenance.get(cell_id)))
            .flatten()
            .cloned();
        let cell = authored_cell;
        let role = if generated_provenance.is_some() {
            CellRole::Action
        } else {
            cell_role(document, &menu_id, cell_id)
        };
        let submenu_center = cell.and_then(|cell| match &cell.content {
            CellContent::Submenu { menu_id: target } => {
                Some(self.requested_child_center(document, &menu_id, target))
            }
            _ => None,
        });
        let event = match cell.map(|cell| &cell.content) {
            Some(CellContent::Submenu { menu_id }) => SessionEvent::OpenChild {
                menu_id: menu_id.clone(),
                origin: submenu_center.unwrap_or(PhysicalPoint { x: 240.0, y: 240.0 }),
                geometry_generation: self.document_generation,
                pointer_baseline: LogicalPoint { x: 240.0, y: 240.0 },
            },
            Some(CellContent::Control {
                control: crate::radial::model::Control::Back,
            }) => SessionEvent::Back {
                geometry_generation: self.document_generation,
                pointer_baseline: LogicalPoint { x: 240.0, y: 240.0 },
            },
            _ if role == CellRole::Action => SessionEvent::ActivateItem {
                cell: cell_id.clone(),
                role,
                gesture: crate::radial::model::ClickGesture::Primary,
                source: crate::commands::ActivationSource::Click,
                geometry_generation: self.document_generation,
            },
            _ => {
                let point = LogicalPoint { x: 240.0, y: 240.0 };
                let reducer = self.reducer.as_mut().unwrap();
                let _ = reducer.reduce(SessionEvent::PointerDown {
                    point,
                    cell: Some(cell_id.clone()),
                    role,
                    button: crate::radial::session::PointerButton::Primary,
                    geometry_generation: self.document_generation,
                });
                SessionEvent::PointerUp {
                    point,
                    cell: Some(cell_id.clone()),
                    role,
                    button: crate::radial::session::PointerButton::Primary,
                    geometry_generation: self.document_generation,
                }
            }
        };
        let intents = self.reducer.as_mut().unwrap().reduce(event);
        for intent in intents {
            match intent {
                SessionIntent::Dispatch { cell_id, .. } => {
                    self.intercepted_dispatches += 1;
                    if let Some(provenance) = generated_provenance.clone() {
                        self.last_intercepted_dynamic_dispatch = Some(InterceptedDynamicDispatch {
                            generated_cell_id: cell_id,
                            provenance,
                        });
                    }
                }
                SessionIntent::OpenSubmenu { .. } => {
                    if let Some(CellContent::Submenu { menu_id }) = cell.map(|cell| &cell.content) {
                        let _ = self
                            .reducer
                            .as_mut()
                            .unwrap()
                            .reduce(SessionEvent::OpenChild {
                                menu_id: menu_id.clone(),
                                origin: submenu_center
                                    .unwrap_or(PhysicalPoint { x: 240.0, y: 240.0 }),
                                geometry_generation: self.document_generation,
                                pointer_baseline: LogicalPoint { x: 240.0, y: 240.0 },
                            });
                    }
                }
                SessionIntent::Back => {}
                SessionIntent::BeginNativeDrag { .. } => self.simulated_drags += 1,
                SessionIntent::CloseTree | SessionIntent::PageChanged { .. } => {}
            }
        }
    }

    fn requested_child_center(
        &self,
        document: &RadialDocument,
        parent_menu_id: &MenuId,
        child_menu_id: &MenuId,
    ) -> PhysicalPoint {
        let root_center = self
            .frozen_center
            .unwrap_or(PhysicalPoint { x: 240.0, y: 240.0 });
        let current_frame_id = self
            .reducer
            .as_ref()
            .and_then(|reducer| reducer.state.stack.last())
            .map(|frame| frame.frame_id);
        let same_center = current_frame_id
            .and_then(|frame_id| self.navigation_frames.get(&frame_id))
            .map_or(root_center, |(_, input)| input.layout.origin);
        let Some(parent) = document
            .menus
            .iter()
            .find(|menu| &menu.id == parent_menu_id)
        else {
            return same_center;
        };
        if parent.submenu_presentation == crate::radial::model::SubmenuPresentation::SameCenter {
            return same_center;
        }
        let Some(parent_frame_id) = self
            .reducer
            .as_ref()
            .and_then(|reducer| reducer.state.stack.last())
            .map(|frame| frame.frame_id)
        else {
            return same_center;
        };
        self.navigation_frames
            .get(&parent_frame_id)
            .and_then(|(_, input)| {
                let source_cell = parent
                    .rings
                    .iter()
                    .flat_map(|ring| &ring.cells)
                    .find(|cell| {
                        matches!(&cell.content, CellContent::Submenu { menu_id } if menu_id == child_menu_id)
                    })?;
                let cell = input
                    .layout
                    .cells
                    .iter()
                    .find(|cell| cell.cell_id == source_cell.id)?;
                Some(shape_center(&cell.shape, input.layout.scale_factor))
            })
            .unwrap_or(same_center)
    }

    fn begin_drag(
        &mut self,
        document: &RadialDocument,
        menu_id: &MenuId,
        cell_id: &CellId,
        point: LogicalPoint,
        generation: u64,
    ) {
        if cell_role(document, menu_id, cell_id) != CellRole::Drag {
            return;
        }
        self.drag_cell = Some(cell_id.clone());
        let _ = self
            .reducer
            .as_mut()
            .unwrap()
            .reduce(SessionEvent::PointerDown {
                point,
                cell: Some(cell_id.clone()),
                role: CellRole::Drag,
                button: crate::radial::session::PointerButton::Primary,
                geometry_generation: generation,
            });
    }

    fn continue_drag(&mut self, point: LogicalPoint, hovered: Option<CellId>, generation: u64) {
        if self.drag_cell.is_none() {
            return;
        }
        let intents = self
            .reducer
            .as_mut()
            .unwrap()
            .reduce(SessionEvent::PointerMoved {
                point,
                hovered,
                geometry_generation: generation,
            });
        self.simulated_drags += intents
            .iter()
            .filter(|intent| matches!(intent, SessionIntent::BeginNativeDrag { .. }))
            .count();
    }

    pub(super) fn ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &RadialDocument,
        generation: u64,
        zoom: f32,
        preset: PreviewPreset,
        selection: Option<&StableSelection>,
        prepared: Option<&PreparedFrameInput>,
        editor_session: AuthoringSessionId,
        show_expected_layout_diagnostics: bool,
        mode: DesignerMode,
        authoring_session: Option<&mut RadialAuthoringSession>,
        projected_selection: &mut Option<ProjectedSelection>,
        drag_payload: &mut Option<DragPayload>,
        placement_draft: &mut Option<PlacementDraft>,
        pending_drop: &mut Option<PendingCellDrop>,
        properties_popup: &mut Option<StableSelection>,
        visited_path: &mut VisitedMenuPath,
        canvas_pan: &mut CanvasPoint,
        pan_drag_start: &mut Option<CanvasPoint>,
    ) {
        let selected_menu = match selection {
            Some(
                StableSelection::Menu(id)
                | StableSelection::Ring { menu_id: id, .. }
                | StableSelection::Cell { menu_id: id, .. },
            ) => Some(id.clone()),
            _ => None,
        };
        let selected_skin = match selection {
            Some(StableSelection::Skin(id)) => Some(id.clone()),
            _ => None,
        };
        let root = selected_menu
            .or_else(|| self.root.clone())
            .unwrap_or_else(|| document.default_menu_id.clone());
        // Design gestures must address stable entities in the authoring draft.
        // Preview presets may intentionally clone/truncate rings and rewrite
        // IDs, so keep those representative documents on the explicit
        // Preview/Test side of the split only.
        let synthetic = if mode == DesignerMode::Design {
            document.clone()
        } else {
            representative_document(document, preset, &root, selected_skin.as_ref())
        };
        let document = &synthetic;
        let mut menu_id = if mode == DesignerMode::Design {
            visited_path
                .current()
                .cloned()
                .filter(|id| document.menus.iter().any(|menu| &menu.id == id))
                .unwrap_or_else(|| root.clone())
        } else {
            self.current_menu()
                .cloned()
                .filter(|id| document.menus.iter().any(|menu| &menu.id == id))
                .unwrap_or_else(|| root.clone())
        };
        let token = format!("{root:?}:{selected_skin:?}:{preset:?}");
        if self.reducer.is_none()
            || self.document_generation != generation
            || self.selection_token != token
        {
            self.reset(document, &root, generation);
            self.selection_token = token;
            menu_id = root;
        }
        let Some(menu) = document.menus.iter().find(|menu| menu.id == menu_id) else {
            return;
        };
        if self.preparation_notice.is_some() {
            self.cancel_tooltip();
            if let Some(notice) = &self.preparation_notice {
                super::show_resource_notice(ui, notice);
            }
            if ui.button("Retry preview preparation").clicked() {
                self.retry_preparation();
            }
            return;
        }
        let Some(input) = prepared else {
            self.cancel_tooltip();
            ui.colored_label(ui.visuals().error_fg_color, "Preparing preview resources…");
            return;
        };
        let layout = input.layout.clone();
        super::show_radial_diagnostics(
            ui,
            input.diagnostics.iter(),
            show_expected_layout_diagnostics,
            "embedded-preview",
        );
        if mode == DesignerMode::Design {
            self.design_ui(
                ui,
                document,
                menu,
                generation,
                editor_session,
                &input,
                authoring_session,
                projected_selection,
                drag_payload,
                placement_draft,
                pending_drop,
                properties_popup,
                visited_path,
                zoom,
                canvas_pan,
                pan_drag_start,
            );
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Back").clicked() {
                self.back();
                ui.ctx().request_repaint();
            }
            ui.label(format!("Menu: {}", menu.name))
                .on_hover_text(menu.name.clone());
        });
        if self.current_menu() != Some(&menu_id) {
            self.cancel_tooltip();
            return;
        }
        let available = ui.available_size();
        if available.x <= 1.0 || available.y <= 1.0 {
            self.cancel_tooltip();
            return;
        }
        let canvas_size = egui::vec2(available.x, available.y);
        let (canvas_rect, response) =
            ui.allocate_exact_size(canvas_size, egui::Sense::click_and_drag());
        let canvas_painter = ui.painter().with_clip_rect(canvas_rect);
        let transform = CanvasTransform {
            zoom,
            ..CanvasTransform::fit(
                CanvasPoint::new(canvas_rect.min.x, canvas_rect.min.y),
                CanvasPoint::new(canvas_rect.width(), canvas_rect.height()),
                CanvasPoint::new(layout.visual_extent.min.x, layout.visual_extent.min.y),
                CanvasPoint::new(layout.visual_extent.max.x, layout.visual_extent.max.y),
                layout.scale_factor.get() as f32,
            )
        }
        .normalized();

        let pointer_point = response
            .interact_pointer_pos()
            .filter(|_| response.hovered())
            .map(|pointer| {
                let point = transform.screen_to_world(CanvasPoint::new(pointer.x, pointer.y));
                LogicalPoint {
                    x: point.x,
                    y: point.y,
                }
            });
        let pointer_down = response.is_pointer_button_down_on()
            || response.drag_started()
            || response.dragged()
            || response.drag_stopped()
            || response.clicked();
        if pointer_down {
            self.cancel_tooltip();
        }

        if response.drag_started()
            && let Some((pointer, point)) = response.interact_pointer_pos().zip(pointer_point)
        {
            let _ = pointer;
            if let Some(cell) = layout.hit_test(point) {
                self.begin_drag(document, &menu.id, &cell.cell_id, point, generation);
            }
        }
        if response.dragged()
            && self.drag_cell.is_some()
            && let Some((pointer, point)) = response.interact_pointer_pos().zip(pointer_point)
        {
            let _ = pointer;
            self.continue_drag(
                point,
                layout.hit_test(point).map(|cell| cell.cell_id.clone()),
                generation,
            );
        }
        if response.drag_stopped() {
            self.drag_cell = None;
        }
        if response.clicked()
            && let Some(point) = pointer_point
            && let Some(cell) = layout.hit_test(point)
        {
            self.activate(document, Some(input), &cell.cell_id);
            ui.ctx().request_repaint();
        }

        if response.hovered() && !pointer_down {
            let hit = pointer_point.and_then(|point| layout.hit_test(point));
            self.hovered_cell = hit.map(|cell| cell.cell_id.clone());
            let tooltip_cell = pointer_point
                .and_then(|point| layout.geometric_hover_cell(point))
                .map(|cell| cell.cell_id.clone());
            let identity = tooltip_cell
                .as_ref()
                .filter(|cell| input.resources.tooltips.contains_key(*cell))
                .and_then(|cell| {
                    self.reducer
                        .as_ref()
                        .and_then(|reducer| reducer.state.stack.last())
                        .map(|frame| TooltipIdentity {
                            session_id: SessionId::new(format!(
                                "editor-preview-{}",
                                editor_session.0
                            )),
                            frame_id: frame.frame_id,
                            layout_generation: input.scene.generation,
                            cell_id: cell.clone(),
                        })
                });
            self.tooltip_hover.observe(
                identity,
                true,
                monotonic_ms(),
                self.tooltip_preferences.delay_ms,
            );
        } else if !response.hovered() || pointer_down {
            self.cancel_tooltip();
        }

        let now = monotonic_ms();
        let candidate = self
            .tooltip_hover
            .candidate()
            .map(|(identity, deadline)| (identity.clone(), deadline));
        if let Some((identity, deadline)) = candidate {
            if deadline <= now {
                self.tooltip_hover.expire(&identity, now);
            } else {
                ui.ctx()
                    .request_repaint_after(Duration::from_millis(deadline - now));
            }
        }
        let current_identity = self
            .tooltip_hover
            .visible()
            .filter(|identity| {
                identity.session_id
                    == SessionId::new(format!("editor-preview-{}", editor_session.0))
                    && self
                        .reducer
                        .as_ref()
                        .and_then(|reducer| reducer.state.stack.last())
                        .is_some_and(|frame| frame.frame_id == identity.frame_id)
                    && identity.layout_generation == input.scene.generation
            })
            .map(|identity| identity.cell_id.clone());
        let selected = self.hovered_cell.as_ref().or_else(|| {
            self.reducer
                .as_ref()
                .and_then(|reducer| reducer.state.selected.as_ref())
        });
        let scene = build_scene_prepared_selected_tooltip(
            &layout,
            input.scene.generation,
            &input.resources,
            selected,
            current_identity.as_ref(),
            input.work_area,
        );
        let Ok(frame) = self.compositor.compose(&scene, layout.scale_factor, 0) else {
            ui.colored_label(
                ui.visuals().error_fg_color,
                "Error: preview compositor failed",
            );
            return;
        };
        let size = [frame.image.width() as usize, frame.image.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, frame.image.as_raw());
        if let Some(texture) = self.texture.as_mut() {
            texture.set(color, egui::TextureOptions::LINEAR);
        } else {
            self.texture = Some(ui.ctx().load_texture(
                "radial-editor-preview",
                color,
                egui::TextureOptions::LINEAR,
            ));
        }
        if let Some(texture) = &self.texture {
            let scene_rect = transform_logical_rect(&transform, frame.logical_bounds);
            canvas_painter.image(
                texture.id(),
                scene_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn design_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &RadialDocument,
        menu: &crate::radial::model::MenuDefinition,
        generation: u64,
        editor_session: AuthoringSessionId,
        input: &PreparedFrameInput,
        mut authoring_session: Option<&mut RadialAuthoringSession>,
        projected_selection: &mut Option<ProjectedSelection>,
        drag_payload: &mut Option<DragPayload>,
        placement_draft: &mut Option<PlacementDraft>,
        pending_drop: &mut Option<PendingCellDrop>,
        properties_popup: &mut Option<StableSelection>,
        visited_path: &mut VisitedMenuPath,
        zoom: f32,
        canvas_pan: &mut CanvasPoint,
        pan_drag_start: &mut Option<CanvasPoint>,
    ) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(visited_path.as_slice().len() > 1, egui::Button::new("Back"))
                .clicked()
            {
                visited_path.back();
                if let Some(parent) = visited_path.current().cloned()
                    && let Some(session) = authoring_session.as_deref_mut()
                {
                    session.select(Some(StableSelection::Menu(parent)));
                }
                self.cancel_tooltip();
                ui.ctx().request_repaint();
            }
            ui.label(format!("Design · {}", menu.name))
                .on_hover_text(menu.name.clone());
            let breadcrumb = visited_path
                .as_slice()
                .iter()
                .filter_map(|id| {
                    document
                        .menus
                        .iter()
                        .find(|candidate| &candidate.id == id)
                        .map(|candidate| candidate.name.clone())
                })
                .collect::<Vec<_>>()
                .join(" › ");
            ui.small(breadcrumb);
        });
        let available = ui.available_size();
        if available.x <= 1.0 || available.y <= 1.0 {
            return;
        }
        let canvas_size = egui::vec2(available.x, available.y);
        let (canvas_rect, response) =
            ui.allocate_exact_size(canvas_size, egui::Sense::click_and_drag());
        let canvas_painter = ui.painter().with_clip_rect(canvas_rect);
        let transform = CanvasTransform::fit(
            CanvasPoint::new(canvas_rect.min.x, canvas_rect.min.y),
            CanvasPoint::new(canvas_rect.width(), canvas_rect.height()),
            CanvasPoint::new(
                input.layout.visual_extent.min.x,
                input.layout.visual_extent.min.y,
            ),
            CanvasPoint::new(
                input.layout.visual_extent.max.x,
                input.layout.visual_extent.max.y,
            ),
            input.layout.scale_factor.get() as f32,
        );
        let transform = CanvasTransform {
            zoom,
            pan: *canvas_pan,
            ..transform
        }
        .normalized();
        let pointer_world = response
            .interact_pointer_pos()
            .filter(|_| response.hovered())
            .map(|point| transform.screen_to_world(CanvasPoint::new(point.x, point.y)));
        let hovered = pointer_world.and_then(|point| {
            input.layout.geometric_hover_cell(LogicalPoint {
                x: point.x,
                y: point.y,
            })
        });
        let hovered_provenance =
            hovered.map(|cell| projected_provenance(document, &menu.id, cell, &input.provenance));
        let center_hit = pointer_world.is_some_and(|point| {
            hovered.is_none()
                && menu.center_action.is_none()
                && menu.center_control.is_none()
                && distance(point, input.layout.center) <= input.layout.center_radius
        });

        if response.drag_started() {
            *pending_drop = None;
            *pan_drag_start = None;
            match design_drag_disposition(hovered_provenance.as_ref(), center_hit) {
                DesignDragDisposition::Authored => {
                    *drag_payload = hovered_provenance.clone().map(|source| DragPayload {
                        source,
                        generation: crate::radial::authoring::DraftGeneration(generation),
                    });
                }
                DesignDragDisposition::Center => {
                    *drag_payload = Some(DragPayload {
                        source: ProjectedCellProvenance::Center,
                        generation: crate::radial::authoring::DraftGeneration(generation),
                    });
                    *projected_selection = Some(ProjectedSelection {
                        label: "Choose an empty slot for the new cell".into(),
                        provenance: ProjectedCellProvenance::Center,
                    });
                }
                DesignDragDisposition::ReadOnly => {
                    *projected_selection = Some(ProjectedSelection {
                        label: "Generated preview cells are read-only and cannot be dragged".into(),
                        provenance: hovered_provenance
                            .clone()
                            .unwrap_or(ProjectedCellProvenance::Background),
                    });
                }
                DesignDragDisposition::Pan => {
                    *pan_drag_start = Some(*canvas_pan);
                }
            }
        }
        if response.dragged()
            && drag_payload.is_none()
            && let Some(start) = *pan_drag_start
        {
            let delta = response.drag_delta();
            *canvas_pan = CanvasPoint::new(start.x + delta.x, start.y + delta.y);
        }
        if response.drag_stopped() {
            if let Some(payload) = drag_payload.take() {
                let destination = hovered_provenance.clone();
                if matches!(&payload.source, ProjectedCellProvenance::Center) {
                    let target = destination
                        .as_ref()
                        .and_then(ProjectedCellProvenance::authored_ids)
                        .and_then(|(menu_id, ring_id, cell_id)| {
                            document
                                .menus
                                .iter()
                                .find(|menu| &menu.id == menu_id)
                                .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
                                .and_then(|ring| ring.cells.iter().find(|cell| &cell.id == cell_id))
                                .filter(|cell| matches!(&cell.content, CellContent::Spacer))
                                .map(|_| (menu_id.clone(), ring_id.clone(), cell_id.clone()))
                        });
                    if payload.generation == crate::radial::authoring::DraftGeneration(generation) {
                        if let Some((menu_id, ring_id, cell_id)) = target {
                            if let Some(session) = authoring_session.as_deref_mut() {
                                session.select(Some(StableSelection::Cell {
                                    menu_id: menu_id.clone(),
                                    ring_id: ring_id.clone(),
                                    cell_id: cell_id.clone(),
                                }));
                            }
                            *properties_popup = Some(StableSelection::Cell {
                                menu_id: menu_id.clone(),
                                ring_id: ring_id.clone(),
                                cell_id: cell_id.clone(),
                            });
                            *placement_draft = Some(PlacementDraft::new(
                                menu_id,
                                ring_id,
                                cell_id,
                                payload.generation,
                            ));
                            *projected_selection = Some(ProjectedSelection {
                                label: "Placement draft · choose content in the inspector".into(),
                                provenance: destination.unwrap_or(ProjectedCellProvenance::Center),
                            });
                        } else {
                            *placement_draft = None;
                            *projected_selection = Some(ProjectedSelection {
                                label: "Placement cancelled: drop on an empty authored slot".into(),
                                provenance: destination.unwrap_or(ProjectedCellProvenance::Center),
                            });
                        }
                    } else {
                        *placement_draft = None;
                        *projected_selection = Some(ProjectedSelection {
                            label: "Placement cancelled: draft is stale".into(),
                            provenance: destination.unwrap_or(ProjectedCellProvenance::Center),
                        });
                    }
                } else {
                    match (
                        payload.source.authored_ids(),
                        destination
                            .as_ref()
                            .and_then(ProjectedCellProvenance::authored_ids),
                    ) {
                        (
                            Some((source_menu, source_ring, source_cell)),
                            Some((dest_menu, dest_ring, dest_cell)),
                        ) => {
                            let destination_index =
                                cell_index(document, dest_menu, dest_ring, Some(dest_cell));
                            let destination_occupied = document
                                .menus
                                .iter()
                                .find(|menu| &menu.id == dest_menu)
                                .and_then(|menu| {
                                    menu.rings.iter().find(|ring| &ring.id == dest_ring)
                                })
                                .and_then(|ring| ring.cells.get(destination_index))
                                .is_some_and(|cell| !matches!(&cell.content, CellContent::Spacer));
                            if source_menu == dest_menu
                                && source_ring == dest_ring
                                && source_cell == dest_cell
                            {
                                // Dropping a cell onto itself is an explicit no-op,
                                // not an occupied-slot confirmation.
                            } else if destination_occupied {
                                *pending_drop = Some(PendingCellDrop {
                                    source_menu: source_menu.clone(),
                                    source_ring: source_ring.clone(),
                                    source_cell: source_cell.clone(),
                                    destination_menu: dest_menu.clone(),
                                    destination_ring: dest_ring.clone(),
                                    destination_cell: dest_cell.clone(),
                                    destination_index,
                                    generation: payload.generation,
                                });
                                if let Some(destination) = destination {
                                    *projected_selection = Some(ProjectedSelection {
                                        label: "Destination occupied — choose Swap or Cancel"
                                            .into(),
                                        provenance: destination,
                                    });
                                }
                            } else {
                                let result = authoring_session
                                    .as_deref_mut()
                                    .ok_or(
                                        crate::radial::authoring::menu::MenuEditError::MissingEntity,
                                    )
                                    .and_then(|session| {
                                        crate::radial::authoring::menu::move_cell_to_slot(
                                            session,
                                            (source_menu, source_ring, source_cell),
                                            (dest_menu, dest_ring, destination_index),
                                            crate::radial::authoring::menu::CellDropResolution::MoveIntoSpacer,
                                            payload.generation,
                                        )
                                    });
                                if let Err(error) = result {
                                    if let Some(destination) = destination {
                                        *projected_selection = Some(ProjectedSelection {
                                            label: format!("Drop cancelled: {error:?}"),
                                            provenance: destination,
                                        });
                                    }
                                }
                            }
                        }
                        (_, Some(_)) => {
                            if let Some(destination) = destination {
                                *projected_selection = Some(ProjectedSelection {
                                    label: "Drop cancelled: source is not an authored slot".into(),
                                    provenance: destination,
                                });
                            }
                        }
                        (_, None) => {
                            *projected_selection = Some(ProjectedSelection {
                                label: "Drop cancelled: generated preview cells are read-only"
                                    .into(),
                                provenance: destination
                                    .unwrap_or(ProjectedCellProvenance::Background),
                            });
                        }
                    }
                }
            }
            *pan_drag_start = None;
        }
        if response.clicked() || response.secondary_clicked() {
            if let Some(provenance) = hovered_provenance.clone() {
                *projected_selection = Some(ProjectedSelection {
                    label: hovered.map_or_else(String::new, |cell| cell.label.clone()),
                    provenance: provenance.clone(),
                });
                if response.secondary_clicked()
                    && let Some((menu_id, ring_id, cell_id)) = provenance.authored_ids()
                {
                    *properties_popup = Some(StableSelection::Cell {
                        menu_id: menu_id.clone(),
                        ring_id: ring_id.clone(),
                        cell_id: cell_id.clone(),
                    });
                }
                if let Some((menu_id, ring_id, cell_id)) = provenance.authored_ids()
                    && let Some(session) = authoring_session.as_deref_mut()
                {
                    let hovered_is_spacer = document
                        .menus
                        .iter()
                        .find(|candidate| &candidate.id == menu_id)
                        .and_then(|candidate| {
                            candidate.rings.iter().find(|ring| &ring.id == ring_id)
                        })
                        .and_then(|ring| {
                            ring.cells.iter().find(|candidate| &candidate.id == cell_id)
                        })
                        .is_some_and(|candidate| matches!(&candidate.content, CellContent::Spacer));
                    if hovered_is_spacer {
                        *placement_draft = Some(PlacementDraft::new(
                            menu_id.clone(),
                            ring_id.clone(),
                            cell_id.clone(),
                            crate::radial::authoring::DraftGeneration(generation),
                        ));
                        *properties_popup = Some(StableSelection::Cell {
                            menu_id: menu_id.clone(),
                            ring_id: ring_id.clone(),
                            cell_id: cell_id.clone(),
                        });
                    }
                    session.select(Some(StableSelection::Cell {
                        menu_id: menu_id.clone(),
                        ring_id: ring_id.clone(),
                        cell_id: cell_id.clone(),
                    }));
                }
            } else if let Some(point) = pointer_world
                && menu.center_action.is_none()
                && menu.center_control.is_none()
                && distance(point, input.layout.center) <= input.layout.center_radius
            {
                // The center affordance is a placement draft only.  It never
                // enters the runtime reducer or dispatch path.
                let empty_slots = empty_authored_slots(&menu);
                let label = match empty_slots.as_slice() {
                    [] => "No empty authored slot — add or resize a ring first".into(),
                    [_] => "Placement draft · choose content in the inspector".into(),
                    _ => "Choose a highlighted empty slot, or drag + onto it".into(),
                };
                *projected_selection = Some(ProjectedSelection {
                    label,
                    provenance: ProjectedCellProvenance::Center,
                });
                if let [(ring_id, cell_id)] = empty_slots.as_slice()
                    && let Some(session) = authoring_session.as_deref_mut()
                {
                    let selection = StableSelection::Cell {
                        menu_id: menu.id.clone(),
                        ring_id: ring_id.clone(),
                        cell_id: cell_id.clone(),
                    };
                    *placement_draft = Some(PlacementDraft::new(
                        menu.id.clone(),
                        ring_id.clone(),
                        cell_id.clone(),
                        crate::radial::authoring::DraftGeneration(generation),
                    ));
                    *properties_popup = Some(selection.clone());
                    session.select(Some(selection));
                }
            }
            if response.secondary_clicked() {
                ui.ctx().request_repaint();
            }
        }
        if response.double_clicked()
            && let Some(ProjectedCellProvenance::Authored {
                menu_id,
                ring_id,
                cell_id,
            }) = hovered_provenance.as_ref()
            && let Some(cell) = document
                .menus
                .iter()
                .find(|candidate| &candidate.id == menu_id)
                .and_then(|candidate| candidate.rings.iter().find(|ring| &ring.id == ring_id))
                .and_then(|ring| ring.cells.iter().find(|cell| &cell.id == cell_id))
            && let CellContent::Submenu { menu_id: child } = &cell.content
        {
            if visited_path.enter(child.clone()) {
                if let Some(session) = authoring_session.as_deref_mut() {
                    session.select(Some(StableSelection::Menu(child.clone())));
                }
                self.cancel_tooltip();
                ui.ctx().request_repaint();
            }
        }

        let scene = build_scene_prepared_selected_tooltip(
            &input.layout,
            input.scene.generation,
            &input.resources,
            None,
            None,
            input.work_area,
        );
        let Ok(frame) = self
            .compositor
            .compose(&scene, input.layout.scale_factor, 0)
        else {
            ui.colored_label(
                ui.visuals().error_fg_color,
                "Error: preview compositor failed",
            );
            return;
        };
        let size = [frame.image.width() as usize, frame.image.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, frame.image.as_raw());
        if let Some(texture) = self.texture.as_mut() {
            texture.set(color, egui::TextureOptions::LINEAR);
        } else {
            self.texture = Some(ui.ctx().load_texture(
                "radial-designer-preview",
                color,
                egui::TextureOptions::LINEAR,
            ));
        }
        if let Some(texture) = &self.texture {
            let scene_rect = transform_logical_rect(&transform, frame.logical_bounds);
            canvas_painter.image(
                texture.id(),
                scene_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        if let Some(payload) = drag_payload.as_ref() {
            if let Some(cell) = hovered {
                let target_is_authored = hovered_provenance
                    .as_ref()
                    .is_some_and(|provenance| provenance.is_authored());
                let target_is_spacer = target_is_authored
                    && document
                        .menus
                        .iter()
                        .find(|candidate| candidate.id == menu.id)
                        .and_then(|candidate| {
                            candidate.rings.iter().find(|ring| ring.id == cell.ring_id)
                        })
                        .and_then(|ring| {
                            ring.cells
                                .iter()
                                .find(|candidate| candidate.id == cell.cell_id)
                        })
                        .is_some_and(|candidate| matches!(&candidate.content, CellContent::Spacer));
                let color = if target_is_spacer {
                    egui::Color32::from_rgb(110, 220, 150)
                } else if target_is_authored {
                    egui::Color32::from_rgb(245, 190, 85)
                } else {
                    egui::Color32::from_rgb(230, 100, 100)
                };
                paint_cell_outline(
                    &canvas_painter,
                    &transform,
                    &cell.shape,
                    egui::Stroke::new(2.0_f32, color),
                );
            }
            if let Some(pointer) = response.interact_pointer_pos() {
                let label = payload
                    .source
                    .authored_ids()
                    .and_then(|(source_menu, source_ring, source_cell)| {
                        document
                            .menus
                            .iter()
                            .find(|candidate| &candidate.id == source_menu)
                            .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == source_ring))
                            .and_then(|ring| ring.cells.iter().find(|cell| &cell.id == source_cell))
                            .map(|cell| cell.label.clone())
                    })
                    .filter(|label| !label.trim().is_empty())
                    .unwrap_or_else(|| "Authored cell".into());
                canvas_painter.text(
                    pointer + egui::vec2(12.0, 12.0),
                    egui::Align2::LEFT_TOP,
                    label,
                    egui::TextStyle::Body.resolve(ui.style()),
                    egui::Color32::from_rgb(240, 240, 240),
                );
            }
        }
        for cell in &input.layout.cells {
            if cell.actionable {
                continue;
            }
            paint_passive_cell_outline(&canvas_painter, &transform, &cell.shape);
        }
        if menu.center_action.is_none() && menu.center_control.is_none() {
            let center = transform.world_to_screen(CanvasPoint::new(
                input.layout.center.x,
                input.layout.center.y,
            ));
            canvas_painter.text(
                egui::pos2(center.x, center.y),
                egui::Align2::CENTER_CENTER,
                "+",
                egui::TextStyle::Heading.resolve(ui.style()),
                ui.visuals().widgets.inactive.fg_stroke.color,
            );
        }
        if let Some(selected) = projected_selection.as_ref() {
            ui.small(format!("Selected: {}", selected.label));
        }
        if drag_payload.is_some() {
            ui.small("Dragging authored cell — drop on a spacer slot");
        }
        if placement_draft.is_some() {
            ui.small("Placement draft — choose content in the inspector or cancel");
        }
        let _ = editor_session;
    }
}

fn distance(left: CanvasPoint, right: LogicalPoint) -> f32 {
    let dx = left.x - right.x;
    let dy = left.y - right.y;
    (dx * dx + dy * dy).sqrt()
}

fn projected_provenance(
    document: &RadialDocument,
    menu_id: &MenuId,
    cell: &crate::radial::geometry::CellLayout,
    prepared_provenance: &BTreeMap<CellId, crate::radial::bindings::ProjectedDynamicProvenance>,
) -> ProjectedCellProvenance {
    if cell.cell_id.as_str() == "__center" {
        return ProjectedCellProvenance::Center;
    }
    if cell.cell_id.as_str() == "__background" {
        return ProjectedCellProvenance::Background;
    }
    if cell.cell_id.as_str().starts_with("__radial_") {
        return ProjectedCellProvenance::Control;
    }
    // Authored membership is authoritative even when a user chose an ID that
    // resembles the projection's historical `dyn:` display format.
    if let Some(authored) = document
        .menus
        .iter()
        .find(|menu| &menu.id == menu_id)
        .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == &cell.ring_id))
        .and_then(|ring| {
            ring.cells
                .iter()
                .find(|candidate| candidate.id == cell.cell_id)
        })
    {
        return ProjectedCellProvenance::Authored {
            menu_id: menu_id.clone(),
            ring_id: cell.ring_id.clone(),
            cell_id: authored.id.clone(),
        };
    }
    if let Some(projected) = prepared_provenance.get(&cell.cell_id)
        && let Some(source) = document
            .menus
            .iter()
            .find(|menu| &menu.id == menu_id)
            .and_then(|menu| {
                menu.rings
                    .iter()
                    .flat_map(|ring| &ring.cells)
                    .find(|candidate| candidate.id == projected.source_cell_id)
            })
            .and_then(|source| match &source.content {
                CellContent::Dynamic { source } => Some(source.clone()),
                _ => None,
            })
    {
        return ProjectedCellProvenance::Dynamic {
            menu_id: menu_id.clone(),
            ring_id: cell.ring_id.clone(),
            source_cell_id: projected.source_cell_id.clone(),
            source,
            result_index: projected.result_index,
            fingerprint: projected.fingerprint.clone(),
        };
    }
    ProjectedCellProvenance::Dynamic {
        menu_id: menu_id.clone(),
        ring_id: cell.ring_id.clone(),
        source_cell_id: cell.cell_id.clone(),
        source: crate::radial::model::DynamicSource::Favorites,
        result_index: 0,
        fingerprint: crate::radial::dynamic::SourceFingerprint {
            generation: 0,
            source: "unknown-dynamic".into(),
            query: None,
        },
    }
}

fn cell_index(
    document: &RadialDocument,
    menu_id: &MenuId,
    ring_id: &RingId,
    cell_id: Option<&CellId>,
) -> usize {
    cell_id
        .and_then(|cell_id| {
            document
                .menus
                .iter()
                .find(|menu| &menu.id == menu_id)
                .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
                .and_then(|ring| ring.cells.iter().position(|cell| &cell.id == cell_id))
        })
        .unwrap_or(usize::MAX)
}

fn transform_logical_rect(transform: &CanvasTransform, rect: LogicalRect) -> egui::Rect {
    let min = transform.world_to_screen(CanvasPoint::new(rect.min.x, rect.min.y));
    let max = transform.world_to_screen(CanvasPoint::new(rect.max.x, rect.max.y));
    egui::Rect::from_min_max(egui::pos2(min.x, min.y), egui::pos2(max.x, max.y))
}

fn paint_passive_cell_outline(
    painter: &egui::Painter,
    transform: &CanvasTransform,
    shape: &HitShape,
) {
    paint_cell_outline(
        painter,
        transform,
        shape,
        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(130)),
    );
}

fn paint_cell_outline(
    painter: &egui::Painter,
    transform: &CanvasTransform,
    shape: &HitShape,
    stroke: egui::Stroke,
) {
    match shape {
        HitShape::Circle { center, radius } => {
            let world_center = *center;
            let screen_center =
                transform.world_to_screen(CanvasPoint::new(world_center.x, world_center.y));
            let edge = transform
                .world_to_screen(CanvasPoint::new(world_center.x + radius, world_center.y));
            painter.circle_stroke(
                egui::pos2(screen_center.x, screen_center.y),
                (edge.x - screen_center.x).abs(),
                stroke,
            );
        }
        HitShape::Wedge {
            center,
            inner_radius,
            outer_radius,
            start_angle,
            end_angle,
        } => {
            let world_center = *center;
            let screen_center =
                transform.world_to_screen(CanvasPoint::new(world_center.x, world_center.y));
            let outer = transform.world_to_screen(CanvasPoint::new(
                world_center.x + outer_radius * start_angle.cos(),
                world_center.y + outer_radius * start_angle.sin(),
            ));
            let inner = transform.world_to_screen(CanvasPoint::new(
                world_center.x + inner_radius * end_angle.cos(),
                world_center.y + inner_radius * end_angle.sin(),
            ));
            painter.line_segment(
                [
                    egui::pos2(screen_center.x, screen_center.y),
                    egui::pos2(outer.x, outer.y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(screen_center.x, screen_center.y),
                    egui::pos2(inner.x, inner.y),
                ],
                stroke,
            );
        }
    }
}

fn authoring_error(error: &crate::radial::authoring::AuthoringError) -> String {
    match error {
        crate::radial::authoring::AuthoringError::AssetOverlayInvalid(message) => message.clone(),
        other => format!("{other:?}"),
    }
}

fn preview_pointer_point(
    pointer: egui::Pos2,
    rect: egui::Rect,
    layout: &crate::radial::geometry::LayoutSnapshot,
) -> LogicalPoint {
    let delta = pointer - rect.min;
    let size = rect.size();
    let uv = egui::vec2(delta.x / size.x, delta.y / size.y);
    LogicalPoint {
        x: layout.visual_extent.min.x
            + uv.x * (layout.visual_extent.max.x - layout.visual_extent.min.x),
        y: layout.visual_extent.min.y
            + uv.y * (layout.visual_extent.max.y - layout.visual_extent.min.y),
    }
}

fn preview_scene_rect(
    scene_bounds: LogicalRect,
    wheel_bounds: LogicalRect,
    canvas_rect: egui::Rect,
) -> egui::Rect {
    let world_width = (wheel_bounds.max.x - wheel_bounds.min.x).max(f32::EPSILON);
    let world_height = (wheel_bounds.max.y - wheel_bounds.min.y).max(f32::EPSILON);
    egui::Rect::from_min_max(
        egui::pos2(
            canvas_rect.min.x
                + (scene_bounds.min.x - wheel_bounds.min.x) * canvas_rect.width() / world_width,
            canvas_rect.min.y
                + (scene_bounds.min.y - wheel_bounds.min.y) * canvas_rect.height() / world_height,
        ),
        egui::pos2(
            canvas_rect.min.x
                + (scene_bounds.max.x - wheel_bounds.min.x) * canvas_rect.width() / world_width,
            canvas_rect.min.y
                + (scene_bounds.max.y - wheel_bounds.min.y) * canvas_rect.height() / world_height,
        ),
    )
}

fn cell_role(document: &RadialDocument, menu_id: &MenuId, cell_id: &CellId) -> CellRole {
    if cell_id.as_str().starts_with("__radial_page_next:") {
        return CellRole::NextPage;
    }
    if cell_id.as_str().starts_with("__radial_page_previous:") {
        return CellRole::PreviousPage;
    }
    let menu = document.menus.iter().find(|menu| &menu.id == menu_id);
    if let Some(menu) = menu {
        let special = match cell_id.as_str() {
            "__center" => menu
                .center_control
                .map(control_cell_role)
                .or_else(|| menu.center_action.as_ref().map(|_| CellRole::Action)),
            "__background" => menu
                .background_control
                .map(control_cell_role)
                .or_else(|| menu.background_action.as_ref().map(|_| CellRole::Action)),
            _ => None,
        };
        if let Some(role) = special {
            return role;
        }
    }
    menu.and_then(|menu| {
        menu.rings
            .iter()
            .flat_map(|ring| &ring.cells)
            .find(|cell| &cell.id == cell_id)
    })
    .map_or(CellRole::Unavailable, |cell| match &cell.content {
        CellContent::Action { .. } | CellContent::Dynamic { .. } => CellRole::Action,
        CellContent::Submenu { .. } => CellRole::Submenu,
        CellContent::Control { control } => match control {
            crate::radial::model::Control::Back => CellRole::Back,
            crate::radial::model::Control::Close => CellRole::Close,
            crate::radial::model::Control::NextPage => CellRole::NextPage,
            crate::radial::model::Control::PreviousPage => CellRole::PreviousPage,
            crate::radial::model::Control::Drag => CellRole::Drag,
        },
        CellContent::Spacer => CellRole::Spacer,
    })
}

fn control_cell_role(control: crate::radial::model::Control) -> CellRole {
    match control {
        crate::radial::model::Control::Back => CellRole::Back,
        crate::radial::model::Control::Close => CellRole::Close,
        crate::radial::model::Control::NextPage => CellRole::NextPage,
        crate::radial::model::Control::PreviousPage => CellRole::PreviousPage,
        crate::radial::model::Control::Drag => CellRole::Drag,
    }
}

fn representative_document(
    document: &RadialDocument,
    preset: PreviewPreset,
    selected_menu: &MenuId,
    selected_skin: Option<&SkinId>,
) -> RadialDocument {
    let mut result = document.clone();
    let Some(menu_index) = result
        .menus
        .iter()
        .position(|menu| &menu.id == selected_menu)
    else {
        return result;
    };
    if let Some(skin) = selected_skin {
        result.menus[menu_index].skin_id = skin.clone();
    }
    match preset {
        PreviewPreset::Current | PreviewPreset::HighDpi => {}
        PreviewPreset::OneRing => result.menus[menu_index].rings.truncate(1),
        PreviewPreset::MultiRing => {
            if let Some(mut ring) = result.menus[menu_index].rings.first().cloned() {
                ring.id = crate::radial::model::RingId::new("preview-outer");
                ring.radius += 72.0;
                for cell in &mut ring.cells {
                    cell.id = CellId::new(format!("{}-preview-outer", cell.id));
                }
                result.menus[menu_index].rings.push(ring);
            }
        }
        PreviewPreset::LongLabels => {
            for cell in result.menus[menu_index]
                .rings
                .iter_mut()
                .flat_map(|ring| &mut ring.cells)
            {
                cell.label = format!("{} — representative long label", cell.label);
            }
        }
        PreviewPreset::Submenu => {
            let mut child = result.menus[menu_index].clone();
            let mut ordinal = 1usize;
            child.id = loop {
                let id = MenuId::new(format!("preview-submenu-{ordinal}"));
                if !result.menus.iter().any(|menu| menu.id == id) {
                    break id;
                }
                ordinal += 1;
            };
            child.name = "Preview submenu".into();
            if let Some(cell) = result.menus[menu_index]
                .rings
                .first_mut()
                .and_then(|ring| ring.cells.first_mut())
            {
                cell.content = CellContent::Submenu {
                    menu_id: child.id.clone(),
                };
            }
            result.menus.push(child);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authoring_session() -> RadialAuthoringSession {
        let document = std::sync::Arc::new(RadialDocument::starter());
        RadialAuthoringSession::new(crate::radial::authoring::AuthoringSnapshot::new(
            document,
            "preview-test",
        ))
    }

    #[test]
    fn center_add_auto_targets_only_one_unambiguous_empty_slot() {
        let mut menu = RadialDocument::starter().menus[0].clone();
        menu.rings.truncate(1);
        menu.rings[0].cells.truncate(1);
        menu.rings[0].cells[0].content = CellContent::Spacer;
        let one = empty_authored_slots(&menu);
        assert_eq!(one.len(), 1);

        let mut second_ring = menu.rings[0].clone();
        second_ring.id = RingId::new("second-ring");
        second_ring.cells[0].id = CellId::new("second-slot");
        menu.rings.push(second_ring);
        assert_eq!(empty_authored_slots(&menu).len(), 2);

        menu.rings.iter_mut().for_each(|ring| ring.cells.clear());
        assert!(empty_authored_slots(&menu).is_empty());
    }

    #[test]
    fn generated_drag_is_read_only_instead_of_becoming_canvas_pan() {
        let generated = ProjectedCellProvenance::Dynamic {
            menu_id: MenuId::new("menu"),
            ring_id: RingId::new("ring"),
            source_cell_id: CellId::new("source"),
            source: crate::radial::model::DynamicSource::Favorites,
            result_index: 0,
            fingerprint: crate::radial::dynamic::SourceFingerprint {
                generation: 1,
                source: "favorites".into(),
                query: None,
            },
        };
        assert_eq!(
            design_drag_disposition(Some(&generated), false),
            DesignDragDisposition::ReadOnly
        );
        assert_eq!(
            design_drag_disposition(None, false),
            DesignDragDisposition::Pan
        );
        assert_eq!(
            design_drag_disposition(None, true),
            DesignDragDisposition::Center
        );
    }

    #[test]
    fn closing_or_replacing_preview_clears_pending_and_visible_tooltips() {
        let mut preview = EmbeddedPreview::default();
        let identity = TooltipIdentity {
            session_id: SessionId::new("editor-preview-test"),
            frame_id: FrameId(1),
            layout_generation: 1,
            cell_id: CellId::new("test-cell"),
        };

        preview.hovered_cell = Some(identity.cell_id.clone());
        preview
            .tooltip_hover
            .observe(Some(identity.clone()), true, 10, 300);
        preview.cancel_tooltip();
        assert!(preview.tooltip_hover.candidate().is_none());
        assert!(preview.hovered_cell.is_none());

        preview
            .tooltip_hover
            .observe(Some(identity.clone()), true, 10, 0);
        assert!(preview.tooltip_hover.expire(&identity, 10));
        preview.hovered_cell = Some(identity.cell_id.clone());
        preview.cancel_tooltip();
        assert!(preview.tooltip_hover.visible().is_none());
        assert!(preview.hovered_cell.is_none());
    }

    #[test]
    fn embedded_tooltip_overflow_keeps_the_wheel_canvas_transform_fixed() {
        let document = RadialDocument::starter();
        let layout = layout_document_menu(
            &document,
            &document.menus[0],
            PhysicalPoint { x: 240.0, y: 240.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 480.0, y: 480.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.55,
        )
        .unwrap();
        let canvas = egui::Rect::from_min_size(egui::pos2(12.0, 34.0), egui::vec2(360.0, 360.0));
        let wheel = layout.visual_extent;
        let expanded = LogicalRect {
            min: LogicalPoint {
                x: wheel.min.x - 100.0,
                y: wheel.min.y - 50.0,
            },
            max: LogicalPoint {
                x: wheel.max.x + 180.0,
                y: wheel.max.y + 80.0,
            },
        };
        let expanded_screen = preview_scene_rect(expanded, wheel, canvas);
        let scale_x = expanded_screen.width() / (expanded.max.x - expanded.min.x);
        let scale_y = expanded_screen.height() / (expanded.max.y - expanded.min.y);
        assert_eq!(preview_scene_rect(wheel, wheel, canvas), canvas);
        assert!(
            (expanded_screen.min.x + (wheel.min.x - expanded.min.x) * scale_x - canvas.min.x).abs()
                < 0.001
        );
        assert!(
            (expanded_screen.max.y - (expanded.max.y - wheel.max.y) * scale_y - canvas.max.y).abs()
                < 0.001
        );
        let pointer_at_wheel_center = preview_pointer_point(canvas.center(), canvas, &layout);
        assert_eq!(pointer_at_wheel_center, layout.center);
    }

    fn sync_current(
        preview: &mut EmbeddedPreview,
        session: &mut RadialAuthoringSession,
        client: &AuthoringClient,
    ) {
        preview.sync_preparation(
            session,
            Some(client),
            PreviewPreset::Current,
            None,
            TooltipPreferences::default(),
        );
    }

    fn finish_preparation_request(
        preview: &mut EmbeddedPreview,
        session: &mut RadialAuthoringSession,
        client: &AuthoringClient,
        request: crate::radial::authoring::AuthoringRequest,
        preparer: &mut crate::radial::preparation::PreviewFramePreparer,
    ) -> std::sync::Arc<PreparedFrameInput> {
        let crate::radial::authoring::AuthoringRequest::PrepareEmbeddedPreview {
            id,
            generation,
            editor_session,
            candidate,
            menu_id,
            selected,
            anchor,
            work_area,
            scale,
            token,
            projection,
        } = request
        else {
            panic!("expected an embedded preview preparation request")
        };
        let input = std::sync::Arc::new(
            preparer
                .prepare(
                    &candidate,
                    &menu_id,
                    anchor,
                    work_area,
                    scale,
                    generation.0,
                    selected.as_ref(),
                    &projection,
                )
                .unwrap(),
        );
        assert!(session.accept_reply(
            crate::radial::authoring::AuthoringReply::EmbeddedPreviewPrepared {
                id,
                generation,
                editor_session,
                token,
                input,
            }
        ));
        sync_current(preview, session, client);
        preview.prepared_frame(session).unwrap()
    }

    fn finish_pending_preparation(
        preview: &mut EmbeddedPreview,
        session: &mut RadialAuthoringSession,
        client: &AuthoringClient,
        endpoint: &crate::radial::authoring::AuthoringMainEndpoint,
        preparer: &mut crate::radial::preparation::PreviewFramePreparer,
    ) -> std::sync::Arc<PreparedFrameInput> {
        let request = endpoint
            .request_rx
            .try_recv()
            .expect("sync_current should enqueue an embedded preview request");
        finish_preparation_request(preview, session, client, request, preparer)
    }

    #[test]
    fn rejected_overlay_is_memoized_until_inputs_change_or_retry() {
        let (client, _endpoint) = crate::radial::authoring::authoring_control_service();
        let mut session = authoring_session();
        session
            .pending_assets
            .additions
            .push(crate::radial::authoring::ManagedAssetAddition {
                record: crate::radial::model::AssetRecord {
                    id: crate::radial::model::AssetId::new("corrupt-preview"),
                    kind: crate::radial::model::MediaKind::Image,
                    relative_path: "images/corrupt-preview.png".into(),
                    content_sha256: "0".repeat(64),
                    byte_len: 3,
                },
                bytes: std::sync::Arc::from([1_u8, 2, 3]),
            });
        let mut preview = EmbeddedPreview::default();

        sync_current(&mut preview, &mut session, &client);
        assert_eq!(preview.preparation_attempts, 1);
        assert!(preview.preparation_notice.as_ref().is_some_and(|notice| {
            notice.severity == super::super::ResourceNoticeSeverity::Error
        }));
        assert!(session.pending_request.is_none());
        sync_current(&mut preview, &mut session, &client);
        assert_eq!(preview.preparation_attempts, 1);

        preview.retry_preparation();
        sync_current(&mut preview, &mut session, &client);
        assert_eq!(preview.preparation_attempts, 2);

        session.pending_assets.additions[0].record.content_sha256 = "1".repeat(64);
        sync_current(&mut preview, &mut session, &client);
        assert_eq!(preview.preparation_attempts, 3);
    }

    #[test]
    fn disconnected_preview_transport_clears_exact_pending_request_and_does_not_retry() {
        let (client, endpoint) = crate::radial::authoring::authoring_control_service();
        drop(endpoint);
        let mut session = authoring_session();
        let mut preview = EmbeddedPreview::default();

        sync_current(&mut preview, &mut session, &client);
        assert_eq!(preview.preparation_attempts, 1);
        assert!(session.pending_request.is_none());
        assert!(preview.preparation_notice.as_ref().is_some_and(|notice| {
            notice.severity == super::super::ResourceNoticeSeverity::Error
                && notice.message.contains("ServiceClosed")
        }));
        sync_current(&mut preview, &mut session, &client);
        assert_eq!(preview.preparation_attempts, 1);
    }

    #[test]
    fn navigation_uses_session_reducer_and_dispatch_is_only_counted() {
        let mut document = RadialDocument::starter();
        let root = document.menus[0].id.clone();
        let mut child = document.menus[0].clone();
        child.id = MenuId::new("child");
        child.name = "Child".into();
        document.menus[0].rings[0].cells[0].content = CellContent::Submenu {
            menu_id: child.id.clone(),
        };
        document.menus.push(child.clone());
        let mut preview = EmbeddedPreview::default();
        preview.reset(&document, &root, 1);
        let submenu = document.menus[0].rings[0].cells[0].id.clone();
        preview.activate(&document, None, &submenu);
        assert_eq!(preview.current_menu(), Some(&child.id));
        preview.back();
        assert_eq!(preview.current_menu(), Some(&root));
        let action = document.menus[0].rings[0].cells[1].id.clone();
        document.menus[0].rings[0].cells[1].content = CellContent::Action {
            binding: crate::radial::model::ActionBinding::Persisted {
                action: crate::universal_actions::PersistedUniversalActionRef {
                    target: None,
                    action_id: crate::universal_actions::ActionId::new("test"),
                },
            },
        };
        preview.activate(&document, None, &action);
        assert_eq!(preview.intercepted_dispatches, 1);
    }

    #[test]
    fn generated_preview_click_intercepts_exact_typed_dynamic_provenance() {
        let document = RadialDocument::starter();
        let menu = document
            .menus
            .iter()
            .find(|menu| {
                menu.rings.iter().any(|ring| {
                    ring.cells
                        .iter()
                        .any(|cell| matches!(&cell.content, CellContent::Dynamic { .. }))
                })
            })
            .expect("starter document should contain a dynamic source menu");
        let menu_id = menu.id.clone();
        let dynamic = crate::radial::preparation::synthetic_preview_dynamic(menu);
        let mut preparer =
            crate::radial::preparation::PreviewFramePreparer::new(Default::default());
        let prepared = preparer
            .prepare(
                &document,
                &menu_id,
                PhysicalPoint { x: 240.0, y: 240.0 },
                PhysicalRect {
                    min: PhysicalPoint { x: 0.0, y: 0.0 },
                    max: PhysicalPoint { x: 480.0, y: 480.0 },
                },
                ScaleFactor::new(1.0).unwrap(),
                23,
                None,
                &crate::radial::preparation::PreviewProjection {
                    dynamic,
                    ..Default::default()
                },
            )
            .expect("synthetic preview should prepare");
        let (generated_cell_id, expected_provenance) = prepared
            .provenance
            .iter()
            .next()
            .expect("projection should carry generated provenance");
        let mut preview = EmbeddedPreview::default();
        preview.reset(&document, &menu_id, 23);
        preview.activate(&document, Some(&prepared), generated_cell_id);

        assert_eq!(preview.intercepted_dispatches, 1);
        let intercepted = preview
            .last_intercepted_dynamic_dispatch
            .as_ref()
            .expect("generated dispatch should retain its provenance");
        assert_eq!(&intercepted.generated_cell_id, generated_cell_id);
        assert_eq!(&intercepted.provenance, expected_provenance);
    }

    #[test]
    fn embedded_preview_uses_frozen_synthetic_center_and_current_parent_scope() {
        let mut document = RadialDocument::starter();
        let root = document.default_menu_id.clone();
        let child_id = MenuId::new("starter-favorites");
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == root)
            .unwrap()
            .submenu_presentation = crate::radial::model::SubmenuPresentation::SameCenter;
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == child_id)
            .unwrap()
            .submenu_presentation = crate::radial::model::SubmenuPresentation::Cascade;
        let document = std::sync::Arc::new(document);
        let mut session =
            RadialAuthoringSession::new(crate::radial::authoring::AuthoringSnapshot::new(
                std::sync::Arc::clone(&document),
                "preview-frozen-placement",
            ));
        let (client, endpoint) = crate::radial::authoring::authoring_control_service();
        let mut preview = EmbeddedPreview::default();

        sync_current(&mut preview, &mut session, &client);
        let request = endpoint.request_rx.try_recv().unwrap();
        let crate::radial::authoring::AuthoringRequest::PrepareEmbeddedPreview {
            id,
            generation,
            editor_session,
            candidate,
            menu_id,
            anchor,
            work_area,
            scale,
            token,
            projection,
            ..
        } = request
        else {
            panic!("embedded root preparation was not requested")
        };
        let synthetic_work_area = PhysicalRect {
            min: PhysicalPoint { x: 0.0, y: 0.0 },
            max: PhysicalPoint { x: 480.0, y: 480.0 },
        };
        assert_eq!(anchor, PhysicalPoint { x: 240.0, y: 240.0 });
        assert_eq!(work_area, synthetic_work_area);
        assert_eq!(scale, ScaleFactor::new(1.0).unwrap());
        assert_eq!(projection.placement, PreviewPlacement::FlexibleRoot);
        let mut preparer =
            crate::radial::preparation::PreviewFramePreparer::new(Default::default());
        let input = std::sync::Arc::new(
            crate::radial::authoring::native_preview::build_preview_frame_input_projected(
                &mut preparer,
                &candidate,
                &menu_id,
                anchor,
                work_area,
                scale,
                generation.0,
                None,
                &projection,
            )
            .unwrap(),
        );
        let visible_center = input.layout.origin;
        let frozen_scale = input.layout.scale_factor;
        assert_eq!(visible_center, anchor);
        assert!(session.accept_reply(
            crate::radial::authoring::AuthoringReply::EmbeddedPreviewPrepared {
                id,
                generation,
                editor_session,
                token,
                input,
            }
        ));
        preview.sync_preparation(
            &mut session,
            Some(&client),
            PreviewPreset::Current,
            None,
            TooltipPreferences::default(),
        );
        assert_eq!(preview.frozen_center, Some(visible_center));

        preview.activate(&document, None, &CellId::new("starter-root-favorites"));
        assert_eq!(preview.current_menu(), Some(&child_id));
        sync_current(&mut preview, &mut session, &client);
        let child_request = endpoint.request_rx.try_recv().unwrap();
        let crate::radial::authoring::AuthoringRequest::PrepareEmbeddedPreview {
            menu_id,
            anchor,
            work_area,
            scale,
            projection,
            ..
        } = child_request
        else {
            panic!("embedded child preparation was not requested")
        };
        assert_eq!(menu_id, child_id);
        assert_eq!(anchor, visible_center);
        assert_eq!(work_area, synthetic_work_area);
        assert_eq!(scale, frozen_scale);
        assert_eq!(projection.placement, PreviewPlacement::FixedCenter);
    }

    #[test]
    fn embedded_preview_cascade_same_center_chain_keeps_ancestors_inert_and_back_exact() {
        let mut document = RadialDocument::starter();
        let root = document.default_menu_id.clone();
        let favorites = MenuId::new("starter-favorites");
        let applications = MenuId::new("starter-applications");
        let root_menu = document
            .menus
            .iter_mut()
            .find(|menu| menu.id == root)
            .unwrap();
        root_menu.submenu_presentation = crate::radial::model::SubmenuPresentation::Cascade;
        root_menu.rings[0].radius = 40.0;
        let favorites_menu = document
            .menus
            .iter_mut()
            .find(|menu| menu.id == favorites)
            .unwrap();
        favorites_menu.submenu_presentation = crate::radial::model::SubmenuPresentation::SameCenter;
        favorites_menu.rings[0].cells[0].content = CellContent::Submenu {
            menu_id: applications.clone(),
        };
        let document = std::sync::Arc::new(document);
        let mut session =
            RadialAuthoringSession::new(crate::radial::authoring::AuthoringSnapshot::new(
                std::sync::Arc::clone(&document),
                "preview-mixed-placement",
            ));
        let (client, endpoint) = crate::radial::authoring::authoring_control_service();
        let mut preview = EmbeddedPreview::default();
        let mut preparer =
            crate::radial::preparation::PreviewFramePreparer::new(Default::default());

        sync_current(&mut preview, &mut session, &client);
        let root_frame = finish_pending_preparation(
            &mut preview,
            &mut session,
            &client,
            &endpoint,
            &mut preparer,
        );
        let root_center = root_frame.layout.origin;

        preview.activate(&document, None, &CellId::new("starter-root-favorites"));
        sync_current(&mut preview, &mut session, &client);
        let child_request = endpoint
            .request_rx
            .try_recv()
            .expect("child navigation should enqueue exactly one preparation request");
        let crate::radial::authoring::AuthoringRequest::PrepareEmbeddedPreview {
            projection, ..
        } = &child_request
        else {
            panic!("expected Cascade child preparation")
        };
        assert_eq!(
            projection.placement,
            PreviewPlacement::Cascade {
                fallback_center: root_center,
            }
        );
        let child_frame = finish_preparation_request(
            &mut preview,
            &mut session,
            &client,
            child_request,
            &mut preparer,
        );
        assert_eq!(child_frame.placement, PreparedPlacement::Cascade);
        assert_ne!(child_frame.layout.origin, root_center);
        let retained_root_cell = child_frame
            .layout
            .cells
            .iter()
            .find(|cell| cell.cell_id.as_str() == "starter-root-favorites")
            .expect("Cascade child retains the displayed parent cells");
        assert!(!retained_root_cell.actionable);
        let child_center = child_frame.layout.origin;
        let child_frame_id = preview.reducer.as_ref().unwrap().state.stack[1].frame_id;

        preview.activate(&document, None, &CellId::new("starter-favorites-source"));
        assert_eq!(preview.current_menu(), Some(&applications));
        sync_current(&mut preview, &mut session, &client);
        let grandchild_request = endpoint
            .request_rx
            .try_recv()
            .expect("SameCenter navigation should enqueue one grandchild preparation");
        let crate::radial::authoring::AuthoringRequest::PrepareEmbeddedPreview {
            anchor,
            projection,
            ..
        } = &grandchild_request
        else {
            panic!("expected SameCenter grandchild preparation")
        };
        assert_eq!(*anchor, child_center);
        assert_eq!(projection.placement, PreviewPlacement::FixedCenter);
        let grandchild_frame = finish_preparation_request(
            &mut preview,
            &mut session,
            &client,
            grandchild_request,
            &mut preparer,
        );
        assert_eq!(grandchild_frame.layout.origin, child_center);
        assert!(
            grandchild_frame
                .layout
                .cells
                .iter()
                .any(|cell| cell.cell_id.as_str() == "__center" && cell.actionable)
        );
        assert!(
            !grandchild_frame
                .layout
                .cells
                .iter()
                .any(|cell| cell.cell_id.as_str() == "starter-root-favorites")
        );

        preview.activate(&document, None, &CellId::new("__center"));
        assert_eq!(
            preview
                .reducer
                .as_ref()
                .unwrap()
                .state
                .stack
                .last()
                .unwrap()
                .frame_id,
            child_frame_id
        );
        sync_current(&mut preview, &mut session, &client);
        assert!(endpoint.request_rx.try_recv().is_err());
        assert_eq!(
            preview.prepared_frame(&session).unwrap().as_ref(),
            child_frame.as_ref()
        );
        preview.activate(&document, None, &CellId::new("__center"));
        assert_eq!(preview.current_menu(), Some(&root));
        sync_current(&mut preview, &mut session, &client);
        assert!(endpoint.request_rx.try_recv().is_err());
        assert_eq!(
            preview.prepared_frame(&session).unwrap().as_ref(),
            root_frame.as_ref()
        );
    }

    #[test]
    fn embedded_preview_simulates_drag_and_page_without_dispatching() {
        let mut document = RadialDocument::starter();
        let menu_index = document
            .menus
            .iter()
            .position(|menu| menu.id.as_str() == "starter-applications")
            .unwrap();
        document.menus[menu_index].center_control = Some(crate::radial::model::Control::Drag);
        let menu_id = document.menus[menu_index].id.clone();
        let dynamic =
            crate::radial::preparation::synthetic_preview_dynamic(&document.menus[menu_index]);
        let mut preparer =
            crate::radial::preparation::PreviewFramePreparer::new(Default::default());
        let prepared = preparer
            .prepare(
                &document,
                &menu_id,
                PhysicalPoint { x: 240.0, y: 240.0 },
                PhysicalRect {
                    min: PhysicalPoint { x: 0.0, y: 0.0 },
                    max: PhysicalPoint { x: 480.0, y: 480.0 },
                },
                ScaleFactor::new(1.0).unwrap(),
                11,
                None,
                &crate::radial::preparation::PreviewProjection {
                    dynamic,
                    ..Default::default()
                },
            )
            .unwrap();
        let page_cell = prepared
            .layout
            .cells
            .iter()
            .find(|cell| cell.cell_id.as_str().starts_with("__radial_page_next:"))
            .unwrap()
            .cell_id
            .clone();
        let mut preview = EmbeddedPreview::default();
        preview.reset(&document, &menu_id, 11);
        preview.reducer.as_mut().unwrap().state.stack[0].page_count = prepared.page_count;
        preview.activate(&document, None, &page_cell);
        assert_eq!(preview.reducer.as_ref().unwrap().state.stack[0].page, 1);
        preview.begin_drag(
            &document,
            &menu_id,
            &CellId::new("__center"),
            LogicalPoint { x: 240.0, y: 240.0 },
            11,
        );
        preview.continue_drag(
            LogicalPoint { x: 260.0, y: 240.0 },
            Some(CellId::new("__center")),
            11,
        );
        assert_eq!(preview.simulated_drags, 1);
        assert_eq!(preview.intercepted_dispatches, 0);
    }

    #[test]
    fn representative_preview_uses_selected_menu_and_unreferenced_skin() {
        let mut document = RadialDocument::starter();
        let root = document.menus[0].id.clone();
        let mut other = document.menus[0].clone();
        other.id = MenuId::new("other");
        other.name = "Other".into();
        document.menus.push(other.clone());
        let mut skin = document.skins[0].clone();
        skin.id = SkinId::new("unreferenced");
        document.skins.push(skin.clone());
        let preview =
            representative_document(&document, PreviewPreset::Current, &other.id, Some(&skin.id));
        assert_eq!(
            preview
                .menus
                .iter()
                .find(|menu| menu.id == other.id)
                .unwrap()
                .skin_id,
            skin.id
        );
        assert_eq!(
            preview
                .menus
                .iter()
                .find(|menu| menu.id == root)
                .unwrap()
                .skin_id,
            document.menus[0].skin_id
        );
    }
}
