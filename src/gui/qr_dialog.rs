use crate::qr::{self, GeneratedQr, QrErrorCorrection, QrGenerationError, QrPayloadMetadata};
use eframe::egui;
use image::RgbaImage;

fn viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("QR Generator")
        .with_inner_size([620.0, 700.0])
        .with_min_inner_size([320.0, 420.0])
        .with_resizable(true)
}

/// Fresh invocations reset transient state; panel reopening only changes visibility.
pub struct QrDialogState {
    pub open: bool,
    pub source: String,
    pub error_correction: QrErrorCorrection,
    pub focus_source: bool,
    pub feedback: Option<String>,
    advanced_open: bool,
    metadata: QrPayloadMetadata,
    generated: Option<GeneratedQr>,
    raster: Option<RgbaImage>,
    generation_error: Option<QrGenerationError>,
    texture: Option<egui::TextureHandle>,
    dirty: bool,
    revision: u64,
}

impl Default for QrDialogState {
    fn default() -> Self {
        Self {
            open: false,
            source: String::new(),
            error_correction: QrErrorCorrection::default(),
            focus_source: false,
            feedback: None,
            advanced_open: false,
            metadata: QrPayloadMetadata::from_source(""),
            generated: None,
            raster: None,
            generation_error: None,
            texture: None,
            dirty: false,
            revision: 0,
        }
    }
}

impl QrDialogState {
    pub fn open(&mut self, initial_text: Option<&str>) {
        *self = Self {
            open: true,
            source: initial_text.unwrap_or_default().to_owned(),
            focus_source: true,
            dirty: true,
            ..Self::default()
        };
        self.refresh_generation();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.focus_source = false;
        self.feedback = None;
        self.generated = None;
        self.raster = None;
        self.generation_error = None;
        self.texture = None;
        self.dirty = true;
    }

    pub fn set_source(&mut self, source: String) {
        if self.source != source {
            self.source = source;
            self.dirty = true;
            self.refresh_generation();
        }
    }

    pub fn set_error_correction(&mut self, level: QrErrorCorrection) {
        if self.error_correction != level {
            self.error_correction = level;
            self.dirty = true;
            self.refresh_generation();
        }
    }

    fn refresh_generation(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        self.revision = self.revision.wrapping_add(1);
        self.metadata = QrPayloadMetadata::from_source(&self.source);
        self.generated = None;
        self.raster = None;
        self.generation_error = None;
        self.texture = None;
        self.feedback = None;
        if self.source.is_empty() {
            return;
        }
        match qr::generate(&self.source, self.error_correction) {
            Ok(generated) => {
                self.raster = Some(generated.to_rgba_image());
                self.generated = Some(generated);
            }
            Err(error) => self.generation_error = Some(error),
        }
    }

    fn refresh_texture(&mut self, ctx: &egui::Context) {
        if self.texture.is_some() {
            return;
        }
        if let Some(raster) = &self.raster {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [raster.width() as usize, raster.height() as usize],
                raster.as_raw(),
            );
            self.texture =
                Some(ctx.load_texture("qr_preview", image, egui::TextureOptions::NEAREST));
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        self.refresh_generation();
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("qr_generator_viewport"),
            viewport_builder(),
            |child, class| {
                let independent = class == egui::ViewportClass::Immediate;
                if independent && self.focus_source {
                    child.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                let close = child.input_mut(|input| {
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                        || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                        || (independent && input.viewport().close_requested())
                });
                if close {
                    self.close();
                } else if independent {
                    egui::CentralPanel::default().show(child, |ui| self.body(ui));
                } else {
                    // Test contexts embed viewports; keep the fallback within its parent.
                    let size = child.input(|input| input.screen_rect().size());
                    let available = (size - egui::vec2(24.0, 48.0)).max(egui::vec2(1.0, 1.0));
                    let mut open = self.open;
                    egui::Window::new("QR Generator")
                        .id(egui::Id::new("qr_generator_embedded"))
                        .open(&mut open)
                        .collapsible(false)
                        .default_size(available.min(egui::vec2(620.0, 700.0)))
                        .min_size(available.min(egui::vec2(320.0, 420.0)))
                        .max_size(available)
                        .show(child, |ui| self.body(ui));
                    if !open {
                        self.close();
                    }
                }
                if independent && !self.open {
                    child.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
        );
    }

    fn body(&mut self, ui: &mut egui::Ui) {
        let mut close = false;
        egui::ScrollArea::vertical()
            .id_source("qr_contents")
            .show(ui, |ui| {
                    ui.label("Text or URL");
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut self.source)
                            .id(egui::Id::new("qr_source"))
                            .desired_width(f32::INFINITY)
                            .desired_rows(5)
                            .hint_text("Enter or paste text to generate a QR code."),
                    );
                    if self.focus_source {
                        response.request_focus();
                        self.focus_source = false;
                    }
                    if response.changed() {
                        self.dirty = true;
                        self.refresh_generation();
                    }
                    ui.label(format!(
                        "{} characters · {} UTF-8 bytes",
                        self.metadata.character_count, self.metadata.utf8_byte_count
                    ));
                    ui.separator();
                    let mut selected = self.error_correction;
                    let advanced = egui::CollapsingHeader::new("Advanced")
                        .id_source("qr_advanced")
                        .open(Some(self.advanced_open))
                        .show(ui, |ui| {
                            ui.label("Higher error correction improves recovery from damage, but reduces payload capacity.");
                            egui::ComboBox::from_id_source("qr_error_correction")
                                .selected_text(selected.label())
                                .show_ui(ui, |ui| {
                                    for level in [QrErrorCorrection::Low, QrErrorCorrection::Medium,
                                        QrErrorCorrection::Quartile, QrErrorCorrection::High] {
                                        ui.selectable_value(&mut selected, level, level.label());
                                    }
                                });
                        });
                    if advanced.header_response.clicked() {
                        self.advanced_open = !self.advanced_open;
                    }
                    self.set_error_correction(selected);
                    ui.separator();
                    if let Some(error) = self.generation_error {
                        let message = match error {
                            QrGenerationError::CapacityExceeded => {
                                "Text exceeds QR capacity. Shorten text or choose lower error correction."
                            },
                            QrGenerationError::EncodingFailed => "Unable to generate QR. Edit the text and try again.",
                            QrGenerationError::EmptyPayload => "Enter or paste text to generate a QR code.",
                        };
                        ui.colored_label(ui.visuals().error_fg_color, message);
                    } else if self.source.is_empty() {
                        ui.label("Enter or paste text to generate a QR code.");
                    } else {
                        self.refresh_texture(ui.ctx());
                        if let Some(texture) = &self.texture {
                            let side = ui.available_width().min(400.0);
                            ui.horizontal(|ui| {
                                ui.add_space(((ui.available_width() - side) / 2.0).max(0.0));
                                ui.image((texture.id(), egui::vec2(side, side)));
                            });
                        }
                    }
                    ui.separator();
                    close = ui.button("Close").clicked();
                });
        if close {
            self.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_viewport_has_scannable_size_and_embedded_escape_cleans_up() {
        let builder = viewport_builder();
        assert_eq!(builder.inner_size, Some(egui::vec2(620.0, 700.0)));
        let ctx = egui::Context::default();
        let mut state = QrDialogState::default();
        state.open(Some("exact source"));
        let _ = ctx.run(egui::RawInput::default(), |ctx| state.show(ctx));
        assert!(!state.focus_source);
        state.feedback = Some("old".into());
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = ctx.run(input, |ctx| state.show(ctx));
        assert!(!state.open);
        assert!(state.feedback.is_none() && state.texture.is_none() && state.raster.is_none());
        assert_eq!(state.source, "exact source");
    }

    #[test]
    fn qr_fresh_invocation_resets_state_and_preserves_exact_source() {
        let mut state = QrDialogState::default();
        let source = "  caf\u{e9}\n\u{1f512} ";
        state.open(Some(source));
        assert_eq!(state.source, source);
        assert!(state.open && state.focus_source);
        assert!(state.raster.is_some());
        assert_eq!(state.error_correction, QrErrorCorrection::Medium);
        state.set_error_correction(QrErrorCorrection::High);
        state.feedback = Some("Old status".into());
        state.focus_source = false;
        state.open(None);
        assert!(state.source.is_empty());
        assert!(state.focus_source);
        assert!(state.feedback.is_none());
        assert!(state.raster.is_none());
        assert!(state.generation_error.is_none());
        assert_eq!(state.error_correction, QrErrorCorrection::Medium);
    }

    #[test]
    fn qr_editor_refreshes_exact_source_and_reuses_unchanged_generation_and_texture() {
        let mut state = QrDialogState::default();
        state.open(Some("first"));
        let initial_revision = state.revision;
        let ctx = egui::Context::default();
        state.refresh_texture(&ctx);
        let texture_id = state.texture.as_ref().unwrap().id();
        state.refresh_generation();
        state.refresh_texture(&ctx);
        state.set_source("first".into());
        assert_eq!(state.revision, initial_revision);
        assert_eq!(state.texture.as_ref().unwrap().id(), texture_id);
        let source = "  caf\u{e9}\n\u{1f512} ";
        state.set_source(source.into());
        assert_eq!(state.source, source);
        assert_eq!(state.metadata, QrPayloadMetadata::from_source(source));
        assert_eq!(
            state.generated,
            Some(qr::generate(source, QrErrorCorrection::Medium).unwrap())
        );
        assert!(state.texture.is_none());
        assert!(state.revision > initial_revision);
        state.set_source("   ".into());
        assert!(state.raster.is_some());
        state.set_source(String::new());
        assert!(
            state.raster.is_none() && state.texture.is_none() && state.generation_error.is_none()
        );
    }

    #[test]
    fn qr_correction_choices_regenerate_and_capacity_changes_preserve_source() {
        let mut state = QrDialogState::default();
        state.open(Some("short text"));
        for level in [
            QrErrorCorrection::Low,
            QrErrorCorrection::Medium,
            QrErrorCorrection::Quartile,
            QrErrorCorrection::High,
        ] {
            state.set_error_correction(level);
            assert_eq!(state.generated.as_ref().unwrap().error_correction(), level);
            let revision = state.revision;
            state.set_error_correction(level);
            assert_eq!(state.revision, revision);
        }
        let source = "x".repeat(1500);
        state.open(Some(&source));
        assert!(state.raster.is_some());
        state.refresh_texture(&egui::Context::default());
        state.set_error_correction(QrErrorCorrection::High);
        assert_eq!(state.error_correction, QrErrorCorrection::High);
        assert_eq!(state.source, source);
        assert_eq!(
            state.generation_error,
            Some(QrGenerationError::CapacityExceeded)
        );
        assert!(state.generated.is_none() && state.raster.is_none() && state.texture.is_none());
        state.set_error_correction(QrErrorCorrection::Low);
        assert_eq!(state.source, source);
        assert!(state.generated.is_some() && state.raster.is_some());
        assert!(state.generation_error.is_none());
        state.advanced_open = true;
        state.open(None);
        assert!(!state.advanced_open);
        assert_eq!(state.error_correction, QrErrorCorrection::Medium);
    }

    #[test]
    fn qr_capacity_failure_clears_stale_output_and_recovers_without_truncation() {
        let mut state = QrDialogState::default();
        state.open(Some("valid"));
        state.refresh_texture(&egui::Context::default());
        assert!(state.texture.is_some());
        let oversized = "x".repeat(10000);
        state.set_source(oversized.clone());
        assert_eq!(state.source, oversized);
        assert_eq!(state.metadata.utf8_byte_count, 10000);
        assert_eq!(
            state.generation_error,
            Some(QrGenerationError::CapacityExceeded)
        );
        assert!(state.generated.is_none() && state.raster.is_none() && state.texture.is_none());
        state.set_source("recovered".into());
        assert!(state.generated.is_some() && state.raster.is_some());
        assert!(state.generation_error.is_none());
        state.close();
        assert!(state.raster.is_none() && state.texture.is_none());
        state.open = true;
        state.refresh_generation();
        assert!(state.raster.is_some());
        assert_eq!(state.source, "recovered");
    }
}
