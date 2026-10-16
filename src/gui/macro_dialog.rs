use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, labeled_form_row, semantic_status,
    with_button_emphasis,
};
use crate::plugins::macros::{MACROS_FILE, MacroEntry, MacroStep, load_macros, replace_macros};
use eframe::egui;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use log::debug;

/// Dialog for creating and editing macros.
///
/// `category_filter` stores user input for a fuzzy search over plugin names,
/// including a special `app` category listing all configured applications.
/// Matching names are shown in a scrollable list; selecting one writes it to
/// `add_plugin` and clears the filter.
pub struct MacroDialog {
    pub open: bool,
    entries: Vec<MacroEntry>,
    edit_idx: Option<usize>,
    label: String,
    desc: String,
    steps: Vec<MacroStep>,
    auto_delay: bool,
    auto_delay_secs: f32,
    add_plugin: String,
    category_filter: String,
    add_filter: String,
    add_args: String,
    load_error: Option<String>,
    #[cfg(test)]
    test_layout: Option<MacroLayoutObservation>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct MacroScrollLayout {
    id: egui::Id,
    parent_max_rect: egui::Rect,
    parent_clip_rect: egui::Rect,
    inner_rect: egui::Rect,
    content_size: egui::Vec2,
    offset: egui::Vec2,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct MacroLayoutObservation {
    scroll: MacroScrollLayout,
    picker_scroll: Option<MacroScrollLayout>,
    primary_rect: egui::Rect,
    secondary_rect: egui::Rect,
    window_rect: egui::Rect,
    visible: bool,
    sizing_pass: bool,
    close_requested: bool,
}

impl Default for MacroDialog {
    fn default() -> Self {
        Self {
            open: false,
            entries: Vec::new(),
            edit_idx: None,
            label: String::new(),
            desc: String::new(),
            steps: Vec::new(),
            auto_delay: false,
            auto_delay_secs: 1.0,
            add_plugin: String::new(),
            category_filter: String::new(),
            add_filter: String::new(),
            add_args: String::new(),
            load_error: None,
            #[cfg(test)]
            test_layout: None,
        }
    }
}

impl MacroDialog {
    /// Load macros and reset dialog state, including the category filter.
    pub fn open(&mut self) {
        let _ = self.load_from(MACROS_FILE);
        self.open = true;
        self.edit_idx = None;
        self.label.clear();
        self.desc.clear();
        self.steps.clear();
        self.auto_delay = false;
        self.auto_delay_secs = 1.0;
        self.add_plugin.clear();
        self.category_filter.clear();
        self.add_filter.clear();
        self.add_args.clear();
    }

    /// Ensure the dialog is open, loading persisted entries only on a closed
    /// to open transition. Pinned panels call this on each application frame.
    pub fn ensure_open(&mut self) {
        if !self.open {
            self.open();
        }
    }

    fn load_from(&mut self, path: &str) -> anyhow::Result<()> {
        match load_macros(path) {
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

    fn commit_entries(&mut self, path: &str, candidate: Vec<MacroEntry>) -> anyhow::Result<()> {
        match replace_macros(path, candidate) {
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

    fn save(&mut self, app: &mut LauncherApp, candidate: Vec<MacroEntry>) -> bool {
        if let Err(e) = self.commit_entries(MACROS_FILE, candidate) {
            app.report_error_message("ui operation", format!("Failed to save macros: {e}"));
            false
        } else {
            app.search();
            app.focus_input();
            true
        }
    }

    /// Return plugin names sorted alphabetically and filtered by `filter`.
    fn matching_plugins<'a>(filter: &str, names: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
        let matcher = SkimMatcherV2::default();
        let mut names: Vec<&'a str> = names.collect();
        let total = names.len();
        let filter = filter.to_lowercase();
        names.sort_unstable();
        let filtered: Vec<&'a str> = names
            .into_iter()
            .filter(|name| {
                filter.is_empty() || matcher.fuzzy_match(&name.to_lowercase(), &filter).is_some()
            })
            .collect();
        if !filter.is_empty() {
            debug!(
                "matching_plugins: filter '{filter}' returned {} of {total}",
                filtered.len()
            );
        }
        filtered
    }

    /// Record the selected plugin and clear the category filter.
    fn select_plugin(add_plugin: &mut String, category_filter: &mut String, name: &str) {
        debug!("select_plugin: {name}");
        *add_plugin = name.to_string();
        category_filter.clear();
    }

    /// Render the dialog, including the fuzzy plugin picker used when adding
    /// macro steps. Plugin names are filtered with `SkimMatcherV2`; choosing a
    /// name stores it in `add_plugin` and clears `category_filter`.
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let pinned = app.pinned_panels.contains(&crate::gui::Panel::MacroDialog);
        let mut close = false;
        let mut save_candidate = None;
        let mut open = self.open;
        #[cfg(test)]
        let mut test_layout = None;
        #[cfg(test)]
        let mut test_visible = false;
        #[cfg(test)]
        let mut test_sizing_pass = false;
        let _window_output = egui::Window::new("Macros")
            .open(&mut open)
            .resizable(true)
            .max_width(ctx.content_rect().width().max(1.0))
            .max_height(ctx.content_rect().height().max(1.0))
            .show(ctx, |ui| {
                #[cfg(test)]
                {
                    test_visible = ui.is_visible();
                    test_sizing_pass = ui.is_sizing_pass();
                }
                if let Some(error) = &self.load_error {
                    compact_section_heading(
                        ui,
                        "Saved macros",
                        Some("The current macro file could not be loaded."),
                    );
                    semantic_status(
                        ui,
                        StatusTone::Error,
                        "Macros are read-only",
                        Some("Loading failed; the last good entries are retained."),
                    );
                    let footer_height = macro_footer_reserved_height(ui, &["Close"]);
                    let parent_max_rect = ui.max_rect();
                    let parent_clip_rect = ui.clip_rect();
                    let body_height = (ui.available_height() - footer_height).max(0.0);
                    let error_output = egui::ScrollArea::vertical()
                        .id_salt("macro_load_error")
                        .auto_shrink([false, false])
                        .min_scrolled_height(0.0)
                        .max_height(body_height)
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(error).wrap());
                        });
                    ui.separator();
                    let mut close_rect = egui::Rect::NOTHING;
                    ui.horizontal_wrapped(|ui| {
                        let close_button = ui.button("Close");
                        let close_button =
                            with_button_emphasis(ui, close_button, ButtonEmphasis::Primary);
                        close_rect = close_button.rect;
                        if close_button.clicked() {
                            close = true;
                        }
                    });
                    #[cfg(test)]
                    {
                        test_layout = Some(MacroLayoutObservation {
                            scroll: macro_scroll_layout(
                                error_output.id,
                                parent_max_rect,
                                parent_clip_rect,
                                error_output.inner_rect,
                                error_output.content_size,
                                error_output.state.offset,
                            ),
                            picker_scroll: None,
                            primary_rect: close_rect,
                            secondary_rect: egui::Rect::NOTHING,
                            window_rect: egui::Rect::NOTHING,
                            visible: test_visible,
                            sizing_pass: test_sizing_pass,
                            close_requested: close,
                        });
                    }
                    #[cfg(not(test))]
                    let _ = (error_output, parent_max_rect, parent_clip_rect, close_rect);
                } else if let Some(idx) = self.edit_idx {
                    compact_section_heading(
                        ui,
                        if idx == self.entries.len() {
                            "New macro"
                        } else {
                            "Edit macro"
                        },
                        Some("Describe the macro and arrange its steps."),
                    );
                    let footer_height = macro_footer_reserved_height(ui, &["Save", "Cancel"]);
                    let parent_max_rect = ui.max_rect();
                    let parent_clip_rect = ui.clip_rect();
                    let body_height = (ui.available_height() - footer_height).max(0.0);
                    #[cfg(test)]
                    let mut picker_scroll = None;
                    let edit_output = egui::ScrollArea::vertical()
                        .id_salt("macro_edit_scroll")
                        .auto_shrink([false, false])
                        .min_scrolled_height(0.0)
                        .max_height(body_height)
                        .show(ui, |ui| {
                            compact_section_heading(ui, "Identity", None);
                            labeled_form_row(ui, "Label", |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.label)
                                        .desired_width(ui.available_width().max(72.0)),
                                )
                            });
                            labeled_form_row(ui, "Description", |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.desc)
                                        .desired_width(ui.available_width().max(72.0)),
                                )
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.checkbox(&mut self.auto_delay, "Automatic delay");
                                if self.auto_delay {
                                    let mut auto_delay_secs = self.auto_delay_secs;
                                    let delay = ui.add(
                                        egui::DragValue::new(&mut auto_delay_secs)
                                            .speed(0.1)
                                            .range(0.0..=60.0)
                                            .suffix("s"),
                                    );
                                    if delay.changed() {
                                        self.auto_delay_secs = auto_delay_secs;
                                    }
                                }
                            });

                            compact_section_heading(ui, "Steps", None);
                            let mut remove_step: Option<usize> = None;
                            let mut move_up: Option<usize> = None;
                            let mut move_down: Option<usize> = None;
                            for i in 0..self.steps.len() {
                                ui.push_id(("macro_step", i), |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(format!("{}.", i + 1));
                                        ui.add(
                                            egui::Label::new(&self.steps[i].label)
                                                .wrap()
                                                .selectable(false),
                                        );
                                        let mut args =
                                            self.steps[i].args.clone().unwrap_or_default();
                                        let args_response = ui.add(
                                            egui::TextEdit::singleline(&mut args)
                                                .hint_text("Arguments")
                                                .desired_width(
                                                    ui.available_width().min(180.0).max(72.0),
                                                ),
                                        );
                                        if args_response.changed() {
                                            self.steps[i].args = Some(args);
                                        }
                                        if !self.auto_delay {
                                            let mut seconds =
                                                self.steps[i].delay_ms as f32 / 1000.0;
                                            let delay_response = ui.add(
                                                egui::DragValue::new(&mut seconds)
                                                    .speed(0.1)
                                                    .range(0.0..=60.0)
                                                    .suffix("s"),
                                            );
                                            if delay_response.changed() {
                                                self.steps[i].delay_ms = (seconds * 1000.0) as u64;
                                            }
                                        }
                                        if ui.button("Up").clicked() {
                                            move_up = Some(i);
                                        }
                                        if ui.button("Down").clicked() {
                                            move_down = Some(i);
                                        }
                                        let remove_button = ui.button("Remove");
                                        if with_button_emphasis(
                                            ui,
                                            remove_button,
                                            ButtonEmphasis::Destructive,
                                        )
                                        .clicked()
                                        {
                                            remove_step = Some(i);
                                        }
                                    });
                                });
                            }
                            if let Some(i) = move_up
                                && i > 0
                            {
                                self.steps.swap(i, i - 1);
                            }
                            if let Some(i) = move_down
                                && i + 1 < self.steps.len()
                            {
                                self.steps.swap(i, i + 1);
                            }
                            if let Some(i) = remove_step {
                                self.steps.remove(i);
                            }

                            compact_section_heading(ui, "Add a step", None);
                            labeled_form_row(ui, "Category", |ui| {
                                // Free-form text input used to fuzzily match plugin names.
                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut self.category_filter)
                                        .desired_width(ui.available_width().max(72.0)),
                                );
                                if response.changed() {
                                    debug!("category_filter set to '{}'", self.category_filter);
                                }
                                response
                            });
                            // Collect plugin names matching the fuzzy category filter,
                            // including the special `app` category.
                            let available_plugin_names = app.plugins.plugin_names();
                            let plugin_names = MacroDialog::matching_plugins(
                                &self.category_filter,
                                available_plugin_names
                                    .iter()
                                    .map(String::as_str)
                                    .chain(std::iter::once("app")),
                            );
                            let _plugin_output = egui::ScrollArea::vertical()
                                .id_salt("macro_plugin_list")
                                .auto_shrink([false, false])
                                .max_height(100.0)
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        for name in plugin_names {
                                            // Choosing a plugin stores it and clears the filter.
                                            let response = ui.button(name);
                                            if response.clicked() {
                                                debug!("selected plugin {name}");
                                                MacroDialog::select_plugin(
                                                    &mut self.add_plugin,
                                                    &mut self.category_filter,
                                                    name,
                                                );
                                            }
                                        }
                                    });
                                });
                            labeled_form_row(ui, "Filter", |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.add_filter)
                                        .desired_width(ui.available_width().max(72.0)),
                                )
                            });
                            labeled_form_row(ui, "Arguments", |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.add_args)
                                        .desired_width(ui.available_width().max(72.0)),
                                )
                            });
                            if self.add_plugin == "app" {
                                let filter = self.add_filter.trim().to_lowercase();
                                let _picker_max_rect = ui.max_rect();
                                let _picker_clip_rect = ui.clip_rect();
                                let app_output = egui::ScrollArea::vertical()
                                    .id_salt("macro_app_list")
                                    .auto_shrink([false, false])
                                    .max_height(100.0)
                                    .show(ui, |ui| {
                                        for act in app.actions.iter() {
                                            if !filter.is_empty()
                                                && !act.label.to_lowercase().contains(&filter)
                                                && !act.desc.to_lowercase().contains(&filter)
                                                && !act.action.to_lowercase().contains(&filter)
                                            {
                                                continue;
                                            }
                                            ui.horizontal_wrapped(|ui| {
                                                ui.vertical(|ui| {
                                                    ui.label(&act.label);
                                                    ui.add(
                                                        egui::Label::new(&act.desc)
                                                            .wrap()
                                                            .selectable(false),
                                                    );
                                                });
                                                if ui.button("Add").clicked() {
                                                    let args = if self.add_args.trim().is_empty() {
                                                        act.args.clone()
                                                    } else {
                                                        Some(self.add_args.clone())
                                                    };
                                                    self.steps.push(MacroStep {
                                                        label: act.label.clone(),
                                                        command: act.action.clone(),
                                                        args,
                                                        delay_ms: 0,
                                                    });
                                                    self.add_args.clear();
                                                }
                                            });
                                            ui.add_space(ui.spacing().item_spacing.y);
                                        }
                                    });
                                #[cfg(test)]
                                {
                                    picker_scroll = Some(macro_scroll_layout(
                                        app_output.id,
                                        _picker_max_rect,
                                        _picker_clip_rect,
                                        app_output.inner_rect,
                                        app_output.content_size,
                                        app_output.state.offset,
                                    ));
                                }
                                #[cfg(not(test))]
                                let _ = app_output;
                            } else if let Some(plugin) =
                                app.plugins.iter().find(|p| p.name() == self.add_plugin)
                            {
                                let filter = self.add_filter.trim().to_lowercase();
                                let mut actions = if plugin.name() == "folders" {
                                    plugin.search(&format!("f list {}", self.add_filter))
                                } else if plugin.name() == "bookmarks" {
                                    plugin.search(&format!("bm list {}", self.add_filter))
                                } else {
                                    plugin.commands()
                                };
                                let _action_output = egui::ScrollArea::vertical()
                                    .id_salt("macro_action_list")
                                    .auto_shrink([false, false])
                                    .max_height(100.0)
                                    .show(ui, |ui| {
                                        for act in actions.drain(..) {
                                            if !filter.is_empty()
                                                && !act.label.to_lowercase().contains(&filter)
                                                && !act.desc.to_lowercase().contains(&filter)
                                                && !act.action.to_lowercase().contains(&filter)
                                            {
                                                continue;
                                            }
                                            ui.horizontal_wrapped(|ui| {
                                                ui.vertical(|ui| {
                                                    ui.label(&act.label);
                                                    ui.add(
                                                        egui::Label::new(&act.desc)
                                                            .wrap()
                                                            .selectable(false),
                                                    );
                                                });
                                                if ui.button("Add").clicked() {
                                                    let mut command = act.action.clone();
                                                    let mut args =
                                                        if self.add_args.trim().is_empty() {
                                                            None
                                                        } else {
                                                            Some(self.add_args.clone())
                                                        };

                                                    if let Some(q) = command.strip_prefix("query:")
                                                    {
                                                        let mut q = q.to_string();
                                                        if let Some(ref a) = args {
                                                            q.push_str(a);
                                                        }
                                                        if let Some(res) =
                                                            plugin.search(&q).into_iter().next()
                                                        {
                                                            command = res.action;
                                                            args = res.args;
                                                        } else {
                                                            command = q;
                                                            args = None;
                                                        }
                                                    }

                                                    self.steps.push(MacroStep {
                                                        label: act.label.clone(),
                                                        command,
                                                        args,
                                                        delay_ms: 0,
                                                    });
                                                    // Reset pending args after adding a step.
                                                    self.add_args.clear();
                                                }
                                            });
                                            ui.add_space(ui.spacing().item_spacing.y);
                                        }
                                    });
                            }
                        });
                    ui.separator();
                    let mut save_rect = egui::Rect::NOTHING;
                    let mut cancel_rect = egui::Rect::NOTHING;
                    ui.horizontal_wrapped(|ui| {
                        let save_button = ui.button("Save");
                        let save_button =
                            with_button_emphasis(ui, save_button, ButtonEmphasis::Primary);
                        save_rect = save_button.rect;
                        if save_button.clicked() {
                            if self.label.trim().is_empty() {
                                app.report_error_message("ui operation", "Label required");
                            } else {
                                let mut steps = self.steps.clone();
                                for step in &mut steps {
                                    if let Some(args) = &step.args
                                        && args.trim().is_empty()
                                    {
                                        step.args = None;
                                    }
                                }
                                let mut candidate = self.entries.clone();
                                if idx == candidate.len() {
                                    candidate.push(MacroEntry {
                                        label: self.label.clone(),
                                        desc: self.desc.clone(),
                                        auto_delay_ms: if self.auto_delay {
                                            Some((self.auto_delay_secs * 1000.0) as u64)
                                        } else {
                                            None
                                        },
                                        steps,
                                    });
                                } else if let Some(entry) = candidate.get_mut(idx) {
                                    entry.label = self.label.clone();
                                    entry.desc = self.desc.clone();
                                    entry.auto_delay_ms = if self.auto_delay {
                                        Some((self.auto_delay_secs * 1000.0) as u64)
                                    } else {
                                        None
                                    };
                                    entry.steps = steps;
                                }
                                save_candidate = Some(candidate);
                            }
                        }
                        let cancel_button = ui.button("Cancel");
                        cancel_rect = cancel_button.rect;
                        if cancel_button.clicked() {
                            self.edit_idx = None;
                            self.auto_delay = false;
                            self.auto_delay_secs = 1.0;
                            // Cancel editing and reset plugin selection state.
                            self.add_plugin.clear();
                            self.add_filter.clear();
                            self.add_args.clear();
                        }
                    });
                    #[cfg(test)]
                    {
                        test_layout = Some(MacroLayoutObservation {
                            scroll: macro_scroll_layout(
                                edit_output.id,
                                parent_max_rect,
                                parent_clip_rect,
                                edit_output.inner_rect,
                                edit_output.content_size,
                                edit_output.state.offset,
                            ),
                            picker_scroll,
                            primary_rect: save_rect,
                            secondary_rect: cancel_rect,
                            window_rect: egui::Rect::NOTHING,
                            visible: test_visible,
                            sizing_pass: test_sizing_pass,
                            close_requested: close,
                        });
                    }
                    #[cfg(not(test))]
                    let _ = (
                        edit_output,
                        parent_max_rect,
                        parent_clip_rect,
                        save_rect,
                        cancel_rect,
                    );
                } else {
                    compact_section_heading(
                        ui,
                        "Saved macros",
                        Some("Build reusable command sequences from apps and plugins."),
                    );
                    let footer_height = macro_footer_reserved_height(ui, &["Add Macro", "Close"]);
                    let parent_max_rect = ui.max_rect();
                    let parent_clip_rect = ui.clip_rect();
                    let body_height = (ui.available_height() - footer_height).max(0.0);
                    let mut remove: Option<usize> = None;
                    let list_output = egui::ScrollArea::vertical()
                        .id_salt("macro_entry_list")
                        .auto_shrink([false, false])
                        .min_scrolled_height(0.0)
                        .max_height(body_height)
                        .show(ui, |ui| {
                            for idx in 0..self.entries.len() {
                                let entry = &self.entries[idx];
                                ui.group(|ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.vertical(|ui| {
                                            compact_section_heading(ui, &entry.label, None);
                                            ui.add(
                                                egui::Label::new(&entry.desc)
                                                    .wrap()
                                                    .selectable(false),
                                            );
                                        });
                                        let edit_button = ui.button("Edit");
                                        if edit_button.clicked() {
                                            self.edit_idx = Some(idx);
                                            self.label = entry.label.clone();
                                            self.desc = entry.desc.clone();
                                            self.steps = entry.steps.clone();
                                            if let Some(ms) = entry.auto_delay_ms {
                                                self.auto_delay = true;
                                                self.auto_delay_secs = ms as f32 / 1000.0;
                                            } else {
                                                self.auto_delay = false;
                                                self.auto_delay_secs = 1.0;
                                            }
                                            self.add_plugin.clear();
                                            self.add_filter.clear();
                                        }
                                        let remove_button = ui.button("Remove");
                                        if with_button_emphasis(
                                            ui,
                                            remove_button,
                                            ButtonEmphasis::Destructive,
                                        )
                                        .clicked()
                                        {
                                            remove = Some(idx);
                                        }
                                    });
                                });
                                ui.add_space(ui.spacing().item_spacing.y);
                            }
                        });
                    ui.separator();
                    let mut add_rect = egui::Rect::NOTHING;
                    let mut close_rect = egui::Rect::NOTHING;
                    ui.horizontal_wrapped(|ui| {
                        let add_button = ui.button("Add Macro");
                        let add_button =
                            with_button_emphasis(ui, add_button, ButtonEmphasis::Primary);
                        add_rect = add_button.rect;
                        if add_button.clicked() {
                            self.edit_idx = Some(self.entries.len());
                            self.label.clear();
                            self.desc.clear();
                            self.steps.clear();
                            self.auto_delay = false;
                            self.auto_delay_secs = 1.0;
                            self.add_plugin.clear();
                            self.add_filter.clear();
                            self.add_args.clear();
                        }
                        let close_button = ui.button("Close");
                        close_rect = close_button.rect;
                        if close_button.clicked() {
                            close = true;
                        }
                    });
                    #[cfg(test)]
                    {
                        test_layout = Some(MacroLayoutObservation {
                            scroll: macro_scroll_layout(
                                list_output.id,
                                parent_max_rect,
                                parent_clip_rect,
                                list_output.inner_rect,
                                list_output.content_size,
                                list_output.state.offset,
                            ),
                            picker_scroll: None,
                            primary_rect: add_rect,
                            secondary_rect: close_rect,
                            window_rect: egui::Rect::NOTHING,
                            visible: test_visible,
                            sizing_pass: test_sizing_pass,
                            close_requested: close,
                        });
                    }
                    #[cfg(not(test))]
                    let _ = (
                        list_output,
                        parent_max_rect,
                        parent_clip_rect,
                        add_rect,
                        close_rect,
                    );
                    if let Some(idx) = remove {
                        let mut candidate = self.entries.clone();
                        candidate.remove(idx);
                        save_candidate = Some(candidate);
                    }
                }
            });
        self.open = open || pinned;
        #[cfg(test)]
        if let Some(mut layout) = test_layout {
            layout.window_rect = _window_output
                .as_ref()
                .map_or(egui::Rect::NOTHING, |window| window.response.rect);
            self.test_layout = Some(layout);
        }
        if let Some(candidate) = save_candidate
            && self.save(app, candidate)
        {
            self.edit_idx = None;
            self.label.clear();
            self.desc.clear();
            self.steps.clear();
            self.auto_delay = false;
            self.auto_delay_secs = 1.0;
            self.add_plugin.clear();
            self.add_filter.clear();
            self.add_args.clear();
        }
        if close && !pinned {
            self.open = false;
        }
    }
}

#[cfg(test)]
fn macro_scroll_layout(
    id: egui::Id,
    parent_max_rect: egui::Rect,
    parent_clip_rect: egui::Rect,
    inner_rect: egui::Rect,
    content_size: egui::Vec2,
    offset: egui::Vec2,
) -> MacroScrollLayout {
    MacroScrollLayout {
        id,
        parent_max_rect,
        parent_clip_rect,
        inner_rect,
        content_size,
        offset,
    }
}

fn macro_footer_reserved_height(ui: &mut egui::Ui, labels: &[&str]) -> f32 {
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
    let text_color = ui.visuals().text_color();
    let available_width = ui.available_width().max(1.0);
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
    use super::*;

    #[test]
    fn matching_plugins_returns_all_when_filter_empty() {
        let dlg = MacroDialog::default();
        let plugins = ["alpha", "beta", "app"];
        let matches = MacroDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        assert_eq!(matches, vec!["alpha", "app", "beta"]);
    }

    #[test]
    fn matching_plugins_returns_empty_when_no_match() {
        let dlg = MacroDialog {
            category_filter: "zzz".into(),
            ..Default::default()
        };
        let plugins = ["alpha", "beta", "app"];
        let matches = MacroDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        assert!(matches.is_empty());
    }

    #[test]
    fn matching_plugins_is_case_insensitive() {
        let dlg = MacroDialog {
            category_filter: "AP".into(),
            ..Default::default()
        };
        let plugins = ["alpha", "beta", "app"];
        let matches = MacroDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        assert_eq!(matches, vec!["alpha", "app"]);
    }

    #[test]
    fn fuzzy_filter_lists_matching_plugins() {
        let dlg = MacroDialog {
            category_filter: "ap".into(),
            ..Default::default()
        };
        let plugins = ["alpha", "beta", "app"];
        let matches = MacroDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        assert_eq!(matches, vec!["alpha", "app"]);
    }

    #[test]
    fn selecting_plugin_after_filtering_updates_state() {
        let mut dlg = MacroDialog {
            category_filter: "ap".into(),
            ..Default::default()
        };
        let plugins = ["alpha", "app"];
        let matches = MacroDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        MacroDialog::select_plugin(&mut dlg.add_plugin, &mut dlg.category_filter, matches[0]);
        assert_eq!(dlg.add_plugin, "alpha");
        assert!(dlg.category_filter.is_empty());
    }

    #[test]
    fn app_category_is_included_and_selectable() {
        let mut dlg = MacroDialog {
            category_filter: "ap".into(),
            ..Default::default()
        };
        let plugins = ["alpha", "app"];
        let matches = MacroDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        assert!(matches.contains(&"app"));
        MacroDialog::select_plugin(&mut dlg.add_plugin, &mut dlg.category_filter, "app");
        assert_eq!(dlg.add_plugin, "app");
        assert!(dlg.category_filter.is_empty());
    }

    #[test]
    fn invalid_reload_and_commit_keep_last_good_and_read_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("macros.json");
        let initial = vec![MacroEntry {
            label: "saved".into(),
            desc: "Saved".into(),
            auto_delay_ms: None,
            steps: Vec::new(),
        }];
        crate::plugins::macros::save_macros(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = MacroDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        let invalid = b"invalid macros";
        std::fs::write(&path, invalid).unwrap();
        assert!(dialog.load_from(path.to_str().unwrap()).is_err());
        assert_eq!(dialog.entries, initial);
        assert!(dialog.load_error.is_some());
        assert!(
            dialog
                .commit_entries(path.to_str().unwrap(), Vec::new())
                .is_err()
        );
        assert_eq!(dialog.entries, initial);
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }

    struct CurrentDirGuard(std::path::PathBuf);

    impl CurrentDirGuard {
        fn enter(path: &std::path::Path) -> Self {
            let original = std::env::current_dir().unwrap();
            std::env::set_current_dir(path).unwrap();
            Self(original)
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.0).unwrap();
        }
    }

    fn test_app(ctx: &egui::Context, root: &std::path::Path) -> LauncherApp {
        use crate::plugin::PluginManager;
        use crate::settings::Settings;
        use std::sync::{Arc, atomic::AtomicBool};

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

    fn test_input(screen_rect: egui::Rect, time: f64, events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(screen_rect),
            time: Some(time),
            events,
            ..Default::default()
        }
    }

    fn render_test_frame(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        input: egui::RawInput,
    ) -> Option<MacroLayoutObservation> {
        let mut dialog = std::mem::take(&mut app.macro_dialog);
        let output = ctx.run_ui(input, |_ui| dialog.ui(ctx, app));
        let layout = dialog.test_layout;
        app.macro_dialog = dialog;
        output.drop_without_applying_deltas();
        layout
    }

    fn assert_macro_scroll_layout(layout: MacroScrollLayout, screen_rect: egui::Rect, owner: &str) {
        assert!(
            layout.parent_max_rect.is_positive(),
            "{owner} has invalid parent max rect: {layout:?}"
        );
        assert!(
            layout.parent_clip_rect.is_positive(),
            "{owner} has invalid parent clip rect: {layout:?}"
        );
        assert!(
            layout.parent_clip_rect.intersects(layout.inner_rect),
            "{owner} is outside its parent clip: layout={layout:?}, screen={screen_rect:?}"
        );
        assert!(
            layout.inner_rect.is_positive(),
            "{owner} has invalid inner rect: {layout:?}"
        );
        assert!(
            screen_rect.contains_rect(layout.inner_rect),
            "{owner} is outside the screen: layout={layout:?}, screen={screen_rect:?}"
        );
        let max_scroll_y = (layout.content_size.y - layout.inner_rect.height()).max(0.0);
        assert!(
            layout.offset.x.is_finite() && layout.offset.y.is_finite(),
            "{owner} has non-finite offset: {layout:?}"
        );
        assert!(
            layout.offset.x >= 0.0 && layout.offset.y >= 0.0,
            "{owner} has negative offset: {layout:?}"
        );
        assert!(
            layout.offset.x <= 0.5,
            "{owner} has unexpected horizontal offset: {layout:?}"
        );
        assert!(
            layout.offset.y <= max_scroll_y + 0.5,
            "{owner} offset exceeds native scroll range {max_scroll_y}: {layout:?}"
        );
    }

    #[test]
    fn constrained_legacy_macro_preserves_pinned_draft_and_keeps_long_editor_reachable() {
        use crate::actions::Action;
        use crate::gui::Panel;

        let _lock = crate::gui::TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let _cwd = CurrentDirGuard::enter(directory.path());
        let fixture = MacroEntry {
            label: "Persisted macro".into(),
            desc: "A persisted description used to confirm pinned initial loading.".into(),
            auto_delay_ms: Some(1750),
            steps: (0..14)
                .map(|index| MacroStep {
                    label: format!("Long step {index}: {}", "command label ".repeat(7)),
                    command: format!("C:\\Program Files\\Example\\tool-{index}.exe"),
                    args: Some(format!("--option-{} {}", index, "argument ".repeat(10))),
                    delay_ms: 2300 + index,
                })
                .collect(),
        };
        let original_entries: Vec<_> = (0..8)
            .map(|index| MacroEntry {
                label: format!("Persisted macro {index}"),
                desc: format!(
                    "A persisted description that wraps in the saved list. {}",
                    "detail ".repeat(8)
                ),
                ..fixture.clone()
            })
            .collect();
        crate::plugins::macros::save_macros(MACROS_FILE, &original_entries).unwrap();
        let persisted_before = std::fs::read(MACROS_FILE).unwrap();

        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, directory.path());
        app.pinned_panels.push(Panel::MacroDialog);
        app.ensure_open(Panel::MacroDialog);
        assert!(app.macro_dialog.open);
        assert_eq!(app.macro_dialog.entries, original_entries);

        app.macro_dialog.edit_idx = Some(0);
        let persisted_entry = app.macro_dialog.entries[0].clone();
        app.macro_dialog.label = persisted_entry.label;
        app.macro_dialog.desc = persisted_entry.desc;
        app.macro_dialog.steps = persisted_entry.steps;
        app.macro_dialog.auto_delay = true;
        app.macro_dialog.auto_delay_secs = 1.75;
        app.macro_dialog.label = "Uncommitted pinned draft".into();
        app.macro_dialog.desc = "A draft that must survive pinned close and reopen.".into();
        app.macro_dialog.steps[0].args = Some("--unsaved-argument-value".into());
        app.macro_dialog.auto_delay = true;
        app.macro_dialog.auto_delay_secs = 4.25;
        app.macro_dialog.add_plugin = "app".into();
        app.macro_dialog.category_filter = "ap".into();
        app.macro_dialog.add_filter = String::new();
        app.macro_dialog.add_args = "--pending-argument".into();
        app.ensure_open(Panel::MacroDialog);
        assert_eq!(app.macro_dialog.label, "Uncommitted pinned draft");
        assert_eq!(
            app.macro_dialog.desc,
            "A draft that must survive pinned close and reopen."
        );
        assert_eq!(
            app.macro_dialog.steps[0].args.as_deref(),
            Some("--unsaved-argument-value")
        );

        app.macro_dialog.edit_idx = None;
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(430.0, 320.0));
        let mut browse_layouts = Vec::new();
        for frame in 0..4 {
            if let Some(layout) = render_test_frame(
                &ctx,
                &mut app,
                test_input(screen_rect, frame as f64, Vec::new()),
            ) && layout.visible
                && !layout.sizing_pass
            {
                browse_layouts.push(layout);
            }
        }
        assert!(browse_layouts.len() >= 2);
        let browse_layout = *browse_layouts.last().expect("the macro list should render");
        let browse_scroll_id = browse_layout.scroll.id;
        for layout in browse_layouts {
            assert_macro_scroll_layout(layout.scroll, screen_rect, "macro browse list");
            assert_eq!(layout.scroll.id, browse_scroll_id);
        }
        assert!(browse_layout.scroll.content_size.y > browse_layout.scroll.inner_rect.height());
        assert!(browse_layout.window_rect.is_positive());
        assert!(screen_rect.contains_rect(browse_layout.window_rect));
        assert!(browse_layout.primary_rect.is_positive());
        assert!(browse_layout.secondary_rect.is_positive());
        assert!(
            browse_layout
                .window_rect
                .contains_rect(browse_layout.primary_rect)
        );
        assert!(
            browse_layout
                .window_rect
                .contains_rect(browse_layout.secondary_rect)
        );
        assert!(browse_layout.primary_rect.top() >= browse_layout.scroll.inner_rect.bottom() - 1.0);

        let close_point = browse_layout.secondary_rect.center();
        render_test_frame(
            &ctx,
            &mut app,
            test_input(
                screen_rect,
                4.0,
                vec![
                    egui::Event::PointerMoved(close_point),
                    egui::Event::PointerButton {
                        pos: close_point,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::default(),
                    },
                ],
            ),
        );
        let release = render_test_frame(
            &ctx,
            &mut app,
            test_input(
                screen_rect,
                4.1,
                vec![egui::Event::PointerButton {
                    pos: close_point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                }],
            ),
        )
        .expect("the release frame should render the macro list");
        assert!(release.visible && !release.sizing_pass);
        assert!(
            release.close_requested,
            "Close should register a click on release"
        );
        assert!(
            app.macro_dialog.open,
            "a pinned Close click should not close the panel"
        );
        app.ensure_open(Panel::MacroDialog);
        assert_eq!(app.macro_dialog.label, "Uncommitted pinned draft");
        assert_eq!(
            app.macro_dialog.desc,
            "A draft that must survive pinned close and reopen."
        );
        assert_eq!(
            app.macro_dialog.steps[0].args.as_deref(),
            Some("--unsaved-argument-value")
        );

        let draft_entries = app.macro_dialog.entries.clone();
        let draft_steps = app.macro_dialog.steps.clone();
        let draft_label = app.macro_dialog.label.clone();
        let draft_desc = app.macro_dialog.desc.clone();
        app.macro_dialog.edit_idx = Some(0);
        app.actions = std::sync::Arc::new(
            (0..16)
                .map(|index| Action {
                    label: format!("Selectable app {index}"),
                    desc: format!(
                        "A long description that wraps in the picker. {}",
                        "detail ".repeat(8)
                    ),
                    action: format!("C:\\Apps\\Selectable\\app-{index}.exe"),
                    args: Some("--picker-argument".into()),
                })
                .collect(),
        );
        let mut edit_layouts = Vec::new();
        for frame in 5..10 {
            if let Some(layout) = render_test_frame(
                &ctx,
                &mut app,
                test_input(screen_rect, frame as f64, Vec::new()),
            ) && layout.visible
                && !layout.sizing_pass
            {
                edit_layouts.push(layout);
            }
        }
        assert!(edit_layouts.len() >= 2);
        let edit_scroll_id = edit_layouts[0].scroll.id;
        for layout in &edit_layouts {
            assert_macro_scroll_layout(layout.scroll, screen_rect, "macro edit form");
            assert_eq!(layout.scroll.id, edit_scroll_id);
            assert!(layout.primary_rect.is_positive());
            assert!(layout.secondary_rect.is_positive());
            assert!(layout.window_rect.contains_rect(layout.primary_rect));
            assert!(layout.window_rect.contains_rect(layout.secondary_rect));
            assert!(layout.primary_rect.top() >= layout.scroll.inner_rect.bottom() - 1.0);
        }
        let first_edit_layout = edit_layouts[0];
        let picker = first_edit_layout
            .picker_scroll
            .expect("the app picker should render in the macro edit form");
        let mut outer_state = egui::scroll_area::State::load(&ctx, first_edit_layout.scroll.id)
            .expect("the macro edit form should store its native scroll state");
        let max_outer_scroll_y = (first_edit_layout.scroll.content_size.y
            - first_edit_layout.scroll.inner_rect.height())
        .max(0.0);
        let target_outer_scroll_y = (outer_state.offset.y + picker.inner_rect.center().y
            - first_edit_layout.scroll.inner_rect.center().y)
            .clamp(0.0, max_outer_scroll_y);
        outer_state.offset.y = target_outer_scroll_y;
        outer_state.store(&ctx, first_edit_layout.scroll.id);

        let mut scrolled_layouts = Vec::new();
        for frame in 11..15 {
            if let Some(layout) = render_test_frame(
                &ctx,
                &mut app,
                test_input(screen_rect, frame as f64, Vec::new()),
            ) && layout.visible
                && !layout.sizing_pass
            {
                scrolled_layouts.push(layout);
            }
        }
        assert!(scrolled_layouts.len() >= 2);
        let picker_scroll_id = scrolled_layouts[0]
            .picker_scroll
            .expect("the app picker should render")
            .id;
        for layout in scrolled_layouts {
            assert_macro_scroll_layout(layout.scroll, screen_rect, "scrolled macro edit form");
            assert!(
                layout.scroll.offset.y > 0.5,
                "macro edit form did not reach target offset {target_outer_scroll_y}: {layout:?}"
            );
            assert!(layout.scroll.content_size.y > layout.scroll.inner_rect.height());
            let picker = layout.picker_scroll.expect("the app picker should render");
            assert_macro_scroll_layout(picker, screen_rect, "scrolled app picker");
            assert!(picker.content_size.y > picker.inner_rect.height());
            assert_eq!(picker.id, picker_scroll_id);
        }
        assert_eq!(app.macro_dialog.entries, draft_entries);
        assert_eq!(app.macro_dialog.steps, draft_steps);
        assert_eq!(app.macro_dialog.label, draft_label);
        assert_eq!(app.macro_dialog.desc, draft_desc);
        assert!(app.macro_dialog.auto_delay);
        assert_eq!(app.macro_dialog.auto_delay_secs, 4.25);
        assert_eq!(std::fs::read(MACROS_FILE).unwrap(), persisted_before);
    }
}
