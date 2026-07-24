use multi_launcher::clipboard_modify::clipboard::{
    ClipboardBackend, ClipboardError, ClipboardService,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Fake {
    text: Mutex<Option<String>>,
    reads: Mutex<VecDeque<Result<String, ClipboardError>>>,
    writes: Mutex<VecDeque<Result<(), ClipboardError>>>,
    committed: Mutex<Vec<String>>,
}
impl Fake {
    fn text(value: &str) -> Self {
        Self {
            text: Mutex::new(Some(value.into())),
            ..Default::default()
        }
    }
}
impl ClipboardBackend for Arc<Fake> {
    fn read_text(&self) -> Result<String, ClipboardError> {
        if let Some(result) = self.reads.lock().unwrap().pop_front() {
            return result;
        }
        self.text
            .lock()
            .unwrap()
            .clone()
            .ok_or(ClipboardError::NonText)
    }
    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        if let Some(result) = self.writes.lock().unwrap().pop_front() {
            result?;
        }
        *self.text.lock().unwrap() = Some(text.into());
        self.committed.lock().unwrap().push(text.into());
        Ok(())
    }
}

#[test]
fn retries_transient_reads_and_writes_and_succeeds() {
    let fake = Arc::new(Fake::text("old"));
    fake.reads
        .lock()
        .unwrap()
        .extend([Err(ClipboardError::Busy("read".into())), Ok("old".into())]);
    fake.writes
        .lock()
        .unwrap()
        .extend([Err(ClipboardError::Transient("write".into())), Ok(())]);
    let service = ClipboardService::new(fake.clone());
    let record = service.commit_output("new".into(), "test").unwrap();
    assert_eq!(record.original_text, "old");
    assert_eq!(&*fake.text.lock().unwrap(), &Some("new".into()));
}

#[test]
fn retry_exhaustion_non_text_and_empty_text_are_distinct() {
    let fake = Arc::new(Fake::text("x"));
    fake.reads
        .lock()
        .unwrap()
        .extend((0..4).map(|_| Err(ClipboardError::Busy("busy".into()))));
    assert!(matches!(
        ClipboardService::new(fake).read_text_for_modify(),
        Err(ClipboardError::Busy(_))
    ));
    assert!(matches!(
        ClipboardService::new(Arc::new(Fake::default())).read_text_for_modify(),
        Err(ClipboardError::NonText)
    ));
    assert_eq!(
        ClipboardService::new(Arc::new(Fake::text("")))
            .read_text_for_modify()
            .unwrap(),
        ""
    );
}

#[test]
fn immediate_noop_replaces_undo_and_successful_undo_clears_it() {
    let service = ClipboardService::new(Arc::new(Fake::text("same")));
    service.commit_output("first".into(), "first").unwrap();
    let record = service.commit_output("first".into(), "noop").unwrap();
    assert_eq!(record.operation_label, "noop");
    assert_eq!(record.original_text, "first");
    service.undo().unwrap();
    assert!(service.undo_record().is_none());
}

#[test]
fn external_change_requires_dialog_confirmation_and_becomes_undo_source() {
    let fake = Arc::new(Fake::text("baseline"));
    let service = ClipboardService::new(fake.clone());
    *fake.text.lock().unwrap() = Some("external".into());
    assert!(matches!(
        service.commit_dialog("baseline", "working", "out", false, "dialog"),
        Err(ClipboardError::ConfirmationRequired(_))
    ));
    let record = service
        .commit_dialog("baseline", "working", "out", true, "dialog")
        .unwrap();
    assert_eq!(record.original_text, "external");
    assert_eq!(&*fake.text.lock().unwrap(), &Some("out".into()));
}

#[test]
fn failed_undo_retains_the_record() {
    let fake = Arc::new(Fake::text("a"));
    let service = ClipboardService::new(fake.clone());
    service.commit_output("b".into(), "change").unwrap();
    fake.writes
        .lock()
        .unwrap()
        .push_back(Err(ClipboardError::Permanent("denied".into())));
    assert!(service.undo().is_err());
    assert!(service.undo_record().is_some());
}
