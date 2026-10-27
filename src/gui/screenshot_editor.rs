use crate::annotation::raster::{
    self, Color as RasterColor, Point as RasterPoint, Rect as RasterRect,
};
use crate::annotation::redaction::{self, RedactionError, RedactionOperation};
use crate::gui::LauncherApp;
use eframe::egui::{
    self, Color32, PointerButton, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2,
};
use egui::emath::GuiRounding as _;
use egui_toast::{Toast, ToastKind, ToastOptions};
use image::RgbaImage;
use std::path::PathBuf;
#[cfg(feature = "qa-framebuffer-capture")]
use std::{fs::OpenOptions, io::Read, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkupTool {
    Pen,
    Arrow,
    Rectangle,
    Highlight,
    Marker,
    Text,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarkupStroke {
    pub points: Vec<Pos2>,
    pub color: Color32,
    pub thickness: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarkupRect {
    pub rect: Rect,
    pub color: Color32,
    pub thickness: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarkupArrow {
    pub start: Pos2,
    pub end: Pos2,
    pub color: Color32,
    pub thickness: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarkupText {
    pub position: Pos2,
    pub text: String,
    pub color: Color32,
    pub size: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarkupMarker {
    pub center: Pos2,
    pub number: u64,
    pub color: Color32,
    pub size: f32,
}

/// Ephemeral state for the Text tool while the user is typing directly onto the canvas.
///
/// This intentionally captures color/size at creation time so each text instance is independent.
#[derive(Clone, Debug, PartialEq)]
struct ActiveText {
    position: Pos2,
    text: String,
    color: Color32,
    size: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MarkupLayer {
    Stroke(MarkupStroke),
    Rectangle(MarkupRect),
    Arrow(MarkupArrow),
    Highlight(MarkupRect),
    Marker(MarkupMarker),
    Text(MarkupText),
    Redaction(RedactionOperation),
}

#[derive(Clone, Debug, Default)]
pub struct MarkupHistory {
    layers: Vec<MarkupLayer>,
    redo_stack: Vec<MarkupLayer>,
}

impl MarkupHistory {
    pub fn layers(&self) -> &[MarkupLayer] {
        &self.layers
    }

    pub fn push(&mut self, layer: MarkupLayer) {
        self.layers.push(layer);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) -> bool {
        if let Some(layer) = self.layers.pop() {
            self.redo_stack.push(layer);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(layer) = self.redo_stack.pop() {
            self.layers.push(layer);
            true
        } else {
            false
        }
    }
}

fn draw_line(img: &mut RgbaImage, start: Pos2, end: Pos2, color: Color32, thickness: f32) {
    raster::draw_line(
        img,
        RasterPoint::new(start.x, start.y),
        RasterPoint::new(end.x, end.y),
        RasterColor(color.to_array()),
        thickness,
    );
}

fn draw_rect_outline(img: &mut RgbaImage, rect: Rect, color: Color32, thickness: f32) {
    raster::draw_rect_outline(
        img,
        RasterRect::from_corners(
            RasterPoint::new(rect.min.x, rect.min.y),
            RasterPoint::new(rect.max.x, rect.max.y),
        ),
        RasterColor(color.to_array()),
        thickness,
    );
}

fn draw_rect_fill(img: &mut RgbaImage, rect: Rect, color: Color32) {
    raster::draw_rect_fill(
        img,
        RasterRect::from_corners(
            RasterPoint::new(rect.min.x, rect.min.y),
            RasterPoint::new(rect.max.x, rect.max.y),
        ),
        RasterColor(color.to_array()),
    );
}

fn rotate_vec(vec: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(vec.x * cos - vec.y * sin, vec.x * sin + vec.y * cos)
}

fn default_font_arc() -> Option<(ab_glyph::FontArc, egui::FontTweak)> {
    raster::default_font_arc()
}

fn draw_text(
    img: &mut RgbaImage,
    font: &ab_glyph::FontArc,
    tweak: egui::FontTweak,
    pos: Pos2,
    text: &str,
    color: Color32,
    size: f32,
) {
    raster::draw_text(
        img,
        font,
        tweak,
        RasterPoint::new(pos.x, pos.y),
        text,
        RasterColor(color.to_array()),
        size,
    );
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MarkerGeometry {
    text_origin: Pos2,
    text_bounds: Rect,
    radius: f32,
}

fn marker_geometry(
    center: Pos2,
    number: u64,
    size: f32,
    font: Option<&(ab_glyph::FontArc, egui::FontTweak)>,
) -> MarkerGeometry {
    let label = number.to_string();
    let measured =
        font.and_then(|(font, tweak)| raster::measure_text_ink_bounds(font, tweak, &label, size));
    let fallback = raster::TextInkBounds {
        min: RasterPoint::new(0.0, size * 0.15),
        max: RasterPoint::new(size * label.chars().count() as f32 * 0.62, size * 0.88),
    };
    let measured = measured.unwrap_or(fallback);
    let measured_center = measured.center();
    let text_origin = Pos2::new(center.x - measured_center.x, center.y - measured_center.y);
    let text_bounds = Rect::from_min_max(
        text_origin + Vec2::new(measured.min.x, measured.min.y),
        text_origin + Vec2::new(measured.max.x, measured.max.y),
    );
    let padding = (size * 0.2).max(2.0);
    let radius = (measured.width().hypot(measured.height()) * 0.5 + padding).max(size * 0.55);
    MarkerGeometry {
        text_origin,
        text_bounds,
        radius,
    }
}

fn marker_text_color(background: Color32) -> Color32 {
    let linear = |channel: u8| {
        let value = channel as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * linear(background.r())
        + 0.7152 * linear(background.g())
        + 0.0722 * linear(background.b());
    if luminance > 0.179 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

pub fn render_markup_layers(
    base: &RgbaImage,
    layers: &[MarkupLayer],
) -> Result<RgbaImage, RedactionError> {
    let mut img = base.clone();
    let font = default_font_arc();
    for layer in layers {
        match layer {
            MarkupLayer::Stroke(stroke) => {
                for points in stroke.points.windows(2) {
                    draw_line(
                        &mut img,
                        points[0],
                        points[1],
                        stroke.color,
                        stroke.thickness,
                    );
                }
            }
            MarkupLayer::Rectangle(rect) => {
                draw_rect_outline(&mut img, rect.rect, rect.color, rect.thickness);
            }
            MarkupLayer::Arrow(arrow) => {
                draw_line(
                    &mut img,
                    arrow.start,
                    arrow.end,
                    arrow.color,
                    arrow.thickness,
                );
                let dir = arrow.end - arrow.start;
                let len = dir.length();
                if len > 0.5 {
                    let unit = dir / len;
                    let head_len = (10.0 + arrow.thickness * 2.0).min(len * 0.5);
                    let angle = 30.0_f32.to_radians();
                    let left = arrow.end - rotate_vec(unit, angle) * head_len;
                    let right = arrow.end - rotate_vec(unit, -angle) * head_len;
                    draw_line(&mut img, arrow.end, left, arrow.color, arrow.thickness);
                    draw_line(&mut img, arrow.end, right, arrow.color, arrow.thickness);
                }
            }
            MarkupLayer::Highlight(rect) => {
                draw_rect_fill(&mut img, rect.rect, rect.color);
            }
            MarkupLayer::Marker(marker) => {
                let geometry =
                    marker_geometry(marker.center, marker.number, marker.size, font.as_ref());
                raster::draw_circle(
                    &mut img,
                    RasterPoint::new(marker.center.x, marker.center.y),
                    geometry.radius,
                    RasterColor(marker.color.to_array()),
                );
                if let Some((font, tweak)) = &font {
                    draw_text(
                        &mut img,
                        font,
                        tweak.clone(),
                        geometry.text_origin,
                        &marker.number.to_string(),
                        marker_text_color(marker.color),
                        marker.size,
                    );
                }
            }
            MarkupLayer::Text(text) => {
                if let Some((font, tweak)) = &font {
                    draw_text(
                        &mut img,
                        font,
                        tweak.clone(),
                        text.position,
                        &text.text,
                        text.color,
                        text.size,
                    );
                }
            }
            MarkupLayer::Redaction(operation) => {
                redaction::apply_redaction(&mut img, *operation)?;
            }
        }
    }
    Ok(img)
}

/// Editor window for captured screenshots allowing simple cropping and annotation.
///
/// Cropping is initiated by dragging with the secondary mouse button. Markup
/// tools are selected from the toolbar and applied with the primary mouse
/// button. When saving or copying the screenshot the selected region and
/// markup layers are applied to the output image.
pub struct ScreenshotEditor {
    pub open: bool,
    image: RgbaImage,
    color_image: egui::ColorImage,
    tex: Option<TextureHandle>,
    crop_start: Option<Pos2>,
    crop_rect: Option<Rect>,
    active_start: Option<Pos2>,
    active_end: Option<Pos2>,
    active_stroke: Option<MarkupStroke>,
    active_text: Option<ActiveText>,
    last_marker_click_frame: Option<u64>,
    history: MarkupHistory,
    path: PathBuf,
    _clip: bool,
    auto_save: bool,
    zoom: f32,
    tool: MarkupTool,
    color_index: usize,
    thickness: f32,
    text_size: f32,
    marker_font: Option<(ab_glyph::FontArc, egui::FontTweak)>,
    #[cfg(test)]
    test_content_clip: Option<Rect>,
    #[cfg(test)]
    test_window_width: Option<f32>,
    #[cfg(test)]
    test_slider_response_and_slot: Option<[(Rect, Rect, Rect); 2]>,
    #[cfg(test)]
    test_slider_sizing: Option<[f32; 5]>,
    #[cfg(test)]
    test_tool_row_rect: Option<Rect>,
    #[cfg(test)]
    test_image_rect: Option<Rect>,
    #[cfg(test)]
    test_marker_preview: Vec<MarkerPreviewObservation>,
    #[cfg(test)]
    test_force_marker_discard: bool,
    #[cfg(test)]
    test_marker_ui_passes: Vec<(u64, usize)>,
    #[cfg(feature = "qa-framebuffer-capture")]
    capture_request_id: Option<u64>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct MarkerPreviewObservation {
    number: u64,
    center: Pos2,
    radius: f32,
    ink_bounds: Rect,
}

fn screenshot_editor_window<'a>(test_width: Option<f32>) -> egui::Window<'a> {
    let window = egui::Window::new("Screenshot Editor");
    if let Some(width) = test_width {
        window
            .default_width(width)
            .min_width(width)
            .max_width(width)
    } else {
        window
    }
}

fn add_sized_with_layout_bounds(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    widget: impl egui::Widget,
) -> (egui::Response, Rect, Rect) {
    let layout = egui::Layout::centered_and_justified(ui.layout().main_dir());
    let allocation = ui.allocate_ui_with_layout(size, layout, |ui| {
        let requested_slot = ui.max_rect();
        (ui.add(widget), requested_slot)
    });
    (
        allocation.inner.0,
        allocation.inner.1,
        allocation.response.rect,
    )
}

impl ScreenshotEditor {
    /// Create a new editor from the captured image.
    pub fn new(
        img: RgbaImage,
        path: PathBuf,
        clip: bool,
        auto_save: bool,
        tool: MarkupTool,
    ) -> Self {
        let size = [img.width() as usize, img.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
        Self {
            open: true,
            image: img,
            color_image,
            tex: None,
            crop_start: None,
            crop_rect: None,
            active_start: None,
            active_end: None,
            active_stroke: None,
            active_text: None,
            last_marker_click_frame: None,
            history: MarkupHistory::default(),
            path,
            _clip: clip,
            auto_save,
            zoom: 1.0,
            tool,
            color_index: 0,
            thickness: 4.0,
            text_size: 18.0,
            marker_font: default_font_arc(),
            #[cfg(test)]
            test_content_clip: None,
            #[cfg(test)]
            test_window_width: None,
            #[cfg(test)]
            test_slider_response_and_slot: None,
            #[cfg(test)]
            test_slider_sizing: None,
            #[cfg(test)]
            test_tool_row_rect: None,
            #[cfg(test)]
            test_image_rect: None,
            #[cfg(test)]
            test_marker_preview: Vec::new(),
            #[cfg(test)]
            test_force_marker_discard: false,
            #[cfg(test)]
            test_marker_ui_passes: Vec::new(),
            #[cfg(feature = "qa-framebuffer-capture")]
            capture_request_id: None,
        }
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) fn qa_mark_capture_request(&mut self, request_id: u64) {
        self.capture_request_id = Some(request_id);
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) fn qa_capture_request_id(&self) -> Option<u64> {
        self.capture_request_id
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) fn qa_write_applied_png(&self, path: &Path) -> std::io::Result<(String, u32, u32)> {
        let image = self.apply_edits().map_err(std::io::Error::other)?;
        let dimensions = (image.width(), image.height());
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let result = (|| {
            image::DynamicImage::ImageRgba8(image)
                .write_to(&mut file, image::ImageFormat::Png)
                .map_err(std::io::Error::other)?;
            file.sync_all()?;
            drop(file);
            let mut bytes = Vec::new();
            std::fs::File::open(path)?.read_to_end(&mut bytes)?;
            use sha2::Digest as _;
            let sha256 = format!("{:x}", sha2::Sha256::digest(&bytes));
            Ok((sha256, dimensions.0, dimensions.1))
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(path);
        }
        result
    }

    fn apply_edits(&self) -> Result<RgbaImage, RedactionError> {
        let mut img = render_markup_layers(&self.image, self.history.layers())?;
        if let Some(rect) = self.crop_rect {
            let x1 = rect.min.x.max(0.0) as u32;
            let y1 = rect.min.y.max(0.0) as u32;
            let x2 = rect.max.x.min(img.width() as f32) as u32;
            let y2 = rect.max.y.min(img.height() as f32) as u32;
            let w = (x2 - x1).max(1);
            let h = (y2 - y1).max(1);
            img = image::imageops::crop_imm(&img, x1, y1, w, h).to_image();
        }
        Ok(img)
    }

    fn save_image(&self, path: &PathBuf) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let img = self.apply_edits()?;
        img.save(path)?;
        Ok(())
    }

    fn copy_to_clipboard(&self) -> anyhow::Result<()> {
        use std::borrow::Cow;
        let img = self.apply_edits()?;
        let (w, h) = img.dimensions();
        let mut cb = arboard::Clipboard::new()?;
        cb.set_image(arboard::ImageData {
            width: w as usize,
            height: h as usize,
            bytes: Cow::Owned(img.into_raw()),
        })?;
        Ok(())
    }

    fn palette() -> [Color32; 5] {
        [
            Color32::from_rgb(231, 76, 60),
            Color32::from_rgb(241, 196, 15),
            Color32::from_rgb(46, 204, 113),
            Color32::from_rgb(52, 152, 219),
            Color32::from_rgb(155, 89, 182),
        ]
    }

    fn current_color(&self) -> Color32 {
        let base = Self::palette()[self.color_index];
        if self.tool == MarkupTool::Highlight {
            Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 96)
        } else {
            base
        }
    }

    fn push_layer(&mut self, layer: MarkupLayer) {
        self.history.push(layer);
    }

    fn undo(&mut self) {
        self.history.undo();
    }

    fn redo(&mut self) {
        self.history.redo();
    }

    fn next_marker_number(&self) -> Option<u64> {
        self.history
            .layers()
            .iter()
            .filter_map(|layer| match layer {
                MarkupLayer::Marker(marker) => Some(marker.number),
                _ => None,
            })
            .max()
            .unwrap_or(0)
            .checked_add(1)
    }

    fn place_marker(&mut self, center: Pos2) -> bool {
        let Some(number) = self.next_marker_number() else {
            return false;
        };
        self.push_layer(MarkupLayer::Marker(MarkupMarker {
            center,
            number,
            color: self.current_color(),
            size: self.text_size,
        }));
        true
    }

    fn commit_active_text(&mut self) {
        if let Some(active) = self.active_text.take()
            && !active.text.is_empty()
        {
            self.push_layer(MarkupLayer::Text(MarkupText {
                position: active.position,
                text: active.text,
                color: active.color,
                size: active.size,
            }));
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        #[cfg(test)]
        let test_width = self.test_window_width;
        #[cfg(not(test))]
        let test_width = None;
        screenshot_editor_window(test_width)
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                #[cfg(test)]
                {
                    self.test_content_clip = Some(ui.clip_rect());
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Save").clicked() {
                        let mut saved_to: Option<PathBuf> = None;
                        let res = if self.auto_save {
                            saved_to = Some(self.path.clone());
                            self.save_image(&self.path)
                        } else if let Some(path) = rfd::FileDialog::new()
                            .add_filter("PNG", &["png"])
                            .save_file()
                        {
                            self.path = path.clone();
                            saved_to = Some(path.clone());
                            self.save_image(&path)
                        } else {
                            Ok(())
                        };
                        match res {
                            Ok(()) => {
                                if let Some(path) = saved_to
                                    && app.enable_toasts
                                {
                                    app.add_toast(Toast {
                                        text: format!("Saved screenshot {}", path.display()).into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(app.toast_duration as f64),

                                        style: Default::default(),
                                    });
                                }
                            }
                            Err(e) => {
                                app.report_error(
                                    "ui operation",
                                    format!("Failed to save screenshot: {e}"),
                                );
                            }
                        }
                    }
                    if ui.button("Copy").clicked() {
                        if let Err(e) = self.copy_to_clipboard() {
                            app.report_error(
                                "ui operation",
                                format!("Failed to copy screenshot: {e}"),
                            );
                        } else {
                            if app.enable_toasts {
                                app.add_toast(Toast {
                                    text: "Copied current markup to clipboard".into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(app.toast_duration as f64),

                                    style: Default::default(),
                                });
                            }

                            if app.get_screenshot_save_file() {
                                let _ = if self.auto_save {
                                    self.save_image(&self.path)
                                } else if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("PNG", &["png"])
                                    .save_file()
                                {
                                    self.path = path.clone();
                                    self.save_image(&path)
                                } else {
                                    Ok(())
                                };
                            }
                        }
                    }
                });
                let style = ui.style();
                let label_font = style
                    .override_font_id
                    .clone()
                    .or_else(|| {
                        style
                            .override_text_style
                            .as_ref()
                            .map(|text_style| text_style.resolve(style))
                    })
                    .unwrap_or_else(|| egui::TextStyle::Body.resolve(style));
                let value_display_font = style
                    .override_font_id
                    .clone()
                    .unwrap_or_else(|| style.drag_value_text_style.resolve(style));
                let value_edit_font = style.drag_value_text_style.resolve(style);
                let text_color = ui.visuals().text_color();
                let extra_line_spacing = ui.spacing().extra_text_line_spacing;
                let (label_width, value_text_width, text_height) = ui.fonts_mut(|fonts| {
                    let label_width = ["Zoom", "Text Size"]
                        .into_iter()
                        .map(|text| {
                            fonts
                                .layout_no_wrap(text.to_owned(), label_font.clone(), text_color)
                                .size()
                                .x
                        })
                        .fold(0.0_f32, f32::max);
                    // The slider's DragValue adapts precision to its track size. Six decimal
                    // places covers the precision available at the minimum track width below.
                    // Display text honors override_font_id, while the edit field uses the
                    // configured DragValue style, so reserve the larger of both.
                    let mut value_text_width = 0.0_f32;
                    for font in [&value_display_font, &value_edit_font] {
                        for text in ["4.888888", "48.888888"] {
                            value_text_width = value_text_width.max(
                                fonts
                                    .layout_no_wrap(text.to_owned(), font.clone(), text_color)
                                    .size()
                                    .x,
                            );
                        }
                    }
                    let text_height = fonts
                        .row_height(&label_font)
                        .max(fonts.row_height(&value_display_font))
                        .max(fonts.row_height(&value_edit_font))
                        + extra_line_spacing;
                    (label_width, value_text_width, text_height)
                });
                let slider_row_height = ui.spacing().interact_size.y.max(text_height);
                let spacing = ui.spacing().item_spacing.x;
                let value_width = (value_text_width + ui.spacing().button_padding.x * 2.0)
                    .max(ui.spacing().interact_size.x);
                let fixed_slider_width = label_width + value_width + spacing * 2.0;
                let minimum_track_width = (slider_row_height * 1.25).max(40.0);
                #[cfg(test)]
                let parent_available_width = ui.available_width().max(0.0);
                // `horizontal_wrapped` allocates from the available rect, which is rounded to
                // UI points. Use that same width for the two-up threshold and slot calculation.
                let available_width = ui.available_size_before_wrap().x.max(0.0);
                let minimum_inline_width =
                    (fixed_slider_width + minimum_track_width) * 2.0 + spacing;
                let slider_slot_width = if available_width >= minimum_inline_width {
                    ((available_width - spacing) / 2.0).floor_ui()
                } else {
                    available_width
                };
                let track_width = (slider_slot_width - fixed_slider_width).max(minimum_track_width);
                let previous_slider_width = ui.spacing().slider_width;
                ui.spacing_mut().slider_width = track_width;
                #[cfg(test)]
                let mut actual_slider_row_width = None;
                ui.horizontal_wrapped(|ui| {
                    #[cfg(test)]
                    {
                        actual_slider_row_width = Some(ui.max_rect().width());
                    }
                    let slider_size = egui::vec2(slider_slot_width, slider_row_height);
                    let (_zoom_response, _zoom_requested_slot, _zoom_layout_rect) =
                        add_sized_with_layout_bounds(
                            ui,
                            slider_size,
                            egui::Slider::new(&mut self.zoom, 0.1..=4.0).text("Zoom"),
                        );
                    let (_text_size_response, _text_size_requested_slot, _text_size_layout_rect) =
                        add_sized_with_layout_bounds(
                            ui,
                            slider_size,
                            egui::Slider::new(&mut self.text_size, 6.0..=48.0).text("Text Size"),
                        );
                    #[cfg(test)]
                    {
                        self.test_slider_response_and_slot = Some([
                            (_zoom_response.rect, _zoom_requested_slot, _zoom_layout_rect),
                            (
                                _text_size_response.rect,
                                _text_size_requested_slot,
                                _text_size_layout_rect,
                            ),
                        ]);
                    }
                });
                ui.spacing_mut().slider_width = previous_slider_width;
                #[cfg(test)]
                {
                    self.test_slider_sizing = Some([
                        parent_available_width,
                        available_width,
                        actual_slider_row_width.unwrap_or_default(),
                        minimum_inline_width,
                        slider_slot_width,
                    ]);
                }
                let prev_tool = self.tool;
                let _tool_row = ui.horizontal_wrapped(|ui| {
                    ui.label("Tool");
                    ui.selectable_value(&mut self.tool, MarkupTool::Pen, "Pen");
                    ui.selectable_value(&mut self.tool, MarkupTool::Arrow, "Arrow");
                    ui.selectable_value(&mut self.tool, MarkupTool::Rectangle, "Rect");
                    ui.selectable_value(&mut self.tool, MarkupTool::Highlight, "Highlight");
                    ui.selectable_value(&mut self.tool, MarkupTool::Marker, "Marker");
                    ui.selectable_value(&mut self.tool, MarkupTool::Text, "Text");
                });
                #[cfg(test)]
                {
                    self.test_tool_row_rect = Some(_tool_row.response.rect);
                }
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label("Color");
                    for (idx, color) in Self::palette().iter().enumerate() {
                        let selected = self.color_index == idx;
                        let mut button = egui::Button::new(format!("{}", idx + 1))
                            .fill(*color)
                            .stroke(Stroke::new(1.0_f32, Color32::BLACK));
                        if selected {
                            button = button.stroke(Stroke::new(2.0_f32, Color32::WHITE));
                        }
                        if ui.add(button).clicked() {
                            self.color_index = idx;
                        }
                    }
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Thickness {}", self.thickness as i32));
                    if ui.button("−").clicked() {
                        self.thickness = (self.thickness - 1.0).max(1.0);
                    }
                    if ui.button("+").clicked() {
                        self.thickness = (self.thickness + 1.0).min(20.0);
                    }
                    if ui.button("Undo").clicked() {
                        self.undo();
                    }
                    if ui.button("Redo").clicked() {
                        self.redo();
                    }
                });

                // If we switched away from the Text tool, commit any active text.
                if prev_tool == MarkupTool::Text && self.tool != MarkupTool::Text {
                    self.commit_active_text();
                }

                let pressed_undo = ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl);
                let pressed_redo = ctx.input(|i| {
                    (i.key_pressed(egui::Key::Y) && i.modifiers.ctrl)
                        || (i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && i.modifiers.shift)
                });
                if pressed_undo {
                    self.undo();
                }
                if pressed_redo {
                    self.redo();
                }
                if ctx.input(|i| i.key_pressed(egui::Key::OpenBracket)) {
                    self.thickness = (self.thickness - 1.0).max(1.0);
                }
                if ctx.input(|i| i.key_pressed(egui::Key::CloseBracket)) {
                    self.thickness = (self.thickness + 1.0).min(20.0);
                }
                // Text tool (Paint-like): click to place an insertion point, then type directly onto the canvas.
                // Each text instance captures color/size at creation time.
                if self.tool == MarkupTool::Text {
                    if let Some(active) = &mut self.active_text {
                        // Collect typed characters from this frame.
                        let events = ctx.input(|i| i.events.clone());
                        for ev in events {
                            if let egui::Event::Text(s) = ev {
                                for ch in s.chars() {
                                    // Conservative filter: alphanumeric + whitespace.
                                    if ch.is_alphanumeric() || ch.is_whitespace() {
                                        active.text.push(ch);
                                    }
                                }
                            }
                        }

                        // Basic editing.
                        if ctx.input(|i| i.key_pressed(egui::Key::Backspace)) {
                            active.text.pop();
                        }

                        // Enter commits and returns to "waiting for click".
                        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.commit_active_text();
                        }

                        // Escape cancels the active text instance.
                        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                            self.active_text = None;
                        }
                    }
                } else if self.active_text.is_some() {
                    // Switching away from the Text tool commits the active text (Paint-like behavior).
                    self.commit_active_text();
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Num1)) {
                    self.color_index = 0;
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num2)) {
                    self.color_index = 1;
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num3)) {
                    self.color_index = 2;
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num4)) {
                    self.color_index = 3;
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num5)) {
                    self.color_index = 4;
                }
                let tex = self.tex.get_or_insert_with(|| {
                    ctx.load_texture(
                        "screenshot",
                        self.color_image.clone(),
                        TextureOptions::LINEAR,
                    )
                });
                let img_size = egui::vec2(
                    self.color_image.size[0] as f32,
                    self.color_image.size[1] as f32,
                );
                let display = img_size * self.zoom;
                let canvas_sense = if self.tool == MarkupTool::Marker {
                    Sense::click_and_drag()
                } else {
                    Sense::drag()
                };
                let (response, painter) = ui.allocate_painter(display, canvas_sense);
                #[cfg(test)]
                {
                    self.test_image_rect = Some(response.rect);
                    self.test_marker_preview.clear();
                    if self.tool == MarkupTool::Marker {
                        self.test_marker_ui_passes
                            .push((ctx.cumulative_frame_nr(), ctx.current_pass_index()));
                    }
                }
                let zoom = self.zoom;
                let rect_min = response.rect.min;
                let to_img = |pos: Pos2| ((pos - rect_min) / zoom).to_pos2();
                let to_screen = |p: Pos2| rect_min + (p * zoom).to_vec2();
                painter.image(
                    tex.id(),
                    response.rect,
                    Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
                let marker_secondary_press = self.tool == MarkupTool::Marker
                    && response.hovered()
                    && ctx.input(|input| input.pointer.button_pressed(PointerButton::Secondary));
                let secondary_drag_started = self.tool != MarkupTool::Marker
                    && response.drag_started_by(PointerButton::Secondary);
                if (secondary_drag_started || marker_secondary_press)
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    self.crop_start = Some(to_img(pos));
                    self.crop_rect = None;
                }
                if response.dragged_by(PointerButton::Secondary)
                    && let Some(start) = self.crop_start
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    self.crop_rect = Some(Rect::from_two_pos(start, to_img(pos)));
                }
                if response.drag_stopped_by(PointerButton::Secondary)
                    || (self.tool == MarkupTool::Marker
                        && response.clicked_by(PointerButton::Secondary))
                {
                    self.crop_start = None;
                }

                // A marker is a click tool, not a drag tool. Keep the action on the release
                // edge and remember the egui frame so a retained/replayed UI pass cannot add it
                // a second time.
                let marker_click_frame = ctx.cumulative_frame_nr();
                if self.tool == MarkupTool::Marker
                    && response.clicked_by(PointerButton::Primary)
                    && self.last_marker_click_frame != Some(marker_click_frame)
                    && let Some(pos) = response.interact_pointer_pos()
                    && self.place_marker(to_img(pos))
                {
                    self.last_marker_click_frame = Some(marker_click_frame);
                }
                #[cfg(test)]
                if self.test_force_marker_discard {
                    self.test_force_marker_discard = false;
                    ctx.request_discard("screenshot marker single-action retained-pass test");
                }

                if response.drag_started_by(PointerButton::Primary)
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    let start = to_img(pos);
                    match self.tool {
                        MarkupTool::Pen => {
                            self.active_stroke = Some(MarkupStroke {
                                points: vec![start],
                                color: self.current_color(),
                                thickness: self.thickness,
                            });
                        }
                        MarkupTool::Arrow | MarkupTool::Rectangle | MarkupTool::Highlight => {
                            self.active_start = Some(start);
                            self.active_end = Some(start);
                        }
                        MarkupTool::Text => {
                            // If a text instance is currently active, commit it and start a new one.
                            if self.active_text.is_some() {
                                self.commit_active_text();
                            }
                            self.active_text = Some(ActiveText {
                                position: start,
                                text: String::new(),
                                color: self.current_color(),
                                size: self.text_size,
                            });
                        }
                        MarkupTool::Marker => {}
                    }
                }
                if response.dragged_by(PointerButton::Primary)
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    let current = to_img(pos);
                    match self.tool {
                        MarkupTool::Pen => {
                            if let Some(stroke) = &mut self.active_stroke {
                                stroke.points.push(current);
                            }
                        }
                        MarkupTool::Arrow | MarkupTool::Rectangle | MarkupTool::Highlight => {
                            self.active_end = Some(current);
                        }
                        MarkupTool::Marker | MarkupTool::Text => {}
                    }
                }
                if response.drag_stopped_by(PointerButton::Primary) {
                    match self.tool {
                        MarkupTool::Pen => {
                            if let Some(stroke) = self.active_stroke.take()
                                && stroke.points.len() > 1
                            {
                                self.push_layer(MarkupLayer::Stroke(stroke));
                            }
                        }
                        MarkupTool::Arrow => {
                            if let (Some(start), Some(end)) =
                                (self.active_start.take(), self.active_end.take())
                            {
                                self.push_layer(MarkupLayer::Arrow(MarkupArrow {
                                    start,
                                    end,
                                    color: self.current_color(),
                                    thickness: self.thickness,
                                }));
                            }
                        }
                        MarkupTool::Rectangle => {
                            if let (Some(start), Some(end)) =
                                (self.active_start.take(), self.active_end.take())
                            {
                                self.push_layer(MarkupLayer::Rectangle(MarkupRect {
                                    rect: Rect::from_two_pos(start, end),
                                    color: self.current_color(),
                                    thickness: self.thickness,
                                }));
                            }
                        }
                        MarkupTool::Highlight => {
                            if let (Some(start), Some(end)) =
                                (self.active_start.take(), self.active_end.take())
                            {
                                self.push_layer(MarkupLayer::Highlight(MarkupRect {
                                    rect: Rect::from_two_pos(start, end),
                                    color: self.current_color(),
                                    thickness: self.thickness,
                                }));
                            }
                        }
                        MarkupTool::Marker | MarkupTool::Text => {}
                    }
                    self.active_start = None;
                    self.active_end = None;
                    self.active_stroke = None;
                }
                if let Some(rect) = self.crop_rect {
                    let draw = Rect::from_min_max(to_screen(rect.min), to_screen(rect.max));
                    painter.rect_stroke(
                        draw,
                        0.0,
                        Stroke::new(1.0_f32, Color32::GREEN),
                        egui::StrokeKind::Middle,
                    );
                }
                for layer in self.history.layers() {
                    match layer {
                        MarkupLayer::Stroke(stroke) => {
                            for points in stroke.points.windows(2) {
                                painter.line_segment(
                                    [to_screen(points[0]), to_screen(points[1])],
                                    Stroke::new(stroke.thickness, stroke.color),
                                );
                            }
                        }
                        MarkupLayer::Rectangle(rect) => {
                            let draw = Rect::from_min_max(
                                to_screen(rect.rect.min),
                                to_screen(rect.rect.max),
                            );
                            painter.rect_stroke(
                                draw,
                                0.0,
                                Stroke::new(rect.thickness, rect.color),
                                egui::StrokeKind::Middle,
                            );
                        }
                        MarkupLayer::Arrow(arrow) => {
                            painter.line_segment(
                                [to_screen(arrow.start), to_screen(arrow.end)],
                                Stroke::new(arrow.thickness, arrow.color),
                            );
                            let dir = arrow.end - arrow.start;
                            let len = dir.length();
                            if len > 0.5 {
                                let unit = dir / len;
                                let head_len = (10.0 + arrow.thickness * 2.0).min(len * 0.5);
                                let angle = 30.0_f32.to_radians();
                                let left = arrow.end - rotate_vec(unit, angle) * head_len;
                                let right = arrow.end - rotate_vec(unit, -angle) * head_len;
                                painter.line_segment(
                                    [to_screen(arrow.end), to_screen(left)],
                                    Stroke::new(arrow.thickness, arrow.color),
                                );
                                painter.line_segment(
                                    [to_screen(arrow.end), to_screen(right)],
                                    Stroke::new(arrow.thickness, arrow.color),
                                );
                            }
                        }
                        MarkupLayer::Highlight(rect) => {
                            let draw = Rect::from_min_max(
                                to_screen(rect.rect.min),
                                to_screen(rect.rect.max),
                            );
                            painter.rect_filled(draw, 0.0, rect.color);
                        }
                        MarkupLayer::Marker(marker) => {
                            let geometry = marker_geometry(
                                marker.center,
                                marker.number,
                                marker.size,
                                self.marker_font.as_ref(),
                            );
                            let center = to_screen(geometry.text_bounds.center());
                            painter.circle_filled(center, geometry.radius * zoom, marker.color);
                            let text_color = marker_text_color(marker.color);
                            let galley = painter.layout_no_wrap(
                                marker.number.to_string(),
                                egui::FontId::proportional(marker.size * zoom),
                                text_color,
                            );
                            let text_position = center - galley.mesh_bounds.center().to_vec2();
                            let ink_bounds = galley.mesh_bounds.translate(text_position.to_vec2());
                            painter.galley(text_position, galley, text_color);
                            #[cfg(test)]
                            self.test_marker_preview.push(MarkerPreviewObservation {
                                number: marker.number,
                                center,
                                radius: geometry.radius * zoom,
                                ink_bounds,
                            });
                            #[cfg(not(test))]
                            let _ = ink_bounds;
                        }
                        MarkupLayer::Text(text) => {
                            painter.text(
                                to_screen(text.position),
                                egui::Align2::LEFT_TOP,
                                &text.text,
                                egui::FontId::proportional(text.size),
                                text.color,
                            );
                        }
                        // M1-A provides the authoritative raster/export operation. The canvas
                        // preview and redaction controls are integrated in M1-B; do not present
                        // a rectangle overlay as if it were the applied effect.
                        MarkupLayer::Redaction(_) => {}
                    }
                }
                if let (Some(start), Some(end)) = (self.active_start, self.active_end) {
                    let rect = Rect::from_two_pos(start, end);
                    match self.tool {
                        MarkupTool::Arrow => {
                            painter.line_segment(
                                [to_screen(start), to_screen(end)],
                                Stroke::new(self.thickness, self.current_color()),
                            );
                        }
                        MarkupTool::Rectangle => {
                            let draw = Rect::from_min_max(to_screen(rect.min), to_screen(rect.max));
                            painter.rect_stroke(
                                draw,
                                0.0,
                                Stroke::new(self.thickness, self.current_color()),
                                egui::StrokeKind::Middle,
                            );
                        }
                        MarkupTool::Highlight => {
                            let draw = Rect::from_min_max(to_screen(rect.min), to_screen(rect.max));
                            painter.rect_filled(draw, 0.0, self.current_color());
                        }
                        MarkupTool::Pen | MarkupTool::Marker | MarkupTool::Text => {}
                    }
                }
                if let Some(stroke) = &self.active_stroke {
                    for points in stroke.points.windows(2) {
                        painter.line_segment(
                            [to_screen(points[0]), to_screen(points[1])],
                            Stroke::new(stroke.thickness, stroke.color),
                        );
                    }
                }
                if let Some(active) = &self.active_text
                    && !active.text.is_empty()
                {
                    painter.text(
                        to_screen(active.position),
                        egui::Align2::LEFT_TOP,
                        &active.text,
                        egui::FontId::proportional(active.size),
                        active.color,
                    );
                }
            });
        self.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::AtomicBool};

    const CONTROLS: [&str; 19] = [
        "Save",
        "Copy",
        "Pen",
        "Arrow",
        "Rect",
        "Highlight",
        "Marker",
        "Text",
        "1",
        "2",
        "3",
        "4",
        "5",
        "−",
        "+",
        "Undo",
        "Redo",
        "Zoom",
        "Text Size",
    ];

    #[derive(Clone, Copy, Debug)]
    struct SliderLayoutBounds {
        response: Rect,
        requested_slot: Rect,
        allocated: Rect,
        track: Rect,
        label: Rect,
        value: Rect,
    }

    fn screenshot_editor_test_app(context: &egui::Context, root: &std::path::Path) -> LauncherApp {
        let mut settings = crate::settings::Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.enable_toasts = false;
        settings.show_inline_errors = false;
        settings.show_error_toasts = false;
        settings.dashboard.enabled = false;
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        LauncherApp::new(
            context,
            Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            settings,
            None,
            None,
            Some(std::collections::HashSet::new()),
            Some(std::collections::HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn control_bounds(
        output: &egui::FullOutput,
        label: &str,
    ) -> Option<(egui::accesskit::Role, Rect)> {
        output
            .platform_output
            .accesskit_update
            .as_ref()?
            .nodes
            .iter()
            .find_map(|(_, node)| {
                if node.label() != Some(label) {
                    return None;
                }
                let bounds = node.bounds()?;
                Some((
                    node.role(),
                    Rect::from_min_max(
                        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
                    ),
                ))
            })
    }

    fn slider_accessibility_bounds(
        output: &egui::FullOutput,
        expected_label: &str,
    ) -> Option<(Rect, Rect, Rect)> {
        let nodes = &output.platform_output.accesskit_update.as_ref()?.nodes;
        let (_, slider) = nodes.iter().find(|(_, node)| {
            node.role() == egui::accesskit::Role::Slider && node.label() == Some(expected_label)
        })?;
        let label_id = slider.labelled_by().iter().find(|label_id| {
            nodes.iter().any(|(id, node)| {
                id == *label_id
                    && node.role() == egui::accesskit::Role::Label
                    && node.value() == Some(expected_label)
            })
        })?;
        let track = slider.bounds()?;
        let label = nodes.iter().find(|(id, _)| *id == *label_id)?.1.bounds()?;
        let value = nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::SpinButton
                    && node.labelled_by().contains(label_id)
            })?
            .1
            .bounds()?;
        let to_rect = |bounds: egui::accesskit::Rect| {
            Rect::from_min_max(
                Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
            )
        };
        Some((to_rect(track), to_rect(label), to_rect(value)))
    }

    fn settled_toolbar_controls(
        context: &egui::Context,
        editor: &mut ScreenshotEditor,
        app: &mut LauncherApp,
        screen: Rect,
    ) -> (
        [Option<(egui::accesskit::Role, Rect)>; CONTROLS.len()],
        Rect,
        [Option<SliderLayoutBounds>; 2],
        Rect,
    ) {
        let mut settled_controls = [None; CONTROLS.len()];
        let mut settled_content_clip = None;
        let mut settled_slider_layout = [None; 2];
        let mut settled_tool_row = None;
        for _ in 0..3 {
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    focused: true,
                    ..Default::default()
                },
                |ctx| editor.ui(ctx, app),
            );
            let controls = CONTROLS.map(|label| control_bounds(&output, label));
            let slider_accessibility = [
                slider_accessibility_bounds(&output, "Zoom"),
                slider_accessibility_bounds(&output, "Text Size"),
            ];
            let slider_layout = editor.test_slider_response_and_slot.map(|layout| {
                [
                    slider_accessibility[0].map(|(track, label, value)| SliderLayoutBounds {
                        response: layout[0].0,
                        requested_slot: layout[0].1,
                        allocated: layout[0].2,
                        track,
                        label,
                        value,
                    }),
                    slider_accessibility[1].map(|(track, label, value)| SliderLayoutBounds {
                        response: layout[1].0,
                        requested_slot: layout[1].1,
                        allocated: layout[1].2,
                        track,
                        label,
                        value,
                    }),
                ]
            });
            let content_clip = editor.test_content_clip;
            let tool_row = editor.test_tool_row_rect;
            output.drop_without_applying_deltas();
            settled_content_clip = content_clip.or(settled_content_clip);
            settled_tool_row = tool_row.or(settled_tool_row);
            for (settled, observed) in settled_controls.iter_mut().zip(controls) {
                *settled = observed.or(*settled);
            }
            if let Some(slider_layout) = slider_layout {
                for (settled, observed) in settled_slider_layout.iter_mut().zip(slider_layout) {
                    *settled = observed.or(*settled);
                }
            }
        }
        (
            settled_controls,
            settled_content_clip.expect("the Screenshot Editor exposes its clip"),
            settled_slider_layout,
            settled_tool_row.expect("the Tool row exposes its Response rectangle"),
        )
    }

    fn assert_toolbar_controls_reachable(
        controls: &[Option<(egui::accesskit::Role, Rect)>; CONTROLS.len()],
        screen: Rect,
        content_clip: Rect,
    ) {
        for (label, observation) in CONTROLS.into_iter().zip(controls.iter().copied()) {
            let (role, bounds) = observation.unwrap_or_else(|| {
                panic!(
                    "{label} has no native bound; input screen {screen:?}; window content clip {content_clip:?}"
                )
            });
            let diagnostic = format!(
                "{label} ({role:?}) bounds {bounds:?}; input screen {screen:?}; window content clip {content_clip:?}"
            );
            assert!(bounds.is_positive(), "{diagnostic}");
            assert!(screen.contains_rect(bounds), "{diagnostic}");
            assert!(content_clip.contains_rect(bounds), "{diagnostic}");
        }
    }

    fn slider_full_bounds(slider: SliderLayoutBounds) -> Rect {
        slider
            .response
            .union(slider.track)
            .union(slider.label)
            .union(slider.value)
    }

    fn assert_slider_layout_reachable(
        sliders: [Option<SliderLayoutBounds>; 2],
        content_clip: Rect,
    ) -> [SliderLayoutBounds; 2] {
        let [zoom, text_size] = sliders
            .map(|bounds| bounds.expect("slider response, label, and value bounds are available"));
        for (name, slider) in [("Zoom", zoom), ("Text Size", text_size)] {
            let full_bounds = slider_full_bounds(slider);
            for (part, bounds) in [
                ("full widget", full_bounds),
                ("Response.rect", slider.response),
                ("track", slider.track),
                ("label", slider.label),
                ("value", slider.value),
                ("post-layout allocation", slider.allocated),
            ] {
                let diagnostic = format!(
                    "{name} {part} {bounds:?}; requested slot {:?}; post-layout allocation {:?}; window content clip {content_clip:?}",
                    slider.requested_slot, slider.allocated
                );
                assert!(bounds.is_positive(), "{diagnostic}");
                assert!(content_clip.contains_rect(bounds), "{diagnostic}");
            }
            assert!(slider.allocated.contains_rect(full_bounds));
            assert!(slider.requested_slot.left() <= full_bounds.left());
            assert!(full_bounds.right() <= slider.requested_slot.right());
            assert!(slider.response.contains_rect(slider.track));
        }
        [zoom, text_size]
    }

    fn screenshot_editor_frame(
        context: &egui::Context,
        editor: &mut ScreenshotEditor,
        app: &mut LauncherApp,
        screen: Rect,
        events: Vec<egui::Event>,
    ) {
        screenshot_editor_frame_output(context, editor, app, screen, events)
            .drop_without_applying_deltas();
    }

    fn screenshot_editor_frame_output(
        context: &egui::Context,
        editor: &mut ScreenshotEditor,
        app: &mut LauncherApp,
        screen: Rect,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        context.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| {
                editor.ui(ctx, app);
            },
        )
    }

    fn marker_layers(history: &MarkupHistory) -> Vec<MarkupMarker> {
        history
            .layers()
            .iter()
            .filter_map(|layer| match layer {
                MarkupLayer::Marker(marker) => Some(marker.clone()),
                _ => None,
            })
            .collect()
    }

    fn redaction_bounds(
        first: Pos2,
        second: Pos2,
    ) -> crate::annotation::redaction::RedactionBounds {
        crate::annotation::redaction::RedactionBounds::from_corners(
            first.x, first.y, second.x, second.y,
        )
        .unwrap()
    }

    fn generated_grid_image(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            if (x / 8 + y / 8) % 2 == 0 {
                image::Rgba([34, 54, 74, 255])
            } else {
                image::Rgba([56, 76, 96, 255])
            }
        })
    }

    #[test]
    fn screenshot_marker_history_numbers_follow_retained_layers_across_undo_redo_and_tool_switches()
    {
        let image = RgbaImage::from_pixel(40, 30, image::Rgba([20, 30, 40, 255]));
        let mut editor = ScreenshotEditor::new(
            image.clone(),
            PathBuf::from("unused.png"),
            false,
            false,
            MarkupTool::Marker,
        );
        editor.color_index = 1;
        editor.text_size = 24.0;
        assert_eq!(editor.next_marker_number(), Some(1));
        assert!(editor.place_marker(Pos2::new(5.0, 5.0)));
        let first = marker_layers(&editor.history)[0].clone();
        assert_eq!(first.number, 1);
        assert_eq!(first.color, ScreenshotEditor::palette()[1]);
        assert_eq!(first.size, 24.0);

        editor.tool = MarkupTool::Pen;
        editor.text_size = 32.0;
        editor.color_index = 4;
        editor.tool = MarkupTool::Marker;
        assert!(editor.place_marker(Pos2::new(10.0, 5.0)));
        assert_eq!(marker_layers(&editor.history)[1].number, 2);
        editor.undo();
        assert_eq!(editor.next_marker_number(), Some(2));
        editor.redo();
        assert_eq!(marker_layers(&editor.history)[1].number, 2);

        for x in [15.0, 20.0, 25.0] {
            assert!(editor.place_marker(Pos2::new(x, 10.0)));
        }
        assert_eq!(
            marker_layers(&editor.history)
                .iter()
                .map(|marker| marker.number)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
        editor.undo();
        editor.undo();
        editor.undo();
        assert_eq!(editor.next_marker_number(), Some(3));
        editor.color_index = 2;
        editor.text_size = 18.0;
        assert!(editor.place_marker(Pos2::new(30.0, 10.0)));
        assert_eq!(
            marker_layers(&editor.history)
                .iter()
                .map(|marker| marker.number)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(editor.history.redo_stack.is_empty());
        let replacement = marker_layers(&editor.history)[2].clone();
        assert_eq!(replacement.color, ScreenshotEditor::palette()[2]);
        assert_eq!(replacement.size, 18.0);

        let fresh = ScreenshotEditor::new(
            image,
            PathBuf::from("another-unused.png"),
            false,
            false,
            MarkupTool::Marker,
        );
        assert_eq!(fresh.next_marker_number(), Some(1));
    }

    #[test]
    fn screenshot_redaction_layers_compose_current_pixels_and_follow_history() {
        use crate::annotation::redaction::{RedactionEffect, RedactionOperation};

        let first_base = RgbaImage::from_fn(4, 1, |x, _| {
            let value = [20, 40, 60, 80][x as usize];
            image::Rgba([value, value, value, 255])
        });
        let second_base = RgbaImage::from_fn(4, 1, |x, _| {
            let value = [200, 220, 60, 80][x as usize];
            image::Rgba([value, value, value, 255])
        });
        let solid = RedactionOperation::new(
            redaction_bounds(Pos2::new(0.0, 0.0), Pos2::new(2.0, 1.0)),
            RedactionEffect::solid_black(),
        );
        let blur = RedactionOperation::new(
            redaction_bounds(Pos2::new(0.0, 0.0), Pos2::new(4.0, 1.0)),
            RedactionEffect::blur(1).unwrap(),
        );
        let mut history = MarkupHistory::default();
        history.push(MarkupLayer::Redaction(solid));
        let solid_only = render_markup_layers(&first_base, history.layers()).unwrap();
        assert_eq!(solid_only.get_pixel(0, 0).0, [0, 0, 0, 255]);
        assert_eq!(solid_only.get_pixel(1, 0).0, [0, 0, 0, 255]);

        history.push(MarkupLayer::Redaction(blur));
        let composed = render_markup_layers(&first_base, history.layers()).unwrap();
        let alternate_composed = render_markup_layers(&second_base, history.layers()).unwrap();
        assert_eq!(
            composed, alternate_composed,
            "a later effect must sample the already-redacted working pixels"
        );
        assert_eq!(
            (0..4)
                .map(|x| composed.get_pixel(x, 0)[0])
                .collect::<Vec<_>>(),
            [0, 20, 47, 73]
        );
        assert_eq!(
            match &history.layers()[1] {
                MarkupLayer::Redaction(operation) => operation.effect(),
                other => panic!("expected redaction layer, got {other:?}"),
            },
            RedactionEffect::blur(1).unwrap(),
            "captured history parameters do not track subsequent tool settings"
        );
        let later_tool_setting = RedactionEffect::pixelate(4).unwrap();
        assert_ne!(later_tool_setting, blur.effect());

        assert!(history.undo());
        assert_eq!(
            render_markup_layers(&first_base, history.layers()).unwrap(),
            solid_only
        );
        assert!(history.redo());
        assert_eq!(
            render_markup_layers(&first_base, history.layers()).unwrap(),
            composed
        );
        assert!(history.undo());
        history.push(MarkupLayer::Highlight(MarkupRect {
            rect: Rect::from_min_max(Pos2::new(3.0, 0.0), Pos2::new(4.0, 1.0)),
            color: Color32::RED,
            thickness: 1.0,
        }));
        let annotated = render_markup_layers(&first_base, history.layers()).unwrap();
        assert_eq!(annotated.get_pixel(3, 0).0, [255, 0, 0, 255]);
        assert!(history.redo_stack.is_empty());
        assert!(history.undo());
        assert!(history.redo());
        assert_eq!(
            render_markup_layers(&first_base, history.layers()).unwrap(),
            annotated
        );
        assert!(history.undo());
        history.push(MarkupLayer::Text(MarkupText {
            position: Pos2::new(0.0, 0.0),
            text: "later ordinary layer".to_owned(),
            color: Color32::WHITE,
            size: 8.0,
        }));
        assert!(history.redo_stack.is_empty());
    }

    #[test]
    fn screenshot_redaction_apply_edits_crops_after_effect_and_saved_png_matches() {
        use crate::annotation::redaction::{RedactionEffect, RedactionOperation};

        let base = generated_grid_image(8, 6);
        let mut editor = ScreenshotEditor::new(
            base.clone(),
            PathBuf::from("unused-redaction.png"),
            false,
            false,
            MarkupTool::Pen,
        );
        editor.push_layer(MarkupLayer::Redaction(RedactionOperation::new(
            redaction_bounds(Pos2::new(2.2, 1.1), Pos2::new(6.0, 5.0)),
            RedactionEffect::solid([210, 5, 9]),
        )));
        editor.crop_rect = Some(Rect::from_min_max(Pos2::new(1.0, 1.0), Pos2::new(7.0, 5.0)));

        let full = render_markup_layers(&base, editor.history.layers()).unwrap();
        assert_eq!(full.get_pixel(2, 2).0, [210, 5, 9, 255]);
        let expected = image::imageops::crop_imm(&full, 1, 1, 6, 4).to_image();
        let applied = editor.apply_edits().unwrap();
        assert_eq!(applied, expected);
        assert_eq!(applied.get_pixel(1, 1).0, [210, 5, 9, 255]);

        let root = tempfile::tempdir().unwrap();
        let output_path = root.path().join("redacted.png");
        editor.save_image(&output_path).unwrap();
        let saved = image::io::Reader::open(output_path)
            .unwrap()
            .decode()
            .unwrap()
            .to_rgba8();
        assert_eq!(saved, applied);
    }

    #[test]
    fn screenshot_marker_raster_bounds_center_digits_and_crop_before_saved_export() {
        let base = generated_grid_image(132, 96);
        let font = default_font_arc().expect("the bundled proportional font is available");
        let bounds_9 = raster::measure_text_ink_bounds(&font.0, &font.1, "9", 24.0).unwrap();
        let bounds_10 = raster::measure_text_ink_bounds(&font.0, &font.1, "10", 24.0).unwrap();
        assert!(bounds_9.width() > 0.0 && bounds_9.height() > 0.0);
        assert!(bounds_10.width() > bounds_9.width());

        let color = ScreenshotEditor::palette()[0];
        let dark_badge_color = Color32::from_rgb(35, 35, 35);
        let markers = [
            MarkupMarker {
                center: Pos2::new(1.0, 1.0),
                number: 1,
                color,
                size: 24.0,
            },
            MarkupMarker {
                center: Pos2::new(40.0, 30.0),
                number: 9,
                color,
                size: 24.0,
            },
            MarkupMarker {
                center: Pos2::new(82.0, 30.0),
                number: 10,
                color: dark_badge_color,
                size: 24.0,
            },
            MarkupMarker {
                center: Pos2::new(128.0, 92.0),
                number: 11,
                color,
                size: 24.0,
            },
        ];
        let layers = markers
            .iter()
            .cloned()
            .map(MarkupLayer::Marker)
            .collect::<Vec<_>>();

        for marker in &markers[1..3] {
            let geometry = marker_geometry(marker.center, marker.number, marker.size, Some(&font));
            assert!((geometry.text_bounds.center() - marker.center).length() < 0.001);
            assert!(geometry.radius > geometry.text_bounds.width() * 0.5);
            assert!(geometry.radius > geometry.text_bounds.height() * 0.5);
        }
        let geometry_9 = marker_geometry(Pos2::ZERO, 9, 24.0, Some(&font));
        let geometry_10 = marker_geometry(Pos2::ZERO, 10, 24.0, Some(&font));
        assert!(geometry_10.radius > geometry_9.radius);

        let rendered = render_markup_layers(&base, &layers).unwrap();
        assert_ne!(rendered.get_pixel(0, 0), base.get_pixel(0, 0));
        let marker_10 = &markers[2];
        assert_eq!(marker_text_color(marker_10.color), Color32::WHITE);
        let white_pixels = rendered
            .enumerate_pixels()
            .filter(|(x, y, pixel)| {
                (pixel[0] > 220 && pixel[1] > 220 && pixel[2] > 220)
                    && (*x as f32 - marker_10.center.x).abs() < 16.0
                    && (*y as f32 - marker_10.center.y).abs() < 16.0
            })
            .map(|(x, y, _)| (x as f32, y as f32))
            .collect::<Vec<_>>();
        assert!(
            !white_pixels.is_empty(),
            "marker digits should use contrast text"
        );
        let ink_center = Pos2::new(
            (white_pixels
                .iter()
                .map(|point| point.0)
                .fold(f32::INFINITY, f32::min)
                + white_pixels
                    .iter()
                    .map(|point| point.0)
                    .fold(f32::NEG_INFINITY, f32::max))
                * 0.5,
            (white_pixels
                .iter()
                .map(|point| point.1)
                .fold(f32::INFINITY, f32::min)
                + white_pixels
                    .iter()
                    .map(|point| point.1)
                    .fold(f32::NEG_INFINITY, f32::max))
                * 0.5,
        );
        assert!((ink_center - marker_10.center).length() <= 1.5);

        let mut editor = ScreenshotEditor::new(
            base.clone(),
            PathBuf::from("unused.png"),
            false,
            false,
            MarkupTool::Marker,
        );
        editor.history = MarkupHistory::default();
        for marker in markers {
            editor.push_layer(MarkupLayer::Marker(marker));
        }
        editor.crop_rect = Some(Rect::from_min_max(
            Pos2::new(0.0, 0.0),
            Pos2::new(100.0, 72.0),
        ));
        let full = render_markup_layers(&base, editor.history.layers()).unwrap();
        let expected = image::imageops::crop_imm(&full, 0, 0, 100, 72).to_image();
        let applied = editor.apply_edits().unwrap();
        assert_eq!(applied, expected);

        let evidence_dir = std::env::var_os("ML_SCREENSHOT_MARKER_EVIDENCE_DIR");
        let tempdir = tempfile::tempdir().unwrap();
        let output_dir = evidence_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| tempdir.path().to_path_buf());
        std::fs::create_dir_all(&output_dir).unwrap();
        let output_path = output_dir.join("screenshot-marker-cropped.png");
        editor.save_image(&output_path).unwrap();
        let saved = image::io::Reader::open(&output_path)
            .unwrap()
            .decode()
            .unwrap()
            .to_rgba8();
        assert_eq!(saved, applied);

        if std::env::var_os("ML_SCREENSHOT_MARKER_EVIDENCE_DIR").is_some() {
            let observation = format!(
                "fixture=generated-grid\nmarkers=1,9,10,11\nfont_size=24\n9_ink={:?}\n10_ink={:?}\n9_radius={}\n10_radius={}\ncrop=100x72\n",
                geometry_9.text_bounds,
                geometry_10.text_bounds,
                geometry_9.radius,
                geometry_10.radius
            );
            std::fs::write(output_dir.join("screenshot-marker-raster.txt"), observation).unwrap();
        }
    }

    #[test]
    fn screenshot_marker_canvas_click_is_single_shot_zoomed_and_matches_preview_export() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let root = tempfile::tempdir().unwrap();
        let mut app = screenshot_editor_test_app(&context, root.path());
        context.style_mut_of(context.theme(), |style| style.animation_time = 0.0);
        let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 760.0));
        let image = generated_grid_image(200, 130);
        let mut editor = ScreenshotEditor::new(
            image,
            root.path().join("unused.png"),
            false,
            false,
            MarkupTool::Marker,
        );
        editor.test_window_width = Some(720.0);
        editor.zoom = 1.5;
        editor.text_size = 24.0;
        editor.color_index = 1;
        for marker in [
            MarkupMarker {
                center: Pos2::new(5.0, 5.0),
                number: 1,
                color: ScreenshotEditor::palette()[0],
                size: 24.0,
            },
            MarkupMarker {
                center: Pos2::new(55.0, 25.0),
                number: 9,
                color: ScreenshotEditor::palette()[2],
                size: 28.0,
            },
            MarkupMarker {
                center: Pos2::new(105.0, 25.0),
                number: 10,
                color: ScreenshotEditor::palette()[3],
                size: 32.0,
            },
        ] {
            editor.push_layer(MarkupLayer::Marker(marker));
        }

        for _ in 0..3 {
            screenshot_editor_frame(&context, &mut editor, &mut app, screen, Vec::new());
        }
        editor.test_marker_ui_passes.clear();
        let image_rect = editor.test_image_rect.expect("canvas response is retained");
        assert_eq!(editor.test_marker_preview.len(), 3);
        for observation in &editor.test_marker_preview {
            let marker = marker_layers(&editor.history)
                .into_iter()
                .find(|marker| marker.number == observation.number)
                .unwrap();
            let expected_center = image_rect.min + (marker.center * editor.zoom).to_vec2();
            assert!((observation.center - expected_center).length() < 0.01);
            assert!(observation.radius > 0.0);
            assert!(observation.ink_bounds.is_positive());
            assert!((observation.ink_bounds.center() - observation.center).length() < 0.1);
            for corner in [
                observation.ink_bounds.left_top(),
                observation.ink_bounds.right_top(),
                observation.ink_bounds.left_bottom(),
                observation.ink_bounds.right_bottom(),
            ] {
                assert!(
                    (corner - observation.center).length() <= observation.radius + 0.5,
                    "digit ink escaped its measured badge: {observation:?}"
                );
            }
        }

        let placed_at = Pos2::new(150.0, 78.0);
        let screen_pos = image_rect.min + (placed_at * editor.zoom).to_vec2();
        screenshot_editor_frame(
            &context,
            &mut editor,
            &mut app,
            screen,
            vec![
                egui::Event::PointerMoved(screen_pos),
                egui::Event::PointerButton {
                    pos: screen_pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        assert_eq!(marker_layers(&editor.history).len(), 3);
        editor.test_force_marker_discard = true;
        screenshot_editor_frame(
            &context,
            &mut editor,
            &mut app,
            screen,
            vec![egui::Event::PointerButton {
                pos: screen_pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        let retained_passes = &editor.test_marker_ui_passes;
        assert!(
            retained_passes.len() >= 2
                && retained_passes[retained_passes.len() - 2].0
                    == retained_passes[retained_passes.len() - 1].0
                && retained_passes[retained_passes.len() - 2].1
                    < retained_passes[retained_passes.len() - 1].1,
            "the release input must be replayed in a later egui pass: {retained_passes:?}"
        );
        let markers = marker_layers(&editor.history);
        assert_eq!(
            markers.len(),
            4,
            "a repeated UI pass must not duplicate a click"
        );
        assert_eq!(markers[3].number, 11);
        assert!((markers[3].center - placed_at).length() < 0.01);
        assert_eq!(markers[3].color, ScreenshotEditor::palette()[1]);
        assert_eq!(markers[3].size, 24.0);
        editor.color_index = 4;
        editor.text_size = 42.0;

        // A primary drag does not add a click marker.
        let drag_start = image_rect.min + (Pos2::new(120.0, 85.0) * editor.zoom).to_vec2();
        let drag_end = drag_start + egui::vec2(36.0, 24.0);
        screenshot_editor_frame(
            &context,
            &mut editor,
            &mut app,
            screen,
            vec![
                egui::Event::PointerMoved(drag_start),
                egui::Event::PointerButton {
                    pos: drag_start,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        screenshot_editor_frame(
            &context,
            &mut editor,
            &mut app,
            screen,
            vec![egui::Event::PointerMoved(drag_end)],
        );
        screenshot_editor_frame(
            &context,
            &mut editor,
            &mut app,
            screen,
            vec![egui::Event::PointerButton {
                pos: drag_end,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        assert_eq!(marker_layers(&editor.history).len(), 4);

        // The secondary-button crop path remains independent of marker placement.
        let crop_start = image_rect.min + (Pos2::new(10.0, 10.0) * editor.zoom).to_vec2();
        let crop_end = image_rect.min + (Pos2::new(178.0, 112.0) * editor.zoom).to_vec2();
        screenshot_editor_frame(
            &context,
            &mut editor,
            &mut app,
            screen,
            vec![
                egui::Event::PointerMoved(crop_start),
                egui::Event::PointerButton {
                    pos: crop_start,
                    button: PointerButton::Secondary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        assert!(
            (editor
                .crop_start
                .expect("an owned secondary press immediately starts the crop")
                - Pos2::new(10.0, 10.0))
            .length()
                < 0.01
        );
        screenshot_editor_frame(
            &context,
            &mut editor,
            &mut app,
            screen,
            vec![egui::Event::PointerMoved(crop_end)],
        );
        screenshot_editor_frame(
            &context,
            &mut editor,
            &mut app,
            screen,
            vec![egui::Event::PointerButton {
                pos: crop_end,
                button: PointerButton::Secondary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        assert_eq!(marker_layers(&editor.history).len(), 4);
        let crop = editor
            .crop_rect
            .expect("secondary drag still defines a crop");
        assert!(crop.is_positive());
        assert!((crop.min - Pos2::new(10.0, 10.0)).length() < 0.01);
        assert!((crop.max - Pos2::new(178.0, 112.0)).length() < 0.01);

        for _ in 0..2 {
            screenshot_editor_frame(&context, &mut editor, &mut app, screen, Vec::new());
        }
        assert!(editor.test_marker_preview.iter().any(|observation| {
            observation.number == 11
                && (observation.ink_bounds.center() - observation.center).length() < 0.1
        }));
        let evidence_dir = std::env::var_os("ML_SCREENSHOT_MARKER_EVIDENCE_DIR");
        let tempdir = tempfile::tempdir().unwrap();
        let output_dir = evidence_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| tempdir.path().to_path_buf());
        std::fs::create_dir_all(&output_dir).unwrap();
        let output_path = output_dir.join("screenshot-marker-ui-crop.png");
        let expected = editor.apply_edits().unwrap();
        editor.save_image(&output_path).unwrap();
        let saved = image::io::Reader::open(&output_path)
            .unwrap()
            .decode()
            .unwrap()
            .to_rgba8();
        assert_eq!(saved, expected);
        assert_eq!(expected.width(), crop.width() as u32);
        assert_eq!(expected.height(), crop.height() as u32);

        if std::env::var_os("ML_SCREENSHOT_MARKER_EVIDENCE_DIR").is_some() {
            let observations = editor
                .test_marker_preview
                .iter()
                .map(|observation| {
                    format!(
                        "number={} center={:?} radius={} actual_galley_ink={:?}\n",
                        observation.number,
                        observation.center,
                        observation.radius,
                        observation.ink_bounds
                    )
                })
                .collect::<String>();
            std::fs::write(
                output_dir.join("screenshot-marker-ui-preview.txt"),
                observations,
            )
            .unwrap();
        }

        // Existing tools retain drag-only sensing: Text starts at the initial press, including
        // an ordinary click with no pointer movement.
        editor.tool = MarkupTool::Text;
        screenshot_editor_frame(&context, &mut editor, &mut app, screen, Vec::new());
        let text_image_rect = editor
            .test_image_rect
            .expect("Text canvas response is retained");
        let text_position = Pos2::new(72.0, 54.0);
        let text_screen_position = text_image_rect.min + (text_position * editor.zoom).to_vec2();
        screenshot_editor_frame(
            &context,
            &mut editor,
            &mut app,
            screen,
            vec![
                egui::Event::PointerMoved(text_screen_position),
                egui::Event::PointerButton {
                    pos: text_screen_position,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        let active_text_position = editor
            .active_text
            .as_ref()
            .expect("Text click creates an insertion point")
            .position;
        assert!(
            (active_text_position - text_position).length() < 0.01,
            "a Text-tool click should preserve its original press position: {active_text_position:?}"
        );
        screenshot_editor_frame(
            &context,
            &mut editor,
            &mut app,
            screen,
            vec![egui::Event::Text("legacy text".to_owned())],
        );
        screenshot_editor_frame(
            &context,
            &mut editor,
            &mut app,
            screen,
            vec![egui::Event::PointerButton {
                pos: text_screen_position,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        let output =
            screenshot_editor_frame_output(&context, &mut editor, &mut app, screen, Vec::new());
        let marker_button = control_bounds(&output, "Marker")
            .expect("the Marker tool remains an actual selectable response")
            .1;
        output.drop_without_applying_deltas();
        let marker_button_center = marker_button.center();
        screenshot_editor_frame(
            &context,
            &mut editor,
            &mut app,
            screen,
            vec![
                egui::Event::PointerMoved(marker_button_center),
                egui::Event::PointerButton {
                    pos: marker_button_center,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        screenshot_editor_frame(
            &context,
            &mut editor,
            &mut app,
            screen,
            vec![egui::Event::PointerButton {
                pos: marker_button_center,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        assert!(editor.active_text.is_none());
        assert!(
            matches!(editor.history.layers().last(), Some(MarkupLayer::Text(text)) if text.text == "legacy text")
        );
    }

    #[test]
    fn screenshot_chrome_scaled_toolbar_controls_remain_reachable_without_editing_image() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let root = tempfile::tempdir().unwrap();
        let mut app = screenshot_editor_test_app(&context, root.path());
        context.style_mut_of(context.theme(), |style| {
            style.animation_time = 0.0;
            style.override_font_id = Some(egui::FontId::proportional(22.0));
        });

        let image = RgbaImage::from_pixel(32, 32, image::Rgba([30, 60, 90, 255]));
        let source_pixels = image.as_raw().clone();
        let mut editor = ScreenshotEditor::new(
            image,
            root.path().join("unused.png"),
            false,
            false,
            MarkupTool::Pen,
        );
        editor.test_window_width = Some(640.0);
        let history_layers = editor.history.layers.clone();
        let redo_layers = editor.history.redo_stack.clone();
        let initial_tool = editor.tool;
        let initial_color = editor.color_index;
        let initial_thickness = editor.thickness;
        let initial_zoom = editor.zoom;
        let initial_text_size = editor.text_size;
        let initial_crop_start = editor.crop_start;
        let initial_crop_rect = editor.crop_rect;
        let initial_active_start = editor.active_start;
        let initial_active_end = editor.active_end;
        let initial_active_stroke = editor.active_stroke.clone();
        let initial_active_text = editor.active_text.clone();
        let wide_screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 620.0));
        let (wide_controls, wide_content_clip, wide_slider_layout, wide_tool_row) =
            settled_toolbar_controls(&context, &mut editor, &mut app, wide_screen);
        assert_toolbar_controls_reachable(&wide_controls, wide_screen, wide_content_clip);
        assert!(wide_tool_row.is_positive());
        assert!(wide_content_clip.contains_rect(wide_tool_row));
        let [wide_zoom, wide_text_size] =
            assert_slider_layout_reachable(wide_slider_layout, wide_content_clip);
        let wide_zoom_bounds = slider_full_bounds(wide_zoom);
        let wide_text_size_bounds = slider_full_bounds(wide_text_size);
        let wide_slider_sizing = editor.test_slider_sizing.unwrap();
        assert!(
            (wide_zoom_bounds.top()..=wide_zoom_bounds.bottom())
                .contains(&wide_text_size_bounds.center().y)
                && (wide_text_size_bounds.top()..=wide_text_size_bounds.bottom())
                    .contains(&wide_zoom_bounds.center().y),
            "wide sliders do not share a vertical band: parent/rounded-row/actual-row/min-inline/slot widths {wide_slider_sizing:?}; Zoom {wide_zoom:?}; Text Size {wide_text_size:?}"
        );
        assert!(!wide_zoom_bounds.intersects(wide_text_size_bounds));
        assert!(wide_zoom_bounds.bottom() < wide_tool_row.top());
        assert!(wide_text_size_bounds.bottom() < wide_tool_row.top());

        let point = Pos2::new(wide_zoom.track.right() - 3.0, wide_zoom.track.center().y);
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(wide_screen),
                    focused: true,
                    events: vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::default(),
                        },
                    ],
                    ..Default::default()
                },
                |ctx| editor.ui(ctx, &mut app),
            )
            .drop_without_applying_deltas();
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(wide_screen),
                    focused: true,
                    events: vec![egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::default(),
                    }],
                    ..Default::default()
                },
                |ctx| editor.ui(ctx, &mut app),
            )
            .drop_without_applying_deltas();
        assert!(editor.zoom > initial_zoom);
        assert_eq!(editor.text_size, initial_text_size);

        editor.test_window_width = Some(420.0);
        let narrow_screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(440.0, 620.0));
        let (narrow_controls, narrow_content_clip, narrow_slider_layout, narrow_tool_row) =
            settled_toolbar_controls(&context, &mut editor, &mut app, narrow_screen);
        assert!(wide_content_clip.width() > narrow_content_clip.width());
        assert_toolbar_controls_reachable(&narrow_controls, narrow_screen, narrow_content_clip);
        assert!(narrow_tool_row.is_positive());
        assert!(narrow_content_clip.contains_rect(narrow_tool_row));
        let [narrow_zoom, narrow_text_size] =
            assert_slider_layout_reachable(narrow_slider_layout, narrow_content_clip);
        let narrow_zoom_bounds = slider_full_bounds(narrow_zoom);
        let narrow_text_size_bounds = slider_full_bounds(narrow_text_size);
        assert!(narrow_zoom_bounds.bottom() < narrow_text_size_bounds.top());
        assert!(narrow_zoom_bounds.bottom() < narrow_tool_row.top());
        assert!(narrow_text_size_bounds.bottom() < narrow_tool_row.top());
        assert!(editor.open);
        assert_eq!(editor.image.as_raw(), &source_pixels);
        assert_eq!(editor.history.layers, history_layers);
        assert_eq!(editor.history.redo_stack, redo_layers);
        assert_eq!(editor.crop_start, initial_crop_start);
        assert_eq!(editor.crop_rect, initial_crop_rect);
        assert_eq!(editor.active_start, initial_active_start);
        assert_eq!(editor.active_end, initial_active_end);
        assert_eq!(editor.active_stroke, initial_active_stroke);
        assert_eq!(editor.active_text, initial_active_text);
        assert_eq!(editor.tool, initial_tool);
        assert_eq!(editor.color_index, initial_color);
        assert_eq!(editor.thickness, initial_thickness);
        assert!(editor.zoom > initial_zoom);
        assert_eq!(editor.text_size, initial_text_size);
    }
}
