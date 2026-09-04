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
                ui.label(format!(
                    "Overwrite {} with the selected crop?",
                    editor.destination.source().filename()
                ));
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "All macros and actions referencing this file will see the cropped pixels.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Confirm Overwrite").clicked() {
                        editor.overwrite_confirmation = false;
                        confirm_overwrite = true;
                    }
                    if ui.button("Cancel").clicked() {
                        editor.overwrite_confirmation = false;
                    }
                });
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
}
