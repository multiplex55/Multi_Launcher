use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use eframe::egui;
use std::path::PathBuf;

/// Simple panel to display an image that can be zoomed and scrolled.
pub struct ImagePanel {
    pub open: bool,
    path: PathBuf,
    texture: Option<egui::TextureHandle>,
    zoom: f32,
    session: u64,
    pub(super) host: super::utility_window_host::ImageWindowHost,
    #[cfg(test)]
    test_controls: Vec<(String, egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_external_opens: usize,
}

impl ImagePanel {
    pub fn new(path: PathBuf, session: u64) -> Self {
        Self {
            open: true,
            path,
            texture: None,
            zoom: 1.0,
            session,
            host: Default::default(),
            #[cfg(test)]
            test_controls: Vec::new(),
            #[cfg(test)]
            test_external_opens: 0,
        }
    }

    pub(crate) fn query_focus_layer_id(&self) -> egui::LayerId {
        egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new(("image-viewer", self.session)),
        )
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

    #[cfg(test)]
    pub(super) fn test_set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom;
    }
    #[cfg(test)]
    pub(super) fn test_zoom(&self) -> f32 {
        self.zoom
    }

    #[cfg(test)]
    pub(super) fn test_control(&self, name: &str) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.test_controls
            .iter()
            .find(|(key, ..)| key == name)
            .map(|(_, rect, clip, enabled)| (*rect, *clip, *enabled))
    }
    #[cfg(test)]
    pub(super) fn test_texture_id(&self) -> Option<egui::TextureId> {
        self.texture.as_ref().map(|texture| texture.id())
    }
    #[cfg(test)]
    pub(super) fn test_external_opens(&self) -> usize {
        self.test_external_opens
    }

    pub(super) fn session(&self) -> u64 {
        self.session
    }
    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }
    pub(super) fn render_host(
        &mut self,
        ctx: &egui::Context,
        allowed: bool,
        first: bool,
        child: Option<&mut egui::Ui>,
        presentation: super::utility_host::UtilityPresentation,
    ) -> ImageHostActions {
        self.load_texture(ctx);
        let native = child.is_some();
        let mut open = self.open;
        let mut zoom = self.zoom;
        let mut external_open = false;
        let mut host_request = None;
        let mut close = false;
        #[cfg(test)]
        let mut test_controls = Vec::new();
        let mut body = |ui: &mut egui::Ui| {
            let interactive = allowed && ui.is_visible() && !ui.is_sizing_pass();
            let (host, next) =
                super::utility_host::presentation_button(ui, presentation, interactive);
            #[cfg(test)]
            test_controls.push(("host".into(), host.rect, ui.clip_rect(), host.enabled()));
            if first && !ctx.will_discard() && host.clicked() {
                host_request = Some(next);
            }
            ui.add_enabled_ui(interactive, |ui| {
                if native {
                    let name = self
                        .path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Image");
                    ui.label(name)
                        .on_hover_text(self.path.display().to_string());
                }
                ui.horizontal(|ui| {
                    let slider = ui.add(egui::Slider::new(&mut zoom, 0.1..=5.0).text("Zoom"));
                    let external = ui.button("Open in Default Viewer");
                    let close_button = ui.button("Close");
                    #[cfg(test)]
                    for (name, response) in [
                        ("zoom", &slider),
                        ("external", &external),
                        ("close", &close_button),
                    ] {
                        test_controls.push((
                            name.into(),
                            response.rect,
                            ui.clip_rect(),
                            response.enabled(),
                        ));
                    }
                    #[cfg(not(test))]
                    let _ = slider;
                    if external.clicked() {
                        external_open = true;
                    }
                    if close_button.clicked() {
                        close = true;
                    }
                });
                if let Some(tex) = &self.texture {
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.add(egui::Image::new(tex).fit_to_exact_size(tex.size_vec2() * zoom));
                    });
                } else {
                    ui.label("Failed to load image");
                }
            });
        };
        if let Some(ui) = child {
            body(ui);
        } else {
            let title = self
                .path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Image");
            egui::Window::new(title)
                .id(egui::Id::new(("image-viewer", self.session)))
                .resizable(true)
                .show_utility(
                    ctx,
                    "image-viewer",
                    egui::Id::new(("image-viewer", self.session)),
                    Some(&mut open),
                    crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(
                            egui::vec2(620.0, 480.0),
                        ),
                        egui::vec2(200.0, 160.0),
                        [None, None],
                    ),
                    |ui| body(ui),
                );
        }
        #[cfg(test)]
        {
            self.test_controls = test_controls;
        }
        ImageHostActions {
            session: self.session,
            zoom,
            close: !open || close,
            host_request,
            external_open: external_open && first && allowed && !ctx.will_discard(),
        }
    }
    pub(super) fn apply_host_actions(&mut self, actions: ImageHostActions) {
        if !self.open || self.session != actions.session {
            return;
        }
        self.zoom = actions.zoom;
        if actions.external_open {
            #[cfg(test)]
            {
                self.test_external_opens += 1;
            }
            #[cfg(not(test))]
            {
                let _ = open::that(&self.path);
            }
        }
    }
}

pub(super) struct ImageHostActions {
    session: u64,
    zoom: f32,
    pub(super) close: bool,
    pub(super) host_request: Option<super::utility_host::UtilityPresentation>,
    external_open: bool,
}
