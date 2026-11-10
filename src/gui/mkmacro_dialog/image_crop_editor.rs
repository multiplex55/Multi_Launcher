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
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
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
pub(in crate::gui) enum CropUiAction {
    SaveAs,
    RequestOverwrite,
    ConfirmOverwrite,
    Cancel,
    Retired,
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
    pub(super) epoch: u64,
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
    #[cfg(test)]
    test_clip: Option<egui::Rect>,
    #[cfg(test)]
    pub(super) test_status: Option<(&'static str, egui::Rect, egui::Rect)>,
}

impl Default for ImageCropEditorState {
    fn default() -> Self {
        Self {
            epoch: 0,
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
            #[cfg(test)]
            test_clip: None,
            #[cfg(test)]
            test_status: None,
        }
    }
}

impl ImageCropEditorState {
    pub(super) fn destination(&self) -> &ImageCropDestination { &self.destination }
    #[cfg(test)]
    pub(super) fn test_seed_loading(&mut self, destination: ImageCropDestination) {
        self.cancel();
        self.epoch = self.epoch.wrapping_add(1).max(1);
        self.open = true;
        self.loading = true;
        self.destination = destination;
    }
    #[cfg(test)]
    pub(super) fn test_set_error(&mut self) {
        self.loading = false;
        self.validation_message = Some("Fixture load failed".into());
    }
    #[cfg(test)]
    pub(super) fn test_set_ready(&mut self) {
        self.loading = false;
        self.validation_message = None;
        self.save_as_filename = "should-not-write.png".into();
        self.canvas = Some(CropCanvas::from_image(RgbaImage::new(4, 4)).unwrap());
    }
    #[cfg(test)]
    pub(super) fn test_save_button(&self) -> Option<(egui::Rect, egui::Rect, bool)> {
        let (rect, enabled) = self.canvas.as_ref()?.test_save_button()?;
        Some((rect, self.test_clip?, enabled))
    }
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
        self.epoch = self.epoch.wrapping_add(1).max(1);
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

    pub(super) fn poll_load(&mut self, ctx: &egui::Context) {
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

#[derive(Clone)]
pub(in crate::gui) struct CropCheckpoint {
    canvas: Option<crate::gui::image_crop_editor::CropCanvasCheckpoint>,
    save_as_filename: String,
    overwrite_confirmation: bool,
    collision_message: Option<String>,
    validation_message: Option<String>,
    persistence_message: Option<String>,
}
#[derive(Default)]
pub(in crate::gui) struct CropActions {
    presentation: Option<super::super::utility_host::UtilityPresentation>,
    intent: Option<CropUiAction>,
}

fn confirmation_id(epoch: u64) -> egui::Id {
    egui::Id::new(("mkmacro-crop-overwrite-confirmation", epoch))
}
fn render_contents(
    ui: &mut egui::Ui,
    editor: &mut ImageCropEditorState,
    confirmation_allowed: bool,
    result: &mut CropActions,
) {
    #[cfg(test)]
    {
        editor.test_clip = Some(ui.clip_rect());
    }
    if let Some(canvas) = editor.canvas.as_mut() {
        let mut options =
            CropEditorUiOptions::new("Crop Reference Image", "mkmacro_crop_reference_image");
        options.source_label = Some(format!(
            "Source: mkmacro_assets/{}",
            editor.destination.source().filename()
        ));
        options.collision_message = editor.collision_message.clone();
        options.persistence_message = editor.persistence_message.clone();
        options.allow_overwrite = true;
        result.intent = match canvas.show_contents(ui, &options, Some(&mut editor.save_as_filename))
        {
            Some(CropEditorAction::SaveAs) => Some(CropUiAction::SaveAs),
            Some(CropEditorAction::RequestOverwrite) => Some(CropUiAction::RequestOverwrite),
            Some(CropEditorAction::Cancel) => Some(CropUiAction::Cancel),
            None => None,
        };
    } else {
        if editor.loading {
            let response = ui.label("Loading reference image…");
            #[cfg(test)] { editor.test_status = Some(("loading", response.rect, ui.clip_rect())); }
            #[cfg(not(test))] let _ = response;
        }
        if let Some(error) = &editor.validation_message {
            let response = ui.colored_label(ui.visuals().error_fg_color, error);
            #[cfg(test)] { editor.test_status = Some(("error", response.rect, ui.clip_rect())); }
            #[cfg(not(test))] let _ = response;
        }
        if ui.button("Cancel").clicked() {
            result.intent = Some(CropUiAction::Cancel);
        }
    }
    if editor.overwrite_confirmation {
        let mut open = true;
        egui::Window::new("Confirm overwrite").id(confirmation_id(editor.epoch)).collapsible(false).resizable(false).open(&mut open).show(ui.ctx(), |ui| {
            ui.add_enabled_ui(confirmation_allowed, |ui| {
                let footer = overwrite_confirmation_footer_height(ui);
                let height = (ui.available_rect_before_wrap().intersect(ui.clip_rect()).height() - footer).max(0.0);
                let body = egui::ScrollArea::vertical().id_salt("mkmacro_crop_overwrite_confirmation_body").max_height(height).min_scrolled_height(0.0).show(ui, |ui| {
                    let copy = ui.add(egui::Label::new(format!("Overwrite {} with the selected crop?", editor.destination.source().filename())).wrap_mode(egui::TextWrapMode::Wrap));
                    let warning = semantic_status(ui, StatusTone::Warning, "All macros and actions referencing this file will see the cropped pixels.", None);
                    (copy, warning)
                });
                let (confirm, cancel) = ui.horizontal_wrapped(|ui| {
                    let confirm = ui.add(egui::Button::new("Confirm Overwrite").wrap());
                    let cancel = ui.add(egui::Button::new("Cancel").wrap());
                    if confirm.clicked() { result.intent = Some(CropUiAction::ConfirmOverwrite); }
                    if cancel.clicked() { editor.overwrite_confirmation = false; }
                    (confirm, cancel)
                }).inner;
                #[cfg(test)]
                { editor.overwrite_chrome = Some(OverwriteChromeObservation { body: (body.id, body.inner_rect, body.content_size, body.state.offset), copy: (body.inner.0.id, body.inner.0.rect), warning: (body.inner.1.id, body.inner.1.rect), confirm_button: (confirm.id, confirm.rect), cancel_button: (cancel.id, cancel.rect), footer_reserved_height: footer }); }
                #[cfg(not(test))]
                let _ = (body, confirm, cancel);
            });
        });
        if !open && confirmation_allowed {
            editor.overwrite_confirmation = false;
        }
    }
}

impl super::super::utility_window_host::HostedUtilityDialog for ImageCropEditorState {
    const KEY: &'static str = "mkmacro-reference-crop";
    const TITLE: &'static str = "Crop Reference Image";
    const SIZE: egui::Vec2 = egui::vec2(860.0, 680.0);
    const MINIMUM: egui::Vec2 = egui::vec2(320.0, 240.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = ();
    type Actions = CropActions;
    type Checkpoint = CropCheckpoint;
    type Command = CropUiAction;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, egui::Id::new(Self::KEY))
    }
    fn navigation_owner(_: u64) -> super::super::jump_mode::ui::NavigationOwner {
        super::super::jump_mode::ui::NavigationOwner::Blocked
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.epoch
    }
    fn parent_targets_allowed(&self, _: &()) -> bool {
        false
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: super::super::jump_mode::ui::NavigationViewportBinding,
        ctx: &egui::Context,
        popup: Option<egui::LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|i| i.focused)
            && ctx.memory(|m| m.top_modal_layer().is_none())
            && super::super::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|layer| layer == binding.base_layer)
            && super::super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|layer| {
                layer == binding.base_layer
                    || Some(layer) == popup
                    || layer == egui::LayerId::background()
            })
    }
    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        viewport: egui::ViewportId,
        instance: u64,
    ) -> Option<super::super::jump_mode::ui::NavigationViewportBinding> {
        self.overwrite_confirmation.then_some(
            super::super::jump_mode::ui::NavigationViewportBinding {
                viewport,
                base_layer: egui::LayerId::new(egui::Order::Middle, confirmation_id(instance)),
                owner: super::super::jump_mode::ui::NavigationOwner::MkMacro { instance },
            },
        )
    }
    fn capture(&self) -> CropCheckpoint {
        CropCheckpoint {
            canvas: self.canvas.as_ref().map(|canvas| canvas.capture_ui()),
            save_as_filename: self.save_as_filename.clone(),
            overwrite_confirmation: self.overwrite_confirmation,
            collision_message: self.collision_message.clone(),
            validation_message: self.validation_message.clone(),
            persistence_message: self.persistence_message.clone(),
        }
    }
    fn restore(&mut self, checkpoint: CropCheckpoint) {
        if let (Some(canvas), Some(saved)) = (&mut self.canvas, checkpoint.canvas) {
            canvas.restore_ui(saved);
        }
        self.save_as_filename = checkpoint.save_as_filename;
        self.overwrite_confirmation = checkpoint.overwrite_confirmation;
        self.collision_message = checkpoint.collision_message;
        self.validation_message = checkpoint.validation_message;
        self.persistence_message = checkpoint.persistence_message;
    }
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (
            Option<&super::super::jump_mode::ui::JumpModeScope>,
            Option<&super::super::jump_mode::ui::JumpModeScope>,
        ),
        _: super::super::jump_mode::ui::NavigationOwner,
        allowed: bool,
        child_owner: super::super::jump_mode::ui::NavigationOwner,
        first: bool,
        _: u64,
        _: &(),
        child: Option<&mut egui::Ui>,
        presentation: super::super::utility_host::UtilityPresentation,
    ) -> CropActions {
        let mut result = CropActions::default();
        let mut open = true;
        let mut body = |ui: &mut egui::Ui| {
            let (button, next) =
                super::super::utility_host::presentation_button(ui, presentation, allowed);
            if first && !ctx.will_discard() && button.clicked() {
                result.presentation = Some(next);
            }
            let confirm_allowed =
                allowed || child_owner != super::super::jump_mode::ui::NavigationOwner::Blocked;
            ui.add_enabled_ui(allowed, |ui| {
                render_contents(ui, self, confirm_allowed, &mut result)
            });
        };
        if let Some(ui) = child {
            body(ui);
        } else {
            let id = egui::Id::new(Self::KEY);
            egui::Window::new(Self::TITLE)
                .id(id)
                .resizable(true)
                .show_utility(
                    ctx,
                    Self::KEY,
                    id,
                    Some(&mut open),
                    crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::outer(Self::SIZE),
                    body,
                );
        }
        if !open {
            result.intent = Some(CropUiAction::Cancel);
        }
        if !first || ctx.will_discard() {
            result.presentation = None;
            result.intent = None;
        }
        result
    }
    fn host_request(
        actions: &CropActions,
    ) -> Option<super::super::utility_host::UtilityPresentation> {
        actions.presentation
    }
    fn closed_command() -> CropUiAction {
        CropUiAction::Retired
    }
    fn command_closes(_: &CropUiAction) -> bool {
        false
    }
    fn complete_actions(&mut self, actions: CropActions, _: Option<usize>) -> Vec<CropUiAction> {
        actions.intent.into_iter().collect()
    }
    #[cfg(test)]
    fn test_button(_: &CropActions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}

#[cfg(test)]
pub fn show(ctx: &egui::Context, dialog: &mut MkMacroDialog) {
    let mut editor = std::mem::take(&mut dialog.image_crop_editor);
    if !editor.open {
        dialog.image_crop_editor = editor;
        return;
    }
    editor.poll_load(ctx);
    let mut result = CropActions::default();
    let mut open = true;
    let id = egui::Id::new("mkmacro-reference-crop");
    egui::Window::new("Crop Reference Image")
        .id(id)
        .show_utility(
            ctx,
            "mkmacro-reference-crop",
            id,
            Some(&mut open),
            crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::outer(egui::vec2(
                860.0, 680.0,
            )),
            |ui| render_contents(ui, &mut editor, true, &mut result),
        );
    if !open {
        result.intent = Some(CropUiAction::Cancel);
    }
    if let Some(intent) = result.intent {
        apply_action(ctx, dialog, &mut editor, intent);
    }
    dialog.image_crop_editor = editor;
}

pub(super) fn apply_action(
    ctx: &egui::Context,
    dialog: &mut MkMacroDialog,
    editor: &mut ImageCropEditorState,
    action: CropUiAction,
) {
    match action {
        CropUiAction::SaveAs => editor.save_as(dialog),
        CropUiAction::ConfirmOverwrite => {
            editor.overwrite_confirmation = false;
            editor.overwrite_current(dialog);
        }
        CropUiAction::RequestOverwrite => editor.overwrite_confirmation = true,
        CropUiAction::Cancel | CropUiAction::Retired => {
            editor.complete(ImageCropResult::Cancelled, true)
        }
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
                ))
            }
            ImageCropResult::Error(error) => editor.persistence_message = Some(error.clone()),
        }
    }
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
