//! Calendar data models and utilities.
use crate::common::json_watch::{watch_json, JsonWatcher};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use once_cell::sync::Lazy;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, RwLock,
};

pub const CALENDAR_EVENTS_FILE: &str = "calendar/events.json";
pub const CALENDAR_STATE_FILE: &str = "calendar/state.json";

static CALENDAR_VERSION: AtomicU64 = AtomicU64::new(0);

pub fn calendar_version() -> u64 {
    CALENDAR_VERSION.load(Ordering::SeqCst)
}

fn bump_calendar_version() {
    CALENDAR_VERSION.fetch_add(1, Ordering::SeqCst);
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Reminder {
    pub minutes_before: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NthWeekday {
    pub nth: i8,
    #[serde(with = "weekday_serde")]
    pub weekday: Weekday,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum RecurrenceEnd {
    Never,
    OnDate {
        #[serde(with = "naive_date_serde")]
        date: NaiveDate,
    },
    AfterCount {
        count: u32,
    },
}

impl Default for RecurrenceEnd {
    fn default() -> Self {
        RecurrenceEnd::Never
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum RecurrenceFrequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum CustomRecurrenceUnit {
    Days,
    Weeks,
    Months,
    Years,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RecurrenceRule {
    pub frequency: RecurrenceFrequency,
    #[serde(default = "default_interval")]
    pub interval: u32,
    #[serde(default, with = "weekday_vec_serde")]
    pub weekly_days: Vec<Weekday>,
    #[serde(default)]
    pub nth_weekday: Option<NthWeekday>,
    #[serde(default)]
    pub end: RecurrenceEnd,
    #[serde(default)]
    pub custom_unit: Option<CustomRecurrenceUnit>,
}

fn default_interval() -> u32 {
    1
}

impl RecurrenceRule {
    fn interval_days(&self) -> i64 {
        self.interval.max(1) as i64
    }

    fn interval_count(&self) -> i32 {
        self.interval.max(1) as i32
    }

    fn effective_custom_unit(&self) -> CustomRecurrenceUnit {
        self.custom_unit
            .clone()
            .unwrap_or(CustomRecurrenceUnit::Days)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    #[serde(with = "naive_datetime_serde")]
    pub start: NaiveDateTime,
    #[serde(default, with = "option_naive_datetime_serde")]
    pub end: Option<NaiveDateTime>,
    #[serde(default)]
    pub duration_minutes: Option<i64>,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub recurrence: Option<RecurrenceRule>,
    #[serde(default)]
    pub reminders: Vec<Reminder>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default = "default_created_at", with = "naive_datetime_serde")]
    pub created_at: NaiveDateTime,
    #[serde(default, with = "option_naive_datetime_serde")]
    pub updated_at: Option<NaiveDateTime>,
}

fn default_created_at() -> NaiveDateTime {
    chrono::Local::now().naive_local()
}

impl CalendarEvent {
    pub fn resolved_end(&self) -> NaiveDateTime {
        if let Some(end) = self.end {
            end
        } else if let Some(minutes) = self.duration_minutes {
            self.start + Duration::minutes(minutes)
        } else if self.all_day {
            self.start + Duration::days(1)
        } else {
            self.start
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecurrenceMetadata {
    pub occurrence_index: u32,
    pub rule: RecurrenceRule,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventInstance {
    pub instance_id: String,
    pub source_event_id: String,
    #[serde(with = "naive_datetime_serde")]
    pub start: NaiveDateTime,
    #[serde(with = "naive_datetime_serde")]
    pub end: NaiveDateTime,
    pub all_day: bool,
    #[serde(default)]
    pub recurrence: Option<RecurrenceMetadata>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CalendarState {
    #[serde(default, with = "option_naive_datetime_serde")]
    pub last_opened: Option<NaiveDateTime>,
    #[serde(default, with = "option_naive_date_serde")]
    pub last_viewed_day: Option<NaiveDate>,
}

#[derive(Clone, Debug, Default)]
pub struct CalendarSnapshot {
    pub events_today: Vec<EventInstance>,
    pub events_next_7_days: Vec<EventInstance>,
    pub month_markers: Vec<NaiveDate>,
    pub next_trigger: Option<NaiveDateTime>,
}

#[derive(Clone, Debug, Default)]
struct CalendarIndex {
    titles: Vec<(String, String)>,
    tags: HashMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Default)]
struct CalendarIndexState {
    version: u64,
    index: CalendarIndex,
}

pub static CALENDAR_DATA: Lazy<Arc<RwLock<Vec<CalendarEvent>>>> = Lazy::new(|| {
    Arc::new(RwLock::new(
        load_events(CALENDAR_EVENTS_FILE).unwrap_or_default(),
    ))
});

static CALENDAR_INDEX: Lazy<Arc<RwLock<CalendarIndexState>>> =
    Lazy::new(|| Arc::new(RwLock::new(CalendarIndexState::default())));

pub fn load_events(path: &str) -> anyhow::Result<Vec<CalendarEvent>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<CalendarEvent> = serde_json::from_str(&content)?;
    Ok(list)
}

pub fn save_events(path: &str, events: &[CalendarEvent]) -> anyhow::Result<()> {
    ensure_parent_dir(path)?;
    let json = serde_json::to_string_pretty(events)?;
    std::fs::write(path, json)?;
    update_cache(events.to_vec());
    Ok(())
}

pub fn refresh_events_from_disk(path: &str) -> anyhow::Result<Vec<CalendarEvent>> {
    let list = load_events(path)?;
    let mut should_update = true;
    if let Ok(guard) = CALENDAR_DATA.read() {
        should_update = *guard != list;
    }
    if should_update {
        update_cache(list.clone());
    }
    Ok(list)
}

pub fn load_state(path: &str) -> anyhow::Result<CalendarState> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(CalendarState::default());
    }
    let state: CalendarState = serde_json::from_str(&content)?;
    Ok(state)
}

pub fn save_state(path: &str, state: &CalendarState) -> anyhow::Result<()> {
    ensure_parent_dir(path)?;
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(path, json)?;
    Ok(())
}

fn ensure_parent_dir(path: &str) -> anyhow::Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

fn update_cache(list: Vec<CalendarEvent>) {
    if let Ok(mut lock) = CALENDAR_DATA.write() {
        *lock = list;
    }
    bump_calendar_version();
    refresh_index();
}

fn refresh_index() {
    let events = CALENDAR_DATA.read().map(|d| d.clone()).unwrap_or_default();
    let index = build_index(&events);
    let version = calendar_version();
    if let Ok(mut guard) = CALENDAR_INDEX.write() {
        guard.version = version;
        guard.index = index;
    }
}

fn build_index(events: &[CalendarEvent]) -> CalendarIndex {
    let mut tags: HashMap<String, Vec<String>> = HashMap::new();
    let mut titles = Vec::new();
    for event in events {
        titles.push((event.id.clone(), event.title.to_lowercase()));
        for tag in &event.tags {
            tags.entry(tag.to_lowercase())
                .or_default()
                .push(event.id.clone());
        }
    }
    CalendarIndex { titles, tags }
}

pub fn search_by_title(query: &str) -> Vec<CalendarEvent> {
    let query = query.to_lowercase();
    let data = CALENDAR_DATA.read().map(|d| d.clone()).unwrap_or_default();
    let mut ids = HashSet::new();
    if let Ok(mut guard) = CALENDAR_INDEX.write() {
        if guard.version != calendar_version() {
            guard.index = build_index(&data);
            guard.version = calendar_version();
        }
        for (id, title) in &guard.index.titles {
            if title.contains(&query) {
                ids.insert(id.clone());
            }
        }
    }
    data.into_iter().filter(|e| ids.contains(&e.id)).collect()
}

pub fn search_by_tag(tag: &str) -> Vec<CalendarEvent> {
    let tag = tag.to_lowercase();
    let data = CALENDAR_DATA.read().map(|d| d.clone()).unwrap_or_default();
    let mut ids: HashSet<String> = HashSet::new();
    if let Ok(mut guard) = CALENDAR_INDEX.write() {
        if guard.version != calendar_version() {
            guard.index = build_index(&data);
            guard.version = calendar_version();
        }
        if let Some(list) = guard.index.tags.get(&tag) {
            ids.extend(list.iter().cloned());
        }
    }
    data.into_iter().filter(|e| ids.contains(&e.id)).collect()
}

pub fn watch_calendar_events(path: &str) -> Option<JsonWatcher> {
    let watch_path = path.to_string();
    watch_json(path, move || {
        if let Ok(list) = load_events(&watch_path) {
            update_cache(list);
        }
    })
    .ok()
}

pub fn expand_instances(
    events: &[CalendarEvent],
    range_start: NaiveDateTime,
    range_end: NaiveDateTime,
    limit: usize,
) -> Vec<EventInstance> {
    let mut instances = Vec::new();
    for event in events {
        if instances.len() >= limit {
            break;
        }
        expand_event_instances(event, range_start, range_end, limit, &mut instances);
    }
    instances.sort_by_key(|i| i.start);
    instances.truncate(limit);
    instances
}

fn expand_event_instances(
    event: &CalendarEvent,
    range_start: NaiveDateTime,
    range_end: NaiveDateTime,
    limit: usize,
    instances: &mut Vec<EventInstance>,
) {
    if instances.len() >= limit {
        return;
    }
    let duration = event.resolved_end() - event.start;
    match &event.recurrence {
        None => {
            if event.start < range_end && event.resolved_end() >= range_start {
                instances.push(EventInstance {
                    instance_id: instance_id(&event.id, event.start),
                    source_event_id: event.id.clone(),
                    start: event.start,
                    end: event.start + duration,
                    all_day: event.all_day,
                    recurrence: None,
                });
            }
        }
        Some(rule) => {
            let mut occurrence_index: u32 = 0;
            let mut produced = 0usize;
            let mut exhausted = false;
            let mut cursor = event.start;
            let max_iterations = limit.saturating_mul(4).max(64);
            let mut iterations = 0usize;
            loop {
                if instances.len() >= limit || exhausted || iterations >= max_iterations {
                    break;
                }
                iterations += 1;
                let mut starts = Vec::new();
                match rule.frequency {
                    RecurrenceFrequency::Daily => {
                        starts.push(cursor);
                        cursor = cursor + Duration::days(rule.interval_days());
                    }
                    RecurrenceFrequency::Weekly => {
                        starts.extend(generate_weekly_occurrences(
                            rule,
                            event.start,
                            occurrence_index,
                        ));
                        occurrence_index += 1;
                    }
                    RecurrenceFrequency::Monthly => {
                        if let Some(next) =
                            generate_monthly_occurrence(rule, event.start, occurrence_index)
                        {
                            starts.push(next);
                        }
                        occurrence_index += 1;
                    }
                    RecurrenceFrequency::Yearly => {
                        if let Some(next) =
                            generate_yearly_occurrence(rule, event.start, occurrence_index)
                        {
                            starts.push(next);
                        }
                        occurrence_index += 1;
                    }
                    RecurrenceFrequency::Custom => match rule.effective_custom_unit() {
                        CustomRecurrenceUnit::Days => {
                            starts.push(cursor);
                            cursor = cursor + Duration::days(rule.interval_days());
                        }
                        CustomRecurrenceUnit::Weeks => {
                            starts.extend(generate_weekly_occurrences(
                                rule,
                                event.start,
                                occurrence_index,
                            ));
                            occurrence_index += 1;
                        }
                        CustomRecurrenceUnit::Months => {
                            if let Some(next) =
                                generate_monthly_occurrence(rule, event.start, occurrence_index)
                            {
                                starts.push(next);
                            }
                            occurrence_index += 1;
                        }
                        CustomRecurrenceUnit::Years => {
                            if let Some(next) =
                                generate_yearly_occurrence(rule, event.start, occurrence_index)
                            {
                                starts.push(next);
                            }
                            occurrence_index += 1;
                        }
                    },
                }

                for start in starts {
                    if start < event.start {
                        continue;
                    }
                    if is_past_end(rule, start.date(), produced as u32) {
                        exhausted = true;
                        break;
                    }
                    produced += 1;
                    if start < range_end && start + duration >= range_start {
                        instances.push(EventInstance {
                            instance_id: instance_id(&event.id, start),
                            source_event_id: event.id.clone(),
                            start,
                            end: start + duration,
                            all_day: event.all_day,
                            recurrence: Some(RecurrenceMetadata {
                                occurrence_index: (produced - 1) as u32,
                                rule: rule.clone(),
                            }),
                        });
                        if instances.len() >= limit {
                            break;
                        }
                    }
                    if start > range_end {
                        exhausted = true;
                        break;
                    }
                }
            }
        }
    }
}

fn generate_weekly_occurrences(
    rule: &RecurrenceRule,
    base: NaiveDateTime,
    week_index: u32,
) -> Vec<NaiveDateTime> {
    let mut days = if rule.weekly_days.is_empty() {
        vec![base.weekday()]
    } else {
        rule.weekly_days.clone()
    };
    days.sort_by_key(|d| d.num_days_from_monday());
    days.dedup();

    let week_start = base.date() - Duration::days(base.weekday().num_days_from_monday() as i64);
    let week_offset = (week_index as i64) * (rule.interval_count() as i64) * 7;
    let target_week_start = week_start + Duration::days(week_offset);

    let time = base.time();
    days.into_iter()
        .map(|weekday| {
            let offset = weekday.num_days_from_monday() as i64;
            let date = target_week_start + Duration::days(offset);
            NaiveDateTime::new(date, time)
        })
        .collect()
}

fn generate_monthly_occurrence(
    rule: &RecurrenceRule,
    base: NaiveDateTime,
    month_index: u32,
) -> Option<NaiveDateTime> {
    let date = base.date();
    let month_target = add_months(date, month_index as i32 * rule.interval_count());
    match &rule.nth_weekday {
        Some(nth) => nth_weekday_of_month(month_target.year(), month_target.month(), nth),
        None => NaiveDate::from_ymd_opt(month_target.year(), month_target.month(), date.day()),
    }
    .map(|d| NaiveDateTime::new(d, base.time()))
}

fn generate_yearly_occurrence(
    rule: &RecurrenceRule,
    base: NaiveDateTime,
    year_index: u32,
) -> Option<NaiveDateTime> {
    let date = base.date();
    let year = date.year() + (year_index as i32 * rule.interval_count());
    match &rule.nth_weekday {
        Some(nth) => nth_weekday_of_month(year, date.month(), nth),
        None => NaiveDate::from_ymd_opt(year, date.month(), date.day()),
    }
    .map(|d| NaiveDateTime::new(d, base.time()))
}

fn add_months(date: NaiveDate, months: i32) -> NaiveDate {
    let mut year = date.year();
    let mut month = date.month() as i32 + months;
    while month > 12 {
        month -= 12;
        year += 1;
    }
    while month <= 0 {
        month += 12;
        year -= 1;
    }
    NaiveDate::from_ymd_opt(year, month as u32, 1).unwrap_or(date)
}

fn nth_weekday_of_month(year: i32, month: u32, nth: &NthWeekday) -> Option<NaiveDate> {
    let nth_val = nth.nth;
    if nth_val == 0 {
        return None;
    }
    let first_day = NaiveDate::from_ymd_opt(year, month, 1)?;
    if nth_val > 0 {
        let offset = (7 + nth.weekday.num_days_from_monday() as i32
            - first_day.weekday().num_days_from_monday() as i32)
            % 7;
        let day = first_day + Duration::days(offset as i64) + Duration::weeks((nth_val - 1) as i64);
        if day.month() == month {
            Some(day)
        } else {
            None
        }
    } else {
        let next_month = add_months(first_day, 1);
        let last_day = next_month - Duration::days(1);
        let offset = (7 + last_day.weekday().num_days_from_monday() as i32
            - nth.weekday.num_days_from_monday() as i32)
            % 7;
        let day = last_day - Duration::days(offset as i64) - Duration::weeks((-nth_val - 1) as i64);
        if day.month() == month {
            Some(day)
        } else {
            None
        }
    }
}

fn is_past_end(rule: &RecurrenceRule, date: NaiveDate, count: u32) -> bool {
    match rule.end {
        RecurrenceEnd::Never => false,
        RecurrenceEnd::OnDate { date: end_date } => date > end_date,
        RecurrenceEnd::AfterCount { count: max } => count >= max,
    }
}

fn instance_id(event_id: &str, start: NaiveDateTime) -> String {
    format!("{}-{}", event_id, start.format("%Y%m%dT%H%M%S"))
}

pub fn build_snapshot(now: NaiveDateTime) -> CalendarSnapshot {
    let events = CALENDAR_DATA.read().map(|d| d.clone()).unwrap_or_default();
    let today_start = NaiveDateTime::new(now.date(), NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let today_end = today_start + Duration::days(1);
    let week_end = now + Duration::days(7);

    let events_today = expand_instances(&events, today_start, today_end, 128);
    let events_next_7_days = expand_instances(&events, now, week_end, 256);

    let month_start = NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap_or(now.date());
    let next_month = add_months(month_start, 1);
    let month_instances = expand_instances(
        &events,
        NaiveDateTime::new(month_start, NaiveTime::from_hms_opt(0, 0, 0).unwrap()),
        NaiveDateTime::new(next_month, NaiveTime::from_hms_opt(0, 0, 0).unwrap()),
        512,
    );
    let mut markers: Vec<NaiveDate> = month_instances.iter().map(|i| i.start.date()).collect();
    markers.sort();
    markers.dedup();

    let next_trigger = events_next_7_days
        .iter()
        .filter(|e| e.start >= now)
        .map(|e| e.start)
        .min();

    CalendarSnapshot {
        events_today,
        events_next_7_days,
        month_markers: markers,
        next_trigger,
    }
}

mod naive_datetime_serde {
    use super::*;

    pub fn serialize<S>(value: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.format("%Y-%m-%dT%H:%M:%S").to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S").map_err(serde::de::Error::custom)
    }
}

mod option_naive_datetime_serde {
    use super::*;

    pub fn serialize<S>(value: &Option<NaiveDateTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(v) => serializer.serialize_some(&v.format("%Y-%m-%dT%H:%M:%S").to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<NaiveDateTime>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = Option::<String>::deserialize(deserializer)?;
        match opt {
            Some(s) => NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S")
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

mod naive_date_serde {
    use super::*;

    pub fn serialize<S>(value: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.format("%Y-%m-%d").to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(serde::de::Error::custom)
    }
}

mod option_naive_date_serde {
    use super::*;

    pub fn serialize<S>(value: &Option<NaiveDate>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(v) => serializer.serialize_some(&v.format("%Y-%m-%d").to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<NaiveDate>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = Option::<String>::deserialize(deserializer)?;
        match opt {
            Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

mod weekday_serde {
    use super::*;

    pub fn serialize<S>(value: &Weekday, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{:?}", value))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Weekday, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "Mon" => Ok(Weekday::Mon),
            "Tue" => Ok(Weekday::Tue),
            "Wed" => Ok(Weekday::Wed),
            "Thu" => Ok(Weekday::Thu),
            "Fri" => Ok(Weekday::Fri),
            "Sat" => Ok(Weekday::Sat),
            "Sun" => Ok(Weekday::Sun),
            _ => Err(serde::de::Error::custom("invalid weekday")),
        }
    }
}

mod weekday_vec_serde {
    use super::*;

    pub fn serialize<S>(value: &Vec<Weekday>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let list: Vec<String> = value.iter().map(|d| format!("{:?}", d)).collect();
        list.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Weekday>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let list = Vec::<String>::deserialize(deserializer)?;
        list.into_iter()
            .map(|s| match s.as_str() {
                "Mon" => Ok(Weekday::Mon),
                "Tue" => Ok(Weekday::Tue),
                "Wed" => Ok(Weekday::Wed),
                "Thu" => Ok(Weekday::Thu),
                "Fri" => Ok(Weekday::Fri),
                "Sat" => Ok(Weekday::Sat),
                "Sun" => Ok(Weekday::Sun),
                _ => Err(serde::de::Error::custom("invalid weekday")),
            })
            .collect()
    }
}
