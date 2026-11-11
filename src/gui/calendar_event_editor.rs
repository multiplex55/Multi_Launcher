use crate::dashboard::DashboardRefreshRequest;
use crate::gui::LauncherApp;
use crate::gui::calendar_event_details::RecurrenceScope;
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, secondary_help, semantic_status,
    with_button_emphasis,
};
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
#[cfg(test)]
use crate::plugins::calendar::CALENDAR_EVENTS_FILE;
use crate::plugins::calendar::{
    CalendarEvent, CustomRecurrenceUnit, RecurrenceEnd, RecurrenceFrequency, RecurrenceRule,
    Reminder, new_event_id, update_events,
};
use chrono::{Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Weekday};
use eframe::egui;
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct CalendarEditorHostActions {
    pub(super) host_request: Option<super::utility_host::UtilityPresentation>,
    pub(crate) close: bool,
    save: bool,
    epoch: u64,
    #[cfg(test)]
    pub(crate) host_button: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    pub(crate) save_button: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    pub(crate) title_field: Option<(egui::Rect, egui::Rect, bool)>,
}

#[derive(Clone)]
pub struct CalendarEventEditor {
    host_session_epoch: u64,
    host_popups: Vec<super::calendar_window_host::CalendarPopupAnchor>,
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
    source_baseline: Option<CalendarEvent>,
    split_from: Option<SplitScope>,
    #[cfg(test)]
    test_layout: Option<CalendarEditorLayout>,
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct CalendarEditorLayout {
    visible: bool,
    sizing_pass: bool,
    parent_clip_rect: egui::Rect,
    body_id: Option<egui::Id>,
    body_inner_rect: egui::Rect,
    body_clip_rect: egui::Rect,
    body_content_size: egui::Vec2,
    body_max_height: f32,
    recurrence_end_rect: egui::Rect,
    footer_button_rects: [egui::Rect; 2],
    footer_reserved_height: f32,
    window_rect: egui::Rect,
}

#[cfg(test)]
impl Default for CalendarEditorLayout {
    fn default() -> Self {
        Self {
            visible: false,
            sizing_pass: false,
            parent_clip_rect: egui::Rect::NOTHING,
            body_id: None,
            body_inner_rect: egui::Rect::NOTHING,
            body_clip_rect: egui::Rect::NOTHING,
            body_content_size: egui::Vec2::ZERO,
            body_max_height: 0.0,
            recurrence_end_rect: egui::Rect::NOTHING,
            footer_button_rects: [egui::Rect::NOTHING; 2],
            footer_reserved_height: 0.0,
            window_rect: egui::Rect::NOTHING,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecurrenceEndMode {
    Never,
    OnDate,
    AfterCount,
}

#[derive(Clone, Debug)]
pub struct SplitScope {
    event_id: String,
    occurrence_start: NaiveDateTime,
    scope: RecurrenceScope,
}

impl Default for CalendarEventEditor {
    fn default() -> Self {
        Self {
            host_session_epoch: 0,
            host_popups: Vec::new(),
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
            source_baseline: None,
            split_from: None,
            #[cfg(test)]
            test_layout: None,
        }
    }
}

impl CalendarEventEditor {
    pub fn open_new(&mut self, date: NaiveDate) {
        let now = Local::now().naive_local();
        self.reset_new(date, now.format("%H:%M").to_string());
    }

    pub(crate) fn open_new_at(&mut self, start: NaiveDateTime) -> Result<(), String> {
        if start.second() != 0 || start.nanosecond() != 0 {
            return Err("Calendar Event times must be aligned to whole minutes.".into());
        }
        self.reset_new(start.date(), start.format("%H:%M").to_string());
        Ok(())
    }

    fn reset_new(&mut self, date: NaiveDate, time: String) {
        self.host_session_epoch = self.host_session_epoch.wrapping_add(1).max(1);
        self.title.clear();
        self.start_date = date.format("%Y-%m-%d").to_string();
        self.start_time = time;
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
        self.source_baseline = None;
        self.split_from = None;
    }

    pub fn open_edit(
        &mut self,
        event: &CalendarEvent,
        occurrence_start: Option<NaiveDateTime>,
        split_scope: Option<SplitScope>,
    ) {
        self.host_session_epoch = self.host_session_epoch.wrapping_add(1).max(1);
        self.source_baseline = Some(event.clone());
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
        if let Some(split) = &self.split_from
            && matches!(split.scope, RecurrenceScope::This)
        {
            self.repeat_enabled = false;
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

    #[cfg(test)]
    fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        let actions = self.render_inner(ctx, app.calendar_editor_open, true, true, None, None);
        self.apply_host_actions(actions, app, CALENDAR_EVENTS_FILE);
    }
    pub(super) fn popup_anchors(&self) -> &[super::calendar_window_host::CalendarPopupAnchor] {
        &self.host_popups
    }

    pub(crate) fn owned_popup_layer(
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
    #[cfg(test)]
    pub(crate) fn test_set_title(&mut self, title: &str) {
        self.title = title.to_owned();
    }

    #[cfg(test)]
    pub(crate) fn test_title(&self) -> &str {
        &self.title
    }

    #[cfg(test)]
    pub(crate) fn test_start_date(&self) -> &str {
        &self.start_date
    }

    pub(crate) fn host_session_epoch(&self) -> u64 {
        self.host_session_epoch
    }
    pub(super) fn render_host(
        &mut self,
        ctx: &egui::Context,
        visible: bool,
        owner: bool,
        first: bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: super::utility_host::UtilityPresentation,
    ) -> CalendarEditorHostActions {
        self.render_inner(ctx, visible, owner, first, child_ui, Some(presentation))
    }
    fn render_inner(
        &mut self,
        ctx: &egui::Context,
        visible: bool,
        owner: bool,
        first: bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: Option<super::utility_host::UtilityPresentation>,
    ) -> CalendarEditorHostActions {
        let mut actions = CalendarEditorHostActions {
            epoch: self.host_session_epoch,
            ..Default::default()
        };
        if !visible {
            return actions;
        }
        let mut open = visible;
        let mut close_requested = false;
        #[cfg(test)]
        let mut test_layout = CalendarEditorLayout::default();
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
                #[cfg(test)]
                {
                    test_layout.visible = ui.is_visible();
                    test_layout.sizing_pass = ui.is_sizing_pass();
                    test_layout.parent_clip_rect = ui.clip_rect();
                }

                let footer_reserved_height = calendar_editor_footer_reserved_height(ui);
                let body_max_height = (ui.available_height() - footer_reserved_height).max(0.0);
                let body_max_width = ui.available_width().max(0.0);
                #[cfg(test)]
                let mut body_clip_rect = egui::Rect::NOTHING;
                let body_output = egui::ScrollArea::vertical()
                    .id_salt("calendar_event_editor_body")
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .max_height(body_max_height)
                    .show(ui, |ui| {
                        ui.set_max_width(body_max_width);

                        compact_section_heading(ui, "Event", None);
                        let title_width = ui.available_width().max(1.0);
                        let title_error = self.errors.get("title").cloned();
                        let _title = editor_text_field(
                            ui,
                            "Title",
                            &mut self.title,
                            title_width,
                            title_error.as_deref(),
                        );
                        #[cfg(test)]
                        {
                            actions.title_field = Some((_title, ui.clip_rect(), owner));
                        }

                        compact_section_heading(
                            ui,
                            "Schedule",
                            Some("Use YYYY-MM-DD dates and 24-hour HH:MM times."),
                        );
                        let column_width = editor_datetime_column_width(ui);
                        let start_date_error = self.errors.get("start_date").cloned();
                        let start_time_error = self.errors.get("start_time").cloned();
                        ui.horizontal_wrapped(|ui| {
                            editor_text_field(
                                ui,
                                "Start Date (YYYY-MM-DD)",
                                &mut self.start_date,
                                column_width,
                                start_date_error.as_deref(),
                            );
                            editor_text_field(
                                ui,
                                "Start Time (HH:MM)",
                                &mut self.start_time,
                                column_width,
                                start_time_error.as_deref(),
                            );
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut self.all_day, "All day");
                        });

                        let end_date_error = self.errors.get("end_date").cloned();
                        let end_time_error = self.errors.get("end_time").cloned();
                        ui.horizontal_wrapped(|ui| {
                            editor_text_field(
                                ui,
                                "End Date (optional)",
                                &mut self.end_date,
                                column_width,
                                end_date_error.as_deref(),
                            );
                            editor_text_field(
                                ui,
                                "End Time (optional)",
                                &mut self.end_time,
                                column_width,
                                end_time_error.as_deref(),
                            );
                        });
                        let duration_error = self.errors.get("duration").cloned();
                        let duration_width = ui.available_width().max(1.0);
                        editor_text_field(
                            ui,
                            "Duration (minutes, optional)",
                            &mut self.duration_minutes,
                            duration_width,
                            duration_error.as_deref(),
                        );

                        compact_section_heading(
                            ui,
                            "Notes and tags",
                            Some("Notes are optional. Separate multiple tags with commas."),
                        );
                        let notes_label = ui.label("Notes");
                        ui.add(
                            egui::TextEdit::multiline(&mut self.notes)
                                .desired_width(ui.available_width().max(1.0))
                                .desired_rows(4),
                        )
                        .labelled_by(notes_label.id);

                        compact_section_heading(
                            ui,
                            "Recurrence",
                            Some("Repeat this event on the selected schedule."),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut self.repeat_enabled, "Enable recurrence");
                        });
                        if self.repeat_enabled {
                            ui.horizontal_wrapped(|ui| {
                                let combo =
                                    egui::ComboBox::from_id_salt("calendar_repeat_frequency")
                                        .selected_text(format!("{:?}", self.repeat_frequency))
                                        .show_ui(ui, |ui| {
                                            ui.add_enabled_ui(allowed, |ui| {
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
                                        });
                                self.host_popups.push(
                                    super::calendar_window_host::CalendarPopupAnchor::new(
                                        ui,
                                        &combo.response,
                                        self.host_session_epoch,
                                    ),
                                );
                                ui.label("Every");
                                ui.add(
                                    egui::DragValue::new(&mut self.repeat_interval).range(1..=365),
                                );
                            });
                            if self.repeat_frequency == RecurrenceFrequency::Custom {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Repeat unit");
                                    let combo =
                                        egui::ComboBox::from_id_salt("calendar_repeat_custom")
                                            .selected_text(format!("{:?}", self.repeat_custom_unit))
                                            .show_ui(ui, |ui| {
                                                ui.add_enabled_ui(allowed, |ui| {
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
                                        if ui
                                            .selectable_label(selected, weekday.to_string())
                                            .clicked()
                                        {
                                            if selected {
                                                self.weekly_days.retain(|d| *d != weekday);
                                            } else {
                                                self.weekly_days.push(weekday);
                                            }
                                        }
                                    }
                                });
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Ends");
                                let combo = egui::ComboBox::from_id_salt("calendar_repeat_end")
                                    .selected_text(match self.repeat_end_mode {
                                        RecurrenceEndMode::Never => "Never",
                                        RecurrenceEndMode::OnDate => "On date",
                                        RecurrenceEndMode::AfterCount => "After count",
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.add_enabled_ui(allowed, |ui| {
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
                                self.host_popups.push(
                                    super::calendar_window_host::CalendarPopupAnchor::new(
                                        ui,
                                        &combo.response,
                                        self.host_session_epoch,
                                    ),
                                );
                            });
                            match self.repeat_end_mode {
                                RecurrenceEndMode::OnDate => {
                                    let error = self.errors.get("repeat_end_date").cloned();
                                    let width = ui.available_width().max(1.0);
                                    let field_rect = editor_text_field(
                                        ui,
                                        "Repeat ends on (YYYY-MM-DD)",
                                        &mut self.repeat_end_date,
                                        width,
                                        error.as_deref(),
                                    );
                                    #[cfg(test)]
                                    {
                                        test_layout.recurrence_end_rect = field_rect;
                                    }
                                    #[cfg(not(test))]
                                    let _ = field_rect;
                                }
                                RecurrenceEndMode::AfterCount => {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label("After");
                                        ui.add(
                                            egui::DragValue::new(&mut self.repeat_end_count)
                                                .range(1..=999),
                                        );
                                        ui.label("occurrences");
                                    });
                                }
                                RecurrenceEndMode::Never => {}
                            }
                        }

                        compact_section_heading(ui, "Reminders", None);
                        secondary_help(ui, "Enter minutes before the event, separated by commas.");
                        let reminders_error = self.errors.get("reminders").cloned();
                        let reminders_width = ui.available_width().max(1.0);
                        editor_text_field(
                            ui,
                            "Reminders (minutes before, comma separated)",
                            &mut self.reminders,
                            reminders_width,
                            reminders_error.as_deref(),
                        );
                        let tags_width = ui.available_width().max(1.0);
                        editor_text_field(
                            ui,
                            "Tags (comma separated)",
                            &mut self.tags,
                            tags_width,
                            None,
                        );

                        #[cfg(test)]
                        {
                            body_clip_rect = ui.clip_rect();
                        }
                    });

                ui.separator();
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
                        actions.save_button = Some((save.rect, ui.clip_rect(), save.enabled()));
                    }
                    if save.clicked() && allowed {
                        actions.save = true;
                    }
                    let cancel = ui.button("Cancel");
                    #[cfg(test)]
                    {
                        cancel_rect = cancel.rect;
                    }
                    if cancel.clicked() && allowed {
                        close_requested = true;
                    }
                });

                #[cfg(test)]
                {
                    test_layout.body_inner_rect = body_output.inner_rect;
                    test_layout.body_id = Some(body_output.id);
                    test_layout.body_clip_rect = body_clip_rect;
                    test_layout.body_content_size = body_output.content_size;
                    test_layout.body_max_height = body_max_height;
                    test_layout.footer_button_rects = [save_rect, cancel_rect];
                    test_layout.footer_reserved_height = footer_reserved_height;
                }
                #[cfg(not(test))]
                let _ = (body_output, footer_reserved_height);
            });
        };
        let window_output = if let Some(ui) = child_ui {
            draw_body(ui);
            None
        } else {
            egui::Window::new("Calendar Event")
                .resizable(true)
                .default_size((420.0, 400.0))
                .max_size(ctx.content_rect().size())
                .show_utility(
                    ctx,
                    "calendar-event-editor",
                    egui::Id::new("Calendar Event"),
                    Some(&mut open),
                    crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(
                            egui::Vec2::from((420.0, 400.0)),
                        ),
                        egui::Vec2::ZERO,
                        [None, None],
                    )
                    .with_maximum(ctx.content_rect().size()),
                    draw_body,
                )
        };

        #[cfg(test)]
        {
            if let Some(window_output) = window_output {
                test_layout.window_rect = window_output.response.rect;
                self.test_layout = Some(test_layout);
            }
        }
        #[cfg(not(test))]
        let _ = window_output;
        actions.close = owner && first && (!open || close_requested);
        actions
    }

    pub(crate) fn apply_host_actions(
        &mut self,
        actions: CalendarEditorHostActions,
        app: &mut LauncherApp,
        path: &str,
    ) {
        if actions.epoch != self.host_session_epoch || !app.calendar_editor_open {
            return;
        }
        if actions.close {
            app.calendar_editor_open = false;
            return;
        }
        if actions.save {
            match self.save_at(app, path) {
                Ok(()) => app.calendar_editor_open = false,
                Err(error) => app.report_error("ui operation", error),
            }
        }
    }

    #[cfg(test)]
    fn save(&mut self, app: &mut LauncherApp) -> Result<(), String> {
        self.save_at(app, CALENDAR_EVENTS_FILE)
    }
    fn save_at(&mut self, app: &mut LauncherApp, path: &str) -> Result<(), String> {
        self.errors.clear();
        let title = self.title.trim();
        if title.is_empty() {
            self.errors.insert("title", "Title is required".into());
        }
        let start_date = parse_date(&self.start_date).inspect_err(|err| {
            self.errors.insert("start_date", err.clone());
        })?;
        let start_time = if self.all_day {
            NaiveTime::from_hms_opt(0, 0, 0).unwrap()
        } else {
            parse_time(&self.start_time).inspect_err(|err| {
                self.errors.insert("start_time", err.clone());
            })?
        };
        let start = NaiveDateTime::new(start_date, start_time);

        let end_date = if self.end_date.trim().is_empty() {
            None
        } else {
            Some(parse_date(&self.end_date).inspect_err(|err| {
                self.errors.insert("end_date", err.clone());
            })?)
        };
        let end_time = if self.end_time.trim().is_empty() {
            None
        } else {
            Some(parse_time(&self.end_time).inspect_err(|err| {
                self.errors.insert("end_time", err.clone());
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

        if let Some(end) = end
            && end < start
        {
            self.errors
                .insert("end_date", "End must be after start".into());
        }

        let reminders = parse_reminders(&self.reminders).inspect_err(|err| {
            self.errors.insert("reminders", err.clone());
        })?;

        let recurrence = if self.repeat_enabled {
            let end = match self.repeat_end_mode {
                RecurrenceEndMode::Never => RecurrenceEnd::Never,
                RecurrenceEndMode::OnDate => {
                    let date = parse_date(&self.repeat_end_date).inspect_err(|err| {
                        self.errors.insert("repeat_end_date", err.clone());
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

        let now = Local::now().naive_local();
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
            created_at: now,
            updated_at: Some(now),
            entity_refs: Vec::new(),
        };

        let commit = match (&self.source_baseline, &self.split_from) {
            (Some(source), Some(scope)) => EditorCommit::Split(source, scope),
            (Some(source), None) => EditorCommit::Update(source),
            (None, _) => EditorCommit::Create,
        };
        persist_editor_event(path, event, commit).map_err(|e| e.to_string())?;
        self.split_from = None;
        app.dashboard_data_cache
            .request_refresh(DashboardRefreshRequest::Calendar);
        app.calendar_selected_event = Some(id);
        Ok(())
    }
}

fn editor_text_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    width: f32,
    error: Option<&str>,
) -> egui::Rect {
    ui.vertical(|ui| {
        ui.set_max_width(width.max(1.0));
        let label_response = ui.add(egui::Label::new(label).wrap());
        let input = ui
            .add(egui::TextEdit::singleline(value).desired_width(width.max(1.0)))
            .labelled_by(label_response.id);
        if let Some(error) = error {
            semantic_status(
                ui,
                StatusTone::Error,
                &format!("Invalid {label}"),
                Some(error),
            );
        }
        input.rect
    })
    .inner
}

fn editor_datetime_column_width(ui: &egui::Ui) -> f32 {
    ((ui.available_width() - ui.spacing().item_spacing.x).max(0.0) * 0.5)
        .min(220.0)
        .max(72.0)
}

fn calendar_editor_footer_reserved_height(ui: &mut egui::Ui) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let native_button_style = style.button_style(
        &egui::widget_style::Classes::default(),
        egui::widget_style::WidgetState::Inactive,
    );
    let button_font = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| native_button_style.text_style.font_id.clone());
    let button_frame = native_button_style.frame;
    let button_height = (ui.fonts_mut(|fonts| fonts.row_height(&button_font))
        + spacing.extra_text_line_spacing
        + button_frame.total_margin().sum().y)
        .max(spacing.interact_size.y);
    let available_width = ui.available_width().max(1.0);
    let text_color = ui.visuals().text_color();
    let mut row_width = 0.0;
    let mut row_count = 1_usize;
    for label in ["Save", "Cancel"] {
        let text_width = ui.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap(label.to_owned(), button_font.clone(), text_color)
                .size()
                .x
        });
        let width = (text_width + button_frame.total_margin().sum().x).max(spacing.interact_size.x);
        if row_width > 0.0 && row_width + spacing.item_spacing.x + width > available_width {
            row_count += 1;
            row_width = width;
        } else {
            if row_width > 0.0 {
                row_width += spacing.item_spacing.x;
            }
            row_width += width;
        }
    }
    let separator_spacing = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let height = row_count as f32 * button_height
        + row_count.saturating_sub(1) as f32 * spacing.item_spacing.y
        + 2.0 * spacing.item_spacing.y
        + separator_spacing;
    let pixels_per_point = ui.pixels_per_point().max(f32::EPSILON);
    (height * pixels_per_point).ceil() / pixels_per_point
}
enum EditorCommit<'a> {
    Create,
    Update(&'a CalendarEvent),
    Split(&'a CalendarEvent, &'a SplitScope),
}

fn require_unchanged_source(
    current: &CalendarEvent,
    expected: &CalendarEvent,
) -> anyhow::Result<()> {
    use super::calendar_event_details::calendar_occurrence_stamp as stamp;
    anyhow::ensure!(
        current.id == expected.id
            && current.title == expected.title
            && stamp(current.start) == stamp(expected.start)
            && current.end.map(stamp) == expected.end.map(stamp)
            && current.duration_minutes == expected.duration_minutes
            && current.all_day == expected.all_day
            && current.notes == expected.notes
            && current.recurrence == expected.recurrence
            && current.reminders == expected.reminders
            && current.tags == expected.tags
            && current.category == expected.category,
        "The event changed while this editor was open."
    );
    Ok(())
}

fn persist_editor_event(
    path: &str,
    mut event: CalendarEvent,
    commit: EditorCommit<'_>,
) -> anyhow::Result<()> {
    update_events(path, move |events| {
        match commit {
            EditorCommit::Create => {
                anyhow::ensure!(
                    !events.iter().any(|existing| existing.id == event.id),
                    "An event already uses this draft's identifier."
                );
                events.push(event);
            }
            EditorCommit::Update(source) => {
                let existing = events
                    .iter_mut()
                    .find(|existing| existing.id == event.id)
                    .ok_or_else(|| {
                        anyhow::anyhow!("The event was removed while this editor was open.")
                    })?;
                require_unchanged_source(existing, source)?;
                event.created_at = existing.created_at;
                event.entity_refs = existing.entity_refs.clone();
                event.category = existing.category.clone();
                *existing = event;
            }
            EditorCommit::Split(source, scope) => {
                apply_split_scope(events, source, scope)?;
                events.push(event);
            }
        }
        Ok(true)
    })?;
    Ok(())
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

fn apply_split_scope(
    events: &mut Vec<CalendarEvent>,
    source: &CalendarEvent,
    scope: &SplitScope,
) -> anyhow::Result<()> {
    let event = events
        .iter()
        .find(|e| e.id == scope.event_id)
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!("The recurring event was removed while this editor was open.")
        })?;
    require_unchanged_source(&event, source)?;
    anyhow::ensure!(
        super::calendar_event_details::calendar_occurrence_is_current(
            &event,
            scope.occurrence_start,
            None
        ),
        "The recurring occurrence changed while this editor was open."
    );
    let occurrence_start =
        super::calendar_event_details::calendar_occurrence_stamp(scope.occurrence_start);
    match (event.recurrence.clone(), scope.scope) {
        (Some(rule), RecurrenceScope::ThisAndFollowing) => {
            if occurrence_start <= event.start {
                events.retain(|e| e.id != event.id);
            } else {
                let end_date = occurrence_start.date() - Duration::days(1);
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
                if let Some(next) = next_occurrence(&event, occurrence_start) {
                    let mut updated = event.clone();
                    updated.start = next;
                    events.retain(|e| e.id != event.id);
                    events.push(updated);
                } else {
                    events.retain(|e| e.id != event.id);
                }
            } else {
                let end_date = occurrence_start.date() - Duration::days(1);
                let mut updated = event.clone();
                updated.recurrence = Some(RecurrenceRule {
                    end: RecurrenceEnd::OnDate { date: end_date },
                    ..rule
                });
                events.retain(|e| e.id != event.id);
                events.push(updated);
                if let Some(next) = next_occurrence(&event, occurrence_start) {
                    let mut next_event = event.clone();
                    next_event.id = new_event_id();
                    next_event.start = next;
                    next_event.updated_at = Some(Local::now().naive_local());
                    events.push(next_event);
                }
            }
        }
        _ => anyhow::bail!("This event no longer supports editing an individual occurrence."),
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

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use crate::common::entity_ref::{EntityKind, EntityRef};
    use crate::common::persistence::save_json_atomic;
    use crate::plugins::calendar::{
        CALENDAR_DATA, calendar_test_guard, load_events, restore_calendar_test_data,
    };
    use std::sync::{Arc, atomic::AtomicBool};

    #[test]
    fn date_time_studio_calendar_prefill_requires_minute_alignment_and_preserves_exact_minute() {
        let mut editor = CalendarEventEditor::default();
        let start = NaiveDate::from_ymd_opt(2026, 11, 2)
            .unwrap()
            .and_hms_opt(14, 35, 0)
            .unwrap();
        editor.open_new_at(start).unwrap();
        assert_eq!(editor.start_date, "2026-11-02");
        assert_eq!(editor.start_time, "14:35");

        editor.title = "Keep this draft".into();
        let subminute = start + Duration::milliseconds(1);
        assert!(editor.open_new_at(subminute).is_err());
        assert_eq!(editor.title, "Keep this draft");
        assert_eq!(editor.start_time, "14:35");
    }

    #[test]
    fn date_time_studio_prefill_is_machine_local_unsaved_and_refuses_open_editor_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, directory.path());
        let instant = NaiveDate::from_ymd_opt(2026, 11, 2)
            .unwrap()
            .and_hms_opt(14, 35, 0)
            .unwrap()
            .and_utc();
        let expected_local = instant.with_timezone(&Local).naive_local();
        let result = app.prepare_calendar_event_at(instant).unwrap();
        assert!(result.contains("this computer's local time"));
        assert!(app.calendar_editor_open);
        assert_eq!(
            app.calendar_event_editor.start_date,
            expected_local.format("%Y-%m-%d").to_string()
        );
        assert_eq!(
            app.calendar_event_editor.start_time,
            expected_local.format("%H:%M").to_string()
        );
        assert!(app.calendar_event_editor.title.is_empty());

        app.calendar_event_editor.title = "Existing unsaved draft".into();
        let second = instant + Duration::hours(1);
        assert!(app.prepare_calendar_event_at(second).is_err());
        assert_eq!(app.calendar_event_editor.title, "Existing unsaved draft");
        assert_eq!(
            app.calendar_event_editor.start_date,
            expected_local.format("%Y-%m-%d").to_string()
        );
        assert_eq!(
            app.calendar_event_editor.start_time,
            expected_local.format("%H:%M").to_string()
        );
    }

    fn event(id: &str, title: &str) -> CalendarEvent {
        CalendarEvent {
            id: id.into(),
            title: title.into(),
            start: NaiveDate::from_ymd_opt(2026, 9, 6)
                .unwrap()
                .and_hms_opt(9, 0, 0)
                .unwrap(),
            end: None,
            duration_minutes: Some(30),
            all_day: false,
            notes: None,
            recurrence: None,
            reminders: Vec::new(),
            tags: Vec::new(),
            category: None,
            created_at: NaiveDate::from_ymd_opt(2020, 1, 1)
                .unwrap()
                .and_hms_opt(8, 0, 0)
                .unwrap(),
            updated_at: None,
            entity_refs: vec![EntityRef::new(EntityKind::Note, "source", None)],
        }
    }

    fn test_app(ctx: &egui::Context, root: &std::path::Path) -> LauncherApp {
        let mut settings = crate::settings::Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        settings.enable_toasts = false;
        settings.show_inline_errors = false;
        settings.show_error_toasts = false;
        settings.dashboard.enabled = false;
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            settings,
            None,
            None,
            Some(std::collections::HashSet::new()),
            Some(std::collections::HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn render_editor(
        ctx: &egui::Context,
        editor: &mut CalendarEventEditor,
        app: &mut LauncherApp,
        width: f32,
        height: f32,
        time: f64,
    ) -> Option<CalendarEditorLayout> {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, height),
                )),
                time: Some(time),
                ..Default::default()
            },
            |ui| editor.ui(ui.ctx(), app),
        );
        let layout = editor.test_layout.clone();
        output.drop_without_applying_deltas();
        layout
    }

    #[test]
    fn editor_commit_uses_latest_valid_event_and_rejects_corruption() {
        let _test = calendar_test_guard();
        let original = CALENDAR_DATA.read().unwrap().clone();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.json");
        let stored = event("existing", "Stored");
        save_json_atomic(&path, &vec![stored.clone()]).unwrap();
        persist_editor_event(
            path.to_str().unwrap(),
            event("existing", "Edited"),
            EditorCommit::Update(&stored),
        )
        .unwrap();
        let loaded = load_events(path.to_str().unwrap()).unwrap();
        assert_eq!(loaded[0].title, "Edited");
        assert_eq!(loaded[0].created_at, stored.created_at);
        assert_eq!(loaded[0].entity_refs, stored.entity_refs);

        let invalid = b"invalid calendar events";
        std::fs::write(&path, invalid).unwrap();
        assert!(
            persist_editor_event(
                path.to_str().unwrap(),
                event("existing", "Lost"),
                EditorCommit::Update(&stored)
            )
            .is_err()
        );
        assert!(
            persist_editor_event(
                path.to_str().unwrap(),
                event("split", "Lost split"),
                EditorCommit::Split(
                    &stored,
                    &SplitScope {
                        event_id: "existing".into(),
                        occurrence_start: stored.start,
                        scope: RecurrenceScope::ThisAndFollowing,
                    }
                ),
            )
            .is_err()
        );
        assert_eq!(std::fs::read(path).unwrap(), invalid);
        restore_calendar_test_data(original);
    }

    #[test]
    fn retained_editor_update_and_split_cannot_restore_a_deleted_event() {
        let _test = calendar_test_guard();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.json");
        let path_str = path.to_str().unwrap();
        save_json_atomic(&path, &Vec::<CalendarEvent>::new()).unwrap();
        let before = std::fs::read(&path).unwrap();
        let draft = event("deleted", "Unsaved editor");
        let error = persist_editor_event(path_str, draft.clone(), EditorCommit::Update(&draft))
            .unwrap_err();
        assert!(error.to_string().contains("removed"));
        let error = persist_editor_event(
            path_str,
            event("split", "Unsaved occurrence"),
            EditorCommit::Split(
                &draft,
                &SplitScope {
                    event_id: draft.id.clone(),
                    occurrence_start: draft.start,
                    scope: RecurrenceScope::This,
                },
            ),
        )
        .unwrap_err();
        assert!(error.to_string().contains("removed"));
        assert_eq!(std::fs::read(&path).unwrap(), before);

        let mut changed = draft.clone();
        changed.start += Duration::days(1);
        save_json_atomic(&path, &vec![changed]).unwrap();
        let before = std::fs::read(&path).unwrap();
        let error = persist_editor_event(path_str, draft.clone(), EditorCommit::Update(&draft))
            .unwrap_err();
        assert!(error.to_string().contains("changed"));
        let error = persist_editor_event(
            path_str,
            event("split", "Unsaved occurrence"),
            EditorCommit::Split(
                &draft,
                &SplitScope {
                    event_id: draft.id.clone(),
                    occurrence_start: draft.start,
                    scope: RecurrenceScope::This,
                },
            ),
        )
        .unwrap_err();
        assert!(error.to_string().contains("changed"));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn expanded_recurrence_validation_layout_is_bounded_and_render_does_not_commit() {
        let _test = calendar_test_guard();
        let original = CALENDAR_DATA.read().unwrap().clone();
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, directory.path());
        let mut editor = CalendarEventEditor::default();
        editor.open_new(NaiveDate::from_ymd_opt(2026, 9, 6).expect("valid test date"));
        editor.title = "Expanded recurrence".into();
        editor.start_date = "2026-09-06".into();
        editor.start_time = "09:30".into();
        editor.repeat_enabled = true;
        editor.repeat_frequency = RecurrenceFrequency::Weekly;
        editor.repeat_interval = 2;
        editor.weekly_days = vec![Weekday::Mon, Weekday::Wed, Weekday::Fri];
        editor.repeat_end_mode = RecurrenceEndMode::OnDate;
        editor.repeat_end_date = "not-a-date".into();
        assert_eq!(editor.save(&mut app), Err("Use YYYY-MM-DD".into()));
        assert_eq!(
            editor.errors.get("repeat_end_date").map(String::as_str),
            Some("Use YYYY-MM-DD")
        );

        let before = (
            editor.title.clone(),
            editor.start_date.clone(),
            editor.start_time.clone(),
            editor.repeat_enabled,
            editor.repeat_frequency.clone(),
            editor.repeat_interval,
            editor.weekly_days.clone(),
            editor.repeat_end_mode,
            editor.repeat_end_date.clone(),
            editor.errors.clone(),
            editor.current_event_id.clone(),
            CALENDAR_DATA.read().unwrap().clone(),
        );
        app.calendar_editor_open = true;
        let initial_layout = render_editor(&ctx, &mut editor, &mut app, 360.0, 480.0, 1.0)
            .expect("the visible editor window should report layout");
        let body_id = initial_layout
            .body_id
            .expect("the editor body should have stable scroll identity");
        let mut body_state = egui::scroll_area::State::load(&ctx, body_id)
            .expect("the rendered editor should register native scroll state");
        body_state.offset.y =
            (initial_layout.body_content_size.y - initial_layout.body_clip_rect.height()).max(0.0);
        body_state.store(&ctx, body_id);
        let mut settled_layout = None;
        for frame in 0..3 {
            settled_layout = render_editor(
                &ctx,
                &mut editor,
                &mut app,
                360.0,
                480.0,
                1.1 + f64::from(frame) * 0.1,
            );
        }
        let layout = settled_layout.expect("the editor should report its settled layout");

        assert!(layout.visible);
        assert!(!layout.sizing_pass);
        assert!(layout.window_rect.is_positive());
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(360.0, 480.0));
        assert!(screen_rect.contains_rect(layout.window_rect));
        assert!(layout.parent_clip_rect.is_positive());
        assert!(layout.body_inner_rect.is_positive());
        assert!(layout.body_clip_rect.is_positive());
        assert!(layout.parent_clip_rect.contains_rect(layout.body_clip_rect));
        assert!(layout.body_content_size.y > layout.body_clip_rect.height());
        assert!(layout.body_max_height >= layout.body_clip_rect.height());
        assert!(
            layout
                .body_clip_rect
                .contains_rect(layout.recurrence_end_rect)
        );
        assert!(layout.footer_reserved_height > 0.0);
        let [save_rect, cancel_rect] = layout.footer_button_rects;
        assert!(save_rect.is_positive());
        assert!(cancel_rect.is_positive());
        assert!(layout.parent_clip_rect.contains_rect(save_rect));
        assert!(layout.parent_clip_rect.contains_rect(cancel_rect));
        assert!(layout.body_clip_rect.max.y <= save_rect.min.y);

        assert_eq!(editor.title, before.0);
        assert_eq!(editor.start_date, before.1);
        assert_eq!(editor.start_time, before.2);
        assert_eq!(editor.repeat_enabled, before.3);
        assert_eq!(editor.repeat_frequency, before.4);
        assert_eq!(editor.repeat_interval, before.5);
        assert_eq!(editor.weekly_days, before.6);
        assert_eq!(editor.repeat_end_mode, before.7);
        assert_eq!(editor.repeat_end_date, before.8);
        assert_eq!(editor.errors, before.9);
        assert_eq!(editor.current_event_id, before.10);
        assert!(editor.split_from.is_none());
        assert_eq!(*CALENDAR_DATA.read().unwrap(), before.11);
        assert!(app.calendar_editor_open);
        restore_calendar_test_data(original);
    }
}
