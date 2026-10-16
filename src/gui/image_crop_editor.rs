//! Reusable egui crop canvas.
//!
//! This component owns only image pixels and presentation/interaction state.
//! Filesystem paths, persistence, and domain-specific destinations belong to
//! the adapters that embed it.

use crate::gui::theme::widgets::{ButtonEmphasis, with_button_emphasis};
use crate::image_crop::{ImageCropRect, crop_rgba, validate_crop_rect};
use eframe::egui;
use image::RgbaImage;

pub const MIN_CROP_ZOOM: f32 = 0.05;
pub const MAX_CROP_ZOOM: f32 = 16.0;
const HANDLE_SIZE: f32 = 12.0;
const MIN_VIEWPORT_SIZE: egui::Vec2 = egui::Vec2::new(320.0, 260.0);

#[cfg(test)]
#[derive(Clone, Copy)]
struct CropChromeObservation {
    window_rect: Option<egui::Rect>,
    metadata: Option<(egui::Id, egui::Rect, egui::Vec2, egui::Vec2)>,
    source_label: Option<(egui::Id, egui::Rect)>,
    collision_label: Option<(egui::Id, egui::Rect)>,
    persistence_label: Option<(egui::Id, egui::Rect)>,
    filename_label: Option<(egui::Id, egui::Rect)>,
    filename_edit: Option<(egui::Id, egui::Rect)>,
    viewport: egui::Rect,
    reset_button: (egui::Id, egui::Rect),
    full_image_button: (egui::Id, egui::Rect),
    save_as_button: (egui::Id, egui::Rect, bool),
    overwrite_button: Option<(egui::Id, egui::Rect, bool)>,
    cancel_button: (egui::Id, egui::Rect),
    footer_reserved_height: f32,
}

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
    #[cfg(test)]
    test_chrome: Option<CropChromeObservation>,
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
            #[cfg(test)]
            test_chrome: None,
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
        painter.rect_filled(
            viewport,
            egui::CornerRadius::ZERO,
            egui::Color32::from_gray(24),
        );
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
            egui::CornerRadius::ZERO,
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
            egui::CornerRadius::ZERO,
            egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 210, 50)),
            egui::StrokeKind::Middle,
        );
        for (_, center) in Self::handle_centers(selection_rect) {
            let handle_rect =
                egui::Rect::from_center_size(center, egui::vec2(HANDLE_SIZE, HANDLE_SIZE));
            painter.rect_filled(
                handle_rect,
                egui::CornerRadius::same(2),
                egui::Color32::WHITE,
            );
            painter.rect_stroke(
                handle_rect,
                egui::CornerRadius::same(2),
                egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
                egui::StrokeKind::Middle,
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
        save_as_filename: Option<&mut String>,
    ) -> Option<CropEditorAction> {
        if !self.open {
            return None;
        }
        self.ensure_texture(ctx, &options.window_id);
        let mut open = true;
        let mut action = None;
        #[cfg(test)]
        let mut test_chrome = None;
        let available_rect = ctx.content_rect();
        let max_window_width = available_rect.width().max(1.0);
        let max_window_height = available_rect.height().max(1.0);
        let window_output = egui::Window::new(&options.title)
            .id(egui::Id::new(&options.window_id))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .max_width(max_window_width)
            .max_height(max_window_height)
            .default_size([860.0_f32.min(max_window_width), 680.0_f32.min(max_window_height)])
            .min_size([560.0_f32.min(max_window_width), 480.0_f32.min(max_window_height)])
            .show(ctx, |ui| {
                #[cfg(test)]
                let mut filename_label_response = None;
                #[cfg(test)]
                let mut filename_edit_response = None;
                if let Some(filename) = save_as_filename {
                    let label = ui.label("Save As filename");
                    #[cfg(test)]
                    {
                        filename_label_response = Some(label);
                    }
                    #[cfg(not(test))]
                    let _ = label;
                    let edit = ui.add(
                        egui::TextEdit::singleline(filename).desired_width(ui.available_width()),
                    );
                    #[cfg(test)]
                    {
                        filename_edit_response = Some(edit);
                    }
                    #[cfg(not(test))]
                    let _ = edit;
                }
                let footer_reserved_height = crop_editor_footer_reserved_height(
                    ui,
                    options.allow_overwrite,
                );
                let metadata_height = crop_metadata_height_budget(
                    ui,
                    ui.available_height(),
                    footer_reserved_height,
                );
                #[cfg(test)]
                let mut source_label_response = None;
                #[cfg(test)]
                let mut collision_label_response = None;
                #[cfg(test)]
                let mut persistence_label_response = None;
                let metadata = egui::ScrollArea::vertical()
                    .id_salt("crop_editor_metadata")
                    .auto_shrink([false, true])
                    .min_scrolled_height(0.0)
                    .max_height(metadata_height)
                    .show(ui, |ui| {
                        if let Some(source_label) = &options.source_label {
                            let response = ui.add(
                                egui::Label::new(source_label)
                                    .wrap_mode(egui::TextWrapMode::Wrap),
                            );
                            #[cfg(test)]
                            {
                                source_label_response = Some(response);
                            }
                            #[cfg(not(test))]
                            let _ = response;
                        }
                        if let Some(message) = &self.validation_message {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(message.as_str())
                                        .color(ui.visuals().error_fg_color),
                                )
                                .wrap_mode(egui::TextWrapMode::Wrap),
                            );
                        }
                        if self.source.is_some() {
                            let selection = self.selection.unwrap();
                            ui.horizontal_wrapped(|ui| {
                                ui.label(format!(
                                    "{} × {} px",
                                    selection.width, selection.height
                                ));
                                ui.label(format!("Zoom {:.0}%", self.zoom * 100.0));
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(
                                            "Left drag creates/moves/resizes · middle drag pans · wheel zooms",
                                        )
                                        .small(),
                                    )
                                    .wrap_mode(egui::TextWrapMode::Wrap),
                                );
                            });
                        }
                        if let Some(message) = &options.collision_message {
                            let response = ui.add(
                                egui::Label::new(
                                    egui::RichText::new(message.as_str())
                                        .color(ui.visuals().warn_fg_color),
                                )
                                .wrap_mode(egui::TextWrapMode::Wrap),
                            );
                            #[cfg(test)]
                            {
                                collision_label_response = Some(response);
                            }
                            #[cfg(not(test))]
                            let _ = response;
                        }
                        if let Some(message) = &options.persistence_message {
                            let response = ui.add(
                                egui::Label::new(
                                    egui::RichText::new(message.as_str())
                                        .color(ui.visuals().error_fg_color),
                                )
                                .wrap_mode(egui::TextWrapMode::Wrap),
                            );
                            #[cfg(test)]
                            {
                                persistence_label_response = Some(response);
                            }
                            #[cfg(not(test))]
                            let _ = response;
                        }
                    });
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
                    available.x.max(1.0),
                    (available.y - footer_reserved_height).max(1.0),
                );
                let (viewport, response) =
                    ui.allocate_exact_size(viewport_size, egui::Sense::click_and_drag());
                let _ = self.viewport_transform(viewport).unwrap();
                if response.hovered() {
                    let scroll = ui.input(|input| input.smooth_scroll_delta.y);
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
                ui.separator();
                let (reset_button, full_image_button) = ui
                    .horizontal_wrapped(|ui| {
                        let reset = ui.button("Reset");
                        let full_image = ui.button("Select Full Image");
                        let reset_clicked = reset.clicked();
                        let full_image_clicked = full_image.clicked();
                        if reset_clicked || full_image_clicked {
                            self.reset_selection();
                        }
                        (reset, full_image)
                    })
                    .inner;
                let (save_as_button, overwrite_button, cancel_button) = ui
                    .horizontal_wrapped(|ui| {
                        let save_as = ui.add_enabled(self.can_save(), egui::Button::new("Save As"));
                        let save_as = with_button_emphasis(ui, save_as, ButtonEmphasis::Primary);
                        if save_as.clicked() {
                            action = Some(CropEditorAction::SaveAs);
                        }
                        let overwrite = options.allow_overwrite.then(|| {
                            let button = ui.add_enabled(
                                self.can_save(),
                                egui::Button::new("Overwrite Current"),
                            );
                            with_button_emphasis(ui, button, ButtonEmphasis::Destructive)
                        });
                        if overwrite.as_ref().is_some_and(egui::Response::clicked) {
                            action = Some(CropEditorAction::RequestOverwrite);
                        }
                        let cancel = ui.button("Cancel");
                        if cancel.clicked() {
                            action = Some(CropEditorAction::Cancel);
                        }
                        (save_as, overwrite, cancel)
                    })
                    .inner;
                #[cfg(test)]
                {
                    test_chrome = Some(CropChromeObservation {
                        window_rect: None,
                        metadata: Some((
                            metadata.id,
                            metadata.inner_rect,
                            metadata.content_size,
                            metadata.state.offset,
                        )),
                        source_label: source_label_response
                            .map(|response| (response.id, response.rect)),
                        collision_label: collision_label_response
                            .map(|response| (response.id, response.rect)),
                        persistence_label: persistence_label_response
                            .map(|response| (response.id, response.rect)),
                        filename_label: filename_label_response
                            .map(|response| (response.id, response.rect)),
                        filename_edit: filename_edit_response
                            .map(|response| (response.id, response.rect)),
                        viewport,
                        reset_button: (reset_button.id, reset_button.rect),
                        full_image_button: (full_image_button.id, full_image_button.rect),
                        save_as_button: (
                            save_as_button.id,
                            save_as_button.rect,
                            save_as_button.enabled(),
                        ),
                        overwrite_button: overwrite_button.map(|response| {
                            (response.id, response.rect, response.enabled())
                        }),
                        cancel_button: (cancel_button.id, cancel_button.rect),
                        footer_reserved_height,
                    });
                }
                #[cfg(not(test))]
                let _ = (
                    metadata,
                    reset_button,
                    full_image_button,
                    save_as_button,
                    overwrite_button,
                    cancel_button,
                );
            });
        #[cfg(test)]
        {
            self.test_chrome = test_chrome.map(|mut observation| {
                observation.window_rect = window_output.map(|output| output.response.rect);
                observation
            });
        }
        #[cfg(not(test))]
        let _ = window_output;
        if !open {
            self.open = false;
            action = Some(CropEditorAction::Cancel);
        }
        action
    }
}

fn crop_metadata_height_budget(
    ui: &egui::Ui,
    available_height: f32,
    footer_reserved_height: f32,
) -> f32 {
    let available_for_metadata = (available_height - footer_reserved_height).max(0.0);
    let body_font = egui::FontSelection::Default.resolve(ui.style());
    let readable_row = ui.fonts_mut(|fonts| fonts.row_height(&body_font))
        + ui.spacing().extra_text_line_spacing
        + ui.spacing().item_spacing.y;
    let preserve_canvas = (available_for_metadata - MIN_VIEWPORT_SIZE.y).max(0.0);
    preserve_canvas
        .max(readable_row.min(available_for_metadata))
        .min(available_for_metadata)
}

fn crop_editor_footer_reserved_height(ui: &egui::Ui, allow_overwrite: bool) -> f32 {
    let primary_actions = button_row_reserved_height(ui, &["Reset", "Select Full Image"]);
    let save_actions = if allow_overwrite {
        button_row_reserved_height(ui, &["Save As", "Overwrite Current", "Cancel"])
    } else {
        button_row_reserved_height(ui, &["Save As", "Cancel"])
    };
    let spacing = ui.spacing().clone();
    let separator_spacing = ui
        .style()
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let height = primary_actions
        + spacing.item_spacing.y
        + save_actions
        + 2.0 * spacing.item_spacing.y
        + separator_spacing;
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    (height * pixels_per_point).ceil() / pixels_per_point
}

fn button_row_reserved_height(ui: &egui::Ui, labels: &[&str]) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let native_button_style = style.button_style(
        &egui::widget_style::Classes::default(),
        egui::widget_style::WidgetState::Inactive,
    );
    let button_font = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| native_button_style.text_style.font_id.clone());
    let frame_margin = if ui.visuals().button_frame {
        native_button_style.frame.total_margin().sum()
    } else {
        egui::Vec2::ZERO
    };
    let available_width = ui.available_width().max(1.0);
    let text_width = (available_width - frame_margin.x).max(1.0);
    let wrap_mode = ui.wrap_mode();
    let mut row_width = 0.0;
    let mut row_height = 0.0f32;
    let mut actions_height = 0.0;

    for label in labels {
        let text_size = egui::WidgetText::from(*label)
            .into_galley(ui, Some(wrap_mode), text_width, button_font.clone())
            .size();
        let button_width = (text_size.x + frame_margin.x)
            .max(spacing.interact_size.x)
            .min(available_width);
        let button_height = (text_size.y + spacing.extra_text_line_spacing + frame_margin.y)
            .max(spacing.interact_size.y);
        if row_width > 0.0 && row_width + spacing.item_spacing.x + button_width > available_width {
            actions_height += row_height + spacing.item_spacing.y;
            row_width = button_width;
            row_height = button_height;
        } else {
            if row_width > 0.0 {
                row_width += spacing.item_spacing.x;
            }
            row_width += button_width;
            row_height = row_height.max(button_height);
        }
    }
    actions_height + row_height.max(spacing.interact_size.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crop_input(size: egui::Vec2) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            focused: true,
            ..Default::default()
        }
    }

    fn render_crop_frame(
        ctx: &egui::Context,
        state: &mut ImageCropEditorState,
        options: &CropEditorUiOptions,
        filename: &mut String,
        size: egui::Vec2,
    ) -> (egui::FullOutput, Option<CropEditorAction>) {
        let mut action = None;
        let output = ctx.run_ui(crop_input(size), |ctx| {
            action = state.show_with_save_as_filename(ctx, options, Some(filename));
        });
        (output, action)
    }

    fn collect_text_shapes(
        shape: &egui::epaint::Shape,
        clip_rect: egui::Rect,
        output: &mut Vec<(String, egui::Rect, egui::Rect, bool)>,
    ) {
        match shape {
            egui::epaint::Shape::Text(text) => output.push((
                text.galley.job.text.clone(),
                egui::Rect::from_min_size(text.pos, text.galley.size()),
                clip_rect,
                text.galley.elided,
            )),
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text_shapes(shape, clip_rect, output);
                }
            }
            _ => {}
        }
    }

    fn visible_text_shapes(
        output: &egui::FullOutput,
        bounds: egui::Rect,
    ) -> Vec<(String, egui::Rect, egui::Rect)> {
        let mut texts = Vec::new();
        for clipped_shape in &output.shapes {
            collect_text_shapes(&clipped_shape.shape, clipped_shape.clip_rect, &mut texts);
        }
        texts
            .into_iter()
            .filter_map(|(text, galley_rect, clip_rect, elided)| {
                (!elided
                    && bounds.intersects(galley_rect)
                    && clip_rect.intersects(galley_rect)
                    && galley_rect.left() >= bounds.left() - 1.0
                    && galley_rect.right() <= bounds.right() + 1.0
                    && galley_rect.left() >= clip_rect.left() - 1.0
                    && galley_rect.right() <= clip_rect.right() + 1.0)
                    .then_some((text, galley_rect, clip_rect))
            })
            .collect()
    }

    fn settle_metadata_target(
        ctx: &egui::Context,
        state: &mut ImageCropEditorState,
        options: &CropEditorUiOptions,
        filename: &mut String,
        size: egui::Vec2,
        observation: CropChromeObservation,
        target_rect: egui::Rect,
        align_bottom: bool,
    ) -> Option<(
        CropChromeObservation,
        Vec<(String, egui::Rect, egui::Rect)>,
        Option<CropEditorAction>,
    )> {
        let (scroll_id, scroll_rect, content_size, offset) = observation.metadata?;
        let mut scroll = egui::scroll_area::State::load(ctx, scroll_id)?;
        let max_offset = (content_size.y - scroll_rect.height()).max(0.0);
        let target_content_y = if align_bottom {
            target_rect.bottom() + offset.y - scroll_rect.bottom()
        } else {
            target_rect.top() + offset.y - scroll_rect.top()
        };
        scroll.offset = egui::vec2(0.0, target_content_y.clamp(0.0, max_offset));
        scroll.store(ctx, scroll_id);

        let mut latest = None;
        let mut visible_text = Vec::new();
        let mut action = None;
        for _ in 0..2 {
            let (output, frame_action) = render_crop_frame(ctx, state, options, filename, size);
            latest = state.test_chrome;
            visible_text = latest
                .and_then(|layout| layout.metadata.map(|(_, rect, _, _)| rect))
                .map(|rect| visible_text_shapes(&output, rect))
                .unwrap_or_default();
            action = frame_action;
            output.drop_without_applying_deltas();
        }
        latest.map(|layout| (layout, visible_text, action))
    }

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

    #[test]
    fn crop_chrome_bounds_long_metadata_and_keeps_canvas_and_actions_reachable() {
        let ctx = egui::Context::default();
        let source = RgbaImage::from_fn(64, 48, |x, y| {
            image::Rgba([x as u8, y as u8, (x + y) as u8, 255])
        });
        let mut state = ImageCropEditorState::from_image(source).unwrap();
        let selection = rect(4, 6, 29, 21);
        state.set_selection(selection).unwrap();
        let source_bytes = state.source_image().unwrap().as_raw().clone();
        let source_label = "Source: C:\\Users\\Jay\\Pictures\\Launch captures\\2026\\October\\The very long original image filename with a detailed subject and full-resolution dimensions.png".to_owned();
        let collision_message = "Destination already exists: C:\\Users\\Jay\\Pictures\\Launch captures\\2026\\October\\Exports\\The detailed crop of the original high-resolution image already exists and must be replaced or saved under another name.png".to_owned();
        let persistence_message = "Could not save C:\\Users\\Jay\\Pictures\\Launch captures\\2026\\October\\Exports\\The detailed crop of the original high-resolution image because the destination is read-only. The current source image and crop selection remain unchanged.".to_owned();
        let mut options = CropEditorUiOptions::new("Crop image", "crop_chrome_contract");
        options.source_label = Some(source_label.clone());
        options.collision_message = Some(collision_message.clone());
        options.persistence_message = Some(persistence_message.clone());
        options.allow_overwrite = true;
        let mut filename = "detailed-crop".to_owned();
        let initial_size = egui::vec2(852.0, 460.0);
        let mut initial_layout = None;
        let mut initial_text = Vec::new();
        let mut initial_action = None;
        for _ in 0..3 {
            let (output, action) =
                render_crop_frame(&ctx, &mut state, &options, &mut filename, initial_size);
            initial_layout = state.test_chrome;
            initial_text = initial_layout
                .and_then(|layout| layout.metadata.map(|(_, rect, _, _)| rect))
                .map(|rect| visible_text_shapes(&output, rect))
                .unwrap_or_default();
            initial_action = action;
            output.drop_without_applying_deltas();
        }

        let initial_layout = initial_layout.expect("settled crop frame captures its responses");
        let _source_rect = initial_layout
            .source_label
            .map(|(_, rect)| rect)
            .expect("source response is captured");
        let collision_rect = initial_layout
            .collision_label
            .map(|(_, rect)| rect)
            .expect("collision response is captured");
        let (collision_top, collision_top_text, collision_action) = settle_metadata_target(
            &ctx,
            &mut state,
            &options,
            &mut filename,
            initial_size,
            initial_layout,
            collision_rect,
            false,
        )
        .expect("native scroll state and collision response are available");
        let collision_bottom_rect = collision_top
            .collision_label
            .map(|(_, rect)| rect)
            .expect("collision response remains captured after scrolling");
        let (collision_bottom, collision_bottom_text, collision_bottom_action) =
            settle_metadata_target(
                &ctx,
                &mut state,
                &options,
                &mut filename,
                initial_size,
                collision_top,
                collision_bottom_rect,
                true,
            )
            .expect("collision can be scrolled to its lower lines");
        let persistence_rect = collision_bottom
            .persistence_label
            .map(|(_, rect)| rect)
            .expect("persistence response remains captured after scrolling");
        let (persistence_top, persistence_top_text, persistence_action) = settle_metadata_target(
            &ctx,
            &mut state,
            &options,
            &mut filename,
            initial_size,
            collision_bottom,
            persistence_rect,
            false,
        )
        .expect("persistence response can be scrolled into view");
        let persistence_bottom_rect = persistence_top
            .persistence_label
            .map(|(_, rect)| rect)
            .expect("persistence response remains captured after scrolling");
        let (persistence_bottom, persistence_bottom_text, persistence_bottom_action) =
            settle_metadata_target(
                &ctx,
                &mut state,
                &options,
                &mut filename,
                initial_size,
                persistence_top,
                persistence_bottom_rect,
                true,
            )
            .expect("persistence text can be scrolled through to its final lines");

        let window_rect = persistence_bottom
            .window_rect
            .expect("crop window response is captured");
        let (metadata_id, metadata_rect, content_size, scroll_offset) = persistence_bottom
            .metadata
            .expect("metadata ScrollArea exposes its native state and viewport");
        let (_, initial_metadata_rect, _, _) = initial_layout
            .metadata
            .expect("initial crop frame exposes its metadata viewport");
        let max_scroll_offset = (content_size.y - metadata_rect.height()).max(0.0);
        let save_as_rect = persistence_bottom.save_as_button.1;
        let overwrite_rect = persistence_bottom
            .overwrite_button
            .map(|(_, rect, _)| rect)
            .expect("overwrite response is present for existing sources");
        let label_bounds = |texts: &[(String, egui::Rect, egui::Rect)], expected: &str| {
            texts
                .iter()
                .find(|(text, text_rect, clip_rect)| {
                    text == expected
                        && metadata_rect.intersects(*text_rect)
                        && clip_rect.intersects(*text_rect)
                })
                .map(|(_, text_rect, _)| *text_rect)
        };
        let initial_source_bounds = label_bounds(&initial_text, &source_label);
        let collision_top_bounds = label_bounds(&collision_top_text, &collision_message);
        let collision_bottom_bounds = label_bounds(&collision_bottom_text, &collision_message);
        let persistence_top_bounds = label_bounds(&persistence_top_text, &persistence_message);
        let persistence_bottom_bounds =
            label_bounds(&persistence_bottom_text, &persistence_message);

        assert!(
            initial_source_bounds.is_some_and(|rect| initial_metadata_rect.intersects(rect)),
            "full source text is emitted inside the bounded body"
        );
        assert!(
            collision_top_bounds.is_some_and(|rect| rect.top() >= metadata_rect.top() - 1.0),
            "collision text can be reached from its first lines"
        );
        assert!(
            collision_bottom_bounds
                .is_some_and(|rect| rect.bottom() <= metadata_rect.bottom() + 1.0),
            "collision text can be reached through its final lines"
        );
        assert!(
            persistence_top_bounds.is_some_and(|rect| rect.top() >= metadata_rect.top() - 1.0),
            "persistence text can be reached from its first lines"
        );
        assert!(
            persistence_bottom_bounds
                .is_some_and(|rect| rect.bottom() <= metadata_rect.bottom() + 1.0),
            "persistence text can be reached through its final lines"
        );
        let workspace_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, initial_size);
        assert!(workspace_rect.contains_rect(window_rect));
        assert!(window_rect.contains_rect(metadata_rect));
        assert!(window_rect.contains_rect(persistence_bottom.viewport));
        assert!(window_rect.contains_rect(save_as_rect));
        assert!(window_rect.contains_rect(overwrite_rect));
        assert!(window_rect.contains_rect(persistence_bottom.cancel_button.1));
        assert!(window_rect.contains_rect(persistence_bottom.filename_label.unwrap().1));
        let filename_edit_rect = persistence_bottom.filename_edit.unwrap().1;
        assert!(window_rect.contains_rect(filename_edit_rect));
        assert!(filename_edit_rect.bottom() <= metadata_rect.top());
        assert!(metadata_rect.bottom() <= persistence_bottom.viewport.top());
        assert!(persistence_bottom.viewport.bottom() <= persistence_bottom.reset_button.1.top());
        assert!(persistence_bottom.viewport.width() >= MIN_VIEWPORT_SIZE.x);
        assert!(persistence_bottom.viewport.height() >= MIN_VIEWPORT_SIZE.y * 0.75);
        assert!(persistence_bottom.footer_reserved_height > 0.0);
        assert!(
            window_rect.bottom() - persistence_bottom.viewport.bottom()
                >= persistence_bottom.footer_reserved_height - 2.0
        );
        assert!(persistence_bottom.save_as_button.2);
        assert!(persistence_bottom.overwrite_button.unwrap().2);
        assert!(initial_layout.reset_button.0 != initial_layout.full_image_button.0);
        assert!(initial_layout.save_as_button.0 != persistence_bottom.cancel_button.0);
        assert!(
            persistence_bottom.save_as_button.0 != persistence_bottom.overwrite_button.unwrap().0
        );
        assert!(
            persistence_bottom.overwrite_button.unwrap().0 != persistence_bottom.cancel_button.0
        );
        assert!(scroll_offset.y >= -0.5 && scroll_offset.y <= max_scroll_offset + 0.5);
        assert_eq!(
            egui::scroll_area::State::load(&ctx, metadata_id).map(|scroll| scroll.offset.y),
            Some(scroll_offset.y)
        );
        assert!(content_size.y > metadata_rect.height());
        assert_eq!(initial_action, None);
        assert_eq!(collision_action, None);
        assert_eq!(collision_bottom_action, None);
        assert_eq!(persistence_action, None);
        assert_eq!(persistence_bottom_action, None);
        assert_eq!(state.source_image().unwrap().as_raw(), &source_bytes);
        assert_eq!(state.selection(), Some(selection));
        assert_eq!(filename, "detailed-crop");
    }
}
