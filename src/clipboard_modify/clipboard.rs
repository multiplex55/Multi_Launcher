use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};

use super::executor::{Cancellation, ExecuteError, execute_pipeline, execute_stages};
use super::model::{ClipboardModifierCatalog, StageSpec};

pub const RETRY_ATTEMPTS: usize = 4;
pub const RETRY_INITIAL_DELAY: Duration = Duration::from_millis(20);
pub const RETRY_MAX_DELAY: Duration = Duration::from_millis(120);
pub const RETRY_TOTAL_DEADLINE: Duration = Duration::from_millis(750);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardError {
    NonText,
    InvalidContent(String),
    Transient(String),
    Busy(String),
    Permanent(String),
    ConfirmationRequired(ClipboardConflict),
    NoUndo,
    Transform(String),
    Config(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardConflict {
    pub baseline: ClipboardSummary,
    pub current: ClipboardSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardSummary {
    pub bytes: usize,
    pub chars: usize,
    pub fingerprint: u64,
}

impl ClipboardSummary {
    pub fn new(text: &str) -> Self {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        Self {
            bytes: text.len(),
            chars: text.chars().count(),
            fingerprint: h.finish(),
        }
    }
}

impl ClipboardError {
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Transient(_) | Self::Busy(_))
    }
}
impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl std::error::Error for ClipboardError {}
impl From<ExecuteError> for ClipboardError {
    fn from(e: ExecuteError) -> Self {
        Self::Transform(e.to_string())
    }
}

pub trait ClipboardBackend: Send + Sync {
    fn read_text(&self) -> Result<String, ClipboardError>;
    fn write_text(&self, text: &str) -> Result<(), ClipboardError>;
}

#[derive(Debug, Default)]
pub struct ArboardClipboardBackend;
impl ClipboardBackend for ArboardClipboardBackend {
    fn read_text(&self) -> Result<String, ClipboardError> {
        let mut cb = arboard::Clipboard::new().map_err(classify_arboard)?;
        cb.get_text().map_err(classify_arboard)
    }
    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        let mut cb = arboard::Clipboard::new().map_err(classify_arboard)?;
        cb.set_text(text.to_string()).map_err(classify_arboard)
    }
}
fn classify_arboard(e: arboard::Error) -> ClipboardError {
    let s = e.to_string();
    match e {
        arboard::Error::ContentNotAvailable => ClipboardError::NonText,
        arboard::Error::ClipboardOccupied => ClipboardError::Busy(s),
        arboard::Error::ConversionFailure | arboard::Error::Unknown { .. } => {
            ClipboardError::InvalidContent(s)
        }
        _ => ClipboardError::Permanent(s),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoRecord {
    pub original_text: String,
    pub modified_text: String,
    pub operation_label: String,
}

pub struct ClipboardService<B> {
    backend: B,
    commit: Mutex<()>,
    undo: Mutex<Option<UndoRecord>>,
}
impl<B: ClipboardBackend> ClipboardService<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            commit: Mutex::new(()),
            undo: Mutex::new(None),
        }
    }
    pub fn undo_record(&self) -> Option<UndoRecord> {
        self.undo.lock().unwrap().clone()
    }

    pub fn apply_stages<C: Cancellation + ?Sized>(
        &self,
        stages: &[StageSpec],
        catalog: &ClipboardModifierCatalog,
        label: &str,
        c: &C,
    ) -> Result<UndoRecord, ClipboardError> {
        let a = self.read_retry()?;
        let out = execute_stages(&a, stages, catalog, c)?;
        self.commit_output(out, label)
    }
    pub fn apply_pipeline<C: Cancellation + ?Sized>(
        &self,
        name: &str,
        catalog: &ClipboardModifierCatalog,
        label: &str,
        c: &C,
    ) -> Result<UndoRecord, ClipboardError> {
        let a = self.read_retry()?;
        let out = execute_pipeline(&a, name, catalog, c)?;
        self.commit_output(out, label)
    }
    pub fn commit_output(&self, output: String, label: &str) -> Result<UndoRecord, ClipboardError> {
        let _g = self.commit.lock().unwrap();
        let b = self.read_retry()?;
        self.write_retry(&output)?;
        let rec = UndoRecord {
            original_text: b,
            modified_text: output,
            operation_label: label.to_string(),
        };
        *self.undo.lock().unwrap() = Some(rec.clone());
        Ok(rec)
    }
    pub fn commit_dialog(
        &self,
        baseline: &str,
        _working_source: &str,
        output: &str,
        confirmed: bool,
        label: &str,
    ) -> Result<UndoRecord, ClipboardError> {
        let _g = self.commit.lock().unwrap();
        let mut current = self.read_retry()?;
        if current.as_bytes() != baseline.as_bytes() && !confirmed {
            return Err(ClipboardError::ConfirmationRequired(ClipboardConflict {
                baseline: ClipboardSummary::new(baseline),
                current: ClipboardSummary::new(&current),
            }));
        }
        if confirmed {
            current = self.read_retry()?;
        }
        self.write_retry(output)?;
        let rec = UndoRecord {
            original_text: current,
            modified_text: output.to_string(),
            operation_label: label.to_string(),
        };
        *self.undo.lock().unwrap() = Some(rec.clone());
        Ok(rec)
    }
    pub fn undo(&self) -> Result<UndoRecord, ClipboardError> {
        let _g = self.commit.lock().unwrap();
        let rec = self
            .undo
            .lock()
            .unwrap()
            .clone()
            .ok_or(ClipboardError::NoUndo)?;
        let _diagnostic_current = self.read_retry().ok();
        self.write_retry(&rec.original_text)?;
        *self.undo.lock().unwrap() = None;
        Ok(rec)
    }
    fn read_retry(&self) -> Result<String, ClipboardError> {
        retry(|| self.backend.read_text())
    }
    fn write_retry(&self, text: &str) -> Result<(), ClipboardError> {
        retry(|| self.backend.write_text(text))
    }
}

fn retry<T>(mut op: impl FnMut() -> Result<T, ClipboardError>) -> Result<T, ClipboardError> {
    let start = Instant::now();
    let mut delay = RETRY_INITIAL_DELAY;
    for attempt in 1..=RETRY_ATTEMPTS {
        match op() {
            Err(e)
                if e.is_transient()
                    && attempt < RETRY_ATTEMPTS
                    && start.elapsed() + delay < RETRY_TOTAL_DEADLINE =>
            {
                sleep(delay);
                delay = (delay * 2).min(RETRY_MAX_DELAY);
            }
            other => return other,
        }
    }
    unreachable!()
}

pub type ProductionClipboardService = ClipboardService<ArboardClipboardBackend>;
pub fn production_clipboard_service() -> ProductionClipboardService {
    ClipboardService::new(ArboardClipboardBackend)
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::collections::VecDeque;

    #[derive(Debug, Clone)]
    pub enum Op {
        Read(Result<String, ClipboardError>),
        Write(Result<(), ClipboardError>),
        Delay(Duration),
        External(String),
    }
    #[derive(Debug, Default)]
    pub struct FakeClipboardBackend {
        text: Mutex<Option<String>>,
        ops: Mutex<VecDeque<Op>>,
        writes: Mutex<Vec<String>>,
    }
    impl FakeClipboardBackend {
        pub fn with_text(text: impl Into<String>) -> Self {
            Self {
                text: Mutex::new(Some(text.into())),
                ..Self::default()
            }
        }
        pub fn non_text() -> Self {
            Self {
                text: Mutex::new(None),
                ..Self::default()
            }
        }
        pub fn push(&self, op: Op) {
            self.ops.lock().unwrap().push_back(op);
        }
        pub fn writes(&self) -> Vec<String> {
            self.writes.lock().unwrap().clone()
        }
    }
    impl ClipboardBackend for FakeClipboardBackend {
        fn read_text(&self) -> Result<String, ClipboardError> {
            loop {
                match self.ops.lock().unwrap().pop_front() {
                    Some(Op::Read(r)) => return r,
                    Some(Op::Delay(d)) => sleep(d),
                    Some(Op::External(s)) => *self.text.lock().unwrap() = Some(s),
                    Some(Op::Write(w)) => {
                        self.ops.lock().unwrap().push_front(Op::Write(w));
                        break;
                    }
                    None => break,
                }
            }
            self.text
                .lock()
                .unwrap()
                .clone()
                .ok_or(ClipboardError::NonText)
        }
        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            loop {
                match self.ops.lock().unwrap().pop_front() {
                    Some(Op::Write(r)) => {
                        r?;
                        break;
                    }
                    Some(Op::Delay(d)) => sleep(d),
                    Some(Op::External(s)) => *self.text.lock().unwrap() = Some(s),
                    Some(Op::Read(r)) => {
                        self.ops.lock().unwrap().push_front(Op::Read(r));
                        break;
                    }
                    None => break,
                }
            }
            *self.text.lock().unwrap() = Some(text.to_string());
            self.writes.lock().unwrap().push(text.to_string());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{FakeClipboardBackend, Op};
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;

    fn svc(text: &str) -> ClipboardService<FakeClipboardBackend> {
        ClipboardService::new(FakeClipboardBackend::with_text(text))
    }

    #[test]
    fn empty_text_is_accepted() {
        let s = svc("");
        assert_eq!(s.commit_output("x".into(), "op").unwrap().original_text, "");
    }

    #[test]
    fn non_text_content_is_rejected() {
        let s = ClipboardService::new(FakeClipboardBackend::non_text());
        assert!(matches!(
            s.commit_output("x".into(), "op"),
            Err(ClipboardError::NonText)
        ));
    }

    #[test]
    fn no_op_writes_replace_undo() {
        let s = svc("same");
        let r = s.commit_output("same".into(), "noop").unwrap();
        assert_eq!(r.original_text, "same");
        assert_eq!(s.undo_record().unwrap().operation_label, "noop");
    }

    #[test]
    fn failed_writes_preserve_previous_undo() {
        let b = FakeClipboardBackend::with_text("a");
        let s = ClipboardService::new(b);
        s.commit_output("b".into(), "ok").unwrap();
        s.backend
            .push(Op::Write(Err(ClipboardError::Permanent("no".into()))));
        assert!(s.commit_output("c".into(), "bad").is_err());
        assert_eq!(s.undo_record().unwrap().modified_text, "b");
    }

    #[test]
    fn undo_ignores_external_changes_and_success_clears() {
        let s = svc("a");
        s.commit_output("b".into(), "op").unwrap();
        s.backend.push(Op::External("external".into()));
        let r = s.undo().unwrap();
        assert_eq!(r.original_text, "a");
        assert!(s.undo_record().is_none());
        assert_eq!(s.backend.writes().last().unwrap(), "a");
    }

    #[test]
    fn failed_undo_preserves_record() {
        let s = svc("a");
        s.commit_output("b".into(), "op").unwrap();
        s.backend
            .push(Op::Write(Err(ClipboardError::Permanent("no".into()))));
        assert!(s.undo().is_err());
        assert!(s.undo_record().is_some());
    }

    #[test]
    fn transient_read_and_write_failures_are_retried() {
        let b = FakeClipboardBackend::with_text("a");
        b.push(Op::Read(Err(ClipboardError::Busy("busy".into()))));
        b.push(Op::Read(Ok("a".into())));
        b.push(Op::Write(Err(ClipboardError::Transient("again".into()))));
        b.push(Op::Write(Ok(())));
        let s = ClipboardService::new(b);
        assert!(s.commit_output("b".into(), "op").is_ok());
    }

    #[test]
    fn permanent_errors_are_not_retried() {
        let b = FakeClipboardBackend::with_text("a");
        b.push(Op::Read(Err(ClipboardError::Permanent("bad".into()))));
        b.push(Op::Read(Ok("should-not-read".into())));
        let s = ClipboardService::new(b);
        assert!(matches!(
            s.commit_output("b".into(), "op"),
            Err(ClipboardError::Permanent(_))
        ));
    }

    #[test]
    fn dialog_conflict_and_confirmed_undo_replaces_actual_current() {
        let b = FakeClipboardBackend::with_text("baseline");
        b.push(Op::External("changed".into()));
        let s = ClipboardService::new(b);
        assert!(matches!(
            s.commit_dialog("baseline", "baseline", "out", false, "dlg"),
            Err(ClipboardError::ConfirmationRequired(_))
        ));
        let r = s
            .commit_dialog("baseline", "baseline", "out", true, "dlg")
            .unwrap();
        assert_eq!(r.original_text, "changed");
    }

    #[test]
    fn commits_cannot_interleave_but_computations_can_finish_any_order() {
        let s = Arc::new(svc("start"));
        let barrier = Arc::new(Barrier::new(3));
        let mut handles = Vec::new();
        for (out, delay) in [("slow", 30), ("fast", 0)] {
            let s = s.clone();
            let b = barrier.clone();
            handles.push(thread::spawn(move || {
                let source = s.read_retry().unwrap();
                b.wait();
                thread::sleep(Duration::from_millis(delay));
                s.commit_output(format!("{source}-{out}"), out).unwrap()
            }));
        }
        barrier.wait();
        let mut records = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>();
        records.sort_by(|a, b| a.operation_label.cmp(&b.operation_label));
        assert_eq!(records.len(), 2);
        assert_eq!(s.backend.writes().len(), 2);
        assert_eq!(
            s.undo_record().unwrap().original_text,
            if s.undo_record().unwrap().modified_text.ends_with("fast") {
                "start"
            } else {
                "start-fast"
            }
        );
    }
}
