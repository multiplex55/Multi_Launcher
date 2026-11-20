//! Converts a bounded, permission-checked Windows snapshot into local Inbox changes.
use super::{Category, Content, FactId, Inbox, Ingest, Settings, Source, Status};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub(super) const MAX_TOASTS: u32 = 512;
pub(super) const MAX_TEXT_BYTES: usize = 2048;
pub(super) const MAX_TEXT_PARTS: u32 = 8;
const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;
const TICKS_PER_SECOND: i64 = 10_000_000;

pub(super) struct ObservedToast {
    pub id: FactId,
    pub occurred_at: i64,
    pub title: Option<String>,
    pub body: Option<String>,
}

impl ObservedToast {
    fn text_version(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        for text in [&self.title, &self.body] {
            if let Some(text) = text {
                hash.update((text.len() as u64).to_le_bytes());
                hash.update(text.as_bytes());
            } else {
                hash.update(u64::MAX.to_le_bytes());
            }
        }
        hash.finalize().into()
    }
}

pub(super) struct ToastObservation {
    pub toasts: Vec<ObservedToast>,
    // Only a complete read may confirm that a previous notification is gone.
    pub complete: bool,
}

#[derive(Clone, Default)]
pub(super) struct ToastProjection {
    seen: HashMap<FactId, [u8; 32]>,
    settings: Option<Settings>,
}

impl ToastProjection {
    pub(super) fn apply(
        &mut self,
        inbox: &mut Inbox,
        observation: ToastObservation,
        now: i64,
        includes: impl Fn(&str) -> bool,
    ) -> bool {
        let settings = inbox.settings();
        if !settings.external_enabled || settings.excluded.contains(&Source::Windows) {
            self.seen.clear();
            self.settings = None;
            return false;
        }
        if self.settings.as_ref() != Some(settings) {
            self.seen.clear();
            self.settings = Some(settings.clone());
        }
        let included = |id: &FactId| match id {
            FactId::Windows { app, .. } => includes(app) && !settings.excluded_apps.contains(app),
            _ => false,
        };
        let missing: Vec<_> = if observation.complete {
            let current: HashSet<_> = observation.toasts.iter().map(|row| &row.id).collect();
            inbox
                .records()
                .iter()
                .filter(|record| {
                    record.status == Status::Current
                        && included(&record.id)
                        && !current.contains(&record.id)
                })
                .map(|record| record.id.clone())
                .collect()
        } else {
            Vec::new()
        };
        let mut changed = false;
        for id in missing {
            changed |= inbox.mark_source_removed(&id, now);
        }

        let mut seen = HashMap::with_capacity(observation.toasts.len());
        for row in observation.toasts {
            let version = row.text_version();
            if seen.insert(row.id.clone(), version).is_some() {
                continue;
            }
            let current = inbox.records().iter().find(|record| record.id == row.id);
            let generation = match current.map(|record| (&record.content, record.status)) {
                Some((
                    Content::External {
                        generation,
                        title,
                        body,
                        ..
                    },
                    status,
                )) => {
                    if status == Status::Current && *title == row.title && *body == row.body {
                        continue;
                    }
                    let Some(next) = generation.checked_add(1) else {
                        continue;
                    };
                    next
                }
                None if self.seen.get(&row.id) == Some(&version) => continue,
                None => 1,
                _ => continue,
            };
            let result = inbox.ingest_windows(
                row.id,
                generation,
                row.occurred_at,
                Category::Information,
                Status::Current,
                row.title.as_deref(),
                row.body.as_deref(),
                now,
            );
            changed |= matches!(
                result,
                Ingest::Added { .. } | Ingest::Updated { .. } | Ingest::Capacity
            );
        }
        self.seen = seen;
        changed
    }
}

pub(super) fn unix_seconds(created_ticks: i64) -> Option<i64> {
    let delta = created_ticks.checked_sub(UNIX_EPOCH_TICKS)?;
    (delta >= 0).then_some(delta / TICKS_PER_SECOND)
}

pub(super) fn text_from_utf16(units: &[u16], limit: usize) -> Option<String> {
    let mut text = String::new();
    for next in char::decode_utf16(units.iter().copied().take(limit.saturating_add(2))) {
        let value = next.unwrap_or(char::REPLACEMENT_CHARACTER);
        if value.is_control() && value != '\n' && value != '\t' {
            continue;
        }
        if text.len() + value.len_utf8() > limit {
            break;
        }
        text.push(value);
    }
    let value = text.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Inbox {
        let mut inbox = Inbox::default();
        let mut settings = inbox.settings().clone();
        settings.external_enabled = true;
        settings.retain_external_text = true;
        inbox.set_settings(settings, 100).unwrap();
        inbox
    }

    fn observed(created: i64, title: &str) -> ObservedToast {
        ObservedToast {
            id: FactId::Windows {
                app: "Fixture.App".into(),
                notification: 3,
                created,
            },
            occurred_at: created,
            title: Some(title.into()),
            body: None,
        }
    }

    #[test]
    fn notification_inbox_windows_projection_deduplicates_update_and_reused_id() {
        let mut inbox = fixture();
        let mut projection = ToastProjection::default();
        let apply = |projection: &mut ToastProjection, inbox: &mut Inbox, rows, complete| {
            projection.apply(
                inbox,
                ToastObservation {
                    toasts: rows,
                    complete,
                },
                100,
                |_| true,
            )
        };
        assert!(apply(
            &mut projection,
            &mut inbox,
            vec![observed(95, "First")],
            true
        ));
        let first = observed(95, "First").id;
        assert!(inbox.set_read(&first, true));
        assert!(!apply(
            &mut projection,
            &mut inbox,
            vec![observed(95, "First")],
            true
        ));
        assert!(apply(
            &mut projection,
            &mut inbox,
            vec![observed(95, "Updated")],
            true
        ));
        assert!(inbox.records()[0].read);
        assert_eq!(inbox.records()[0].status, Status::Current);
        assert!(!apply(&mut projection, &mut inbox, Vec::new(), false));
        assert_eq!(inbox.records()[0].status, Status::Current);
        assert!(apply(
            &mut projection,
            &mut inbox,
            vec![observed(99, "ID reused")],
            true
        ));
        assert_eq!(inbox.records().len(), 2);
        assert_eq!(inbox.records()[0].status, Status::SourceRemoved);
        assert_eq!(inbox.records()[1].status, Status::Current);
        assert!(!inbox.records()[1].read);
    }

    #[test]
    fn notification_inbox_windows_unsigned_time_and_utf16_caps() {
        assert_eq!(unix_seconds(UNIX_EPOCH_TICKS), Some(0));
        assert_eq!(
            unix_seconds(UNIX_EPOCH_TICKS + 91 * TICKS_PER_SECOND),
            Some(91)
        );
        assert_eq!(unix_seconds(UNIX_EPOCH_TICKS - 1), None);
        let text: Vec<_> = "a\u{0001}😂😂".encode_utf16().collect();
        assert_eq!(text_from_utf16(&text, 5).as_deref(), Some("a😂"));
        assert_eq!(text_from_utf16(&[0, b' ' as u16], 32), None);
    }
}
