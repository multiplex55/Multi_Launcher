use crate::clipboard_modify::clipboard::{ArboardClipboardBackend, ClipboardBackend};
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
    pub feedback: Option<Result<String, String>>,
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

    fn paste(&mut self, clipboard: &impl ClipboardBackend) {
        match clipboard.read_text() {
            Ok(source) => {
                self.set_source(source);
                self.feedback = Some(Ok("Pasted clipboard text".into()));
            }
            Err(_) => {
                self.feedback = Some(Err(
                    "Could not read clipboard text. Copy text and try again.".into(),
                ))
            }
        }
    }

    fn copy_text(&mut self, clipboard: &impl ClipboardBackend) {
        if self.source.is_empty() {
            return;
        }
        self.feedback = Some(
            clipboard
                .write_text(&self.source)
                .map(|()| "Copied text to clipboard".into())
                .map_err(|_| "Could not copy text to clipboard. Try again.".into()),
        );
    }

    fn copy_qr_with(&mut self, write: impl FnOnce(arboard::ImageData<'_>) -> Result<(), ()>) {
        let Some(raster) = &self.raster else {
            return;
        };
        let image = arboard::ImageData {
            width: raster.width() as usize,
            height: raster.height() as usize,
            bytes: std::borrow::Cow::Borrowed(raster.as_raw()),
        };
        self.feedback = Some(
            write(image)
                .map(|()| "Copied QR image to clipboard".into())
                .map_err(|()| "Could not copy QR image. Try again.".into()),
        );
    }

    fn save_png_with(
        &mut self,
        choose: impl FnOnce() -> Option<std::path::PathBuf>,
        write: impl FnOnce(&std::path::Path, &RgbaImage) -> Result<(), ()>,
    ) {
        let Some(raster) = &self.raster else {
            return;
        };
        let Some(path) = choose() else {
            return;
        };
        // Write exactly the destination confirmed by the native dialog.
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        {
            self.feedback = Some(Err("Choose a filename ending in .png.".into()));
            return;
        }
        self.feedback = Some(
            write(&path, raster)
                .map(|()| "Saved QR PNG".into())
                .map_err(|()| "Could not save QR PNG. Check the destination and try again.".into()),
        );
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
                    ui.horizontal(|ui| {
                        if ui.button("Paste").clicked() {
                            self.paste(&ArboardClipboardBackend);
                        }
                        if ui.add_enabled(!self.source.is_empty(), egui::Button::new("Copy Text")).clicked() {
                            self.copy_text(&ArboardClipboardBackend);
                        }
                    });
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
                    ui.horizontal(|ui| {
                        let valid = self.raster.is_some();
                        if ui.add_enabled(valid, egui::Button::new("Copy QR")).clicked() {
                            self.copy_qr_with(|image| {
                                arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_image(image)).map_err(|_| ())
                            });
                        }
                        if ui.add_enabled(valid, egui::Button::new("Save PNG")).clicked() {
                            self.save_png_with(
                                || rfd::FileDialog::new().add_filter("PNG image", &["png"])
                                    .set_file_name("multi_launcher_qr.png").save_file(),
                                |path, raster| raster.save_with_format(path, image::ImageFormat::Png).map_err(|_| ()),
                            );
                        }
                    });
                    ui.separator();
                    if let Some(feedback) = &self.feedback {
                        match feedback {
                            Ok(message) => { ui.label(message); }
                            Err(message) => { ui.colored_label(ui.visuals().error_fg_color, message); }
                        }
                    }
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

    #[derive(Default)]
    struct FakeClipboard {
        text: std::sync::Mutex<String>,
        reads: std::sync::atomic::AtomicUsize,
        writes: std::sync::atomic::AtomicUsize,
        fail: std::sync::atomic::AtomicBool,
    }
    impl ClipboardBackend for FakeClipboard {
        fn read_text(&self) -> Result<String, crate::clipboard_modify::clipboard::ClipboardError> {
            self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(
                    crate::clipboard_modify::clipboard::ClipboardError::Permanent("secret".into()),
                );
            }
            Ok(self.text.lock().unwrap().clone())
        }
        fn write_text(
            &self,
            text: &str,
        ) -> Result<(), crate::clipboard_modify::clipboard::ClipboardError> {
            self.writes
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(
                    crate::clipboard_modify::clipboard::ClipboardError::Permanent("secret".into()),
                );
            }
            *self.text.lock().unwrap() = text.into();
            Ok(())
        }
    }

    #[test]
    fn qr_text_clipboard_actions_are_explicit_exact_and_preserve_state_on_failure() {
        use std::sync::atomic::Ordering::SeqCst;
        let clipboard = FakeClipboard::default();
        let mut state = QrDialogState::default();
        state.open(None);
        state.open(Some("original"));
        state.set_source("edited".into());
        state.set_error_correction(QrErrorCorrection::Low);
        assert_eq!(clipboard.reads.load(SeqCst), 0);
        assert_eq!(clipboard.writes.load(SeqCst), 0);
        let source = "  caf\u{e9}\n\u{1f512} ";
        *clipboard.text.lock().unwrap() = source.into();
        state.paste(&clipboard);
        assert_eq!(state.source, source);
        assert!(state.raster.is_some());
        assert_eq!(clipboard.reads.load(SeqCst), 1);
        assert!(matches!(state.feedback, Some(Ok(_))));
        let revision = state.revision;
        let raster = state.raster.clone();
        state.copy_text(&clipboard);
        assert_eq!(*clipboard.text.lock().unwrap(), source);
        assert_eq!(clipboard.writes.load(SeqCst), 1);
        assert_eq!(state.revision, revision);
        assert_eq!(state.raster, raster);
        clipboard.fail.store(true, SeqCst);
        state.paste(&clipboard);
        assert_eq!(state.source, source);
        assert_eq!(state.raster, raster);
        assert_eq!(state.revision, revision);
        assert!(matches!(state.feedback, Some(Err(_))));
        assert!(!format!("{:?}", state.feedback).contains("secret"));
        state.copy_text(&clipboard);
        assert_eq!(state.source, source);
        assert_eq!(state.raster, raster);
        assert!(matches!(state.feedback, Some(Err(_))));
        clipboard.fail.store(false, SeqCst);
        let oversized = "x".repeat(10000);
        state.set_source(oversized.clone());
        assert!(state.raster.is_none());
        state.copy_text(&clipboard);
        assert_eq!(*clipboard.text.lock().unwrap(), oversized);
        state.set_source(String::new());
        state.copy_text(&clipboard);
        assert_eq!(clipboard.writes.load(SeqCst), 3);
    }

    #[test]
    fn qr_image_copy_uses_cached_rgba_and_preserves_state_on_failure() {
        let mut state = QrDialogState::default();
        state.copy_qr_with(|_| panic!("empty output must not call writer"));
        state.open(Some("exact source"));
        state.refresh_texture(&egui::Context::default());
        let raster = state.raster.clone().unwrap();
        let revision = state.revision;
        let texture = state.texture.as_ref().unwrap().id();
        state.copy_qr_with(|image| {
            assert_eq!(image.width, raster.width() as usize);
            assert_eq!(image.height, raster.height() as usize);
            assert_eq!(image.bytes.as_ref(), raster.as_raw());
            Ok(())
        });
        assert!(matches!(state.feedback, Some(Ok(_))));
        state.copy_qr_with(|_| Err(()));
        assert!(matches!(state.feedback, Some(Err(_))));
        assert_eq!(state.source, "exact source");
        assert_eq!(state.raster.as_ref(), Some(&raster));
        assert_eq!(state.revision, revision);
        assert_eq!(state.texture.as_ref().unwrap().id(), texture);
        assert!(state.open);
        state.set_source("x".repeat(10000));
        state.copy_qr_with(|_| panic!("invalid output must not call writer"));
    }

    #[test]
    fn qr_png_save_roundtrips_cached_raster_without_changing_confirmed_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("neutral.PNG");
        let mut state = QrDialogState::default();
        state.open(Some("private source"));
        state.refresh_texture(&egui::Context::default());
        let raster = state.raster.clone().unwrap();
        let texture = state.texture.as_ref().unwrap().id();
        let revision = state.revision;
        state.save_png_with(
            || Some(path.clone()),
            |chosen, image| {
                assert_eq!(chosen, path);
                image
                    .save_with_format(chosen, image::ImageFormat::Png)
                    .map_err(|_| ())
            },
        );
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(decoded, raster);
        assert_eq!(*decoded.get_pixel(0, 0), image::Rgba([255, 255, 255, 255]));
        assert!(matches!(state.feedback, Some(Ok(_))));
        assert_eq!(state.revision, revision);
        assert_eq!(state.texture.as_ref().unwrap().id(), texture);
        assert_eq!(state.source, "private source");
        assert!(state.open);
    }

    #[test]
    fn qr_png_cancel_invalid_paths_and_write_errors_have_no_unrequested_effects() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = QrDialogState::default();
        state.save_png_with(
            || panic!("empty must not choose"),
            |_, _| panic!("empty must not write"),
        );
        state.open(Some("source"));
        state.feedback = Some(Ok("prior status".into()));
        let feedback = state.feedback.clone();
        state.save_png_with(|| None, |_, _| panic!("cancel must not write"));
        assert_eq!(state.feedback, feedback);
        for filename in ["neutral", "neutral.jpg"] {
            let path = temp.path().join(filename);
            state.save_png_with(
                || Some(path.clone()),
                |_, _| panic!("invalid suffix must not write"),
            );
            assert!(!path.exists());
            assert!(!path.with_extension("png").exists());
            assert!(matches!(state.feedback, Some(Err(_))));
        }
        let raster = state.raster.clone();
        let revision = state.revision;
        state.save_png_with(|| Some(temp.path().join("neutral.png")), |_, _| Err(()));
        assert!(matches!(state.feedback, Some(Err(_))));
        assert_eq!(state.raster, raster);
        assert_eq!(state.source, "source");
        assert_eq!(state.revision, revision);
        assert!(state.open);
        state.set_source("x".repeat(10000));
        state.save_png_with(
            || panic!("invalid must not choose"),
            |_, _| panic!("invalid must not write"),
        );
    }

    #[test]
    fn qr_viewport_has_scannable_size_and_embedded_escape_cleans_up() {
        let builder = viewport_builder();
        assert_eq!(builder.inner_size, Some(egui::vec2(620.0, 700.0)));
        let ctx = egui::Context::default();
        let mut state = QrDialogState::default();
        state.open(Some("exact source"));
        let _ = ctx.run(egui::RawInput::default(), |ctx| state.show(ctx));
        assert!(!state.focus_source);
        state.feedback = Some(Ok("old".into()));
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
        state.feedback = Some(Ok("Old status".into()));
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
