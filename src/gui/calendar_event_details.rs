use crate::gui::LauncherApp;
use crate::plugins::calendar::{
    new_event_id, save_events, snooze_event, CalendarEvent, EventInstance, RecurrenceEnd,
    RecurrenceRule, CALENDAR_DATA, CALENDAR_EVENTS_FILE,
};
use chrono::{Duration, Local, NaiveDateTime};
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecurrenceScope {
    This,
    ThisAndFollowing,
    All,
}

impl RecurrenceScope {
    fn label(self) -> &'static str {
        match self {
            RecurrenceScope::This => "This",
            RecurrenceScope::ThisAndFollowing => "This & Following",
            RecurrenceScope::All => "All",
        }
    }
}

pub struct CalendarEventDetails {
    pub delete_scope: RecurrenceScope,
    pub edit_scope: RecurrenceScope,
    pub snooze_minutes: i64,
    instance_start: Option<NaiveDateTime>,
    instance_end: Option<NaiveDateTime>,
}

impl Default for CalendarEventDetails {
    fn default() -> Self {
        Self {
            delete_scope: RecurrenceScope::This,
            edit_scope: RecurrenceScope::All,
            snooze_minutes: 15,
            instance_start: None,
            instance_end: None,
        }
    }
}

impl CalendarEventDetails {
    pub fn open(&mut self, instance: EventInstance) {
        self.instance_start = Some(instance.start);
        self.instance_end = Some(instance.end);
        self.delete_scope = RecurrenceScope::This;
        self.edit_scope = RecurrenceScope::All;
        self.snooze_minutes = 15;
    }

    fn event_duration(event: &CalendarEvent) -> Duration {
        if let Some(end) = event.end {
            end - event.start
        } else if let Some(minutes) = event.duration_minutes {
            Duration::minutes(minutes)
        } else if event.all_day {
            Duration::days(1)
        } else {
            let fallback_end = event.resolved_end();
            fallback_end - event.start
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !app.calendar_details_open {
            return;
        }
        let Some(event_id) = app.calendar_selected_event.clone() else {
            app.calendar_details_open = false;
            return;
        };
        let event = {
            let data = CALENDAR_DATA.read().map(|d| d.clone()).unwrap_or_default();
            data.into_iter().find(|e| e.id == event_id)
        };
        let Some(event) = event else {
            app.calendar_details_open = false;
            return;
        };

        let title = event.title.clone();
        let start = self.instance_start.unwrap_or(event.start);
        let end = self.instance_end.unwrap_or_else(|| event.resolved_end());
        let time_label = if event.all_day {
            format!("{} (all day)", start.format("%Y-%m-%d"))
        } else {
            format!(
                "{} – {}",
                start.format("%Y-%m-%d %H:%M"),
                end.format("%Y-%m-%d %H:%M")
            )
        };
        let tags = if event.tags.is_empty() {
            "—".to_string()
        } else {
            event.tags.join(", ")
        };

        let mut open = app.calendar_details_open;
        egui::Window::new("Event Details")
            .open(&mut open)
            .resizable(true)
            .default_size((360.0, 220.0))
            .show(ctx, |ui| {
                ui.heading(&title);
                ui.label(time_label);
                ui.label(format!("Tags: {tags}"));
                if let Some(notes) = &event.notes {
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .show(ui, |ui| {
                            ui.label(notes);
                        });
                }
                ui.separator();
                if event.recurrence.is_some() {
                    ui.horizontal(|ui| {
                        ui.label("Edit scope");
                        egui::ComboBox::from_id_source("calendar_edit_scope")
                            .selected_text(self.edit_scope.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.edit_scope,
                                    RecurrenceScope::This,
                                    RecurrenceScope::This.label(),
                                );
                                ui.selectable_value(
                                    &mut self.edit_scope,
                                    RecurrenceScope::ThisAndFollowing,
                                    RecurrenceScope::ThisAndFollowing.label(),
                                );
                                ui.selectable_value(
                                    &mut self.edit_scope,
                                    RecurrenceScope::All,
                                    RecurrenceScope::All.label(),
                                );
                            });
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button("Edit").clicked() {
                        let split_scope = match (event.recurrence.is_some(), self.edit_scope) {
                            (true, RecurrenceScope::This) => Some((RecurrenceScope::This, start)),
                            (true, RecurrenceScope::ThisAndFollowing) => {
                                Some((RecurrenceScope::ThisAndFollowing, start))
                            }
                            _ => None,
                        };
                        app.open_calendar_editor(Some(event.clone()), split_scope);
                        open = false;
                    }
                    if ui.button("Duplicate").clicked() {
                        let mut events =
                            CALENDAR_DATA.read().map(|d| d.clone()).unwrap_or_default();
                        let duration = Self::event_duration(&event);
                        let new_event = CalendarEvent {
                            id: new_event_id(),
                            title: event.title.clone(),
                            start,
                            end: Some(start + duration).filter(|_| event.end.is_some()),
                            duration_minutes: event.duration_minutes,
                            all_day: event.all_day,
                            notes: event.notes.clone(),
                            recurrence: None,
                            reminders: event.reminders.clone(),
                            tags: event.tags.clone(),
                            category: event.category.clone(),
                            created_at: Local::now().naive_local(),
                            updated_at: None,
                        };
                        events.push(new_event);
                        if let Err(err) = save_events(CALENDAR_EVENTS_FILE, &events) {
                            app.set_error(format!("Failed to duplicate event: {err}"));
                        } else {
                            app.dashboard_data_cache.refresh_calendar();
                        }
                    }
                });
                if event.recurrence.is_some() {
                    ui.horizontal(|ui| {
                        ui.label("Delete scope");
                        egui::ComboBox::from_id_source("calendar_delete_scope")
                            .selected_text(self.delete_scope.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.delete_scope,
                                    RecurrenceScope::This,
                                    RecurrenceScope::This.label(),
                                );
                                ui.selectable_value(
                                    &mut self.delete_scope,
                                    RecurrenceScope::ThisAndFollowing,
                                    RecurrenceScope::ThisAndFollowing.label(),
                                );
                                ui.selectable_value(
                                    &mut self.delete_scope,
                                    RecurrenceScope::All,
                                    RecurrenceScope::All.label(),
                                );
                            });
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        if let Err(err) = delete_event_with_scope(&event, start, self.delete_scope)
                        {
                            app.set_error(format!("Failed to delete event: {err}"));
                        } else {
                            app.dashboard_data_cache.refresh_calendar();
                            open = false;
                            app.calendar_selected_event = None;
                        }
                    }
                    ui.separator();
                    ui.label("Snooze (minutes)");
                    ui.add(egui::DragValue::new(&mut self.snooze_minutes).clamp_range(1..=1440));
                    if ui.button("Snooze").clicked() {
                        if let Err(err) =
                            snooze_event(&event.id, Duration::minutes(self.snooze_minutes))
                        {
                            app.set_error(format!("Failed to snooze event: {err}"));
                        } else {
                            app.dashboard_data_cache.refresh_calendar();
                        }
                    }
                });
            });
        app.calendar_details_open = open;
    }
}

fn delete_event_with_scope(
    event: &CalendarEvent,
    occurrence_start: NaiveDateTime,
    scope: RecurrenceScope,
) -> anyhow::Result<()> {
    let mut events = CALENDAR_DATA.read().map(|d| d.clone()).unwrap_or_default();
    match (event.recurrence.clone(), scope) {
        (_, RecurrenceScope::All) | (None, _) => {
            events.retain(|e| e.id != event.id);
        }
        (Some(rule), RecurrenceScope::ThisAndFollowing) => {
            if occurrence_start <= event.start {
                events.retain(|e| e.id != event.id);
            } else {
                let end_date = occurrence_start.date() - chrono::Duration::days(1);
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
            if occurrence_start <= event.start {
                if let Some(next) = next_occurrence(event, occurrence_start) {
                    let mut updated = event.clone();
                    updated.start = next;
                    events.retain(|e| e.id != event.id);
                    events.push(updated);
                } else {
                    events.retain(|e| e.id != event.id);
                }
            } else {
                let end_date = occurrence_start.date() - chrono::Duration::days(1);
                let mut updated = event.clone();
                updated.recurrence = Some(RecurrenceRule {
                    end: RecurrenceEnd::OnDate { date: end_date },
                    ..rule
                });
                events.retain(|e| e.id != event.id);
                events.push(updated);
                if let Some(next) = next_occurrence(event, occurrence_start) {
                    let mut next_event = event.clone();
                    next_event.id = new_event_id();
                    next_event.start = next;
                    next_event.updated_at = Some(Local::now().naive_local());
                    events.push(next_event);
                }
            }
        }
    }
    save_events(CALENDAR_EVENTS_FILE, &events)?;
    Ok(())
}

fn next_occurrence(event: &CalendarEvent, after: NaiveDateTime) -> Option<NaiveDateTime> {
    let events = vec![event.clone()];
    let start = after + Duration::seconds(1);
    let end = after + Duration::days(400);
    let instances = crate::plugins::calendar::expand_instances(&events, start, end, 2);
    instances.first().map(|i| i.start)
}
