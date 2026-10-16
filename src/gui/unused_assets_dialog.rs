use crate::gui::LauncherApp;
use crate::gui::Panel;
use crate::gui::theme::widgets::{ButtonEmphasis, with_button_emphasis};
use crate::plugins::note::{assets_dir, unused_assets};
use eframe::egui;

#[derive(Default)]
pub struct UnusedAssetsDialog {
    pub open: bool,
    assets: Vec<String>,
    selected: Vec<bool>,
    #[cfg(test)]
    inventory_loader: Option<Box<dyn FnMut() -> Vec<String>>>,
    #[cfg(test)]
    test_layout: Option<UnusedAssetsLayoutObservation>,
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct UnusedAssetsLayoutObservation {
    window_rect: Option<egui::Rect>,
    scroll_id: egui::Id,
    scroll_rect: egui::Rect,
    content_size: egui::Vec2,
    scroll_offset: egui::Vec2,
    footer_reserved_height: f32,
    delete_id: egui::Id,
    delete_rect: egui::Rect,
    close_id: egui::Id,
    close_rect: egui::Rect,
    selection: Option<(egui::Id, egui::Rect)>,
    last_asset_rect: Option<egui::Rect>,
    close_requested: bool,
}

impl UnusedAssetsDialog {
    pub fn open(&mut self) {
        self.assets = self.load_inventory();
        self.selected = vec![false; self.assets.len()];
        self.open = true;
    }

    pub fn ensure_open(&mut self) {
        if !self.open {
            self.open();
        }
    }

    fn load_inventory(&mut self) -> Vec<String> {
        #[cfg(test)]
        if let Some(loader) = self.inventory_loader.as_mut() {
            return loader();
        }
        unused_assets()
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut open_flag = self.open;
        let mut close = false;
        #[cfg(test)]
        let mut test_layout = None;
        #[cfg(test)]
        let mut first_selection = None;
        #[cfg(test)]
        let mut last_asset_rect = None;
        let window_output = egui::Window::new("Unused Note Assets")
            .open(&mut open_flag)
            .resizable(true)
            .max_width(ctx.content_rect().width().max(1.0))
            .max_height(ctx.content_rect().height().max(1.0))
            .default_size((300.0, 200.0))
            .show(ctx, |ui| {
                let footer_height =
                    unused_assets_footer_reserved_height(ui, &["Delete Selected", "Close"]);
                let list_height = (ui.available_height() - footer_height).max(0.0);
                let _list = egui::ScrollArea::vertical()
                    .id_salt("unused_note_assets_list")
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .max_height(list_height)
                    .show(ui, |ui| {
                        if self.assets.is_empty() {
                            ui.label("No unused assets found");
                        } else {
                            for (idx, asset) in self.assets.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    let _selection = ui.checkbox(&mut self.selected[idx], "");
                                    #[cfg(test)]
                                    if idx == 0 {
                                        first_selection = Some((_selection.id, _selection.rect));
                                    }
                                    let label = ui.add(
                                        egui::Label::new(asset).wrap_mode(egui::TextWrapMode::Wrap),
                                    );
                                    #[cfg(test)]
                                    if idx + 1 == self.assets.len() {
                                        last_asset_rect = Some(label.rect);
                                    }
                                    #[cfg(not(test))]
                                    let _ = label;
                                });
                            }
                        }
                    });
                ui.separator();
                #[cfg(test)]
                let mut delete_response = None;
                #[cfg(test)]
                let mut close_response = None;
                ui.horizontal_wrapped(|ui| {
                    let response = ui.button("Delete Selected");
                    let response = with_button_emphasis(ui, response, ButtonEmphasis::Destructive);
                    if response.clicked() {
                        for i in (0..self.assets.len()).rev() {
                            if self.selected[i] {
                                let path = assets_dir().join(&self.assets[i]);
                                if let Err(e) = std::fs::remove_file(&path) {
                                    app.report_error_message(
                                        "ui operation",
                                        format!("Failed to delete {}: {e}", self.assets[i]),
                                    );
                                } else {
                                    self.assets.remove(i);
                                    self.selected.remove(i);
                                }
                            }
                        }
                    }
                    #[cfg(test)]
                    {
                        delete_response = Some(response);
                    }
                    let response = ui.button("Close");
                    if response.clicked() {
                        close = true;
                    }
                    #[cfg(test)]
                    {
                        close_response = Some(response);
                    }
                });
                #[cfg(test)]
                if let (Some(delete), Some(close)) = (delete_response, close_response) {
                    test_layout = Some(UnusedAssetsLayoutObservation {
                        window_rect: None,
                        scroll_id: _list.id,
                        scroll_rect: _list.inner_rect,
                        content_size: _list.content_size,
                        scroll_offset: _list.state.offset,
                        footer_reserved_height: footer_height,
                        delete_id: delete.id,
                        delete_rect: delete.rect,
                        close_id: close.id,
                        close_rect: close.rect,
                        selection: first_selection,
                        last_asset_rect,
                        close_requested: false,
                    });
                }
            });
        #[cfg(test)]
        {
            self.test_layout = test_layout.map(|mut layout: UnusedAssetsLayoutObservation| {
                layout.window_rect = window_output.map(|output| output.response.rect);
                layout.close_requested = close || !open_flag;
                layout
            });
        }
        #[cfg(not(test))]
        let _ = window_output;
        let pinned = app.pinned_panels.contains(&Panel::UnusedAssetsDialog);
        if close && !pinned {
            open_flag = false;
        }
        self.open = open_flag || pinned;
    }
}

fn unused_assets_footer_reserved_height(ui: &mut egui::Ui, labels: &[&str]) -> f32 {
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
    let wrap_mode = ui.wrap_mode();
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    let mut row_width = 0.0;
    let mut row_height = 0.0f32;
    let mut actions_height = 0.0;

    for label in labels {
        let text_size = egui::WidgetText::from(*label)
            .into_galley(ui, Some(wrap_mode), text_width, button_font.clone())
            .size();
        let button_width = (text_size.x + frame_margin.x)
            .max(spacing.interact_size.x)
            .min(available_width);
        let button_height = (text_size.y + spacing.extra_text_line_spacing + frame_margin.y)
            .max(spacing.interact_size.y);
        if row_width > 0.0 && row_width + spacing.item_spacing.x + button_width > available_width {
            actions_height += row_height + spacing.item_spacing.y;
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
    actions_height += row_height.max(spacing.interact_size.y);
    let separator_spacing = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let footer_height = actions_height + 2.0 * spacing.item_spacing.y + separator_spacing;
    (footer_height * pixels_per_point).ceil() / pixels_per_point
}

#[cfg(test)]
mod tests {
    use super::UnusedAssetsLayoutObservation;
    use crate::gui::{LauncherApp, Panel};
    use crate::plugin::PluginManager;
    use crate::settings::Settings;
    use eframe::egui;
    use std::cell::Cell;
    use std::sync::{Arc, atomic::AtomicBool};

    fn test_app(ctx: &egui::Context, root: &std::path::Path) -> LauncherApp {
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn input(size: egui::Vec2, events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn render_frame(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut dialog = std::mem::take(&mut app.unused_assets_dialog);
        let output = ctx.run_ui(input(size, events), |ctx| dialog.ui(ctx, app));
        app.unused_assets_dialog = dialog;
        output
    }

    fn node_for_response(
        output: &egui::FullOutput,
        response_id: egui::Id,
    ) -> Option<(egui::accesskit::Role, Option<String>, egui::Rect)> {
        let node_id = egui::accesskit::NodeId(response_id.value());
        let (_, node) = output
            .platform_output
            .accesskit_update
            .as_ref()?
            .nodes
            .iter()
            .find(|(id, _)| *id == node_id)?;
        let bounds = node.bounds()?;
        Some((
            node.role(),
            node.label().map(str::to_owned),
            egui::Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            ),
        ))
    }

    fn labelled_button(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
        output
            .platform_output
            .accesskit_update
            .as_ref()?
            .nodes
            .iter()
            .find_map(|(_, node)| {
                if node.role() != egui::accesskit::Role::Button || node.label() != Some(label) {
                    return None;
                }
                let bounds = node.bounds()?;
                Some(egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ))
            })
    }

    fn collect_text_shapes(
        shape: &egui::epaint::Shape,
        clip_rect: egui::Rect,
        output: &mut Vec<(String, egui::Rect, egui::Rect)>,
    ) {
        match shape {
            egui::epaint::Shape::Text(text) => output.push((
                text.galley.job.text.clone(),
                egui::Rect::from_min_size(text.pos, text.galley.size()),
                clip_rect,
            )),
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text_shapes(shape, clip_rect, output);
                }
            }
            _ => {}
        }
    }

    fn text_shapes(output: &egui::FullOutput) -> Vec<String> {
        let mut texts = Vec::new();
        for clipped_shape in &output.shapes {
            collect_text_shapes(&clipped_shape.shape, clipped_shape.clip_rect, &mut texts);
        }
        texts.into_iter().map(|(text, _, _)| text).collect()
    }

    fn visible_text_shapes(output: &egui::FullOutput, bounds: egui::Rect) -> Vec<String> {
        let mut texts = Vec::new();
        for clipped_shape in &output.shapes {
            collect_text_shapes(&clipped_shape.shape, clipped_shape.clip_rect, &mut texts);
        }
        texts
            .into_iter()
            .filter_map(|(text, galley_rect, clip_rect)| {
                (bounds.contains_rect(galley_rect) && clip_rect.contains_rect(galley_rect))
                    .then_some(text)
            })
            .collect()
    }

    fn click_at(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        size: egui::Vec2,
        rect: egui::Rect,
    ) -> bool {
        let point = rect.center();
        let press = render_frame(
            ctx,
            app,
            size,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        press.drop_without_applying_deltas();

        let release = render_frame(
            ctx,
            app,
            size,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        let close_requested = app
            .unused_assets_dialog
            .test_layout
            .is_some_and(|layout| layout.close_requested);
        release.drop_without_applying_deltas();
        close_requested
    }

    fn first_asset() -> String {
        initial_inventory()[0].clone()
    }

    fn last_asset() -> String {
        initial_inventory()[15].clone()
    }

    fn initial_inventory() -> Vec<String> {
        (0..16)
            .map(|index| {
                format!(
                    "unused image {index:02} {}.png",
                    "very-long-note-asset-segment-".repeat(4)
                )
            })
            .collect()
    }

    fn assert_scroll_is_legal(layout: UnusedAssetsLayoutObservation) -> bool {
        layout.content_size.y > layout.scroll_rect.height()
            && layout.scroll_offset.x.abs() <= f32::EPSILON
            && layout.scroll_offset.y >= -f32::EPSILON
            && layout.scroll_offset.y
                <= (layout.content_size.y - layout.scroll_rect.height()).max(0.0) + f32::EPSILON
    }

    #[test]
    fn pinned_open_preserves_selection_until_explicit_close_and_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        let mut app = test_app(&ctx, directory.path());
        let load_count = std::rc::Rc::new(Cell::new(0));
        let loader_count = load_count.clone();
        app.unused_assets_dialog.inventory_loader = Some(Box::new(move || {
            let calls = loader_count.get() + 1;
            loader_count.set(calls);
            if calls == 1 {
                initial_inventory()
            } else {
                vec!["fresh inventory after reopen.png".to_owned()]
            }
        }));
        app.pinned_panels.push(Panel::UnusedAssetsDialog);
        app.ensure_open(Panel::UnusedAssetsDialog);
        assert_eq!(load_count.get(), 1);

        let size = egui::vec2(300.0, 200.0);
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let mut initial_layout = None;
        let mut close_window_rect = None;
        let mut delete_node = None;
        let mut close_node = None;
        let mut selection_node = None;
        let mut labels = Vec::new();
        for _ in 0..3 {
            let initial = render_frame(&ctx, &mut app, size, Vec::new());
            initial_layout = app.unused_assets_dialog.test_layout;
            close_window_rect = labelled_button(&initial, "Close window");
            delete_node =
                initial_layout.and_then(|layout| node_for_response(&initial, layout.delete_id));
            close_node =
                initial_layout.and_then(|layout| node_for_response(&initial, layout.close_id));
            selection_node = initial_layout
                .and_then(|layout| layout.selection)
                .and_then(|(id, _)| node_for_response(&initial, id));
            labels = text_shapes(&initial);
            initial.drop_without_applying_deltas();
        }

        let initial_layout = initial_layout.expect("unused-asset layout is observed");
        let window_rect = initial_layout
            .window_rect
            .expect("the native window is rendered");
        let (delete_role, delete_label, delete_access_rect) =
            delete_node.expect("Delete Selected has an AccessKit node");
        let (close_role, close_label, close_access_rect) =
            close_node.expect("Close has an AccessKit node");
        let (selection_role, _, _) =
            selection_node.expect("the first asset checkbox is accessible");
        let close_window_rect = close_window_rect.expect("pinned window exposes its native X");
        let selection_rect = initial_layout
            .selection
            .expect("the first asset exposes a checkbox response")
            .1;

        assert!(screen_rect.contains_rect(window_rect));
        assert!(window_rect.contains_rect(initial_layout.scroll_rect));
        assert!(initial_layout.scroll_rect.contains_rect(selection_rect));
        assert!(window_rect.contains_rect(initial_layout.delete_rect));
        assert!(window_rect.contains_rect(initial_layout.close_rect));
        assert!(initial_layout.scroll_rect.bottom() <= initial_layout.delete_rect.top());
        assert!(initial_layout.scroll_rect.bottom() <= initial_layout.close_rect.top());
        assert!(initial_layout.footer_reserved_height > 0.0);
        assert!(assert_scroll_is_legal(initial_layout));
        assert_eq!(delete_role, egui::accesskit::Role::Button);
        assert_eq!(delete_label.as_deref(), Some("Delete Selected"));
        assert_eq!(close_role, egui::accesskit::Role::Button);
        assert_eq!(close_label.as_deref(), Some("Close"));
        assert_eq!(selection_role, egui::accesskit::Role::CheckBox);
        assert!(initial_layout.delete_rect.intersects(delete_access_rect));
        assert!(initial_layout.close_rect.intersects(close_access_rect));
        assert!(labels.iter().any(|text| text == &first_asset()));

        click_at(&ctx, &mut app, size, selection_rect);
        assert!(app.unused_assets_dialog.selected[0]);
        app.ensure_open(Panel::UnusedAssetsDialog);
        assert_eq!(load_count.get(), 1);
        assert!(app.unused_assets_dialog.selected[0]);

        let state = egui::scroll_area::State::load(&ctx, initial_layout.scroll_id);
        assert!(state.is_some());
        let max_offset =
            (initial_layout.content_size.y - initial_layout.scroll_rect.height()).max(0.0);
        if let Some(mut state) = state {
            state.offset.y = max_offset;
            state.store(&ctx, initial_layout.scroll_id);
        }
        let mut scrolled_layout = None;
        let mut scrolled_labels = Vec::new();
        for _ in 0..2 {
            let scrolled = render_frame(&ctx, &mut app, size, Vec::new());
            scrolled_layout = app.unused_assets_dialog.test_layout;
            scrolled_labels = scrolled_layout
                .map(|layout| visible_text_shapes(&scrolled, layout.scroll_rect))
                .unwrap_or_default();
            scrolled.drop_without_applying_deltas();
        }
        let scrolled_layout = scrolled_layout.expect("the native list retains its scroll state");
        let last_asset_rect = scrolled_layout
            .last_asset_rect
            .expect("the full last asset label reports its actual widget bounds");
        assert!(assert_scroll_is_legal(scrolled_layout));
        assert!(scrolled_layout.scroll_offset.y > 0.0);
        assert!(scrolled_labels.iter().any(|text| text == &last_asset()));
        assert!(scrolled_layout.scroll_rect.contains_rect(last_asset_rect));

        let x_requested = click_at(&ctx, &mut app, size, close_window_rect);
        assert!(
            x_requested,
            "the actual native X requested close before pin refusal"
        );
        assert!(app.unused_assets_dialog.open);
        assert!(app.unused_assets_dialog.selected[0]);
        app.ensure_open(Panel::UnusedAssetsDialog);
        assert_eq!(load_count.get(), 1);
        assert!(app.unused_assets_dialog.selected[0]);

        let pinned_close_rect = app
            .unused_assets_dialog
            .test_layout
            .expect("the pinned window remains rendered")
            .close_rect;
        let close_requested = click_at(&ctx, &mut app, size, pinned_close_rect);
        assert!(
            close_requested,
            "the actual Close button requested close before pin refusal"
        );
        assert!(app.unused_assets_dialog.open);
        assert!(app.unused_assets_dialog.selected[0]);
        app.ensure_open(Panel::UnusedAssetsDialog);
        assert_eq!(load_count.get(), 1);
        assert!(app.unused_assets_dialog.selected[0]);

        app.pinned_panels
            .retain(|panel| *panel != Panel::UnusedAssetsDialog);
        let close_rect = app
            .unused_assets_dialog
            .test_layout
            .expect("Close remains available when unpinned")
            .close_rect;
        let unpinned_close_requested = click_at(&ctx, &mut app, size, close_rect);
        assert!(unpinned_close_requested);
        assert!(!app.unused_assets_dialog.open);

        app.ensure_open(Panel::UnusedAssetsDialog);
        assert_eq!(load_count.get(), 2);
        assert!(app.unused_assets_dialog.open);
        assert_eq!(
            app.unused_assets_dialog.assets,
            vec!["fresh inventory after reopen.png".to_owned()]
        );
        assert_eq!(app.unused_assets_dialog.selected, vec![false]);
    }
}
