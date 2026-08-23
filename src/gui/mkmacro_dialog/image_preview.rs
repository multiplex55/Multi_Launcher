//! Bounded, failure-tolerant previews for macro PNG assets.
use crate::mkmacro::{MkMacroStore, decode_reference_png};
use eframe::egui;
use std::{
    collections::HashMap,
    fs,
    sync::{Arc, Mutex, mpsc},
    time::SystemTime,
};

pub const PREVIEW_BOUND: f32 = 220.0;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct PreviewKey {
    macro_id: u64,
    asset_id: u64,
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Clone)]
struct CachedPreview {
    texture: egui::TextureHandle,
    width: u32,
    height: u32,
}

struct DecodedPreview {
    rgba: Vec<u8>,
    thumbnail_width: u32,
    thumbnail_height: u32,
    width: u32,
    height: u32,
}

type PreviewResult = Result<DecodedPreview, String>;

#[derive(Clone)]
enum PreviewEntry {
    Loading(Arc<Mutex<mpsc::Receiver<PreviewResult>>>),
    Ready(CachedPreview),
    Failed(String),
}

#[derive(Clone, Default)]
struct PreviewCache(HashMap<PreviewKey, PreviewEntry>);

pub fn fitted_size(width: u32, height: u32, bound: f32) -> egui::Vec2 {
    if width == 0 || height == 0 {
        return egui::Vec2::ZERO;
    }
    let scale = (bound / width as f32).min(bound / height as f32).min(1.0);
    egui::vec2(width as f32 * scale, height as f32 * scale)
}

fn decode_thumbnail(path: &std::path::Path) -> PreviewResult {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let image = decode_reference_png(&bytes).map_err(|error| format!("{error:#}"))?;
    let (width, height) = image.dimensions();
    let thumbnail = image::imageops::thumbnail(&image, PREVIEW_BOUND as u32, PREVIEW_BOUND as u32);
    Ok(DecodedPreview {
        thumbnail_width: thumbnail.width(),
        thumbnail_height: thumbnail.height(),
        rgba: thumbnail.into_raw(),
        width,
        height,
    })
}

pub fn show(ui: &mut egui::Ui, store: &MkMacroStore, macro_id: u64, asset_id: u64) {
    let Ok(path) = store.asset_path(macro_id, asset_id) else {
        ui.colored_label(egui::Color32::RED, "No reference image selected");
        return;
    };
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => {
            ui.colored_label(
                egui::Color32::RED,
                format!("Missing reference image (asset {asset_id}): {error}"),
            );
            return;
        }
    };
    let key = PreviewKey {
        macro_id,
        asset_id,
        len: metadata.len(),
        modified: metadata.modified().ok(),
    };
    let cache_id = egui::Id::new("mkmacro-image-preview-cache");

    // Cache access is intentionally separate from decoding and texture upload.
    let entry = ui.ctx().data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<PreviewCache>(cache_id);
        cache
            .0
            .retain(|old, _| old.macro_id != macro_id || old.asset_id != asset_id || old == &key);
        cache.0.get(&key).cloned()
    });
    let entry = entry.unwrap_or_else(|| {
        let (tx, rx) = mpsc::channel();
        let ctx = ui.ctx().clone();
        let worker_path = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(decode_thumbnail(&worker_path));
            ctx.request_repaint();
        });
        let loading = PreviewEntry::Loading(Arc::new(Mutex::new(rx)));
        ui.ctx().data_mut(|data| {
            data.get_temp_mut_or_default::<PreviewCache>(cache_id)
                .0
                .insert(key.clone(), loading.clone());
        });
        loading
    });

    let entry = match entry {
        PreviewEntry::Loading(rx) => match rx.lock().unwrap().try_recv() {
            Ok(Ok(decoded)) => {
                // No egui temp-data borrow is held while the texture is created.
                let texture = ui.ctx().load_texture(
                    format!("mkmacro-preview-{macro_id}-{asset_id}-{}", metadata.len()),
                    egui::ColorImage::from_rgba_unmultiplied(
                        [
                            decoded.thumbnail_width as usize,
                            decoded.thumbnail_height as usize,
                        ],
                        &decoded.rgba,
                    ),
                    Default::default(),
                );
                PreviewEntry::Ready(CachedPreview {
                    texture,
                    width: decoded.width,
                    height: decoded.height,
                })
            }
            Ok(Err(error)) => PreviewEntry::Failed(error),
            Err(mpsc::TryRecvError::Empty) => {
                ui.spinner();
                ui.label("Loading reference image preview...");
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(50));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                PreviewEntry::Failed("preview worker stopped unexpectedly".into())
            }
        },
        other => other,
    };
    ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<PreviewCache>(cache_id)
            .0
            .insert(key, entry.clone());
    });
    match entry {
        PreviewEntry::Ready(cached) => {
            ui.image((
                cached.texture.id(),
                fitted_size(cached.width, cached.height, PREVIEW_BOUND),
            ));
            ui.label(format!(
                "{} — {} × {} px",
                path.file_name().unwrap_or_default().to_string_lossy(),
                cached.width,
                cached.height
            ));
            ui.small(format!("Asset ID {asset_id}"));
        }
        PreviewEntry::Failed(error) => {
            ui.colored_label(
                egui::Color32::RED,
                format!("Missing or corrupt reference image (asset {asset_id}): {error}"),
            );
        }
        PreviewEntry::Loading(_) => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sizes_are_bounded_and_keep_aspect() {
        for (w, h) in [
            (400, 100),
            (100, 400),
            (400, 400),
            (100_000, 50_000),
            (20, 10),
        ] {
            let s = fitted_size(w, h, 220.0);
            assert!(s.x <= 220.0 && s.y <= 220.0);
            assert!((s.x / s.y - w as f32 / h as f32).abs() < 0.01);
        }
        assert_eq!(fitted_size(20, 10, 220.0), egui::vec2(20.0, 10.0));
        assert_eq!(fitted_size(0, 3, 220.0), egui::Vec2::ZERO);
    }

    #[test]
    fn thumbnail_pixels_are_bounded_but_source_dimensions_are_retained() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.png");
        image::RgbaImage::new(800, 400).save(&path).unwrap();
        let decoded = decode_thumbnail(&path).unwrap();
        assert_eq!((decoded.width, decoded.height), (800, 400));
        assert_eq!(
            (decoded.thumbnail_width, decoded.thumbnail_height),
            (220, 110)
        );
        assert_eq!(decoded.rgba.len(), 220 * 110 * 4);
    }
}
