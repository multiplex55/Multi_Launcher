//! Calendar data models and utilities.
use crate::actions::Action;
use crate::common::entity_ref::{EntityKind, EntityRef};
use crate::common::json_watch::{JsonWatcher, watch_json};
use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use crate::common::query::parse_query_filters;
use crate::common::strip_prefix_ci;
use crate::plugin::Plugin;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use once_cell::sync::Lazy;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{
    Arc, Mutex, MutexGuard, RwLock,
    atomic::{AtomicU64, Ordering},
};

pub const CALENDAR_EVENTS_FILE: &str = "calendar/events.json";
pub const CALENDAR_STATE_FILE: &str = "calendar/state.json";

static CALENDAR_VERSION: AtomicU64 = AtomicU64::new(0);
static NEXT_EVENT_ID: AtomicU64 = AtomicU64::new(1);
static CALENDAR_EVENT_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
#[cfg(test)]
static CALENDAR_TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[cfg(test)]
pub(crate) fn calendar_test_guard() -> MutexGuard<'static, ()> {
    CALENDAR_TEST_MUTEX
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
pub(crate) fn restore_calendar_test_data(events: Vec<CalendarEvent>) {
    *CALENDAR_DATA
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = events;
    *CALENDAR_INDEX
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = CalendarIndexState::default();
}

pub fn calendar_version() -> u64 {
    CALENDAR_VERSION.load(Ordering::SeqCst)
}

fn bump_calendar_version() {
    CALENDAR_VERSION.fetch_add(1, Ordering::SeqCst);
}

fn next_event_id() -> String {
    let next = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("evt-{ts}-{next}")
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum RecurrenceEnd {
    #[default]
    Never,
    OnDate {
        #[serde(with = "naive_date_serde")]
        date: NaiveDate,
    },
    AfterCount {
        count: u32,
    },
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
    #[serde(default)]
    pub entity_refs: Vec<EntityRef>,
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
    pub event_titles: HashMap<String, String>,
    pub event_tags: HashMap<String, Vec<String>>,
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
    let events = load_events(CALENDAR_EVENTS_FILE).unwrap_or_else(|error| {
        tracing::error!(%error, "calendar startup retained invalid event store");
        Vec::new()
    });
    Arc::new(RwLock::new(events))
});

static CALENDAR_INDEX: Lazy<Arc<RwLock<CalendarIndexState>>> =
    Lazy::new(|| Arc::new(RwLock::new(CalendarIndexState::default())));

pub fn load_events(path: &str) -> anyhow::Result<Vec<CalendarEvent>> {
    let _transaction = calendar_event_transaction_guard();
    load_events_unlocked(path).map_err(Into::into)
}

pub fn load_events_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<CalendarEvent>>, PersistenceError> {
    load_json(path)
}

fn load_events_unlocked(path: &str) -> Result<Vec<CalendarEvent>, PersistenceError> {
    Ok(match load_events_typed(path)? {
        LoadState::Missing | LoadState::Empty => Vec::new(),
        LoadState::Loaded(events) => events,
    })
}

pub fn save_events(path: &str, events: &[CalendarEvent]) -> anyhow::Result<()> {
    replace_events(path, events.to_vec()).map(|_| ())
}

pub fn replace_events(
    path: &str,
    replacement: Vec<CalendarEvent>,
) -> anyhow::Result<Vec<CalendarEvent>> {
    update_events(path, move |events| {
        *events = replacement;
        Ok(true)
    })
}

pub fn update_events(
    path: &str,
    mutate: impl FnOnce(&mut Vec<CalendarEvent>) -> anyhow::Result<bool>,
) -> anyhow::Result<Vec<CalendarEvent>> {
    update_events_with_save(path, mutate, |path, events| {
        save_json_atomic(path, events).map_err(Into::into)
    })
}

fn update_events_with_save(
    path: &str,
    mutate: impl FnOnce(&mut Vec<CalendarEvent>) -> anyhow::Result<bool>,
    save: impl FnOnce(&str, &[CalendarEvent]) -> anyhow::Result<()>,
) -> anyhow::Result<Vec<CalendarEvent>> {
    // Publication touches the lazy process cache. Initialize it before the
    // transaction lock so startup loading cannot recursively acquire it.
    Lazy::force(&CALENDAR_DATA);
    let _transaction = calendar_event_transaction_guard();
    let mut events = load_events_unlocked(path)?;
    if mutate(&mut events)? {
        save(path, &events)?;
        publish_calendar_snapshot(events.clone());
    }
    Ok(events)
}

pub fn refresh_events_from_disk(path: &str) -> anyhow::Result<Vec<CalendarEvent>> {
    Lazy::force(&CALENDAR_DATA);
    let _transaction = calendar_event_transaction_guard();
    let events = match load_events_typed(path)? {
        LoadState::Missing => {
            anyhow::bail!("calendar events file was removed; retaining last-good state")
        }
        LoadState::Empty => Vec::new(),
        LoadState::Loaded(events) => events,
    };
    let should_publish = CALENDAR_DATA
        .read()
        .map(|current| *current != events)
        .unwrap_or(true);
    if should_publish {
        publish_calendar_snapshot(events.clone());
    }
    Ok(events)
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
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn calendar_event_transaction_guard() -> MutexGuard<'static, ()> {
    CALENDAR_EVENT_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn publish_calendar_snapshot(list: Vec<CalendarEvent>) {
    let index = build_index(&list);
    *CALENDAR_DATA
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = list;
    bump_calendar_version();
    let version = calendar_version();
    let mut guard = CALENDAR_INDEX
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.version = version;
    guard.index = index;
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
        if let Err(error) = refresh_events_from_disk(&watch_path) {
            tracing::error!(%error, "invalid calendar reload retained last-good events");
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
                        cursor += Duration::days(rule.interval_days());
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
                            cursor += Duration::days(rule.interval_days());
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

pub(crate) fn add_months(date: NaiveDate, months: i32) -> NaiveDate {
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

pub fn new_event_id() -> String {
    next_event_id()
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

    let event_titles = events
        .iter()
        .map(|event| (event.id.clone(), event.title.clone()))
        .collect();
    let event_tags = events
        .iter()
        .map(|event| (event.id.clone(), event.tags.clone()))
        .collect();

    CalendarSnapshot {
        events_today,
        events_next_7_days,
        month_markers: markers,
        next_trigger,
        event_titles,
        event_tags,
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

#[derive(Debug, Clone)]
pub struct CalendarAddRequest {
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub all_day: bool,
    pub title: String,
    pub notes: Option<String>,
    pub refs: Vec<EntityRef>,
}

#[derive(Debug, Clone)]
pub struct CalendarSearchRequest {
    pub query: String,
    pub tags: Vec<String>,
    pub exclude_tags: Vec<String>,
    pub after: Option<NaiveDate>,
}

pub fn parse_date_reference(input: &str, now: NaiveDate) -> Option<NaiveDate> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_lowercase();
    if lower == "today" {
        return Some(now);
    }
    if lower == "tomorrow" {
        return Some(now + Duration::days(1));
    }
    if let Some(rest) = lower.strip_prefix("next ")
        && let Some(target) = parse_weekday(rest)
    {
        return Some(next_weekday(now, target));
    }
    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").ok()
}

pub fn parse_calendar_add(input: &str, now: NaiveDateTime) -> Result<CalendarAddRequest, String> {
    let mut parts = input.splitn(2, '|');
    let left = parts.next().unwrap_or("").trim();
    let notes = parts
        .next()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    if left.is_empty() {
        return Err("Expected a date, time, and title".into());
    }
    let raw_tokens: Vec<&str> = left.split_whitespace().collect();
    if raw_tokens.is_empty() {
        return Err("Expected a date, time, and title".into());
    }
    let mut refs = Vec::new();
    let mut tokens = Vec::new();
    for token in raw_tokens {
        if let Some(stripped) = token.strip_prefix('@')
            && let Some((kind, id)) = stripped.split_once(':')
        {
            let kind = match kind.to_ascii_lowercase().as_str() {
                "todo" => Some(EntityKind::Todo),
                "note" => Some(EntityKind::Note),
                _ => None,
            };
            if let Some(kind) = kind {
                refs.push(EntityRef::new(kind, id.trim().to_string(), None));
                continue;
            }
        }
        tokens.push(token);
    }
    let (date, consumed) = parse_date_tokens(&tokens, now.date())
        .ok_or_else(|| "Invalid date (use today, tomorrow, next mon, or YYYY-MM-DD)".to_string())?;
    let time_token = tokens
        .get(consumed)
        .ok_or_else(|| "Expected a time (e.g., 09:00) or all-day".to_string())?;
    let time_spec = parse_time_spec(time_token)?;
    let title_tokens = tokens.get(consumed + 1..).unwrap_or(&[]);
    if title_tokens.is_empty() {
        return Err("Expected a title after the time".into());
    }
    let title = title_tokens.join(" ");
    Ok(CalendarAddRequest {
        date,
        time: time_spec.time,
        all_day: time_spec.all_day,
        title,
        notes,
        refs,
    })
}

pub fn parse_calendar_search(input: &str) -> Result<CalendarSearchRequest, String> {
    let filters = parse_query_filters(input, &["tag:"]);
    let mut query_parts = Vec::new();
    let mut after = None;
    for token in filters.remaining_tokens {
        if let Some(date) = token.strip_prefix("after:") {
            let parsed = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
                .map_err(|_| "Invalid after: date (use YYYY-MM-DD)".to_string())?;
            after = Some(parsed);
        } else {
            query_parts.push(token);
        }
    }
    Ok(CalendarSearchRequest {
        query: query_parts.join(" "),
        tags: filters.include_tags,
        exclude_tags: filters.exclude_tags,
        after,
    })
}

pub fn add_event(request: CalendarAddRequest, now: NaiveDateTime) -> anyhow::Result<CalendarEvent> {
    add_event_at(CALENDAR_EVENTS_FILE, request, now)
}

fn add_event_at(
    path: &str,
    request: CalendarAddRequest,
    now: NaiveDateTime,
) -> anyhow::Result<CalendarEvent> {
    let start_time = request
        .time
        .unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let start = NaiveDateTime::new(request.date, start_time);
    let event = CalendarEvent {
        id: next_event_id(),
        title: request.title,
        start,
        end: None,
        duration_minutes: None,
        all_day: request.all_day,
        notes: request.notes,
        recurrence: None,
        reminders: Vec::new(),
        tags: Vec::new(),
        category: None,
        created_at: now,
        updated_at: None,
        entity_refs: request.refs,
    };
    let event_to_add = event.clone();
    update_events(path, move |events| {
        events.push(event_to_add);
        Ok(true)
    })?;
    Ok(event)
}

pub fn snooze_event(event_id: &str, duration: Duration) -> anyhow::Result<bool> {
    snooze_event_at(CALENDAR_EVENTS_FILE, event_id, duration)
}

fn snooze_event_at(path: &str, event_id: &str, duration: Duration) -> anyhow::Result<bool> {
    let mut updated = false;
    let now = chrono::Local::now().naive_local();
    let event_id = event_id.to_owned();
    update_events(path, move |events| {
        for event in events {
            if event.id == event_id {
                event.start += duration;
                if let Some(end) = event.end {
                    event.end = Some(end + duration);
                }
                event.updated_at = Some(now);
                updated = true;
                break;
            }
        }
        Ok(updated)
    })?;
    Ok(updated)
}

pub fn search_events(request: &CalendarSearchRequest) -> Vec<CalendarEvent> {
    let data = CALENDAR_DATA.read().map(|d| d.clone()).unwrap_or_default();
    let query = request.query.to_lowercase();
    let mut results: Vec<CalendarEvent> = data
        .into_iter()
        .filter(|event| {
            if let Some(after) = request.after
                && event.start.date() < after
            {
                return false;
            }
            if !request.tags.is_empty() || !request.exclude_tags.is_empty() {
                let tags = event
                    .tags
                    .iter()
                    .map(|t| t.to_lowercase())
                    .collect::<Vec<_>>();
                if !request.tags.is_empty() && !request.tags.iter().all(|t| tags.contains(t)) {
                    return false;
                }
                if request.exclude_tags.iter().any(|t| tags.contains(t)) {
                    return false;
                }
            }
            if query.is_empty() {
                return true;
            }
            let title = event.title.to_lowercase();
            let notes = event.notes.as_deref().unwrap_or("").to_lowercase();
            title.contains(&query) || notes.contains(&query)
        })
        .collect();
    results.sort_by_key(|event| event.start);
    results
}

fn parse_date_tokens(tokens: &[&str], now: NaiveDate) -> Option<(NaiveDate, usize)> {
    if tokens.is_empty() {
        return None;
    }
    if tokens[0].eq_ignore_ascii_case("next")
        && let Some(next) = tokens.get(1)
        && let Some(day) = parse_weekday(next)
    {
        return Some((next_weekday(now, day), 2));
    }
    parse_date_reference(tokens[0], now).map(|date| (date, 1))
}

fn parse_weekday(token: &str) -> Option<Weekday> {
    match token.to_lowercase().as_str() {
        "mon" | "monday" => Some(Weekday::Mon),
        "tue" | "tues" | "tuesday" => Some(Weekday::Tue),
        "wed" | "wednesday" => Some(Weekday::Wed),
        "thu" | "thurs" | "thursday" => Some(Weekday::Thu),
        "fri" | "friday" => Some(Weekday::Fri),
        "sat" | "saturday" => Some(Weekday::Sat),
        "sun" | "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

fn next_weekday(date: NaiveDate, target: Weekday) -> NaiveDate {
    let current = date.weekday().num_days_from_monday() as i64;
    let target = target.num_days_from_monday() as i64;
    let mut diff = (target - current + 7) % 7;
    if diff == 0 {
        diff = 7;
    }
    date + Duration::days(diff)
}

struct TimeSpec {
    time: Option<NaiveTime>,
    all_day: bool,
}

fn parse_time_spec(token: &str) -> Result<TimeSpec, String> {
    let lower = token.trim().to_lowercase();
    if lower == "all-day" || lower == "allday" {
        return Ok(TimeSpec {
            time: None,
            all_day: true,
        });
    }
    let time = parse_time_reference(&lower)
        .ok_or_else(|| "Invalid time (use HH:MM, 9am, or all-day)".to_string())?;
    Ok(TimeSpec {
        time: Some(time),
        all_day: false,
    })
}

fn parse_time_reference(input: &str) -> Option<NaiveTime> {
    let lower = input.trim().to_lowercase();
    if lower == "noon" {
        return NaiveTime::from_hms_opt(12, 0, 0);
    }
    if lower == "midnight" {
        return NaiveTime::from_hms_opt(0, 0, 0);
    }
    let (time_part, meridiem) = if let Some(stripped) = lower.strip_suffix("am") {
        (stripped.trim(), Some("am"))
    } else if let Some(stripped) = lower.strip_suffix("pm") {
        (stripped.trim(), Some("pm"))
    } else {
        (lower.as_str(), None)
    };
    let (mut hour, minute) = if let Some((h, m)) = time_part.split_once(':') {
        (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?)
    } else {
        match time_part.len() {
            0 => return None,
            1 | 2 => (time_part.parse::<u32>().ok()?, 0),
            3 => {
                let (h, m) = time_part.split_at(1);
                (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?)
            }
            _ => {
                let (h, m) = time_part.split_at(2);
                (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?)
            }
        }
    };
    if minute > 59 || hour > 23 {
        return None;
    }
    if let Some(meridiem) = meridiem {
        if hour == 12 {
            hour = 0;
        }
        if meridiem == "pm" {
            hour += 12;
        }
    }
    NaiveTime::from_hms_opt(hour, minute, 0)
}

pub fn parse_duration_spec(input: &str) -> Option<Duration> {
    let trimmed = input.trim().to_lowercase();
    if trimmed.is_empty() {
        return None;
    }
    let (value, unit) = trimmed
        .chars()
        .partition::<String, _>(|c| c.is_ascii_digit());
    let amount = value.parse::<i64>().ok()?;
    match unit.as_str() {
        "m" | "min" | "mins" | "minute" | "minutes" => Some(Duration::minutes(amount)),
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(Duration::hours(amount)),
        "d" | "day" | "days" => Some(Duration::days(amount)),
        "w" | "wk" | "wks" | "week" | "weeks" => Some(Duration::weeks(amount)),
        _ => None,
    }
}

pub fn format_event_label(event: &CalendarEvent) -> String {
    if event.all_day {
        format!(
            "{} ({} all-day)",
            event.title,
            event.start.format("%Y-%m-%d")
        )
    } else {
        format!(
            "{} ({} {})",
            event.title,
            event.start.format("%Y-%m-%d"),
            event.start.format("%H:%M")
        )
    }
}

pub struct CalendarPlugin;

impl Plugin for CalendarPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        let Some(rest) = strip_prefix_ci(trimmed, "cal") else {
            return Vec::new();
        };
        let rest = rest.trim();
        if rest.is_empty() {
            return vec![Action {
                label: "Open calendar".into(),
                desc: "Calendar".into(),
                action: "calendar:open".into(),
                args: None,
            }];
        }
        let rest_lc = rest.to_lowercase();
        if matches!(rest_lc.as_str(), "day" | "week" | "month") {
            return vec![Action {
                label: format!("Open calendar ({rest_lc} view)"),
                desc: "Calendar".into(),
                action: format!("calendar:open:{rest_lc}"),
                args: None,
            }];
        }
        if rest_lc == "upcoming" {
            return vec![Action {
                label: "Show upcoming events".into(),
                desc: "Calendar".into(),
                action: "calendar:upcoming".into(),
                args: None,
            }];
        }
        if let Some(find) = strip_prefix_ci(rest, "find") {
            let query = find.trim();
            if query.is_empty() {
                return Vec::new();
            }
            return vec![Action {
                label: format!("Search calendar for \"{query}\""),
                desc: "Calendar".into(),
                action: format!("calendar:search:{query}"),
                args: None,
            }];
        }
        if let Some(add) = strip_prefix_ci(rest, "add") {
            let input = add.trim();
            if input.is_empty() {
                return Vec::new();
            }
            let label = match parse_calendar_add(input, chrono::Local::now().naive_local()) {
                Ok(request) => {
                    let time_label = if request.all_day {
                        "all-day".to_string()
                    } else {
                        request
                            .time
                            .map(|t| t.format("%H:%M").to_string())
                            .unwrap_or_else(|| "time".to_string())
                    };
                    format!(
                        "Add {} on {} ({})",
                        request.title,
                        request.date.format("%Y-%m-%d"),
                        time_label
                    )
                }
                Err(_) => "Quick add calendar event".into(),
            };
            return vec![Action {
                label,
                desc: "Calendar".into(),
                action: format!("calendar:add:{input}"),
                args: None,
            }];
        }
        if let Some(snooze) = strip_prefix_ci(rest, "snooze") {
            let input = snooze.trim();
            if input.is_empty() {
                return Vec::new();
            }
            return vec![Action {
                label: format!("Snooze calendar reminder ({input})"),
                desc: "Calendar".into(),
                action: format!("calendar:snooze:{input}"),
                args: None,
            }];
        }
        if let Some(date) = parse_date_reference(rest, chrono::Local::now().naive_local().date()) {
            return vec![Action {
                label: format!("Jump to {}", date.format("%Y-%m-%d")),
                desc: "Calendar".into(),
                action: format!("calendar:jump:{rest}"),
                args: None,
            }];
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "calendar"
    }

    fn description(&self) -> &str {
        "Calendar commands (prefix: `cal`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "cal".into(),
                desc: "Calendar".into(),
                action: "query:cal".into(),
                args: None,
            },
            Action {
                label: "cal day".into(),
                desc: "Calendar".into(),
                action: "query:cal day".into(),
                args: None,
            },
            Action {
                label: "cal week".into(),
                desc: "Calendar".into(),
                action: "query:cal week".into(),
                args: None,
            },
            Action {
                label: "cal month".into(),
                desc: "Calendar".into(),
                action: "query:cal month".into(),
                args: None,
            },
            Action {
                label: "cal today".into(),
                desc: "Calendar".into(),
                action: "query:cal today".into(),
                args: None,
            },
            Action {
                label: "cal tomorrow".into(),
                desc: "Calendar".into(),
                action: "query:cal tomorrow".into(),
                args: None,
            },
            Action {
                label: "cal next mon".into(),
                desc: "Calendar".into(),
                action: "query:cal next mon".into(),
                args: None,
            },
            Action {
                label: "cal add".into(),
                desc: "Calendar".into(),
                action: "query:cal add ".into(),
                args: None,
            },
            Action {
                label: "cal find".into(),
                desc: "Calendar".into(),
                action: "query:cal find ".into(),
                args: None,
            },
            Action {
                label: "cal upcoming".into(),
                desc: "Calendar".into(),
                action: "query:cal upcoming".into(),
                args: None,
            },
            Action {
                label: "cal snooze".into(),
                desc: "Calendar".into(),
                action: "query:cal snooze ".into(),
                args: None,
            },
        ]
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use crate::common::persistence::PersistenceError;
    use std::sync::{Arc, Barrier};

    fn event(id: &str, title: &str) -> CalendarEvent {
        CalendarEvent {
            id: id.into(),
            title: title.into(),
            start: NaiveDate::from_ymd_opt(2026, 9, 6)
                .unwrap()
                .and_hms_opt(9, 30, 0)
                .unwrap(),
            end: Some(
                NaiveDate::from_ymd_opt(2026, 9, 6)
                    .unwrap()
                    .and_hms_opt(10, 0, 0)
                    .unwrap(),
            ),
            duration_minutes: None,
            all_day: false,
            notes: Some("notes".into()),
            recurrence: Some(RecurrenceRule {
                frequency: RecurrenceFrequency::Weekly,
                interval: 2,
                weekly_days: vec![Weekday::Sun],
                nth_weekday: None,
                end: RecurrenceEnd::AfterCount { count: 3 },
                custom_unit: None,
            }),
            reminders: vec![Reminder { minutes_before: 15 }],
            tags: vec!["work".into()],
            category: Some("focus".into()),
            created_at: NaiveDate::from_ymd_opt(2026, 9, 1)
                .unwrap()
                .and_hms_opt(8, 0, 0)
                .unwrap(),
            updated_at: None,
            entity_refs: vec![EntityRef::new(EntityKind::Note, "daily", None)],
        }
    }

    fn set_calendar_data(events: Vec<CalendarEvent>) -> Vec<CalendarEvent> {
        let mut current = CALENDAR_DATA
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::replace(&mut *current, events)
    }

    #[test]
    fn event_persistence_states_parent_creation_and_schema_are_compatible() {
        let _test = calendar_test_guard();
        let original = set_calendar_data(Vec::new());
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert_eq!(load_events_typed(&missing).unwrap(), LoadState::Missing);

        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, " \r\n").unwrap();
        assert_eq!(load_events_typed(&empty).unwrap(), LoadState::Empty);

        let nested = directory.path().join("calendar").join("events.json");
        let expected = vec![event("evt-compatible", "Compatible")];
        save_events(nested.to_str().unwrap(), &expected).unwrap();
        assert_eq!(
            load_events_typed(&nested).unwrap(),
            LoadState::Loaded(expected.clone())
        );
        assert_eq!(load_events(nested.to_str().unwrap()).unwrap(), expected);
        let persisted = std::fs::read_to_string(nested).unwrap();
        assert_eq!(persisted, serde_json::to_string_pretty(&expected).unwrap());
        assert!(persisted.contains("\"Weekly\""));
        assert!(persisted.contains("2026-09-06T09:30:00"));

        restore_calendar_test_data(original);
    }

    #[test]
    fn malformed_and_unreadable_events_reject_all_mutation_families_unchanged() {
        let _test = calendar_test_guard();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.json");
        let invalid = b"invalid calendar events";
        std::fs::write(&path, invalid).unwrap();
        let path = path.to_str().unwrap();
        assert!(save_events(path, &[event("replacement", "Replacement")]).is_err());
        assert!(
            add_event_at(
                path,
                CalendarAddRequest {
                    date: NaiveDate::from_ymd_opt(2026, 9, 6).unwrap(),
                    time: NaiveTime::from_hms_opt(9, 0, 0),
                    all_day: false,
                    title: "Add".into(),
                    notes: None,
                    refs: Vec::new(),
                },
                NaiveDate::from_ymd_opt(2026, 9, 1)
                    .unwrap()
                    .and_hms_opt(8, 0, 0)
                    .unwrap(),
            )
            .is_err()
        );
        assert!(snooze_event_at(path, "event", Duration::minutes(5)).is_err());
        for _family in ["edit", "delete", "duplicate", "split"] {
            assert!(
                update_events(path, |events| {
                    events.clear();
                    Ok(true)
                })
                .is_err()
            );
        }
        assert_eq!(std::fs::read(path).unwrap(), invalid);
        assert!(matches!(
            load_events_typed(directory.path()).unwrap_err(),
            PersistenceError::Read { .. }
        ));
        assert!(save_events(directory.path().to_str().unwrap(), &[]).is_err());
    }

    #[test]
    fn failed_event_save_retains_disk_data_index_and_version() {
        let _test = calendar_test_guard();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.json");
        let disk = vec![event("disk", "Disk")];
        std::fs::write(&path, serde_json::to_vec_pretty(&disk).unwrap()).unwrap();
        let memory = vec![event("memory", "Memory")];
        let original = set_calendar_data(memory.clone());
        *CALENDAR_INDEX.write().unwrap() = CalendarIndexState {
            version: 777,
            index: CalendarIndex {
                titles: vec![("sentinel".into(), "sentinel".into())],
                tags: HashMap::new(),
            },
        };
        let version = calendar_version();
        let result = update_events_with_save(
            path.to_str().unwrap(),
            |events| {
                events.push(event("lost", "Lost"));
                Ok(true)
            },
            |_path, _events| anyhow::bail!("deterministic calendar save failure"),
        );
        assert!(result.is_err());
        assert_eq!(load_events(path.to_str().unwrap()).unwrap(), disk);
        assert_eq!(*CALENDAR_DATA.read().unwrap(), memory);
        assert_eq!(calendar_version(), version);
        let index = CALENDAR_INDEX.read().unwrap();
        assert_eq!(index.version, 777);
        assert_eq!(index.index.titles[0].0, "sentinel");
        drop(index);
        restore_calendar_test_data(original);
    }

    #[test]
    fn missing_first_event_and_concurrent_additions_are_retained() {
        let _test = calendar_test_guard();
        let original = set_calendar_data(Vec::new());
        let directory = tempfile::tempdir().unwrap();
        let path = Arc::new(
            directory
                .path()
                .join("nested")
                .join("events.json")
                .to_string_lossy()
                .into_owned(),
        );
        let barrier = Arc::new(Barrier::new(3));
        let handles = ["First", "Second"].map(|title| {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                add_event_at(
                    &path,
                    CalendarAddRequest {
                        date: NaiveDate::from_ymd_opt(2026, 9, 6).unwrap(),
                        time: NaiveTime::from_hms_opt(9, 0, 0),
                        all_day: false,
                        title: title.into(),
                        notes: None,
                        refs: Vec::new(),
                    },
                    NaiveDate::from_ymd_opt(2026, 9, 1)
                        .unwrap()
                        .and_hms_opt(8, 0, 0)
                        .unwrap(),
                )
                .unwrap();
            })
        });
        barrier.wait();
        for handle in handles {
            handle.join().unwrap();
        }
        let events = load_events(&path).unwrap();
        assert_eq!(events.len(), 2);
        assert!(events.iter().any(|event| event.title == "First"));
        assert!(events.iter().any(|event| event.title == "Second"));
        restore_calendar_test_data(original);
    }

    #[test]
    fn watcher_reload_retains_invalid_recovers_and_deduplicates_self_write() {
        let _test = calendar_test_guard();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.json");
        let original = set_calendar_data(Vec::new());
        let local = vec![event("local", "Local")];
        save_events(path.to_str().unwrap(), &local).unwrap();
        let version = calendar_version();
        refresh_events_from_disk(path.to_str().unwrap()).unwrap();
        assert_eq!(calendar_version(), version);

        let invalid = b"invalid events";
        std::fs::write(&path, invalid).unwrap();
        assert!(refresh_events_from_disk(path.to_str().unwrap()).is_err());
        assert_eq!(*CALENDAR_DATA.read().unwrap(), local);
        assert_eq!(calendar_version(), version);
        assert_eq!(std::fs::read(&path).unwrap(), invalid);

        std::fs::remove_file(&path).unwrap();
        assert!(refresh_events_from_disk(path.to_str().unwrap()).is_err());
        assert_eq!(*CALENDAR_DATA.read().unwrap(), local);
        assert_eq!(calendar_version(), version);

        let external = vec![event("external", "External")];
        std::fs::write(&path, serde_json::to_vec_pretty(&external).unwrap()).unwrap();
        refresh_events_from_disk(path.to_str().unwrap()).unwrap();
        assert_eq!(*CALENDAR_DATA.read().unwrap(), external);
        assert_eq!(calendar_version(), version + 1);
        assert_eq!(search_by_title("external"), external);
        restore_calendar_test_data(original);
    }
}
