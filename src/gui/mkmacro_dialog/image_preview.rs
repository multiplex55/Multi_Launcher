//! Bounded, failure-tolerant previews for macro PNG assets.
use crate::mkmacro::MkMacroStore;
use eframe::egui;
use std::{collections::HashMap, fs, time::SystemTime};

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

#[derive(Clone, Default)]
struct PreviewCache(HashMap<PreviewKey, CachedPreview>);

pub fn fitted_size(width: u32, height: u32, bound: f32) -> egui::Vec2 {
    if width == 0 || height == 0 {
        return egui::Vec2::ZERO;
    }
    let scale = (bound / width as f32).min(bound / height as f32).min(1.0);
    egui::vec2(width as f32 * scale, height as f32 * scale)
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
    let cached: Result<CachedPreview, String> = ui.ctx().data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<PreviewCache>(cache_id);
        // A changed file identity invalidates the former texture for this asset.
        cache
            .0
            .retain(|old, _| old.macro_id != macro_id || old.asset_id != asset_id || old == &key);
        if let Some(value) = cache.0.get(&key) {
            return Ok(value.clone());
        }
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?
            .to_rgba8();
        if image.width() == 0 || image.height() == 0 {
            return Err("image has zero dimensions".into());
        }
        let size = [image.width() as usize, image.height() as usize];
        let value = CachedPreview {
            texture: ui.ctx().load_texture(
                format!("mkmacro-preview-{macro_id}-{asset_id}-{}", metadata.len()),
                egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
                Default::default(),
            ),
            width: image.width(),
            height: image.height(),
        };
        cache.0.insert(key.clone(), value.clone());
        Ok(value)
    });
    match cached {
        Ok(cached) => {
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
        Err(error) => {
            ui.colored_label(
                egui::Color32::RED,
                format!("Missing or corrupt reference image (asset {asset_id}): {error}"),
            );
        }
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
}
