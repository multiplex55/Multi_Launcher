//! MkMacro adapter for the reusable crop canvas.
//!
//! This layer retains managed-PNG authoring, draft identity checks, collision
//! reporting, and overwrite confirmation. The canvas itself has no knowledge
//! of those domain concerns.

pub use super::image_authoring_destination::ImageCropDestination;
use super::{MkMacroDialog, image_authoring::normalize_image_filename};
use crate::gui::image_crop_editor::{
    CropEditorAction, CropEditorUiOptions, ImageCropEditorState as CropCanvas,
};
pub use crate::gui::image_crop_editor::{
    CropFloatRect, CropHandle, CropPoint, CropTransform, MAX_CROP_ZOOM, MIN_CROP_ZOOM, clamp_rect,
    display_to_source, move_rect, normalize_drag, resize_rect, selection_from_drag,
    source_to_display,
};
use crate::gui::theme::widgets::{StatusTone, semantic_status};
use crate::mkmacro::{
    ImageAssetAuthoringService, ImageImportChoice, ImageImportResult, MkImageRef, MkMacroStore,
};
use eframe::egui;
use image::RgbaImage;
use std::sync::{Arc, mpsc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageCropResult {
    Cancelled,
    Overwritten,
    SavedAs(MkImageRef),
    Collision(MkImageRef),
    Error(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageCropCompletion {
    pub destination: ImageCropDestination,
    pub result: ImageCropResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CropUiAction {
    SaveAs,
    RequestOverwrite,
    ConfirmOverwrite,
    Cancel,
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct OverwriteChromeObservation {
    body: (egui::Id, egui::Rect, egui::Vec2, egui::Vec2),
    copy: (egui::Id, egui::Rect),
    warning: (egui::Id, egui::Rect),
    confirm_button: (egui::Id, egui::Rect),
    cancel_button: (egui::Id, egui::Rect),
    footer_reserved_height: f32,
}

pub struct ImageCropEditorState {
    open: bool,
    destination: ImageCropDestination,
    loading: bool,
    worker: Option<mpsc::Receiver<Result<RgbaImage, String>>>,
    canvas: Option<CropCanvas>,
    save_as_filename: String,
    overwrite_confirmation: bool,
    collision_message: Option<String>,
    validation_message: Option<String>,
    persistence_message: Option<String>,
    completion: Option<ImageCropCompletion>,
    #[cfg(test)]
    overwrite_chrome: Option<OverwriteChromeObservation>,
}

impl Default for ImageCropEditorState {
    fn default() -> Self {
        Self {
            open: false,
            destination: ImageCropDestination::ImageActionReference {
                macro_id: 0,
                step_id: None,
                draft_generation: 0,
                source: MkImageRef::default(),
            },
            loading: false,
            worker: None,
            canvas: None,
            save_as_filename: String::new(),
            overwrite_confirmation: false,
            collision_message: None,
            validation_message: None,
            persistence_message: None,
            completion: None,
            #[cfg(test)]
            overwrite_chrome: None,
        }
    }
}

impl ImageCropEditorState {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(
        &mut self,
        store: Arc<MkMacroStore>,
        destination: ImageCropDestination,
    ) -> Result<(), &'static str> {
        if self.open {
            return Err("An image crop is already in progress");
        }
        self.cancel();
        let source = destination.source().clone();
        self.open = true;
        self.destination = destination;
        self.loading = true;
        self.save_as_filename = suggested_crop_filename(&source);
        self.collision_message = None;
        self.validation_message = None;
        self.persistence_message = None;
        self.canvas = None;
        let (sender, receiver) = mpsc::channel();
        self.worker = Some(receiver);
        std::thread::spawn(move || {
            let result = store
                .validate_image_ref(&source)
                .map_err(|error| format!("{error:#}"));
            let _ = sender.send(result);
        });
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.open = false;
        self.loading = false;
        self.worker = None;
        if let Some(canvas) = &mut self.canvas {
            canvas.cancel();
        }
        self.canvas = None;
        self.overwrite_confirmation = false;
        self.completion = None;
    }

    pub fn take_completion(&mut self) -> Option<ImageCropCompletion> {
        self.completion.take()
    }

    fn complete(&mut self, result: ImageCropResult, close: bool) {
        self.completion = Some(ImageCropCompletion {
            destination: self.destination.clone(),
            result,
        });
        if close {
            self.open = false;
            self.loading = false;
            self.worker = None;
            if let Some(canvas) = &mut self.canvas {
                canvas.cancel();
            }
            self.canvas = None;
            self.overwrite_confirmation = false;
        }
    }

    fn poll_load(&mut self, ctx: &egui::Context) {
        let Some(worker) = &self.worker else {
            return;
        };
        let result = match worker.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint();
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Reference image loader stopped unexpectedly".into())
            }
        };
        self.worker = None;
        self.loading = false;
        match result {
            Ok(image) => match CropCanvas::from_image(image) {
                Ok(canvas) => {
                    self.canvas = Some(canvas);
                    self.validation_message = None;
                }
                Err(error) => {
                    self.canvas = None;
                    self.validation_message =
                        Some(format!("Reference image validation failed: {error}"));
                }
            },
            Err(error) => {
                self.canvas = None;
                self.validation_message =
                    Some(format!("Reference image validation failed: {error}"));
            }
        }
    }

    fn render(&mut self, ctx: &egui::Context) -> Option<CropUiAction> {
        self.poll_load(ctx);
        if !self.open {
            return None;
        }
        let Some(canvas) = self.canvas.as_mut() else {
            if self.loading {
                egui::Window::new("Crop Reference Image")
                    .id(egui::Id::new("mkmacro_crop_reference_image"))
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.label("Loading reference image…");
                    });
            } else if let Some(error) = &self.validation_message {
                let mut open = true;
                egui::Window::new("Crop Reference Image")
                    .id(egui::Id::new("mkmacro_crop_reference_image"))
                    .collapsible(false)
                    .open(&mut open)
                    .show(ctx, |ui| {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                        if ui.button("Cancel").clicked() {
                            self.open = false;
                        }
                    });
                if !open {
                    self.open = false;
                }
            }
            return None;
        };
        let mut options =
            CropEditorUiOptions::new("Crop Reference Image", "mkmacro_crop_reference_image");
        options.source_label = Some(format!(
            "Source: mkmacro_assets/{}",
            self.destination.source().filename()
        ));
        options.collision_message = self.collision_message.clone();
        options.persistence_message = self.persistence_message.clone();
        options.allow_overwrite = true;
        match canvas.show_with_save_as_filename(ctx, &options, Some(&mut self.save_as_filename)) {
            Some(CropEditorAction::SaveAs) => Some(CropUiAction::SaveAs),
            Some(CropEditorAction::RequestOverwrite) => Some(CropUiAction::RequestOverwrite),
            Some(CropEditorAction::Cancel) => Some(CropUiAction::Cancel),
            None => None,
        }
    }

    fn cropped_image(&self) -> Result<RgbaImage, String> {
        self.canvas
            .as_ref()
            .ok_or_else(|| "Reference image is not loaded".to_owned())?
            .crop_result()
            .map_err(|error| error.to_string())
    }

    fn save_as(&mut self, dialog: &mut MkMacroDialog) {
        self.collision_message = None;
        self.persistence_message = None;
        let image = match normalize_image_filename(&self.save_as_filename) {
            Ok(image) => image,
            Err(error) => {
                self.complete(ImageCropResult::Error(error), false);
                return;
            }
        };
        self.save_as_filename = image.filename().to_owned();
        let cropped = match self.cropped_image() {
            Ok(image) => image,
            Err(error) => {
                self.complete(ImageCropResult::Error(error), false);
                return;
            }
        };
        let result = ImageAssetAuthoringService::new(&dialog.store).stage_rgba(
            &cropped,
            image.clone(),
            ImageImportChoice::SaveAs(image),
        );
        match result {
            Ok(ImageImportResult::Imported(image)) => {
                self.complete(ImageCropResult::SavedAs(image), true)
            }
            Ok(ImageImportResult::Collision { image }) => {
                self.complete(ImageCropResult::Collision(image), false)
            }
            Ok(ImageImportResult::Cancelled) => self.complete(ImageCropResult::Cancelled, true),
            Err(error) => self.complete(
                ImageCropResult::Error(format!("Reference image: {error:#}")),
                false,
            ),
        }
    }

    fn overwrite_current(&mut self, dialog: &mut MkMacroDialog) {
        self.persistence_message = None;
        let cropped = match self.cropped_image() {
            Ok(image) => image,
            Err(error) => {
                self.complete(ImageCropResult::Error(error), false);
                return;
            }
        };
        let result = ImageAssetAuthoringService::new(&dialog.store).stage_rgba(
            &cropped,
            self.destination.source().clone(),
            ImageImportChoice::ReplaceExisting,
        );
        match result {
            Ok(ImageImportResult::Imported(_)) => self.complete(ImageCropResult::Overwritten, true),
            Ok(ImageImportResult::Collision { image }) => {
                self.complete(ImageCropResult::Collision(image), false)
            }
            Ok(ImageImportResult::Cancelled) => self.complete(ImageCropResult::Cancelled, true),
            Err(error) => self.complete(
                ImageCropResult::Error(format!("Reference image: {error:#}")),
                false,
            ),
        }
    }
}

fn suggested_crop_filename(source: &MkImageRef) -> String {
    let filename = source.filename();
    let stem = filename
        .len()
        .checked_sub(4)
        .filter(|index| filename[*index..].eq_ignore_ascii_case(".png"))
        .map(|index| &filename[..index])
        .unwrap_or(filename);
    if stem.is_empty() {
        "cropped.png".into()
    } else {
        normalize_image_filename(&format!("{stem}_cropped"))
            .map(|image| image.filename().to_owned())
            .unwrap_or_else(|_| "cropped.png".into())
    }
}

fn confirmation_button_row_height(ui: &egui::Ui, labels: &[&str]) -> f32 {
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
    let mut row_width = 0.0;
    let mut row_height = 0.0_f32;
    let mut total_height = 0.0;
    for label in labels {
        let text_size = egui::WidgetText::from(*label)
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Wrap),
                text_width,
                button_font.clone(),
            )
            .size();
        let button_width = (text_size.x + frame_margin.x)
            .max(spacing.interact_size.x)
            .min(available_width);
        let button_height = (text_size.y + spacing.extra_text_line_spacing + frame_margin.y)
            .max(spacing.interact_size.y);
        if row_width > 0.0 && row_width + spacing.item_spacing.x + button_width > available_width {
            total_height += row_height + spacing.item_spacing.y;
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
    total_height += row_height.max(spacing.interact_size.y);
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    (total_height * pixels_per_point).ceil() / pixels_per_point
}

fn overwrite_confirmation_footer_height(ui: &egui::Ui) -> f32 {
    let height = confirmation_button_row_height(ui, &["Confirm Overwrite", "Cancel"])
        + ui.spacing().item_spacing.y;
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    (height * pixels_per_point).ceil() / pixels_per_point
}

pub fn show(ctx: &egui::Context, dialog: &mut MkMacroDialog) {
    let mut editor = std::mem::take(&mut dialog.image_crop_editor);
    if !editor.open {
        dialog.image_crop_editor = editor;
        return;
    }
    let action = editor.render(ctx);
    match action {
        Some(CropUiAction::SaveAs) => editor.save_as(dialog),
        Some(CropUiAction::ConfirmOverwrite) => editor.overwrite_current(dialog),
        Some(CropUiAction::RequestOverwrite) => editor.overwrite_confirmation = true,
        Some(CropUiAction::Cancel) => editor.complete(ImageCropResult::Cancelled, true),
        None => {}
    }
    let mut confirm_overwrite = false;
    if editor.overwrite_confirmation {
        let mut open = true;
        egui::Window::new("Confirm overwrite")
            .id(egui::Id::new("mkmacro_crop_overwrite_confirmation"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                let footer_reserved_height = overwrite_confirmation_footer_height(ui);
                let body_max_height =
                    (ui.available_height() - footer_reserved_height).max(0.0);
                let copy_text = format!(
                    "Overwrite {} with the selected crop?",
                    editor.destination.source().filename()
                );
                let mut copy_response = None;
                let mut warning_response = None;
                let body = egui::ScrollArea::vertical()
                    .id_salt("mkmacro_crop_overwrite_confirmation_body")
                    .max_height(body_max_height)
                    .min_scrolled_height(0.0)
                    .show(ui, |ui| {
                        copy_response = Some(
                            ui.add(
                                egui::Label::new(&copy_text)
                                    .wrap_mode(egui::TextWrapMode::Wrap),
                            ),
                        );
                        warning_response = Some(semantic_status(
                            ui,
                            StatusTone::Warning,
                            "All macros and actions referencing this file will see the cropped pixels.",
                            None,
                        ));
                    });
                let (confirm_response, cancel_response) = ui
                    .horizontal_wrapped(|ui| {
                        let confirm = ui.add(egui::Button::new("Confirm Overwrite").wrap());
                        let cancel = ui.add(egui::Button::new("Cancel").wrap());
                        if confirm.clicked() {
                            editor.overwrite_confirmation = false;
                            confirm_overwrite = true;
                        }
                        if cancel.clicked() {
                            editor.overwrite_confirmation = false;
                        }
                        (confirm, cancel)
                    })
                    .inner;
                #[cfg(test)]
                {
                    editor.overwrite_chrome = Some(OverwriteChromeObservation {
                        body: (
                            body.id,
                            body.inner_rect,
                            body.content_size,
                            body.state.offset,
                        ),
                        copy: copy_response
                            .map(|response| (response.id, response.rect))
                            .unwrap_or((egui::Id::NULL, egui::Rect::NOTHING)),
                        warning: warning_response
                            .map(|response| (response.id, response.rect))
                            .unwrap_or((egui::Id::NULL, egui::Rect::NOTHING)),
                        confirm_button: (confirm_response.id, confirm_response.rect),
                        cancel_button: (cancel_response.id, cancel_response.rect),
                        footer_reserved_height,
                    });
                }
                #[cfg(not(test))]
                let _ = (
                    body,
                    copy_response,
                    warning_response,
                    confirm_response,
                    cancel_response,
                );
            });
        if !open {
            editor.overwrite_confirmation = false;
        }
    }
    if confirm_overwrite {
        editor.overwrite_current(dialog);
    }
    if let Some(completion) = editor.take_completion() {
        match &completion.result {
            ImageCropResult::Cancelled => {}
            ImageCropResult::Overwritten => {
                super::image_preview::invalidate(
                    ctx,
                    &dialog.store,
                    completion.destination.source(),
                );
                ctx.request_repaint();
            }
            ImageCropResult::SavedAs(image) => {
                if !dialog
                    .action_editor
                    .apply_crop_completion(&completion, dialog.selected_macro_id)
                {
                    dialog.action_editor.capture_message = Some(format!(
                        "The asset was saved, but the original image editor target changed: {}",
                        image.filename()
                    ));
                }
            }
            ImageCropResult::Collision(image) => {
                editor.collision_message = Some(format!(
                    "Reference image filename already exists: {}",
                    image.filename()
                ));
            }
            ImageCropResult::Error(error) => {
                editor.persistence_message = Some(error.clone());
            }
        }
    }
    dialog.image_crop_editor = editor;
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn same_rect(left: egui::Rect, right: egui::Rect) -> bool {
        (left.min.x - right.min.x).abs() < 0.5
            && (left.min.y - right.min.y).abs() < 0.5
            && (left.max.x - right.max.x).abs() < 0.5
            && (left.max.y - right.max.y).abs() < 0.5
    }

    #[test]
    fn managed_png_suggested_names_preserve_existing_behavior() {
        assert_eq!(
            suggested_crop_filename(&MkImageRef::from_filename("login_button.png")),
            "login_button_cropped.png"
        );
        assert_eq!(
            suggested_crop_filename(&MkImageRef::from_filename(".png")),
            "cropped.png"
        );
        assert_eq!(
            suggested_crop_filename(&MkImageRef::from_filename("name.with.dots.PNG")),
            "name.with.dots_cropped.png"
        );
    }

    #[test]
    fn overwrite_confirmation_bounds_long_copy_above_actions_without_completing() {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let mut dialog = MkMacroDialog::new(Arc::new(store));
        let filename = format!(
            "{}reference-image.png",
            "long-crop-target-name-with-context-".repeat(8)
        );
        let source = MkImageRef::from_filename(&filename);
        let destination = ImageCropDestination::ImageActionReference {
            macro_id: 7,
            step_id: Some(101),
            draft_generation: 3,
            source: source.clone(),
        };
        let mut editor = ImageCropEditorState::default();
        editor.open = true;
        editor.loading = true;
        editor.destination = destination.clone();
        editor.overwrite_confirmation = true;
        dialog.image_crop_editor = editor;

        let copy_text = format!("Overwrite {} with the selected crop?", source.filename());
        let warning_text =
            "All macros and actions referencing this file will see the cropped pixels.";
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(520.0, 340.0));
        let input = || egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        ctx.style_mut_of(ctx.theme(), |style| {
            style.override_font_id = Some(egui::FontId::new(22.0, egui::FontFamily::Proportional));
        });

        let mut previous_body = None;
        let mut latest = None;
        let mut stable_body = false;
        let mut copy_unelided = false;
        let mut warning_visible_at_bottom = false;
        let mut bottom_offset_observed = false;
        for _ in 0..5 {
            let output = ctx.run_ui(input(), |ctx| show(ctx, &mut dialog));
            let observation = dialog.image_crop_editor.overwrite_chrome;
            let mut text_shapes = Vec::new();
            for clipped_shape in &output.shapes {
                collect_text_shapes(
                    &clipped_shape.shape,
                    clipped_shape.clip_rect,
                    &mut text_shapes,
                );
            }
            output.drop_without_applying_deltas();

            if let Some(observation) = observation {
                let (scroll_id, body_rect, content_size, offset) = observation.body;
                stable_body = previous_body.is_some_and(|previous| same_rect(previous, body_rect));
                previous_body = Some(body_rect);

                copy_unelided |= text_shapes.iter().any(|(text, bounds, clip, elided)| {
                    text == &copy_text
                        && !elided
                        && bounds.is_positive()
                        && bounds.left() >= body_rect.left() - 0.5
                        && bounds.right() <= body_rect.right() + 0.5
                        && clip.left() <= body_rect.left() + 0.5
                        && clip.right() >= body_rect.right() - 0.5
                });
                warning_visible_at_bottom =
                    text_shapes.iter().any(|(text, bounds, clip, elided)| {
                        text == warning_text
                            && !elided
                            && body_rect.contains_rect(*bounds)
                            && clip.contains_rect(*bounds)
                    });
                let max_offset = (content_size.y - body_rect.height()).max(0.0);
                bottom_offset_observed |= (offset.y - max_offset).abs() < 0.75;
                if max_offset > 0.0
                    && let Some(mut scroll) = egui::scroll_area::State::load(&ctx, scroll_id)
                {
                    scroll.offset.y = max_offset;
                    scroll.store(&ctx, scroll_id);
                }
                latest = Some(observation);
            }
        }

        let observation = latest.expect("the overwrite confirmation produces native geometry");
        let (_, body_rect, content_size, offset) = observation.body;
        let max_offset = (content_size.y - body_rect.height()).max(0.0);
        let footer_top = observation
            .confirm_button
            .1
            .union(observation.cancel_button.1)
            .top();
        assert!(stable_body, "the overwrite body viewport settles");
        assert!(
            content_size.y > body_rect.height(),
            "the long copy has a real scroll range"
        );
        assert!(bottom_offset_observed && (offset.y - max_offset).abs() < 0.75);
        assert!(
            copy_unelided,
            "the full destination copy is rendered without elision"
        );
        assert!(
            warning_visible_at_bottom,
            "the complete warning is reachable inside the settled body viewport"
        );
        assert!(
            body_rect.bottom() <= footer_top,
            "confirmation body {body_rect:?} ends before the native actions at {footer_top}"
        );
        assert!(
            screen_rect.contains_rect(body_rect),
            "confirmation body {body_rect:?} fits the actual screen {screen_rect:?}"
        );
        for (name, (id, bounds)) in [
            ("destination copy", observation.copy),
            ("warning", observation.warning),
            ("Confirm Overwrite", observation.confirm_button),
            ("Cancel", observation.cancel_button),
        ] {
            assert_ne!(id, egui::Id::NULL, "{name} has an actual native response");
            assert!(bounds.is_positive(), "{name} has actual bounds {bounds:?}");
        }
        assert!(
            body_rect.contains_rect(observation.warning.1),
            "the warning response {0:?} remains inside the body {body_rect:?}",
            observation.warning.1
        );
        for (name, bounds) in [
            ("Confirm Overwrite", observation.confirm_button.1),
            ("Cancel", observation.cancel_button.1),
        ] {
            assert!(
                bounds.is_positive() && screen_rect.contains_rect(bounds),
                "{name} bounds {bounds:?} fit the actual screen {screen_rect:?}"
            );
        }
        assert!(
            observation.footer_reserved_height
                >= observation
                    .confirm_button
                    .1
                    .union(observation.cancel_button.1)
                    .height(),
            "the native footer is covered by its reservation"
        );
        assert!(dialog.image_crop_editor.is_open());
        assert!(dialog.image_crop_editor.overwrite_confirmation);
        assert_eq!(dialog.image_crop_editor.destination, destination);
        assert!(dialog.image_crop_editor.completion.is_none());
    }
}
