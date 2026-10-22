use crate::annotation::raster::{
    self, Color as RasterColor, Point as RasterPoint, Rect as RasterRect,
};
use crate::gui::LauncherApp;
use eframe::egui::{
    self, Color32, PointerButton, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2,
};
use egui::emath::GuiRounding as _;
use egui_toast::{Toast, ToastKind, ToastOptions};
use image::RgbaImage;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkupTool {
    Pen,
    Arrow,
    Rectangle,
    Highlight,
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
    Text(MarkupText),
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

pub fn render_markup_layers(base: &RgbaImage, layers: &[MarkupLayer]) -> RgbaImage {
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
        }
    }
    img
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
    history: MarkupHistory,
    path: PathBuf,
    _clip: bool,
    auto_save: bool,
    zoom: f32,
    tool: MarkupTool,
    color_index: usize,
    thickness: f32,
    text_size: f32,
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
            history: MarkupHistory::default(),
            path,
            _clip: clip,
            auto_save,
            zoom: 1.0,
            tool,
            color_index: 0,
            thickness: 4.0,
            text_size: 18.0,
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
        }
    }

    fn apply_edits(&self) -> RgbaImage {
        let mut img = render_markup_layers(&self.image, self.history.layers());
        if let Some(rect) = self.crop_rect {
            let x1 = rect.min.x.max(0.0) as u32;
            let y1 = rect.min.y.max(0.0) as u32;
            let x2 = rect.max.x.min(img.width() as f32) as u32;
            let y2 = rect.max.y.min(img.height() as f32) as u32;
            let w = (x2 - x1).max(1);
            let h = (y2 - y1).max(1);
            img = image::imageops::crop_imm(&img, x1, y1, w, h).to_image();
        }
        img
    }

    fn save_image(&self, path: &PathBuf) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let img = self.apply_edits();
        img.save(path)?;
        Ok(())
    }

    fn copy_to_clipboard(&self) -> anyhow::Result<()> {
        use std::borrow::Cow;
        let img = self.apply_edits();
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
                let (response, painter) = ui.allocate_painter(display, Sense::drag());
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
                if response.drag_started_by(PointerButton::Secondary)
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
                if response.drag_stopped_by(PointerButton::Secondary) {
                    self.crop_start = None;
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
                        MarkupTool::Text => {}
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
                        MarkupTool::Text => {}
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
                        MarkupLayer::Text(text) => {
                            painter.text(
                                to_screen(text.position),
                                egui::Align2::LEFT_TOP,
                                &text.text,
                                egui::FontId::proportional(text.size),
                                text.color,
                            );
                        }
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
                        MarkupTool::Pen | MarkupTool::Text => {}
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

    const CONTROLS: [&str; 18] = [
        "Save",
        "Copy",
        "Pen",
        "Arrow",
        "Rect",
        "Highlight",
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

    #[test]
    fn screenshot_chrome_scaled_toolbar_controls_remain_reachable_without_editing_image() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let root = tempfile::tempdir().unwrap();
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
        let mut app = LauncherApp::new(
            &context,
            Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test(),
            root.path()
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            root.path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            settings,
            None,
            None,
            Some(std::collections::HashSet::new()),
            Some(std::collections::HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
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
