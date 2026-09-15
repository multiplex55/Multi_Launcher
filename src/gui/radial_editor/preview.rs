use crate::radial::authoring::StableSelection;
use crate::radial::authoring::native_preview::build_preview_frame_input;
use crate::radial::compositor::CompositorCache;
use crate::radial::geometry::{LogicalPoint, PhysicalPoint, PhysicalRect, ScaleFactor};
use crate::radial::model::{
    CellContent, CellId, InvocationId, MenuId, RadialDocument, SessionId, SkinId,
};
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
    compositor: CompositorCache,
    texture: Option<egui::TextureHandle>,
    pub(super) intercepted_dispatches: usize,
}

impl Default for EmbeddedPreview {
    fn default() -> Self {
        Self {
            reducer: None,
            root: None,
            document_generation: 0,
            selection_token: String::new(),
            compositor: CompositorCache::default(),
            texture: None,
            intercepted_dispatches: 0,
        }
    }
}

impl EmbeddedPreview {
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
        let Some(cell) = document
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| {
                menu.rings
                    .iter()
                    .flat_map(|ring| &ring.cells)
                    .find(|cell| &cell.id == cell_id)
            })
        else {
            return;
        };
        let role = match &cell.content {
            CellContent::Action { .. } | CellContent::Dynamic { .. } => CellRole::Action,
            CellContent::Submenu { .. } => CellRole::Submenu,
            CellContent::Control {
                control: crate::radial::model::Control::Back,
            } => CellRole::Back,
            CellContent::Control {
                control: crate::radial::model::Control::Close,
            } => CellRole::Close,
            _ => CellRole::Spacer,
        };
        let event = match &cell.content {
            CellContent::Submenu { menu_id } => SessionEvent::OpenChild {
                menu_id: menu_id.clone(),
                origin: PhysicalPoint { x: 240.0, y: 240.0 },
                geometry_generation: self.document_generation,
                pointer_baseline: LogicalPoint { x: 240.0, y: 240.0 },
            },
            CellContent::Control {
                control: crate::radial::model::Control::Back,
            } => SessionEvent::Back {
                geometry_generation: self.document_generation,
                pointer_baseline: LogicalPoint { x: 240.0, y: 240.0 },
            },
            _ => SessionEvent::ActivateItem {
                cell: cell_id.clone(),
                role,
                gesture: crate::radial::model::ClickGesture::Primary,
                source: crate::commands::ActivationSource::Click,
                geometry_generation: self.document_generation,
            },
        };
        let intents = self.reducer.as_mut().unwrap().reduce(event);
        for intent in intents {
            match intent {
                SessionIntent::Dispatch { .. } => self.intercepted_dispatches += 1,
                SessionIntent::OpenSubmenu { .. } => {
                    if let CellContent::Submenu { menu_id } = &cell.content {
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
                SessionIntent::CloseTree | SessionIntent::PageChanged { .. } => {}
            }
        }
    }

    pub(super) fn ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &RadialDocument,
        generation: u64,
        zoom: f32,
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
        let work = PhysicalRect {
            min: PhysicalPoint { x: 0.0, y: 0.0 },
            max: PhysicalPoint { x: 480.0, y: 480.0 },
        };
        let scale = if preset == PreviewPreset::HighDpi {
            2.0
        } else {
            1.0
        };
        let scale_factor = ScaleFactor::new(scale).unwrap();
        let Ok(input) = build_preview_frame_input(
            document,
            &menu.id,
            PhysicalPoint { x: 240.0, y: 240.0 },
            work,
            scale_factor,
            generation,
            self.reducer
                .as_ref()
                .and_then(|reducer| reducer.state.selected.as_ref()),
        ) else {
            ui.colored_label(egui::Color32::RED, "Preview layout is invalid");
            return;
        };
        let layout = input.layout;
        let Ok(frame) = self.compositor.compose(&input.scene, scale_factor, 0) else {
            ui.colored_label(egui::Color32::RED, "Preview compositor failed");
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
                    .sense(egui::Sense::click()),
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
        }
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
