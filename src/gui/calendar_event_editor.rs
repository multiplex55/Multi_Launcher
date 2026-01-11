use crate::gui::calendar_event_details::RecurrenceScope;
use crate::gui::LauncherApp;
use crate::plugins::calendar::{
    new_event_id, save_events, CalendarEvent, CustomRecurrenceUnit, RecurrenceEnd,
    RecurrenceFrequency, RecurrenceRule, Reminder, CALENDAR_DATA, CALENDAR_EVENTS_FILE,
};
use chrono::{Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use eframe::egui;
use std::collections::BTreeMap;

pub struct CalendarEventEditor {
    title: String,
    start_date: String,
    start_time: String,
    all_day: bool,
    end_date: String,
    end_time: String,
    duration_minutes: String,
    notes: String,
    tags: String,
    reminders: String,
    repeat_enabled: bool,
    repeat_frequency: RecurrenceFrequency,
    repeat_interval: u32,
    repeat_end_mode: RecurrenceEndMode,
    repeat_end_date: String,
    repeat_end_count: u32,
    repeat_custom_unit: CustomRecurrenceUnit,
    weekly_days: Vec<Weekday>,
    errors: BTreeMap<&'static str, String>,
    current_event_id: Option<String>,
    split_from: Option<SplitScope>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecurrenceEndMode {
    Never,
    OnDate,
    AfterCount,
}

#[derive(Clone, Debug)]
struct SplitScope {
    event_id: String,
    occurrence_start: NaiveDateTime,
    scope: RecurrenceScope,
}

impl Default for CalendarEventEditor {
    fn default() -> Self {
        Self {
            title: String::new(),
            start_date: String::new(),
            start_time: String::new(),
            all_day: false,
            end_date: String::new(),
            end_time: String::new(),
            duration_minutes: String::new(),
            notes: String::new(),
            tags: String::new(),
            reminders: String::new(),
            repeat_enabled: false,
            repeat_frequency: RecurrenceFrequency::Daily,
            repeat_interval: 1,
            repeat_end_mode: RecurrenceEndMode::Never,
            repeat_end_date: String::new(),
            repeat_end_count: 1,
            repeat_custom_unit: CustomRecurrenceUnit::Days,
            weekly_days: Vec::new(),
            errors: BTreeMap::new(),
            current_event_id: None,
            split_from: None,
        }
    }
}

impl CalendarEventEditor {
    pub fn open_new(&mut self, date: NaiveDate) {
        let now = Local::now().naive_local();
        self.title.clear();
        self.start_date = date.format("%Y-%m-%d").to_string();
        self.start_time = now.format("%H:%M").to_string();
        self.all_day = false;
        self.end_date.clear();
        self.end_time.clear();
        self.duration_minutes.clear();
        self.notes.clear();
        self.tags.clear();
        self.reminders.clear();
        self.repeat_enabled = false;
        self.repeat_frequency = RecurrenceFrequency::Daily;
        self.repeat_interval = 1;
        self.repeat_end_mode = RecurrenceEndMode::Never;
        self.repeat_end_date.clear();
        self.repeat_end_count = 1;
        self.repeat_custom_unit = CustomRecurrenceUnit::Days;
        self.weekly_days.clear();
        self.errors.clear();
        self.current_event_id = None;
        self.split_from = None;
    }

    pub fn open_edit(
        &mut self,
        event: &CalendarEvent,
        occurrence_start: Option<NaiveDateTime>,
        split_scope: Option<SplitScope>,
    ) {
        let start = occurrence_start.unwrap_or(event.start);
        let end = if let Some(occurrence) = occurrence_start {
            occurrence + event_duration(event)
        } else {
            event.resolved_end()
        };
        self.title = event.title.clone();
        self.start_date = start.format("%Y-%m-%d").to_string();
        self.start_time = start.format("%H:%M").to_string();
        self.all_day = event.all_day;
        self.end_date = end.format("%Y-%m-%d").to_string();
        self.end_time = end.format("%H:%M").to_string();
        self.duration_minutes = event
            .duration_minutes
            .map(|v| v.to_string())
            .unwrap_or_default();
        self.notes = event.notes.clone().unwrap_or_default();
        self.tags = event.tags.join(", ");
        self.reminders = event
            .reminders
            .iter()
            .map(|r| r.minutes_before.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        if let Some(rule) = &event.recurrence {
            self.repeat_enabled = true;
            self.repeat_frequency = rule.frequency.clone();
            self.repeat_interval = rule.interval;
            self.weekly_days = rule.weekly_days.clone();
            match rule.end {
                RecurrenceEnd::Never => self.repeat_end_mode = RecurrenceEndMode::Never,
                RecurrenceEnd::OnDate { date } => {
                    self.repeat_end_mode = RecurrenceEndMode::OnDate;
                    self.repeat_end_date = date.format("%Y-%m-%d").to_string();
                }
                RecurrenceEnd::AfterCount { count } => {
                    self.repeat_end_mode = RecurrenceEndMode::AfterCount;
                    self.repeat_end_count = count;
                }
            }
            self.repeat_custom_unit = rule
                .custom_unit
                .clone()
                .unwrap_or(CustomRecurrenceUnit::Days);
        } else {
            self.repeat_enabled = false;
        }
        self.errors.clear();
        self.split_from = split_scope;
        if let Some(split) = &self.split_from {
            if matches!(split.scope, RecurrenceScope::This) {
                self.repeat_enabled = false;
            }
        }
        if self.split_from.is_some() {
            self.current_event_id = None;
        } else {
            self.current_event_id = Some(event.id.clone());
        }
    }

    pub fn open(
        &mut self,
        event: Option<CalendarEvent>,
        split_scope: Option<(RecurrenceScope, NaiveDateTime)>,
    ) {
        if let Some(event) = event {
            let mut occurrence = None;
            let scope = split_scope.map(|(scope, occurrence_start)| {
                occurrence = Some(occurrence_start);
                SplitScope {
                    event_id: event.id.clone(),
                    occurrence_start,
                    scope,
                }
            });
            self.open_edit(&event, occurrence, scope);
        } else {
            let today = Local::now().naive_local().date();
            self.open_new(today);
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !app.calendar_editor_open {
            return;
        }
        let mut open = app.calendar_editor_open;
        egui::Window::new("Calendar Event")
            .open(&mut open)
            .resizable(true)
            .default_size((420.0, 400.0))
            .show(ctx, |ui| {
                ui.label("Title");
                ui.text_edit_singleline(&mut self.title);
                if let Some(err) = self.errors.get("title") {
                    ui.colored_label(egui::Color32::RED, err);
                }

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Start Date (YYYY-MM-DD)");
                        ui.text_edit_singleline(&mut self.start_date);
                        if let Some(err) = self.errors.get("start_date") {
                            ui.colored_label(egui::Color32::RED, err);
                        }
                    });
                    ui.vertical(|ui| {
                        ui.label("Start Time (HH:MM)");
                        ui.text_edit_singleline(&mut self.start_time);
                        if let Some(err) = self.errors.get("start_time") {
                            ui.colored_label(egui::Color32::RED, err);
                        }
                    });
                });
                ui.checkbox(&mut self.all_day, "All day");

                ui.separator();
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label("End Date (optional)");
                        ui.text_edit_singleline(&mut self.end_date);
                        if let Some(err) = self.errors.get("end_date") {
                            ui.colored_label(egui::Color32::RED, err);
                        }
                    });
                    ui.vertical(|ui| {
                        ui.label("End Time (optional)");
                        ui.text_edit_singleline(&mut self.end_time);
                        if let Some(err) = self.errors.get("end_time") {
                            ui.colored_label(egui::Color32::RED, err);
                        }
                    });
                });
                ui.label("Duration (minutes, optional)");
                ui.text_edit_singleline(&mut self.duration_minutes);
                if let Some(err) = self.errors.get("duration") {
                    ui.colored_label(egui::Color32::RED, err);
                }

                ui.separator();
                ui.label("Notes");
                ui.add(
                    egui::TextEdit::multiline(&mut self.notes)
                        .desired_width(f32::INFINITY)
                        .desired_rows(4),
                );

                ui.separator();
                ui.label("Repeat");
                ui.checkbox(&mut self.repeat_enabled, "Enable recurrence");
                if self.repeat_enabled {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_source("calendar_repeat_frequency")
                            .selected_text(format!("{:?}", self.repeat_frequency))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.repeat_frequency,
                                    RecurrenceFrequency::Daily,
                                    "Daily",
                                );
                                ui.selectable_value(
                                    &mut self.repeat_frequency,
                                    RecurrenceFrequency::Weekly,
                                    "Weekly",
                                );
                                ui.selectable_value(
                                    &mut self.repeat_frequency,
                                    RecurrenceFrequency::Monthly,
                                    "Monthly",
                                );
                                ui.selectable_value(
                                    &mut self.repeat_frequency,
                                    RecurrenceFrequency::Yearly,
                                    "Yearly",
                                );
                                ui.selectable_value(
                                    &mut self.repeat_frequency,
                                    RecurrenceFrequency::Custom,
                                    "Custom",
                                );
                            });
                        ui.label("Every");
                        ui.add(
                            egui::DragValue::new(&mut self.repeat_interval).clamp_range(1..=365),
                        );
                    });
                    if self.repeat_frequency == RecurrenceFrequency::Custom {
                        egui::ComboBox::from_id_source("calendar_repeat_custom")
                            .selected_text(format!("{:?}", self.repeat_custom_unit))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.repeat_custom_unit,
                                    CustomRecurrenceUnit::Days,
                                    "Days",
                                );
                                ui.selectable_value(
                                    &mut self.repeat_custom_unit,
                                    CustomRecurrenceUnit::Weeks,
                                    "Weeks",
                                );
                                ui.selectable_value(
                                    &mut self.repeat_custom_unit,
                                    CustomRecurrenceUnit::Months,
                                    "Months",
                                );
                                ui.selectable_value(
                                    &mut self.repeat_custom_unit,
                                    CustomRecurrenceUnit::Years,
                                    "Years",
                                );
                            });
                    }
                    if self.repeat_frequency == RecurrenceFrequency::Weekly {
                        ui.horizontal_wrapped(|ui| {
                            for weekday in [
                                Weekday::Mon,
                                Weekday::Tue,
                                Weekday::Wed,
                                Weekday::Thu,
                                Weekday::Fri,
                                Weekday::Sat,
                                Weekday::Sun,
                            ] {
                                let selected = self.weekly_days.contains(&weekday);
                                if ui.selectable_label(selected, weekday.to_string()).clicked() {
                                    if selected {
                                        self.weekly_days.retain(|d| *d != weekday);
                                    } else {
                                        self.weekly_days.push(weekday);
                                    }
                                }
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label("Ends");
                        egui::ComboBox::from_id_source("calendar_repeat_end")
                            .selected_text(match self.repeat_end_mode {
                                RecurrenceEndMode::Never => "Never",
                                RecurrenceEndMode::OnDate => "On date",
                                RecurrenceEndMode::AfterCount => "After count",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.repeat_end_mode,
                                    RecurrenceEndMode::Never,
                                    "Never",
                                );
                                ui.selectable_value(
                                    &mut self.repeat_end_mode,
                                    RecurrenceEndMode::OnDate,
                                    "On date",
                                );
                                ui.selectable_value(
                                    &mut self.repeat_end_mode,
                                    RecurrenceEndMode::AfterCount,
                                    "After count",
                                );
                            });
                    });
                    match self.repeat_end_mode {
                        RecurrenceEndMode::OnDate => {
                            ui.text_edit_singleline(&mut self.repeat_end_date);
                            if let Some(err) = self.errors.get("repeat_end_date") {
                                ui.colored_label(egui::Color32::RED, err);
                            }
                        }
                        RecurrenceEndMode::AfterCount => {
                            ui.add(
                                egui::DragValue::new(&mut self.repeat_end_count)
                                    .clamp_range(1..=999),
                            );
                        }
                        RecurrenceEndMode::Never => {}
                    }
                }

                ui.separator();
                ui.label("Reminders (minutes before, comma separated)");
                ui.text_edit_singleline(&mut self.reminders);
                if let Some(err) = self.errors.get("reminders") {
                    ui.colored_label(egui::Color32::RED, err);
                }
                ui.label("Tags (comma separated)");
                ui.text_edit_singleline(&mut self.tags);

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        if let Err(err) = self.save(app) {
                            app.set_error(err);
                        } else {
                            open = false;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        open = false;
                    }
                });
            });
        app.calendar_editor_open = open;
    }

    fn save(&mut self, app: &mut LauncherApp) -> Result<(), String> {
        self.errors.clear();
        let title = self.title.trim();
        if title.is_empty() {
            self.errors.insert("title", "Title is required".into());
        }
        let start_date = parse_date(&self.start_date).map_err(|err| {
            self.errors.insert("start_date", err.clone());
            err
        })?;
        let start_time = if self.all_day {
            NaiveTime::from_hms_opt(0, 0, 0).unwrap()
        } else {
            parse_time(&self.start_time).map_err(|err| {
                self.errors.insert("start_time", err.clone());
                err
            })?
        };
        let start = NaiveDateTime::new(start_date, start_time);

        let end_date = if self.end_date.trim().is_empty() {
            None
        } else {
            Some(parse_date(&self.end_date).map_err(|err| {
                self.errors.insert("end_date", err.clone());
                err
            })?)
        };
        let end_time = if self.end_time.trim().is_empty() {
            None
        } else {
            Some(parse_time(&self.end_time).map_err(|err| {
                self.errors.insert("end_time", err.clone());
                err
            })?)
        };
        let end = match (end_date, end_time) {
            (Some(date), Some(time)) => Some(NaiveDateTime::new(date, time)),
            (Some(date), None) => Some(NaiveDateTime::new(
                date,
                NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
            )),
            (None, Some(_)) => {
                self.errors.insert("end_date", "End date required".into());
                None
            }
            (None, None) => None,
        };

        let duration_minutes = if self.duration_minutes.trim().is_empty() {
            None
        } else {
            match self.duration_minutes.trim().parse::<i64>() {
                Ok(value) if value > 0 => Some(value),
                _ => {
                    self.errors
                        .insert("duration", "Duration must be a positive number".into());
                    None
                }
            }
        };

        if let Some(end) = end {
            if end < start {
                self.errors
                    .insert("end_date", "End must be after start".into());
            }
        }

        let reminders = parse_reminders(&self.reminders).map_err(|err| {
            self.errors.insert("reminders", err.clone());
            err
        })?;

        let recurrence = if self.repeat_enabled {
            let end = match self.repeat_end_mode {
                RecurrenceEndMode::Never => RecurrenceEnd::Never,
                RecurrenceEndMode::OnDate => {
                    let date = parse_date(&self.repeat_end_date).map_err(|err| {
                        self.errors.insert("repeat_end_date", err.clone());
                        err
                    })?;
                    RecurrenceEnd::OnDate { date }
                }
                RecurrenceEndMode::AfterCount => RecurrenceEnd::AfterCount {
                    count: self.repeat_end_count,
                },
            };
            Some(RecurrenceRule {
                frequency: self.repeat_frequency.clone(),
                interval: self.repeat_interval,
                weekly_days: self.weekly_days.clone(),
                nth_weekday: None,
                end,
                custom_unit: if self.repeat_frequency == RecurrenceFrequency::Custom {
                    Some(self.repeat_custom_unit.clone())
                } else {
                    None
                },
            })
        } else {
            None
        };

        if !self.errors.is_empty() {
            return Err("Fix validation errors".into());
        }

        let mut events = CALENDAR_DATA.read().map(|d| d.clone()).unwrap_or_default();
        let now = Local::now().naive_local();
        let existing = self
            .current_event_id
            .as_ref()
            .and_then(|id| events.iter().find(|e| &e.id == id).cloned());
        let id = self.current_event_id.clone().unwrap_or_else(new_event_id);
        let event = CalendarEvent {
            id: id.clone(),
            title: title.to_string(),
            start,
            end,
            duration_minutes,
            all_day: self.all_day,
            notes: if self.notes.trim().is_empty() {
                None
            } else {
                Some(self.notes.clone())
            },
            recurrence,
            reminders,
            tags: parse_tags(&self.tags),
            category: None,
            created_at: existing.as_ref().map(|e| e.created_at).unwrap_or(now),
            updated_at: Some(now),
        };

        if let Some(scope) = self.split_from.take() {
            apply_split_scope(&mut events, &scope).map_err(|err| err.to_string())?;
            events.push(event);
        } else if let Some(pos) = events.iter().position(|e| e.id == id) {
            events[pos] = event;
        } else {
            events.push(event);
        }

        save_events(CALENDAR_EVENTS_FILE, &events).map_err(|e| e.to_string())?;
        app.dashboard_data_cache.refresh_calendar();
        app.calendar_selected_event = Some(id);
        Ok(())
    }
}

fn parse_date(input: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(input.trim(), "%Y-%m-%d").map_err(|_| "Use YYYY-MM-DD".to_string())
}

fn parse_time(input: &str) -> Result<NaiveTime, String> {
    NaiveTime::parse_from_str(input.trim(), "%H:%M").map_err(|_| "Use HH:MM".to_string())
}

fn parse_reminders(input: &str) -> Result<Vec<Reminder>, String> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut reminders = Vec::new();
    for part in input.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value = trimmed
            .parse::<i64>()
            .map_err(|_| "Reminders must be minutes".to_string())?;
        reminders.push(Reminder {
            minutes_before: value,
        });
    }
    Ok(reminders)
}

fn parse_tags(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .map(|tag| tag.to_string())
        .collect()
}

fn apply_split_scope(events: &mut Vec<CalendarEvent>, scope: &SplitScope) -> anyhow::Result<()> {
    let Some(event) = events.iter().find(|e| e.id == scope.event_id).cloned() else {
        return Ok(());
    };
    match (event.recurrence.clone(), scope.scope) {
        (Some(rule), RecurrenceScope::ThisAndFollowing) => {
            if scope.occurrence_start <= event.start {
                events.retain(|e| e.id != event.id);
            } else {
                let end_date = scope.occurrence_start.date() - Duration::days(1);
                let mut updated = event.clone();
                updated.recurrence = Some(RecurrenceRule {
                    end: RecurrenceEnd::OnDate { date: end_date },
                    ..rule
                });
                events.retain(|e| e.id != event.id);
                events.push(updated);
            }
        }
        (Some(rule), RecurrenceScope::This) => {
            if scope.occurrence_start <= event.start {
                if let Some(next) = next_occurrence(&event, scope.occurrence_start) {
                    let mut updated = event.clone();
                    updated.start = next;
                    events.retain(|e| e.id != event.id);
                    events.push(updated);
                } else {
                    events.retain(|e| e.id != event.id);
                }
            } else {
                let end_date = scope.occurrence_start.date() - Duration::days(1);
                let mut updated = event.clone();
                updated.recurrence = Some(RecurrenceRule {
                    end: RecurrenceEnd::OnDate { date: end_date },
                    ..rule
                });
                events.retain(|e| e.id != event.id);
                events.push(updated);
                if let Some(next) = next_occurrence(&event, scope.occurrence_start) {
                    let mut next_event = event.clone();
                    next_event.id = new_event_id();
                    next_event.start = next;
                    next_event.updated_at = Some(Local::now().naive_local());
                    events.push(next_event);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn next_occurrence(event: &CalendarEvent, after: NaiveDateTime) -> Option<NaiveDateTime> {
    let events = vec![event.clone()];
    let start = after + Duration::seconds(1);
    let end = after + Duration::days(400);
    let instances = crate::plugins::calendar::expand_instances(&events, start, end, 2);
    instances.first().map(|i| i.start)
}

fn event_duration(event: &CalendarEvent) -> Duration {
    if let Some(end) = event.end {
        end - event.start
    } else if let Some(minutes) = event.duration_minutes {
        Duration::minutes(minutes)
    } else if event.all_day {
        Duration::days(1)
    } else {
        event.resolved_end() - event.start
    }
}
