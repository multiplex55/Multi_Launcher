use crate::gui::LauncherApp;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke};
use egui_extras::RetainedImage;
use image::RgbaImage;
use std::path::PathBuf;

/// Editor window for captured screenshots allowing simple cropping and annotation.
///
/// Cropping is initiated by dragging with the primary mouse button. Holding
/// `Shift` while dragging creates a red annotation rectangle. When saving or
/// copying the screenshot the selected region and annotations are applied to the
/// output image.
pub struct ScreenshotEditor {
    pub open: bool,
    image: RgbaImage,
    tex: RetainedImage,
    crop_start: Option<Pos2>,
    crop_rect: Option<Rect>,
    ann_start: Option<Pos2>,
    annotations: Vec<Rect>,
    path: PathBuf,
    _clip: bool,
    auto_save: bool,
    zoom: f32,
}

impl ScreenshotEditor {
    /// Create a new editor from the captured image.
    pub fn new(img: RgbaImage, path: PathBuf, clip: bool, auto_save: bool) -> Self {
        let size = [img.width() as usize, img.height() as usize];
        let tex = RetainedImage::from_color_image(
            "screenshot",
            egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()),
        );
        Self {
            open: true,
            image: img,
            tex,
            crop_start: None,
            crop_rect: None,
            ann_start: None,
            annotations: Vec::new(),
            path,
            _clip: clip,
            auto_save,
            zoom: 1.0,
        }
    }

    fn apply_edits(&self) -> RgbaImage {
        let mut img = self.image.clone();
        // draw annotations first
        for rect in &self.annotations {
            let x1 = rect.min.x.max(0.0) as u32;
            let y1 = rect.min.y.max(0.0) as u32;
            let x2 = rect.max.x.min(img.width() as f32) as u32;
            let y2 = rect.max.y.min(img.height() as f32) as u32;
            for y in y1..y2 {
                for x in x1..x2 {
                    img.put_pixel(x, y, image::Rgba([255, 0, 0, 128]));
                }
            }
        }
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
                        let res = if self.auto_save {
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
                        if let Err(e) = res {
                            app.set_error(format!("Failed to save screenshot: {e}"));
                        }
                    }
                    if ui.button("Copy").clicked() {
                        if let Err(e) = self.copy_to_clipboard() {
                            app.set_error(format!("Failed to copy screenshot: {e}"));
                        } else if app.get_screenshot_save_file() {
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
                    ui.add(egui::Slider::new(&mut self.zoom, 0.1..=4.0).text("Zoom"));
                });
                let tex = self.tex.texture_id(ctx);
                let img_size = self.tex.size_vec2();
                let display = img_size * self.zoom;
                let (response, painter) = ui.allocate_painter(display, Sense::drag());
                let to_img = |pos: Pos2| {
                    let offset = response.rect.min;
                    ((pos - offset) / self.zoom).to_pos2()
                };
                let to_screen = |p: Pos2| response.rect.min + (p * self.zoom).to_vec2();
                painter.image(
                    tex,
                    response.rect,
                    Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
                if response.drag_started() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        if ctx.input(|i| i.modifiers.shift) {
                            self.ann_start = Some(to_img(pos));
                        } else {
                            self.crop_start = Some(to_img(pos));
                            self.crop_rect = None;
                        }
                    }
                }
                if response.dragged() {
                    if let Some(start) = self.crop_start {
                        if let Some(pos) = response.interact_pointer_pos() {
                            self.crop_rect = Some(Rect::from_two_pos(start, to_img(pos)));
                        }
                    }
                    if let Some(start) = self.ann_start {
                        if let Some(pos) = response.interact_pointer_pos() {
                            self.annotations
                                .push(Rect::from_two_pos(start, to_img(pos)));
                            self.ann_start = None;
                        }
                    }
                }
                if response.dragged_stopped() {
                    self.crop_start = None;
                    self.ann_start = None;
                }
                if let Some(rect) = self.crop_rect {
                    let draw = Rect::from_min_max(to_screen(rect.min), to_screen(rect.max));
                    painter.rect_stroke(draw, 0.0, Stroke::new(1.0, Color32::GREEN));
                }
                for rect in &self.annotations {
                    let draw = Rect::from_min_max(to_screen(rect.min), to_screen(rect.max));
                    painter.rect_stroke(draw, 0.0, Stroke::new(1.0, Color32::RED));
                }
            });
        self.open = open;
    }
}
