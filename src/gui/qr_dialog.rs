use super::utility_host::UtilityPresentation;
use crate::clipboard_modify::clipboard::{ArboardClipboardBackend, ClipboardBackend};
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::qr::{self, GeneratedQr, QrErrorCorrection, QrGenerationError, QrPayloadMetadata};
use eframe::egui;

#[derive(Clone, Copy)]
pub(super) enum QrAction {
    Paste,
    CopyText,
    CopyQr,
    SavePng,
}
pub(super) struct QrActions {
    source: String,
    correction: QrErrorCorrection,
    advanced: bool,
    focus_source: bool,
    action: Option<QrAction>,
    pub(super) source_clipboard: Option<String>,
    editor_state: Option<egui::widgets::text_edit::TextEditState>,
    editor_undo: Option<egui::util::undoer::Undoer<(egui::text::CCursorRange, String)>>,
    cut: bool,
    pub(super) host_request: Option<UtilityPresentation>,
    pub(super) close: bool,
}

use image::RgbaImage;

pub(super) fn viewport_id(session: u64) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("qr-generator", session))
}

#[cfg(feature = "qa-framebuffer-capture")]
#[derive(Clone)]
pub(super) struct QrViewportObservation {
    pub child_viewport_id: egui::ViewportId,
    pub expected_viewport_id: egui::ViewportId,
    pub callback_class: egui::ViewportClass,
    pub callback_frame_nr: u64,
    pub callback_pass_nr: u64,
    pub callback_pass_index: usize,
    pub visible: Option<bool>,
    pub minimized: Option<bool>,
    pub occluded: Option<bool>,
    pub focused: Option<bool>,
    pub body_rendered: bool,
    pub still_open: bool,
    pub callback_observed_at_unix_ms: u64,
    pub completed_frame_nr: Option<u64>,
    pub completed_pass_nr: Option<u64>,
    pub renderer_returned_at_unix_ms: Option<u64>,
}

#[cfg(feature = "qa-framebuffer-capture")]
impl QrViewportObservation {
    fn is_immediate_target(&self) -> bool {
        self.callback_class == egui::ViewportClass::Immediate
            && self.child_viewport_id == self.expected_viewport_id
            && self.body_rendered
            && self.still_open
    }
}

#[cfg(feature = "qa-framebuffer-capture")]
fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

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
    epoch: u64,
    previous_editor: Option<egui::Id>,
    #[cfg(feature = "qa-framebuffer-capture")]
    observation: Option<QrViewportObservation>,
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
            epoch: 0,
            previous_editor: None,
            #[cfg(feature = "qa-framebuffer-capture")]
            observation: None,
        }
    }
}

impl QrDialogState {
    pub fn open(&mut self, initial_text: Option<&str>) {
        let epoch = self.epoch.wrapping_add(1);
        let previous_editor = self.previous_editor;
        *self = Self {
            epoch,
            previous_editor,
            open: true,
            source: initial_text.unwrap_or_default().to_owned(),
            focus_source: true,
            dirty: true,
            ..Self::default()
        };
        self.refresh_generation();
    }

    pub(super) fn ensure_open(&mut self) {
        if !self.open {
            self.epoch = self.epoch.wrapping_add(1);
            self.open = true;
            self.focus_source = true;
            self.refresh_generation();
        }
    }

    pub fn close(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
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

    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }

    fn editor_id(&self) -> egui::Id {
        egui::Id::new(("qr_source", self.epoch))
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(super) fn take_observation(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<QrViewportObservation> {
        let mut observation = self.observation.take()?;
        if observation.is_immediate_target() {
            observation.completed_frame_nr =
                Some(ctx.cumulative_frame_nr_for(observation.child_viewport_id));
            observation.completed_pass_nr =
                Some(ctx.cumulative_pass_nr_for(observation.child_viewport_id));
        }
        observation.renderer_returned_at_unix_ms = Some(unix_millis());
        Some(observation)
    }

    pub(super) fn render_host(
        &mut self,
        ctx: &egui::Context,
        owner: bool,
        first: bool,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> QrActions {
        let editor_id = self.editor_id();
        if let Some(previous) = self.previous_editor.replace(editor_id)
            && previous != editor_id
        {
            ctx.data_mut(|data| data.remove::<egui::widgets::text_edit::TextEditState>(previous));
        }
        let mut actions = QrActions {
            source: self.source.clone(),
            correction: self.error_correction,
            advanced: self.advanced_open,
            focus_source: self.focus_source,
            action: None,
            source_clipboard: None,
            editor_state: None,
            editor_undo: None,
            cut: false,
            host_request: None,
            close: false,
        };
        let original_editor_state =
            egui::widgets::text_edit::TextEditState::load(ctx, self.editor_id());
        let original_undo = original_editor_state.as_ref().map(|state| state.undoer());
        actions.cut = ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Cut))
        });
        let mut window_open = self.open;
        #[cfg(feature = "qa-framebuffer-capture")]
        if child.is_some() && ctx.viewport_id() == viewport_id(self.epoch) {
            let (visible, minimized, occluded, focused) = ctx.input(|input| {
                let viewport = input.viewport();
                (
                    viewport.visible(),
                    viewport.minimized,
                    viewport.occluded,
                    viewport.focused,
                )
            });
            self.observation = Some(QrViewportObservation {
                child_viewport_id: ctx.viewport_id(),
                expected_viewport_id: viewport_id(self.epoch),
                callback_class: egui::ViewportClass::Immediate,
                callback_frame_nr: ctx.cumulative_frame_nr(),
                callback_pass_nr: ctx.cumulative_pass_nr(),
                callback_pass_index: ctx.current_pass_index(),
                visible,
                minimized,
                occluded,
                focused,
                body_rendered: true,
                still_open: self.open,
                callback_observed_at_unix_ms: unix_millis(),
                completed_frame_nr: None,
                completed_pass_nr: None,
                renderer_returned_at_unix_ms: None,
            });
        }
        let mut body = |ui: &mut egui::Ui| {
            let active =
                owner && first && ui.is_visible() && !ui.is_sizing_pass() && !ctx.will_discard();
            let (button, request) =
                super::utility_host::presentation_button(ui, presentation, active);
            if button.clicked() {
                actions.host_request = Some(request);
            }
            ui.add_enabled_ui(active, |ui| self.body(ui, &mut actions));
        };
        if let Some(ui) = child {
            body(ui);
        } else {
            let available = (ctx.input(|input| input.content_rect().size())
                - egui::vec2(24.0, 48.0))
            .max(egui::vec2(1.0, 1.0));
            egui::Window::new("QR Generator")
                .id(egui::Id::new("qr_generator_embedded"))
                .collapsible(false)
                .default_size(available.min(egui::vec2(620.0, 700.0)))
                .min_size(available.min(egui::vec2(320.0, 420.0)))
                .max_size(available)
                .show_utility(
                    ctx,
                    "qr-generator",
                    egui::Id::new("qr_generator_embedded"),
                    Some(&mut window_open),
                    crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(
                            available.min(egui::vec2(620.0, 700.0)),
                        ),
                        egui::Vec2::ZERO,
                        [None, None],
                    )
                    .with_minimum(available.min(egui::vec2(320.0, 420.0)))
                    .with_maximum(available),
                    body,
                );
        }
        actions.editor_state = egui::widgets::text_edit::TextEditState::load(ctx, self.editor_id());
        actions.editor_undo = actions.editor_state.as_ref().map(|state| state.undoer());
        if let (Some(mut state), Some(undo)) = (original_editor_state, original_undo) {
            state.set_undoer(undo);
            state.store(ctx, self.editor_id());
        }
        if !window_open && first {
            actions.close = true;
        }
        actions
    }

    pub(super) fn commit(&mut self, ctx: &egui::Context, actions: QrActions) -> bool {
        self.commit_with_clipboard(ctx, actions, &ArboardClipboardBackend)
    }

    fn commit_with_clipboard(
        &mut self,
        ctx: &egui::Context,
        mut actions: QrActions,
        clipboard: &impl ClipboardBackend,
    ) -> bool {
        let changed = self.source != actions.source
            || self.error_correction != actions.correction
            || self.advanced_open != actions.advanced
            || self.focus_source != actions.focus_source
            || actions.action.is_some()
            || actions.source_clipboard.is_some();
        let clipboard_result = actions
            .source_clipboard
            .take()
            .map(|text| clipboard.write_text(&text));
        let clipboard_failed = clipboard_result.as_ref().is_some_and(Result::is_err);
        if clipboard_failed && actions.cut {
            actions.source = self.source.clone();
        }
        if self.source != actions.source || self.error_correction != actions.correction {
            self.source = actions.source;
            self.error_correction = actions.correction;
            self.dirty = true;
            self.refresh_generation();
        }
        if let Some(result) = clipboard_result {
            self.feedback = Some(
                result
                    .map(|()| "Copied text to clipboard".into())
                    .map_err(|_| "Could not copy text to clipboard. Try again.".into()),
            );
        }
        self.advanced_open = actions.advanced;
        self.focus_source = actions.focus_source;
        if !(clipboard_failed && actions.cut)
            && let (Some(mut state), Some(undo)) = (actions.editor_state, actions.editor_undo)
        {
            state.set_undoer(undo);
            state.store(ctx, self.editor_id());
        }
        if let Some(action) = actions.action {
            match action {
                QrAction::Paste => self.paste(&ArboardClipboardBackend),
                QrAction::CopyText => self.copy_text(&ArboardClipboardBackend),
                QrAction::CopyQr if self.raster.is_none() => {
                    self.feedback = Some(Err(
                        "Unable to generate QR. Edit the text or choose lower error correction."
                            .into(),
                    ))
                }
                QrAction::SavePng if self.raster.is_none() => {
                    self.feedback = Some(Err(
                        "Unable to generate QR. Edit the text or choose lower error correction."
                            .into(),
                    ))
                }
                QrAction::CopyQr => self.copy_qr_with(|image| {
                    arboard::Clipboard::new()
                        .and_then(|mut clipboard| clipboard.set_image(image))
                        .map_err(|_| ())
                }),
                QrAction::SavePng => self.save_png_with(
                    || {
                        rfd::FileDialog::new()
                            .add_filter("PNG image", &["png"])
                            .set_file_name("multi_launcher_qr.png")
                            .save_file()
                    },
                    |path, raster| {
                        raster
                            .save_with_format(path, image::ImageFormat::Png)
                            .map_err(|_| ())
                    },
                ),
            }
        }
        changed
    }

    fn export_candidate_enabled(&self, source: &str, correction: QrErrorCorrection) -> bool {
        let changed = source != self.source || correction != self.error_correction;
        (!changed && self.raster.is_some()) || (changed && !source.is_empty())
    }

    fn body(&mut self, ui: &mut egui::Ui, actions: &mut QrActions) {
        let mut close = false;
        egui::ScrollArea::vertical()
            .id_salt("qr_contents")
            .show(ui, |ui| {
                    ui.label("Text or URL");
                    let output_start = ui.ctx().output(|output| output.commands.len());
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut actions.source)
                            .id(self.editor_id())
                            .desired_width(f32::INFINITY)
                            .desired_rows(5)
                            .hint_text("Enter or paste text to generate a QR code."),
                    );
                    ui.ctx().output_mut(|output| {
                        let mut index = output_start;
                        while index < output.commands.len() {
                            if matches!(output.commands[index], egui::OutputCommand::CopyText(_)) {
                                if let egui::OutputCommand::CopyText(text) = output.commands.remove(index) { actions.source_clipboard = Some(text); }
                            } else { index += 1; }
                        }
                    });
                    if actions.focus_source && ui.is_enabled() { response.request_focus(); actions.focus_source = false; }
                    ui.horizontal(|ui| {
                        if ui.button("Paste").clicked() {
                            actions.action = Some(QrAction::Paste);
                        }
                        if ui.add_enabled(!actions.source.is_empty(), egui::Button::new("Copy Text")).clicked() {
                            actions.action = Some(QrAction::CopyText);
                        }
                    });
                    ui.label(format!(
                        "{} characters · {} UTF-8 bytes",
                        self.metadata.character_count, self.metadata.utf8_byte_count
                    ));
                    ui.separator();
                    let mut selected = actions.correction;
                    let advanced = egui::CollapsingHeader::new("Advanced")
                        .id_salt("qr_advanced")
                        .open(Some(actions.advanced))
                        .show(ui, |ui| {
                            ui.label("Higher error correction improves recovery from damage, but reduces payload capacity.");
                            ui.horizontal_wrapped(|ui| {
                                for level in [QrErrorCorrection::Low, QrErrorCorrection::Medium,
                                    QrErrorCorrection::Quartile, QrErrorCorrection::High] {
                                    ui.selectable_value(&mut selected, level, level.label());
                                }
                            });
                        });
                    if advanced.header_response.clicked() {
                        actions.advanced = !actions.advanced;
                    }
                    actions.correction = selected;
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
                        let valid = self.export_candidate_enabled(&actions.source, actions.correction);
                        if ui.add_enabled(valid, egui::Button::new("Copy QR")).clicked() {
                            actions.action = Some(QrAction::CopyQr);
                        }
                        if ui.add_enabled(valid, egui::Button::new("Save PNG")).clicked() {
                            actions.action = Some(QrAction::SavePng);
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
            actions.close = true;
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
    fn qr_shared_host_retains_escape_and_cleans_up() {
        use super::super::jump_mode::ui::NavigationOwner;
        use super::super::utility_window_host::{HostInput, QrHostCommand, QrWindowHost};
        let builder = viewport_builder();
        assert_eq!(builder.inner_size, Some(egui::vec2(620.0, 700.0)));
        let ctx = egui::Context::default();
        let mut state = QrDialogState::default();
        let mut host = QrWindowHost::default();
        state.open(Some("exact source"));
        let run = |state: &mut QrDialogState, host: &mut QrWindowHost, input: egui::RawInput| {
            ctx.run_ui(input, |ui| {
                host.show(
                    ui.ctx(),
                    state,
                    &true,
                    HostInput {
                        first: true,
                        root_visible: true,
                        root_input: true,
                        native_input: true,
                        root_owner: NavigationOwner::Blocked,
                        parent_owner: true,
                        root_jump_active: false,
                        close_shortcut: true,
                        allow_os_close: true,
                        confirmation_owner: NavigationOwner::Blocked,
                        parent_instance: 0,
                        root_scopes: (None, None),
                    },
                )
            })
            .drop_without_applying_deltas();
            if let Some(completion) = host.complete(&ctx, state) {
                for command in completion.commands {
                    match command {
                        QrHostCommand::Closed => state.close(),
                        QrHostCommand::Apply(actions) => {
                            state.commit(&ctx, actions);
                        }
                    }
                }
            }
        };
        for _ in 0..3 {
            run(&mut state, &mut host, egui::RawInput::default());
        }
        assert!(!state.focus_source);
        let old_editor = state.editor_id();
        ctx.memory_mut(|memory| memory.request_focus(old_editor));
        let mut editor_state =
            egui::widgets::text_edit::TextEditState::load(&ctx, old_editor).unwrap();
        editor_state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(state.source.chars().count()),
            )));
        editor_state.store(&ctx, old_editor);
        let mut edit = egui::RawInput::default();
        edit.events.push(egui::Event::Text(" edited".into()));
        run(&mut state, &mut host, edit);
        assert_eq!(state.source, "exact source edited");
        state.open(Some("fresh"));
        assert_ne!(state.editor_id(), old_editor);
        for _ in 0..3 {
            run(&mut state, &mut host, egui::RawInput::default());
        }
        assert!(egui::widgets::text_edit::TextEditState::load(&ctx, old_editor).is_none());
        ctx.memory_mut(|memory| memory.request_focus(state.editor_id()));
        let mut undo = egui::RawInput::default();
        undo.events.push(egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        });
        run(&mut state, &mut host, undo);
        assert_eq!(state.source, "fresh");
        state.feedback = Some(Ok("old".into()));
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        run(&mut state, &mut host, input);
        assert!(!state.open);
        assert!(state.feedback.is_none() && state.texture.is_none() && state.raster.is_none());
        assert_eq!(state.source, "fresh");
    }

    #[test]
    fn staged_qr_edit_preserves_bytes_and_regenerates_once_on_commit() {
        let mut state = QrDialogState::default();
        state.open(Some("old"));
        let old_revision = state.revision;
        let actions = QrActions {
            source: "  雪\r\n🔒 ".into(),
            correction: QrErrorCorrection::High,
            advanced: true,
            focus_source: false,
            action: None,
            source_clipboard: None,
            editor_state: None,
            editor_undo: None,
            cut: false,
            host_request: None,
            close: false,
        };
        assert_eq!(state.source, "old");
        state.commit(&egui::Context::default(), actions);
        assert_eq!(state.source, "  雪\r\n🔒 ");
        assert_eq!(state.error_correction, QrErrorCorrection::High);
        assert_eq!(state.revision, old_revision + 1);
        assert!(state.advanced_open);
    }

    #[test]
    fn export_requires_cached_raster_or_a_nonempty_changed_candidate() {
        let mut state = QrDialogState::default();
        state.open(Some(&"x".repeat(20_000)));
        assert!(state.raster.is_none());
        assert!(!state.export_candidate_enabled(&state.source, state.error_correction));
        assert!(state.export_candidate_enabled("new valid input", state.error_correction));
        assert!(!state.export_candidate_enabled("", state.error_correction));
    }

    #[test]
    fn failed_cut_preserves_source_and_reports_clipboard_error() {
        use std::sync::atomic::Ordering::SeqCst;
        let clipboard = FakeClipboard::default();
        clipboard.fail.store(true, SeqCst);
        let mut state = QrDialogState::default();
        state.open(Some("original"));
        let before = state.revision;
        let actions = QrActions {
            source: "inal".into(),
            correction: state.error_correction,
            advanced: false,
            focus_source: false,
            action: None,
            source_clipboard: Some("orig".into()),
            editor_state: None,
            editor_undo: None,
            cut: true,
            host_request: None,
            close: false,
        };
        state.commit_with_clipboard(&egui::Context::default(), actions, &clipboard);
        assert_eq!(state.source, "original");
        assert_eq!(state.revision, before);
        assert!(matches!(state.feedback, Some(Err(_))));
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

#[cfg(all(test, feature = "qa-framebuffer-capture"))]
mod qa_observation_tests {
    use super::*;

    #[test]
    fn observation_uses_the_actual_immediate_child_callback_identity() {
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        egui::Context::set_immediate_viewport_renderer(|parent_ctx, mut immediate| {
            assert_eq!(parent_ctx.viewport_id(), egui::ViewportId::ROOT);
            let child_id = immediate.ids.this;
            let mut raw_input = egui::RawInput {
                viewport_id: child_id,
                ..Default::default()
            };
            raw_input
                .viewports
                .insert(child_id, egui::ViewportInfo::default());
            let output = parent_ctx.run_ui(raw_input, |ui| {
                (immediate.viewport_ui_cb)(ui);
            });
            output.drop_without_applying_deltas();
        });

        let mut state = QrDialogState::default();
        state.open(Some("https://example.invalid/qr"));
        let mut observation = None;
        let mut host = super::super::utility_window_host::QrWindowHost::default();
        let output = ctx.run_ui(Default::default(), |ui| {
            use super::super::jump_mode::ui::NavigationOwner;
            use super::super::utility_window_host::HostInput;
            assert_eq!(ui.ctx().viewport_id(), egui::ViewportId::ROOT);
            host.prepare_capture_popout(ui.ctx(), &state);
            host.show(
                ui.ctx(),
                &mut state,
                &true,
                HostInput {
                    first: true,
                    root_visible: true,
                    root_input: true,
                    native_input: true,
                    root_owner: NavigationOwner::Blocked,
                    parent_owner: true,
                    root_jump_active: false,
                    close_shortcut: true,
                    allow_os_close: true,
                    confirmation_owner: NavigationOwner::Blocked,
                    parent_instance: 0,
                    root_scopes: (None, None),
                },
            );
            observation = state.take_observation(ui.ctx());
        });
        output.drop_without_applying_deltas();

        let observation = observation.expect("the immediate callback should be observed");
        assert_eq!(observation.child_viewport_id, viewport_id(state.epoch()));
        assert!(observation.callback_class == egui::ViewportClass::Immediate);
        assert!(observation.body_rendered);
        assert!(observation.still_open);
        assert_eq!(observation.visible, None);
        assert!(observation.completed_frame_nr.is_some());
        assert!(observation.completed_pass_nr.is_some());
        assert!(observation.renderer_returned_at_unix_ms.is_some());
    }
}
