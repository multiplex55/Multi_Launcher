use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::calendar::{add_months, parse_date_reference, CalendarSnapshot, EventInstance};
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// Example dashboard slot:
// {"widget":"calendar","row":0,"col":0,"row_span":2,"col_span":2,
//  "settings":{"mode":"week","range_days":7,"max_items":8,"show_tags":true}}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalendarWidgetMode {
    Day,
    Week,
    Month,
}

impl Default for CalendarWidgetMode {
    fn default() -> Self {
        CalendarWidgetMode::Week
    }
}

fn default_range_days() -> u32 {
    7
}

fn default_show_completed() -> bool {
    true
}

fn default_show_tags() -> bool {
    true
}

fn default_max_items() -> usize {
    8
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarWidgetConfig {
    #[serde(default)]
    pub mode: CalendarWidgetMode,
    #[serde(default)]
    pub default_date: Option<String>,
    #[serde(default = "default_range_days")]
    pub range_days: u32,
    #[serde(default = "default_show_completed")]
    pub show_completed: bool,
    #[serde(default = "default_show_tags")]
    pub show_tags: bool,
    #[serde(default = "default_max_items")]
    pub max_items: usize,
    #[serde(default)]
    pub compact: bool,
}

impl Default for CalendarWidgetConfig {
    fn default() -> Self {
        Self {
            mode: CalendarWidgetMode::default(),
            default_date: None,
            range_days: default_range_days(),
            show_completed: default_show_completed(),
            show_tags: default_show_tags(),
            max_items: default_max_items(),
            compact: false,
        }
    }
}

pub struct CalendarWidget {
    cfg: CalendarWidgetConfig,
    selected_date: Option<NaiveDate>,
    view_month: Option<NaiveDate>,
}

impl CalendarWidget {
    pub fn new(cfg: CalendarWidgetConfig) -> Self {
        Self {
            cfg,
            selected_date: None,
            view_month: None,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(
            ui,
            value,
            ctx,
            |ui, cfg: &mut CalendarWidgetConfig, _ctx| {
                let mut changed = false;
                ui.heading("View");
                egui::ComboBox::from_label("Mode")
                    .selected_text(match cfg.mode {
                        CalendarWidgetMode::Day => "Day",
                        CalendarWidgetMode::Week => "Week",
                        CalendarWidgetMode::Month => "Month",
                    })
                    .show_ui(ui, |ui| {
                        changed |= ui
                            .selectable_value(&mut cfg.mode, CalendarWidgetMode::Day, "Day")
                            .changed();
                        changed |= ui
                            .selectable_value(&mut cfg.mode, CalendarWidgetMode::Week, "Week")
                            .changed();
                        changed |= ui
                            .selectable_value(&mut cfg.mode, CalendarWidgetMode::Month, "Month")
                            .changed();
                    });
                ui.horizontal(|ui| {
                    ui.label("Default date");
                    let mut default_date = cfg.default_date.clone().unwrap_or_default();
                    if ui.text_edit_singleline(&mut default_date).changed() {
                        cfg.default_date = if default_date.trim().is_empty() {
                            None
                        } else {
                            Some(default_date.trim().to_string())
                        };
                        changed = true;
                    }
                });
                ui.label("Use today, tomorrow, next mon, or YYYY-MM-DD.");
                ui.separator();
                ui.heading("List limits");
                ui.horizontal(|ui| {
                    ui.label("Range days");
                    changed |= ui
                        .add(egui::DragValue::new(&mut cfg.range_days).clamp_range(1..=30))
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Max items");
                    changed |= ui
                        .add(egui::DragValue::new(&mut cfg.max_items).clamp_range(1..=50))
                        .changed();
                });
                changed |= ui
                    .checkbox(&mut cfg.show_completed, "Include completed/past")
                    .changed();
                ui.separator();
                ui.heading("Display");
                changed |= ui.checkbox(&mut cfg.show_tags, "Show tags").changed();
                changed |= ui.checkbox(&mut cfg.compact, "Compact layout").changed();
                changed
            },
        )
    }

    fn resolve_default_date(&self, now: NaiveDate) -> NaiveDate {
        self.cfg
            .default_date
            .as_deref()
            .and_then(|input| parse_date_reference(input, now))
            .unwrap_or(now)
    }

    fn ensure_selection(&mut self, now: NaiveDate) {
        if self.selected_date.is_none() {
            self.selected_date = Some(self.resolve_default_date(now));
        }
        if self.view_month.is_none() {
            if let Some(date) = self.selected_date {
                self.view_month =
                    Some(NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date));
            }
        }
    }

    fn is_completed(&self, now: NaiveDateTime, instance: &EventInstance) -> bool {
        instance.end < now
    }

    fn collect_events<'a>(
        &self,
        calendar: &'a CalendarSnapshot,
        start_date: NaiveDate,
        end_date: NaiveDate,
        today: NaiveDate,
    ) -> Vec<&'a EventInstance> {
        let mut instances: Vec<&EventInstance> = calendar
            .events_next_7_days
            .iter()
            .filter(|instance| {
                let date = instance.start.date();
                date >= start_date && date <= end_date
            })
            .collect();
        if start_date == today && end_date == today {
            instances = calendar.events_today.iter().collect();
        }
        instances.sort_by_key(|instance| instance.start);
        instances
    }

    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        selected_date: NaiveDate,
    ) -> Option<WidgetAction> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.label(match self.cfg.mode {
                CalendarWidgetMode::Day => {
                    format!("Day view • {}", selected_date.format("%Y-%m-%d"))
                }
                CalendarWidgetMode::Week => {
                    format!("Week view • {}", selected_date.format("%Y-%m-%d"))
                }
                CalendarWidgetMode::Month => {
                    format!("Month view • {}", selected_date.format("%B %Y"))
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("+ Add").clicked() {
                    let input = format!("{} all-day New event", selected_date.format("%Y-%m-%d"));
                    action = Some(WidgetAction {
                        action: Action {
                            label: "Add calendar event".into(),
                            desc: "Calendar".into(),
                            action: format!("calendar:add:{input}"),
                            args: None,
                        },
                        query_override: Some("cal add".into()),
                    });
                }
                if ui.small_button("Open").clicked() {
                    let view = match self.cfg.mode {
                        CalendarWidgetMode::Day => "day",
                        CalendarWidgetMode::Week => "week",
                        CalendarWidgetMode::Month => "month",
                    };
                    action = Some(WidgetAction {
                        action: Action {
                            label: "Open calendar".into(),
                            desc: "Calendar".into(),
                            action: format!("calendar:open:{view}"),
                            args: None,
                        },
                        query_override: None,
                    });
                }
            });
        });
        action
    }

    fn render_month_grid(
        &mut self,
        ui: &mut egui::Ui,
        calendar: &CalendarSnapshot,
        selected_date: NaiveDate,
    ) {
        let Some(view_month) = self.view_month else {
            return;
        };
        let markers = build_marker_set(calendar, view_month);
        ui.horizontal(|ui| {
            if ui.button("<").clicked() {
                self.view_month = Some(add_months(view_month, -1));
            }
            if ui.button(">").clicked() {
                self.view_month = Some(add_months(view_month, 1));
            }
            ui.label(view_month.format("%B %Y").to_string());
        });
        ui.separator();
        month_grid(
            ui,
            view_month,
            &markers,
            selected_date,
            self.cfg.compact,
            |date| {
                self.selected_date = Some(date);
            },
        );
    }

    fn render_event_list(
        &self,
        ui: &mut egui::Ui,
        calendar: &CalendarSnapshot,
        selected_date: NaiveDate,
        now: NaiveDateTime,
        today: NaiveDate,
    ) -> Option<WidgetAction> {
        let mut action = None;
        let range_days = self.cfg.range_days.max(1) as i64;
        let end_date = match self.cfg.mode {
            CalendarWidgetMode::Day => selected_date,
            CalendarWidgetMode::Week => {
                selected_date + Duration::days(range_days.saturating_sub(1))
            }
            CalendarWidgetMode::Month => {
                let month_start =
                    NaiveDate::from_ymd_opt(selected_date.year(), selected_date.month(), 1)
                        .unwrap_or(selected_date);
                add_months(month_start, 1) - Duration::days(1)
            }
        };
        let mut instances = self.collect_events(calendar, selected_date, end_date, today);
        if !self.cfg.show_completed {
            instances.retain(|instance| !self.is_completed(now, instance));
        }
        if instances.is_empty() {
            ui.label("No calendar snapshot data for this range.");
            return None;
        }
        if instances.len() > self.cfg.max_items {
            instances.truncate(self.cfg.max_items);
        }
        let text_style = if self.cfg.compact {
            egui::TextStyle::Small
        } else {
            egui::TextStyle::Body
        };
        for instance in instances {
            let title = calendar
                .event_titles
                .get(&instance.source_event_id)
                .cloned()
                .unwrap_or_else(|| "Event".to_string());
            let time_label = if instance.all_day {
                "All day".to_string()
            } else {
                format!(
                    "{} - {}",
                    instance.start.format("%H:%M"),
                    instance.end.format("%H:%M")
                )
            };
            let label = format!("{} • {}", instance.start.format("%Y-%m-%d"), time_label);
            let button = egui::Button::new(
                egui::RichText::new(format!("{title} ({label})")).text_style(text_style.clone()),
            );
            if ui.add(button).clicked() {
                action = Some(WidgetAction {
                    action: Action {
                        label: "Open calendar".into(),
                        desc: "Calendar".into(),
                        action: "calendar:open".into(),
                        args: None,
                    },
                    query_override: None,
                });
            }
            if self.cfg.show_tags {
                if let Some(tags) = calendar.event_tags.get(&instance.source_event_id) {
                    if !tags.is_empty() {
                        let tag_label = format!("#{}", tags.join(" #"));
                        ui.label(egui::RichText::new(tag_label).text_style(egui::TextStyle::Small));
                    }
                }
            }
            ui.add_space(if self.cfg.compact { 2.0 } else { 6.0 });
        }
        action
    }
}

impl Default for CalendarWidget {
    fn default() -> Self {
        Self::new(CalendarWidgetConfig::default())
    }
}

impl Widget for CalendarWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let now = Local::now().naive_local();
        let today = now.date();
        self.ensure_selection(today);
        let selected_date = self.selected_date.unwrap_or(today);

        let snapshot = ctx.data_cache.snapshot();
        let calendar = snapshot.calendar.as_ref();

        if let Some(action) = self.render_header(ui, selected_date) {
            return Some(action);
        }

        ui.separator();
        match self.cfg.mode {
            CalendarWidgetMode::Month => {
                self.render_month_grid(ui, calendar, selected_date);
                ui.separator();
            }
            _ => {}
        }

        let list_action = self.render_event_list(ui, calendar, selected_date, now, today);
        list_action
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<CalendarWidgetConfig>(settings.clone()) {
            self.cfg = cfg;
            self.selected_date = None;
            self.view_month = None;
        }
    }
}

fn build_marker_set(snapshot: &CalendarSnapshot, view_month: NaiveDate) -> HashSet<NaiveDate> {
    let mut set = HashSet::new();
    let month = view_month.month();
    let year = view_month.year();
    for date in &snapshot.month_markers {
        if date.month() == month && date.year() == year {
            set.insert(*date);
        }
    }
    set
}

fn month_grid(
    ui: &mut egui::Ui,
    month: NaiveDate,
    markers: &HashSet<NaiveDate>,
    selected_date: NaiveDate,
    compact: bool,
    mut on_select: impl FnMut(NaiveDate),
) {
    let first_weekday = month.weekday().num_days_from_sunday() as i32;
    let days = days_in_month(month);
    let spacing = if compact { [2.0, 2.0] } else { [4.0, 4.0] };
    egui::Grid::new("calendar_widget_month_grid")
        .num_columns(7)
        .spacing(spacing)
        .show(ui, |ui| {
            for weekday in [
                chrono::Weekday::Sun,
                chrono::Weekday::Mon,
                chrono::Weekday::Tue,
                chrono::Weekday::Wed,
                chrono::Weekday::Thu,
                chrono::Weekday::Fri,
                chrono::Weekday::Sat,
            ] {
                ui.label(weekday.to_string());
            }
            ui.end_row();

            let mut day = 1;
            for i in 0..42 {
                if i % 7 == 0 && i != 0 {
                    ui.end_row();
                }
                if i < first_weekday || day > days as i32 {
                    ui.label(" ");
                    continue;
                }
                let date = NaiveDate::from_ymd_opt(month.year(), month.month(), day as u32)
                    .unwrap_or(month);
                let mut label = day.to_string();
                if markers.contains(&date) {
                    label.push('•');
                }
                let mut text = egui::RichText::new(label);
                if date == selected_date {
                    text = text.strong();
                }
                let resp = ui.selectable_label(date == selected_date, text);
                if resp.clicked() {
                    on_select(date);
                }
                day += 1;
            }
        });
}

fn days_in_month(month: NaiveDate) -> u32 {
    let next_month = add_months(month, 1);
    let last_day = next_month - Duration::days(1);
    last_day.day()
}
