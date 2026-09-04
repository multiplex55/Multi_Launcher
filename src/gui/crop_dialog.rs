//! Standalone crop dialog state and image persistence.
//!
//! The persistence boundary is deliberately explicit about the destination
//! encoder and about overwrite confirmation. A missing or unknown extension
//! is an error; the dialog never guesses a format from image contents.

use crate::gui::image_crop_editor::{
    CropEditorAction, CropEditorUiOptions, ImageCropEditorState as CropCanvas,
};
use eframe::egui;
use image::{DynamicImage, ImageFormat, RgbImage, Rgba, RgbaImage};
use rfd::FileDialog;
use std::io::Cursor;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CropSourceContext {
    ExistingFile { source_path: PathBuf },
    Screenshot { default_directory: PathBuf },
}

impl CropSourceContext {
    pub fn source_path(&self) -> Option<&Path> {
        match self {
            Self::ExistingFile { source_path } => Some(source_path),
            Self::Screenshot { .. } => None,
        }
    }

    pub fn default_directory(&self) -> PathBuf {
        match self {
            Self::ExistingFile { source_path } => source_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            Self::Screenshot { default_directory } => default_directory.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CropEncoder {
    Png,
    Jpeg,
    Bmp,
}

pub fn select_encoder(path: &Path) -> Result<CropEncoder, String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or_else(|| format!("Image destination has no extension: {}", path.display()))?;
    match extension.to_ascii_lowercase().as_str() {
        "png" => Ok(CropEncoder::Png),
        "jpg" | "jpeg" => Ok(CropEncoder::Jpeg),
        "bmp" => Ok(CropEncoder::Bmp),
        other => Err(format!(
            "Unsupported image extension .{other}; use .png, .jpg, .jpeg, or .bmp"
        )),
    }
}

pub fn encoder_for_path(path: &Path) -> Result<CropEncoder, String> {
    select_encoder(path)
}

/// Generate `original_name_cropped.ext` beside an existing source file.
pub fn cropped_destination_path(source_path: &Path) -> Result<PathBuf, String> {
    let encoder = select_encoder(source_path)?;
    let file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "Image source has no usable filename: {}",
                source_path.display()
            )
        })?;
    let extension = source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or_else(|| format!("Image source has no extension: {}", source_path.display()))?;
    let stem_len = file_name
        .len()
        .checked_sub(extension.len() + 1)
        .ok_or_else(|| {
            format!(
                "Image source has no usable filename: {}",
                source_path.display()
            )
        })?;
    let stem = &file_name[..stem_len];
    let stem = if stem.is_empty() { "cropped" } else { stem };
    let extension = match encoder {
        CropEncoder::Png => extension,
        CropEncoder::Jpeg => extension,
        CropEncoder::Bmp => extension,
    };
    Ok(source_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(format!("{stem}_cropped.{extension}")))
}

pub fn cropped_path(source_path: &Path) -> Result<PathBuf, String> {
    cropped_destination_path(source_path)
}

pub fn screenshot_filename() -> String {
    format!(
        "multi_launcher_{}.png",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    )
}

pub fn screenshot_destination_path(default_directory: &Path) -> PathBuf {
    default_directory.join(screenshot_filename())
}

pub fn destination_exists(path: &Path) -> bool {
    path.exists()
}

/// Decode a supported image after validating the path extension explicitly.
pub fn decode_rgba(path: &Path) -> Result<RgbaImage, String> {
    select_encoder(path)?;
    image::open(path)
        .map(|image| image.to_rgba8())
        .map_err(|error| format!("Failed to decode {}: {error}", path.display()))
}

/// Encode RGBA pixels using the explicitly selected destination format.
pub fn encode_rgba(image: &RgbaImage, encoder: CropEncoder) -> Result<Vec<u8>, String> {
    let mut output = Cursor::new(Vec::new());
    let dynamic = match encoder {
        CropEncoder::Png => DynamicImage::ImageRgba8(image.clone()),
        CropEncoder::Bmp => DynamicImage::ImageRgba8(image.clone()),
        CropEncoder::Jpeg => {
            let rgb = RgbImage::from_fn(image.width(), image.height(), |x, y| {
                let Rgba([r, g, b, _]) = *image.get_pixel(x, y);
                image::Rgb([r, g, b])
            });
            DynamicImage::ImageRgb8(rgb)
        }
    };
    let format = match encoder {
        CropEncoder::Png => ImageFormat::Png,
        CropEncoder::Jpeg => ImageFormat::Jpeg,
        CropEncoder::Bmp => ImageFormat::Bmp,
    };
    dynamic
        .write_to(&mut output, format)
        .map_err(|error| format!("Failed to encode crop: {error}"))?;
    Ok(output.into_inner())
}

pub fn encode_image(image: &RgbaImage, encoder: CropEncoder) -> Result<Vec<u8>, String> {
    encode_rgba(image, encoder)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CropWriteError {
    Collision(PathBuf),
    Failed(String),
}

impl std::fmt::Display for CropWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Collision(path) => write!(f, "Destination already exists: {}", path.display()),
            Self::Failed(error) => f.write_str(error),
        }
    }
}

/// Encode before checking/writing and refuse to replace an existing path
/// unless the caller has completed its explicit confirmation step.
pub fn write_crop(
    destination: &Path,
    image: &RgbaImage,
    allow_overwrite: bool,
) -> Result<(), CropWriteError> {
    let encoder = select_encoder(destination).map_err(CropWriteError::Failed)?;
    let bytes = encode_rgba(image, encoder).map_err(CropWriteError::Failed)?;
    if destination.exists() && !allow_overwrite {
        return Err(CropWriteError::Collision(destination.to_path_buf()));
    }
    if let Some(parent) = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        return Err(CropWriteError::Failed(format!(
            "Failed to create {}: {error}",
            parent.display()
        )));
    }
    std::fs::write(destination, bytes).map_err(|error| {
        CropWriteError::Failed(format!("Failed to save {}: {error}", destination.display()))
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingConfirmation {
    SaveAs(PathBuf),
    OverwriteSource(PathBuf),
}

/// The standalone crop dialog. It owns the reusable canvas and only performs
/// writes after an explicit destination/source confirmation.
pub struct CropDialogState {
    open: bool,
    canvas: Option<CropCanvas>,
    context: Option<CropSourceContext>,
    suggested_name: String,
    pending_confirmation: Option<PendingConfirmation>,
    message: Option<String>,
}

impl Default for CropDialogState {
    fn default() -> Self {
        Self {
            open: false,
            canvas: None,
            context: None,
            suggested_name: String::new(),
            pending_confirmation: None,
            message: None,
        }
    }
}

impl CropDialogState {
    pub fn open(&mut self, context: CropSourceContext, image: RgbaImage) -> Result<(), String> {
        let canvas = CropCanvas::from_image(image).map_err(|error| error.to_string())?;
        let suggested_name = match &context {
            CropSourceContext::ExistingFile { source_path } => {
                cropped_destination_path(source_path)?
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("cropped.png")
                    .to_owned()
            }
            CropSourceContext::Screenshot { .. } => screenshot_filename(),
        };
        self.open = true;
        self.suggested_name = suggested_name;
        self.canvas = Some(canvas);
        self.context = Some(context);
        self.pending_confirmation = None;
        self.message = None;
        Ok(())
    }

    pub fn open_existing_file(&mut self, source_path: PathBuf) -> Result<(), String> {
        let image = decode_rgba(&source_path)?;
        self.open(CropSourceContext::ExistingFile { source_path }, image)
    }

    pub fn open_screenshot(
        &mut self,
        image: RgbaImage,
        default_directory: PathBuf,
    ) -> Result<(), String> {
        self.open(CropSourceContext::Screenshot { default_directory }, image)
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn source_context(&self) -> Option<&CropSourceContext> {
        self.context.as_ref()
    }

    pub fn canvas(&self) -> Option<&CropCanvas> {
        self.canvas.as_ref()
    }

    pub fn canvas_mut(&mut self) -> Option<&mut CropCanvas> {
        self.canvas.as_mut()
    }

    pub fn cancel(&mut self) {
        self.open = false;
        if let Some(canvas) = &mut self.canvas {
            canvas.cancel();
        }
        self.canvas = None;
        self.context = None;
        self.pending_confirmation = None;
    }

    fn persist(&mut self, destination: PathBuf, allow_overwrite: bool) {
        let Some(canvas) = self.canvas.as_ref() else {
            self.message = Some("Crop source is not loaded".into());
            return;
        };
        let cropped = match canvas.crop_result() {
            Ok(image) => image,
            Err(error) => {
                self.message = Some(error.to_string());
                return;
            }
        };
        match write_crop(&destination, &cropped, allow_overwrite) {
            Ok(()) => self.cancel(),
            Err(CropWriteError::Collision(path)) => {
                self.pending_confirmation = Some(PendingConfirmation::SaveAs(path));
                self.message =
                    Some("The destination already exists. Confirm replacement to continue.".into());
            }
            Err(CropWriteError::Failed(error)) => self.message = Some(error),
        }
    }

    fn choose_save_as(&mut self) {
        let Some(context) = self.context.as_ref() else {
            return;
        };
        let dialog = FileDialog::new()
            .set_directory(context.default_directory())
            .set_file_name(&self.suggested_name)
            .add_filter("PNG, JPEG, or BMP image", &["png", "jpg", "jpeg", "bmp"]);
        if let Some(destination) = dialog.save_file() {
            if destination.exists() {
                self.pending_confirmation = Some(PendingConfirmation::SaveAs(destination));
                self.message =
                    Some("The destination already exists. Confirm replacement to continue.".into());
            } else {
                self.persist(destination, false);
            }
        }
    }

    fn confirm_pending(&mut self) {
        let Some(pending) = self.pending_confirmation.take() else {
            return;
        };
        let path = match pending {
            PendingConfirmation::SaveAs(path) | PendingConfirmation::OverwriteSource(path) => path,
        };
        self.persist(path, true);
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let Some(canvas) = self.canvas.as_mut() else {
            self.message = Some("Crop source is not loaded".into());
            self.open = false;
            return;
        };
        let mut options = CropEditorUiOptions::new("Crop Image", "standalone_crop_image");
        options.source_label = Some(match &self.context {
            Some(CropSourceContext::ExistingFile { source_path }) => {
                format!("Source: {}", source_path.display())
            }
            Some(CropSourceContext::Screenshot { .. }) => "Source: screenshot".into(),
            None => "Source: unavailable".into(),
        });
        options.persistence_message = self.message.clone();
        options.allow_overwrite =
            matches!(self.context, Some(CropSourceContext::ExistingFile { .. }));
        match canvas.show(ctx, &options) {
            Some(CropEditorAction::SaveAs) => self.choose_save_as(),
            Some(CropEditorAction::RequestOverwrite) => {
                if let Some(CropSourceContext::ExistingFile { source_path }) = &self.context {
                    self.pending_confirmation =
                        Some(PendingConfirmation::OverwriteSource(source_path.clone()));
                    self.message =
                        Some("Confirm replacing the current source image to continue.".into());
                }
            }
            Some(CropEditorAction::Cancel) => self.cancel(),
            None => {}
        }
        if let Some(pending) = &self.pending_confirmation {
            let mut open = true;
            let label = match pending {
                PendingConfirmation::SaveAs(path) => format!("Replace {}?", path.display()),
                PendingConfirmation::OverwriteSource(path) => {
                    format!("Overwrite {}?", path.display())
                }
            };
            egui::Window::new("Confirm crop overwrite")
                .id(egui::Id::new("standalone_crop_overwrite_confirmation"))
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(label);
                    ui.label("The existing file will not be changed until you confirm.");
                    ui.horizontal(|ui| {
                        if ui.button("Confirm Replacement").clicked() {
                            // The state is consumed after the window borrow ends.
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(egui::Id::new("standalone_crop_confirm"), true);
                            });
                        }
                        if ui.button("Cancel").clicked() {
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(egui::Id::new("standalone_crop_cancel"), true);
                            });
                        }
                    });
                });
            let confirm = ctx.data(|data| {
                data.get_temp::<bool>(egui::Id::new("standalone_crop_confirm"))
                    .unwrap_or(false)
            });
            let cancel = ctx.data(|data| {
                data.get_temp::<bool>(egui::Id::new("standalone_crop_cancel"))
                    .unwrap_or(false)
            });
            if confirm || cancel || !open {
                ctx.data_mut(|data| {
                    data.remove::<bool>(egui::Id::new("standalone_crop_confirm"));
                    data.remove::<bool>(egui::Id::new("standalone_crop_cancel"));
                });
                if confirm {
                    self.confirm_pending();
                } else {
                    self.pending_confirmation = None;
                    self.message = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn rgba() -> RgbaImage {
        RgbaImage::from_pixel(2, 1, Rgba([10, 20, 30, 77]))
    }

    #[test]
    fn cropped_names_preserve_stems_extensions_case_and_parent() {
        let dir = tempdir().unwrap();
        for (name, expected) in [
            ("ordinary.png", "ordinary_cropped.png"),
            ("name.with.dots.jpg", "name.with.dots_cropped.jpg"),
            ("photo.jpeg", "photo_cropped.jpeg"),
            ("CAPTURE.PNG", "CAPTURE_cropped.PNG"),
            ("CAPTURE.JPEG", "CAPTURE_cropped.JPEG"),
        ] {
            assert_eq!(
                cropped_destination_path(&dir.path().join(name)).unwrap(),
                dir.path().join(expected)
            );
        }
        assert!(cropped_destination_path(&dir.path().join("extensionless")).is_err());
    }

    #[test]
    fn encoders_dispatch_case_insensitively_and_reject_unknown_extensions() {
        assert_eq!(
            select_encoder(Path::new("x.PNG")).unwrap(),
            CropEncoder::Png
        );
        assert_eq!(
            select_encoder(Path::new("x.jpeg")).unwrap(),
            CropEncoder::Jpeg
        );
        assert_eq!(
            select_encoder(Path::new("x.BMP")).unwrap(),
            CropEncoder::Bmp
        );
        assert!(select_encoder(Path::new("x.gif")).is_err());
        assert!(select_encoder(Path::new("x")).is_err());
    }

    #[test]
    fn jpeg_encoding_drops_alpha_only_at_the_format_boundary() {
        let bytes = encode_rgba(&rgba(), CropEncoder::Jpeg).unwrap();
        let decoded = image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg)
            .unwrap()
            .to_rgb8();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert!((i16::from(decoded.get_pixel(0, 0)[0]) - 10).abs() < 5);
        assert!((i16::from(decoded.get_pixel(0, 0)[1]) - 20).abs() < 5);
        assert!((i16::from(decoded.get_pixel(0, 0)[2]) - 30).abs() < 5);
    }

    #[test]
    fn collision_is_reported_before_destination_changes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("crop.bmp");
        std::fs::write(&path, b"sentinel").unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(matches!(
            write_crop(&path, &rgba(), false),
            Err(CropWriteError::Collision(_))
        ));
        assert_eq!(std::fs::read(path).unwrap(), before);
    }

    #[test]
    fn invalid_existing_context_does_not_leave_an_empty_dialog_open() {
        let mut dialog = CropDialogState::default();
        assert!(
            dialog
                .open(
                    CropSourceContext::ExistingFile {
                        source_path: PathBuf::from("image.gif"),
                    },
                    rgba(),
                )
                .is_err()
        );
        assert!(!dialog.is_open());
        assert!(dialog.canvas().is_none());
    }
}
