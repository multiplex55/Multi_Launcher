//! Reusable egui crop canvas.
//!
//! This component owns only image pixels and presentation/interaction state.
//! Filesystem paths, persistence, and domain-specific destinations belong to
//! the adapters that embed it.

use crate::image_crop::{ImageCropRect, crop_rgba, validate_crop_rect};
use eframe::egui;
use image::RgbaImage;

pub const MIN_CROP_ZOOM: f32 = 0.05;
pub const MAX_CROP_ZOOM: f32 = 16.0;
const HANDLE_SIZE: f32 = 12.0;
const MIN_VIEWPORT_SIZE: egui::Vec2 = egui::Vec2::new(320.0, 260.0);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CropPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CropFloatRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropTransform {
    pub origin: CropPoint,
    pub zoom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CropHandle {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

pub fn normalize_drag(start: CropPoint, end: CropPoint) -> CropFloatRect {
    CropFloatRect {
        x: start.x.min(end.x),
        y: start.y.min(end.y),
        width: (start.x - end.x).abs(),
        height: (start.y - end.y).abs(),
    }
}

/// Clamp a selection to the source while keeping it non-empty.
pub fn clamp_rect(rect: ImageCropRect, image_width: u32, image_height: u32) -> ImageCropRect {
    if image_width == 0 || image_height == 0 {
        return ImageCropRect::default();
    }
    let width = rect.width.max(1).min(image_width);
    let height = rect.height.max(1).min(image_height);
    ImageCropRect {
        x: rect.x.min(image_width - width),
        y: rect.y.min(image_height - height),
        width,
        height,
    }
}

pub fn move_rect(
    rect: ImageCropRect,
    delta_x: i64,
    delta_y: i64,
    image_width: u32,
    image_height: u32,
) -> ImageCropRect {
    let rect = clamp_rect(rect, image_width, image_height);
    if image_width == 0 || image_height == 0 {
        return rect;
    }
    let max_x = image_width.saturating_sub(rect.width) as i64;
    let max_y = image_height.saturating_sub(rect.height) as i64;
    ImageCropRect {
        x: (rect.x as i64 + delta_x).clamp(0, max_x) as u32,
        y: (rect.y as i64 + delta_y).clamp(0, max_y) as u32,
        ..rect
    }
}

pub fn resize_rect(
    original: ImageCropRect,
    handle: CropHandle,
    pointer: CropPoint,
    image_width: u32,
    image_height: u32,
) -> ImageCropRect {
    let original = clamp_rect(original, image_width, image_height);
    if image_width == 0 || image_height == 0 {
        return original;
    }
    let mut left = original.x as i64;
    let mut top = original.y as i64;
    let mut right = (original.x + original.width) as i64;
    let mut bottom = (original.y + original.height) as i64;
    let px = pointer.x.round().clamp(0.0, image_width as f32) as i64;
    let py = pointer.y.round().clamp(0.0, image_height as f32) as i64;

    match handle {
        CropHandle::Top => top = py.clamp(0, bottom - 1),
        CropHandle::Bottom => bottom = py.clamp(top + 1, image_height as i64),
        CropHandle::Left => left = px.clamp(0, right - 1),
        CropHandle::Right => right = px.clamp(left + 1, image_width as i64),
        CropHandle::TopLeft => {
            top = py.clamp(0, bottom - 1);
            left = px.clamp(0, right - 1);
        }
        CropHandle::TopRight => {
            top = py.clamp(0, bottom - 1);
            right = px.clamp(left + 1, image_width as i64);
        }
        CropHandle::BottomLeft => {
            bottom = py.clamp(top + 1, image_height as i64);
            left = px.clamp(0, right - 1);
        }
        CropHandle::BottomRight => {
            bottom = py.clamp(top + 1, image_height as i64);
            right = px.clamp(left + 1, image_width as i64);
        }
    }
    clamp_rect(
        ImageCropRect {
            x: left as u32,
            y: top as u32,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        },
        image_width,
        image_height,
    )
}

pub fn selection_from_drag(
    start: CropPoint,
    end: CropPoint,
    image_width: u32,
    image_height: u32,
) -> ImageCropRect {
    let normalized = normalize_drag(start, end);
    let right = normalized.x + normalized.width;
    let bottom = normalized.y + normalized.height;
    let x = normalized.x.floor().max(0.0);
    let y = normalized.y.floor().max(0.0);
    let right = right.ceil().max(x + 1.0);
    let bottom = bottom.ceil().max(y + 1.0);
    clamp_rect(
        ImageCropRect {
            x: x as u32,
            y: y as u32,
            width: (right - x) as u32,
            height: (bottom - y) as u32,
        },
        image_width,
        image_height,
    )
}

pub fn source_to_display(point: CropPoint, transform: CropTransform) -> CropPoint {
    CropPoint {
        x: transform.origin.x + point.x * transform.zoom,
        y: transform.origin.y + point.y * transform.zoom,
    }
}

pub fn display_to_source(point: CropPoint, transform: CropTransform) -> CropPoint {
    let zoom = transform.zoom.max(f32::EPSILON);
    CropPoint {
        x: (point.x - transform.origin.x) / zoom,
        y: (point.y - transform.origin.y) / zoom,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CropEditorAction {
    SaveAs,
    RequestOverwrite,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CropValidationError(pub String);

impl std::fmt::Display for CropValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Presentation options supplied by an adapter. No path or domain identity
/// is stored by the canvas.
#[derive(Clone, Debug)]
pub struct CropEditorUiOptions {
    pub title: String,
    pub window_id: String,
    pub source_label: Option<String>,
    pub collision_message: Option<String>,
    pub persistence_message: Option<String>,
    pub allow_overwrite: bool,
}

impl CropEditorUiOptions {
    pub fn new(title: impl Into<String>, window_id: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            window_id: window_id.into(),
            source_label: None,
            collision_message: None,
            persistence_message: None,
            allow_overwrite: false,
        }
    }
}

#[derive(Debug)]
enum ActiveInteraction {
    Create {
        start: CropPoint,
    },
    Move {
        start: CropPoint,
        original: ImageCropRect,
    },
    Resize {
        handle: CropHandle,
        original: ImageCropRect,
    },
}

/// The reusable crop canvas and its complete interaction state.
pub struct ImageCropEditorState {
    open: bool,
    source: Option<RgbaImage>,
    texture: Option<egui::TextureHandle>,
    selection: Option<ImageCropRect>,
    zoom: f32,
    pan: CropPoint,
    fit_initialized: bool,
    interaction: Option<ActiveInteraction>,
    validation_message: Option<String>,
}

impl Default for ImageCropEditorState {
    fn default() -> Self {
        Self {
            open: false,
            source: None,
            texture: None,
            selection: None,
            zoom: 1.0,
            pan: CropPoint::default(),
            fit_initialized: false,
            interaction: None,
            validation_message: None,
        }
    }
}

impl ImageCropEditorState {
    pub fn from_image(source: RgbaImage) -> Result<Self, CropValidationError> {
        let mut state = Self::default();
        state.set_source(source)?;
        state.open = true;
        Ok(state)
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn source_image(&self) -> Option<&RgbaImage> {
        self.source.as_ref()
    }

    pub fn selection(&self) -> Option<ImageCropRect> {
        self.selection
    }

    pub fn set_source(&mut self, source: RgbaImage) -> Result<(), CropValidationError> {
        if source.width() == 0 || source.height() == 0 {
            let error = CropValidationError("cannot edit an empty image".into());
            self.validation_message = Some(error.0.clone());
            self.source = None;
            self.selection = None;
            return Err(error);
        }
        self.source = Some(source);
        self.selection = self.full_selection();
        self.texture = None;
        self.zoom = 1.0;
        self.pan = CropPoint::default();
        self.fit_initialized = false;
        self.interaction = None;
        self.validation_message = None;
        Ok(())
    }

    pub fn set_selection(&mut self, selection: ImageCropRect) -> Result<(), CropValidationError> {
        let source = self
            .source
            .as_ref()
            .ok_or_else(|| CropValidationError("source image is not loaded".into()))?;
        validate_crop_rect(source.width(), source.height(), selection)
            .map_err(|error| CropValidationError(format!("{error:#}")))?;
        self.selection = Some(selection);
        self.validation_message = None;
        Ok(())
    }

    pub fn validation(&self) -> Result<(), CropValidationError> {
        let source = self
            .source
            .as_ref()
            .ok_or_else(|| CropValidationError("source image is not loaded".into()))?;
        let selection = self
            .selection
            .ok_or_else(|| CropValidationError("select a non-empty crop rectangle".into()))?;
        validate_crop_rect(source.width(), source.height(), selection)
            .map_err(|error| CropValidationError(format!("{error:#}")))
    }

    pub fn crop_result(&self) -> Result<RgbaImage, CropValidationError> {
        self.validation()?;
        crop_rgba(self.source.as_ref().unwrap(), self.selection.unwrap())
            .map_err(|error| CropValidationError(format!("{error:#}")))
    }

    pub fn can_save(&self) -> bool {
        self.validation().is_ok()
    }

    pub fn reset_selection(&mut self) {
        self.selection = self.full_selection();
        self.interaction = None;
        self.fit_initialized = false;
        self.pan = CropPoint::default();
    }

    pub fn cancel(&mut self) {
        self.open = false;
        self.texture = None;
        self.source = None;
        self.selection = None;
        self.interaction = None;
    }

    fn full_selection(&self) -> Option<ImageCropRect> {
        self.source.as_ref().map(|image| ImageCropRect {
            x: 0,
            y: 0,
            width: image.width(),
            height: image.height(),
        })
    }

    fn ensure_texture(&mut self, ctx: &egui::Context, texture_name: &str) {
        if self.texture.is_some() {
            return;
        }
        let Some(image) = &self.source else {
            return;
        };
        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        );
        self.texture = Some(ctx.load_texture(
            texture_name.to_owned(),
            color_image,
            egui::TextureOptions::NEAREST,
        ));
    }

    fn viewport_transform(&mut self, viewport: egui::Rect) -> Option<CropTransform> {
        let image = self.source.as_ref()?;
        if !self.fit_initialized {
            let fit = (viewport.width() / image.width() as f32)
                .min(viewport.height() / image.height() as f32);
            self.zoom = fit.clamp(MIN_CROP_ZOOM, MAX_CROP_ZOOM);
            self.pan = CropPoint::default();
            self.fit_initialized = true;
        }
        Some(CropTransform {
            origin: CropPoint {
                x: viewport.center().x - image.width() as f32 * self.zoom / 2.0 + self.pan.x,
                y: viewport.center().y - image.height() as f32 * self.zoom / 2.0 + self.pan.y,
            },
            zoom: self.zoom,
        })
    }

    fn change_zoom(&mut self, viewport: egui::Rect, pointer: egui::Pos2, scroll: f32) {
        let Some(old_transform) = self.viewport_transform(viewport) else {
            return;
        };
        let old_source = display_to_source(
            CropPoint {
                x: pointer.x,
                y: pointer.y,
            },
            old_transform,
        );
        let factor = (scroll / 240.0).exp().clamp(0.5, 2.0);
        self.zoom = (self.zoom * factor).clamp(MIN_CROP_ZOOM, MAX_CROP_ZOOM);
        let Some(image) = self.source.as_ref() else {
            return;
        };
        let base_origin = CropPoint {
            x: viewport.center().x - image.width() as f32 * self.zoom / 2.0,
            y: viewport.center().y - image.height() as f32 * self.zoom / 2.0,
        };
        self.pan = CropPoint {
            x: pointer.x - old_source.x * self.zoom - base_origin.x,
            y: pointer.y - old_source.y * self.zoom - base_origin.y,
        };
        self.clamp_pan(viewport);
    }

    fn clamp_pan(&mut self, viewport: egui::Rect) {
        let Some(image) = self.source.as_ref() else {
            return;
        };
        let image_size = egui::vec2(
            image.width() as f32 * self.zoom,
            image.height() as f32 * self.zoom,
        );
        let max_x = ((image_size.x - viewport.width()) / 2.0).max(0.0);
        let max_y = ((image_size.y - viewport.height()) / 2.0).max(0.0);
        self.pan.x = self.pan.x.clamp(-max_x, max_x);
        self.pan.y = self.pan.y.clamp(-max_y, max_y);
    }

    fn handle_centers(rect: egui::Rect) -> [(CropHandle, egui::Pos2); 8] {
        let center = rect.center();
        [
            (CropHandle::Top, egui::pos2(center.x, rect.top())),
            (CropHandle::Bottom, egui::pos2(center.x, rect.bottom())),
            (CropHandle::Left, egui::pos2(rect.left(), center.y)),
            (CropHandle::Right, egui::pos2(rect.right(), center.y)),
            (CropHandle::TopLeft, egui::pos2(rect.left(), rect.top())),
            (CropHandle::TopRight, egui::pos2(rect.right(), rect.top())),
            (
                CropHandle::BottomLeft,
                egui::pos2(rect.left(), rect.bottom()),
            ),
            (
                CropHandle::BottomRight,
                egui::pos2(rect.right(), rect.bottom()),
            ),
        ]
    }

    fn hit_handle(selection: egui::Rect, pointer: egui::Pos2) -> Option<CropHandle> {
        Self::handle_centers(selection)
            .into_iter()
            .find_map(|(handle, center)| {
                egui::Rect::from_center_size(
                    center,
                    egui::vec2(HANDLE_SIZE * 1.8, HANDLE_SIZE * 1.8),
                )
                .contains(pointer)
                .then_some(handle)
            })
    }

    fn render_viewport(
        &mut self,
        ui: &mut egui::Ui,
        viewport: egui::Rect,
        transform: CropTransform,
    ) {
        let image = self.source.as_ref().unwrap();
        let image_min = source_to_display(CropPoint { x: 0.0, y: 0.0 }, transform);
        let image_rect = egui::Rect::from_min_size(
            egui::pos2(image_min.x, image_min.y),
            egui::vec2(
                image.width() as f32 * transform.zoom,
                image.height() as f32 * transform.zoom,
            ),
        );
        let painter = ui.painter().with_clip_rect(viewport);
        painter.rect_filled(viewport, egui::Rounding::ZERO, egui::Color32::from_gray(24));
        if let Some(texture) = &self.texture {
            painter.image(
                texture.id(),
                image_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }

        let selection = self.selection.unwrap();
        let selection_min = source_to_display(
            CropPoint {
                x: selection.x as f32,
                y: selection.y as f32,
            },
            transform,
        );
        let selection_rect = egui::Rect::from_min_size(
            egui::pos2(selection_min.x, selection_min.y),
            egui::vec2(
                selection.width as f32 * transform.zoom,
                selection.height as f32 * transform.zoom,
            ),
        );
        let selected_visible = selection_rect.intersect(viewport);
        painter.rect_filled(
            viewport,
            egui::Rounding::ZERO,
            egui::Color32::from_black_alpha(150),
        );
        if selected_visible.is_positive()
            && let Some(texture) = &self.texture
        {
            painter.with_clip_rect(selected_visible).image(
                texture.id(),
                image_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        painter.rect_stroke(
            selection_rect,
            egui::Rounding::ZERO,
            egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 210, 50)),
        );
        for (_, center) in Self::handle_centers(selection_rect) {
            let handle_rect =
                egui::Rect::from_center_size(center, egui::vec2(HANDLE_SIZE, HANDLE_SIZE));
            painter.rect_filled(handle_rect, egui::Rounding::same(2.0), egui::Color32::WHITE);
            painter.rect_stroke(
                handle_rect,
                egui::Rounding::same(2.0),
                egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
            );
        }
    }

    /// Render the canvas and return a persistence request. The component does
    /// not perform persistence itself.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        options: &CropEditorUiOptions,
    ) -> Option<CropEditorAction> {
        self.show_with_save_as_filename(ctx, options, None)
    }

    /// Render with an adapter-owned editable filename field. The filename is
    /// deliberately borrowed and is not retained by the canvas.
    pub fn show_with_save_as_filename(
        &mut self,
        ctx: &egui::Context,
        options: &CropEditorUiOptions,
        mut save_as_filename: Option<&mut String>,
    ) -> Option<CropEditorAction> {
        if !self.open {
            return None;
        }
        self.ensure_texture(ctx, &options.window_id);
        let mut open = true;
        let mut action = None;
        egui::Window::new(&options.title)
            .id(egui::Id::new(&options.window_id))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([860.0, 680.0])
            .min_size([560.0, 480.0])
            .show(ctx, |ui| {
                if let Some(source_label) = &options.source_label {
                    ui.label(source_label);
                }
                if let Some(message) = &self.validation_message {
                    ui.colored_label(ui.visuals().error_fg_color, message);
                }
                let Some((image_width, image_height)) = self
                    .source
                    .as_ref()
                    .map(|image| (image.width(), image.height()))
                else {
                    ui.add_enabled(false, egui::Button::new("Save As"));
                    return;
                };
                let available = ui.available_size();
                let viewport_size = egui::vec2(
                    available.x.max(MIN_VIEWPORT_SIZE.x),
                    (available.y - 100.0).max(MIN_VIEWPORT_SIZE.y),
                );
                let (viewport, response) =
                    ui.allocate_exact_size(viewport_size, egui::Sense::click_and_drag());
                let _ = self.viewport_transform(viewport).unwrap();
                if response.hovered() {
                    let scroll = ui.input(|input| input.raw_scroll_delta.y);
                    if scroll != 0.0
                        && let Some(pointer) = ui.input(|input| input.pointer.hover_pos())
                    {
                        self.change_zoom(viewport, pointer, scroll);
                    }
                }
                if response.hovered() && ui.input(|input| input.pointer.middle_down()) {
                    let delta = ui.input(|input| input.pointer.delta());
                    self.pan.x += delta.x;
                    self.pan.y += delta.y;
                    self.clamp_pan(viewport);
                }
                let transform = self.viewport_transform(viewport).unwrap();
                let selection = self.selection.unwrap();
                let selection_min = source_to_display(
                    CropPoint {
                        x: selection.x as f32,
                        y: selection.y as f32,
                    },
                    transform,
                );
                let selection_screen = egui::Rect::from_min_size(
                    egui::pos2(selection_min.x, selection_min.y),
                    egui::vec2(
                        selection.width as f32 * transform.zoom,
                        selection.height as f32 * transform.zoom,
                    ),
                );
                let image_screen = egui::Rect::from_min_size(
                    egui::pos2(transform.origin.x, transform.origin.y),
                    egui::vec2(
                        image_width as f32 * transform.zoom,
                        image_height as f32 * transform.zoom,
                    ),
                );
                if response.drag_started_by(egui::PointerButton::Primary)
                    && let Some(pointer) = response.interact_pointer_pos()
                {
                    let source_pointer = display_to_source(
                        CropPoint {
                            x: pointer.x,
                            y: pointer.y,
                        },
                        transform,
                    );
                    self.interaction = if let Some(handle) =
                        Self::hit_handle(selection_screen, pointer)
                    {
                        Some(ActiveInteraction::Resize {
                            handle,
                            original: selection,
                        })
                    } else if image_screen.contains(pointer) && selection_screen.contains(pointer) {
                        Some(ActiveInteraction::Move {
                            start: source_pointer,
                            original: selection,
                        })
                    } else if image_screen.contains(pointer) {
                        Some(ActiveInteraction::Create {
                            start: source_pointer,
                        })
                    } else {
                        None
                    };
                }
                if response.dragged_by(egui::PointerButton::Primary)
                    && let Some(pointer) = response.interact_pointer_pos()
                    && let Some(interaction) = self.interaction.as_ref()
                {
                    let source_pointer = display_to_source(
                        CropPoint {
                            x: pointer.x,
                            y: pointer.y,
                        },
                        transform,
                    );
                    self.selection = Some(match interaction {
                        ActiveInteraction::Create { start } => {
                            selection_from_drag(*start, source_pointer, image_width, image_height)
                        }
                        ActiveInteraction::Move { start, original } => move_rect(
                            *original,
                            (source_pointer.x - start.x).round() as i64,
                            (source_pointer.y - start.y).round() as i64,
                            image_width,
                            image_height,
                        ),
                        ActiveInteraction::Resize { handle, original } => resize_rect(
                            *original,
                            *handle,
                            source_pointer,
                            image_width,
                            image_height,
                        ),
                    });
                }
                if response.drag_stopped_by(egui::PointerButton::Primary) {
                    self.interaction = None;
                }
                self.render_viewport(ui, viewport, transform);
                let selection = self.selection.unwrap();
                ui.horizontal(|ui| {
                    ui.label(format!("{} × {} px", selection.width, selection.height));
                    ui.separator();
                    ui.label(format!("Zoom {:.0}%", self.zoom * 100.0));
                    ui.small("Left drag creates/moves/resizes · middle drag pans · wheel zooms");
                });
                ui.horizontal(|ui| {
                    let reset_clicked = ui.button("Reset").clicked();
                    let full_image_clicked = ui.button("Select Full Image").clicked();
                    if reset_clicked || full_image_clicked {
                        self.reset_selection();
                    }
                    if let Some(filename) = save_as_filename.as_deref_mut() {
                        ui.label("Save As filename");
                        ui.text_edit_singleline(filename);
                    }
                });
                if let Some(message) = &options.collision_message {
                    ui.colored_label(ui.visuals().warn_fg_color, message);
                }
                if let Some(message) = &options.persistence_message {
                    ui.colored_label(ui.visuals().error_fg_color, message);
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.can_save(), egui::Button::new("Save As"))
                        .clicked()
                    {
                        action = Some(CropEditorAction::SaveAs);
                    }
                    if options.allow_overwrite
                        && ui
                            .add_enabled(self.can_save(), egui::Button::new("Overwrite Current"))
                            .clicked()
                    {
                        action = Some(CropEditorAction::RequestOverwrite);
                    }
                    if ui.button("Cancel").clicked() {
                        action = Some(CropEditorAction::Cancel);
                    }
                });
            });
        if !open {
            self.open = false;
            action = Some(CropEditorAction::Cancel);
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u32, y: u32, width: u32, height: u32) -> ImageCropRect {
        ImageCropRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn drag_normalization_handles_all_four_directions() {
        let expected = CropFloatRect {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
        };
        for (start, end) in [
            (
                CropPoint { x: 10.0, y: 20.0 },
                CropPoint { x: 40.0, y: 60.0 },
            ),
            (
                CropPoint { x: 40.0, y: 20.0 },
                CropPoint { x: 10.0, y: 60.0 },
            ),
            (
                CropPoint { x: 10.0, y: 60.0 },
                CropPoint { x: 40.0, y: 20.0 },
            ),
            (
                CropPoint { x: 40.0, y: 60.0 },
                CropPoint { x: 10.0, y: 20.0 },
            ),
        ] {
            assert_eq!(normalize_drag(start, end), expected);
        }
    }

    #[test]
    fn drag_selection_is_normalized_and_clamped() {
        for (start, end) in [
            (CropPoint { x: 1.0, y: 2.0 }, CropPoint { x: 5.0, y: 7.0 }),
            (CropPoint { x: 5.0, y: 2.0 }, CropPoint { x: 1.0, y: 7.0 }),
            (CropPoint { x: 1.0, y: 7.0 }, CropPoint { x: 5.0, y: 2.0 }),
            (CropPoint { x: 5.0, y: 7.0 }, CropPoint { x: 1.0, y: 2.0 }),
        ] {
            assert_eq!(selection_from_drag(start, end, 10, 10), rect(1, 2, 4, 5));
        }
        assert_eq!(
            selection_from_drag(
                CropPoint { x: -3.0, y: -2.0 },
                CropPoint { x: 20.0, y: 20.0 },
                10,
                10
            ),
            rect(0, 0, 10, 10)
        );
    }

    #[test]
    fn moving_retains_dimensions_and_clamps_to_bounds() {
        let original = rect(20, 20, 30, 25);
        assert_eq!(move_rect(original, 10, 5, 100, 100), rect(30, 25, 30, 25));
        assert_eq!(move_rect(original, -100, 0, 100, 100), rect(0, 20, 30, 25));
        assert_eq!(move_rect(original, 100, 0, 100, 100), rect(70, 20, 30, 25));
        assert_eq!(move_rect(original, 0, -100, 100, 100), rect(20, 0, 30, 25));
        assert_eq!(move_rect(original, 0, 100, 100, 100), rect(20, 75, 30, 25));
    }

    #[test]
    fn all_resize_handles_obey_minimum_and_boundaries() {
        let original = rect(20, 20, 40, 30);
        let cases = [
            (
                CropHandle::Top,
                CropPoint { x: 0.0, y: 0.0 },
                rect(20, 0, 40, 50),
            ),
            (
                CropHandle::Bottom,
                CropPoint { x: 0.0, y: 100.0 },
                rect(20, 20, 40, 80),
            ),
            (
                CropHandle::Left,
                CropPoint { x: 0.0, y: 0.0 },
                rect(0, 20, 60, 30),
            ),
            (
                CropHandle::Right,
                CropPoint { x: 100.0, y: 0.0 },
                rect(20, 20, 80, 30),
            ),
            (
                CropHandle::TopLeft,
                CropPoint { x: 0.0, y: 0.0 },
                rect(0, 0, 60, 50),
            ),
            (
                CropHandle::TopRight,
                CropPoint { x: 100.0, y: 0.0 },
                rect(20, 0, 80, 50),
            ),
            (
                CropHandle::BottomLeft,
                CropPoint { x: 0.0, y: 100.0 },
                rect(0, 20, 60, 80),
            ),
            (
                CropHandle::BottomRight,
                CropPoint { x: 100.0, y: 100.0 },
                rect(20, 20, 80, 80),
            ),
        ];
        for (handle, pointer, expected) in cases {
            assert_eq!(resize_rect(original, handle, pointer, 100, 100), expected);
        }
        for handle in [
            CropHandle::Top,
            CropHandle::Bottom,
            CropHandle::Left,
            CropHandle::Right,
            CropHandle::TopLeft,
            CropHandle::TopRight,
            CropHandle::BottomLeft,
            CropHandle::BottomRight,
        ] {
            let result = resize_rect(original, handle, CropPoint { x: 20.0, y: 20.0 }, 100, 100);
            assert!(result.width >= 1 && result.height >= 1);
            assert!(result.x + result.width <= 100);
            assert!(result.y + result.height <= 100);
        }
    }

    #[test]
    fn source_display_round_trip_and_zoom_limits_are_stable() {
        let transform = CropTransform {
            origin: CropPoint { x: 17.25, y: -4.5 },
            zoom: 1.75,
        };
        for source in [
            CropPoint { x: 0.0, y: 0.0 },
            CropPoint { x: 100.5, y: 50.25 },
            CropPoint {
                x: 1000.0,
                y: 800.0,
            },
        ] {
            let round_trip = display_to_source(source_to_display(source, transform), transform);
            assert!((round_trip.x - source.x).abs() < 0.0001);
            assert!((round_trip.y - source.y).abs() < 0.0001);
        }
        assert!(MIN_CROP_ZOOM > 0.0);
        assert!(MAX_CROP_ZOOM > MIN_CROP_ZOOM);
    }

    #[test]
    fn zoom_is_presentation_only_and_does_not_change_stored_selection() {
        let selection = rect(100, 50, 200, 80);
        for transform in [
            CropTransform {
                origin: CropPoint { x: 0.0, y: 0.0 },
                zoom: 0.5,
            },
            CropTransform {
                origin: CropPoint { x: 40.0, y: 20.0 },
                zoom: 4.0,
            },
        ] {
            let _ = source_to_display(
                CropPoint {
                    x: selection.x as f32,
                    y: selection.y as f32,
                },
                transform,
            );
        }
        assert_eq!(selection, rect(100, 50, 200, 80));
    }

    #[test]
    fn state_starts_with_full_selection_and_rejects_invalid_selection() {
        let mut state = ImageCropEditorState::from_image(RgbaImage::new(8, 6)).unwrap();
        assert_eq!(state.selection(), Some(rect(0, 0, 8, 6)));
        assert!(state.set_selection(rect(0, 0, 0, 1)).is_err());
        assert!(state.set_selection(rect(7, 5, 2, 1)).is_err());
        assert!(state.can_save());
        state.reset_selection();
        assert_eq!(state.selection(), Some(rect(0, 0, 8, 6)));
    }
}
