//! Nonblocking source/output state for the XML toolkit.
//!
//! Source edits replace a single pending slot and advance a generation. The
//! COM owner parses them sequentially; results for older generations are
//! discarded. Formatting uses that same parsed document and publishes a
//! separate output value. Callers must opt in explicitly before copying output
//! into their editor source.

use crate::xml_toolkit_engine::{MAX_XML_INPUT_BYTES, XmlEngineError, XmlFormatMode};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
#[cfg(any(windows, test))]
use std::thread;
use std::thread::JoinHandle;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct XmlSourceRevision(u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XmlSessionError {
    Engine(XmlEngineError),
    Stopped,
    StaleRevision,
    SourceNotReady,
    RevisionExhausted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XmlParseStatus {
    Empty,
    Pending,
    Ready,
    Failed(XmlEngineError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlFormatOutput {
    pub revision: XmlSourceRevision,
    pub mode: XmlFormatMode,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XmlOutputStatus {
    Empty,
    Pending(XmlFormatMode),
    Ready(Arc<XmlFormatOutput>),
    Failed(XmlEngineError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlSessionSnapshot {
    pub revision: Option<XmlSourceRevision>,
    pub parse: XmlParseStatus,
    pub output: XmlOutputStatus,
}

impl Default for XmlSessionSnapshot {
    fn default() -> Self {
        Self {
            revision: None,
            parse: XmlParseStatus::Empty,
            output: XmlOutputStatus::Empty,
        }
    }
}

struct Shared {
    mailbox: Mutex<Mailbox>,
    wake: Condvar,
    // Kept with the shared state so an owner that requests nonblocking close
    // can retain and reap the worker after it reaches a terminal state.
    join: Mutex<Option<JoinHandle<()>>>,
}

struct Mailbox {
    snapshot: Arc<XmlSessionSnapshot>,
    pending_source: Option<SourceJob>,
    pending_format: Option<FormatJob>,
    stopped: bool,
}

struct SourceJob {
    revision: XmlSourceRevision,
    input: SourceInput,
}

enum SourceInput {
    Empty,
    Text(String),
    Failed(XmlEngineError),
}

struct FormatJob {
    revision: XmlSourceRevision,
    mode: XmlFormatMode,
}

/// One bounded source/format bridge. Its mailbox retains at most one pending
/// source and one pending format request, so rapid edits do not queue one COM
/// parse or spawn a worker per keystroke. `snapshot` is cheap to poll from egui.
pub struct XmlEngineSession {
    shared: Arc<Shared>,
}

#[cfg(any(windows, test))]
trait SessionEngine: Send + 'static {
    type Document: Copy;

    fn parse(&mut self, source: String) -> Result<Self::Document, XmlEngineError>;
    fn format(
        &mut self,
        document: Self::Document,
        mode: XmlFormatMode,
    ) -> Result<String, XmlEngineError>;
    fn close(&mut self, document: Self::Document);
}

#[cfg(windows)]
impl SessionEngine for crate::xml_toolkit_engine::XmlEngineWorker {
    type Document = crate::xml_toolkit_engine::XmlDocumentId;

    fn parse(&mut self, source: String) -> Result<Self::Document, XmlEngineError> {
        crate::xml_toolkit_engine::XmlEngineWorker::parse(self, source)
    }

    fn format(
        &mut self,
        document: Self::Document,
        mode: XmlFormatMode,
    ) -> Result<String, XmlEngineError> {
        crate::xml_toolkit_engine::XmlEngineWorker::format(self, document, mode)
    }

    fn close(&mut self, document: Self::Document) {
        let _ = crate::xml_toolkit_engine::XmlEngineWorker::close_document(self, document);
    }
}

impl XmlEngineSession {
    /// Starts the single XML COM owner and its coalescing caller bridge.
    pub fn start() -> Result<Self, XmlSessionError> {
        #[cfg(windows)]
        {
            let engine = crate::xml_toolkit_engine::XmlEngineWorker::start()
                .map_err(XmlSessionError::Engine)?;
            Self::start_with_engine(engine)
        }
        #[cfg(not(windows))]
        {
            Err(XmlSessionError::Engine(XmlEngineError::UnsupportedPlatform))
        }
    }

    #[cfg(any(windows, test))]
    fn start_with_engine<E: SessionEngine>(engine: E) -> Result<Self, XmlSessionError> {
        let shared = Arc::new(Shared {
            mailbox: Mutex::new(Mailbox {
                snapshot: Arc::new(XmlSessionSnapshot::default()),
                pending_source: None,
                pending_format: None,
                stopped: false,
            }),
            wake: Condvar::new(),
            join: Mutex::new(None),
        });
        let worker_shared = Arc::clone(&shared);
        let join = thread::Builder::new()
            .name("xml-source-session".to_owned())
            .spawn(move || session_main(engine, worker_shared))
            .map_err(|_| XmlSessionError::Engine(XmlEngineError::WorkerUnavailable))?;
        *lock(&shared.join) = Some(join);
        Ok(Self { shared })
    }

    /// Replaces the active source without waiting for parsing. Older queued
    /// edits are coalesced; any in-flight stale result is discarded.
    pub fn replace_source(&self, source: String) -> Result<XmlSourceRevision, XmlSessionError> {
        let mut mailbox = lock(&self.shared.mailbox);
        if mailbox.stopped {
            return Err(XmlSessionError::Stopped);
        }
        let revision = mailbox.snapshot.revision.map_or(Ok(1), |revision| {
            revision
                .0
                .checked_add(1)
                .ok_or(XmlSessionError::RevisionExhausted)
        })?;
        let revision = XmlSourceRevision(revision);
        let (input, parse) = if source.is_empty() {
            (SourceInput::Empty, XmlParseStatus::Empty)
        } else if source.len() > MAX_XML_INPUT_BYTES {
            (
                SourceInput::Failed(XmlEngineError::InputTooLarge {
                    limit: MAX_XML_INPUT_BYTES,
                }),
                XmlParseStatus::Pending,
            )
        } else {
            (SourceInput::Text(source), XmlParseStatus::Pending)
        };
        mailbox.snapshot = Arc::new(XmlSessionSnapshot {
            revision: Some(revision),
            parse,
            output: XmlOutputStatus::Empty,
        });
        mailbox.pending_source = Some(SourceJob { revision, input });
        mailbox.pending_format = None;
        self.shared.wake.notify_one();
        Ok(revision)
    }

    /// Requests a transformation for the current parsed source. This only
    /// updates the bounded mailbox; it never waits for COM or changes source.
    pub fn format(
        &self,
        revision: XmlSourceRevision,
        mode: XmlFormatMode,
    ) -> Result<(), XmlSessionError> {
        let mut mailbox = lock(&self.shared.mailbox);
        if mailbox.stopped {
            return Err(XmlSessionError::Stopped);
        }
        if mailbox.snapshot.revision != Some(revision) {
            return Err(XmlSessionError::StaleRevision);
        }
        if mailbox.snapshot.parse != XmlParseStatus::Ready {
            return Err(XmlSessionError::SourceNotReady);
        }
        let mut snapshot = (*mailbox.snapshot).clone();
        snapshot.output = XmlOutputStatus::Pending(mode);
        mailbox.snapshot = Arc::new(snapshot);
        mailbox.pending_format = Some(FormatJob { revision, mode });
        self.shared.wake.notify_one();
        Ok(())
    }

    /// Returns an immutable, cheap-to-clone latest snapshot. Large output text
    /// remains behind an `Arc`, so polling does not duplicate it each frame.
    pub fn snapshot(&self) -> Arc<XmlSessionSnapshot> {
        Arc::clone(&lock(&self.shared.mailbox).snapshot)
    }

    /// Requests nonblocking worker shutdown. Keep this session as the owner
    /// until `try_reap` returns true, or call `shutdown` from an app-shutdown
    /// path away from egui to stop and join synchronously.
    pub fn request_shutdown(&self) {
        self.signal_shutdown();
    }

    /// Joins the worker only after it has finished, so polling this method is
    /// safe on a UI path. Returns false while the worker is still terminating.
    pub fn try_reap(&self) -> Result<bool, XmlSessionError> {
        let mut join = lock(&self.shared.join);
        let Some(handle) = join.as_ref() else {
            return Ok(true);
        };
        if !handle.is_finished() {
            return Ok(false);
        }
        let Some(handle) = join.take() else {
            return Ok(true);
        };
        handle
            .join()
            .map_err(|_| XmlSessionError::Engine(XmlEngineError::WorkerUnavailable))?;
        Ok(true)
    }

    /// Stops the worker and waits for COM cleanup. Call this away from egui's
    /// frame path; dropping the session only signals shutdown.
    pub fn shutdown(self) -> Result<(), XmlSessionError> {
        self.signal_shutdown();
        let join = lock(&self.shared.join).take();
        if let Some(join) = join {
            join.join()
                .map_err(|_| XmlSessionError::Engine(XmlEngineError::WorkerUnavailable))?;
        }
        Ok(())
    }

    fn signal_shutdown(&self) {
        let mut mailbox = lock(&self.shared.mailbox);
        mailbox.stopped = true;
        mailbox.pending_source = None;
        mailbox.pending_format = None;
        self.shared.wake.notify_one();
    }
}

impl Drop for XmlEngineSession {
    fn drop(&mut self) {
        self.signal_shutdown();
        // Drop cannot prove that shutdown has finished. Callers responsible
        // for worker lifetime must retain this owner and reap it explicitly.
    }
}

#[cfg(any(windows, test))]
fn session_main<E: SessionEngine>(mut engine: E, shared: Arc<Shared>) {
    let mut current_document: Option<(XmlSourceRevision, E::Document)> = None;
    loop {
        let action = {
            let mut mailbox = lock(&shared.mailbox);
            loop {
                if mailbox.stopped {
                    break None;
                }
                if let Some(source) = mailbox.pending_source.take() {
                    break Some(SessionAction::Source(source));
                }
                if let Some(format) = mailbox.pending_format.take() {
                    break Some(SessionAction::Format(format));
                }
                mailbox = shared
                    .wake
                    .wait(mailbox)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };

        let Some(action) = action else {
            break;
        };
        match action {
            SessionAction::Source(job) => {
                if let Some((_, document)) = current_document.take() {
                    engine.close(document);
                }
                let result = match job.input {
                    // The empty snapshot is published synchronously by
                    // replace_source; consuming this action only closes the
                    // prior document on the owning worker and never parses.
                    SourceInput::Empty => continue,
                    SourceInput::Text(source) => engine.parse(source),
                    SourceInput::Failed(error) => Err(error),
                };
                let mut mailbox = lock(&shared.mailbox);
                if mailbox.stopped || mailbox.snapshot.revision != Some(job.revision) {
                    drop(mailbox);
                    if let Ok(document) = result {
                        engine.close(document);
                    }
                    continue;
                }
                let mut snapshot = (*mailbox.snapshot).clone();
                match result {
                    Ok(document) => {
                        snapshot.parse = XmlParseStatus::Ready;
                        current_document = Some((job.revision, document));
                    }
                    Err(error) => snapshot.parse = XmlParseStatus::Failed(error),
                }
                mailbox.snapshot = Arc::new(snapshot);
            }
            SessionAction::Format(job) => {
                let Some((revision, document)) = current_document else {
                    continue;
                };
                if revision != job.revision {
                    continue;
                }
                let result = engine.format(document, job.mode);
                let mut mailbox = lock(&shared.mailbox);
                if mailbox.stopped
                    || mailbox.snapshot.revision != Some(job.revision)
                    || mailbox.snapshot.output != XmlOutputStatus::Pending(job.mode)
                {
                    continue;
                }
                let mut snapshot = (*mailbox.snapshot).clone();
                snapshot.output = match result {
                    Ok(text) => XmlOutputStatus::Ready(Arc::new(XmlFormatOutput {
                        revision: job.revision,
                        mode: job.mode,
                        text,
                    })),
                    Err(error) => XmlOutputStatus::Failed(error),
                };
                mailbox.snapshot = Arc::new(snapshot);
            }
        }
    }
    if let Some((_, document)) = current_document.take() {
        engine.close(document);
    }
    drop(engine);
}

#[cfg(any(windows, test))]
enum SessionAction {
    Source(SourceJob),
    Format(FormatJob),
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::time::{Duration, Instant};

    struct FakeEngine {
        parsed_sources: Arc<Mutex<Vec<String>>>,
        closed_documents: Arc<Mutex<Vec<u64>>>,
        next_document: u64,
        blocked_source: Option<String>,
        started: Option<Sender<()>>,
        parse_release: Option<Receiver<()>>,
    }

    impl SessionEngine for FakeEngine {
        type Document = u64;

        fn parse(&mut self, source: String) -> Result<Self::Document, XmlEngineError> {
            self.parsed_sources
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(source.clone());
            if self.blocked_source.as_deref() == Some(source.as_str()) {
                if let Some(started) = self.started.take() {
                    let _ = started.send(());
                }
                if let Some(release) = self.parse_release.take() {
                    let _ = release.recv();
                }
            }
            if source == "invalid" {
                return Err(XmlEngineError::Parse(
                    crate::xml_toolkit_engine::XmlDiagnostic {
                        line: Some(1),
                        column: Some(1),
                        message: "fake parse failure".into(),
                    },
                ));
            }
            let document = self.next_document;
            self.next_document = self.next_document.saturating_add(1);
            Ok(document)
        }

        fn format(
            &mut self,
            document: Self::Document,
            mode: XmlFormatMode,
        ) -> Result<String, XmlEngineError> {
            Ok(format!("{mode:?}:document-{document}"))
        }

        fn close(&mut self, document: Self::Document) {
            self.closed_documents
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(document);
        }
    }

    fn wait_for(
        session: &XmlEngineSession,
        mut predicate: impl FnMut(&XmlSessionSnapshot) -> bool,
    ) -> Arc<XmlSessionSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let snapshot = session.snapshot();
            if predicate(&snapshot) {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "session did not reach expected state"
            );
            thread::yield_now();
        }
    }

    fn fake_engine(
        parsed_sources: Arc<Mutex<Vec<String>>>,
        closed_documents: Arc<Mutex<Vec<u64>>>,
        blocked_source: Option<String>,
        started: Option<Sender<()>>,
        parse_release: Option<Receiver<()>>,
    ) -> FakeEngine {
        FakeEngine {
            parsed_sources,
            closed_documents,
            next_document: 1,
            blocked_source,
            started,
            parse_release,
        }
    }

    #[test]
    fn source_edits_coalesce_and_stale_parse_results_are_discarded() {
        let parsed_sources = Arc::new(Mutex::new(Vec::new()));
        let closed_documents = Arc::new(Mutex::new(Vec::new()));
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let engine = fake_engine(
            Arc::clone(&parsed_sources),
            closed_documents,
            Some("first".into()),
            Some(started_sender),
            Some(release_receiver),
        );
        let session = XmlEngineSession::start_with_engine(engine).unwrap();
        let first = session.replace_source("first".into()).unwrap();
        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("first parse starts on the session worker");
        let _coalesced = session.replace_source("middle".into()).unwrap();
        let latest = session.replace_source("latest".into()).unwrap();
        assert!(latest.0 > first.0);
        release_sender.send(()).unwrap();

        let snapshot = wait_for(&session, |snapshot| {
            snapshot.revision == Some(latest) && snapshot.parse == XmlParseStatus::Ready
        });
        assert_eq!(snapshot.output, XmlOutputStatus::Empty);
        assert_eq!(
            *parsed_sources
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec!["first".to_owned(), "latest".to_owned()]
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn format_output_stays_separate_and_pending_state_is_synchronized() {
        let parsed_sources = Arc::new(Mutex::new(Vec::new()));
        let closed_documents = Arc::new(Mutex::new(Vec::new()));
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let engine = fake_engine(
            parsed_sources,
            closed_documents,
            Some("source two".into()),
            Some(started_sender),
            Some(release_receiver),
        );
        let session = XmlEngineSession::start_with_engine(engine).unwrap();
        let first = session.replace_source("source one".into()).unwrap();
        wait_for(&session, |snapshot| snapshot.parse == XmlParseStatus::Ready);
        session.format(first, XmlFormatMode::Pretty).unwrap();
        let formatted = wait_for(&session, |snapshot| {
            matches!(&snapshot.output, XmlOutputStatus::Ready(_))
        });
        let XmlOutputStatus::Ready(output) = &formatted.output else {
            unreachable!();
        };
        assert_eq!(output.revision, first);
        assert_eq!(output.mode, XmlFormatMode::Pretty);
        assert_eq!(output.text, "Pretty:document-1");

        let second = session.replace_source("source two".into()).unwrap();
        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("second parse is held for deterministic Pending assertions");
        let pending = session.snapshot();
        assert_eq!(pending.revision, Some(second));
        assert_eq!(pending.parse, XmlParseStatus::Pending);
        assert_eq!(pending.output, XmlOutputStatus::Empty);
        assert_eq!(
            session.format(first, XmlFormatMode::Minify),
            Err(XmlSessionError::StaleRevision)
        );
        assert_eq!(
            session.format(second, XmlFormatMode::Minify),
            Err(XmlSessionError::SourceNotReady)
        );
        release_sender.send(()).unwrap();
        wait_for(&session, |snapshot| {
            snapshot.revision == Some(second) && snapshot.parse == XmlParseStatus::Ready
        });
        session.shutdown().unwrap();
    }

    #[test]
    fn empty_invalid_and_recovered_sources_clear_and_close_the_right_documents() {
        let parsed_sources = Arc::new(Mutex::new(Vec::new()));
        let closed_documents = Arc::new(Mutex::new(Vec::new()));
        let engine = fake_engine(
            Arc::clone(&parsed_sources),
            Arc::clone(&closed_documents),
            None,
            None,
            None,
        );
        let session = XmlEngineSession::start_with_engine(engine).unwrap();

        let empty = session.replace_source(String::new()).unwrap();
        let empty_snapshot = session.snapshot();
        assert_eq!(empty_snapshot.revision, Some(empty));
        assert_eq!(empty_snapshot.parse, XmlParseStatus::Empty);
        assert_eq!(empty_snapshot.output, XmlOutputStatus::Empty);
        assert!(
            parsed_sources
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
        );

        let valid_one = session.replace_source("valid one".into()).unwrap();
        wait_for(&session, |snapshot| {
            snapshot.revision == Some(valid_one) && snapshot.parse == XmlParseStatus::Ready
        });
        session.format(valid_one, XmlFormatMode::Pretty).unwrap();
        let formatted = wait_for(&session, |snapshot| {
            matches!(&snapshot.output, XmlOutputStatus::Ready(_))
        });
        let XmlOutputStatus::Ready(output) = &formatted.output else {
            unreachable!();
        };
        assert_eq!(output.text, "Pretty:document-1");

        let invalid = session.replace_source("invalid".into()).unwrap();
        let failed = wait_for(&session, |snapshot| {
            snapshot.revision == Some(invalid)
                && matches!(
                    &snapshot.parse,
                    XmlParseStatus::Failed(XmlEngineError::Parse(_))
                )
        });
        assert_eq!(failed.output, XmlOutputStatus::Empty);
        assert_eq!(
            session.format(invalid, XmlFormatMode::Pretty),
            Err(XmlSessionError::SourceNotReady)
        );
        assert_eq!(
            *closed_documents
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![1]
        );

        let valid_two = session.replace_source("valid two".into()).unwrap();
        wait_for(&session, |snapshot| {
            snapshot.revision == Some(valid_two) && snapshot.parse == XmlParseStatus::Ready
        });
        session.format(valid_two, XmlFormatMode::Minify).unwrap();
        let recovered = wait_for(&session, |snapshot| {
            matches!(&snapshot.output, XmlOutputStatus::Ready(_))
        });
        let XmlOutputStatus::Ready(output) = &recovered.output else {
            unreachable!();
        };
        assert_eq!(output.revision, valid_two);
        assert_eq!(output.mode, XmlFormatMode::Minify);
        assert_eq!(output.text, "Minify:document-2");

        session.shutdown().unwrap();
        assert_eq!(
            *closed_documents
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![1, 2]
        );
        assert_eq!(
            *parsed_sources
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec!["valid one", "invalid", "valid two"]
        );
    }

    #[test]
    fn oversized_edits_fail_without_being_queued_for_the_engine() {
        let parsed_sources = Arc::new(Mutex::new(Vec::new()));
        let closed_documents = Arc::new(Mutex::new(Vec::new()));
        let engine = fake_engine(
            Arc::clone(&parsed_sources),
            closed_documents,
            None,
            None,
            None,
        );
        let session = XmlEngineSession::start_with_engine(engine).unwrap();
        let revision = session
            .replace_source("x".repeat(MAX_XML_INPUT_BYTES + 1))
            .unwrap();
        let snapshot = wait_for(&session, |snapshot| {
            snapshot.revision == Some(revision)
                && matches!(
                    &snapshot.parse,
                    XmlParseStatus::Failed(XmlEngineError::InputTooLarge { .. })
                )
        });
        assert_eq!(snapshot.output, XmlOutputStatus::Empty);
        assert!(
            parsed_sources
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn nonblocking_shutdown_retains_and_reaps_a_busy_worker() {
        let parsed_sources = Arc::new(Mutex::new(Vec::new()));
        let closed_documents = Arc::new(Mutex::new(Vec::new()));
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let engine = fake_engine(
            parsed_sources,
            Arc::clone(&closed_documents),
            Some("held".into()),
            Some(started_sender),
            Some(release_receiver),
        );
        let session = XmlEngineSession::start_with_engine(engine).unwrap();
        session.replace_source("held".into()).unwrap();
        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("worker is inside the controlled parse");

        session.request_shutdown();
        assert!(!session.try_reap().unwrap());
        release_sender.send(()).unwrap();

        let deadline = Instant::now() + Duration::from_secs(3);
        while !session.try_reap().unwrap() {
            assert!(Instant::now() < deadline, "worker did not become reapable");
            thread::yield_now();
        }
        assert_eq!(
            *closed_documents
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![1]
        );
    }
}
