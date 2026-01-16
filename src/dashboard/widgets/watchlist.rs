use super::{Widget, WidgetAction};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::watchlist::{WatchItemSnapshot, WatchStatus, WATCHLIST_DATA, WATCHLIST_FILE};
use crate::{watchlist, watchlist::WatchItemConfig};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatchlistWidgetConfig {}

pub struct WatchlistWidget {
    refresh_pending: bool,
    error: Option<String>,
}

impl WatchlistWidget {
    pub fn new(_cfg: WatchlistWidgetConfig) -> Self {
        Self {
            refresh_pending: false,
            error: None,
        }
    }

    fn action(label: impl Into<String>, action: impl Into<String>) -> Action {
        Action {
            label: label.into(),
            desc: "Watchlist".to_string(),
            action: action.into(),
            args: None,
        }
    }

    fn status_badge(ui: &mut egui::Ui, status: WatchStatus) -> egui::Response {
        let (text, color) = match status {
            WatchStatus::Ok => ("OK", egui::Color32::GREEN),
            WatchStatus::Warn => ("Warn", egui::Color32::YELLOW),
            WatchStatus::Critical => ("Critical", egui::Color32::RED),
        };
        ui.add(
            egui::Button::new(
                egui::RichText::new(text)
                    .color(egui::Color32::WHITE)
                    .small(),
            )
            .fill(color)
            .rounding(4.0)
            .min_size(egui::vec2(48.0, 18.0)),
        )
    }

    fn watchlist_paths() -> HashMap<String, String> {
        WATCHLIST_DATA
            .read()
            .map(|cfg| {
                cfg.items
                    .iter()
                    .filter_map(|item| Self::item_path(item))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn item_path(item: &WatchItemConfig) -> Option<(String, String)> {
        item.path
            .as_ref()
            .map(|path| (item.id.clone(), path.clone()))
    }

    fn invalidate_watchlist_cache(&mut self) {
        if let Err(err) = watchlist::refresh_watchlist_cache(WATCHLIST_FILE) {
            self.error = Some(format!("Failed to refresh watchlist: {err}"));
        } else {
            self.error = None;
        }
    }

    fn refresh_if_needed(&mut self, ctx: &DashboardContext<'_>) {
        if self.refresh_pending {
            ctx.data_cache.maybe_refresh_watchlist(0);
            self.refresh_pending = false;
        }
    }

    fn render_header(ui: &mut egui::Ui) {
        let header = ["Label", "Value", "Delta", "Status", "Last Updated", ""];
        for text in header {
            ui.add(
                egui::Label::new(egui::RichText::new(text).strong())
                    .wrap(false)
                    .truncate(true),
            );
        }
        ui.end_row();
    }

    fn render_row(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &WatchItemSnapshot,
        path: Option<&str>,
    ) -> Option<WidgetAction> {
        let mut clicked = false;

        let label_response = ui.add(
            egui::Label::new(&snapshot.label)
                .wrap(false)
                .sense(egui::Sense::click()),
        );
        clicked |= label_response.clicked();

        let value_response = ui.add(
            egui::Label::new(&snapshot.value_text)
                .wrap(false)
                .sense(egui::Sense::click()),
        );
        clicked |= value_response.clicked();

        let delta = snapshot.delta_text.as_deref().unwrap_or("—");
        let delta_response = ui.add(
            egui::Label::new(delta)
                .wrap(false)
                .sense(egui::Sense::click()),
        );
        clicked |= delta_response.clicked();

        let status_response = Self::status_badge(ui, snapshot.status);
        clicked |= status_response.clicked();

        let updated = snapshot
            .last_updated
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let updated_response = ui.add(
            egui::Label::new(updated)
                .wrap(false)
                .sense(egui::Sense::click()),
        );
        clicked |= updated_response.clicked();

        let mut menu_action = None;
        ui.menu_button("⋯", |ui| {
            if let Some(path) = path {
                if ui.button("Copy path").clicked() {
                    let action = Self::action("Copy watchlist path", format!("clipboard:{path}"));
                    menu_action = Some(WidgetAction {
                        query_override: Some(action.label.clone()),
                        action,
                    });
                    ui.close_menu();
                }
                if ui.button("Open").clicked() {
                    let action = Self::action("Open watchlist path", path.to_string());
                    menu_action = Some(WidgetAction {
                        query_override: Some(action.label.clone()),
                        action,
                    });
                    ui.close_menu();
                }
            }
            if ui.button("Edit watchlist.json").clicked() {
                let action = Self::action("Open watchlist.json", WATCHLIST_FILE.to_string());
                menu_action = Some(WidgetAction {
                    query_override: Some(action.label.clone()),
                    action,
                });
                ui.close_menu();
            }
            if ui.button("Refresh now").clicked() {
                self.invalidate_watchlist_cache();
                self.refresh_pending = true;
                ui.close_menu();
            }
        });
        ui.end_row();

        if menu_action.is_some() {
            return menu_action;
        }

        if clicked {
            if let Some(path) = path {
                let action = Self::action(format!("Open {}", snapshot.label), path.to_string());
                return Some(WidgetAction {
                    query_override: Some(action.label.clone()),
                    action,
                });
            }
        }

        None
    }
}

impl Default for WatchlistWidget {
    fn default() -> Self {
        Self::new(WatchlistWidgetConfig::default())
    }
}

impl Widget for WatchlistWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.refresh_if_needed(ctx);

        if let Some(err) = &self.error {
            ui.colored_label(egui::Color32::YELLOW, err);
        }

        let snapshot = ctx.data_cache.watchlist_snapshot();
        if snapshot.is_empty() {
            ui.label("No watchlist items.");
            return None;
        }

        let paths = Self::watchlist_paths();
        let scroll_id = ui.id().with("watchlist_scroll");
        let mut clicked = None;

        egui::ScrollArea::both()
            .id_source(scroll_id)
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                egui::Grid::new("watchlist_grid")
                    .striped(true)
                    .min_col_width(60.0)
                    .show(ui, |ui| {
                        Self::render_header(ui);
                        for entry in snapshot.iter() {
                            let path = paths.get(&entry.id).map(String::as_str);
                            if clicked.is_none() {
                                clicked = self.render_row(ui, entry, path);
                            } else {
                                let _ = self.render_row(ui, entry, path);
                            }
                        }
                    });
            });

        clicked
    }

    fn header_ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &DashboardContext<'_>,
    ) -> Option<WidgetAction> {
        if ui
            .small_button("Refresh")
            .on_hover_text("Refresh watchlist data now.")
            .clicked()
        {
            self.invalidate_watchlist_cache();
            self.refresh_pending = true;
        }
        None
    }
}
