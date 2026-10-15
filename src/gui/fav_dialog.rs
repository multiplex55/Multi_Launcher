use crate::actions::Action;
use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, labeled_form_row, secondary_help,
    semantic_status, with_button_emphasis,
};
use crate::plugins::fav::{
    FAV_FILE, FavEntry, join_command_args, load_favs, replace_favs, resolve_with_plugin,
};
use eframe::egui;

#[derive(Default)]
pub struct FavDialog {
    pub open: bool,
    entries: Vec<FavEntry>,
    edit_idx: Option<usize>,
    label: String,
    command: String,
    args: String,
    add_plugin: String,
    add_filter: String,
    load_error: Option<String>,
    #[cfg(test)]
    last_layout: Option<FavDialogLayout>,
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct FavWidgetLayout {
    id: egui::Id,
    rect: egui::Rect,
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct FavScrollLayout {
    id: egui::Id,
    parent_max_rect: egui::Rect,
    parent_clip_rect: egui::Rect,
    inner_rect: egui::Rect,
    content_size: egui::Vec2,
    offset: egui::Vec2,
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct FavDialogLayout {
    window_rect: egui::Rect,
    scroll: FavScrollLayout,
    primary: FavWidgetLayout,
    secondary: FavWidgetLayout,
    close_clicked: bool,
    visible: bool,
    sizing_pass: bool,
}

impl FavDialog {
    pub fn open(&mut self) {
        self.open_from(FAV_FILE);
    }

    fn open_from(&mut self, path: &str) {
        let _ = self.load_from(path);
        self.open = true;
        self.edit_idx = None;
        self.label.clear();
        self.command.clear();
        self.args.clear();
        self.add_plugin.clear();
        self.add_filter.clear();
    }

    pub fn ensure_open(&mut self) {
        self.ensure_open_from(FAV_FILE);
    }

    fn ensure_open_from(&mut self, path: &str) {
        if !self.open {
            self.open_from(path);
        }
    }

    pub fn open_edit(&mut self, label: &str) {
        if self.load_from(FAV_FILE).is_err() {
            self.edit_idx = None;
            self.open = true;
            return;
        }
        if let Some(pos) = self.entries.iter().position(|e| e.label == label) {
            self.edit_idx = Some(pos);
            let entry = &self.entries[pos];
            self.label = entry.label.clone();
            self.command = entry.action.clone();
            self.args = entry.args.clone().unwrap_or_default();
        } else {
            self.edit_idx = Some(self.entries.len());
            self.label = label.to_string();
            self.command.clear();
            self.args.clear();
        }
        self.open = true;
    }

    /// Open the add form with a launcher's existing action prefilled without
    /// changing the persisted Favorites schema.
    pub fn open_prefilled_add(&mut self, action: &Action) {
        self.open_prefilled_add_from(action, FAV_FILE);
    }

    fn open_prefilled_add_from(&mut self, action: &Action, path: &str) {
        if self.load_from(path).is_err() {
            self.edit_idx = None;
            self.open = true;
            return;
        }
        self.edit_idx = self
            .entries
            .iter()
            .position(|entry| {
                entry.action == action.action && entry.args.as_deref() == action.args.as_deref()
            })
            .or_else(|| {
                self.entries
                    .iter()
                    .position(|entry| entry.label.eq_ignore_ascii_case(&action.label))
            })
            .or(Some(self.entries.len()));
        self.label.clone_from(&action.label);
        self.command.clone_from(&action.action);
        self.args = action.args.clone().unwrap_or_default();
        self.add_plugin.clear();
        self.add_filter.clear();
        self.open = true;
    }

    fn load_from(&mut self, path: &str) -> anyhow::Result<()> {
        match load_favs(path) {
            Ok(entries) => {
                self.entries = entries;
                self.load_error = None;
                Ok(())
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn commit_entries(&mut self, path: &str, candidate: Vec<FavEntry>) -> anyhow::Result<()> {
        match replace_favs(path, candidate) {
            Ok(committed) => {
                self.entries = committed;
                self.load_error = None;
                Ok(())
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn save(&mut self, app: &mut LauncherApp, candidate: Vec<FavEntry>) -> bool {
        if let Err(e) = self.commit_entries(FAV_FILE, candidate) {
            app.report_error_message("ui operation", format!("Failed to save favorites: {e}"));
            false
        } else {
            app.search();
            app.focus_input();
            true
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let pinned = app.pinned_panels.contains(&crate::gui::Panel::FavDialog);
        let mut window_open = self.open;
        let mut close = false;
        let mut save_candidate = None;
        #[cfg(test)]
        let mut test_scroll = None;
        #[cfg(test)]
        let mut test_primary = None;
        #[cfg(test)]
        let mut test_secondary = None;
        #[cfg(test)]
        let mut test_close_clicked = false;
        #[cfg(test)]
        let mut test_visible = false;
        #[cfg(test)]
        let mut test_sizing_pass = false;
        #[cfg(test)]
        let window_id = egui::Id::new("Favorites");
        let window_output = egui::Window::new("Favorites")
            .resizable(true)
            .max_width(ctx.content_rect().width().max(1.0))
            .max_height(ctx.content_rect().height().max(1.0))
            .open(&mut window_open)
            .show(ctx, |ui| {
                #[cfg(test)]
                {
                    test_visible = ui.is_visible();
                    test_sizing_pass = ui.is_sizing_pass();
                }
                if let Some(error) = &self.load_error {
                    compact_section_heading(ui, "Saved favorites", None);
                    semantic_status(
                        ui,
                        StatusTone::Error,
                        "Favorites are read-only because loading failed",
                        Some("The last good entries remain available."),
                    );
                    let footer_height = favorites_footer_reserved_height(ui, &["Close"]);
                    let error_height = (ui.available_height() - footer_height).max(0.0);
                    #[cfg(test)]
                    let parent_max_rect = ui.max_rect();
                    #[cfg(test)]
                    let parent_clip_rect = ui.clip_rect();
                    let _error_output = egui::ScrollArea::vertical()
                        .id_salt("fav_load_error")
                        .auto_shrink([false, false])
                        .min_scrolled_height(0.0)
                        .max_height(error_height)
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(error).wrap());
                        });
                    #[cfg(test)]
                    {
                        test_scroll = Some(FavScrollLayout {
                            id: _error_output.id,
                            parent_max_rect,
                            parent_clip_rect,
                            inner_rect: _error_output.inner_rect,
                            content_size: _error_output.content_size,
                            offset: _error_output.state.offset,
                        });
                    }
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        let close_response = ui.button("Close");
                        let close_response =
                            with_button_emphasis(ui, close_response, ButtonEmphasis::Primary);
                        #[cfg(test)]
                        {
                            test_close_clicked |= close_response.clicked();
                            test_primary = Some(FavWidgetLayout {
                                id: close_response.id,
                                rect: close_response.rect,
                            });
                        }
                        if close_response.clicked() {
                            close = true;
                        }
                    });
                    return;
                }
                if let Some(idx) = self.edit_idx {
                    compact_section_heading(
                        ui,
                        if idx == self.entries.len() {
                            "New favorite"
                        } else {
                            "Edit favorite"
                        },
                        Some("Choose the label, command, optional arguments, and plugin."),
                    );
                    let footer_height = favorites_footer_reserved_height(ui, &["Save", "Cancel"]);
                    let edit_height = (ui.available_height() - footer_height).max(0.0);
                    #[cfg(test)]
                    let parent_max_rect = ui.max_rect();
                    #[cfg(test)]
                    let parent_clip_rect = ui.clip_rect();
                    let _edit_output = egui::ScrollArea::vertical()
                        .id_salt("fav_edit_form")
                        .auto_shrink([false, false])
                        .min_scrolled_height(0.0)
                        .max_height(edit_height)
                        .show(ui, |ui| {
                            labeled_form_row(ui, "Label", |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.label)
                                        .desired_width(ui.available_width().max(72.0)),
                                )
                            });
                            labeled_form_row(ui, "Command", |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.command)
                                        .desired_width(ui.available_width().max(72.0)),
                                )
                            });
                            labeled_form_row(ui, "Args", |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.args)
                                        .desired_width(ui.available_width().max(72.0)),
                                )
                            });
                            compact_section_heading(ui, "Plugin picker", None);
                            secondary_help(
                                ui,
                                "Filter a plugin's actions or choose one to fill the command.",
                            );
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Category");
                                let mut plugin_names: Vec<_> =
                                    app.plugins.iter().map(|p| p.name().to_string()).collect();
                                plugin_names.sort_unstable();
                                egui::ComboBox::from_id_salt("fav_cat")
                                    .selected_text(if self.add_plugin.is_empty() {
                                        "Select".to_string()
                                    } else {
                                        self.add_plugin.clone()
                                    })
                                    .show_ui(ui, |ui| {
                                        for name in plugin_names.iter() {
                                            ui.selectable_value(
                                                &mut self.add_plugin,
                                                name.to_string(),
                                                name,
                                            );
                                        }
                                    });
                            });
                            labeled_form_row(ui, "Filter", |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.add_filter)
                                        .desired_width(ui.available_width().max(72.0)),
                                )
                            });
                            if let Some(plugin) =
                                app.plugins.iter().find(|p| p.name() == self.add_plugin)
                            {
                                let filter = self.add_filter.trim().to_lowercase();
                                let mut actions = if plugin.name() == "folders" {
                                    plugin.search(&format!("f {}", self.add_filter))
                                } else if plugin.name() == "bookmarks" {
                                    plugin.search(&format!("bm {}", self.add_filter))
                                } else {
                                    plugin.commands()
                                };
                                let picker_height = ui.available_height().min(160.0).max(0.0);
                                egui::ScrollArea::vertical()
                                    .id_salt("fav_plugin_actions")
                                    .auto_shrink([false, false])
                                    .min_scrolled_height(0.0)
                                    .max_height(picker_height)
                                    .show(ui, |ui| {
                                        for act in actions.drain(..) {
                                            if !filter.is_empty()
                                                && !act.label.to_lowercase().contains(&filter)
                                                && !act.desc.to_lowercase().contains(&filter)
                                                && !act.action.to_lowercase().contains(&filter)
                                            {
                                                continue;
                                            }
                                            let action = ui.add(
                                                egui::Button::new(format!(
                                                    "{} - {}",
                                                    act.label, act.desc
                                                ))
                                                .wrap(),
                                            );
                                            if action.clicked() {
                                                let mut cmd = act.action.clone();
                                                let mut args = if self.args.trim().is_empty() {
                                                    None
                                                } else {
                                                    Some(self.args.clone())
                                                };
                                                if let Some(q) = cmd.strip_prefix("query:") {
                                                    let q = join_command_args(q, args.as_deref());
                                                    if let Some(res) =
                                                        plugin.search(&q).into_iter().next()
                                                    {
                                                        cmd = res.action;
                                                        args = res.args;
                                                    } else {
                                                        cmd = q;
                                                        args = None;
                                                    }
                                                }
                                                self.command = cmd;
                                                self.args = args.unwrap_or_default();
                                            }
                                        }
                                    });
                            }
                        });
                    #[cfg(test)]
                    {
                        test_scroll = Some(FavScrollLayout {
                            id: _edit_output.id,
                            parent_max_rect,
                            parent_clip_rect,
                            inner_rect: _edit_output.inner_rect,
                            content_size: _edit_output.content_size,
                            offset: _edit_output.state.offset,
                        });
                    }
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        let save = ui.button("Save");
                        let save = with_button_emphasis(ui, save, ButtonEmphasis::Primary);
                        #[cfg(test)]
                        {
                            test_primary = Some(FavWidgetLayout {
                                id: save.id,
                                rect: save.rect,
                            });
                        }
                        if save.clicked() {
                            if self.label.trim().is_empty() || self.command.trim().is_empty() {
                                app.report_error_message(
                                    "ui operation",
                                    "Label and command required",
                                );
                            } else {
                                let mut cmd = self.command.clone();
                                let mut args_opt = if self.args.trim().is_empty() {
                                    None
                                } else {
                                    Some(self.args.clone())
                                };
                                if let Some(plugin) =
                                    app.plugins.iter().find(|p| p.name() == self.add_plugin)
                                {
                                    let (c, a) = resolve_with_plugin(
                                        plugin.as_ref(),
                                        &cmd,
                                        args_opt.as_deref(),
                                    );
                                    cmd = c;
                                    args_opt = a;
                                }
                                let mut candidate = self.entries.clone();
                                if idx == candidate.len() {
                                    candidate.push(FavEntry {
                                        label: self.label.clone(),
                                        action: cmd.clone(),
                                        args: args_opt.clone(),
                                    });
                                } else if let Some(e) = candidate.get_mut(idx) {
                                    e.label = self.label.clone();
                                    e.action = cmd.clone();
                                    e.args = args_opt.clone();
                                }
                                save_candidate = Some(candidate);
                            }
                        }
                        let cancel = ui.button("Cancel");
                        #[cfg(test)]
                        {
                            test_secondary = Some(FavWidgetLayout {
                                id: cancel.id,
                                rect: cancel.rect,
                            });
                        }
                        if cancel.clicked() {
                            self.edit_idx = None;
                            self.add_plugin.clear();
                            self.add_filter.clear();
                        }
                    });
                } else {
                    compact_section_heading(
                        ui,
                        "Saved favorites",
                        Some("Manage saved labels and launch commands."),
                    );
                    ui.horizontal_wrapped(|ui| {
                        let add = ui.button("Add Fav");
                        let add = with_button_emphasis(ui, add, ButtonEmphasis::Primary);
                        #[cfg(test)]
                        {
                            test_primary = Some(FavWidgetLayout {
                                id: add.id,
                                rect: add.rect,
                            });
                        }
                        if add.clicked() {
                            self.edit_idx = Some(self.entries.len());
                            self.label.clear();
                            self.command.clear();
                            self.args.clear();
                            self.add_plugin.clear();
                            self.add_filter.clear();
                        }
                        let close_response = ui.button("Close");
                        #[cfg(test)]
                        {
                            test_close_clicked |= close_response.clicked();
                            test_secondary = Some(FavWidgetLayout {
                                id: close_response.id,
                                rect: close_response.rect,
                            });
                        }
                        if close_response.clicked() {
                            close = true;
                        }
                    });
                    let list_height = ui.available_height().max(0.0);
                    let mut remove: Option<usize> = None;
                    #[cfg(test)]
                    let parent_max_rect = ui.max_rect();
                    #[cfg(test)]
                    let parent_clip_rect = ui.clip_rect();
                    let _list_output = egui::ScrollArea::vertical()
                        .id_salt("fav_browse_list")
                        .auto_shrink([false, false])
                        .min_scrolled_height(0.0)
                        .max_height(list_height)
                        .show(ui, |ui| {
                            for idx in 0..self.entries.len() {
                                let entry = self.entries[idx].clone();
                                ui.horizontal_wrapped(|ui| {
                                    if ui.button("Edit").clicked() {
                                        self.edit_idx = Some(idx);
                                        self.label = entry.label.clone();
                                        self.command = entry.action.clone();
                                        self.args = entry.args.clone().unwrap_or_default();
                                    }
                                    ui.add(
                                        egui::Label::new(format!(
                                            "{} - {}",
                                            entry.label, entry.action
                                        ))
                                        .wrap(),
                                    );
                                    let remove_response = ui.button("Remove");
                                    if with_button_emphasis(
                                        ui,
                                        remove_response,
                                        ButtonEmphasis::Destructive,
                                    )
                                    .clicked()
                                    {
                                        remove = Some(idx);
                                    }
                                });
                            }
                        });
                    #[cfg(test)]
                    {
                        test_scroll = Some(FavScrollLayout {
                            id: _list_output.id,
                            parent_max_rect,
                            parent_clip_rect,
                            inner_rect: _list_output.inner_rect,
                            content_size: _list_output.content_size,
                            offset: _list_output.state.offset,
                        });
                    }
                    if let Some(idx) = remove {
                        let mut candidate = self.entries.clone();
                        candidate.remove(idx);
                        save_candidate = Some(candidate);
                    }
                }
            });
        #[cfg(test)]
        {
            self.last_layout = match (window_output, test_scroll, test_primary, test_secondary) {
                (Some(window), Some(scroll), Some(primary), Some(secondary)) => {
                    Some(FavDialogLayout {
                        window_rect: ctx
                            .memory(|memory| memory.area_rect(window_id))
                            .unwrap_or(window.response.rect),
                        scroll,
                        primary,
                        secondary,
                        close_clicked: test_close_clicked,
                        visible: test_visible,
                        sizing_pass: test_sizing_pass,
                    })
                }
                _ => None,
            };
        }
        #[cfg(not(test))]
        let _ = window_output;
        if let Some(candidate) = save_candidate
            && self.save(app, candidate)
        {
            self.edit_idx = None;
            self.label.clear();
            self.command.clear();
            self.args.clear();
            self.add_plugin.clear();
            self.add_filter.clear();
        }
        self.open = window_open || pinned;
        if close && !pinned {
            self.open = false;
        }
    }
}

fn favorites_footer_reserved_height(ui: &mut egui::Ui, labels: &[&str]) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let font_id = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| egui::TextStyle::Body.resolve(&style));
    let button_frame = style
        .button_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Inactive,
        )
        .frame;
    let button_height = (ui.fonts_mut(|fonts| fonts.row_height(&font_id))
        + spacing.extra_text_line_spacing
        + button_frame.total_margin().sum().y)
        .max(spacing.interact_size.y);
    let available_width = ui.available_width().max(1.0);
    let text_color = ui.visuals().text_color();
    let mut row_width = 0.0;
    let mut row_count = 1_usize;
    for label in labels {
        let text_width = ui.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap((*label).to_owned(), font_id.clone(), text_color)
                .size()
                .x
        });
        let button_width =
            (text_width + button_frame.total_margin().sum().x).max(spacing.interact_size.x);
        if row_width > 0.0 && row_width + spacing.item_spacing.x + button_width > available_width {
            row_count += 1;
            row_width = button_width;
        } else {
            if row_width > 0.0 {
                row_width += spacing.item_spacing.x;
            }
            row_width += button_width;
        }
    }
    let separator_height = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let height = row_count as f32 * button_height
        + row_count.saturating_sub(1) as f32 * spacing.item_spacing.y
        + 2.0 * spacing.item_spacing.y
        + separator_height;
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    (height * pixels_per_point).ceil() / pixels_per_point
}

#[cfg(test)]
mod tests {
    use super::{FavDialog, FavWidgetLayout};
    use crate::actions::Action;
    use crate::plugins::fav::{FavEntry, save_favs};
    use eframe::egui;

    fn test_app(ctx: &egui::Context, root: &std::path::Path) -> crate::gui::LauncherApp {
        use crate::plugin::PluginManager;
        use crate::settings::Settings;
        use std::sync::{Arc, atomic::AtomicBool};

        crate::gui::LauncherApp::new(
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

    fn dialog_frame(
        ctx: &egui::Context,
        dialog: &mut FavDialog,
        app: &mut crate::gui::LauncherApp,
        screen_size: egui::Vec2,
    ) -> egui::FullOutput {
        dialog_frame_with_events(ctx, dialog, app, screen_size, Vec::new())
    }

    fn dialog_frame_with_events(
        ctx: &egui::Context,
        dialog: &mut FavDialog,
        app: &mut crate::gui::LauncherApp,
        screen_size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size)),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| dialog.ui(ctx, app),
        )
    }

    fn click_dialog_widget(
        ctx: &egui::Context,
        dialog: &mut FavDialog,
        app: &mut crate::gui::LauncherApp,
        screen_size: egui::Vec2,
        rect: egui::Rect,
    ) {
        let point = rect.center();
        dialog_frame_with_events(
            ctx,
            dialog,
            app,
            screen_size,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        )
        .drop_without_applying_deltas();
        dialog_frame_with_events(
            ctx,
            dialog,
            app,
            screen_size,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        )
        .drop_without_applying_deltas();
    }

    fn assert_widget_inside_bounds(
        layout: FavWidgetLayout,
        window: egui::Rect,
        screen: egui::Rect,
        parent_clip: egui::Rect,
    ) {
        assert!(
            window.contains_rect(layout.rect),
            "{:?} outside {window:?}",
            layout.rect
        );
        assert!(
            screen.contains_rect(layout.rect),
            "{:?} outside screen {screen:?}",
            layout.rect
        );
        assert!(
            parent_clip.intersects(layout.rect),
            "{:?} does not intersect parent clip {parent_clip:?}",
            layout.rect
        );
        assert!(
            parent_clip.contains_rect(layout.rect),
            "{:?} is not fully inside parent clip {parent_clip:?}",
            layout.rect
        );
    }

    fn fav(label: &str) -> FavEntry {
        FavEntry {
            label: label.into(),
            action: format!("noop:{label}"),
            args: Some("--arg".into()),
        }
    }

    #[test]
    fn favorites_pinned_open_loads_once_and_preserves_draft_until_close() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fav.json");
        let path_text = path.to_str().unwrap();
        let initial = (0..24)
            .map(|index| FavEntry {
                label: format!("Favorite {index}"),
                action: format!("query:search-workspace-with-a-long-label-{index}"),
                args: Some(format!("--window {index}")),
            })
            .collect::<Vec<_>>();
        save_favs(path_text, &initial).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        let ctx = egui::Context::default();
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        let mut app = test_app(&ctx, directory.path());
        app.pinned_panels.push(crate::gui::Panel::FavDialog);
        let mut dialog = FavDialog::default();

        dialog.ensure_open_from(path_text);
        assert!(dialog.open);
        assert_eq!(dialog.entries, initial);

        dialog.edit_idx = Some(0);
        dialog.label = "unsaved label draft".into();
        dialog.command = "unsaved command draft".into();
        dialog.add_plugin = "folders".into();
        dialog.add_filter = "work".into();
        dialog.ensure_open_from(path_text);
        assert_eq!(dialog.entries, initial);
        assert_eq!(dialog.edit_idx, Some(0));
        assert_eq!(dialog.label, "unsaved label draft");
        assert_eq!(dialog.command, "unsaved command draft");
        assert_eq!(dialog.add_plugin, "folders");
        assert_eq!(dialog.add_filter, "work");

        dialog.edit_idx = None;
        let screen_size = egui::vec2(380.0, 240.0);
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size);
        for _ in 0..3 {
            dialog_frame(&ctx, &mut dialog, &mut app, screen_size).drop_without_applying_deltas();
        }
        let browse = dialog
            .last_layout
            .expect("production Favorites browse layout");
        assert!(browse.visible);
        assert!(!browse.sizing_pass);
        assert!(screen_rect.contains_rect(browse.window_rect));
        assert_widget_inside_bounds(
            browse.primary,
            browse.window_rect,
            screen_rect,
            browse.scroll.parent_clip_rect,
        );
        assert_widget_inside_bounds(
            browse.secondary,
            browse.window_rect,
            screen_rect,
            browse.scroll.parent_clip_rect,
        );
        assert!(browse.primary.id != browse.secondary.id);
        assert!(browse.scroll.inner_rect.is_positive());
        assert!(
            browse
                .scroll
                .parent_max_rect
                .contains_rect(browse.scroll.inner_rect)
        );
        assert!(
            browse
                .scroll
                .parent_clip_rect
                .intersects(browse.scroll.inner_rect)
        );
        assert!(browse.scroll.content_size.y > browse.scroll.inner_rect.height());
        let max_browse_offset =
            (browse.scroll.content_size.y - browse.scroll.inner_rect.height()).max(0.0);
        assert!(browse.scroll.offset.y >= 0.0 && browse.scroll.offset.y <= max_browse_offset);
        assert_ne!(browse.scroll.id, browse.primary.id);

        click_dialog_widget(
            &ctx,
            &mut dialog,
            &mut app,
            screen_size,
            browse.secondary.rect,
        );
        let pinned_close = dialog.last_layout.expect("pinned close response layout");
        assert!(
            pinned_close.close_clicked,
            "Close should register a native click"
        );
        assert!(pinned_close.visible && !pinned_close.sizing_pass);
        assert!(
            dialog.open,
            "a pinned Close click should not close Favorites"
        );
        assert_eq!(dialog.entries, initial);
        assert_eq!(dialog.label, "unsaved label draft");
        assert_eq!(dialog.command, "unsaved command draft");
        assert_eq!(dialog.add_plugin, "folders");
        assert_eq!(dialog.add_filter, "work");
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert!(screen_rect.contains_rect(pinned_close.window_rect));
        assert_widget_inside_bounds(
            pinned_close.secondary,
            pinned_close.window_rect,
            screen_rect,
            pinned_close.scroll.parent_clip_rect,
        );
        dialog.ensure_open_from(path_text);
        assert_eq!(dialog.label, "unsaved label draft");
        assert_eq!(dialog.command, "unsaved command draft");

        dialog.edit_idx = Some(0);
        for _ in 0..3 {
            dialog_frame(&ctx, &mut dialog, &mut app, screen_size).drop_without_applying_deltas();
        }
        let edit = dialog
            .last_layout
            .expect("production Favorites edit layout");
        assert!(edit.visible);
        assert!(!edit.sizing_pass);
        assert!(screen_rect.contains_rect(edit.window_rect));
        assert_widget_inside_bounds(
            edit.primary,
            edit.window_rect,
            screen_rect,
            edit.scroll.parent_clip_rect,
        );
        assert_widget_inside_bounds(
            edit.secondary,
            edit.window_rect,
            screen_rect,
            edit.scroll.parent_clip_rect,
        );
        assert!(edit.primary.id != edit.secondary.id);
        assert!(edit.scroll.inner_rect.is_positive());
        assert!(
            edit.scroll
                .parent_max_rect
                .contains_rect(edit.scroll.inner_rect)
        );
        assert!(
            edit.scroll
                .parent_clip_rect
                .intersects(edit.scroll.inner_rect)
        );
        assert!(edit.scroll.inner_rect.max.y <= edit.primary.rect.min.y);
        assert_ne!(edit.scroll.id, edit.primary.id);
        assert_eq!(dialog.entries, initial);
        assert_eq!(dialog.label, "unsaved label draft");
        assert_eq!(dialog.command, "unsaved command draft");
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);

        dialog.edit_idx = None;
        app.pinned_panels
            .retain(|panel| *panel != crate::gui::Panel::FavDialog);
        for _ in 0..2 {
            dialog_frame(&ctx, &mut dialog, &mut app, screen_size).drop_without_applying_deltas();
        }
        let unpinned_browse = dialog.last_layout.expect("unpinned browse layout");
        click_dialog_widget(
            &ctx,
            &mut dialog,
            &mut app,
            screen_size,
            unpinned_browse.secondary.rect,
        );
        let unpinned_close = dialog.last_layout.expect("unpinned close response layout");
        assert!(
            unpinned_close.close_clicked,
            "Close should register a native click"
        );
        assert!(screen_rect.contains_rect(unpinned_close.window_rect));
        assert_widget_inside_bounds(
            unpinned_close.secondary,
            unpinned_close.window_rect,
            screen_rect,
            unpinned_close.scroll.parent_clip_rect,
        );
        assert!(
            !dialog.open,
            "an unpinned Close click should close Favorites"
        );

        let mut changed = initial.clone();
        changed.push(fav("newly persisted"));
        save_favs(path_text, &changed).unwrap();
        dialog.ensure_open_from(path_text);
        assert!(dialog.open);
        assert_eq!(dialog.entries, changed);
        assert_eq!(dialog.edit_idx, None);
        assert!(dialog.label.is_empty());
        assert!(dialog.command.is_empty());
        assert!(dialog.add_plugin.is_empty());
        assert!(dialog.add_filter.is_empty());
    }

    #[test]
    fn invalid_reload_and_commit_keep_dialog_last_good_and_read_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fav.json");
        let initial = vec![fav("saved")];
        save_favs(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = FavDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        let invalid = b"invalid favorites";
        std::fs::write(&path, invalid).unwrap();

        assert!(dialog.load_from(path.to_str().unwrap()).is_err());
        assert_eq!(dialog.entries, initial);
        assert!(dialog.load_error.is_some());
        assert!(
            dialog
                .commit_entries(path.to_str().unwrap(), vec![fav("lost")])
                .is_err()
        );
        assert_eq!(dialog.entries, initial);
        assert_eq!(std::fs::read(path).unwrap(), invalid);
        assert!(dialog.load_error.is_some());
    }

    #[test]
    fn prefilled_add_preserves_legacy_favorite_fields() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fav.json");
        save_favs(path.to_str().unwrap(), &[fav("saved")]).unwrap();
        let action = Action {
            label: "Project".into(),
            desc: "Folder".into(),
            action: r#"C:\work\project"#.into(),
            args: Some("--reuse-window".into()),
        };
        let mut dialog = FavDialog::default();

        dialog.open_prefilled_add_from(&action, path.to_str().unwrap());

        assert!(dialog.open);
        assert_eq!(dialog.edit_idx, Some(1));
        assert_eq!(dialog.label, action.label);
        assert_eq!(dialog.command, action.action);
        assert_eq!(dialog.args, action.args.unwrap());
    }

    #[test]
    fn prefilled_add_edits_exact_command_or_case_insensitive_label_match() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fav.json");
        let entries = vec![
            FavEntry {
                label: "Other label".into(),
                action: "same:command".into(),
                args: Some("--same".into()),
            },
            FavEntry {
                label: "PROJECT".into(),
                action: "old:command".into(),
                args: None,
            },
        ];
        save_favs(path.to_str().unwrap(), &entries).unwrap();
        let mut dialog = FavDialog::default();

        dialog.open_prefilled_add_from(
            &Action {
                label: "New label".into(),
                desc: String::new(),
                action: "same:command".into(),
                args: Some("--same".into()),
            },
            path.to_str().unwrap(),
        );
        assert_eq!(dialog.edit_idx, Some(0));

        dialog.open_prefilled_add_from(
            &Action {
                label: "project".into(),
                desc: String::new(),
                action: "new:command".into(),
                args: None,
            },
            path.to_str().unwrap(),
        );
        assert_eq!(dialog.edit_idx, Some(1));
        assert_eq!(dialog.command, "new:command");
    }
}
