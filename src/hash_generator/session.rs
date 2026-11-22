use super::{
    HashAlgorithm, HashComparison, HashError, HashInput, HashInputMode, HashJob, HashJobEvent,
    HashProgress, HashRequest, HashRequestId, HashResult, parse_expected_digest,
};
use crate::activity::{
    ActivityCenter, ActivityDiagnostic, ActivityIncarnation, ActivityOperation, ActivityOutcome,
    ActivityOwner, ActivityRetirement, ActivitySlot, ActivityTask,
};
use crate::clipboard_modify::clipboard::{ClipboardBackend, ClipboardError};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Shared draft and calculation state for Hash Generator frontends.
///
/// Native dialogs, focus, and presentation ownership stay in frontend adapters;
/// this session owns the single calculation slot and all source/result state.
pub struct HashSession {
    mode: HashInputMode,
    algorithm: HashAlgorithm,
    text: String,
    clipboard_text: Option<String>,
    clipboard_snapshot_revision: u64,
    file_path: Option<PathBuf>,
    expected: String,
    generation: u64,
    epoch: u64,
    current_request: Option<(u64, HashRequestId)>,
    job: Option<HashJob>,
    result: Option<HashResult>,
    error: Option<String>,
    clipboard_feedback: Option<String>,
    activity_center: Option<ActivityCenter>,
}

impl Default for HashSession {
    fn default() -> Self {
        Self {
            mode: HashInputMode::Text,
            algorithm: HashAlgorithm::Sha256,
            text: String::new(),
            clipboard_text: None,
            clipboard_snapshot_revision: 0,
            file_path: None,
            expected: String::new(),
            generation: 0,
            epoch: 0,
            current_request: None,
            job: None,
            result: None,
            error: None,
            clipboard_feedback: None,
            activity_center: None,
        }
    }
}

impl HashSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attach_activity_center(&mut self, center: ActivityCenter) {
        self.activity_center = Some(center);
    }

    /// Begin a fresh frontend session while retaining any cancelled worker
    /// until its terminal event releases the single job slot.
    pub fn begin(&mut self, path: Option<PathBuf>) {
        self.epoch = self.epoch.wrapping_add(1);
        self.invalidate_current();
        self.mode = if path.is_some() {
            HashInputMode::File
        } else {
            HashInputMode::Text
        };
        self.algorithm = HashAlgorithm::Sha256;
        self.text.clear();
        self.clear_clipboard_snapshot();
        self.file_path = path;
        self.expected.clear();
        self.error = None;
        self.clipboard_feedback = None;
    }

    /// End a frontend session and reset its draft. A retiring job remains in
    /// the slot until `poll` consumes its terminal event.
    pub fn end(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.invalidate_current();
        self.mode = HashInputMode::Text;
        self.algorithm = HashAlgorithm::Sha256;
        self.text.clear();
        self.clear_clipboard_snapshot();
        self.file_path = None;
        self.expected.clear();
        self.error = None;
        self.clipboard_feedback = None;
    }

    /// Invalidate publication during frontend shutdown without joining the
    /// detached worker.
    pub fn shutdown(&mut self) {
        self.invalidate_current();
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn mode(&self) -> HashInputMode {
        self.mode
    }

    pub fn algorithm(&self) -> HashAlgorithm {
        self.algorithm
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn clipboard_byte_len(&self) -> Option<usize> {
        self.clipboard_text.as_ref().map(String::len)
    }

    /// Opaque in-session identity for the currently retained clipboard snapshot.
    /// It contains no clipboard data and changes when the snapshot changes.
    pub fn clipboard_snapshot_revision(&self) -> Option<u64> {
        self.clipboard_text
            .as_ref()
            .map(|_| self.clipboard_snapshot_revision)
    }

    pub fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    pub fn expected(&self) -> &str {
        &self.expected
    }

    pub fn result(&self) -> Option<&HashResult> {
        self.result.as_ref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn clipboard_feedback(&self) -> Option<&str> {
        self.clipboard_feedback.as_deref()
    }

    pub fn current_request_id(&self) -> Option<HashRequestId> {
        self.current_request.map(|(_, request_id)| request_id)
    }

    pub fn has_job(&self) -> bool {
        self.job.is_some()
    }

    pub fn can_copy_digest(&self) -> bool {
        self.result.is_some() && !self.is_running()
    }

    pub fn is_running(&self) -> bool {
        self.job
            .as_ref()
            .is_some_and(|job| self.current_request == Some((self.generation, job.request_id())))
    }

    pub fn is_stopping_stale_job(&self) -> bool {
        self.job.is_some() && !self.is_running()
    }

    pub fn progress(&self) -> Option<HashProgress> {
        self.is_running()
            .then(|| self.job.as_ref().map(HashJob::progress))
            .flatten()
    }

    pub fn comparison(&self) -> Option<Result<HashComparison, String>> {
        let result = self.result.as_ref()?;
        match parse_expected_digest(result.algorithm, &self.expected) {
            Ok(None) => Some(Ok(HashComparison::NotRequested)),
            Ok(Some(_)) => {
                let trimmed = self
                    .expected
                    .trim_matches(|ch: char| ch.is_ascii_whitespace());
                Some(Ok(if result.digest.eq_ignore_ascii_case(trimmed) {
                    HashComparison::Match
                } else {
                    HashComparison::Mismatch
                }))
            }
            Err(error) => Some(Err(error.to_string())),
        }
    }

    pub fn set_mode(&mut self, mode: HashInputMode) {
        if self.mode != mode {
            self.invalidate_current();
            self.mode = mode;
        }
    }

    pub fn set_algorithm(&mut self, algorithm: HashAlgorithm) {
        if self.algorithm != algorithm {
            self.invalidate_current();
            self.algorithm = algorithm;
        }
    }

    pub fn set_text(&mut self, text: String) {
        if self.text != text {
            self.invalidate_current();
            self.text = text;
        }
    }

    pub fn set_file_path(&mut self, path: Option<PathBuf>) {
        if self.file_path != path || self.mode != HashInputMode::File {
            self.invalidate_current();
            self.mode = HashInputMode::File;
            self.file_path = path;
        }
    }

    pub fn set_expected(&mut self, expected: String) {
        if self.expected != expected {
            self.expected = expected;
            self.error = None;
            self.clipboard_feedback = None;
        }
    }

    pub fn load_clipboard(&mut self, clipboard: &dyn ClipboardBackend) {
        self.apply_clipboard_load(clipboard.read_text());
    }

    /// Apply a clipboard read performed by a frontend worker. Keeping this
    /// transition in the session ensures both frontends preserve identical
    /// snapshot, request invalidation, and feedback semantics.
    pub fn apply_clipboard_load(&mut self, result: Result<String, ClipboardError>) {
        match result {
            Ok(text) => {
                if self.mode != HashInputMode::Clipboard
                    || self.clipboard_text.as_deref() != Some(&text)
                {
                    self.invalidate_current();
                    self.mode = HashInputMode::Clipboard;
                    self.clipboard_text = Some(text);
                    self.advance_clipboard_snapshot_revision();
                }
                self.error = None;
                self.clipboard_feedback = Some("Clipboard snapshot loaded for hashing".into());
            }
            Err(error) => {
                self.error = Some(format!("Could not load clipboard: {error}"));
                self.clipboard_feedback = None;
            }
        }
    }

    pub fn calculate(&mut self, wake: Arc<dyn Fn() + Send + Sync>) -> Result<(), String> {
        let activity_center = self.activity_center.clone();
        self.calculate_with_start(wake, activity_center, |request, wake, activity| {
            HashJob::start_with_activity(request, wake, activity)
        })
    }

    pub(crate) fn calculate_with_start(
        &mut self,
        repaint: Arc<dyn Fn() + Send + Sync>,
        activity_center: Option<ActivityCenter>,
        start_job: impl FnOnce(
            HashRequest,
            Arc<dyn Fn() + Send + Sync>,
            Option<ActivityTask>,
        ) -> Result<HashJob, HashError>,
    ) -> Result<(), String> {
        if self.job.is_some() {
            return Err(
                "Wait for the cancelled hash worker to finish before starting another".into(),
            );
        }
        let input = match self.mode {
            HashInputMode::Text => HashInput::Text(self.text.clone()),
            HashInputMode::Clipboard => HashInput::Clipboard(
                self.clipboard_text
                    .clone()
                    .ok_or_else(|| "Load text from the clipboard before calculating".to_owned())?,
            ),
            HashInputMode::File => HashInput::File(
                self.file_path
                    .clone()
                    .ok_or_else(|| "Choose a file before calculating".to_owned())?,
            ),
        };
        let request =
            HashRequest::new(self.algorithm, input, None).map_err(|error| error.to_string())?;
        let request_id = request.request_id();
        let activity = activity_center.as_ref().and_then(|center| {
            center.start(
                ActivityOwner::Hash,
                ActivitySlot::Primary,
                ActivityIncarnation::new(request_id.get()),
                ActivityOperation::CalculateHash,
            )
        });
        let job = match start_job(request, repaint, activity.clone()) {
            Ok(job) => job,
            Err(error) => {
                if let Some(activity) = activity {
                    activity.finish(
                        ActivityOutcome::Failed,
                        Some(ActivityDiagnostic::WorkerUnavailable),
                        ActivityRetirement::Confirmed,
                    );
                }
                return Err(error.to_string());
            }
        };
        self.result = None;
        self.error = None;
        self.clipboard_feedback = None;
        self.current_request = Some((self.generation, request_id));
        self.job = Some(job);
        Ok(())
    }

    pub fn cancel(&mut self) {
        if self.is_running() {
            self.invalidate_current();
        }
    }

    /// Capture the current result as an immutable clipboard write request.
    /// The caller can perform the write asynchronously and later apply its
    /// outcome only if this request is still current.
    pub fn clipboard_copy_request(&self) -> Option<HashClipboardCopy> {
        let result = self.result.as_ref().filter(|_| self.can_copy_digest())?;
        Some(HashClipboardCopy {
            request_id: result.request_id,
            digest: result.digest.clone(),
        })
    }

    pub fn is_clipboard_copy_current(&self, request: &HashClipboardCopy) -> bool {
        self.can_copy_digest()
            && self.result.as_ref().is_some_and(|result| {
                result.request_id == request.request_id && result.digest == request.digest
            })
    }

    /// Apply a clipboard write result only to the exact current digest request.
    /// Failed writes preserve the digest so the user can retry or copy it by
    /// another route.
    pub fn apply_clipboard_copy(
        &mut self,
        request: &HashClipboardCopy,
        outcome: Result<(), ClipboardError>,
    ) -> bool {
        if !self.is_clipboard_copy_current(request) {
            return false;
        }
        match outcome {
            Ok(()) => {
                self.clipboard_feedback = Some("Digest copied".into());
                self.error = None;
                true
            }
            Err(error) => {
                self.clipboard_feedback = None;
                self.error = Some(format!("Could not copy digest: {error}"));
                false
            }
        }
    }

    pub fn copy_digest(&mut self, clipboard: &dyn ClipboardBackend) -> bool {
        let Some(request) = self.clipboard_copy_request() else {
            return false;
        };
        self.apply_clipboard_copy(&request, clipboard.write_text(&request.digest))
    }

    /// Polling is nonblocking. A terminal event always releases the retained
    /// worker slot, but only the matching request and generation may publish.
    pub fn poll(&mut self) -> bool {
        let Some(event) = self.job.as_mut().and_then(HashJob::poll) else {
            return false;
        };
        self.job = None;
        if self.current_request != Some((self.generation, event.request_id)) {
            return true;
        }
        self.current_request = None;
        self.accept_event(event);
        true
    }

    fn accept_event(&mut self, event: HashJobEvent) {
        match event.result {
            Ok(result) if result.request_id == event.request_id => {
                self.result = Some(result);
                self.error = None;
            }
            Ok(_) => {
                self.result = None;
                self.error = Some("Hash worker returned a mismatched request".into());
            }
            Err(super::HashError::Cancelled) => {
                self.result = None;
                self.error = Some("Hash calculation was cancelled".into());
            }
            Err(error) => {
                self.result = None;
                self.error = Some(error.to_string());
            }
        }
    }

    fn invalidate_current(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.current_request = None;
        if let Some(job) = self.job.as_ref() {
            job.cancel();
        }
        self.result = None;
        self.error = None;
        self.clipboard_feedback = None;
    }

    fn advance_clipboard_snapshot_revision(&mut self) {
        self.clipboard_snapshot_revision = self.clipboard_snapshot_revision.wrapping_add(1);
    }

    fn clear_clipboard_snapshot(&mut self) {
        if self.clipboard_text.take().is_some() {
            self.advance_clipboard_snapshot_revision();
        }
    }
}

/// An immutable, request-bound digest ready for one clipboard write. It does
/// not implement Debug so ordinary operation logging cannot expose the value.
#[derive(Clone)]
pub struct HashClipboardCopy {
    request_id: HashRequestId,
    digest: String,
}

impl HashClipboardCopy {
    pub const fn request_id(&self) -> HashRequestId {
        self.request_id
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::clipboard::ClipboardError;
    use sha2::Digest;
    use std::sync::Mutex;
    use std::thread;
    use std::time::Instant;

    struct FakeClipboard {
        read: Result<String, ClipboardError>,
        write_error: Option<String>,
        writes: Mutex<Vec<String>>,
    }

    impl Default for FakeClipboard {
        fn default() -> Self {
            Self {
                read: Ok(String::new()),
                write_error: None,
                writes: Mutex::new(Vec::new()),
            }
        }
    }

    impl ClipboardBackend for FakeClipboard {
        fn read_text(&self) -> Result<String, ClipboardError> {
            self.read.clone()
        }

        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            if let Some(error) = &self.write_error {
                return Err(ClipboardError::Permanent(error.clone()));
            }
            self.writes.lock().unwrap().push(text.to_owned());
            Ok(())
        }
    }

    fn calculate_deferred(session: &mut HashSession) -> Box<dyn FnOnce() + Send> {
        let worker_slot = Arc::new(Mutex::new(None));
        let worker_slot_for_start = worker_slot.clone();
        session
            .calculate_with_start(Arc::new(|| {}), None, move |request, repaint, _activity| {
                let (job, run_worker) = HashJob::start_deferred_for_test(request, repaint)?;
                *worker_slot_for_start
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(run_worker);
                Ok(job)
            })
            .expect("deferred hash calculation starts");

        Box::new(move || {
            let run_worker = worker_slot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
                .expect("deferred worker is available");
            run_worker();
        })
    }

    fn wait_for_terminal(session: &mut HashSession) {
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        while session.has_job() {
            session.poll();
            assert!(Instant::now() < deadline, "hash worker did not finish");
            thread::yield_now();
        }
    }

    #[test]
    fn hash_generator_expected_edits_recompare_without_rehashing_and_copy_only_current_digest() {
        let clipboard = FakeClipboard {
            read: Ok("source".into()),
            ..Default::default()
        };
        let mut session = HashSession::new();
        session.set_text("abc".into());
        session
            .calculate(Arc::new(|| {}))
            .expect("text request starts");
        wait_for_terminal(&mut session);

        let result_id = session.result().unwrap().request_id;
        assert!(session.can_copy_digest());
        session.set_expected(
            " BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD\n".into(),
        );
        assert_eq!(session.result().unwrap().request_id, result_id);
        assert_eq!(session.comparison(), Some(Ok(HashComparison::Match)));
        assert!(session.copy_digest(&clipboard));
        assert_eq!(
            *clipboard.writes.lock().unwrap(),
            ["ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"]
        );

        session.set_expected("not a digest".into());
        assert_eq!(session.result().unwrap().request_id, result_id);
        assert!(matches!(session.comparison(), Some(Err(_))));
        session.set_text("edited".into());
        assert!(session.result().is_none());
        assert!(!session.can_copy_digest());
    }

    #[test]
    fn hash_generator_source_edits_cancel_and_reject_a_stale_completion() {
        let mut session = HashSession::new();
        session.set_text("original".into());
        session
            .calculate(Arc::new(|| {}))
            .expect("text request starts");
        assert!(session.is_running());

        session.set_text("edited before completion".into());
        assert!(session.is_stopping_stale_job());
        assert!(session.result().is_none());
        assert!(!session.can_copy_digest());
        wait_for_terminal(&mut session);
        assert!(session.result().is_none());
    }

    #[test]
    fn hash_generator_clipboard_requires_explicit_load_and_read_errors_preserve_snapshot() {
        let clipboard = FakeClipboard {
            read: Err(ClipboardError::Permanent("unavailable".into())),
            ..Default::default()
        };
        let mut session = HashSession::new();
        session.set_mode(HashInputMode::Clipboard);
        assert!(session.calculate(Arc::new(|| {})).is_err());
        session.load_clipboard(&FakeClipboard {
            read: Ok("prior valid snapshot".into()),
            ..Default::default()
        });
        session.load_clipboard(&clipboard);
        assert_eq!(
            session.clipboard_text.as_deref(),
            Some("prior valid snapshot")
        );
        assert!(session.error().unwrap().contains("unavailable"));
        assert!(clipboard.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn hash_generator_clipboard_outcomes_preserve_private_snapshots_and_exact_utf8() {
        let mut session = HashSession::new();
        session.set_mode(HashInputMode::Clipboard);
        let snapshot = "line one\r\nline two · λ 🐙".to_owned();
        session.apply_clipboard_load(Ok(snapshot.clone()));
        let revision = session.clipboard_snapshot_revision();
        let generation = session.generation();
        assert_eq!(session.clipboard_byte_len(), Some(snapshot.len()));
        session
            .calculate(Arc::new(|| {}))
            .expect("UTF-8 clipboard snapshot is calculable");
        wait_for_terminal(&mut session);
        let result_id = session.result().unwrap().request_id;
        assert_eq!(
            session.result().unwrap().bytes_processed,
            snapshot.len() as u64
        );
        let expected_digest = format!("{:x}", sha2::Sha256::digest(snapshot.as_bytes()));
        assert_eq!(session.result().unwrap().digest, expected_digest);

        session.apply_clipboard_load(Err(ClipboardError::ProtectedItem));
        assert_eq!(session.clipboard_byte_len(), Some(snapshot.len()));
        assert_eq!(session.clipboard_snapshot_revision(), revision);
        assert_eq!(session.generation(), generation);
        assert!(session.error().unwrap().contains("privately"));
        assert_eq!(session.result().unwrap().request_id, result_id);

        session.apply_clipboard_load(Ok(snapshot.clone()));
        assert_eq!(session.clipboard_snapshot_revision(), revision);
        assert_eq!(session.generation(), generation);
        assert_eq!(session.result().unwrap().request_id, result_id);
    }

    #[test]
    fn hash_generator_empty_clipboard_snapshot_is_a_valid_hash_input() {
        let mut session = HashSession::new();
        session.set_mode(HashInputMode::Clipboard);
        session.apply_clipboard_load(Ok(String::new()));
        assert_eq!(session.clipboard_byte_len(), Some(0));
        assert!(session.clipboard_snapshot_revision().is_some());

        session
            .calculate(Arc::new(|| {}))
            .expect("empty clipboard snapshot is calculable");
        wait_for_terminal(&mut session);
        let result = session
            .result()
            .expect("empty clipboard digest is published");
        assert_eq!(result.input_mode, HashInputMode::Clipboard);
        assert_eq!(result.bytes_processed, 0);
    }

    #[test]
    fn hash_generator_clipboard_same_length_replacement_invalidates_result_identity() {
        let mut session = HashSession::new();
        session.set_mode(HashInputMode::Clipboard);
        session.apply_clipboard_load(Ok("first".to_owned()));
        let first_revision = session.clipboard_snapshot_revision().unwrap();
        session
            .calculate(Arc::new(|| {}))
            .expect("first clipboard request starts");
        wait_for_terminal(&mut session);
        assert!(session.can_copy_digest());

        session.apply_clipboard_load(Ok("other".to_owned()));
        assert_eq!(session.clipboard_byte_len(), Some(5));
        assert_ne!(session.clipboard_snapshot_revision(), Some(first_revision));
        assert!(session.result().is_none());
        assert!(!session.can_copy_digest());
    }

    #[test]
    fn hash_generator_async_copy_outcome_is_bound_to_the_current_digest() {
        let mut session = HashSession::new();
        session.set_text("abc".into());
        session.calculate(Arc::new(|| {})).unwrap();
        wait_for_terminal(&mut session);
        let request = session
            .clipboard_copy_request()
            .expect("current digest can be copied");
        assert_eq!(
            request.digest(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        session.set_text("changed".into());
        assert!(!session.apply_clipboard_copy(&request, Ok(())));
        assert!(session.clipboard_feedback().is_none());
        assert!(session.result().is_none());
    }

    #[test]
    fn hash_generator_new_session_resets_draft_and_retains_a_retiring_worker_slot() {
        let mut session = HashSession::new();
        session.begin(None);
        session.set_text("closing".into());
        let finish_worker = calculate_deferred(&mut session);
        session.end();
        assert_eq!(session.mode(), HashInputMode::Text);
        assert_eq!(session.text(), "");
        assert!(session.is_stopping_stale_job());

        session.begin(None);
        assert_eq!(session.text(), "");
        assert!(session.is_stopping_stale_job());
        assert!(session.calculate(Arc::new(|| {})).is_err());
        finish_worker();
        wait_for_terminal(&mut session);
        assert!(session.result().is_none());
    }

    #[test]
    fn hash_generator_lifecycle_rejects_deferred_results_after_edits_and_allows_reruns() {
        for edit in ["source", "algorithm", "mode"] {
            let mut session = HashSession::new();
            session.begin(None);
            session.set_text("before edit".into());

            let finish_old_worker = calculate_deferred(&mut session);
            let old_request_id = session.current_request_id().unwrap();
            finish_old_worker();

            match edit {
                "source" => session.set_text("after edit".into()),
                "algorithm" => session.set_algorithm(HashAlgorithm::Sha512),
                "mode" => session.load_clipboard(&FakeClipboard {
                    read: Ok("clipboard snapshot".into()),
                    ..Default::default()
                }),
                _ => unreachable!(),
            }
            assert!(session.poll());
            assert!(
                session.result().is_none(),
                "stale {edit} result was published"
            );
            assert!(
                session.error().is_none(),
                "stale {edit} error was published"
            );
            assert!(!session.can_copy_digest());

            let finish_current_worker = calculate_deferred(&mut session);
            let current_request_id = session.current_request_id().unwrap();
            assert_ne!(current_request_id, old_request_id);
            finish_current_worker();
            assert!(session.poll());
            let result = session.result().expect("current result is published");
            assert_eq!(result.request_id, current_request_id);
            assert_eq!(result.input_mode, session.mode());
            assert_eq!(result.algorithm, session.algorithm());
            assert!(session.can_copy_digest());
        }
    }

    #[test]
    fn hash_generator_file_invocation_keeps_path_mode_and_generation() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("selected-report.csv");
        std::fs::write(&path, "id,value\r\n1,λ\n").unwrap();

        let mut session = HashSession::new();
        session.begin(None);
        session.set_text("old text invocation".into());
        let finish_old_worker = calculate_deferred(&mut session);
        let old_generation = session.generation();
        finish_old_worker();

        session.begin(Some(path.clone()));
        assert!(session.generation() > old_generation);
        assert_eq!(session.mode(), HashInputMode::File);
        assert_eq!(session.file_path(), Some(path.as_path()));
        assert!(session.poll(), "the prior invocation is retired");
        assert!(session.result().is_none());

        session
            .calculate(Arc::new(|| {}))
            .expect("selected file calculation starts");
        wait_for_terminal(&mut session);
        let result = session.result().expect("file digest is available");
        assert_eq!(result.input_mode, HashInputMode::File);
        assert_eq!(
            result.bytes_processed,
            std::fs::metadata(&path).unwrap().len()
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), "id,value\r\n1,λ\n");
    }

    #[test]
    fn hash_generator_copy_failure_keeps_digest_and_reports_error() {
        let mut session = HashSession::new();
        session.begin(None);
        session.set_text("abc".into());
        session.calculate(Arc::new(|| {})).unwrap();
        wait_for_terminal(&mut session);
        let digest = session.result().unwrap().digest.clone();
        let clipboard = FakeClipboard {
            write_error: Some("clipboard busy".into()),
            ..Default::default()
        };

        assert!(!session.copy_digest(&clipboard));
        assert!(session.error().unwrap().contains("clipboard busy"));
        assert_eq!(session.result().unwrap().digest, digest);
        assert!(clipboard.writes.lock().unwrap().is_empty());
    }
}
