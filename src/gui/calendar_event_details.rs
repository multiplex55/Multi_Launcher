use crate::dashboard::DashboardRefreshRequest;
use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{
    ButtonEmphasis, compact_section_heading, secondary_help, with_button_emphasis,
};
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::plugins::calendar::{
    CALENDAR_DATA, CalendarEvent, EventInstance, RecurrenceEnd, RecurrenceRule, expand_instances,
    new_event_id, snooze_event_at, update_events,
};
use chrono::{Duration, Local, NaiveDateTime, Timelike};
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

pub(crate) enum CalendarDetailsDispatch {
    OpenEditor(CalendarEvent, Option<(RecurrenceScope, NaiveDateTime)>),
}

enum CalendarDetailsEffect {
    Edit(CalendarEvent, Option<(RecurrenceScope, NaiveDateTime)>),
    Duplicate(String, NaiveDateTime),
    Delete(CalendarEvent, NaiveDateTime, RecurrenceScope),
    Snooze(String, i64),
}
#[derive(Default)]
pub(crate) struct CalendarDetailsHostActions {
    pub(super) host_request: Option<super::utility_host::UtilityPresentation>,
    pub(crate) close: bool,
    epoch: u64,
    effect: Option<CalendarDetailsEffect>,
    #[cfg(test)]
    pub(crate) host_button: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    pub(crate) edit_button: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    pub(crate) delete_button: Option<(egui::Rect, egui::Rect, bool)>,
}

#[derive(Clone)]
pub struct CalendarEventDetails {
    host_session_epoch: u64,
    host_popups: Vec<super::calendar_window_host::CalendarPopupAnchor>,
    pub delete_scope: RecurrenceScope,
    pub edit_scope: RecurrenceScope,
    pub snooze_minutes: i64,
    instance: Option<EventInstance>,
}

impl Default for CalendarEventDetails {
    fn default() -> Self {
        Self {
            host_session_epoch: 0,
            host_popups: Vec::new(),
            delete_scope: RecurrenceScope::This,
            edit_scope: RecurrenceScope::All,
            snooze_minutes: 15,
            instance: None,
        }
    }
}

impl CalendarEventDetails {
    pub fn open(&mut self, instance: EventInstance) {
        self.host_session_epoch = self.host_session_epoch.wrapping_add(1).max(1);
        self.instance = Some(instance);
        self.delete_scope = RecurrenceScope::This;
        self.edit_scope = RecurrenceScope::All;
        self.snooze_minutes = 15;
    }

    pub(super) fn shows_occurrence(&self, instance: &EventInstance) -> bool {
        self.instance.as_ref().is_some_and(|current| {
            current.instance_id == instance.instance_id
                && current.source_event_id == instance.source_event_id
                && current.start == instance.start
                && current.end == instance.end
                && current.all_day == instance.all_day
                && match (&current.recurrence, &instance.recurrence) {
                    (Some(left), Some(right)) => left.occurrence_index == right.occurrence_index && left.rule == right.rule,
                    (None, None) => true,
                    _ => false,
                }
        })
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

    #[cfg(test)]
    pub(crate) fn test_instance(&self) -> Option<&EventInstance> {
        self.instance.as_ref()
    }

    pub(crate) fn host_session_epoch(&self) -> u64 {
        self.host_session_epoch
    }
    pub(super) fn popup_anchors(&self) -> &[super::calendar_window_host::CalendarPopupAnchor] {
        &self.host_popups
    }

    pub(super) fn owned_popup_layer(
        &self,
        ctx: &egui::Context,
        parent: egui::LayerId,
    ) -> Option<egui::LayerId> {
        super::calendar_window_host::owned_popup_layer(
            &self.host_popups,
            ctx,
            parent,
            self.host_session_epoch,
        )
    }

    pub(super) fn render_host(
        &mut self,
        ctx: &egui::Context,
        open: bool,
        owner: bool,
        first: bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: super::utility_host::UtilityPresentation,
    ) -> CalendarDetailsHostActions {
        self.render_inner(ctx, open, owner, first, child_ui, Some(presentation))
    }
    fn render_inner(
        &mut self,
        ctx: &egui::Context,
        visible: bool,
        owner: bool,
        first: bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: Option<super::utility_host::UtilityPresentation>,
    ) -> CalendarDetailsHostActions {
        let mut actions = CalendarDetailsHostActions {
            epoch: self.host_session_epoch,
            ..Default::default()
        };
        if !visible {
            return actions;
        }
        let Some(instance) = self.instance.clone() else {
            actions.close = true;
            return actions;
        };
        let event = {
            CALENDAR_DATA.read().ok().and_then(|data| {
                data.iter()
                    .find(|e| e.id == instance.source_event_id)
                    .cloned()
            })
        };
        let Some(event) = event.filter(|event| {
            calendar_occurrence_is_current(event, instance.start, Some(instance.end))
        }) else {
            actions.close = true;
            return actions;
        };

        let title = event.title.clone();
        let start = instance.start;
        let end = instance.end;
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

        let mut open = visible;
        self.host_popups.clear();
        let mut draw_body = |ui: &mut egui::Ui| {
            let allowed =
                owner && first && ui.is_visible() && !ui.is_sizing_pass() && !ctx.will_discard();
            if let Some(current) = presentation {
                let (response, requested) = super::utility_host::presentation_button(
                    ui,
                    current,
                    owner && ui.is_visible() && !ui.is_sizing_pass(),
                );
                #[cfg(test)]
                {
                    actions.host_button = Some((response.rect, ui.clip_rect(), response.enabled()));
                }
                if response.clicked() && allowed {
                    actions.host_request = Some(requested);
                }
            }
            ui.add_enabled_ui(owner, |ui| {
                let body_height = ui.available_height().max(0.0);
                egui::ScrollArea::vertical()
                    .id_salt("calendar_event_details_body")
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .max_height(body_height)
                    .show(ui, |ui| {
                        ui.set_max_width(ui.available_width().max(0.0));
                        compact_section_heading(ui, "Event", None);
                        ui.add(egui::Label::new(egui::RichText::new(&title).heading()).wrap());
                        secondary_help(ui, &time_label);
                        ui.add(egui::Label::new(format!("Tags: {tags}")).wrap());

                        if let Some(notes) = &event.notes {
                            compact_section_heading(ui, "Notes", None);
                            egui::ScrollArea::vertical()
                                .max_height(120.0)
                                .show(ui, |ui| {
                                    ui.add(egui::Label::new(notes).wrap());
                                });
                        }

                        ui.separator();
                        compact_section_heading(ui, "Actions", None);
                        ui.horizontal_wrapped(|ui| {
                            let edit = ui.button("Edit");
                            let edit = with_button_emphasis(ui, edit, ButtonEmphasis::Primary);
                            #[cfg(test)]
                            {
                                actions.edit_button =
                                    Some((edit.rect, ui.clip_rect(), edit.enabled()));
                            }
                            if edit.clicked() && allowed {
                                let split_scope =
                                    match (event.recurrence.is_some(), self.edit_scope) {
                                        (true, RecurrenceScope::This) => {
                                            Some((RecurrenceScope::This, start))
                                        }
                                        (true, RecurrenceScope::ThisAndFollowing) => {
                                            Some((RecurrenceScope::ThisAndFollowing, start))
                                        }
                                        _ => None,
                                    };
                                actions.effect =
                                    Some(CalendarDetailsEffect::Edit(event.clone(), split_scope));
                            }
                            if ui.button("Duplicate").clicked() && allowed {
                                actions.effect =
                                    Some(CalendarDetailsEffect::Duplicate(event.id.clone(), start));
                            }
                        });

                        if event.recurrence.is_some() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Edit scope");
                                let combo = egui::ComboBox::from_id_salt("calendar_edit_scope")
                                    .selected_text(self.edit_scope.label())
                                    .show_ui(ui, |ui| {
                                        ui.add_enabled_ui(allowed, |ui| {
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
                                self.host_popups.push(
                                    super::calendar_window_host::CalendarPopupAnchor::new(
                                        ui,
                                        &combo.response,
                                        self.host_session_epoch,
                                    ),
                                );
                            });
                        }

                        ui.horizontal_wrapped(|ui| {
                            let delete = ui.button("Delete");
                            let delete =
                                with_button_emphasis(ui, delete, ButtonEmphasis::Destructive);
                            #[cfg(test)]
                            {
                                actions.delete_button =
                                    Some((delete.rect, ui.clip_rect(), delete.enabled()));
                            }
                            if delete.clicked() && allowed {
                                actions.effect = Some(CalendarDetailsEffect::Delete(
                                    event.clone(),
                                    start,
                                    self.delete_scope,
                                ));
                            }
                            if event.recurrence.is_some() {
                                ui.label("Delete scope");
                                let combo = egui::ComboBox::from_id_salt("calendar_delete_scope")
                                    .selected_text(self.delete_scope.label())
                                    .show_ui(ui, |ui| {
                                        ui.add_enabled_ui(allowed, |ui| {
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
                                self.host_popups.push(
                                    super::calendar_window_host::CalendarPopupAnchor::new(
                                        ui,
                                        &combo.response,
                                        self.host_session_epoch,
                                    ),
                                );
                            }
                        });

                        ui.horizontal_wrapped(|ui| {
                            ui.label("Snooze (minutes)");
                            ui.add(egui::DragValue::new(&mut self.snooze_minutes).range(1..=1440));
                            if ui.button("Snooze").clicked() && allowed {
                                actions.effect = Some(CalendarDetailsEffect::Snooze(
                                    event.id.clone(),
                                    self.snooze_minutes,
                                ));
                            }
                        });
                    });
            });
        };
        if let Some(ui) = child_ui {
            draw_body(ui);
        } else {
            egui::Window::new("Event Details")
                .resizable(true)
                .default_size((360.0, 220.0))
                .max_size(ctx.content_rect().size())
                .show_utility(
                    ctx,
                    "calendar-event-details",
                    egui::Id::new("Event Details"),
                    Some(&mut open),
                    crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(
                            egui::Vec2::from((360.0, 220.0)),
                        ),
                        egui::Vec2::ZERO,
                        [None, None],
                    )
                    .with_maximum(ctx.content_rect().size()),
                    draw_body,
                );
        }
        actions.close = owner && first && !open;
        actions
    }

    pub(crate) fn apply_host_actions(
        &mut self,
        actions: CalendarDetailsHostActions,
        app: &mut LauncherApp,
        path: &str,
    ) -> Option<CalendarDetailsDispatch> {
        if actions.epoch != self.host_session_epoch || !app.calendar_details_open {
            return None;
        }
        if actions.close {
            app.calendar_details_open = false;
            return None;
        }
        let instance = self.instance.as_ref()?;
        let effect = actions.effect?;
        let current = CALENDAR_DATA.read().ok().is_some_and(|data| {
            data.iter().any(|event| {
                event.id == instance.source_event_id
                    && calendar_occurrence_is_current(event, instance.start, Some(instance.end))
            })
        });
        if !current {
            app.report_error(
                "ui operation",
                "This calendar occurrence is no longer current.",
            );
            app.calendar_details_open = false;
            return None;
        }
        match effect {
            CalendarDetailsEffect::Edit(event, scope) => {
                app.calendar_details_open = false;
                return Some(CalendarDetailsDispatch::OpenEditor(event, scope));
            }
            CalendarDetailsEffect::Duplicate(id, start) => {
                match duplicate_event_at(path, &id, start, Some(instance.end)) {
                    Ok(()) => app
                        .dashboard_data_cache
                        .request_refresh(DashboardRefreshRequest::Calendar),
                    Err(error) => app.report_error(
                        "ui operation",
                        format!("Failed to duplicate event: {error}"),
                    ),
                }
            }
            CalendarDetailsEffect::Delete(event, start, scope) => {
                match delete_event_with_scope_at(path, &event.id, start, Some(instance.end), scope)
                {
                    Ok(()) => {
                        app.dashboard_data_cache
                            .request_refresh(DashboardRefreshRequest::Calendar);
                        app.calendar_details_open = false;
                        if app.calendar_selected_event.as_deref() == Some(event.id.as_str()) {
                            app.calendar_selected_event = None;
                        }
                    }
                    Err(error) => {
                        app.report_error("ui operation", format!("Failed to delete event: {error}"))
                    }
                }
            }
            CalendarDetailsEffect::Snooze(id, minutes) => {
                match snooze_event_at(path, &id, Duration::minutes(minutes)) {
                    Ok(_) => app
                        .dashboard_data_cache
                        .request_refresh(DashboardRefreshRequest::Calendar),
                    Err(error) => {
                        app.report_error("ui operation", format!("Failed to snooze event: {error}"))
                    }
                }
            }
        }
        None
    }
}

pub(super) fn calendar_occurrence_stamp(value: NaiveDateTime) -> NaiveDateTime {
    value.with_nanosecond(0).expect("whole seconds are valid")
}

pub(super) fn calendar_occurrence_is_current(
    event: &CalendarEvent,
    start: NaiveDateTime,
    expected_end: Option<NaiveDateTime>,
) -> bool {
    let start = calendar_occurrence_stamp(start);
    let expected_end = expected_end.map(calendar_occurrence_stamp);
    let Some(range_end) = start.checked_add_signed(Duration::seconds(1)) else {
        return false;
    };
    expand_instances(std::slice::from_ref(event), start, range_end, 64)
        .iter()
        .any(|instance| {
            calendar_occurrence_stamp(instance.start) == start
                && expected_end.is_none_or(|end| calendar_occurrence_stamp(instance.end) == end)
        })
}

fn delete_event_with_scope_at(
    path: &str,
    event_id: &str,
    occurrence_start: NaiveDateTime,
    expected_end: Option<NaiveDateTime>,
    scope: RecurrenceScope,
) -> anyhow::Result<()> {
    let event_id = event_id.to_owned();
    update_events(path, move |events| {
        let event = events
            .iter()
            .find(|event| event.id == event_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("The event was removed while Details was open."))?;
        anyhow::ensure!(
            calendar_occurrence_is_current(&event, occurrence_start, expected_end),
            "This calendar occurrence changed while Details was open."
        );
        apply_delete_scope(
            events,
            &event,
            calendar_occurrence_stamp(occurrence_start),
            scope,
        );
        Ok(true)
    })?;
    Ok(())
}

fn apply_delete_scope(
    events: &mut Vec<CalendarEvent>,
    event: &CalendarEvent,
    occurrence_start: NaiveDateTime,
    scope: RecurrenceScope,
) {
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
}

fn duplicate_event_at(
    path: &str,
    event_id: &str,
    start: NaiveDateTime,
    expected_end: Option<NaiveDateTime>,
) -> anyhow::Result<()> {
    let event_id = event_id.to_owned();
    update_events(path, move |events| {
        let source = events
            .iter()
            .find(|event| event.id == event_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("calendar event no longer exists: {event_id}"))?;
        anyhow::ensure!(
            calendar_occurrence_is_current(&source, start, expected_end),
            "This calendar occurrence changed while Details was open."
        );
        let duration = CalendarEventDetails::event_duration(&source);
        events.push(CalendarEvent {
            id: new_event_id(),
            title: source.title,
            start,
            end: source.end.is_some().then_some(start + duration),
            duration_minutes: source.duration_minutes,
            all_day: source.all_day,
            notes: source.notes,
            recurrence: None,
            reminders: source.reminders,
            tags: source.tags,
            category: source.category,
            created_at: Local::now().naive_local(),
            updated_at: None,
            entity_refs: source.entity_refs,
        });
        Ok(true)
    })?;
    Ok(())
}

fn next_occurrence(event: &CalendarEvent, after: NaiveDateTime) -> Option<NaiveDateTime> {
    let events = vec![event.clone()];
    let start = after + Duration::seconds(1);
    let end = after + Duration::days(400);
    let instances = crate::plugins::calendar::expand_instances(&events, start, end, 2);
    instances.first().map(|i| i.start)
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use crate::common::persistence::save_json_atomic;
    use crate::plugins::calendar::{calendar_test_guard, load_events, restore_calendar_test_data};
    use chrono::NaiveDate;

    fn event() -> CalendarEvent {
        CalendarEvent {
            id: "source".into(),
            title: "Source".into(),
            start: NaiveDate::from_ymd_opt(2026, 9, 6)
                .unwrap()
                .and_hms_opt(9, 0, 0)
                .unwrap(),
            end: None,
            duration_minutes: Some(30),
            all_day: false,
            notes: None,
            recurrence: Some(RecurrenceRule {
                frequency: crate::plugins::calendar::RecurrenceFrequency::Daily,
                interval: 1,
                weekly_days: Vec::new(),
                nth_weekday: None,
                end: RecurrenceEnd::Never,
                custom_unit: None,
            }),
            reminders: Vec::new(),
            tags: Vec::new(),
            category: None,
            created_at: NaiveDate::from_ymd_opt(2026, 9, 1)
                .unwrap()
                .and_hms_opt(8, 0, 0)
                .unwrap(),
            updated_at: None,
            entity_refs: Vec::new(),
        }
    }

    #[test]
    fn stale_details_occurrence_cannot_delete_or_duplicate_the_changed_series() {
        let _test = calendar_test_guard();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.json");
        let mut source = event();
        let stale = source.start + Duration::days(1);
        source.recurrence = None;
        save_json_atomic(&path, &vec![source.clone()]).unwrap();
        let before = std::fs::read(&path).unwrap();
        let path_str = path.to_str().unwrap();
        assert!(
            delete_event_with_scope_at(path_str, &source.id, stale, None, RecurrenceScope::This)
                .is_err()
        );
        assert!(duplicate_event_at(path_str, &source.id, stale, None).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn duplicate_and_delete_use_transaction_and_reject_corruption() {
        let _test = calendar_test_guard();
        let original = CALENDAR_DATA.read().unwrap().clone();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.json");
        let source = event();
        save_json_atomic(&path, &vec![source.clone()]).unwrap();
        duplicate_event_at(path.to_str().unwrap(), &source.id, source.start, None).unwrap();
        assert_eq!(load_events(path.to_str().unwrap()).unwrap().len(), 2);
        delete_event_with_scope_at(
            path.to_str().unwrap(),
            &source.id,
            source.start + Duration::days(1),
            None,
            RecurrenceScope::ThisAndFollowing,
        )
        .unwrap();
        let after_split = load_events(path.to_str().unwrap()).unwrap();
        assert_eq!(after_split.len(), 2);
        assert!(matches!(
            after_split
                .iter()
                .find(|event| event.id == source.id)
                .and_then(|event| event.recurrence.as_ref())
                .map(|rule| &rule.end),
            Some(RecurrenceEnd::OnDate { date }) if *date == source.start.date()
        ));

        let invalid = b"invalid calendar events";
        std::fs::write(&path, invalid).unwrap();
        assert!(
            duplicate_event_at(path.to_str().unwrap(), &source.id, source.start, None).is_err()
        );
        assert!(
            delete_event_with_scope_at(
                path.to_str().unwrap(),
                &source.id,
                source.start,
                None,
                RecurrenceScope::All,
            )
            .is_err()
        );
        assert_eq!(std::fs::read(path).unwrap(), invalid);
        restore_calendar_test_data(original);
    }
}
