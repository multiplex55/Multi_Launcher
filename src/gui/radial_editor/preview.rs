use crate::radial::authoring::StableSelection;
use crate::radial::authoring::{AuthoringClient, RadialAuthoringSession};
use crate::radial::compositor::CompositorCache;
use crate::radial::geometry::{LogicalPoint, PhysicalPoint, PhysicalRect, ScaleFactor};
use crate::radial::model::{
    CellContent, CellId, InvocationId, MenuId, RadialDocument, SessionId, SkinId,
};
use crate::radial::preparation::PreparedFrameInput;
use crate::radial::session::{CellRole, SessionEvent, SessionIntent, SessionReducer};
use eframe::egui;

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
        session
            .embedded_preview
            .as_ref()
            .filter(|(token, _)| token == &self.frame_token)
            .map(|(_, input)| std::sync::Arc::clone(input))
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
        let frame_token = format!(
            "{}:{menu_id}:{selected:?}:{selected_skin:?}:{page}:{preset:?}:assets={}",
            session.generation.0,
            session.pending_assets.preview_identity()
        );
        self.frame_token = frame_token.clone();
        if let Some((token, input)) = session.embedded_preview.as_ref()
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
        #[cfg(test)]
        {
            self.preparation_attempts += 1;
        }
        let request = session.request_embedded_preview(
            std::sync::Arc::new(document),
            menu_id,
            selected,
            PhysicalPoint { x: 240.0, y: 240.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 480.0, y: 480.0 },
            },
            ScaleFactor::new(scale).unwrap(),
            frame_token.clone(),
            page,
            selected_skin,
        );
        match request {
            Ok(request) => {
                let correlation = (request.id(), request.generation(), request.editor_session());
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

    fn fail_preparation(&mut self, fingerprint: String, message: String) {
        self.failed_frame_token = Some(fingerprint);
        self.preparation_notice = Some(super::ResourceNotice::error(message));
    }

    fn retry_preparation(&mut self) {
        self.failed_frame_token = None;
        self.preparation_notice = None;
    }

    pub(super) fn reset(&mut self, document: &RadialDocument, menu_id: &MenuId, generation: u64) {
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
            let _ = reducer.reduce(SessionEvent::Back {
                geometry_generation: self.document_generation,
                pointer_baseline: LogicalPoint { x: 240.0, y: 240.0 },
            });
        }
    }

    pub(super) fn activate(&mut self, document: &RadialDocument, cell_id: &CellId) {
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
        let event = match cell.map(|cell| &cell.content) {
            Some(CellContent::Submenu { menu_id }) => SessionEvent::OpenChild {
                menu_id: menu_id.clone(),
                origin: PhysicalPoint { x: 240.0, y: 240.0 },
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
                                origin: PhysicalPoint { x: 240.0, y: 240.0 },
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
