use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{ButtonEmphasis, secondary_help, with_button_emphasis};
use crate::plugins::folders::{FOLDERS_FILE, load_folders, set_alias};
use eframe::egui;

#[derive(Default)]
pub struct AliasDialog {
    pub open: bool,
    path: String,
    alias: String,
    #[cfg(test)]
    test_layout: Option<AliasDialogLayout>,
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct AliasDialogLayout {
    window_rect: egui::Rect,
    target_parent_clip_rect: egui::Rect,
    target_rect: egui::Rect,
    target_content_size: egui::Vec2,
    alias_rect: egui::Rect,
    save_rect: egui::Rect,
    cancel_rect: egui::Rect,
}

impl AliasDialog {
    pub fn open(&mut self, path: &str) {
        self.path = path.to_string();
        // pre-fill alias with current value if exists
        if let Ok(list) = load_folders(FOLDERS_FILE)
            && let Some(entry) = list.into_iter().find(|f| f.path == self.path)
        {
            if let Some(a) = entry.alias {
                self.alias = a;
            } else {
                self.alias = String::new();
            }
        }
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        #[cfg(test)]
        let window_id = egui::Id::new("Set Folder Alias");
        #[cfg(test)]
        let mut test_layout = None;
        let window_output = egui::Window::new("Set Folder Alias")
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.label("Folder path");
                #[cfg(test)]
                let target_parent_clip_rect = ui.clip_rect();
                let target_width = ui.available_width().min(360.0).max(1.0);
                let _target_output = egui::ScrollArea::both()
                    .id_salt("folder_alias_target")
                    .auto_shrink([true, true])
                    .min_scrolled_height(0.0)
                    .max_width(target_width)
                    .max_height(72.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(self.path.as_str())
                                .wrap_mode(egui::TextWrapMode::Extend),
                        )
                        .on_hover_text(&self.path);
                    });

                let _alias = ui
                    .horizontal_wrapped(|ui| {
                        let label = ui.label("Alias");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.alias)
                                .desired_width(ui.available_width().min(320.0).max(72.0))
                                .id_salt("folder_alias_field"),
                        )
                        .labelled_by(label.id)
                    })
                    .inner;
                secondary_help(ui, "Leave blank to clear the folder alias.");

                #[cfg(test)]
                let mut save_rect = egui::Rect::NOTHING;
                #[cfg(test)]
                let mut cancel_rect = egui::Rect::NOTHING;
                ui.horizontal_wrapped(|ui| {
                    let save = ui.button("Save");
                    let save = with_button_emphasis(ui, save, ButtonEmphasis::Primary);
                    #[cfg(test)]
                    {
                        save_rect = save.rect;
                    }
                    if save.clicked() {
                        if let Err(e) = set_alias(FOLDERS_FILE, &self.path, &self.alias) {
                            app.report_error_message(
                                "ui operation",
                                format!("Failed to save alias: {e}"),
                            );
                        } else {
                            close = true;
                            app.search();
                            app.focus_input();
                        }
                    }
                    let cancel_response = ui.button("Cancel");
                    #[cfg(test)]
                    {
                        cancel_rect = cancel_response.rect;
                    }
                    if cancel_response.clicked() {
                        close = true;
                    }
                });

                #[cfg(test)]
                {
                    test_layout = Some(AliasDialogLayout {
                        window_rect: egui::Rect::NOTHING,
                        target_parent_clip_rect,
                        // egui 0.36 returns the pre-shrink rect; `end` applies auto_shrink before reserving it.
                        target_rect: egui::Rect::from_min_size(
                            _target_output.inner_rect.min,
                            _target_output
                                .inner_rect
                                .size()
                                .min(_target_output.content_size),
                        ),
                        target_content_size: _target_output.content_size,
                        alias_rect: _alias.rect,
                        save_rect,
                        cancel_rect,
                    });
                }
            });
        #[cfg(test)]
        {
            self.test_layout = match (window_output.as_ref(), test_layout) {
                (Some(window), Some(layout)) => Some(AliasDialogLayout {
                    window_rect: ctx
                        .memory(|memory| memory.area_rect(window_id))
                        .unwrap_or(window.response.rect),
                    ..layout
                }),
                _ => None,
            };
        }
        #[cfg(not(test))]
        let _ = window_output;
        if close {
            self.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AliasDialog, AliasDialogLayout};
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

    fn render_frame(
        ctx: &egui::Context,
        dialog: &mut AliasDialog,
        app: &mut crate::gui::LauncherApp,
        screen_size: egui::Vec2,
    ) -> egui::FullOutput {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size)),
                focused: true,
                ..Default::default()
            },
            |ctx| dialog.ui(ctx, app),
        )
    }

    fn assert_reachable(layout: AliasDialogLayout, screen_rect: egui::Rect) {
        assert!(screen_rect.contains_rect(layout.window_rect));
        assert!(layout.window_rect.contains_rect(layout.target_rect));
        assert!(
            layout
                .target_parent_clip_rect
                .contains_rect(layout.target_rect)
        );
        assert!(layout.target_rect.is_positive());
        assert!(layout.target_content_size.x > layout.target_rect.width());
        for widget in [layout.alias_rect, layout.save_rect, layout.cancel_rect] {
            assert!(layout.window_rect.contains_rect(widget));
            assert!(screen_rect.contains_rect(widget));
            assert!(layout.target_parent_clip_rect.contains_rect(widget));
        }
        assert!(
            layout.target_rect.bottom() <= layout.alias_rect.top(),
            "target viewport {:?} (bottom {}) overlaps alias field {:?} (top {}), by {} points",
            layout.target_rect,
            layout.target_rect.bottom(),
            layout.alias_rect,
            layout.alias_rect.top(),
            layout.target_rect.bottom() - layout.alias_rect.top(),
        );
        assert!(!layout.save_rect.intersects(layout.cancel_rect));
    }

    #[test]
    fn long_folder_target_scrolls_and_actions_stay_reachable_without_render_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, directory.path());
        let path = format!("C:\\{}\\record.txt", "long-folder-segment\\".repeat(28));
        let mut dialog = AliasDialog {
            open: true,
            path: path.clone(),
            alias: "draft alias".into(),
            ..Default::default()
        };
        let screen_size = egui::vec2(430.0, 300.0);
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size);
        for _ in 0..3 {
            render_frame(&ctx, &mut dialog, &mut app, screen_size).drop_without_applying_deltas();
        }

        let layout = dialog
            .test_layout
            .expect("folder alias layout was rendered");
        assert_reachable(layout, screen_rect);
        assert!(dialog.open);
        assert_eq!(dialog.path, path);
        assert_eq!(dialog.alias, "draft alias");
    }
}
