use eframe::egui;
use std::path::PathBuf;

/// Simple panel to display an image that can be zoomed and scrolled.
pub struct ImagePanel {
    pub open: bool,
    path: PathBuf,
    texture: Option<egui::TextureHandle>,
    zoom: f32,
}

impl ImagePanel {
    pub fn new(path: PathBuf) -> Self {
        Self {
            open: true,
            path,
            texture: None,
            zoom: 1.0,
        }
    }

    fn load_texture(&mut self, ctx: &egui::Context) {
        if self.texture.is_some() {
            return;
        }
        if let Ok(img) = image::open(&self.path) {
            let size = [img.width() as usize, img.height() as usize];
            let rgba = img.to_rgba8();
            let tex = ctx.load_texture(
                self.path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("image"),
                egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
                egui::TextureOptions::LINEAR,
            );
            self.texture = Some(tex);
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        self.load_texture(ctx);
        let mut open = self.open;
        let title = self
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Image");
        egui::Window::new(title)
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut self.zoom, 0.1..=5.0).text("Zoom"));
                    if ui.button("Open in Default Viewer").clicked() {
                        let _ = open::that(&self.path);
                    }
                });
                if let Some(tex) = &self.texture {
                    let size = tex.size_vec2() * self.zoom;
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.add(egui::Image::new(tex).fit_to_exact_size(size));
                    });
                } else {
                    ui.label("Failed to load image");
                }
            });
        self.open = open;
    }
}
