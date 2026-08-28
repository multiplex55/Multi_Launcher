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

/// Widget-independent description of one browser row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImageAssetBrowserEntry<'a> {
    pub asset_id: u64,
    pub display_name: &'a str,
    pub filename: &'a str,
    pub selected: bool,
    pub preview_key: super::image_preview::PreviewLookupKey,
}

/// Builds rows solely from the active macro's assets. A selected row which does
/// not match the search is retained at the beginning, making its placement
/// deterministic while leaving matching rows in document order.
pub(crate) fn browser_entries<'a>(
    macro_id: u64,
    assets: &'a [MkImageAsset],
    query: &str,
    selected_asset_id: u64,
) -> Vec<ImageAssetBrowserEntry<'a>> {
    let query = query.trim().to_lowercase();
    let matches = |asset: &MkImageAsset| {
        query.is_empty()
            || asset.name.to_lowercase().contains(&query)
            || asset_filename(asset).to_lowercase().contains(&query)
    };
    let row = |asset: &'a MkImageAsset| ImageAssetBrowserEntry {
        asset_id: asset.id,
        display_name: if asset.name.is_empty() {
            asset_filename(asset)
        } else {
            &asset.name
        },
        filename: asset_filename(asset),
        selected: asset.id == selected_asset_id,
        preview_key: super::image_preview::PreviewLookupKey::new(macro_id, asset.id),
    };
    let retained = assets
        .iter()
        .find(|asset| asset.id == selected_asset_id && !matches(asset));
    retained
        .into_iter()
        .chain(assets.iter().filter(|asset| matches(asset)))
        .map(row)
        .collect()
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
    let matches = browser_entries(context.macro_id, context.assets, &query, *asset_id);
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
                let selected = asset.selected;
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
                                asset.asset_id,
                                56.0,
                            );
                            ui.vertical(|ui| {
                                ui.strong(asset.display_name);
                                ui.small(asset.filename);
                                ui.small(format!("ID {}", asset.asset_id));
                            });
                        });
                    });
                let response = ui.interact(
                    row.response.rect,
                    id.with(("row", asset.asset_id)),
                    egui::Sense::click(),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        selected,
                        format!(
                            "{}; {}; ID {}",
                            asset.display_name, asset.filename, asset.asset_id
                        ),
                    )
                });
                if response.clicked() {
                    *asset_id = asset.asset_id;
                    selection = Some(ImageAssetSelection {
                        asset_id: asset.asset_id,
                    });
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
        assert_eq!(selected, 4);
        let event = ImageAssetSelection { asset_id: 8 };
        selected = event.asset_id;
        assert_eq!(selected, 8);
    }

    #[test]
    fn browser_model_is_macro_scoped_complete_and_has_preview_identity() {
        let current = vec![
            asset(1, "Shared", "current/same.png"),
            asset(2, "", "current/two.png"),
        ];
        let other_macro = vec![
            asset(1, "Shared", "other/same.png"),
            asset(3, "Two", "other/two.png"),
        ];
        let rows = browser_entries(10, &current, "", 2);
        assert_eq!(
            rows.iter().map(|row| row.asset_id).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(rows[1].display_name, "two.png");
        assert!(rows[1].selected);
        assert_eq!(
            rows[0].preview_key,
            super::super::image_preview::PreviewLookupKey::new(10, 1)
        );
        assert!(rows.iter().all(|row| {
            !other_macro
                .iter()
                .any(|other| other.id == row.asset_id && other.relative_path == row.filename)
        }));
        assert!(browser_entries(10, &[], "same", 1).is_empty());
    }

    #[test]
    fn browser_searches_names_and_filenames_case_insensitively() {
        let assets = vec![
            asset(1, "Login Button", "shots/unrelated.png"),
            asset(2, "Animal", "shots/Siamese-Cat.PNG"),
            asset(3, "Settings", "shots/gear.png"),
        ];
        assert_eq!(browser_entries(1, &assets, "  bUtTo  ", 0)[0].asset_id, 1);
        assert_eq!(browser_entries(1, &assets, "cat.p", 0)[0].asset_id, 2);
        assert!(browser_entries(1, &assets, "absent", 0).is_empty());
    }

    #[test]
    fn selected_non_match_is_retained_first_without_duplicates() {
        let assets = vec![
            asset(1, "Alpha", "a.png"),
            asset(2, "Beta", "b.png"),
            asset(3, "Alphabet", "c.png"),
        ];
        let retained = browser_entries(7, &assets, "alpha", 2);
        assert_eq!(
            retained.iter().map(|row| row.asset_id).collect::<Vec<_>>(),
            vec![2, 1, 3]
        );
        assert!(retained[0].selected);
        let matching = browser_entries(7, &assets, "beta", 2);
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].asset_id, 2);
    }
}
