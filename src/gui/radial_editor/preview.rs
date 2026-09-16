use crate::radial::authoring::StableSelection;
use crate::radial::authoring::{AuthoringClient, RadialAuthoringSession};
use crate::radial::compositor::CompositorCache;
use crate::radial::geometry::{
    LogicalPoint, PhysicalPoint, PhysicalRect, ScaleFactor, cascade_layout, shape_center,
};
use crate::radial::model::{
    CellContent, CellId, InvocationId, MenuId, RadialDocument, SessionId, SkinId,
};
use crate::radial::preparation::{
    PreparedFrameInput, PreparedPlacement, PreviewPlacement, ensure_preview_center_back,
};
use crate::radial::render::build_scene_prepared_selected;
use crate::radial::session::{CellRole, FrameId, SessionEvent, SessionIntent, SessionReducer};
use eframe::egui;
use std::collections::BTreeMap;

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
    pub(super) simulated_drags: usize,
    drag_cell: Option<CellId>,
    navigation_frames: BTreeMap<FrameId, (String, std::sync::Arc<PreparedFrameInput>)>,
    pending_frame_id: Option<FrameId>,
    pending_frame_token: Option<String>,
    frozen_center: Option<PhysicalPoint>,
    frozen_scale: Option<ScaleFactor>,
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
            simulated_drags: 0,
            drag_cell: None,
            navigation_frames: BTreeMap::new(),
            pending_frame_id: None,
            pending_frame_token: None,
            frozen_center: None,
            frozen_scale: None,
            #[cfg(test)]
            preparation_attempts: 0,
        }
    }
}

impl EmbeddedPreview {
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
            "{}:{menu_id}:{selected:?}:{selected_skin:?}:{page}:{preset:?}:assets={}",
            session.generation.0,
            session.pending_assets.preview_identity()
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
        let request = session.request_embedded_preview_placed(
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

    pub(super) fn activate(&mut self, document: &RadialDocument, cell_id: &CellId) {
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
        let cell = document
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| {
                menu.rings
                    .iter()
                    .flat_map(|ring| &ring.cells)
                    .find(|cell| &cell.id == cell_id)
            });
        let role = cell_role(document, &menu_id, cell_id);
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
                SessionIntent::Dispatch { .. } => self.intercepted_dispatches += 1,
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
        let synthetic = representative_document(document, preset, &root, selected_skin.as_ref());
        let document = &synthetic;
        let mut menu_id = self
            .current_menu()
            .cloned()
            .filter(|id| document.menus.iter().any(|menu| &menu.id == id))
            .unwrap_or_else(|| root.clone());
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
        if let Some(notice) = &self.preparation_notice {
            super::show_resource_notice(ui, notice);
            if ui.button("Retry preview preparation").clicked() {
                self.retry_preparation();
            }
            return;
        }
        let Some(input) = prepared else {
            ui.colored_label(ui.visuals().error_fg_color, "Preparing preview resources…");
            return;
        };
        let layout = input.layout.clone();
        for diagnostic in &input.diagnostics {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("Preview warning: {diagnostic}"),
            );
        }
        let scale_factor = layout.scale_factor;
        let Ok(frame) = self.compositor.compose(&input.scene, scale_factor, 0) else {
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
        ui.horizontal(|ui| {
            if ui.button("Back").clicked() {
                self.back();
            }
            ui.label(format!("Menu: {}", menu.name));
        });
        if let Some(texture) = &self.texture {
            let response = ui.add(
                egui::Image::new(texture)
                    .fit_to_exact_size(egui::vec2(360.0, 360.0) * zoom)
                    .sense(egui::Sense::click_and_drag()),
            );
            if response.clicked()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let delta = pointer - response.rect.min;
                let size = response.rect.size();
                let uv = egui::vec2(delta.x / size.x, delta.y / size.y);
                let point = LogicalPoint {
                    x: layout.visual_extent.min.x
                        + uv.x * (layout.visual_extent.max.x - layout.visual_extent.min.x),
                    y: layout.visual_extent.min.y
                        + uv.y * (layout.visual_extent.max.y - layout.visual_extent.min.y),
                };
                if let Some(cell) = layout.hit_test(point) {
                    self.activate(document, &cell.cell_id);
                }
            }
            if response.drag_started()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let point = preview_pointer_point(pointer, response.rect, &layout);
                if let Some(cell) = layout.hit_test(point) {
                    self.begin_drag(document, &menu.id, &cell.cell_id, point, generation);
                }
            }
            if response.dragged()
                && self.drag_cell.is_some()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let point = preview_pointer_point(pointer, response.rect, &layout);
                self.continue_drag(
                    point,
                    layout.hit_test(point).map(|cell| cell.cell_id.clone()),
                    generation,
                );
            }
            if response.drag_stopped() {
                self.drag_cell = None;
            }
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

fn cell_role(document: &RadialDocument, menu_id: &MenuId, cell_id: &CellId) -> CellRole {
    if cell_id.as_str().starts_with("__radial_page_next:") {
        return CellRole::NextPage;
    }
    if cell_id.as_str().starts_with("__radial_page_previous:") {
        return CellRole::PreviousPage;
    }
    if cell_id.as_str().starts_with("dyn:") {
        return CellRole::Action;
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

    fn sync_current(
        preview: &mut EmbeddedPreview,
        session: &mut RadialAuthoringSession,
        client: &AuthoringClient,
    ) {
        preview.sync_preparation(session, Some(client), PreviewPreset::Current, None);
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
        preview.activate(&document, &submenu);
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
        preview.activate(&document, &action);
        assert_eq!(preview.intercepted_dispatches, 1);
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
        preview.sync_preparation(&mut session, Some(&client), PreviewPreset::Current, None);
        assert_eq!(preview.frozen_center, Some(visible_center));

        preview.activate(&document, &CellId::new("starter-root-favorites"));
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

        preview.activate(&document, &CellId::new("starter-root-favorites"));
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

        preview.activate(&document, &CellId::new("starter-favorites-source"));
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

        preview.activate(&document, &CellId::new("__center"));
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
        preview.activate(&document, &CellId::new("__center"));
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
        preview.activate(&document, &page_cell);
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
