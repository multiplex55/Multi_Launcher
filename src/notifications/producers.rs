//! Sanitized adapters for actual launcher-owned terminal facts.
use super::{Category, FactId, InboxPublisher, InternalEvent, InternalFact, RelatedItem, Status};
use crate::activity::{ActivityCenter, ActivityKey, ActivityOutcome, ActivityOwner};
use crate::hotfolder::{HotFolderExecutionReceipt, HotFolderTerminalStatus};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| i64::try_from(time.as_secs()).ok())
        .unwrap_or(0)
}

pub(crate) fn attach_activity_inbox(
    center: &ActivityCenter,
    publisher: InboxPublisher,
    session: [u8; 16],
) {
    center.set_terminal_observer(Arc::new(move |key, outcome| {
        if let Some(event) = activity_fact(session, key, outcome) {
            let _ = publisher.try_publish(event);
        }
    }));
}

fn activity_fact(
    session: [u8; 16],
    key: ActivityKey,
    outcome: ActivityOutcome,
) -> Option<InternalEvent> {
    let transfer = key.owner() == ActivityOwner::FileTransfer;
    if key.owner() == ActivityOwner::HotFolder
        || outcome == ActivityOutcome::Superseded
        || (outcome == ActivityOutcome::Completed
            && matches!(
                key.owner(),
                ActivityOwner::Indexing | ActivityOwner::FileSearch
            ))
    {
        return None;
    }
    let (category, status, fact) = match (transfer, outcome) {
        (true, ActivityOutcome::Completed) => (
            Category::Completed,
            Status::Completed,
            InternalFact::TransferCompleted,
        ),
        (true, ActivityOutcome::Partial) => (
            Category::Warning,
            Status::Partial,
            InternalFact::TransferPartial,
        ),
        (true, ActivityOutcome::Cancelled) => (
            Category::Information,
            Status::Cancelled,
            InternalFact::TransferCancelled,
        ),
        (true, ActivityOutcome::Failed) => (
            Category::Failed,
            Status::Failed,
            InternalFact::TransferFailed,
        ),
        (false, ActivityOutcome::Completed) => (
            Category::Completed,
            Status::Completed,
            InternalFact::ActivityCompleted,
        ),
        (false, ActivityOutcome::Partial) => (
            Category::Warning,
            Status::Partial,
            InternalFact::ActivityPartial,
        ),
        (false, ActivityOutcome::Cancelled) => (
            Category::Information,
            Status::Cancelled,
            InternalFact::ActivityCancelled,
        ),
        (false, ActivityOutcome::Failed) => (
            Category::Failed,
            Status::Failed,
            InternalFact::ActivityFailed,
        ),
        (_, ActivityOutcome::Superseded) => return None,
    };
    let (id, related) = if transfer {
        let job = key.incarnation().get();
        (
            FactId::Transfer { session, job },
            RelatedItem::Transfer { session, job },
        )
    } else {
        let id = key.operation_id().get();
        (
            FactId::Activity {
                session,
                id,
                generation: key.incarnation().get(),
            },
            RelatedItem::Activity { session, id },
        )
    };
    Some(InternalEvent {
        id,
        occurred_at: unix_now(),
        category,
        status,
        fact,
        related: Some(related),
    })
}

pub(crate) fn publish_hot_folder(publisher: &InboxPublisher, receipt: &HotFolderExecutionReceipt) {
    let (category, status, fact) = match (receipt.status, receipt.durable) {
        (HotFolderTerminalStatus::Success, true) => (
            Category::Completed,
            Status::Completed,
            InternalFact::HotFolderCompleted,
        ),
        (HotFolderTerminalStatus::Success | HotFolderTerminalStatus::Partial, _) => (
            Category::Warning,
            Status::Partial,
            InternalFact::HotFolderPartial,
        ),
        (HotFolderTerminalStatus::Cancelled, _) => (
            Category::Information,
            Status::Cancelled,
            InternalFact::HotFolderCancelled,
        ),
        (HotFolderTerminalStatus::Failure, _) => (
            Category::Failed,
            Status::Failed,
            InternalFact::HotFolderFailed,
        ),
    };
    let rule = receipt.rule_id.as_str().to_owned();
    let _ = publisher.try_publish(InternalEvent {
        id: FactId::HotFolder {
            rule: rule.clone(),
            event: receipt.operation_id.to_le_bytes(),
        },
        occurred_at: unix_now(),
        category,
        status,
        fact,
        related: Some(RelatedItem::HotFolder { rule }),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{
        ActivityIncarnation, ActivityOperation, ActivityRetirement, ActivitySlot,
    };
    use crate::notifications::{InboxService, Source};
    use crate::platform::app_data::AppDataRoot;

    #[test]
    fn notification_inbox_activity_observes_transfer_partial_once() {
        let directory = tempfile::tempdir().unwrap();
        let profile = AppDataRoot::from_path(directory.path());
        let mut worker = InboxService::start(profile, Arc::new(|| {})).unwrap();
        let activity = ActivityCenter::new();
        attach_activity_inbox(&activity, worker.publisher(), [1; 16]);
        let job = activity
            .start(
                ActivityOwner::FileTransfer,
                ActivitySlot::Primary,
                ActivityIncarnation::new(37),
                ActivityOperation::TransferFiles,
            )
            .unwrap();
        job.finish(
            ActivityOutcome::Partial,
            None,
            ActivityRetirement::Confirmed,
        );
        job.finish(
            ActivityOutcome::Completed,
            None,
            ActivityRetirement::Confirmed,
        );
        worker.shutdown();
        let snapshot = worker.snapshot();
        assert_eq!(snapshot.inbox.records().len(), 1);
        let row = &snapshot.inbox.records()[0];
        assert_eq!(row.id.source(), Source::Transfer);
        assert_eq!(row.status, Status::Partial);
        assert_eq!(
            row.related,
            Some(RelatedItem::Transfer {
                session: [1; 16],
                job: 37
            })
        );
    }

    #[test]
    fn notification_inbox_activity_transfer_job_ids_are_scoped_to_launcher_session() {
        let directory = tempfile::tempdir().unwrap();
        let profile = AppDataRoot::from_path(directory.path());
        for session in [[1; 16], [2; 16]] {
            let mut worker = InboxService::start(profile.clone(), Arc::new(|| {})).unwrap();
            let activity = ActivityCenter::new();
            attach_activity_inbox(&activity, worker.publisher(), session);
            let job = activity
                .start(
                    ActivityOwner::FileTransfer,
                    ActivitySlot::Primary,
                    ActivityIncarnation::new(1),
                    ActivityOperation::TransferFiles,
                )
                .unwrap();
            job.finish(
                ActivityOutcome::Completed,
                None,
                ActivityRetirement::Confirmed,
            );
            worker.shutdown();
        }
        let mut worker = InboxService::start(profile, Arc::new(|| {})).unwrap();
        worker.shutdown();
        let snapshot = worker.snapshot();
        let records = snapshot.inbox.records();
        assert_eq!(records.len(), 2);
        for session in [[1; 16], [2; 16]] {
            assert!(
                records
                    .iter()
                    .any(|record| record.id == FactId::Transfer { session, job: 1 })
            );
        }
    }
}
