use super::{
    Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult, edit_typed_settings,
    jump_response_clicked, opaque_dashboard_action_identity, register_jump_response,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::calendar::{CalendarSnapshot, EventInstance, add_months, parse_date_reference};
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// Example dashboard slot:
// {"widget":"calendar","row":0,"col":0,"row_span":2,"col_span":2,
//  "settings":{"mode":"week","range_days":7,"max_items":8,"show_tags":true}}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CalendarWidgetMode {
    Day,
    #[default]
    Week,
    Month,
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
                let mode_anchor = "calendar/mode";
                let mode_popup = ctx.combo_popup_scope(mode_anchor);
                let mode_combo = egui::ComboBox::from_id_salt(ui.id().with("calendar-mode"))
                    .selected_text(match cfg.mode {
                        CalendarWidgetMode::Day => "Day",
                        CalendarWidgetMode::Week => "Week",
                        CalendarWidgetMode::Month => "Month",
                    })
                    .show_ui(ui, |ui| {
                        for (id, mode, label) in [
                            ("day", CalendarWidgetMode::Day, "Day"),
                            ("week", CalendarWidgetMode::Week, "Week"),
                            ("month", CalendarWidgetMode::Month, "Month"),
                        ] {
                            let response = ui.selectable_value(&mut cfg.mode, mode, label);
                            ctx.register_jump_mode_target_in_scope(
                                mode_popup.as_ref(),
                                ui,
                                &response,
                                format!("mode/{id}"),
                                "Calendar mode option",
                                crate::gui::jump_mode::TargetKind::Selectable,
                            );
                            changed |= response.changed();
                        }
                    });
                ctx.register_jump_mode_target(
                    ui,
                    &mode_combo.response,
                    mode_anchor,
                    "Calendar mode",
                    crate::gui::jump_mode::TargetKind::ComboBox,
                );
                ui.horizontal(|ui| {
                    ui.label("Default date");
                    let mut default_date = cfg.default_date.clone().unwrap_or_default();
                    let response = ui.text_edit_singleline(&mut default_date);
                    ctx.register_jump_mode_target(
                        ui,
                        &response,
                        "calendar/default-date",
                        "Default date",
                        crate::gui::jump_mode::TargetKind::TextEntry,
                    );
                    if response.changed() {
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
                    let response = ui.add(egui::DragValue::new(&mut cfg.range_days).range(1..=30));
                    ctx.register_jump_mode_target(
                        ui,
                        &response,
                        "calendar/range-days",
                        "Calendar range",
                        crate::gui::jump_mode::TargetKind::Slider,
                    );
                    changed |= response.changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Max items");
                    let response = ui.add(egui::DragValue::new(&mut cfg.max_items).range(1..=50));
                    ctx.register_jump_mode_target(
                        ui,
                        &response,
                        "calendar/max-items",
                        "Calendar item limit",
                        crate::gui::jump_mode::TargetKind::Slider,
                    );
                    changed |= response.changed();
                });
                let response = ui.checkbox(&mut cfg.show_completed, "Include completed/past");
                ctx.register_jump_mode_target(
                    ui,
                    &response,
                    "calendar/include-completed",
                    "Include completed events",
                    crate::gui::jump_mode::TargetKind::Checkbox,
                );
                changed |= response.changed();
                ui.separator();
                ui.heading("Display");
                let response = ui.checkbox(&mut cfg.show_tags, "Show tags");
                ctx.register_jump_mode_target(
                    ui,
                    &response,
                    "calendar/show-tags",
                    "Show tags",
                    crate::gui::jump_mode::TargetKind::Checkbox,
                );
                changed |= response.changed();
                let response = ui.checkbox(&mut cfg.compact, "Compact layout");
                ctx.register_jump_mode_target(
                    ui,
                    &response,
                    "calendar/compact",
                    "Compact layout",
                    crate::gui::jump_mode::TargetKind::Checkbox,
                );
                changed |= response.changed();
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
        if self.view_month.is_none()
            && let Some(date) = self.selected_date
        {
            self.view_month =
                Some(NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date));
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

    fn date_range(&self, selected_date: NaiveDate) -> (NaiveDate, NaiveDate) {
        let range_days = self.cfg.range_days.max(1) as i64;
        match self.cfg.mode {
            CalendarWidgetMode::Day => (selected_date, selected_date),
            CalendarWidgetMode::Week => (
                selected_date,
                selected_date + Duration::days(range_days.saturating_sub(1)),
            ),
            CalendarWidgetMode::Month => {
                let month_start =
                    NaiveDate::from_ymd_opt(selected_date.year(), selected_date.month(), 1)
                        .unwrap_or(selected_date);
                let month_end = add_months(month_start, 1) - Duration::days(1);
                (month_start, month_end)
            }
        }
    }

    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        selected_date: NaiveDate,
    ) -> Option<WidgetAction> {
        let mut action = None;
        let (start_date, end_date) = self.date_range(selected_date);
        let range_label = format!(
            "{} → {}",
            start_date.format("%Y-%m-%d"),
            end_date.format("%Y-%m-%d")
        );
        ui.horizontal(|ui| {
            ui.label(match self.cfg.mode {
                CalendarWidgetMode::Day => {
                    format!(
                        "Day view • {} • {}",
                        selected_date.format("%Y-%m-%d"),
                        range_label
                    )
                }
                CalendarWidgetMode::Week => {
                    format!(
                        "Week view • {} • {}",
                        selected_date.format("%Y-%m-%d"),
                        range_label
                    )
                }
                CalendarWidgetMode::Month => {
                    format!(
                        "Month view • {} • {}",
                        selected_date.format("%B %Y"),
                        range_label
                    )
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let add_response = ui.small_button("+ Add");
                if jump_response_clicked(
                    ui,
                    ctx,
                    &add_response,
                    || "calendar:add".into(),
                    || "+ Add".into(),
                    0,
                ) {
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
                let open_response = ui.small_button("Open");
                if jump_response_clicked(
                    ui,
                    ctx,
                    &open_response,
                    || "calendar:open".into(),
                    || "Open calendar".into(),
                    1,
                ) {
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
        ctx: &DashboardContext<'_>,
        calendar: &CalendarSnapshot,
        selected_date: NaiveDate,
        calendar_version: u64,
    ) {
        let Some(view_month) = self.view_month else {
            return;
        };
        let markers = build_marker_set(calendar, view_month);
        ui.horizontal(|ui| {
            let previous_response = ui
                .scope_builder(
                    egui::UiBuilder::new().id(ui.id().with("calendar-month-previous")),
                    |ui| ui.button("<"),
                )
                .inner;
            if jump_response_clicked(
                ui,
                ctx,
                &previous_response,
                || "calendar:month-navigation:previous".to_owned(),
                || "Previous calendar month".to_owned(),
                0,
            ) {
                self.view_month = Some(add_months(view_month, -1));
            }
            let next_response = ui
                .scope_builder(
                    egui::UiBuilder::new().id(ui.id().with("calendar-month-next")),
                    |ui| ui.button(">"),
                )
                .inner;
            if jump_response_clicked(
                ui,
                ctx,
                &next_response,
                || "calendar:month-navigation:next".to_owned(),
                || "Next calendar month".to_owned(),
                1,
            ) {
                self.view_month = Some(add_months(view_month, 1));
            }
            ui.label(view_month.format("%B %Y").to_string());
        });
        ui.separator();
        month_grid(
            ui,
            ctx,
            view_month,
            &markers,
            selected_date,
            self.cfg.compact,
            calendar_version,
            |date| {
                self.selected_date = Some(date);
            },
        );
    }

    fn render_event_list(
        &self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        calendar: &CalendarSnapshot,
        selected_date: NaiveDate,
        now: NaiveDateTime,
        today: NaiveDate,
        calendar_version: u64,
    ) -> Option<WidgetAction> {
        let mut action = None;
        let (start_date, end_date) = self.date_range(selected_date);
        let mut instances = self.collect_events(calendar, start_date, end_date, today);
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
        let mut duplicate_occurrences: HashMap<(String, NaiveDateTime, NaiveDateTime), usize> =
            HashMap::new();
        for (visual_order, instance) in instances.into_iter().enumerate() {
            let event_key = (
                instance.source_event_id.clone(),
                instance.start,
                instance.end,
            );
            let occurrence = duplicate_occurrences.entry(event_key).or_default();
            let duplicate_occurrence = *occurrence;
            *occurrence = (*occurrence).saturating_add(1);
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
            let native_id = ui.id().with((
                "calendar-event",
                instance.source_event_id.as_str(),
                instance.start,
                instance.end,
                duplicate_occurrence,
            ));
            let response = ui
                .scope_builder(egui::UiBuilder::new().id(native_id), |ui| ui.add(button))
                .inner;
            if jump_response_clicked(
                ui,
                ctx,
                &response,
                || {
                    calendar_event_identity(
                        instance,
                        duplicate_occurrence,
                        calendar_version,
                        start_date,
                        end_date,
                    )
                },
                || "Calendar event".to_owned(),
                visual_order as u64,
            ) {
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
            if self.cfg.show_tags
                && let Some(tags) = calendar.event_tags.get(&instance.source_event_id)
                && !tags.is_empty()
            {
                let tag_label = format!("#{}", tags.join(" #"));
                ui.label(egui::RichText::new(tag_label).text_style(egui::TextStyle::Small));
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
        let mut selected_date = self.selected_date.unwrap_or(today);

        let snapshot = ctx.data_cache.snapshot();
        let calendar = snapshot.calendar.as_ref();

        if let Some(action) = self.render_header(ui, ctx, selected_date) {
            return Some(action);
        }

        ui.separator();
        if self.cfg.mode == CalendarWidgetMode::Month {
            self.render_month_grid(ui, ctx, calendar, selected_date, ctx.calendar_version);
            selected_date = self.selected_date.unwrap_or(selected_date);
            ui.separator();
        }

        self.render_event_list(
            ui,
            ctx,
            calendar,
            selected_date,
            now,
            today,
            ctx.calendar_version,
        )
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
    ctx: &DashboardContext<'_>,
    month: NaiveDate,
    markers: &HashSet<NaiveDate>,
    selected_date: NaiveDate,
    compact: bool,
    calendar_version: u64,
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
                let response = ui
                    .scope_builder(
                        egui::UiBuilder::new().id(ui.id().with((
                            "calendar-date",
                            date.year(),
                            date.month(),
                            date.day(),
                        ))),
                        |ui| ui.selectable_label(date == selected_date, text),
                    )
                    .inner;
                register_jump_response(
                    ui,
                    ctx,
                    &response,
                    || {
                        format!(
                            "calendar:date:{:04}-{:02}-{:02}/view:{:04}-{:02}/snapshot:{calendar_version}",
                            date.year(),
                            date.month(),
                            date.day(),
                            month.year(),
                            month.month()
                        )
                    },
                    || "Calendar date".to_owned(),
                    crate::gui::jump_mode::TargetKind::Selectable,
                    day as u64,
                );
                if response.clicked() && ctx.jump_mode_activation_allowed {
                    on_select(date);
                }
                day += 1;
            }
        });
}

fn calendar_event_identity(
    instance: &EventInstance,
    duplicate_occurrence: usize,
    calendar_version: u64,
    range_start: NaiveDate,
    range_end: NaiveDate,
) -> String {
    let start = instance.start.format("%Y-%m-%dT%H:%M:%S%.f").to_string();
    let end = instance.end.format("%Y-%m-%dT%H:%M:%S%.f").to_string();
    let range_start = range_start.format("%Y-%m-%d").to_string();
    let range_end = range_end.format("%Y-%m-%d").to_string();
    let mut identity = String::from("calendar-event/");
    for part in [
        instance.source_event_id.as_str(),
        start.as_str(),
        end.as_str(),
        range_start.as_str(),
        range_end.as_str(),
    ] {
        identity.push_str(&part.len().to_string());
        identity.push(':');
        identity.push_str(part);
    }
    identity.push_str(&format!(
        "/snapshot:{calendar_version}/occurrence:{duplicate_occurrence}"
    ));
    format!(
        "calendar:event:{}",
        opaque_dashboard_action_identity(&identity)
    )
}

fn days_in_month(month: NaiveDate) -> u32 {
    let next_month = add_months(month, 1);
    let last_day = next_month - Duration::days(1);
    last_day.day()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ActionIdLookup;
    use crate::dashboard::data_cache::DashboardDataCache;
    use crate::gui::jump_mode::ui::JumpModeUi;
    use crate::gui::jump_mode::{SurfaceId, TargetGeneration};
    use once_cell::sync::Lazy;
    use std::sync::Arc;

    fn dashboard_input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn dashboard_scope(jump: &JumpModeUi) -> crate::gui::JumpModeScope {
        jump.scope(
            SurfaceId::Dashboard,
            TargetGeneration(7),
            "dashboard-test",
            egui::ViewportId::ROOT,
            true,
        )
        .expect("enabled Jump Mode creates a dashboard scope")
        .child_namespace("slot-id:calendar")
        .child_namespace("body")
    }

    fn dashboard_context<'a>(
        plugins: &'a crate::plugin::PluginManager,
        data_cache: &'a DashboardDataCache,
        calendar_version: u64,
        jump_mode_scope: Option<crate::gui::JumpModeScope>,
        jump_mode_activation_allowed: bool,
    ) -> DashboardContext<'a> {
        static EMPTY_USAGE: Lazy<HashMap<String, u32>> = Lazy::new(HashMap::new);
        static EMPTY_LOOKUP: Lazy<ActionIdLookup> = Lazy::new(ActionIdLookup::default);
        DashboardContext {
            actions: &[],
            actions_by_id: &EMPTY_LOOKUP,
            usage: &EMPTY_USAGE,
            plugins,
            enabled_plugins: None,
            default_location: None,
            data_cache,
            actions_version: 0,
            fav_version: 0,
            notes_version: 0,
            todo_version: 0,
            calendar_version,
            clipboard_version: 0,
            snippets_version: 0,
            dashboard_visible: true,
            dashboard_focused: true,
            reduce_dashboard_work_when_unfocused: false,
            diagnostics: None,
            show_diagnostics_widget: false,
            jump_mode_scope,
            jump_mode_activation_allowed,
        }
    }

    fn render_with_input(
        widget: &mut CalendarWidget,
        ctx: &DashboardContext<'_>,
        egui_ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> Option<WidgetAction> {
        render_with_input_and_pass_count(widget, ctx, egui_ctx, events).0
    }

    fn render_with_input_and_pass_count(
        widget: &mut CalendarWidget,
        ctx: &DashboardContext<'_>,
        egui_ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> (Option<WidgetAction>, usize) {
        let mut action = None;
        let output = egui_ctx.run_ui(dashboard_input(events), |ui| {
            ui.push_id("slot-id:calendar", |ui| {
                ui.push_id("body", |ui| {
                    action = widget.render(ui, ctx, WidgetActivation::Click);
                });
            });
        });
        let pass_count = output.platform_output.num_completed_passes;
        output.drop_without_applying_deltas();
        (action, pass_count)
    }

    fn key_event(key: egui::Key, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn jump_mode_dashboard_simple_controls_calendar_focus_and_fresh_actions() {
        use crate::gui::jump_mode::TargetKind;

        let plugins = crate::plugin::PluginManager::new_inert_for_test();
        let data_cache = DashboardDataCache::new();
        let mut snapshot = crate::dashboard::data_cache::DashboardDataSnapshot::default();
        let event_date = NaiveDate::from_ymd_opt(2024, 5, 16).unwrap();
        let start = event_date.and_hms_opt(13, 0, 0).unwrap();
        let instance = EventInstance {
            instance_id: "private-instance-id".into(),
            source_event_id: "private-source-id".into(),
            start,
            end: start + Duration::hours(1),
            all_day: false,
            recurrence: None,
        };
        let mut calendar = CalendarSnapshot {
            events_next_7_days: vec![instance.clone()],
            ..CalendarSnapshot::default()
        };
        calendar.event_titles.insert(
            instance.source_event_id.clone(),
            "Private customer event Ω".into(),
        );
        calendar
            .event_tags
            .insert(instance.source_event_id.clone(), vec!["private tag".into()]);
        snapshot.calendar = Arc::new(calendar);
        data_cache.set_snapshot_for_test(snapshot);

        let mut widget = CalendarWidget::new(CalendarWidgetConfig {
            mode: CalendarWidgetMode::Month,
            default_date: Some("2024-05-15".into()),
            ..CalendarWidgetConfig::default()
        });
        let egui_ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);
        let version = 9;

        // The actual dashboard runs a per-pass target collector. Warm egui's
        // first Grid sizing pass without a collector so this direct widget
        // fixture matches that lifecycle instead of treating discarded
        // measurements as duplicate current responses.
        let unscoped_context = dashboard_context(&plugins, &data_cache, version, None, true);
        let (_, sizing_passes) =
            render_with_input_and_pass_count(&mut widget, &unscoped_context, &egui_ctx, Vec::new());
        assert!(sizing_passes >= 1);

        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, version, Some(scope.clone()), true);
        let (action, collected_passes) =
            render_with_input_and_pass_count(&mut widget, &context, &egui_ctx, Vec::new());
        assert!(action.is_none());
        assert_eq!(
            collected_passes, 1,
            "the warmed Grid collects one current pass"
        );
        jump.finish_ui_pass();
        let registry = jump.pass_registry().unwrap();
        let date_identity = "calendar:date:2024-05-16/view:2024-05/snapshot:9";
        let date_target = scope.target_id(date_identity);
        let date = registry
            .get(&date_target)
            .expect("the actual May 16 cell is visible");
        assert_eq!(date.metadata().kind(), TargetKind::Selectable);
        assert_eq!(date.metadata().label(), "Calendar date");
        let previous = registry
            .targets()
            .iter()
            .find(|target| target.metadata().label() == "Previous calendar month")
            .unwrap_or_else(|| {
                panic!(
                    "the rendered previous-month control is registered; labels={:?}",
                    registry
                        .metadata()
                        .map(|metadata| metadata.label())
                        .collect::<Vec<_>>()
                )
            });
        assert_eq!(previous.metadata().kind(), TargetKind::Button);
        let next = registry
            .targets()
            .iter()
            .find(|target| target.metadata().label() == "Next calendar month")
            .expect("the rendered next-month control is registered");
        let next_target_id = next.id().clone();
        let next_native_id = next.response_id();
        let event_target = registry
            .targets()
            .iter()
            .find(|target| target.metadata().label() == "Calendar event")
            .expect("the rendered event response is registered");
        assert_eq!(event_target.metadata().kind(), TargetKind::Button);
        let all_semantics = registry
            .targets()
            .iter()
            .map(|target| format!("{:?}", target.id().semantic()))
            .collect::<Vec<_>>();
        assert!(all_semantics.iter().all(|semantic| {
            !semantic.contains("private-source-id")
                && !semantic.contains("private-instance-id")
                && !semantic.contains("Private customer event")
                && !semantic.contains("private tag")
        }));
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        assert!(jump.queue_focus_intent(
            date_target.clone(),
            egui::ViewportId::ROOT,
            jump.navigation_epoch(),
        ));
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, version, Some(scope), true);
        assert!(render_with_input(&mut widget, &context, &egui_ctx, Vec::new()).is_none());
        jump.finish_ui_pass();
        let focused_date = jump.pass_registry().unwrap().get(&date_target).unwrap();
        let date_response_id = focused_date.response_id();
        assert_eq!(
            egui_ctx.memory(|memory| memory.focused()),
            Some(date_response_id)
        );
        assert_eq!(widget.selected_date, NaiveDate::from_ymd_opt(2024, 5, 15));
        jump.complete_frame(&egui_ctx, true, true);

        let click_position = jump
            .retained_registry()
            .get(&date_target)
            .unwrap()
            .clipped_rect()
            .center();
        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, version, Some(scope), true);
        assert!(
            render_with_input(
                &mut widget,
                &context,
                &egui_ctx,
                vec![
                    egui::Event::PointerMoved(click_position),
                    egui::Event::PointerButton {
                        pos: click_position,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            )
            .is_none()
        );
        jump.finish_ui_pass();
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, version, Some(scope), true);
        assert!(
            render_with_input(
                &mut widget,
                &context,
                &egui_ctx,
                vec![egui::Event::PointerButton {
                    pos: click_position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            )
            .is_none()
        );
        jump.finish_ui_pass();
        let registry = jump.pass_registry().unwrap();
        assert_eq!(widget.selected_date, Some(event_date));
        let selected_event = registry
            .targets()
            .iter()
            .find(|target| target.metadata().label() == "Calendar event")
            .expect("selecting the civil date leaves its current event response visible");
        let event_target_id = selected_event.id().clone();
        let event_response_id = selected_event.response_id();
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        assert!(jump.queue_focus_intent(
            event_target_id.clone(),
            egui::ViewportId::ROOT,
            jump.navigation_epoch(),
        ));
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, version, Some(scope), true);
        assert!(render_with_input(&mut widget, &context, &egui_ctx, Vec::new()).is_none());
        jump.finish_ui_pass();
        assert_eq!(
            egui_ctx.memory(|memory| memory.focused()),
            Some(event_response_id),
            "hint completion only focuses the event button"
        );
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, version, Some(scope), true);
        let action = render_with_input(
            &mut widget,
            &context,
            &egui_ctx,
            vec![key_event(egui::Key::Enter, true)],
        );
        jump.finish_ui_pass();
        jump.complete_frame(&egui_ctx, true, true);
        assert_eq!(
            action.map(|action| action.action.action),
            Some("calendar:open".into()),
            "a later native Enter invokes the existing event-open action"
        );

        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, version, Some(scope), true);
        assert!(
            render_with_input(
                &mut widget,
                &context,
                &egui_ctx,
                vec![key_event(egui::Key::Enter, false)],
            )
            .is_none()
        );
        jump.finish_ui_pass();
        jump.complete_frame(&egui_ctx, true, true);

        let next_target = next_target_id;
        jump.begin_frame();
        assert!(jump.queue_focus_intent(
            next_target.clone(),
            egui::ViewportId::ROOT,
            jump.navigation_epoch(),
        ));
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, version, Some(scope), true);
        assert!(render_with_input(&mut widget, &context, &egui_ctx, Vec::new()).is_none());
        jump.finish_ui_pass();
        let next_target = jump.pass_registry().unwrap().get(&next_target).unwrap();
        assert_eq!(next_target.response_id(), next_native_id);
        assert_eq!(
            egui_ctx.memory(|memory| memory.focused()),
            Some(next_native_id)
        );
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, version, Some(scope), true);
        assert!(
            render_with_input(
                &mut widget,
                &context,
                &egui_ctx,
                vec![key_event(egui::Key::Enter, true)],
            )
            .is_none()
        );
        jump.finish_ui_pass();
        assert_eq!(widget.view_month, NaiveDate::from_ymd_opt(2024, 6, 1));
        assert_eq!(
            egui_ctx.memory(|memory| memory.focused()),
            Some(next_native_id),
            "a stable month-navigation response keeps native focus after changing months"
        );
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, version, Some(scope), true);
        assert!(
            render_with_input(
                &mut widget,
                &context,
                &egui_ctx,
                vec![key_event(egui::Key::Enter, false)],
            )
            .is_none()
        );
        jump.finish_ui_pass();
        let registry = jump.pass_registry().unwrap();
        let june_next = registry
            .targets()
            .iter()
            .find(|target| target.metadata().label() == "Next calendar month")
            .expect("the next-month target is refreshed for June");
        assert_eq!(june_next.response_id(), next_native_id);
        assert_eq!(
            egui_ctx.memory(|memory| memory.focused()),
            Some(next_native_id)
        );
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, version, Some(scope), true);
        assert!(
            render_with_input(
                &mut widget,
                &context,
                &egui_ctx,
                vec![key_event(egui::Key::Enter, true)],
            )
            .is_none()
        );
        jump.finish_ui_pass();
        assert_eq!(widget.view_month, NaiveDate::from_ymd_opt(2024, 7, 1));
        assert_eq!(
            egui_ctx.memory(|memory| memory.focused()),
            Some(next_native_id)
        );
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, version, Some(scope), true);
        assert!(
            render_with_input(
                &mut widget,
                &context,
                &egui_ctx,
                vec![key_event(egui::Key::Enter, false)],
            )
            .is_none()
        );
        jump.finish_ui_pass();
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, 10, Some(scope), true);
        assert!(render_with_input(&mut widget, &context, &egui_ctx, Vec::new()).is_none());
        jump.finish_ui_pass();
        let refreshed_event = jump
            .pass_registry()
            .unwrap()
            .targets()
            .iter()
            .find(|target| target.metadata().label() == "Calendar event")
            .expect("the event remains visible after a snapshot refresh");
        assert_ne!(refreshed_event.id(), &event_target_id);
        assert_eq!(refreshed_event.response_id(), event_response_id);
    }
}
