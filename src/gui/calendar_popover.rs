use crate::gui::LauncherApp;
use crate::gui::theme::resolved_for_current_context;
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::plugins::calendar::{
    CALENDAR_DATA, CalendarEvent, CalendarSnapshot, EventInstance, add_months, expand_instances,
};
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use eframe::egui;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalendarViewMode {
    Day,
    Week,
}

#[derive(Clone)]
pub struct CalendarPopover {
    host_session_epoch: u64,
    host_popups: Vec<super::calendar_window_host::CalendarPopupAnchor>,
    view_month: NaiveDate,
    view_mode: CalendarViewMode,
}

impl Default for CalendarPopover {
    fn default() -> Self {
        let today = Local::now().naive_local().date();
        Self {
            host_session_epoch: 0,
            host_popups: Vec::new(),
            view_month: NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today),
            view_mode: CalendarViewMode::Day,
        }
    }
}

#[derive(Default)]
pub(crate) struct CalendarPopoverHostActions {
    pub(super) host_request: Option<super::utility_host::UtilityPresentation>,
    pub(crate) close: bool,
    pub(crate) dispatch: Option<CalendarPopoverDispatch>,
    pub(crate) selected_date: Option<NaiveDate>,
    pub(crate) selected_event: Option<String>,
    selection_changed: bool,
    epoch: u64,
    #[cfg(test)]
    pub(crate) host_button: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    pub(crate) agenda_button: Option<(egui::Rect, egui::Rect, bool)>,
}
pub(crate) enum CalendarPopoverDispatch {
    OpenEditor(Option<CalendarEvent>),
    OpenDetails(EventInstance),
}
pub(crate) struct CalendarPopoverRuntime {
    pub(crate) selected_date: Option<NaiveDate>,
    pub(crate) selected_event: Option<String>,
    pub(crate) calendar: std::sync::Arc<CalendarSnapshot>,
}
struct CalendarViewState {
    calendar_selected_date: Option<NaiveDate>,
    calendar_selected_event: Option<String>,
    dispatch: Option<CalendarPopoverDispatch>,
    #[cfg(test)]
    agenda_button: Option<(egui::Rect, egui::Rect, bool)>,
}
impl CalendarPopover {
    pub(crate) fn begin_host_session(&mut self) {
        self.host_session_epoch = self.host_session_epoch.wrapping_add(1).max(1);
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
    pub(crate) fn host_session_epoch(&self) -> u64 {
        self.host_session_epoch
    }
    pub(crate) fn runtime(app: &LauncherApp) -> CalendarPopoverRuntime {
        CalendarPopoverRuntime {
            selected_date: app.calendar_selected_date,
            selected_event: app.calendar_selected_event.clone(),
            calendar: std::sync::Arc::clone(&app.dashboard_data_cache.snapshot().calendar),
        }
    }
    pub(super) fn render_host(
        &mut self,
        ctx: &egui::Context,
        visible: bool,
        owner: bool,
        first: bool,
        runtime: &CalendarPopoverRuntime,
        child_ui: Option<&mut egui::Ui>,
        presentation: super::utility_host::UtilityPresentation,
    ) -> CalendarPopoverHostActions {
        self.render_inner(
            ctx,
            visible,
            owner,
            first,
            runtime,
            child_ui,
            Some(presentation),
        )
    }
    fn render_inner(
        &mut self,
        ctx: &egui::Context,
        visible: bool,
        owner: bool,
        first: bool,
        runtime: &CalendarPopoverRuntime,
        child_ui: Option<&mut egui::Ui>,
        presentation: Option<super::utility_host::UtilityPresentation>,
    ) -> CalendarPopoverHostActions {
        let mut actions = CalendarPopoverHostActions {
            epoch: self.host_session_epoch,
            ..Default::default()
        };
        if !visible {
            return actions;
        }
        let today = Local::now().naive_local().date();
        let mut state = CalendarViewState {
            calendar_selected_date: Some(runtime.selected_date.unwrap_or(today)),
            calendar_selected_event: runtime.selected_event.clone(),
            dispatch: None,
            #[cfg(test)]
            agenda_button: None,
        };
        let selected_date = state.calendar_selected_date.unwrap_or(today);
        let selected_month =
            NaiveDate::from_ymd_opt(selected_date.year(), selected_date.month(), 1)
                .unwrap_or(selected_date);
        if self.view_month != selected_month {
            self.view_month = selected_month;
        }
        if owner && first && !egui::Popup::is_any_open(ctx) && !ctx.will_discard() {
            handle_keyboard(ctx, &mut state, &mut self.view_month);
        }
        let markers = build_marker_set(&runtime.calendar, self.view_month);
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
            ui.add_enabled_ui(owner && first, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("<").clicked() {
                        shift_month(&mut state, -1, &mut self.view_month);
                    }
                    if ui.button(">").clicked() {
                        shift_month(&mut state, 1, &mut self.view_month);
                    }
                    if ui
                        .button(self.view_month.format("%B %Y").to_string())
                        .clicked()
                    {
                        state.calendar_selected_date = Some(today);
                        self.view_month = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
                            .unwrap_or(today);
                    }
                    ui.separator();
                    ui.label("View");
                    let combo = egui::ComboBox::from_id_salt("calendar_view_mode")
                        .selected_text(match self.view_mode {
                            CalendarViewMode::Day => "Day",
                            CalendarViewMode::Week => "Week",
                        })
                        .show_ui(ui, |ui| {
                            ui.add_enabled_ui(allowed, |ui| {
                                ui.selectable_value(
                                    &mut self.view_mode,
                                    CalendarViewMode::Day,
                                    "Day",
                                );
                                ui.selectable_value(
                                    &mut self.view_mode,
                                    CalendarViewMode::Week,
                                    "Week",
                                );
                            });
                        });
                    self.host_popups
                        .push(super::calendar_window_host::CalendarPopupAnchor::new(
                            ui,
                            &combo.response,
                            self.host_session_epoch,
                        ));
                });
                ui.separator();
                // Leave room for the month grid, agenda controls and their spacing.
                let narrow = ui.available_width() < 460.0;
                if narrow {
                    egui::ScrollArea::vertical()
                        .id_salt("calendar_stacked_content")
                        .auto_shrink([false, false])
                        .max_height(ui.available_height().max(0.0))
                        .show(ui, |ui| {
                            month_grid(ui, &mut state, self.view_month, &markers, today);
                            ui.separator();
                            agenda_view(ui, &mut state, self.view_mode, selected_date);
                        });
                } else {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            month_grid(ui, &mut state, self.view_month, &markers, today);
                        });
                        ui.separator();
                        ui.vertical(|ui| {
                            agenda_view(ui, &mut state, self.view_mode, selected_date);
                        });
                    });
                }
            });
        };
        if let Some(ui) = child_ui {
            draw_body(ui);
        } else {
            egui::Window::new("Calendar")
                .resizable(true)
                .default_size((480.0, 320.0))
                .max_size(ctx.content_rect().size())
                .show_utility(
                    ctx,
                    "calendar",
                    egui::Id::new("Calendar"),
                    Some(&mut open),
                    crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(
                            egui::Vec2::from((480.0, 320.0)),
                        ),
                        egui::Vec2::ZERO,
                        [None, None],
                    )
                    .with_maximum(ctx.content_rect().size()),
                    |ui| draw_body(ui),
                );
        }
        if owner && first && !ctx.will_discard() {
            actions.close = !open;
            actions.selection_changed = state.calendar_selected_date != runtime.selected_date
                || state.calendar_selected_event != runtime.selected_event;
            actions.selected_date = state.calendar_selected_date;
            actions.selected_event = state.calendar_selected_event.clone();
            actions.dispatch = state.dispatch.take();
        }
        #[cfg(test)]
        {
            actions.agenda_button = state.agenda_button;
        }
        actions
    }
    pub(crate) fn apply_host_actions(
        &mut self,
        actions: CalendarPopoverHostActions,
        app: &mut LauncherApp,
    ) -> Option<CalendarPopoverDispatch> {
        if !app.calendar_popover_open || actions.epoch != self.host_session_epoch {
            return None;
        }
        if actions.close {
            app.calendar_popover_open = false;
            return None;
        }
        if actions.selection_changed {
            app.calendar_selected_date = actions.selected_date;
            app.calendar_selected_event = actions.selected_event;
        }
        actions.dispatch
    }
}

fn handle_keyboard(ctx: &egui::Context, app: &mut CalendarViewState, view_month: &mut NaiveDate) {
    let Some(selected_date) = app.calendar_selected_date else {
        return;
    };
    let mut updated = None;
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft)) {
        updated = Some(selected_date - Duration::days(1));
    } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight)) {
        updated = Some(selected_date + Duration::days(1));
    } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
        updated = Some(selected_date - Duration::days(7));
    } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
        updated = Some(selected_date + Duration::days(7));
    } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp)) {
        let shifted = shift_date_month(selected_date, -1);
        updated = Some(shifted);
    } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown)) {
        let shifted = shift_date_month(selected_date, 1);
        updated = Some(shifted);
    } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
        if let Some(event_id) = app.calendar_selected_event.clone() {
            if let Some(event) = CALENDAR_DATA
                .read()
                .map(|d| d.clone())
                .unwrap_or_default()
                .into_iter()
                .find(|e| e.id == event_id)
            {
                app.dispatch = Some(CalendarPopoverDispatch::OpenEditor(Some(event)));
            }
        } else {
            app.dispatch = Some(CalendarPopoverDispatch::OpenEditor(None));
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

fn shift_month(app: &mut CalendarViewState, delta: i32, view_month: &mut NaiveDate) {
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
    app: &mut CalendarViewState,
    month: NaiveDate,
    markers: &HashSet<NaiveDate>,
    today: NaiveDate,
) {
    let first_weekday = month.weekday().num_days_from_sunday() as i32;
    let days = days_in_month(month);
    egui::Grid::new("calendar_month_grid")
        .num_columns(7)
        .spacing([4.0, 4.0])
        .show(ui, |ui| {
            for weekday in [
                Weekday::Sun,
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
                Weekday::Sat,
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
                if date == today && Some(date) != app.calendar_selected_date {
                    let accent = resolved_for_current_context(ui.ctx())
                        .map(|theme| theme.palette.accent)
                        .map(|color| {
                            egui::Color32::from_rgba_unmultiplied(
                                color.r, color.g, color.b, color.a,
                            )
                        })
                        .unwrap_or(ui.visuals().selection.bg_fill);
                    text = text.color(accent);
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

fn agenda_view(
    ui: &mut egui::Ui,
    app: &mut CalendarViewState,
    mode: CalendarViewMode,
    date: NaiveDate,
) {
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
            app.dispatch = Some(CalendarPopoverDispatch::OpenEditor(None));
        }
    });
    let events = CALENDAR_DATA.read().map(|d| d.clone()).unwrap_or_default();
    let title_map = events
        .iter()
        .map(|event| (event.id.clone(), event.title.clone()))
        .collect::<HashMap<_, _>>();
    let instances = expand_instances(&events, start, end, 64);
    let agenda_max_height = ui.available_height().min(220.0).max(0.0);
    egui::ScrollArea::vertical()
        .id_salt("calendar_agenda")
        .auto_shrink([false, false])
        .min_scrolled_height(0.0)
        .max_height(agenda_max_height)
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
                #[cfg(test)]
                {
                    app.agenda_button = Some((resp.rect, ui.clip_rect(), resp.enabled()));
                }
                if resp.clicked() {
                    app.calendar_selected_event = Some(instance.source_event_id.clone());
                    app.calendar_selected_date = Some(instance.start.date());
                    app.dispatch = Some(CalendarPopoverDispatch::OpenDetails(instance));
                }
            }
        });
}

fn days_in_month(month: NaiveDate) -> u32 {
    let next_month = add_months(month, 1);
    let last_day = next_month - Duration::days(1);
    last_day.day()
}
