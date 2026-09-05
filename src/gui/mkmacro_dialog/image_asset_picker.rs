//! Shared, searchable browser for references in the flat library.

use crate::mkmacro::{MkImageRef, MkMacroStore};
use eframe::egui;

/// Everything the picker may inspect. Entries always come from the store's
/// direct flat-root enumeration, not from a macro-owned catalog.
#[derive(Clone, Copy)]
pub struct ImageAssetUiContext<'a> {
    pub store: &'a MkMacroStore,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageAssetSelection {
    pub image: MkImageRef,
}

/// Widget-independent description of one browser row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImageAssetBrowserEntry<'a> {
    pub filename: &'a str,
    pub selected: bool,
    pub preview_key: super::image_preview::PreviewLookupKey,
}

/// Builds rows solely from the shared library. A selected row which does
/// not match the search is retained at the beginning, making its placement
/// deterministic while leaving matching rows in document order.
pub(crate) fn browser_entries<'a>(
    assets: &'a [MkImageRef],
    query: &str,
    selected: &MkImageRef,
) -> Vec<ImageAssetBrowserEntry<'a>> {
    let query = query.trim().to_lowercase();
    let matches =
        |asset: &MkImageRef| query.is_empty() || asset.filename().to_lowercase().contains(&query);
    let row = |asset: &'a MkImageRef| ImageAssetBrowserEntry {
        filename: asset.filename(),
        selected: asset == selected,
        preview_key: super::image_preview::PreviewLookupKey::new(asset.clone()),
    };
    let retained = assets
        .iter()
        .find(|asset| **asset == *selected && !matches(asset));
    retained
        .into_iter()
        .chain(assets.iter().filter(|asset| matches(asset)))
        .map(row)
        .collect()
}

/// Filters without reordering: document/source order is the deterministic UI order.
pub fn filtered_assets<'a>(assets: &'a [MkImageRef], query: &str) -> Vec<&'a MkImageRef> {
    let query = query.trim().to_lowercase();
    assets
        .iter()
        .filter(|asset| query.is_empty() || asset.filename().to_lowercase().contains(&query))
        .collect()
}

pub fn selected_asset<'a>(assets: &'a [MkImageRef], image: &MkImageRef) -> Option<&'a MkImageRef> {
    assets.iter().find(|asset| *asset == image)
}

fn select_browser_entry(image: &mut MkImageRef, filename: &str) -> ImageAssetSelection {
    let selected = MkImageRef::from_filename(filename);
    *image = selected.clone();
    ImageAssetSelection { image: selected }
}

/// Render a picker whose persistent state is isolated by `editor_id`.
pub fn show(
    ui: &mut egui::Ui,
    editor_id: impl std::hash::Hash,
    context: ImageAssetUiContext<'_>,
    image: &mut MkImageRef,
) -> Option<ImageAssetSelection> {
    ui.label("Reference Image");
    show_browser(ui, editor_id, context, image)
}

/// Embedded picker variant for callers that already render their own section label.
pub fn show_browser(
    ui: &mut egui::Ui,
    editor_id: impl std::hash::Hash,
    context: ImageAssetUiContext<'_>,
    image: &mut MkImageRef,
) -> Option<ImageAssetSelection> {
    let id = ui.make_persistent_id(("image-asset-picker", editor_id));
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

    let assets = context.store.image_refs().unwrap_or_default();
    if !image.filename().is_empty() && selected_asset(&assets, image).is_none() {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("Missing reference image · {}", image.filename()),
        );
    }
    let matches = browser_entries(&assets, &query, image);
    let mut selection = None;
    egui::ScrollArea::vertical()
        .id_source(id.with("scroll"))
        .max_height(230.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if assets.is_empty() {
                ui.weak("No PNG files are available in mkmacro_assets. Select PNG… or Capture… to add one.");
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
                                &crate::mkmacro::MkImageRef::from_filename(asset.filename),
                                56.0,
                            );
                            ui.vertical(|ui| {
                                ui.small(asset.filename);
                            });
                        });
                    });
                let response = ui.interact(
                    row.response.rect,
                    id.with(("row", asset.filename)),
                    egui::Sense::click(),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        selected,
                        format!(
                            "{}; {}",
                            asset.filename, asset.filename
                        ),
                    )
                });
                if response.clicked() {
                    selection = Some(select_browser_entry(image, asset.filename));
                }
            }
        });
    selection
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image(name: &str) -> MkImageRef {
        MkImageRef::from_filename(name)
    }

    #[test]
    fn filtering_matches_filenames_case_insensitively() {
        let assets = vec![image("LOGIN.png"), image("cat.png")];
        assert_eq!(
            filtered_assets(&assets, " login ")[0].filename(),
            "LOGIN.png"
        );
        assert_eq!(filtered_assets(&assets, "CAT")[0].filename(), "cat.png");
        assert!(filtered_assets(&assets, "LOGIN").len() == 1);
        assert!(filtered_assets(&assets, "missing").is_empty());
        assert_eq!(
            filtered_assets(&assets, "")
                .iter()
                .map(|a| a.filename())
                .collect::<Vec<_>>(),
            vec!["LOGIN.png", "cat.png"]
        );
        assert_eq!(filtered_assets(&assets, "   ").len(), 2);
    }

    #[test]
    fn source_list_is_complete_and_deterministically_ordered() {
        let assets = vec![image("b.png"), image("a.png")];
        assert_eq!(
            filtered_assets(&assets, "")
                .iter()
                .map(|a| a.filename())
                .collect::<Vec<_>>(),
            vec!["b.png", "a.png"]
        );
    }

    #[test]
    fn selected_and_missing_lookup_are_explicit() {
        let assets = vec![image("three.png")];
        assert_eq!(
            selected_asset(&assets, &image("three.png"))
                .unwrap()
                .filename(),
            "three.png"
        );
        assert!(selected_asset(&assets, &image("missing.png")).is_none());
    }

    #[test]
    fn selection_event_updates_only_the_passed_value() {
        let event = ImageAssetSelection {
            image: image("new.png"),
        };
        let selected = event.image;
        assert_eq!(selected.filename(), "new.png");
    }

    #[test]
    fn browser_model_uses_filename_rows_and_shared_preview_identity() {
        let current = vec![image("same.png"), image("two.png")];
        let rows = browser_entries(&current, "", &image("two.png"));
        assert_eq!(
            rows.iter().map(|row| row.filename).collect::<Vec<_>>(),
            vec!["same.png", "two.png"]
        );
        assert!(rows[1].selected);
        assert_eq!(
            rows[0].preview_key,
            super::super::image_preview::PreviewLookupKey::new(image("same.png"))
        );
        assert!(browser_entries(&[], "", &image("same.png")).is_empty());
    }

    #[test]
    fn browser_searches_names_and_filenames_case_insensitively() {
        let assets = vec![
            image("login_button.png"),
            image("Siamese-Cat.PNG"),
            image("gear.png"),
        ];
        assert_eq!(
            browser_entries(&assets, "  bUtTo  ", &MkImageRef::default())[0].filename,
            "login_button.png"
        );
        assert_eq!(
            browser_entries(&assets, "cat.p", &MkImageRef::default())[0].filename,
            "Siamese-Cat.PNG"
        );
        assert!(browser_entries(&assets, "absent", &MkImageRef::default()).is_empty());
    }

    #[test]
    fn selected_non_match_is_retained_first_without_duplicates() {
        let assets = vec![image("a.png"), image("b.png"), image("alphabet.png")];
        let retained = browser_entries(&assets, "alpha", &image("b.png"));
        assert_eq!(
            retained.iter().map(|row| row.filename).collect::<Vec<_>>(),
            vec!["b.png", "alphabet.png"]
        );
        assert!(retained[0].selected);
        let matching = browser_entries(&assets, "b.png", &image("b.png"));
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].filename, "b.png");
    }

    #[test]
    fn missing_current_image_does_not_block_selecting_an_available_row() {
        let assets = vec![image("b.png"), image("a.png")];
        let mut current = image("missing.png");
        let rows = browser_entries(&assets, "A.P", &current);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].filename, "a.png");
        assert!(!rows[0].selected);

        let untouched_sibling_state = 42;
        let event = select_browser_entry(&mut current, rows[0].filename);
        assert_eq!(current, image("a.png"));
        assert_eq!(event.image, image("a.png"));
        assert_eq!(untouched_sibling_state, 42);
        assert_eq!(
            browser_entries(&assets, "", &current)
                .iter()
                .map(|row| (row.filename, row.selected))
                .collect::<Vec<_>>(),
            vec![("b.png", false), ("a.png", true)]
        );
    }
}
