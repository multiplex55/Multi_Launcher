//! Bounded Inbox state. Producers supply facts, not arbitrary internal text.
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

const MAX_CLEARED: usize = 10_000;
const MAX_EXTERNAL_TEXT: usize = 2048;
const MAX_APP_ID: usize = 512;
const DAY: i64 = 86_400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Source {
    Launcher,
    Timer,
    Macro,
    Activity,
    Calendar,
    HotFolder,
    Transfer,
    Windows,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum FactId {
    Launcher {
        session: [u8; 16],
        sequence: u64,
    },
    Timer {
        session: [u8; 16],
        id: u64,
        occurrence: u64,
    },
    Macro {
        session: [u8; 16],
        id: u64,
        run: u64,
        occurrence: u64,
    },
    Activity {
        session: [u8; 16],
        id: u64,
        generation: u64,
    },
    Calendar {
        calendar: [u8; 16],
        event: String,
        occurrence: i64,
        reminder_offset: i64,
        revision: u64,
    },
    HotFolder {
        rule: String,
        event: [u8; 16],
    },
    Transfer {
        receipt: [u8; 16],
        event: u64,
    },
    Windows {
        app: String,
        notification: u32,
        created: i64,
    },
}
impl FactId {
    pub(crate) fn source(&self) -> Source {
        match self {
            Self::Launcher { .. } => Source::Launcher,
            Self::Timer { .. } => Source::Timer,
            Self::Macro { .. } => Source::Macro,
            Self::Activity { .. } => Source::Activity,
            Self::Calendar { .. } => Source::Calendar,
            Self::HotFolder { .. } => Source::HotFolder,
            Self::Transfer { .. } => Source::Transfer,
            Self::Windows { .. } => Source::Windows,
        }
    }
    fn valid(&self) -> bool {
        match self {
            Self::Launcher { session, .. }
            | Self::Timer { session, .. }
            | Self::Macro { session, .. }
            | Self::Activity { session, .. } => *session != [0; 16],
            Self::Calendar {
                calendar, event, ..
            } => *calendar != [0; 16] && valid_owner_id(event),
            Self::HotFolder { rule, event } => {
                crate::hotfolder::RuleId::parse(rule.clone()).is_ok() && *event != [0; 16]
            }
            Self::Transfer { receipt, .. } => *receipt != [0; 16],
            Self::Windows { app, .. } => {
                !app.is_empty() && app.len() <= MAX_APP_ID && !app.chars().any(char::is_control)
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Category {
    Reminder,
    Completed,
    Failed,
    Warning,
    Information,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Severity {
    Info,
    Warning,
    Error,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Status {
    Current,
    Completed,
    Partial,
    Failed,
    Cancelled,
    SourceRemoved,
}
/// Allowlisted, static summaries: no paths, event titles, macro contents, or errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum InternalFact {
    LauncherInfo,
    LauncherWarning,
    LauncherError,
    TimerFinished,
    MacroNotificationSubmitted,
    MacroCompleted,
    MacroFailed,
    ActivityCompleted,
    ActivityFailed,
    CalendarDue,
    HotFolderCompleted,
    HotFolderFailed,
    TransferCompleted,
    TransferPartial,
    TransferFailed,
}
impl InternalFact {
    fn source(self) -> Source {
        match self {
            Self::LauncherInfo | Self::LauncherWarning | Self::LauncherError => Source::Launcher,
            Self::TimerFinished => Source::Timer,
            Self::MacroNotificationSubmitted | Self::MacroCompleted | Self::MacroFailed => {
                Source::Macro
            }
            Self::ActivityCompleted | Self::ActivityFailed => Source::Activity,
            Self::CalendarDue => Source::Calendar,
            Self::HotFolderCompleted | Self::HotFolderFailed => Source::HotFolder,
            Self::TransferCompleted | Self::TransferPartial | Self::TransferFailed => {
                Source::Transfer
            }
        }
    }
    pub(crate) fn summary(self) -> &'static str {
        match self {
            Self::LauncherInfo => "Launcher notification",
            Self::LauncherWarning => "Launcher warning",
            Self::LauncherError => "Launcher error",
            Self::TimerFinished => "Timer finished",
            Self::MacroNotificationSubmitted => "Macro notification submitted",
            Self::MacroCompleted => "Macro completed",
            Self::MacroFailed => "Macro failed",
            Self::ActivityCompleted => "Activity completed",
            Self::ActivityFailed => "Activity failed",
            Self::CalendarDue => "Calendar reminder",
            Self::HotFolderCompleted => "Hot folder task completed",
            Self::HotFolderFailed => "Hot folder task failed",
            Self::TransferCompleted => "Transfer completed",
            Self::TransferPartial => "Transfer partially completed",
            Self::TransferFailed => "Transfer failed",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RelatedItem {
    Timer {
        session: [u8; 16],
        id: u64,
    },
    Macro {
        session: [u8; 16],
        id: u64,
    },
    Activity {
        session: [u8; 16],
        id: u64,
    },
    Calendar {
        calendar: [u8; 16],
        event: String,
        occurrence: i64,
    },
    HotFolder {
        rule: String,
    },
    Transfer {
        receipt: [u8; 16],
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Content {
    Internal(InternalFact),
    External {
        generation: u64,
        title: Option<String>,
        body: Option<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub id: FactId,
    pub occurred_at: i64,
    pub updated_at: i64,
    pub category: Category,
    pub severity: Severity,
    pub status: Status,
    pub read: bool,
    pub pinned: bool,
    pub content: Content,
    pub related: Option<RelatedItem>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    pub retention_days: u16,
    pub max_records: u16,
    pub external_enabled: bool,
    pub persist_external: bool,
    pub retain_external_text: bool,
    pub muted: HashSet<Source>,
    pub excluded: HashSet<Source>,
    pub muted_apps: HashSet<String>,
    pub excluded_apps: HashSet<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            retention_days: 7,
            max_records: 1000,
            external_enabled: false,
            persist_external: false,
            retain_external_text: false,
            muted: HashSet::new(),
            excluded: HashSet::new(),
            muted_apps: HashSet::new(),
            excluded_apps: HashSet::new(),
        }
    }
}
impl Settings {
    pub(crate) fn valid(&self) -> bool {
        (1..=90).contains(&self.retention_days)
            && (1..=5000).contains(&self.max_records)
            && self.muted_apps.len() <= 256
            && self.excluded_apps.len() <= 256
            && self
                .muted_apps
                .iter()
                .chain(&self.excluded_apps)
                .all(|app| {
                    !app.is_empty() && app.len() <= MAX_APP_ID && !app.chars().any(char::is_control)
                })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Inbox {
    settings: Settings,
    records: Vec<Record>,
    cleared: VecDeque<FactId>,
    #[serde(skip)]
    action: Option<ActionTicket>,
    #[serde(skip)]
    next_action: u64,
    #[serde(skip)]
    omitted: u64,
    #[serde(skip)]
    consumed_actions: HashSet<(FactId, ActionKind)>,
}
impl Default for Inbox {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            records: Vec::new(),
            cleared: VecDeque::new(),
            action: None,
            next_action: 0,
            omitted: 0,
            consumed_actions: HashSet::new(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ingest {
    Added { present: bool },
    Updated { present: bool },
    Duplicate,
    Excluded,
    Cleared,
    Capacity,
    Expired,
    Invalid,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsError {
    Invalid,
    PinnedExceedCapacity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ActionKind {
    OpenRelated,
    Snooze,
    CreateTodo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActionTicket {
    serial: u64,
    pub id: FactId,
    pub kind: ActionKind,
    pub related: Option<RelatedItem>,
    updated_at: i64,
    status: Status,
    external_generation: Option<u64>,
}
impl Inbox {
    pub(crate) fn settings(&self) -> &Settings {
        &self.settings
    }
    pub(crate) fn records(&self) -> &[Record] {
        &self.records
    }
    pub(crate) fn omitted(&self) -> u64 {
        self.omitted
    }
    pub(crate) fn chronological(&self) -> Vec<&Record> {
        let mut rows: Vec<_> = self.records.iter().collect();
        rows.sort_by_key(|record| (record.occurred_at, record.updated_at));
        rows
    }
    /// Validate decoded storage before adopting it as live state.
    pub(crate) fn valid_persistent_snapshot(&self) -> bool {
        if !self.settings.valid()
            || self.records.len() > self.settings.max_records as usize
            || self.cleared.len() > MAX_CLEARED
        {
            return false;
        }
        let mut cleared = HashSet::new();
        if !self.cleared.iter().all(|id| {
            id.valid()
                && cleared.insert(id)
                && (id.source() != Source::Windows || self.settings.persist_external)
        }) {
            return false;
        }
        let mut ids = HashSet::new();
        for record in &self.records {
            if !record.id.valid() || !ids.insert(record.id.clone()) || cleared.contains(&record.id)
            {
                return false;
            }
            if record
                .related
                .as_ref()
                .is_some_and(|related| !related_matches(&record.id, related))
            {
                return false;
            }
            match &record.content {
                Content::Internal(fact)
                    if record.id.source() != Source::Windows
                        && fact.source() == record.id.source() => {}
                Content::External { title, body, .. }
                    if record.id.source() == Source::Windows && self.settings.persist_external =>
                {
                    if [title.as_deref(), body.as_deref()]
                        .into_iter()
                        .flatten()
                        .any(|s| s.len() > MAX_EXTERNAL_TEXT)
                        || (!self.settings.retain_external_text
                            && (title.is_some() || body.is_some()))
                    {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }
    pub(crate) fn set_settings(
        &mut self,
        settings: Settings,
        now: i64,
    ) -> Result<(), SettingsError> {
        if !settings.valid() {
            return Err(SettingsError::Invalid);
        }
        if self.records.iter().filter(|r| r.pinned).count() > settings.max_records as usize {
            return Err(SettingsError::PinnedExceedCapacity);
        }
        if !settings.retain_external_text {
            for record in &mut self.records {
                if let Content::External { title, body, .. } = &mut record.content {
                    *title = None;
                    *body = None;
                }
            }
        }
        self.settings = settings;
        self.prune(now);
        Ok(())
    }
    pub(crate) fn ingest_internal(
        &mut self,
        id: FactId,
        occurred_at: i64,
        category: Category,
        status: Status,
        fact: InternalFact,
        related: Option<RelatedItem>,
        now: i64,
    ) -> Ingest {
        if !id.valid()
            || id.source() != fact.source()
            || matches!(id, FactId::Windows { .. })
            || related.as_ref().is_some_and(|r| !related_matches(&id, r))
        {
            return Ingest::Invalid;
        }
        self.ingest(
            Record {
                id,
                occurred_at,
                updated_at: now,
                category,
                severity: match fact {
                    InternalFact::LauncherError
                    | InternalFact::MacroFailed
                    | InternalFact::ActivityFailed
                    | InternalFact::HotFolderFailed
                    | InternalFact::TransferFailed => Severity::Error,
                    InternalFact::LauncherWarning | InternalFact::TransferPartial => {
                        Severity::Warning
                    }
                    _ => severity(category, status),
                },
                status,
                read: false,
                pinned: false,
                content: Content::Internal(fact),
                related,
            },
            now,
        )
    }
    pub(crate) fn ingest_windows(
        &mut self,
        id: FactId,
        generation: u64,
        occurred_at: i64,
        category: Category,
        status: Status,
        title: Option<&str>,
        body: Option<&str>,
        now: i64,
    ) -> Ingest {
        if !id.valid() || !matches!(id, FactId::Windows { .. }) {
            return Ingest::Invalid;
        }
        if !self.settings.external_enabled {
            return Ingest::Excluded;
        }
        if [title, body]
            .into_iter()
            .flatten()
            .any(|s| s.len() > MAX_EXTERNAL_TEXT)
        {
            return Ingest::Invalid;
        }
        let retain = self.settings.retain_external_text;
        self.ingest(
            Record {
                id,
                occurred_at,
                updated_at: now,
                category,
                severity: severity(category, status),
                status,
                read: false,
                pinned: false,
                content: Content::External {
                    generation,
                    title: retain.then(|| title.map(str::to_owned)).flatten(),
                    body: retain.then(|| body.map(str::to_owned)).flatten(),
                },
                related: None,
            },
            now,
        )
    }
    fn ingest(&mut self, mut incoming: Record, now: i64) -> Ingest {
        let source = incoming.id.source();
        if self.settings.excluded.contains(&source)
            || matches!(&incoming.id, FactId::Windows { app, .. } if self.settings.excluded_apps.contains(app))
        {
            return Ingest::Excluded;
        }
        if incoming.occurred_at < now.saturating_sub(i64::from(self.settings.retention_days) * DAY)
            && !self
                .records
                .iter()
                .any(|record| record.id == incoming.id && record.pinned)
        {
            return Ingest::Expired;
        }
        if self.cleared.contains(&incoming.id) {
            return Ingest::Cleared;
        }
        self.prune(now);
        let present = !self.settings.muted.contains(&source)
            && !matches!(&incoming.id, FactId::Windows { app, .. } if self.settings.muted_apps.contains(app));
        if let Some(existing) = self.records.iter_mut().find(|r| r.id == incoming.id) {
            // The source generation orders Windows updates even when the
            // system wall clock moves backward. Internal facts are immutable.
            if !matches!((&existing.content, &incoming.content),
                (Content::External { generation: old, .. }, Content::External { generation: new, .. }) if new > old)
            {
                return Ingest::Duplicate;
            }
            incoming.read = existing.read;
            incoming.pinned = existing.pinned;
            *existing = incoming;
            return Ingest::Updated { present };
        }
        if self.records.len() >= self.settings.max_records as usize {
            self.omitted = self.omitted.saturating_add(1);
            if let Some((index, _)) = self
                .records
                .iter()
                .enumerate()
                .filter(|(_, row)| !row.pinned && row.occurred_at <= incoming.occurred_at)
                .min_by_key(|(_, row)| row.occurred_at)
            {
                let retired = self.records.remove(index).id;
                self.remember_clear(retired);
            } else {
                return Ingest::Capacity;
            }
        }
        self.records.push(incoming);
        Ingest::Added { present }
    }
    pub(crate) fn prune(&mut self, now: i64) {
        let cutoff = now.saturating_sub(i64::from(self.settings.retention_days) * DAY);
        self.records.retain(|r| r.pinned || r.occurred_at >= cutoff);
        while self.records.len() > self.settings.max_records as usize {
            let Some((index, _)) = self
                .records
                .iter()
                .enumerate()
                .filter(|(_, r)| !r.pinned)
                .min_by_key(|(_, r)| r.occurred_at)
            else {
                break;
            };
            let retired = self.records.remove(index).id;
            self.remember_clear(retired);
            self.omitted = self.omitted.saturating_add(1);
        }
        self.consumed_actions
            .retain(|(id, _)| self.records.iter().any(|record| &record.id == id));
        if self
            .action
            .as_ref()
            .is_some_and(|ticket| !self.records.iter().any(|record| record.id == ticket.id))
        {
            self.action = None;
        }
    }
    pub(crate) fn set_read(&mut self, id: &FactId, read: bool) -> bool {
        if let Some(r) = self.records.iter_mut().find(|r| &r.id == id) {
            r.read = read;
            true
        } else {
            false
        }
    }
    pub(crate) fn set_pinned(&mut self, id: &FactId, pinned: bool) -> bool {
        if let Some(r) = self.records.iter_mut().find(|r| &r.id == id) {
            r.pinned = pinned;
            true
        } else {
            false
        }
    }
    pub(crate) fn mark_source_removed(&mut self, id: &FactId, now: i64) -> bool {
        if id.source() != Source::Windows {
            return false;
        }
        if let Some(r) = self
            .records
            .iter_mut()
            .find(|r| &r.id == id && r.status != Status::SourceRemoved)
        {
            r.status = Status::SourceRemoved;
            r.updated_at = now;
            true
        } else {
            false
        }
    }
    pub(crate) fn dismiss(&mut self, id: &FactId) -> bool {
        if let Some(i) = self.records.iter().position(|r| &r.id == id) {
            let id = self.records.remove(i).id;
            self.consumed_actions.retain(|(entry, _)| *entry != id);
            if self.action.as_ref().is_some_and(|ticket| ticket.id == id) {
                self.action = None;
            }
            self.remember_clear(id);
            true
        } else {
            false
        }
    }
    pub(crate) fn clear_source(&mut self, source: Source) {
        let ids: Vec<_> = self
            .records
            .iter()
            .filter(|r| r.id.source() == source)
            .map(|r| r.id.clone())
            .collect();
        for id in ids {
            self.dismiss(&id);
        }
    }
    pub(crate) fn clear_windows_app(&mut self, app: &str) {
        let ids: Vec<_> = self
            .records
            .iter()
            .filter(|row| matches!(&row.id, FactId::Windows {app: sender, ..} if sender == app))
            .map(|row| row.id.clone())
            .collect();
        for id in ids {
            self.dismiss(&id);
        }
    }

    pub(crate) fn clear_all(&mut self) {
        let ids: Vec<_> = self.records.iter().map(|r| r.id.clone()).collect();
        for id in ids {
            self.dismiss(&id);
        }
    }
    fn remember_clear(&mut self, id: FactId) {
        if self.cleared.len() >= MAX_CLEARED {
            self.cleared.pop_front();
        }
        self.cleared.push_back(id);
    }
    pub(crate) fn begin_action(&mut self, id: &FactId, kind: ActionKind) -> Option<ActionTicket> {
        if self.action.is_some() || self.consumed_actions.contains(&(id.clone(), kind)) {
            return None;
        }
        let entry = self.records.iter().find(|record| &record.id == id)?;
        let available = match kind {
            ActionKind::OpenRelated => {
                entry.related.is_some() && entry.status != Status::SourceRemoved
            }
            ActionKind::Snooze => {
                matches!(
                    &entry.related,
                    Some(RelatedItem::Timer { .. } | RelatedItem::Calendar { .. })
                ) && entry.status != Status::SourceRemoved
            }
            ActionKind::CreateTodo => true,
        };
        if !available {
            return None;
        }
        self.next_action = self.next_action.checked_add(1)?;
        let ticket = ActionTicket {
            serial: self.next_action,
            id: id.clone(),
            kind,
            related: entry.related.clone(),
            updated_at: entry.updated_at,
            status: entry.status,
            external_generation: external_generation(&entry.content),
        };
        self.action = Some(ticket.clone());
        Some(ticket)
    }

    pub(crate) fn action_is_current(&self, ticket: &ActionTicket) -> bool {
        self.action.as_ref() == Some(ticket)
            && self.records.iter().any(|entry| {
                entry.id == ticket.id
                    && entry.updated_at == ticket.updated_at
                    && entry.status == ticket.status
                    && external_generation(&entry.content) == ticket.external_generation
                    && entry.related == ticket.related
            })
    }

    pub(crate) fn finish_action(&mut self, ticket: &ActionTicket, succeeded: bool) -> bool {
        if self.action.as_ref() != Some(ticket) {
            return false;
        }
        let current = self.action_is_current(ticket);
        self.action = None;
        if succeeded && current && ticket.kind != ActionKind::OpenRelated {
            self.consumed_actions
                .insert((ticket.id.clone(), ticket.kind));
        }
        true
    }
    /// Durable projection excludes external data unless explicitly opted in.
    pub(crate) fn persistent_snapshot(&self) -> Self {
        let mut copy = self.clone();
        if !copy.settings.persist_external {
            copy.records.retain(|r| r.id.source() != Source::Windows);
            copy.cleared.retain(|id| id.source() != Source::Windows);
        }
        copy.action = None;
        copy.next_action = 0;
        copy.omitted = 0;
        copy.consumed_actions.clear();
        copy
    }
}
fn external_generation(content: &Content) -> Option<u64> {
    match content {
        Content::Internal(_) => None,
        Content::External { generation, .. } => Some(*generation),
    }
}

fn valid_owner_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn severity(category: Category, status: Status) -> Severity {
    if matches!(status, Status::Failed) || matches!(category, Category::Failed) {
        Severity::Error
    } else if matches!(status, Status::Partial) || matches!(category, Category::Warning) {
        Severity::Warning
    } else {
        Severity::Info
    }
}
fn related_matches(id: &FactId, related: &RelatedItem) -> bool {
    match (id, related) {
        (FactId::Timer { session, id, .. }, RelatedItem::Timer { session: s, id: i })
        | (FactId::Macro { session, id, .. }, RelatedItem::Macro { session: s, id: i })
        | (FactId::Activity { session, id, .. }, RelatedItem::Activity { session: s, id: i }) => {
            session == s && id == i
        }
        (
            FactId::Calendar {
                calendar,
                event,
                occurrence,
                ..
            },
            RelatedItem::Calendar {
                calendar: c,
                event: e,
                occurrence: o,
            },
        ) => calendar == c && event == e && occurrence == o,
        (FactId::HotFolder { rule, .. }, RelatedItem::HotFolder { rule: r }) => rule == r,
        (FactId::Transfer { receipt, .. }, RelatedItem::Transfer { receipt: r }) => receipt == r,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn timer(n: u64) -> FactId {
        FactId::Timer {
            session: [1; 16],
            id: n,
            occurrence: 1,
        }
    }
    fn add(inbox: &mut Inbox, id: FactId, now: i64) -> Ingest {
        inbox.ingest_internal(
            id,
            now,
            Category::Completed,
            Status::Completed,
            InternalFact::TimerFinished,
            None,
            now,
        )
    }
    #[test]
    fn notification_inbox_distinct_occurrences_and_external_updates() {
        let mut inbox = Inbox::default();
        let a = FactId::Macro {
            session: [1; 16],
            id: 2,
            run: 3,
            occurrence: 0,
        };
        let b = FactId::Macro {
            session: [1; 16],
            id: 2,
            run: 3,
            occurrence: 1,
        };
        for id in [a, b] {
            assert!(matches!(
                inbox.ingest_internal(
                    id,
                    1,
                    Category::Completed,
                    Status::Completed,
                    InternalFact::MacroCompleted,
                    None,
                    1
                ),
                Ingest::Added { .. }
            ));
        }
        let mut settings = inbox.settings().clone();
        settings.external_enabled = true;
        settings.retain_external_text = true;
        settings.muted_apps.insert("app".into());
        inbox.set_settings(settings, 1).unwrap();
        let id = FactId::Windows {
            app: "app".into(),
            notification: 4,
            created: 5,
        };
        assert_eq!(
            inbox.ingest_windows(
                id.clone(),
                1,
                1,
                Category::Information,
                Status::Current,
                Some("first"),
                None,
                1
            ),
            Ingest::Added { present: false }
        );
        assert_eq!(
            inbox.ingest_windows(
                id.clone(),
                2,
                1,
                Category::Information,
                Status::Current,
                Some("second"),
                None,
                2
            ),
            Ingest::Updated { present: false }
        );
        assert_eq!(
            inbox.ingest_windows(
                id,
                1,
                1,
                Category::Information,
                Status::Current,
                Some("old"),
                None,
                3
            ),
            Ingest::Duplicate
        );
    }
    #[test]
    fn notification_inbox_persistent_projection_and_action_are_bounded() {
        let mut inbox = Inbox::default();
        let id = timer(9);
        let related = RelatedItem::Timer {
            session: [1; 16],
            id: 9,
        };
        assert!(matches!(
            inbox.ingest_internal(
                id.clone(),
                1,
                Category::Completed,
                Status::Completed,
                InternalFact::TimerFinished,
                Some(related),
                1
            ),
            Ingest::Added { .. }
        ));
        let ticket = inbox.begin_action(&id, ActionKind::Snooze).unwrap();
        assert!(inbox.begin_action(&id, ActionKind::CreateTodo).is_none());
        assert!(inbox.action_is_current(&ticket));
        assert!(inbox.finish_action(&ticket, true));
        assert!(inbox.begin_action(&id, ActionKind::Snooze).is_none());
        assert!(inbox.begin_action(&id, ActionKind::OpenRelated).is_some());
        let serialized = serde_json::to_vec(&inbox.persistent_snapshot()).unwrap();
        let decoded: Inbox = serde_json::from_slice(&serialized).unwrap();
        assert!(decoded.valid_persistent_snapshot());
    }
    #[test]
    fn notification_inbox_dedup_mute_and_clear() {
        let mut inbox = Inbox::default();
        let mut settings = inbox.settings().clone();
        settings.muted.insert(Source::Timer);
        inbox.set_settings(settings, 1).unwrap();
        assert_eq!(
            add(&mut inbox, timer(1), 1),
            Ingest::Added { present: false }
        );
        assert_eq!(add(&mut inbox, timer(1), 1), Ingest::Duplicate);
        inbox.clear_source(Source::Timer);
        assert_eq!(add(&mut inbox, timer(1), 1), Ingest::Cleared);
        let mut settings = inbox.settings().clone();
        settings.excluded.insert(Source::Timer);
        inbox.set_settings(settings, 1).unwrap();
        assert_eq!(add(&mut inbox, timer(2), 1), Ingest::Excluded);
    }
    #[test]
    fn notification_inbox_external_action_checks_generation_despite_wall_clock_changes() {
        let mut inbox = Inbox::default();
        let mut settings = inbox.settings().clone();
        settings.external_enabled = true;
        inbox.set_settings(settings, 100).unwrap();
        let id = FactId::Windows {
            app: "Synthetic.Test".into(),
            notification: 1,
            created: 123,
        };
        let emit = |inbox: &mut Inbox, generation, now| {
            inbox.ingest_windows(
                id.clone(),
                generation,
                10,
                Category::Information,
                Status::Current,
                None,
                None,
                now,
            )
        };
        assert!(matches!(emit(&mut inbox, 1, 100), Ingest::Added { .. }));
        let ticket = inbox.begin_action(&id, ActionKind::CreateTodo).unwrap();
        assert!(matches!(emit(&mut inbox, 2, 100), Ingest::Updated { .. }));
        assert!(!inbox.action_is_current(&ticket));
        assert!(inbox.finish_action(&ticket, false));
        assert!(matches!(emit(&mut inbox, 3, 99), Ingest::Updated { .. }));
        assert_eq!(emit(&mut inbox, 2, 101), Ingest::Duplicate);
        assert_eq!(external_generation(&inbox.records()[0].content), Some(3));
    }

    #[test]
    fn notification_inbox_discloses_evictions_and_rejects_replayed_capacity_drops() {
        let mut inbox = Inbox::default();
        let mut settings = inbox.settings().clone();
        settings.max_records = 2;
        inbox.set_settings(settings.clone(), 1).unwrap();
        let emit = |inbox: &mut Inbox, sequence, now| {
            inbox.ingest_internal(
                FactId::Launcher {
                    session: [7; 16],
                    sequence,
                },
                now,
                Category::Information,
                Status::Current,
                InternalFact::LauncherInfo,
                None,
                now,
            )
        };
        emit(&mut inbox, 1, 1);
        emit(&mut inbox, 2, 2);
        assert!(matches!(emit(&mut inbox, 3, 3), Ingest::Added { .. }));
        assert_eq!(inbox.omitted(), 1);
        assert_eq!(emit(&mut inbox, 1, 3), Ingest::Cleared);
        settings.max_records = 1;
        inbox.set_settings(settings, 3).unwrap();
        assert_eq!(inbox.omitted(), 2);
        assert_eq!(inbox.records().len(), 1);
    }

    #[test]
    fn notification_inbox_pins_expiry_and_capacity() {
        let mut inbox = Inbox::default();
        let mut settings = inbox.settings().clone();
        settings.max_records = 1;
        inbox.set_settings(settings, 0).unwrap();
        assert!(matches!(add(&mut inbox, timer(1), 0), Ingest::Added { .. }));
        inbox.set_pinned(&timer(1), true);
        inbox.prune(10 * DAY);
        assert_eq!(add(&mut inbox, timer(2), 10 * DAY), Ingest::Capacity);
        assert_eq!(inbox.omitted(), 1);
        assert_eq!(inbox.records().len(), 1);
    }
    #[test]
    fn notification_inbox_external_text_and_privacy() {
        let mut inbox = Inbox::default();
        let id = FactId::Windows {
            app: "app".into(),
            notification: 1,
            created: 1,
        };
        assert_eq!(
            inbox.ingest_windows(
                id.clone(),
                1,
                1,
                Category::Information,
                Status::Current,
                Some("secret"),
                None,
                1
            ),
            Ingest::Excluded
        );
        let mut settings = inbox.settings().clone();
        settings.external_enabled = true;
        settings.retain_external_text = true;
        inbox.set_settings(settings.clone(), 1).unwrap();
        assert!(matches!(
            inbox.ingest_windows(
                id,
                1,
                1,
                Category::Information,
                Status::Current,
                Some("secret"),
                None,
                1
            ),
            Ingest::Added { .. }
        ));
        assert!(inbox.persistent_snapshot().records().is_empty());
        settings.retain_external_text = false;
        inbox.set_settings(settings, 1).unwrap();
        assert!(matches!(
            &inbox.records()[0].content,
            Content::External {
                title: None,
                body: None,
                ..
            }
        ));
        assert_eq!(
            inbox.ingest_internal(
                timer(1),
                1,
                Category::Information,
                Status::Current,
                InternalFact::MacroFailed,
                None,
                1
            ),
            Ingest::Invalid
        );
    }
}
