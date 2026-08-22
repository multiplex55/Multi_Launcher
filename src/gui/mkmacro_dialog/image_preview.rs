//! Bounded, failure-tolerant previews for macro PNG assets.
use crate::mkmacro::MkMacroStore;
use eframe::egui;

pub const PREVIEW_BOUND: f32 = 220.0;

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
    match image::open(&path).map(|x| x.to_rgba8()) {
        Ok(image) => {
            let size = [image.width() as usize, image.height() as usize];
            let texture = ui.ctx().load_texture(
                format!(
                    "mkmacro-preview-{macro_id}-{asset_id}-{:?}",
                    path.metadata().and_then(|m| m.modified()).ok()
                ),
                egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
                Default::default(),
            );
            ui.image((
                texture.id(),
                fitted_size(image.width(), image.height(), PREVIEW_BOUND),
            ));
            ui.label(format!(
                "{} — {} × {} px",
                path.file_name().unwrap_or_default().to_string_lossy(),
                image.width(),
                image.height()
            ));
            ui.small(format!("Asset ID {asset_id}"));
        }
        Err(error) => {
            ui.colored_label(
                egui::Color32::RED,
                format!("Missing or corrupt reference image: {error}"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sizes_are_bounded_and_keep_aspect() {
        for (w, h) in [(400, 100), (100, 400), (100_000, 50_000), (20, 10)] {
            let s = fitted_size(w, h, 220.0);
            assert!(s.x <= 220.0 && s.y <= 220.0);
            assert!((s.x / s.y - w as f32 / h as f32).abs() < 0.01);
        }
        assert_eq!(fitted_size(0, 3, 220.0), egui::Vec2::ZERO);
    }
}
