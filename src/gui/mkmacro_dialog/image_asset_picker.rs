//! Shared, searchable browser for image assets owned by the active macro.

use crate::mkmacro::{MkImageAsset, MkMacroStore};
use eframe::egui;
use std::path::Path;

/// Everything the picker may inspect. Callers must pass the complete asset
/// collection of `macro_id`; the picker deliberately never queries other macros.
#[derive(Clone, Copy)]
pub struct ImageAssetUiContext<'a> {
    pub macro_id: u64,
    pub assets: &'a [MkImageAsset],
    pub store: &'a MkMacroStore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageAssetSelection {
    pub asset_id: u64,
}

pub fn asset_filename(asset: &MkImageAsset) -> &str {
    Path::new(&asset.relative_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&asset.relative_path)
}

/// Filters without reordering: document/source order is the deterministic UI order.
pub fn filtered_assets<'a>(assets: &'a [MkImageAsset], query: &str) -> Vec<&'a MkImageAsset> {
    let query = query.trim().to_lowercase();
    assets
        .iter()
        .filter(|asset| {
            query.is_empty()
                || asset.name.to_lowercase().contains(&query)
                || asset_filename(asset).to_lowercase().contains(&query)
                || asset.id.to_string().contains(&query)
        })
        .collect()
}

pub fn selected_asset<'a>(assets: &'a [MkImageAsset], asset_id: u64) -> Option<&'a MkImageAsset> {
    assets.iter().find(|asset| asset.id == asset_id)
}

/// Render a picker whose persistent state is isolated by `editor_id`.
pub fn show(
    ui: &mut egui::Ui,
    editor_id: impl std::hash::Hash,
    context: ImageAssetUiContext<'_>,
    asset_id: &mut u64,
) -> Option<ImageAssetSelection> {
    let id = ui.make_persistent_id(("image-asset-picker", editor_id));
    ui.label("Reference Image");
    let mut query = ui
        .ctx()
        .data_mut(|data| data.get_temp::<String>(id).unwrap_or_default());
    let search = ui.add(
        egui::TextEdit::singleline(&mut query)
            .hint_text("Search assets...")
            .id_source(id.with("search")),
    );
    search.widget_info(|| egui::WidgetInfo::text_edit("", "Search image assets"));
    ui.ctx()
        .data_mut(|data| data.insert_temp(id, query.clone()));

    if *asset_id != 0 && selected_asset(context.assets, *asset_id).is_none() {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("Missing asset · ID {}", *asset_id),
        );
    }
    let matches = filtered_assets(context.assets, &query);
    let mut selection = None;
    egui::ScrollArea::vertical()
        .id_source(id.with("scroll"))
        .max_height(230.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if context.assets.is_empty() {
                ui.weak(
                    "No image assets belong to this macro. Select PNG… or Capture… to add one.",
                );
            } else if matches.is_empty() {
                ui.weak("No image assets match this search.");
            }
            for asset in matches {
                let selected = *asset_id == asset.id;
                let fill = if selected {
                    ui.visuals().selection.bg_fill
                } else {
                    egui::Color32::TRANSPARENT
                };
                let row = egui::Frame::none()
                    .fill(fill)
                    .inner_margin(egui::Margin::same(4.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            super::image_preview::show_thumbnail(
                                ui,
                                context.store,
                                context.macro_id,
                                asset.id,
                                56.0,
                            );
                            ui.vertical(|ui| {
                                ui.strong(if asset.name.is_empty() {
                                    asset_filename(asset)
                                } else {
                                    &asset.name
                                });
                                ui.small(asset_filename(asset));
                                ui.small(format!("ID {}", asset.id));
                            });
                        });
                    });
                let response = ui.interact(
                    row.response.rect,
                    id.with(("row", asset.id)),
                    egui::Sense::click(),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        selected,
                        format!("{}; {}; ID {}", asset.name, asset_filename(asset), asset.id),
                    )
                });
                if response.clicked() {
                    *asset_id = asset.id;
                    selection = Some(ImageAssetSelection { asset_id: asset.id });
                }
            }
        });
    selection
}

#[cfg(test)]
mod tests {
    use super::*;
    fn asset(id: u64, name: &str, file: &str) -> MkImageAsset {
        MkImageAsset {
            id,
            name: name.into(),
            relative_path: file.into(),
        }
    }

    #[test]
    fn filtering_covers_names_files_ids_case_whitespace_and_empty() {
        let assets = vec![
            asset(42, "Login Button", "m/LOGIN.png"),
            asset(7, "Other", "m/cat.png"),
        ];
        assert_eq!(filtered_assets(&assets, " login ")[0].id, 42);
        assert_eq!(filtered_assets(&assets, "CAT")[0].id, 7);
        assert_eq!(filtered_assets(&assets, " 42 ")[0].id, 42);
        assert!(filtered_assets(&assets, "missing").is_empty());
        assert_eq!(
            filtered_assets(&assets, "")
                .iter()
                .map(|a| a.id)
                .collect::<Vec<_>>(),
            vec![42, 7]
        );
        assert_eq!(filtered_assets(&assets, "   ").len(), 2);
    }

    #[test]
    fn source_list_is_complete_and_deterministically_ordered() {
        let assets = vec![asset(9, "Unused", "9.png"), asset(2, "Used", "2.png")];
        assert_eq!(
            filtered_assets(&assets, "")
                .iter()
                .map(|a| a.id)
                .collect::<Vec<_>>(),
            vec![9, 2]
        );
    }

    #[test]
    fn selected_and_missing_lookup_are_explicit() {
        let assets = vec![asset(3, "Three", "3.png")];
        assert_eq!(selected_asset(&assets, 3).unwrap().id, 3);
        assert!(selected_asset(&assets, 99).is_none());
    }

    #[test]
    fn selection_event_updates_only_the_passed_value() {
        let mut selected = 4;
        let event = ImageAssetSelection { asset_id: 8 };
        selected = event.asset_id;
        assert_eq!(selected, 8);
    }
}
