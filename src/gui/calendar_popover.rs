use crate::gui::LauncherApp;
use crate::plugins::calendar::{add_months, expand_instances, CalendarSnapshot, CALENDAR_DATA};
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use eframe::egui;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalendarViewMode {
    Day,
    Week,
}

pub struct CalendarPopover {
    view_month: NaiveDate,
    view_mode: CalendarViewMode,
}

impl Default for CalendarPopover {
    fn default() -> Self {
        let today = Local::now().naive_local().date();
        Self {
            view_month: NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today),
            view_mode: CalendarViewMode::Day,
        }
    }
}

impl CalendarPopover {
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !app.calendar_popover_open {
            return;
        }
        let today = Local::now().naive_local().date();
        if app.calendar_selected_date.is_none() {
            app.calendar_selected_date = Some(today);
        }
        let selected_date = app.calendar_selected_date.unwrap_or(today);
        let selected_month =
            NaiveDate::from_ymd_opt(selected_date.year(), selected_date.month(), 1)
                .unwrap_or(selected_date);
        if self.view_month != selected_month {
            self.view_month = selected_month;
        }

        handle_keyboard(ctx, app, &mut self.view_month);

        let snapshot = app.dashboard_data_cache.snapshot();
        let calendar = snapshot.calendar.as_ref();
        let markers = build_marker_set(calendar, self.view_month);

        egui::Window::new("Calendar")
            .open(&mut app.calendar_popover_open)
            .resizable(true)
            .default_size((480.0, 320.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("<").clicked() {
                        shift_month(app, -1, &mut self.view_month);
                    }
                    if ui.button(">").clicked() {
                        shift_month(app, 1, &mut self.view_month);
                    }
                    if ui
                        .button(self.view_month.format("%B %Y").to_string())
                        .clicked()
                    {
                        app.calendar_selected_date = Some(today);
                        self.view_month = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
                            .unwrap_or(today);
                    }
                    ui.separator();
                    ui.label("View");
                    egui::ComboBox::from_id_source("calendar_view_mode")
                        .selected_text(match self.view_mode {
                            CalendarViewMode::Day => "Day",
                            CalendarViewMode::Week => "Week",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.view_mode, CalendarViewMode::Day, "Day");
                            ui.selectable_value(
                                &mut self.view_mode,
                                CalendarViewMode::Week,
                                "Week",
                            );
                        });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        month_grid(ui, app, self.view_month, &markers, today);
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        agenda_view(ui, app, self.view_mode, selected_date);
                    });
                });
            });
    }
}

fn handle_keyboard(ctx: &egui::Context, app: &mut LauncherApp, view_month: &mut NaiveDate) {
    let Some(selected_date) = app.calendar_selected_date else {
        return;
    };
    let mut updated = None;
    if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
        updated = Some(selected_date - Duration::days(1));
    } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
        updated = Some(selected_date + Duration::days(1));
    } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
        updated = Some(selected_date - Duration::days(7));
    } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
        updated = Some(selected_date + Duration::days(7));
    } else if ctx.input(|i| i.key_pressed(egui::Key::PageUp)) {
        let shifted = shift_date_month(selected_date, -1);
        updated = Some(shifted);
    } else if ctx.input(|i| i.key_pressed(egui::Key::PageDown)) {
        let shifted = shift_date_month(selected_date, 1);
        updated = Some(shifted);
    } else if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        if let Some(event_id) = app.calendar_selected_event.clone() {
            if let Some(event) = CALENDAR_DATA
                .read()
                .map(|d| d.clone())
                .unwrap_or_default()
                .into_iter()
                .find(|e| e.id == event_id)
            {
                app.open_calendar_editor(Some(event), None);
            }
        } else {
            app.open_calendar_editor(None, None);
        }
    }
    if let Some(new_date) = updated {
        app.calendar_selected_date = Some(new_date);
        app.calendar_selected_event = None;
        let month_start =
            NaiveDate::from_ymd_opt(new_date.year(), new_date.month(), 1).unwrap_or(new_date);
        *view_month = month_start;
    }
}

fn shift_month(app: &mut LauncherApp, delta: i32, view_month: &mut NaiveDate) {
    if let Some(date) = app.calendar_selected_date {
        let shifted = shift_date_month(date, delta);
        app.calendar_selected_date = Some(shifted);
        app.calendar_selected_event = None;
        *view_month =
            NaiveDate::from_ymd_opt(shifted.year(), shifted.month(), 1).unwrap_or(shifted);
    } else {
        *view_month = add_months(*view_month, delta);
    }
}

fn shift_date_month(date: NaiveDate, delta: i32) -> NaiveDate {
    let base = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date);
    let shifted = add_months(base, delta);
    let days = days_in_month(shifted);
    let day = date.day().min(days);
    NaiveDate::from_ymd_opt(shifted.year(), shifted.month(), day).unwrap_or(shifted)
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
    app: &mut LauncherApp,
    month: NaiveDate,
    markers: &HashSet<NaiveDate>,
    today: NaiveDate,
) {
    let first_weekday = month.weekday().num_days_from_monday() as i32;
    let days = days_in_month(month);
    egui::Grid::new("calendar_month_grid")
        .num_columns(7)
        .spacing([4.0, 4.0])
        .show(ui, |ui| {
            for weekday in [
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
                Weekday::Sat,
                Weekday::Sun,
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
                if date == today {
                    text = text.color(egui::Color32::LIGHT_BLUE);
                }
                if Some(date) == app.calendar_selected_date {
                    text = text.strong();
                }
                let resp = ui.selectable_label(Some(date) == app.calendar_selected_date, text);
                if resp.clicked() {
                    app.calendar_selected_date = Some(date);
                    app.calendar_selected_event = None;
                }
                day += 1;
            }
        });
}

fn agenda_view(ui: &mut egui::Ui, app: &mut LauncherApp, mode: CalendarViewMode, date: NaiveDate) {
    let start = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let end = match mode {
        CalendarViewMode::Day => start + Duration::days(1),
        CalendarViewMode::Week => start + Duration::days(7),
    };
    ui.horizontal(|ui| {
        ui.label(match mode {
            CalendarViewMode::Day => "Agenda",
            CalendarViewMode::Week => "Week agenda",
        });
        if ui.button("+ Add").clicked() {
            app.open_calendar_editor(None, None);
        }
    });
    let events = CALENDAR_DATA.read().map(|d| d.clone()).unwrap_or_default();
    let title_map = events
        .iter()
        .map(|event| (event.id.clone(), event.title.clone()))
        .collect::<HashMap<_, _>>();
    let instances = expand_instances(&events, start, end, 64);
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .show(ui, |ui| {
            if instances.is_empty() {
                ui.label("No events");
            }
            for instance in instances {
                let title = title_map
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
                let label = format!("{title} ({time_label})");
                let resp = ui.selectable_label(false, label);
                if resp.clicked() {
                    app.calendar_selected_event = Some(instance.source_event_id.clone());
                    app.calendar_selected_date = Some(instance.start.date());
                    app.calendar_event_details.open(instance);
                    app.calendar_details_open = true;
                }
            }
        });
}

fn days_in_month(month: NaiveDate) -> u32 {
    let next_month = add_months(month, 1);
    let last_day = next_month - Duration::days(1);
    last_day.day()
}
