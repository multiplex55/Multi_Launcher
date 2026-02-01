use crate::gui::LauncherApp;
use eframe::egui::{
    self, Color32, PointerButton, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2,
};
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
    undo_stack: Vec<MarkupLayer>,
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

fn blend_pixel(img: &mut RgbaImage, x: u32, y: u32, color: Color32) {
    let [r, g, b, a] = color.to_array();
    if a == 0 {
        return;
    }
    let dst = img.get_pixel(x, y).0;
    let src_a = a as f32 / 255.0;
    let dst_a = dst[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 0.0 {
        return;
    }
    let blend = |src: u8, dst: u8| {
        let src_f = src as f32 / 255.0;
        let dst_f = dst as f32 / 255.0;
        ((src_f * src_a + dst_f * dst_a * (1.0 - src_a)) / out_a * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    img.put_pixel(
        x,
        y,
        image::Rgba([
            blend(r, dst[0]),
            blend(g, dst[1]),
            blend(b, dst[2]),
            (out_a * 255.0) as u8,
        ]),
    );
}

fn draw_circle(img: &mut RgbaImage, center: Pos2, radius: f32, color: Color32) {
    if radius <= 0.0 {
        return;
    }
    let radius_sq = radius * radius;
    let width = img.width() as i32;
    let height = img.height() as i32;
    let min_x = (center.x - radius).floor().max(0.0) as i32;
    let max_x = (center.x + radius).ceil().min((width - 1) as f32) as i32;
    let min_y = (center.y - radius).floor().max(0.0) as i32;
    let max_y = (center.y + radius).ceil().min((height - 1) as f32) as i32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 + 0.5 - center.x;
            let dy = y as f32 + 0.5 - center.y;
            if dx * dx + dy * dy <= radius_sq {
                blend_pixel(img, x as u32, y as u32, color);
            }
        }
    }
}

fn draw_line(img: &mut RgbaImage, start: Pos2, end: Pos2, color: Color32, thickness: f32) {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let steps = dx.abs().max(dy.abs()).ceil().max(1.0) as i32;
    let radius = (thickness / 2.0).max(0.5);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let point = Pos2::new(start.x + dx * t, start.y + dy * t);
        draw_circle(img, point, radius, color);
    }
}

fn draw_rect_outline(img: &mut RgbaImage, rect: Rect, color: Color32, thickness: f32) {
    let min = rect.min;
    let max = rect.max;
    draw_line(
        img,
        Pos2::new(min.x, min.y),
        Pos2::new(max.x, min.y),
        color,
        thickness,
    );
    draw_line(
        img,
        Pos2::new(max.x, min.y),
        Pos2::new(max.x, max.y),
        color,
        thickness,
    );
    draw_line(
        img,
        Pos2::new(max.x, max.y),
        Pos2::new(min.x, max.y),
        color,
        thickness,
    );
    draw_line(
        img,
        Pos2::new(min.x, max.y),
        Pos2::new(min.x, min.y),
        color,
        thickness,
    );
}

fn draw_rect_fill(img: &mut RgbaImage, rect: Rect, color: Color32) {
    let width = img.width() as i32;
    let height = img.height() as i32;
    let min_x = rect.min.x.floor().max(0.0) as i32;
    let max_x = rect.max.x.ceil().min((width - 1) as f32) as i32;
    let min_y = rect.min.y.floor().max(0.0) as i32;
    let max_y = rect.max.y.ceil().min((height - 1) as f32) as i32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            blend_pixel(img, x as u32, y as u32, color);
        }
    }
}

fn rotate_vec(vec: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(vec.x * cos - vec.y * sin, vec.x * sin + vec.y * cos)
}

fn default_font_data() -> Option<(egui::FontData, egui::FontTweak)> {
    let definitions = egui::FontDefinitions::default();
    let family = definitions.families.get(&egui::FontFamily::Proportional)?;
    let font_name = family.first()?;
    let data = definitions.font_data.get(font_name)?.clone();
    Some((data.clone(), data.tweak))
}

fn default_font_arc() -> Option<(ab_glyph::FontArc, egui::FontTweak)> {
    let (data, tweak) = default_font_data()?;
    let font = match data.font {
        std::borrow::Cow::Borrowed(bytes) => {
            ab_glyph::FontRef::try_from_slice_and_index(bytes, data.index)
                .map(ab_glyph::FontArc::from)
                .ok()
        }
        std::borrow::Cow::Owned(bytes) => {
            ab_glyph::FontVec::try_from_vec_and_index(bytes, data.index)
                .map(ab_glyph::FontArc::from)
                .ok()
        }
    }?;
    Some((font, tweak))
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
    use ab_glyph::{point, Font, ScaleFont};
    if text.is_empty() {
        return;
    }
    let scaled = font.as_scaled(size * tweak.scale);
    let mut caret = point(pos.x, pos.y + scaled.ascent() + tweak.y_offset * size);
    for ch in text.chars() {
        let mut glyph = scaled.scaled_glyph(ch);
        glyph.position = caret;
        caret.x += scaled.h_advance(glyph.id);
        if let Some(outlined) = scaled.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|x, y, coverage| {
                let px = x as i32 + bounds.min.x as i32;
                let py = y as i32 + bounds.min.y as i32;
                if px >= 0 && py >= 0 && px < img.width() as i32 && py < img.height() as i32 {
                    let alpha = (color.a() as f32 * coverage).round().clamp(0.0, 255.0) as u8;
                    let blended =
                        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
                    blend_pixel(img, px as u32, py as u32, blended);
                }
            });
        }
    }
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
                        *tweak,
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
        if let Some(active) = self.active_text.take() {
            if !active.text.is_empty() {
                self.push_layer(MarkupLayer::Text(MarkupText {
                    position: active.position,
                    text: active.text,
                    color: active.color,
                    size: active.size,
                }));
            }
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Screenshot Editor")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
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
                                if let Some(path) = saved_to {
                                    if app.enable_toasts {
                                        app.add_toast(Toast {
                                            text: format!("Saved screenshot {}", path.display())
                                                .into(),
                                            kind: ToastKind::Success,
                                            options: ToastOptions::default()
                                                .duration_in_seconds(app.toast_duration as f64),
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                app.set_error(format!("Failed to save screenshot: {e}"));
                            }
                        }
                    }
                    if ui.button("Copy").clicked() {
                        if let Err(e) = self.copy_to_clipboard() {
                            app.set_error(format!("Failed to copy screenshot: {e}"));
                        } else {
                            if app.enable_toasts {
                                app.add_toast(Toast {
                                    text: "Copied current markup to clipboard".into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(app.toast_duration as f64),
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
                    ui.add(egui::Slider::new(&mut self.zoom, 0.1..=4.0).text("Zoom"));
                    ui.add(egui::Slider::new(&mut self.text_size, 6.0..=48.0).text("Text Size"));
                });
                let prev_tool = self.tool;
                ui.horizontal(|ui| {
                    ui.label("Tool");
                    ui.selectable_value(&mut self.tool, MarkupTool::Pen, "Pen");
                    ui.selectable_value(&mut self.tool, MarkupTool::Arrow, "Arrow");
                    ui.selectable_value(&mut self.tool, MarkupTool::Rectangle, "Rect");
                    ui.selectable_value(&mut self.tool, MarkupTool::Highlight, "Highlight");
                    ui.selectable_value(&mut self.tool, MarkupTool::Text, "Text");
                    ui.separator();
                    ui.label("Color");
                    for (idx, color) in Self::palette().iter().enumerate() {
                        let selected = self.color_index == idx;
                        let mut button = egui::Button::new(format!("{}", idx + 1))
                            .fill(*color)
                            .stroke(Stroke::new(1.0, Color32::BLACK));
                        if selected {
                            button = button.stroke(Stroke::new(2.0, Color32::WHITE));
                        }
                        if ui.add(button).clicked() {
                            self.color_index = idx;
                        }
                    }
                    ui.separator();
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
                if response.drag_started_by(PointerButton::Secondary) {
                    if let Some(pos) = response.interact_pointer_pos() {
                        self.crop_start = Some(to_img(pos));
                        self.crop_rect = None;
                    }
                }
                if response.dragged_by(PointerButton::Secondary) {
                    if let Some(start) = self.crop_start {
                        if let Some(pos) = response.interact_pointer_pos() {
                            self.crop_rect = Some(Rect::from_two_pos(start, to_img(pos)));
                        }
                    }
                }
                if response.drag_stopped_by(PointerButton::Secondary) {
                    self.crop_start = None;
                }

                if response.drag_started_by(PointerButton::Primary) {
                    if let Some(pos) = response.interact_pointer_pos() {
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
                }
                if response.dragged_by(PointerButton::Primary) {
                    if let Some(pos) = response.interact_pointer_pos() {
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
                }
                if response.drag_stopped_by(PointerButton::Primary) {
                    match self.tool {
                        MarkupTool::Pen => {
                            if let Some(stroke) = self.active_stroke.take() {
                                if stroke.points.len() > 1 {
                                    self.push_layer(MarkupLayer::Stroke(stroke));
                                }
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
                    painter.rect_stroke(draw, 0.0, Stroke::new(1.0, Color32::GREEN));
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
                            painter.rect_stroke(draw, 0.0, Stroke::new(rect.thickness, rect.color));
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
                if let Some(active) = &self.active_text {
                    if !active.text.is_empty() {
                        painter.text(
                            to_screen(active.position),
                            egui::Align2::LEFT_TOP,
                            &active.text,
                            egui::FontId::proportional(active.size),
                            active.color,
                        );
                    }
                }
            });
        self.open = open;
    }
}
