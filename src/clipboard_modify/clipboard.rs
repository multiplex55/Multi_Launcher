use crate::platform::protected_clipboard::{
    ClipboardError as ProtectedClipboardError, ClipboardRead, ClipboardStamp, OpenClipboardFormat,
    StampedClipboardRead, checked_open_clipboard_is_ordinary, current_clipboard_stamp,
    read_clipboard_for_modify, read_stamped_clipboard_for_modify,
};
use std::sync::{Mutex, TryLockError};
use std::thread::sleep;
use std::time::{Duration, Instant};

use super::executor::{Cancellation, ExecuteError, execute_pipeline, execute_stages};
use super::model::{ClipboardModifierCatalog, StageSpec};
use super::parser::ClipboardModifyIntent;

pub const RETRY_ATTEMPTS: usize = 4;
pub const RETRY_INITIAL_DELAY: Duration = Duration::from_millis(20);
pub const RETRY_MAX_DELAY: Duration = Duration::from_millis(120);
pub const RETRY_TOTAL_DEADLINE: Duration = Duration::from_millis(750);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardError {
    NonText,
    ProtectedItem,
    TimedOut,
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
        matches!(self, Self::TimedOut | Self::Transient(_) | Self::Busy(_))
    }
}
impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if matches!(self, Self::ProtectedItem) {
            return f.write_str("This clipboard item was copied privately.");
        }
        if matches!(self, Self::TimedOut) {
            return f.write_str("The clipboard did not respond in time.");
        }
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
    /// Checked commits require a backend that can bind its read to its writer.
    fn read_stamped(&self) -> Result<(String, ClipboardStamp), ClipboardError> {
        Err(ClipboardError::Permanent(
            "Checked clipboard access unavailable".into(),
        ))
    }
    fn write_stamped_if(
        &self,
        _text: &str,
        _stamp: ClipboardStamp,
        _current: &dyn Fn() -> bool,
    ) -> Result<bool, ClipboardError> {
        Err(ClipboardError::Permanent(
            "Checked clipboard access unavailable".into(),
        ))
    }
    fn write_text_if(
        &self,
        text: &str,
        current: &dyn Fn() -> bool,
    ) -> Result<bool, ClipboardError> {
        if !current() {
            return Ok(false);
        }
        self.write_text(text)?;
        Ok(true)
    }
    /// Fail closed unless the backend checks authority after acquiring its writer.
    fn write_text_acquired_if(
        &self,
        _text: &str,
        _current: &dyn Fn() -> bool,
    ) -> Result<bool, ClipboardError> {
        Err(ClipboardError::Permanent("Acquired clipboard writer unavailable".into()))
    }
}

#[derive(Debug, Default)]
pub struct ArboardClipboardBackend;
impl ClipboardBackend for ArboardClipboardBackend {
    fn read_stamped(&self) -> Result<(String, ClipboardStamp), ClipboardError> {
        match read_stamped_clipboard_for_modify() {
            Ok(StampedClipboardRead::Ordinary {
                text,
                stamp: Some(stamp),
            }) => Ok((text, stamp)),
            Ok(StampedClipboardRead::Ordinary { stamp: None, .. }) => Err(
                ClipboardError::Permanent("Clipboard sequence unavailable".into()),
            ),
            Ok(StampedClipboardRead::Private) => Err(ClipboardError::ProtectedItem),
            Ok(StampedClipboardRead::NonText) => Err(ClipboardError::NonText),
            Err(ProtectedClipboardError::TimedOut) => Err(ClipboardError::TimedOut),
            Err(error) if error.is_busy() => Err(ClipboardError::Busy(error.to_string())),
            Err(error) => Err(ClipboardError::Permanent(error.to_string())),
        }
    }
    fn write_stamped_if(
        &self,
        text: &str,
        stamp: ClipboardStamp,
        current: &dyn Fn() -> bool,
    ) -> Result<bool, ClipboardError> {
        let mut clipboard = arboard::Clipboard::new().map_err(classify_arboard)?;
        let writer = clipboard.set();
        match checked_open_clipboard_is_ordinary() {
            Ok(OpenClipboardFormat::Ordinary) => {}
            Ok(OpenClipboardFormat::Private) => return Err(ClipboardError::ProtectedItem),
            Ok(OpenClipboardFormat::NonText) => return Err(ClipboardError::NonText),
            Err(error) if error.is_busy() => return Err(ClipboardError::Busy(error.to_string())),
            Err(error) => return Err(ClipboardError::Permanent(error.to_string())),
        }
        if current_clipboard_stamp() != Some(stamp) || !current() {
            return Ok(false);
        }
        writer.text(text).map_err(classify_arboard)?;
        Ok(true)
    }
    fn read_text(&self) -> Result<String, ClipboardError> {
        map_protected_read(read_clipboard_for_modify())
    }
    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        let mut cb = arboard::Clipboard::new().map_err(classify_arboard)?;
        cb.set_text(text.to_string()).map_err(classify_arboard)
    }
    fn write_text_if(
        &self,
        text: &str,
        current: &dyn Fn() -> bool,
    ) -> Result<bool, ClipboardError> {
        let mut clipboard = arboard::Clipboard::new().map_err(classify_arboard)?;
        let writer = clipboard.set();
        if !current() {
            return Ok(false);
        }
        writer.text(text).map_err(classify_arboard)?;
        Ok(true)
    }
    #[cfg(windows)]
    fn write_text_acquired_if(&self, text: &str, current: &dyn Fn() -> bool) -> Result<bool, ClipboardError> {
        let mut clipboard = arboard::Clipboard::new().map_err(classify_arboard)?;
        let writer = clipboard.set();
        if !current() { return Ok(false); }
        writer.text(text).map_err(classify_arboard)?;
        Ok(true)
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

fn map_protected_read(
    result: Result<ClipboardRead, ProtectedClipboardError>,
) -> Result<String, ClipboardError> {
    match result {
        Ok(ClipboardRead::Ordinary(text)) => Ok(text),
        Ok(ClipboardRead::NonText) => Err(ClipboardError::NonText),
        Ok(ClipboardRead::Private) => Err(ClipboardError::ProtectedItem),
        Err(ProtectedClipboardError::TimedOut) => Err(ClipboardError::TimedOut),
        Err(error) if error.is_busy() => Err(ClipboardError::Busy(error.to_string())),
        Err(error) => Err(ClipboardError::Permanent(error.to_string())),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoRecord {
    pub original_text: String,
    pub modified_text: String,
    pub operation_label: String,
}

/// Exact reviewed clipboard content remains private and is never formatted.
#[derive(Clone)]
pub struct ClipboardReview {
    expected: String,
    stamp: ClipboardStamp,
    pub baseline: ClipboardSummary,
    pub current: ClipboardSummary,
}
impl std::fmt::Debug for ClipboardReview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipboardReview")
            .field("baseline", &self.baseline)
            .field("current", &self.current)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy)]
pub enum CheckedClipboardEffect<'a> {
    Apply { baseline: &'a str },
    CopyResult,
    ReviewedOverwrite { review: &'a ClipboardReview },
}
pub enum CheckedClipboardOutcome {
    Written,
    ReviewRequired(ClipboardReview),
    Superseded,
}

#[derive(Clone, Copy)]
enum RetryPolicy {
    SingleAttempt,
    Bounded,
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

    /// Commit a dialog action only while its protected snapshot and caller's
    /// authority still hold at the backend's acquired-writer boundary.
    pub fn commit_dialog_checked(
        &self,
        effect: CheckedClipboardEffect<'_>,
        output: &str,
        label: &str,
        current: &dyn Fn() -> bool,
    ) -> Result<CheckedClipboardOutcome, ClipboardError> {
        if output.contains('\0') {
            return Err(ClipboardError::InvalidContent(
                "Clipboard text contains NUL".into(),
            ));
        }
        let _guard = self.try_commit_lock()?;
        if !current() {
            return Ok(CheckedClipboardOutcome::Superseded);
        }
        let first = self.backend.read_stamped();
        if !current() {
            return Ok(CheckedClipboardOutcome::Superseded);
        }
        let (mut text, mut stamp) = first?;
        if matches!(
            effect,
            CheckedClipboardEffect::CopyResult | CheckedClipboardEffect::ReviewedOverwrite { .. }
        ) {
            let second = self.backend.read_stamped();
            if !current() {
                return Ok(CheckedClipboardOutcome::Superseded);
            }
            (text, stamp) = second?;
        }
        let expected = match effect {
            CheckedClipboardEffect::Apply { baseline } => {
                Some((baseline, ClipboardSummary::new(baseline), None))
            }
            CheckedClipboardEffect::ReviewedOverwrite { review } => Some((
                review.expected.as_str(),
                review.baseline.clone(),
                Some(review.stamp),
            )),
            CheckedClipboardEffect::CopyResult => None,
        };
        if let Some((expected, baseline, expected_stamp)) = expected {
            if text != expected || expected_stamp.is_some_and(|expected| expected != stamp) {
                return Ok(CheckedClipboardOutcome::ReviewRequired(ClipboardReview {
                    baseline,
                    current: ClipboardSummary::new(&text),
                    expected: text,
                    stamp,
                }));
            }
        }
        if !self.backend.write_stamped_if(output, stamp, current)? {
            return Ok(CheckedClipboardOutcome::Superseded);
        }
        let record = UndoRecord {
            original_text: text,
            modified_text: output.to_owned(),
            operation_label: label.to_owned(),
        };
        *self.undo.lock().unwrap() = Some(record);
        Ok(CheckedClipboardOutcome::Written)
    }

    pub fn read_text_for_modify(&self) -> Result<String, ClipboardError> {
        self.read_with_policy(RetryPolicy::Bounded)
    }

    /// Skip service-level backoff for GUI-owned reads so the next user action
    /// can retry. The platform backend may still incur its own bounded wait.
    pub fn read_text_for_modify_once(&self) -> Result<String, ClipboardError> {
        self.read_with_policy(RetryPolicy::SingleAttempt)
    }

    #[cfg(test)]
    pub fn backend(&self) -> &B {
        &self.backend
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
    /// Execute a typed direct action using protected source A and a fresh
    /// protected replacement B. The caller owns the worker and its admission.
    pub fn execute_checked<C: Cancellation + ?Sized>(
        &self,
        intent: &ClipboardModifyIntent,
        catalog: &ClipboardModifierCatalog,
        label: &str,
        cancellation: &C,
        current: &dyn Fn() -> bool,
    ) -> Result<bool, ClipboardError> {
        if matches!(intent, ClipboardModifyIntent::Undo) {
            return Err(ClipboardError::Config(
                "Undo requires its own checked action".into(),
            ));
        }
        let cancelled = || cancellation.is_cancelled() || !current();
        if cancelled() {
            return Ok(false);
        }
        let source = self.backend.read_text();
        if cancelled() {
            return Ok(false);
        }
        let source = source?;
        let output = super::executor::run_intent(&source, intent, catalog, &cancelled);
        if cancelled() {
            return Ok(false);
        }
        let output = match output {
            Ok(output) => output,
            Err(ExecuteError::Cancelled) => return Ok(false),
            Err(_) => {
                return Err(ClipboardError::Transform(
                    "Clipboard transformation failed".into(),
                ));
            }
        };
        if output.contains('\0') {
            return Err(ClipboardError::InvalidContent(
                "Clipboard text contains NUL".into(),
            ));
        }
        if cancelled() {
            return Ok(false);
        }
        let _guard = self.try_commit_lock()?;
        if cancelled() {
            return Ok(false);
        }
        let replacement = self.backend.read_stamped();
        if cancelled() {
            return Ok(false);
        }
        let (original_text, stamp) = replacement?;
        if original_text.contains('\0') {
            return Err(ClipboardError::InvalidContent(
                "Clipboard text contains NUL".into(),
            ));
        }
        let write = self
            .backend
            .write_stamped_if(&output, stamp, &|| !cancelled());
        let written = match write {
            Ok(written) => written,
            Err(_) if cancelled() => return Ok(false),
            Err(error) => return Err(error),
        };
        if !written {
            return Ok(false);
        }
        *self.undo.lock().unwrap() = Some(UndoRecord {
            original_text,
            modified_text: output,
            operation_label: label.to_owned(),
        });
        Ok(true)
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

    /// GUI commits and Undo must not wait behind a worker's bounded retry loop.
    /// Keep the existing panic-on-poison policy; only ordinary contention maps
    /// to a retryable Busy result.
    fn try_commit_lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, ClipboardError> {
        match self.commit.try_lock() {
            Err(TryLockError::WouldBlock) => Err(ClipboardError::Busy(
                "Another clipboard operation is in progress".into(),
            )),
            result => Ok(result.unwrap()),
        }
    }

    pub fn commit_dialog(
        &self,
        baseline: &str,
        _working_source: &str,
        output: &str,
        confirmed: bool,
        label: &str,
    ) -> Result<UndoRecord, ClipboardError> {
        self.commit_dialog_with_review(baseline, output, confirmed, None, label)
    }

    pub(crate) fn commit_reviewed_dialog(
        &self,
        baseline: &str,
        output: &str,
        expected: &ClipboardSummary,
        label: &str,
    ) -> Result<UndoRecord, ClipboardError> {
        self.commit_dialog_with_review(baseline, output, true, Some(expected), label)
    }

    fn commit_dialog_with_review(
        &self,
        baseline: &str,
        output: &str,
        confirmed: bool,
        expected: Option<&ClipboardSummary>,
        label: &str,
    ) -> Result<UndoRecord, ClipboardError> {
        let _g = self.try_commit_lock()?;
        let mut current = self.read_with_policy(RetryPolicy::SingleAttempt)?;
        if current.as_bytes() != baseline.as_bytes() && !confirmed {
            return Err(ClipboardError::ConfirmationRequired(ClipboardConflict {
                baseline: ClipboardSummary::new(baseline),
                current: ClipboardSummary::new(&current),
            }));
        }
        if confirmed {
            current = self.read_with_policy(RetryPolicy::SingleAttempt)?;
        }
        if expected.is_some_and(|expected| *expected != ClipboardSummary::new(&current)) {
            return Err(ClipboardError::ConfirmationRequired(ClipboardConflict {
                baseline: ClipboardSummary::new(baseline),
                current: ClipboardSummary::new(&current),
            }));
        }
        self.write_with_policy(output, RetryPolicy::SingleAttempt)?;
        let rec = UndoRecord {
            original_text: current,
            modified_text: output.to_string(),
            operation_label: label.to_string(),
        };
        *self.undo.lock().unwrap() = Some(rec.clone());
        Ok(rec)
    }
    pub fn undo(&self) -> Result<UndoRecord, ClipboardError> {
        self.undo_with_policy(RetryPolicy::Bounded)
    }

    /// Restore the last committed text only while the protected clipboard and
    /// the launcher's authority remain current at the acquired writer.
    pub fn undo_checked(&self, current: &dyn Fn() -> bool) -> Result<bool, ClipboardError> {
        if !current() {
            return Ok(false);
        }
        let _guard = self.try_commit_lock()?;
        if !current() {
            return Ok(false);
        }
        let record = self
            .undo
            .lock()
            .unwrap()
            .clone()
            .ok_or(ClipboardError::NoUndo)?;
        if !current() {
            return Ok(false);
        }
        if record.original_text.contains('\0') {
            return Err(ClipboardError::InvalidContent(
                "Clipboard text contains NUL".into(),
            ));
        }
        let snapshot = self.backend.read_stamped();
        if !current() {
            return Ok(false);
        }
        let (_, stamp) = snapshot?;
        if !self
            .backend
            .write_stamped_if(&record.original_text, stamp, current)?
        {
            return Ok(false);
        }
        *self.undo.lock().unwrap() = None;
        Ok(true)
    }

    /// GUI Undo is one attempt; a Busy result leaves the record intact for an
    /// explicit user retry instead of sleeping on the UI thread.
    pub(crate) fn undo_without_retry(&self) -> Result<UndoRecord, ClipboardError> {
        self.undo_with_policy(RetryPolicy::SingleAttempt)
    }

    fn undo_with_policy(&self, policy: RetryPolicy) -> Result<UndoRecord, ClipboardError> {
        let _g = match policy {
            RetryPolicy::SingleAttempt => self.try_commit_lock()?,
            RetryPolicy::Bounded => self.commit.lock().unwrap(),
        };
        let rec = self
            .undo
            .lock()
            .unwrap()
            .clone()
            .ok_or(ClipboardError::NoUndo)?;
        let _current = self.read_with_policy(policy)?;
        self.write_with_policy(&rec.original_text, policy)?;
        *self.undo.lock().unwrap() = None;
        Ok(rec)
    }

    fn read_with_policy(&self, policy: RetryPolicy) -> Result<String, ClipboardError> {
        match policy {
            RetryPolicy::SingleAttempt => self.backend.read_text(),
            RetryPolicy::Bounded => self.read_retry(),
        }
    }

    fn write_with_policy(&self, text: &str, policy: RetryPolicy) -> Result<(), ClipboardError> {
        match policy {
            RetryPolicy::SingleAttempt => self.backend.write_text(text),
            RetryPolicy::Bounded => self.write_retry(text),
        }
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
                let op = self.ops.lock().unwrap().pop_front();
                match op {
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
                let op = self.ops.lock().unwrap().pop_front();
                match op {
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
    #[test]
    fn checked_direct_execution_uses_a_then_replaces_and_undoes_b() {
        use super::{ClipboardBackend, ClipboardError, ClipboardService, UndoRecord};
        use crate::{
            clipboard_modify::{
                defaults::default_catalog,
                model::{OperationId, StageArguments, StageSpec},
                parser::ClipboardModifyIntent,
            },
            platform::protected_clipboard::ClipboardStamp,
        };
        use std::sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        };
        struct State {
            source: Result<String, ClipboardError>,
            current: String,
            sequence: u32,
            b_reads: usize,
            b_error: Option<ClipboardError>,
            stale_writer: bool,
            revoke_writer: bool,
            writes: Vec<String>,
            revoke_source: bool,
        }
        struct Backend {
            state: Mutex<State>,
            live: Arc<AtomicBool>,
        }
        impl ClipboardBackend for Backend {
            fn read_text(&self) -> Result<String, ClipboardError> {
                let s = self.state.lock().unwrap();
                if s.revoke_source {
                    self.live.store(false, Ordering::Release);
                }
                s.source.clone()
            }
            fn read_stamped(&self) -> Result<(String, ClipboardStamp), ClipboardError> {
                let mut s = self.state.lock().unwrap();
                s.b_reads += 1;
                if let Some(error) = &s.b_error {
                    return Err(error.clone());
                }
                Ok((
                    s.current.clone(),
                    ClipboardStamp::from_sequence(s.sequence).unwrap(),
                ))
            }
            fn write_text(&self, _: &str) -> Result<(), ClipboardError> {
                panic!("unchecked write")
            }
            fn write_stamped_if(
                &self,
                text: &str,
                stamp: ClipboardStamp,
                current: &dyn Fn() -> bool,
            ) -> Result<bool, ClipboardError> {
                let mut s = self.state.lock().unwrap();
                if s.stale_writer {
                    s.sequence += 1;
                    s.stale_writer = false;
                }
                if s.revoke_writer {
                    self.live.store(false, Ordering::Release);
                    s.revoke_writer = false;
                }
                if ClipboardStamp::from_sequence(s.sequence) != Some(stamp) || !current() {
                    return Ok(false);
                }
                s.current = text.to_owned();
                s.writes.push(text.to_owned());
                s.sequence += 1;
                Ok(true)
            }
        }
        let live = Arc::new(AtomicBool::new(true));
        let service = ClipboardService::new(Backend {
            live: live.clone(),
            state: Mutex::new(State {
                source: Ok("é source".into()),
                current: "replacement B 😀".into(),
                sequence: 1,
                b_reads: 0,
                b_error: None,
                stale_writer: false,
                revoke_writer: false,
                writes: Vec::new(),
                revoke_source: false,
            }),
        });
        let prior = UndoRecord {
            original_text: "old original".into(),
            modified_text: "old output".into(),
            operation_label: "old".into(),
        };
        *service.undo.lock().unwrap() = Some(prior.clone());
        let intent = ClipboardModifyIntent::Stages(vec![StageSpec {
            operation: OperationId::Uppercase,
            arguments: StageArguments::default(),
        }]);
        let catalog = default_catalog();
        let cancelled = AtomicBool::new(false);
        let current = || live.load(Ordering::Acquire);
        service.backend.state.lock().unwrap().stale_writer = true;
        assert!(
            !service
                .execute_checked(&intent, &catalog, "direct", &cancelled, &current)
                .unwrap()
        );
        assert_eq!(service.undo_record(), Some(prior.clone()));
        assert!(service.backend.state.lock().unwrap().writes.is_empty());
        let sequence = service.backend.state.lock().unwrap().sequence;
        service.backend.state.lock().unwrap().revoke_writer = true;
        assert!(
            !service
                .execute_checked(&intent, &catalog, "direct", &cancelled, &current)
                .unwrap()
        );
        assert_eq!(service.backend.state.lock().unwrap().sequence, sequence);
        assert!(service.backend.state.lock().unwrap().writes.is_empty());
        assert_eq!(service.undo_record(), Some(prior.clone()));
        live.store(true, Ordering::Release);
        assert!(
            service
                .execute_checked(&intent, &catalog, "direct", &cancelled, &current)
                .unwrap()
        );
        assert_eq!(
            service.undo_record().unwrap().original_text,
            "replacement B 😀"
        );
        assert_eq!(service.backend.state.lock().unwrap().current, "É SOURCE");
        assert!(service.undo_checked(&current).unwrap());
        assert_eq!(
            service.backend.state.lock().unwrap().current,
            "replacement B 😀"
        );
        assert_eq!(
            service.backend.state.lock().unwrap().writes,
            vec!["É SOURCE", "replacement B 😀"]
        );
        *service.undo.lock().unwrap() = Some(prior.clone());
        {
            let mut s = service.backend.state.lock().unwrap();
            s.source = Err(ClipboardError::ProtectedItem);
            s.writes.clear();
        }
        assert!(matches!(
            service.execute_checked(&intent, &catalog, "direct", &cancelled, &current),
            Err(ClipboardError::ProtectedItem)
        ));
        assert_eq!(service.undo_record(), Some(prior.clone()));
        {
            let mut s = service.backend.state.lock().unwrap();
            s.source = Ok("fixture".into());
            s.revoke_source = true;
        }
        let reads = service.backend.state.lock().unwrap().b_reads;
        assert!(
            !service
                .execute_checked(&intent, &catalog, "direct", &cancelled, &current)
                .unwrap()
        );
        assert_eq!(service.backend.state.lock().unwrap().b_reads, reads);
        assert_eq!(service.undo_record(), Some(prior.clone()));
        assert!(service.backend.state.lock().unwrap().writes.is_empty());
        live.store(true, Ordering::Release);
        {
            let mut s = service.backend.state.lock().unwrap();
            s.revoke_source = false;
            s.b_error = Some(ClipboardError::NonText);
        }
        assert!(matches!(
            service.execute_checked(&intent, &catalog, "direct", &cancelled, &current),
            Err(ClipboardError::NonText)
        ));
        assert_eq!(service.undo_record(), Some(prior.clone()));
        {
            service.backend.state.lock().unwrap().b_error = None;
        }
        let checks = std::sync::atomic::AtomicUsize::new(0);
        let stop_during_execution = || checks.fetch_add(1, Ordering::AcqRel) >= 2;
        let reads = service.backend.state.lock().unwrap().b_reads;
        assert!(
            !service
                .execute_checked(
                    &intent,
                    &catalog,
                    "direct",
                    &stop_during_execution,
                    &current
                )
                .unwrap()
        );
        assert_eq!(service.backend.state.lock().unwrap().b_reads, reads);
        assert_eq!(service.undo_record(), Some(prior));
        let template = ClipboardModifyIntent::ApplyTemplate {
            name: "markdown-quote".into(),
        };
        assert!(
            service
                .execute_checked(&template, &catalog, "template", &cancelled, &current)
                .unwrap()
        );
        assert_eq!(service.backend.state.lock().unwrap().current, "> fixture");
        let pipeline = ClipboardModifyIntent::ApplySavedPipeline {
            name: "clean-lines".into(),
        };
        assert!(
            service
                .execute_checked(&pipeline, &catalog, "pipeline", &cancelled, &current)
                .unwrap()
        );
        assert_eq!(service.backend.state.lock().unwrap().current, "fixture");
        assert_eq!(service.undo_record().unwrap().original_text, "> fixture");
        let saved = service.undo_record().unwrap();
        {
            let mut s = service.backend.state.lock().unwrap();
            s.source = Ok("bad\0text".into());
            s.writes.clear();
        }
        let identity = ClipboardModifyIntent::Stages(Vec::new());
        assert!(matches!(
            service.execute_checked(&identity, &catalog, "direct", &cancelled, &current),
            Err(ClipboardError::InvalidContent(_))
        ));
        assert!(service.backend.state.lock().unwrap().writes.is_empty());
        assert_eq!(service.undo_record(), Some(saved));
    }
    #[test]
    fn checked_dialog_effects_revalidate_at_writer_and_preserve_undo() {
        use super::{
            CheckedClipboardEffect, CheckedClipboardOutcome, ClipboardBackend, ClipboardError,
            ClipboardService,
        };
        use crate::platform::protected_clipboard::ClipboardStamp;
        use std::sync::{
            Mutex,
            atomic::{AtomicBool, Ordering},
        };
        struct State {
            text: String,
            seq: u32,
            private: bool,
            change_at_open: bool,
            writes: usize,
            reads: usize,
            guard_error: Option<ClipboardError>,
            change_seq_at_open: bool,
        }
        struct Backend(Mutex<State>);
        impl ClipboardBackend for Backend {
            fn read_text(&self) -> Result<String, ClipboardError> {
                Ok(self.0.lock().unwrap().text.clone())
            }
            fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
                let mut s = self.0.lock().unwrap();
                s.text = text.into();
                s.writes += 1;
                Ok(())
            }
            fn read_stamped(&self) -> Result<(String, ClipboardStamp), ClipboardError> {
                let mut s = self.0.lock().unwrap();
                s.reads += 1;
                if s.private {
                    return Err(ClipboardError::ProtectedItem);
                }
                Ok((
                    s.text.clone(),
                    ClipboardStamp::from_sequence(s.seq).unwrap(),
                ))
            }
            fn write_stamped_if(
                &self,
                text: &str,
                stamp: ClipboardStamp,
                current: &dyn Fn() -> bool,
            ) -> Result<bool, ClipboardError> {
                let mut s = self.0.lock().unwrap();
                if let Some(error) = s.guard_error.take() {
                    return Err(error);
                }
                if s.change_at_open {
                    s.private = true;
                    s.seq += 1;
                    s.change_at_open = false;
                }
                if s.change_seq_at_open {
                    s.seq += 1;
                    s.change_seq_at_open = false;
                }
                if s.private {
                    return Err(ClipboardError::ProtectedItem);
                }
                if ClipboardStamp::from_sequence(s.seq) != Some(stamp) || !current() {
                    return Ok(false);
                }
                s.text = text.into();
                s.seq += 1;
                s.writes += 1;
                Ok(true)
            }
        }
        let service = ClipboardService::new(Backend(Mutex::new(State {
            text: "baseline".into(),
            seq: 1,
            private: false,
            change_at_open: false,
            writes: 0,
            reads: 0,
            guard_error: None,
            change_seq_at_open: false,
        })));
        let live = AtomicBool::new(true);
        let current = || live.load(Ordering::Acquire);
        let outcome = service
            .commit_dialog_checked(
                CheckedClipboardEffect::Apply {
                    baseline: "baseline",
                },
                " café\n😀 ",
                "Apply",
                &current,
            )
            .unwrap();
        assert!(matches!(outcome, CheckedClipboardOutcome::Written));
        let undo = service.undo_record().unwrap();
        assert_eq!(undo.original_text, "baseline");
        assert_eq!(undo.modified_text, " café\n😀 ");
        service.backend.0.lock().unwrap().change_at_open = true;
        assert!(matches!(
            service.commit_dialog_checked(
                CheckedClipboardEffect::CopyResult,
                "new",
                "Copy",
                &current
            ),
            Err(ClipboardError::ProtectedItem)
        ));
        assert_eq!(service.undo_record(), Some(undo.clone()));
        assert_eq!(service.backend.0.lock().unwrap().writes, 1);
        {
            let mut s = service.backend.0.lock().unwrap();
            s.private = false;
        }
        for error in [
            ClipboardError::NonText,
            ClipboardError::Busy("busy".into()),
            ClipboardError::Permanent("unavailable".into()),
        ] {
            service.backend.0.lock().unwrap().guard_error = Some(error.clone());
            assert!(
                matches!(service.commit_dialog_checked(CheckedClipboardEffect::CopyResult, "new", "Copy", &current), Err(actual) if actual == error)
            );
            assert_eq!(service.undo_record(), Some(undo.clone()));
        }
        let calls = std::cell::Cell::new(0);
        let revoke = || {
            calls.set(calls.get() + 1);
            calls.get() <= 3
        };
        assert!(matches!(
            service
                .commit_dialog_checked(CheckedClipboardEffect::CopyResult, "new", "Copy", &revoke)
                .unwrap(),
            CheckedClipboardOutcome::Superseded
        ));
        assert_eq!(service.undo_record(), Some(undo));
        assert_eq!(service.backend.0.lock().unwrap().writes, 1);
        let prior_reads = service.backend.0.lock().unwrap().reads;
        let revoke_after_read = || service.backend.0.lock().unwrap().reads == prior_reads;
        assert!(matches!(
            service
                .commit_dialog_checked(
                    CheckedClipboardEffect::CopyResult,
                    "new",
                    "Copy",
                    &revoke_after_read
                )
                .unwrap(),
            CheckedClipboardOutcome::Superseded
        ));
        assert_eq!(service.backend.0.lock().unwrap().reads, prior_reads + 1);
        let prior_reads = service.backend.0.lock().unwrap().reads;
        let revoke_after_read = || service.backend.0.lock().unwrap().reads == prior_reads;
        assert!(matches!(
            service
                .commit_dialog_checked(
                    CheckedClipboardEffect::Apply {
                        baseline: "different"
                    },
                    "new",
                    "Apply",
                    &revoke_after_read
                )
                .unwrap(),
            CheckedClipboardOutcome::Superseded
        ));
        let CheckedClipboardOutcome::ReviewRequired(review) = service
            .commit_dialog_checked(
                CheckedClipboardEffect::Apply { baseline: "old" },
                "new",
                "Apply",
                &current,
            )
            .unwrap()
        else {
            panic!("expected review")
        };
        {
            let mut s = service.backend.0.lock().unwrap();
            s.seq += 1;
        }
        let CheckedClipboardOutcome::ReviewRequired(renewed) = service
            .commit_dialog_checked(
                CheckedClipboardEffect::ReviewedOverwrite { review: &review },
                "new",
                "Apply",
                &current,
            )
            .unwrap()
        else {
            panic!("expected renewed review")
        };
        assert_eq!(renewed.baseline, review.baseline);
        assert_eq!(renewed.current, review.current);
        assert!(matches!(
            service
                .commit_dialog_checked(
                    CheckedClipboardEffect::ReviewedOverwrite { review: &renewed },
                    "new",
                    "Apply",
                    &current
                )
                .unwrap(),
            CheckedClipboardOutcome::Written
        ));
        assert!(
            service
                .commit_dialog_checked(
                    CheckedClipboardEffect::CopyResult,
                    "bad\0text",
                    "Copy",
                    &current
                )
                .is_err()
        );
        assert_eq!(service.backend.0.lock().unwrap().writes, 2);
        let before = service.undo_record().unwrap();
        service.backend.0.lock().unwrap().change_at_open = true;
        assert!(matches!(
            service.undo_checked(&current),
            Err(ClipboardError::ProtectedItem)
        ));
        assert_eq!(service.undo_record(), Some(before.clone()));
        {
            let mut state = service.backend.0.lock().unwrap();
            state.private = false;
            state.text = "different ordinary text".into();
            state.seq += 1;
        }
        service.backend.0.lock().unwrap().change_seq_at_open = true;
        assert!(!service.undo_checked(&current).unwrap());
        assert_eq!(service.undo_record(), Some(before.clone()));
        let checks = std::cell::Cell::new(0);
        let revoke = || {
            checks.set(checks.get() + 1);
            checks.get() < 5
        };
        assert!(!service.undo_checked(&revoke).unwrap());
        assert_eq!(service.undo_record(), Some(before.clone()));
        assert!(service.undo_checked(&current).unwrap());
        assert_eq!(service.backend.0.lock().unwrap().text, before.original_text);
        assert!(service.undo_record().is_none());
        let reads = service.backend.0.lock().unwrap().reads;
        assert!(matches!(
            service.undo_checked(&current),
            Err(ClipboardError::NoUndo)
        ));
        assert_eq!(service.backend.0.lock().unwrap().reads, reads);
    }
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
    fn protected_read_classification_preserves_private_and_transient_states() {
        assert_eq!(
            map_protected_read(Ok(ClipboardRead::Ordinary("fixture".into()))),
            Ok("fixture".into())
        );
        assert_eq!(
            map_protected_read(Ok(ClipboardRead::Ordinary(String::new()))),
            Ok(String::new())
        );
        assert_eq!(
            map_protected_read(Ok(ClipboardRead::NonText)),
            Err(ClipboardError::NonText)
        );
        assert_eq!(
            map_protected_read(Ok(ClipboardRead::Private)),
            Err(ClipboardError::ProtectedItem)
        );
        assert_eq!(
            map_protected_read(Err(ProtectedClipboardError::Busy)),
            Err(ClipboardError::Busy(
                ProtectedClipboardError::Busy.to_string()
            ))
        );
        assert_eq!(
            map_protected_read(Err(ProtectedClipboardError::TimedOut)),
            Err(ClipboardError::TimedOut)
        );
        assert!(ClipboardError::TimedOut.is_transient());
        assert_eq!(
            map_protected_read(Err(ProtectedClipboardError::Unavailable)),
            Err(ClipboardError::Permanent(
                ProtectedClipboardError::Unavailable.to_string()
            ))
        );
    }

    #[test]
    fn timed_out_reads_retry_in_background_but_gui_attempts_once() {
        let background = svc("source");
        background
            .backend()
            .push(Op::Read(Err(ClipboardError::TimedOut)));
        background
            .backend()
            .push(Op::Read(Ok("recovered source".into())));
        assert_eq!(
            background.read_text_for_modify(),
            Ok("recovered source".into())
        );

        let interactive = svc("source");
        interactive
            .backend()
            .push(Op::Read(Err(ClipboardError::TimedOut)));
        interactive
            .backend()
            .push(Op::Read(Ok("queued retry".into())));
        assert_eq!(
            interactive.read_text_for_modify_once(),
            Err(ClipboardError::TimedOut)
        );
        assert_eq!(interactive.backend().read_text(), Ok("queued retry".into()));
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
    fn dialog_commit_read_failure_does_not_overwrite_clipboard() {
        let service = svc("baseline");
        service
            .backend()
            .push(Op::Read(Err(ClipboardError::Permanent(
                "read failed".into(),
            ))));

        assert_eq!(
            service.commit_dialog("baseline", "draft", "output", false, "dialog"),
            Err(ClipboardError::Permanent("read failed".into()))
        );
        assert!(service.backend().writes().is_empty());
        assert!(service.undo_record().is_none());
    }

    #[test]
    fn timed_out_dialog_read_preserves_clipboard_and_existing_undo() {
        let service = svc("original");
        service.commit_output("modified".into(), "setup").unwrap();
        let undo_before = service.undo_record();
        let writes_before = service.backend().writes();
        service
            .backend()
            .push(Op::Read(Err(ClipboardError::TimedOut)));

        assert_eq!(
            service.commit_dialog("modified", "draft", "new output", false, "dialog"),
            Err(ClipboardError::TimedOut)
        );
        assert_eq!(service.backend().writes(), writes_before);
        assert_eq!(service.backend().read_text().unwrap(), "modified");
        assert_eq!(service.undo_record(), undo_before);
    }

    #[test]
    fn dialog_commit_does_not_retry_busy_read_or_write() {
        let read_service = svc("baseline");
        read_service
            .backend()
            .push(Op::Read(Err(ClipboardError::Busy("read busy".into()))));
        read_service
            .backend()
            .push(Op::Read(Ok("queued read retry".into())));
        assert_eq!(
            read_service.commit_dialog("baseline", "draft", "output", false, "dialog"),
            Err(ClipboardError::Busy("read busy".into()))
        );
        assert!(read_service.backend().writes().is_empty());
        assert_eq!(
            read_service.backend().read_text().unwrap(),
            "queued read retry"
        );

        let write_service = svc("baseline");
        write_service
            .backend()
            .push(Op::Write(Err(ClipboardError::Busy("write busy".into()))));
        write_service.backend().push(Op::Write(Ok(())));
        assert_eq!(
            write_service.commit_dialog("baseline", "draft", "output", false, "dialog"),
            Err(ClipboardError::Busy("write busy".into()))
        );
        assert!(write_service.backend().writes().is_empty());
        write_service
            .backend()
            .write_text("queued write retry")
            .unwrap();
        assert_eq!(write_service.backend().writes(), ["queued write retry"]);
    }

    #[test]
    fn reviewed_dialog_recheck_does_not_retry_busy_read() {
        let service = svc("baseline");
        service.backend().push(Op::Read(Ok("baseline".into())));
        service
            .backend()
            .push(Op::Read(Err(ClipboardError::Busy("recheck busy".into()))));
        service
            .backend()
            .push(Op::Read(Ok("queued recheck retry".into())));

        assert_eq!(
            service.commit_reviewed_dialog(
                "baseline",
                "output",
                &ClipboardSummary::new("baseline"),
                "dialog",
            ),
            Err(ClipboardError::Busy("recheck busy".into()))
        );
        assert!(service.backend().writes().is_empty());
        assert_eq!(
            service.backend().read_text().unwrap(),
            "queued recheck retry"
        );
    }

    #[test]
    fn immediate_commits_and_undo_return_busy_when_serialization_lock_is_held() {
        let service = svc("baseline");
        service.commit_output("modified".into(), "setup").unwrap();
        let undo_before = service.undo_record();
        let writes_before = service.backend().writes();
        let lock_busy = ClipboardError::Busy("Another clipboard operation is in progress".into());

        let backend_busy = ClipboardError::Busy("backend busy during dialog commit".into());
        service.backend().push(Op::Read(Err(backend_busy.clone())));
        let guard = service.commit.lock().unwrap();
        assert_eq!(
            service.commit_dialog("modified", "draft", "output", false, "dialog"),
            Err(lock_busy.clone())
        );
        drop(guard);
        assert_eq!(service.backend().read_text(), Err(backend_busy));
        assert_eq!(service.backend().writes(), writes_before);
        assert_eq!(service.undo_record(), undo_before);

        let backend_busy = ClipboardError::Busy("backend busy during reviewed commit".into());
        service.backend().push(Op::Read(Err(backend_busy.clone())));
        let guard = service.commit.lock().unwrap();
        assert_eq!(
            service.commit_reviewed_dialog(
                "modified",
                "output",
                &ClipboardSummary::new("modified"),
                "reviewed dialog",
            ),
            Err(lock_busy.clone())
        );
        drop(guard);
        assert_eq!(service.backend().read_text(), Err(backend_busy));
        assert_eq!(service.backend().writes(), writes_before);
        assert_eq!(service.undo_record(), undo_before);

        let backend_busy = ClipboardError::Busy("backend busy during Undo".into());
        service.backend().push(Op::Read(Err(backend_busy.clone())));
        let guard = service.commit.lock().unwrap();
        assert_eq!(service.undo_without_retry(), Err(lock_busy));
        drop(guard);
        assert_eq!(service.backend().read_text(), Err(backend_busy));
        assert_eq!(service.backend().writes(), writes_before);
        assert_eq!(service.undo_record(), undo_before);
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
    fn failed_undo_read_does_not_overwrite_clipboard_or_consume_record() {
        let s = svc("original");
        s.commit_output("modified".into(), "op").unwrap();
        let undo = s.undo_record().unwrap();
        let writes_before = s.backend().writes();
        s.backend().push(Op::Read(Err(ClipboardError::Permanent(
            "read failed".into(),
        ))));

        assert_eq!(
            s.undo(),
            Err(ClipboardError::Permanent("read failed".into()))
        );
        assert_eq!(s.backend().writes(), writes_before);
        assert_eq!(s.backend().read_text().unwrap(), "modified");
        assert_eq!(s.undo_record(), Some(undo));
    }

    #[test]
    fn interactive_undo_does_not_retry_busy_read_or_write() {
        let read_service = svc("original");
        read_service.commit_output("modified".into(), "op").unwrap();
        read_service
            .backend()
            .push(Op::Read(Err(ClipboardError::Busy("undo read busy".into()))));
        read_service
            .backend()
            .push(Op::Read(Ok("queued undo read retry".into())));
        assert_eq!(
            read_service.undo_without_retry(),
            Err(ClipboardError::Busy("undo read busy".into()))
        );
        assert_eq!(read_service.backend().writes(), ["modified"]);
        assert!(read_service.undo_record().is_some());
        assert_eq!(
            read_service.backend().read_text().unwrap(),
            "queued undo read retry"
        );

        let write_service = svc("original");
        write_service
            .commit_output("modified".into(), "op")
            .unwrap();
        write_service
            .backend()
            .push(Op::Write(Err(ClipboardError::Busy(
                "undo write busy".into(),
            ))));
        write_service.backend().push(Op::Write(Ok(())));
        assert_eq!(
            write_service.undo_without_retry(),
            Err(ClipboardError::Busy("undo write busy".into()))
        );
        assert_eq!(write_service.backend().writes(), ["modified"]);
        assert!(write_service.undo_record().is_some());
        write_service
            .backend()
            .write_text("queued undo write retry")
            .unwrap();
        assert_eq!(
            write_service.backend().writes(),
            ["modified", "queued undo write retry"]
        );
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
    fn bounded_background_retry_terminates_under_sustained_busy() {
        struct SustainedBusyBackend {
            read_attempts: Arc<std::sync::atomic::AtomicUsize>,
            write_attempts: Arc<std::sync::atomic::AtomicUsize>,
            read_busy: bool,
        }
        impl ClipboardBackend for SustainedBusyBackend {
            fn read_text(&self) -> Result<String, ClipboardError> {
                self.read_attempts
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if self.read_busy {
                    Err(ClipboardError::Busy("read busy".into()))
                } else {
                    Ok("source".into())
                }
            }

            fn write_text(&self, _: &str) -> Result<(), ClipboardError> {
                self.write_attempts
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Err(ClipboardError::Busy("write busy".into()))
            }
        }

        let read_attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let read_service = ClipboardService::new(SustainedBusyBackend {
            read_attempts: read_attempts.clone(),
            write_attempts: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            read_busy: true,
        });
        assert!(matches!(
            read_service.read_text_for_modify(),
            Err(ClipboardError::Busy(_))
        ));
        let reads = read_attempts.load(std::sync::atomic::Ordering::Relaxed);
        assert!((1..=RETRY_ATTEMPTS).contains(&reads));

        let write_attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let write_service = ClipboardService::new(SustainedBusyBackend {
            read_attempts: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            write_attempts: write_attempts.clone(),
            read_busy: false,
        });
        assert!(matches!(
            write_service.commit_output("output".into(), "op"),
            Err(ClipboardError::Busy(_))
        ));
        let writes = write_attempts.load(std::sync::atomic::Ordering::Relaxed);
        assert!((1..=RETRY_ATTEMPTS).contains(&writes));
    }

    #[test]
    fn background_timeout_retries_remain_within_the_existing_attempt_budget() {
        struct SustainedTimeoutBackend {
            attempts: Arc<std::sync::atomic::AtomicUsize>,
        }
        impl ClipboardBackend for SustainedTimeoutBackend {
            fn read_text(&self) -> Result<String, ClipboardError> {
                self.attempts
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Err(ClipboardError::TimedOut)
            }

            fn write_text(&self, _: &str) -> Result<(), ClipboardError> {
                Ok(())
            }
        }

        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let service = ClipboardService::new(SustainedTimeoutBackend {
            attempts: attempts.clone(),
        });
        assert_eq!(
            service.read_text_for_modify(),
            Err(ClipboardError::TimedOut)
        );
        let attempts = attempts.load(std::sync::atomic::Ordering::Relaxed);
        assert!((1..=RETRY_ATTEMPTS).contains(&attempts));
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
        assert!(s.backend().writes().is_empty());
        assert_eq!(s.backend().read_text().unwrap(), "should-not-read");
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
