use crate::gui::AddActionDialog;
use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{
    ButtonEmphasis, compact_section_heading, secondary_help, with_button_emphasis,
};
use eframe::egui;

/// State container for the app editor window.
///
/// It tracks the current search filter and manages the nested
/// [`AddActionDialog`] used when creating new apps.
#[derive(Default)]
pub struct ActionsEditor {
    /// Search text used to filter the displayed actions.
    search: String,
    /// Dialog used for creating a new action.
    dialog: AddActionDialog,
    #[cfg(test)]
    test_list_layout: Option<ScrollLayoutObservation>,
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) struct ScrollLayoutObservation {
    pub(crate) id: egui::Id,
    pub(crate) visible: bool,
    pub(crate) sizing_pass: bool,
    pub(crate) parent_max_rect: egui::Rect,
    pub(crate) parent_clip_rect: egui::Rect,
    pub(crate) inner_rect: egui::Rect,
    pub(crate) content_size: egui::Vec2,
    pub(crate) offset: egui::Vec2,
}

impl ActionsEditor {
    /// Open the dialog for editing an existing app.
    pub fn open_edit(&mut self, idx: usize, act: &crate::actions::Action) {
        self.dialog.open_edit(idx, act);
    }

    /// Returns whether the add dialog is currently open.
    pub fn is_dialog_open(&self) -> bool {
        self.dialog.open
    }

    /// Open the add dialog with `path` pre-filled.
    pub fn open_add_with_path(&mut self, path: &str) {
        self.dialog.open_add_with_path(path);
    }

    /// Render the app editor window.
    ///
    /// * `ctx` - Egui context used for drawing the editor UI.
    /// * `app` - Mutable reference to the application state. Actions can be
    ///   added or removed and will be persisted when modified.
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        let mut open = app.show_editor;
        #[cfg(test)]
        let mut test_visible = false;
        #[cfg(test)]
        let mut test_sizing_pass = false;
        egui::Window::new("App Editor")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                #[cfg(test)]
                {
                    test_visible = ui.is_visible();
                    test_sizing_pass = ui.is_sizing_pass();
                }
                compact_section_heading(
                    ui,
                    "Custom apps",
                    Some("Search and manage the apps shown in the launcher."),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label("Search");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .desired_width(ui.available_width().max(72.0)),
                    );
                    let new_app = ui.button("New App");
                    if with_button_emphasis(ui, new_app, ButtonEmphasis::Primary).clicked() {
                        self.dialog.open_add();
                    }
                });

                self.dialog.ui(ctx, app);

                ui.separator();
                let query = self.search.to_lowercase();
                let has_query = !self.search.trim().is_empty();
                let mut remove: Option<usize> = None;
                let parent_max_rect = ui.max_rect();
                let parent_clip_rect = ui.clip_rect();
                let list_output = egui::ScrollArea::vertical()
                    .id_salt("app_editor_action_list")
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .max_height(ui.available_height().max(0.0))
                    .show(ui, |ui| {
                        for (idx, act) in app.actions.iter().enumerate() {
                            if has_query
                                && !act.label.to_lowercase().contains(&query)
                                && !act.desc.to_lowercase().contains(&query)
                                && !act.action.to_lowercase().contains(&query)
                            {
                                continue;
                            }

                            ui.group(|ui| {
                                compact_section_heading(ui, &act.label, None);
                                secondary_help(ui, &act.desc);
                                ui.add(
                                    egui::Label::new(egui::RichText::new(&act.action).monospace())
                                        .wrap(),
                                );
                                if let Some(args) = &act.args {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.weak("Arguments");
                                        ui.add(
                                            egui::Label::new(egui::RichText::new(args).monospace())
                                                .wrap(),
                                        );
                                    });
                                }
                                ui.horizontal_wrapped(|ui| {
                                    let edit = ui.button("Edit");
                                    if edit.clicked() {
                                        self.dialog.open_edit(idx, act);
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

                #[cfg(test)]
                {
                    self.test_list_layout = Some(ScrollLayoutObservation {
                        id: list_output.id,
                        visible: test_visible,
                        sizing_pass: test_sizing_pass,
                        parent_max_rect,
                        parent_clip_rect,
                        inner_rect: list_output.inner_rect,
                        content_size: list_output.content_size,
                        offset: list_output.state.offset,
                    });
                }
                #[cfg(not(test))]
                let _ = (parent_max_rect, parent_clip_rect, list_output);

                if let Some(i) = remove
                    && let Err(e) = app.update_custom_actions(move |actions| {
                        anyhow::ensure!(i < actions.len(), "only custom actions can be removed");
                        actions.remove(i);
                        Ok(())
                    })
                {
                    app.report_error("ui operation", format!("Failed to save: {e}"));
                }
            });

        app.show_editor = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::LauncherApp;
    use crate::plugin::PluginManager;
    use crate::settings::Settings;
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

    fn assert_scroll_layout(layout: ScrollLayoutObservation, screen_rect: egui::Rect) {
        assert!(layout.parent_max_rect.is_positive());
        assert!(layout.parent_clip_rect.is_positive());
        assert!(layout.parent_clip_rect.intersects(layout.inner_rect));
        assert!(layout.inner_rect.is_positive());
        assert!(screen_rect.contains_rect(layout.inner_rect));
        let max_scroll_y = (layout.content_size.y - layout.inner_rect.height()).max(0.0);
        assert!(layout.offset.x.is_finite() && layout.offset.y.is_finite());
        assert!(layout.offset.x >= 0.0 && layout.offset.y >= 0.0);
        assert!(layout.offset.x <= 0.5);
        assert!(layout.offset.y <= max_scroll_y + 0.5);
    }

    #[test]
    fn constrained_editor_and_add_form_keep_full_commands_and_footer_reachable_without_publication()
    {
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, directory.path());
        let long_action = crate::actions::Action {
            label: "Application entry with a long label ".repeat(5),
            desc: "A description long enough to wrap across a narrow editor window. ".repeat(8),
            action: "C:\\Program Files\\Example\\Application.exe".repeat(10),
            args: Some("--option-with-a-long-value ".repeat(10)),
        };
        app.actions = Arc::new(vec![long_action.clone(); 8]);
        app.custom_len = app.actions.len();
        app.show_editor = true;
        let actions_before = Arc::clone(&app.actions);
        let version_before = crate::actions::actions_version();

        let mut editor = ActionsEditor::default();
        editor.open_edit(0, &long_action);
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 260.0));
        let input = || egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };
        let mut editor_layouts = Vec::new();
        let mut add_layout = None;
        for _ in 0..4 {
            let output = ctx.run_ui(input(), |_| {
                editor.ui(&ctx, &mut app);
            });
            if let Some(layout) = editor.test_list_layout
                && layout.visible
                && !layout.sizing_pass
            {
                editor_layouts.push(layout);
            }
            add_layout = editor
                .dialog
                .test_form_layout
                .filter(|layout| layout.visible && !layout.sizing_pass)
                .or(add_layout);
            output.drop_without_applying_deltas();
        }

        let add_layout = add_layout.expect("the nested Add/Edit form should render");
        assert!(add_layout.visible && !add_layout.sizing_pass);
        assert!(editor_layouts.len() >= 2);
        let list_id = editor_layouts[0].id;
        assert!(editor_layouts.iter().all(|layout| layout.id == list_id));
        for layout in editor_layouts {
            assert_scroll_layout(layout, screen_rect);
        }
        assert!(add_layout.window_rect.is_positive());
        assert!(screen_rect.contains_rect(add_layout.window_rect));
        assert!(
            add_layout
                .scroll
                .parent_clip_rect
                .intersects(add_layout.scroll.inner_rect)
        );
        assert!(add_layout.scroll.inner_rect.is_positive());
        let max_scroll_y =
            (add_layout.scroll.content_size.y - add_layout.scroll.inner_rect.height()).max(0.0);
        assert!(add_layout.scroll.offset.x.is_finite() && add_layout.scroll.offset.y.is_finite());
        assert!(add_layout.scroll.offset.x >= 0.0 && add_layout.scroll.offset.y >= 0.0);
        assert!(add_layout.scroll.offset.x <= 0.5);
        assert!(add_layout.scroll.offset.y <= max_scroll_y + 0.5);
        assert!(add_layout.primary_rect.is_positive());
        assert!(add_layout.cancel_rect.is_positive());
        assert!(
            add_layout
                .window_rect
                .contains_rect(add_layout.primary_rect)
        );
        assert!(add_layout.window_rect.contains_rect(add_layout.cancel_rect));
        assert!(add_layout.primary_rect.top() >= add_layout.scroll.inner_rect.bottom() - 1.0);
        assert!(Arc::ptr_eq(&app.actions, &actions_before));
        assert_eq!(crate::actions::actions_version(), version_before);
        let (label, description, path, show_args, args) = editor.dialog.test_draft_fields();
        assert_eq!(label, long_action.label);
        assert_eq!(description, long_action.desc);
        assert_eq!(path, long_action.action);
        assert!(show_args);
        assert_eq!(args, long_action.args.unwrap());
    }
}
