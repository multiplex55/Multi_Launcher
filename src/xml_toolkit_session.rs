//! Nonblocking source/output state for the XML toolkit.
//!
//! Source edits replace a single pending slot and advance a generation. The
//! COM owner parses them sequentially; results for older generations are
//! discarded. Formatting uses that same parsed document and publishes a
//! separate output value. Callers must opt in explicitly before copying output
//! into their editor source.

use crate::xml_toolkit_engine::{
    MAX_XML_INPUT_BYTES, MAX_XML_SCHEMA_DIAGNOSTIC_BYTES, MAX_XPATH_EXPRESSION_BYTES,
    MAX_XPATH_NAMESPACE_BYTES, XmlDocumentId, XmlEngineError, XmlFormatMode, XmlNodeDetailKind,
    XmlNodeId, XmlNodeSnapshot, XmlTreeProjection, XmlXPathResult, validate_xpath_namespaces,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
#[cfg(any(windows, test))]
use std::thread;
use std::thread::JoinHandle;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct XmlSourceRevision(u64);

#[cfg(test)]
impl XmlSourceRevision {
    pub(crate) const fn for_test(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct XmlSelectionRequestId(u64);

impl XmlSelectionRequestId {
    pub(crate) fn from_counter(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct XmlDetailRequestId(u64);

impl XmlDetailRequestId {
    pub(crate) fn from_counter(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct XmlXPathRequestId(u64);

impl XmlXPathRequestId {
    pub(crate) fn from_counter(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct XmlNamespaceContextRevision(u64);

impl XmlNamespaceContextRevision {
    pub(crate) const fn from_counter(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct XmlSchemaRevision(u64);

#[cfg(test)]
impl XmlSchemaRevision {
    pub(crate) const fn for_test(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct XmlSchemaRequestId(u64);

impl XmlSchemaRequestId {
    pub(crate) fn from_counter(value: u64) -> Self {
        Self(value)
    }
}

/// A selected local schema path. It is never resolved or opened by the UI.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct XmlSchemaPath(PathBuf);

impl XmlSchemaPath {
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XmlDetailIntent {
    Inspect,
    Copy,
}

/// Selection identity passed between the tree view and the async session.
/// It is valid only for this source revision and parsed document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XmlNodeSelection {
    pub revision: XmlSourceRevision,
    pub document: XmlDocumentId,
    pub node: XmlNodeId,
    pub request: XmlSelectionRequestId,
}

/// A detail or explicit-copy request is bound to a specific selection and
/// operation ID so late completion cannot be mistaken for a newer action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XmlNodeDetailRequest {
    pub revision: XmlSourceRevision,
    pub document: XmlDocumentId,
    pub node: XmlNodeId,
    pub selection_request: XmlSelectionRequestId,
    pub detail_request: XmlDetailRequestId,
    pub kind: XmlNodeDetailKind,
    pub intent: XmlDetailIntent,
}

/// A single XPath evaluation is bound to the parsed source, document,
/// expression generation, and complete namespace mapping used for it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlXPathQueryRequest {
    pub revision: XmlSourceRevision,
    pub document: XmlDocumentId,
    pub request: XmlXPathRequestId,
    pub expression: String,
    pub namespace_context_revision: XmlNamespaceContextRevision,
    pub namespaces: Vec<(String, String)>,
}

/// One explicit XSD validation request bound to both the parsed source and
/// the current selected schema. The request ID distinguishes repeated runs
/// against otherwise identical revisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlSchemaValidationRequest {
    pub source_revision: XmlSourceRevision,
    pub document: XmlDocumentId,
    pub schema_path: XmlSchemaPath,
    pub schema_revision: XmlSchemaRevision,
    pub request: XmlSchemaRequestId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XmlSchemaStatus {
    NotSelected,
    NotRun,
    Pending(XmlSchemaValidationRequest),
    Valid(XmlSchemaValidationRequest),
    SchemaViolation {
        request: XmlSchemaValidationRequest,
        diagnostic: crate::xml_toolkit_engine::XmlDiagnostic,
    },
    SchemaLoadParseResolveCompileError {
        request: XmlSchemaValidationRequest,
        error: XmlEngineError,
    },
    Unavailable {
        request: XmlSchemaValidationRequest,
        error: XmlEngineError,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlSchemaSessionState {
    pub selected_path: Option<XmlSchemaPath>,
    pub revision: XmlSchemaRevision,
    pub status: XmlSchemaStatus,
}

impl Default for XmlSchemaSessionState {
    fn default() -> Self {
        Self {
            selected_path: None,
            revision: XmlSchemaRevision(0),
            status: XmlSchemaStatus::NotSelected,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XmlSessionError {
    Engine(XmlEngineError),
    Stopped,
    StaleRevision,
    SourceNotReady,
    SelectionUnavailable,
    SchemaNotSelected,
    SchemaRevisionExhausted,
    SchemaRequestExhausted,
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
pub enum XmlTreeStatus {
    Empty,
    Pending,
    Ready(Arc<XmlTreeProjection>),
    Failed(XmlEngineError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlNodeDetailOutput {
    pub request: XmlNodeDetailRequest,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XmlDetailStatus {
    Empty,
    Pending(XmlNodeDetailRequest),
    Ready(Arc<XmlNodeDetailOutput>),
    Failed {
        request: XmlNodeDetailRequest,
        error: XmlEngineError,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XmlXPathStatus {
    Empty,
    NotRun,
    Pending(XmlXPathQueryRequest),
    Nodes {
        request: XmlXPathQueryRequest,
        nodes: Arc<Vec<XmlNodeSnapshot>>,
    },
    /// MSXML returns scalar XPath 1.0 values through an internal XSLT and
    /// exposes their lexical text only. Keep that honest instead of guessing
    /// whether values such as `false` or `0` are booleans or numbers.
    ScalarText {
        request: XmlXPathQueryRequest,
        text: Arc<str>,
    },
    Failed {
        request: XmlXPathQueryRequest,
        error: XmlEngineError,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlSessionSnapshot {
    pub revision: Option<XmlSourceRevision>,
    pub parse: XmlParseStatus,
    pub output: XmlOutputStatus,
    pub tree: XmlTreeStatus,
    pub detail: XmlDetailStatus,
    pub xpath: XmlXPathStatus,
    pub schema: XmlSchemaSessionState,
}

impl Default for XmlSessionSnapshot {
    fn default() -> Self {
        Self {
            revision: None,
            parse: XmlParseStatus::Empty,
            output: XmlOutputStatus::Empty,
            tree: XmlTreeStatus::Empty,
            detail: XmlDetailStatus::Empty,
            xpath: XmlXPathStatus::Empty,
            schema: XmlSchemaSessionState::default(),
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
    pending_detail: Option<XmlNodeDetailRequest>,
    pending_xpath: Option<XmlXPathQueryRequest>,
    pending_schema: Option<XmlSchemaValidationRequest>,
    next_schema_request: u64,
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

struct DetailJob {
    request: XmlNodeDetailRequest,
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

    fn parse(
        &mut self,
        source: String,
    ) -> Result<(Self::Document, Arc<XmlTreeProjection>), XmlEngineError>;
    fn format(
        &mut self,
        document: Self::Document,
        mode: XmlFormatMode,
    ) -> Result<String, XmlEngineError>;
    fn detail(
        &mut self,
        document: Self::Document,
        node: XmlNodeId,
        kind: XmlNodeDetailKind,
    ) -> Result<String, XmlEngineError>;
    fn query(
        &mut self,
        document: Self::Document,
        expression: &str,
        namespaces: &[(String, String)],
    ) -> Result<XmlXPathResult, XmlEngineError>;
    fn validate(
        &mut self,
        document: Self::Document,
        selected_schema: &Path,
    ) -> Result<(), XmlEngineError>;
    fn close(&mut self, document: Self::Document);
}

#[cfg(windows)]
impl SessionEngine for crate::xml_toolkit_engine::XmlEngineWorker {
    type Document = crate::xml_toolkit_engine::XmlDocumentId;

    fn parse(
        &mut self,
        source: String,
    ) -> Result<(Self::Document, Arc<XmlTreeProjection>), XmlEngineError> {
        crate::xml_toolkit_engine::XmlEngineWorker::parse_with_tree(self, source)
    }

    fn detail(
        &mut self,
        document: Self::Document,
        node: XmlNodeId,
        kind: XmlNodeDetailKind,
    ) -> Result<String, XmlEngineError> {
        crate::xml_toolkit_engine::XmlEngineWorker::detail(self, document, node, kind)
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

    fn query(
        &mut self,
        document: Self::Document,
        expression: &str,
        namespaces: &[(String, String)],
    ) -> Result<XmlXPathResult, XmlEngineError> {
        crate::xml_toolkit_engine::XmlEngineWorker::query(
            self,
            document,
            expression.to_owned(),
            namespaces.to_vec(),
        )
    }

    fn validate(
        &mut self,
        document: Self::Document,
        selected_schema: &Path,
    ) -> Result<(), XmlEngineError> {
        crate::xml_toolkit_engine::XmlEngineWorker::validate_selected_schema(
            self,
            document,
            selected_schema.to_path_buf(),
        )
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
                pending_detail: None,
                pending_xpath: None,
                pending_schema: None,
                next_schema_request: 0,
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
        let source_empty = source.is_empty();
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
        let tree = if matches!(&input, SourceInput::Empty) {
            XmlTreeStatus::Empty
        } else {
            XmlTreeStatus::Pending
        };
        mailbox.snapshot = Arc::new(XmlSessionSnapshot {
            revision: Some(revision),
            parse,
            output: XmlOutputStatus::Empty,
            tree,
            detail: XmlDetailStatus::Empty,
            xpath: if source_empty {
                XmlXPathStatus::Empty
            } else {
                XmlXPathStatus::NotRun
            },
            schema: XmlSchemaSessionState {
                selected_path: mailbox.snapshot.schema.selected_path.clone(),
                revision: mailbox.snapshot.schema.revision,
                status: if mailbox.snapshot.schema.selected_path.is_some() {
                    XmlSchemaStatus::NotRun
                } else {
                    XmlSchemaStatus::NotSelected
                },
            },
        });
        mailbox.pending_source = Some(SourceJob { revision, input });
        mailbox.pending_format = None;
        mailbox.pending_detail = None;
        mailbox.pending_xpath = None;
        mailbox.pending_schema = None;
        self.shared.wake.notify_one();
        Ok(revision)
    }

    /// Selects a local XSD path without touching the filesystem. A new
    /// selection immediately revokes any previous result or in-flight
    /// publication, even when the same path is selected again.
    pub fn select_schema_path(&self, path: PathBuf) -> Result<XmlSchemaRevision, XmlSessionError> {
        self.set_schema_selection(Some(XmlSchemaPath::new(path)))
    }

    /// Clears the selected local XSD and revokes any result tied to it.
    pub fn clear_schema_path(&self) -> Result<XmlSchemaRevision, XmlSessionError> {
        self.set_schema_selection(None)
    }

    fn set_schema_selection(
        &self,
        selected_path: Option<XmlSchemaPath>,
    ) -> Result<XmlSchemaRevision, XmlSessionError> {
        let mut mailbox = lock(&self.shared.mailbox);
        if mailbox.stopped {
            return Err(XmlSessionError::Stopped);
        }
        let revision = mailbox
            .snapshot
            .schema
            .revision
            .0
            .checked_add(1)
            .map(XmlSchemaRevision)
            .ok_or(XmlSessionError::SchemaRevisionExhausted)?;
        let status = if selected_path.is_some() {
            XmlSchemaStatus::NotRun
        } else {
            XmlSchemaStatus::NotSelected
        };
        let mut snapshot = (*mailbox.snapshot).clone();
        snapshot.schema = XmlSchemaSessionState {
            selected_path,
            revision,
            status,
        };
        mailbox.snapshot = Arc::new(snapshot);
        mailbox.pending_schema = None;
        self.shared.wake.notify_one();
        Ok(revision)
    }

    /// Queues an explicit validation of the currently accepted XML DOM and
    /// selected schema. No parsing, schema read, or COM call occurs here.
    pub fn validate_selected_schema(&self) -> Result<XmlSchemaValidationRequest, XmlSessionError> {
        let mut mailbox = lock(&self.shared.mailbox);
        if mailbox.stopped {
            return Err(XmlSessionError::Stopped);
        }
        let source_revision = mailbox
            .snapshot
            .revision
            .ok_or(XmlSessionError::SourceNotReady)?;
        if mailbox.snapshot.parse != XmlParseStatus::Ready {
            return Err(XmlSessionError::SourceNotReady);
        }
        let XmlTreeStatus::Ready(projection) = &mailbox.snapshot.tree else {
            return Err(XmlSessionError::SourceNotReady);
        };
        let document = projection.document;
        let schema_path = mailbox
            .snapshot
            .schema
            .selected_path
            .clone()
            .ok_or(XmlSessionError::SchemaNotSelected)?;
        let request_id = mailbox.next_schema_request;
        mailbox.next_schema_request = request_id
            .checked_add(1)
            .ok_or(XmlSessionError::SchemaRequestExhausted)?;
        let request = XmlSchemaValidationRequest {
            source_revision,
            document,
            schema_path,
            schema_revision: mailbox.snapshot.schema.revision,
            request: XmlSchemaRequestId::from_counter(request_id),
        };
        let mut snapshot = (*mailbox.snapshot).clone();
        snapshot.schema.status = XmlSchemaStatus::Pending(request.clone());
        mailbox.snapshot = Arc::new(snapshot);
        mailbox.pending_schema = Some(request.clone());
        self.shared.wake.notify_one();
        Ok(request)
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

    /// Queues an exact node-value or XML-fragment read on the COM owner.
    /// New detail requests replace older pending requests. The returned value
    /// is never copied automatically; callers must revalidate the request
    /// against their current selection before publishing it to a clipboard.
    pub fn request_detail(&self, request: XmlNodeDetailRequest) -> Result<(), XmlSessionError> {
        let mut mailbox = lock(&self.shared.mailbox);
        if mailbox.stopped {
            return Err(XmlSessionError::Stopped);
        }
        if mailbox.snapshot.revision != Some(request.revision) {
            return Err(XmlSessionError::StaleRevision);
        }
        if mailbox.snapshot.parse != XmlParseStatus::Ready {
            return Err(XmlSessionError::SourceNotReady);
        }
        let XmlTreeStatus::Ready(projection) = &mailbox.snapshot.tree else {
            return Err(XmlSessionError::SourceNotReady);
        };
        if request.node.document() != request.document
            || projection.document != request.document
            || projection.node(request.node).is_none()
        {
            return Err(XmlSessionError::SelectionUnavailable);
        }
        let mut snapshot = (*mailbox.snapshot).clone();
        snapshot.detail = XmlDetailStatus::Pending(request);
        mailbox.snapshot = Arc::new(snapshot);
        mailbox.pending_detail = Some(request);
        self.shared.wake.notify_one();
        Ok(())
    }

    /// Immediately revokes the visible result and any queued query after an
    /// expression or namespace-map edit. In-flight COM work is discarded
    /// when it returns because publication requires the exact pending request.
    /// MSXML calls cannot be interrupted mid-call; the single owner remains
    /// retained until the worker reaches a terminal state and can be reaped.
    pub fn invalidate_xpath(&self, revision: XmlSourceRevision) -> Result<(), XmlSessionError> {
        let mut mailbox = lock(&self.shared.mailbox);
        if mailbox.stopped {
            return Err(XmlSessionError::Stopped);
        }
        if mailbox.snapshot.revision != Some(revision) {
            return Err(XmlSessionError::StaleRevision);
        }
        let mut snapshot = (*mailbox.snapshot).clone();
        snapshot.xpath = if snapshot.parse == XmlParseStatus::Empty {
            XmlXPathStatus::Empty
        } else {
            XmlXPathStatus::NotRun
        };
        mailbox.snapshot = Arc::new(snapshot);
        mailbox.pending_xpath = None;
        self.shared.wake.notify_one();
        Ok(())
    }

    /// Coalesces XPath work into one pending slot. The request is a complete
    /// immutable snapshot of the expression and namespace context used by
    /// this evaluation.
    pub fn request_xpath(&self, request: XmlXPathQueryRequest) -> Result<(), XmlSessionError> {
        let mut mailbox = lock(&self.shared.mailbox);
        if mailbox.stopped {
            return Err(XmlSessionError::Stopped);
        }
        if request.expression.len() > MAX_XPATH_EXPRESSION_BYTES {
            return Err(XmlSessionError::Engine(XmlEngineError::XPath(
                "expression exceeds the size limit".into(),
            )));
        }
        validate_xpath_namespaces(&request.namespaces).map_err(XmlSessionError::Engine)?;
        if mailbox.snapshot.revision != Some(request.revision) {
            return Err(XmlSessionError::StaleRevision);
        }
        if mailbox.snapshot.parse != XmlParseStatus::Ready {
            return Err(XmlSessionError::SourceNotReady);
        }
        let XmlTreeStatus::Ready(projection) = &mailbox.snapshot.tree else {
            return Err(XmlSessionError::SourceNotReady);
        };
        if projection.document != request.document {
            return Err(XmlSessionError::SelectionUnavailable);
        }
        let mut snapshot = (*mailbox.snapshot).clone();
        snapshot.xpath = XmlXPathStatus::Pending(request.clone());
        mailbox.snapshot = Arc::new(snapshot);
        mailbox.pending_xpath = Some(request);
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
        mailbox.pending_detail = None;
        mailbox.pending_xpath = None;
        mailbox.pending_schema = None;
        mailbox.snapshot = Arc::new(XmlSessionSnapshot::default());
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
    let mut current_document: Option<(XmlSourceRevision, E::Document, XmlDocumentId)> = None;
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
                if let Some(request) = mailbox.pending_detail.take() {
                    break Some(SessionAction::Detail(DetailJob { request }));
                }
                if let Some(request) = mailbox.pending_xpath.take() {
                    break Some(SessionAction::XPath(request));
                }
                if let Some(request) = mailbox.pending_schema.take() {
                    break Some(SessionAction::Schema(request));
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
                if let Some((_, document, _)) = current_document.take() {
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
                    if let Ok((document, _)) = result {
                        engine.close(document);
                    }
                    continue;
                }
                let mut snapshot = (*mailbox.snapshot).clone();
                match result {
                    Ok((document, projection)) => {
                        snapshot.parse = XmlParseStatus::Ready;
                        let document_id = projection.document;
                        snapshot.tree = XmlTreeStatus::Ready(projection);
                        current_document = Some((job.revision, document, document_id));
                    }
                    Err(error) => {
                        snapshot.parse = XmlParseStatus::Failed(error.clone());
                        snapshot.tree = XmlTreeStatus::Failed(error);
                    }
                }
                mailbox.snapshot = Arc::new(snapshot);
            }
            SessionAction::Format(job) => {
                let Some((revision, document, _)) = current_document else {
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
            SessionAction::Detail(job) => {
                let Some((revision, document, document_id)) = current_document else {
                    continue;
                };
                let request = job.request;
                if revision != request.revision || document_id != request.document {
                    continue;
                }
                let result = engine.detail(document, request.node, request.kind);
                let mut mailbox = lock(&shared.mailbox);
                if mailbox.stopped
                    || mailbox.snapshot.revision != Some(request.revision)
                    || mailbox.snapshot.detail != XmlDetailStatus::Pending(request)
                {
                    continue;
                }
                let mut snapshot = (*mailbox.snapshot).clone();
                snapshot.detail = match result {
                    Ok(value) => {
                        XmlDetailStatus::Ready(Arc::new(XmlNodeDetailOutput { request, value }))
                    }
                    Err(error) => XmlDetailStatus::Failed { request, error },
                };
                mailbox.snapshot = Arc::new(snapshot);
            }
            SessionAction::XPath(request) => {
                let Some((revision, document, document_id)) = current_document else {
                    continue;
                };
                if revision != request.revision || document_id != request.document {
                    continue;
                }
                let result = engine.query(document, &request.expression, &request.namespaces);
                let mut mailbox = lock(&shared.mailbox);
                if mailbox.stopped
                    || mailbox.snapshot.revision != Some(request.revision)
                    || mailbox.snapshot.xpath != XmlXPathStatus::Pending(request.clone())
                {
                    continue;
                }
                let mut snapshot = (*mailbox.snapshot).clone();
                snapshot.xpath = match result {
                    Ok(XmlXPathResult::Nodes(nodes)) => XmlXPathStatus::Nodes {
                        request,
                        nodes: Arc::new(nodes),
                    },
                    Ok(XmlXPathResult::ScalarText(text)) => XmlXPathStatus::ScalarText {
                        request,
                        text: Arc::from(text),
                    },
                    Err(error) => XmlXPathStatus::Failed { request, error },
                };
                mailbox.snapshot = Arc::new(snapshot);
            }
            SessionAction::Schema(request) => {
                let Some((revision, document, document_id)) = current_document else {
                    continue;
                };
                if revision != request.source_revision || document_id != request.document {
                    continue;
                }
                {
                    let mailbox = lock(&shared.mailbox);
                    if mailbox.stopped
                        || mailbox.snapshot.revision != Some(request.source_revision)
                        || mailbox.snapshot.schema.selected_path.as_ref()
                            != Some(&request.schema_path)
                        || mailbox.snapshot.schema.revision != request.schema_revision
                        || mailbox.snapshot.schema.status
                            != XmlSchemaStatus::Pending(request.clone())
                    {
                        continue;
                    }
                }
                let result = engine
                    .validate(document, request.schema_path.as_path())
                    .map_err(bound_schema_error);
                let mut mailbox = lock(&shared.mailbox);
                if mailbox.stopped
                    || mailbox.snapshot.revision != Some(request.source_revision)
                    || mailbox.snapshot.schema.selected_path.as_ref() != Some(&request.schema_path)
                    || mailbox.snapshot.schema.revision != request.schema_revision
                    || mailbox.snapshot.schema.status != XmlSchemaStatus::Pending(request.clone())
                {
                    continue;
                }
                let mut snapshot = (*mailbox.snapshot).clone();
                snapshot.schema.status = match result {
                    Ok(()) => XmlSchemaStatus::Valid(request),
                    Err(XmlEngineError::SchemaValidation(diagnostic)) => {
                        XmlSchemaStatus::SchemaViolation {
                            request,
                            diagnostic,
                        }
                    }
                    Err(
                        error @ (XmlEngineError::UnsupportedPlatform
                        | XmlEngineError::WorkerUnavailable
                        | XmlEngineError::SchemaBackend(_)),
                    ) => XmlSchemaStatus::Unavailable { request, error },
                    Err(error) => {
                        XmlSchemaStatus::SchemaLoadParseResolveCompileError { request, error }
                    }
                };
                mailbox.snapshot = Arc::new(snapshot);
            }
        }
    }
    if let Some((_, document, _)) = current_document.take() {
        engine.close(document);
    }
    drop(engine);
}

#[cfg(any(windows, test))]
enum SessionAction {
    Source(SourceJob),
    Format(FormatJob),
    Detail(DetailJob),
    XPath(XmlXPathQueryRequest),
    Schema(XmlSchemaValidationRequest),
}

fn bound_schema_error(error: XmlEngineError) -> XmlEngineError {
    fn bound_message(message: &mut String) {
        if message.len() <= MAX_XML_SCHEMA_DIAGNOSTIC_BYTES {
            return;
        }
        const SUFFIX: &str = "… (diagnostic clipped)";
        let mut end = MAX_XML_SCHEMA_DIAGNOSTIC_BYTES.saturating_sub(SUFFIX.len());
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        message.push_str(SUFFIX);
    }

    match error {
        XmlEngineError::SchemaValidation(mut diagnostic) => {
            bound_message(&mut diagnostic.message);
            XmlEngineError::SchemaValidation(diagnostic)
        }
        XmlEngineError::SchemaLoad(mut diagnostic) => {
            bound_message(&mut diagnostic.message);
            XmlEngineError::SchemaLoad(diagnostic)
        }
        XmlEngineError::SchemaPolicy(mut message) => {
            bound_message(&mut message);
            XmlEngineError::SchemaPolicy(message)
        }
        XmlEngineError::SchemaBackend(mut message) => {
            bound_message(&mut message);
            XmlEngineError::SchemaBackend(message)
        }
        XmlEngineError::Io(mut message) => {
            bound_message(&mut message);
            XmlEngineError::Io(message)
        }
        other => other,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::xml_toolkit_tree::XmlToolkitTreeViewState;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::time::{Duration, Instant};

    struct FakeEngine {
        parsed_sources: Arc<Mutex<Vec<String>>>,
        closed_documents: Arc<Mutex<Vec<u64>>>,
        validated_schemas: Arc<Mutex<Vec<PathBuf>>>,
        next_document: u64,
        blocked_source: Option<String>,
        started: Option<Sender<()>>,
        parse_release: Option<Receiver<()>>,
        blocked_detail_node: Option<XmlNodeId>,
        detail_started: Option<Sender<()>>,
        detail_release: Option<Receiver<()>>,
        blocked_query_expression: Option<String>,
        query_started: Option<Sender<()>>,
        query_release: Option<Receiver<()>>,
        blocked_schema_path: Option<PathBuf>,
        schema_started: Option<Sender<()>>,
        schema_release: Option<Receiver<()>>,
    }

    impl SessionEngine for FakeEngine {
        type Document = u64;

        fn parse(
            &mut self,
            source: String,
        ) -> Result<(Self::Document, Arc<XmlTreeProjection>), XmlEngineError> {
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
            Ok((document, fake_projection(document)))
        }

        fn format(
            &mut self,
            document: Self::Document,
            mode: XmlFormatMode,
        ) -> Result<String, XmlEngineError> {
            Ok(format!("{mode:?}:document-{document}"))
        }

        fn detail(
            &mut self,
            document: Self::Document,
            node: XmlNodeId,
            kind: XmlNodeDetailKind,
        ) -> Result<String, XmlEngineError> {
            if self.blocked_detail_node == Some(node) {
                if let Some(started) = self.detail_started.take() {
                    let _ = started.send(());
                }
                if let Some(release) = self.detail_release.take() {
                    let _ = release.recv();
                }
            }
            Ok(format!("full-{document}-{kind:?}"))
        }

        fn query(
            &mut self,
            document: Self::Document,
            expression: &str,
            _namespaces: &[(String, String)],
        ) -> Result<XmlXPathResult, XmlEngineError> {
            if self.blocked_query_expression.as_deref() == Some(expression) {
                if let Some(started) = self.query_started.take() {
                    let _ = started.send(());
                }
                if let Some(release) = self.query_release.take() {
                    let _ = release.recv();
                }
            }
            if expression == "nodes:zero" {
                return Ok(XmlXPathResult::Nodes(Vec::new()));
            }
            if let Some(value) = expression.strip_prefix("scalar:") {
                return Ok(XmlXPathResult::ScalarText(value.to_owned()));
            }
            if let Some(message) = expression.strip_prefix("error:") {
                return Err(XmlEngineError::XPath(message.to_owned()));
            }
            let document_id = XmlDocumentId::for_test(document);
            Ok(XmlXPathResult::Nodes(vec![XmlNodeSnapshot {
                id: XmlNodeId::for_test(document_id, 2),
                kind: crate::xml_toolkit_engine::XmlTreeNodeKind::Element,
                qualified_name: "item".into(),
                value: expression.to_owned(),
            }]))
        }

        fn validate(
            &mut self,
            document: Self::Document,
            selected_schema: &Path,
        ) -> Result<(), XmlEngineError> {
            assert!(document > 0);
            self.validated_schemas
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(selected_schema.to_path_buf());
            if self.blocked_schema_path.as_deref() == Some(selected_schema) {
                if let Some(started) = self.schema_started.take() {
                    let _ = started.send(());
                }
                if let Some(release) = self.schema_release.take() {
                    let _ = release.recv();
                }
            }
            match selected_schema.file_name().and_then(|name| name.to_str()) {
                Some("violation.xsd") => Err(XmlEngineError::SchemaValidation(
                    crate::xml_toolkit_engine::XmlDiagnostic {
                        line: Some(3),
                        column: Some(9),
                        message: "fake schema violation".into(),
                    },
                )),
                Some("broken.xsd") => Err(XmlEngineError::SchemaLoad(
                    crate::xml_toolkit_engine::XmlDiagnostic {
                        line: Some(1),
                        column: Some(1),
                        message: "fake XSD syntax failure".into(),
                    },
                )),
                _ => Ok(()),
            }
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
            validated_schemas: Arc::new(Mutex::new(Vec::new())),
            next_document: 1,
            blocked_source,
            started,
            parse_release,
            blocked_detail_node: None,
            detail_started: None,
            detail_release: None,
            blocked_query_expression: None,
            query_started: None,
            query_release: None,
            blocked_schema_path: None,
            schema_started: None,
            schema_release: None,
        }
    }

    fn fake_projection(document: u64) -> Arc<XmlTreeProjection> {
        let document_id = XmlDocumentId::for_test(document);
        let root_id = XmlNodeId::for_test(document_id, 1);
        let element_id = XmlNodeId::for_test(document_id, 2);
        let attribute_id = XmlNodeId::for_test(document_id, 3);
        Arc::new(XmlTreeProjection::from_nodes(
            document_id,
            root_id,
            vec![
                crate::xml_toolkit_engine::XmlTreeNodeSnapshot {
                    id: root_id,
                    parent: None,
                    kind: crate::xml_toolkit_engine::XmlTreeNodeKind::Document,
                    qualified_name: "#document".into(),
                    local_name: "#document".into(),
                    prefix: String::new(),
                    namespace_uri: String::new(),
                    preview: String::new(),
                    preview_truncated: false,
                    attributes: Vec::new(),
                    children: vec![element_id],
                    has_more_attributes: false,
                    has_more_children: false,
                },
                crate::xml_toolkit_engine::XmlTreeNodeSnapshot {
                    id: element_id,
                    parent: Some(root_id),
                    kind: crate::xml_toolkit_engine::XmlTreeNodeKind::Element,
                    qualified_name: "item".into(),
                    local_name: "item".into(),
                    prefix: String::new(),
                    namespace_uri: String::new(),
                    preview: String::new(),
                    preview_truncated: false,
                    attributes: vec![attribute_id],
                    children: Vec::new(),
                    has_more_attributes: false,
                    has_more_children: false,
                },
                crate::xml_toolkit_engine::XmlTreeNodeSnapshot {
                    id: attribute_id,
                    parent: Some(element_id),
                    kind: crate::xml_toolkit_engine::XmlTreeNodeKind::Attribute,
                    qualified_name: "label".into(),
                    local_name: "label".into(),
                    prefix: String::new(),
                    namespace_uri: String::new(),
                    preview: "decoded & 🙂".into(),
                    preview_truncated: false,
                    attributes: Vec::new(),
                    children: Vec::new(),
                    has_more_attributes: false,
                    has_more_children: false,
                },
            ],
            None,
        ))
    }

    fn xpath_request(
        snapshot: &XmlSessionSnapshot,
        request_id: u64,
        expression: &str,
        namespaces: Vec<(String, String)>,
        namespace_context_revision: u64,
    ) -> XmlXPathQueryRequest {
        let XmlTreeStatus::Ready(projection) = &snapshot.tree else {
            panic!("XPath requests require an accepted tree projection");
        };
        XmlXPathQueryRequest {
            revision: snapshot.revision.expect("parsed source has a revision"),
            document: projection.document,
            request: XmlXPathRequestId::from_counter(request_id),
            expression: expression.to_owned(),
            namespace_context_revision: XmlNamespaceContextRevision::from_counter(
                namespace_context_revision,
            ),
            namespaces,
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
    fn m7c_stale_copy_detail_cannot_publish_after_source_replacement() {
        let parsed_sources = Arc::new(Mutex::new(Vec::new()));
        let closed_documents = Arc::new(Mutex::new(Vec::new()));
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let stale_node = XmlNodeId::for_test(XmlDocumentId::for_test(1), 3);
        let mut engine = fake_engine(
            parsed_sources,
            Arc::clone(&closed_documents),
            None,
            None,
            None,
        );
        engine.blocked_detail_node = Some(stale_node);
        engine.detail_started = Some(started_sender);
        engine.detail_release = Some(release_receiver);
        let session = XmlEngineSession::start_with_engine(engine).unwrap();

        let revision = session.replace_source("valid".into()).unwrap();
        let parsed = wait_for(&session, |snapshot| {
            snapshot.revision == Some(revision) && snapshot.parse == XmlParseStatus::Ready
        });
        let XmlTreeStatus::Ready(projection) = &parsed.tree else {
            panic!("the accepted document publishes its projection with parse success");
        };
        let request = XmlNodeDetailRequest {
            revision,
            document: projection.document,
            node: stale_node,
            selection_request: XmlSelectionRequestId::from_counter(8),
            detail_request: XmlDetailRequestId::from_counter(12),
            kind: XmlNodeDetailKind::AttributeValue,
            intent: XmlDetailIntent::Copy,
        };
        session.request_detail(request).unwrap();
        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("the explicit copy request reached the background engine");

        let empty = session.replace_source(String::new()).unwrap();
        let cleared = session.snapshot();
        assert_eq!(cleared.revision, Some(empty));
        assert_eq!(cleared.parse, XmlParseStatus::Empty);
        assert_eq!(cleared.tree, XmlTreeStatus::Empty);
        assert_eq!(cleared.detail, XmlDetailStatus::Empty);
        assert_eq!(
            session.request_detail(request),
            Err(XmlSessionError::StaleRevision)
        );

        release_sender.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while closed_documents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
        {
            assert!(
                Instant::now() < deadline,
                "stale source owner was not closed"
            );
            thread::yield_now();
        }
        assert_eq!(session.snapshot().detail, XmlDetailStatus::Empty);
        session.shutdown().unwrap();
    }

    #[test]
    fn m7c_shutdown_revokes_a_ready_explicit_copy() {
        let parsed_sources = Arc::new(Mutex::new(Vec::new()));
        let closed_documents = Arc::new(Mutex::new(Vec::new()));
        let session = XmlEngineSession::start_with_engine(fake_engine(
            parsed_sources,
            Arc::clone(&closed_documents),
            None,
            None,
            None,
        ))
        .unwrap();

        let revision = session.replace_source("valid".into()).unwrap();
        let parsed = wait_for(&session, |snapshot| {
            snapshot.revision == Some(revision) && snapshot.parse == XmlParseStatus::Ready
        });
        let XmlTreeStatus::Ready(projection) = &parsed.tree else {
            panic!("ready source publishes its tree projection");
        };
        let attribute = projection
            .nodes
            .iter()
            .find(|node| node.kind == crate::xml_toolkit_engine::XmlTreeNodeKind::Attribute)
            .expect("fake source contains an attribute")
            .id;
        let mut tree = XmlToolkitTreeViewState::default();
        tree.sync(&parsed);
        tree.select_node(attribute).unwrap();
        let request = tree
            .request_detail(XmlNodeDetailKind::AttributeValue, XmlDetailIntent::Copy)
            .unwrap();
        session.request_detail(request).unwrap();
        let ready = wait_for(&session, |snapshot| {
            matches!(
                &snapshot.detail,
                XmlDetailStatus::Ready(output) if output.request == request
            )
        });
        assert!(matches!(
            &ready.detail,
            XmlDetailStatus::Ready(output) if output.value == "full-1-AttributeValue"
        ));

        session.request_shutdown();
        let stopped = session.snapshot();
        assert_eq!(*stopped, XmlSessionSnapshot::default());
        assert!(tree.take_valid_copy_completion(&stopped).is_none());

        let deadline = Instant::now() + Duration::from_secs(3);
        while !session.try_reap().unwrap() {
            assert!(
                Instant::now() < deadline,
                "closed session worker was not reapable"
            );
            thread::yield_now();
        }
        assert_eq!(
            *closed_documents
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![1]
        );
    }

    #[test]
    fn m7d_query_statuses_preserve_zero_scalar_and_failure_distinctions() {
        let parsed_sources = Arc::new(Mutex::new(Vec::new()));
        let closed_documents = Arc::new(Mutex::new(Vec::new()));
        let (query_started_sender, query_started_receiver) = mpsc::channel();
        // If an assertion unwinds before the explicit release, dropping the
        // sender disconnects the fake worker's blocking receive and lets it exit.
        let (query_release_sender, query_release_receiver) = mpsc::channel();
        let mut engine = fake_engine(parsed_sources, closed_documents, None, None, None);
        engine.blocked_query_expression = Some("items".into());
        engine.query_started = Some(query_started_sender);
        engine.query_release = Some(query_release_receiver);
        let session = XmlEngineSession::start_with_engine(engine).unwrap();
        let revision = session.replace_source("valid".into()).unwrap();
        let parsed = wait_for(&session, |snapshot| {
            snapshot.revision == Some(revision) && snapshot.parse == XmlParseStatus::Ready
        });
        let XmlTreeStatus::Ready(projection) = &parsed.tree else {
            panic!("successful parse publishes its actual tree");
        };
        let item_id = projection.nodes[1].id;

        let oversized_expression = xpath_request(
            &parsed,
            99,
            &"x".repeat(MAX_XPATH_EXPRESSION_BYTES + 1),
            vec![],
            1,
        );
        assert!(matches!(
            session.request_xpath(oversized_expression),
            Err(XmlSessionError::Engine(XmlEngineError::XPath(message)))
                if message.contains("size limit")
        ));
        let oversized_mapping = xpath_request(
            &parsed,
            100,
            "items",
            vec![("p".into(), "u".repeat(MAX_XPATH_NAMESPACE_BYTES))],
            1,
        );
        assert!(matches!(
            session.request_xpath(oversized_mapping),
            Err(XmlSessionError::Engine(XmlEngineError::XPath(message)))
                if message.contains("size limit")
        ));
        assert_eq!(session.snapshot().xpath, XmlXPathStatus::NotRun);

        let node_request = xpath_request(&parsed, 1, "items", vec![], 1);
        session.request_xpath(node_request.clone()).unwrap();
        query_started_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("the first query is held by the fake engine before publication");
        assert_eq!(
            session.snapshot().xpath,
            XmlXPathStatus::Pending(node_request.clone())
        );
        let pending = session.snapshot();
        assert_eq!(pending.revision, Some(revision));
        assert_eq!(pending.parse, XmlParseStatus::Ready);
        assert_eq!(pending.tree, parsed.tree);
        assert_eq!(pending.output, parsed.output);
        query_release_sender
            .send(())
            .expect("release the query even if later assertions unwind");
        let nodes = wait_for(
            &session,
            |snapshot| matches!(&snapshot.xpath, XmlXPathStatus::Nodes { request, .. } if request == &node_request),
        );
        let XmlXPathStatus::Nodes {
            request,
            nodes: matched,
        } = &nodes.xpath
        else {
            panic!("node query publishes a distinct Nodes result");
        };
        assert_eq!(request, &node_request);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].id, item_id);
        assert_eq!(nodes.parse, XmlParseStatus::Ready);
        assert_eq!(nodes.output, parsed.output);

        let zero_request = xpath_request(&parsed, 2, "nodes:zero", vec![], 1);
        session.request_xpath(zero_request.clone()).unwrap();
        let zero = wait_for(
            &session,
            |snapshot| matches!(&snapshot.xpath, XmlXPathStatus::Nodes { request, .. } if request == &zero_request),
        );
        assert!(matches!(&zero.xpath, XmlXPathStatus::Nodes { nodes, .. } if nodes.is_empty()));

        for (request_id, lexical_value) in [(3, "false"), (4, "0"), (5, "")] {
            let request = xpath_request(
                &parsed,
                request_id,
                &format!("scalar:{lexical_value}"),
                vec![],
                1,
            );
            session.request_xpath(request.clone()).unwrap();
            let scalar = wait_for(
                &session,
                |snapshot| matches!(&snapshot.xpath, XmlXPathStatus::ScalarText { request: current, .. } if current == &request),
            );
            assert!(matches!(
                &scalar.xpath,
                XmlXPathStatus::ScalarText { request: current, text }
                    if current == &request && text.as_ref() == lexical_value
            ));
        }

        let failure_request = xpath_request(&parsed, 6, "error:malformed expression", vec![], 1);
        session.request_xpath(failure_request.clone()).unwrap();
        let failed = wait_for(
            &session,
            |snapshot| matches!(&snapshot.xpath, XmlXPathStatus::Failed { request, .. } if request == &failure_request),
        );
        assert!(matches!(
            &failed.xpath,
            XmlXPathStatus::Failed { error: XmlEngineError::XPath(message), .. }
                if message == "malformed expression"
        ));
        assert_eq!(failed.parse, XmlParseStatus::Ready);
        assert_eq!(failed.tree, parsed.tree);
        assert_eq!(failed.output, parsed.output);

        let next_revision = session.replace_source("replacement".into()).unwrap();
        assert_eq!(session.snapshot().xpath, XmlXPathStatus::NotRun);
        let replacement = wait_for(&session, |snapshot| {
            snapshot.revision == Some(next_revision) && snapshot.parse == XmlParseStatus::Ready
        });
        assert_eq!(replacement.xpath, XmlXPathStatus::NotRun);
        session.shutdown().unwrap();
    }

    #[test]
    fn m7d_query_edits_revoke_inflight_results_and_coalesce_pending_work() {
        let parsed_sources = Arc::new(Mutex::new(Vec::new()));
        let closed_documents = Arc::new(Mutex::new(Vec::new()));
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let mut engine = fake_engine(parsed_sources, closed_documents, None, None, None);
        engine.blocked_query_expression = Some("slow".into());
        engine.query_started = Some(started_sender);
        engine.query_release = Some(release_receiver);
        let session = XmlEngineSession::start_with_engine(engine).unwrap();
        let revision = session.replace_source("valid".into()).unwrap();
        let parsed = wait_for(&session, |snapshot| {
            snapshot.revision == Some(revision) && snapshot.parse == XmlParseStatus::Ready
        });

        let slow = xpath_request(&parsed, 1, "slow", vec![], 1);
        session.request_xpath(slow).unwrap();
        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("the old query is active on the single engine owner");
        session.invalidate_xpath(revision).unwrap();
        assert_eq!(session.snapshot().xpath, XmlXPathStatus::NotRun);

        let superseded = xpath_request(&parsed, 2, "superseded", vec![], 2);
        let latest = xpath_request(&parsed, 3, "latest", vec![("p".into(), "urn:p".into())], 3);
        session.request_xpath(superseded).unwrap();
        session.request_xpath(latest.clone()).unwrap();
        assert_eq!(
            session.snapshot().xpath,
            XmlXPathStatus::Pending(latest.clone())
        );
        release_sender.send(()).unwrap();

        let completed = wait_for(
            &session,
            |snapshot| matches!(&snapshot.xpath, XmlXPathStatus::Nodes { request, .. } if request == &latest),
        );
        assert!(matches!(
            &completed.xpath,
            XmlXPathStatus::Nodes { request, nodes }
                if request == &latest && nodes[0].value == "latest"
        ));
        assert_eq!(completed.revision, Some(revision));
        assert_eq!(completed.parse, XmlParseStatus::Ready);
        assert_eq!(completed.tree, parsed.tree);
        session.shutdown().unwrap();
    }

    #[test]
    fn m7e_schema_validation_is_explicit_revision_bound_and_preserves_other_results() {
        let parsed_sources = Arc::new(Mutex::new(Vec::new()));
        let closed_documents = Arc::new(Mutex::new(Vec::new()));
        let validated_schemas = Arc::new(Mutex::new(Vec::new()));
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let mut engine = fake_engine(
            parsed_sources,
            Arc::clone(&closed_documents),
            None,
            None,
            None,
        );
        engine.validated_schemas = Arc::clone(&validated_schemas);
        engine.blocked_schema_path = Some(PathBuf::from("slow.xsd"));
        engine.schema_started = Some(started_sender);
        engine.schema_release = Some(release_receiver);
        let session = XmlEngineSession::start_with_engine(engine).unwrap();

        assert_eq!(
            session.validate_selected_schema(),
            Err(XmlSessionError::SourceNotReady)
        );
        let invalid_revision = session.replace_source("invalid".into()).unwrap();
        wait_for(&session, |snapshot| {
            snapshot.revision == Some(invalid_revision)
                && matches!(&snapshot.parse, XmlParseStatus::Failed(_))
        });
        session
            .select_schema_path(PathBuf::from("slow.xsd"))
            .unwrap();
        assert_eq!(
            session.validate_selected_schema(),
            Err(XmlSessionError::SourceNotReady)
        );

        let first_revision = session.replace_source("first valid XML".into()).unwrap();
        let _first = wait_for(&session, |snapshot| {
            snapshot.revision == Some(first_revision) && snapshot.parse == XmlParseStatus::Ready
        });
        session.clear_schema_path().unwrap();
        assert_eq!(
            session.validate_selected_schema(),
            Err(XmlSessionError::SchemaNotSelected)
        );
        session
            .select_schema_path(PathBuf::from("slow.xsd"))
            .unwrap();
        assert!(
            validated_schemas
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty(),
            "schema validation is never implicit after source parsing"
        );
        let old_request = session.validate_selected_schema().unwrap();
        assert_eq!(
            session.snapshot().schema.status,
            XmlSchemaStatus::Pending(old_request.clone())
        );
        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("the old schema validation is active on the engine owner");

        let second_revision = session.replace_source("second valid XML".into()).unwrap();
        assert_eq!(session.snapshot().schema.status, XmlSchemaStatus::NotRun);
        assert_eq!(
            session.validate_selected_schema(),
            Err(XmlSessionError::SourceNotReady)
        );
        session
            .select_schema_path(PathBuf::from("violation.xsd"))
            .unwrap();
        assert_eq!(session.snapshot().schema.status, XmlSchemaStatus::NotRun);
        release_sender.send(()).unwrap();

        let second_parsed = wait_for(&session, |snapshot| {
            snapshot.revision == Some(second_revision) && snapshot.parse == XmlParseStatus::Ready
        });
        assert_eq!(second_parsed.schema.status, XmlSchemaStatus::NotRun);
        session
            .format(second_revision, XmlFormatMode::Pretty)
            .unwrap();
        let formatted = wait_for(&session, |snapshot| {
            matches!(&snapshot.output, XmlOutputStatus::Ready(_))
        });
        let query = xpath_request(&formatted, 10, "//item", Vec::new(), 0);
        session.request_xpath(query.clone()).unwrap();
        let second = wait_for(
            &session,
            |snapshot| matches!(&snapshot.xpath, XmlXPathStatus::Nodes { request, .. } if request == &query),
        );
        let new_request = session.validate_selected_schema().unwrap();
        let failed = wait_for(
            &session,
            |snapshot| matches!(&snapshot.schema.status, XmlSchemaStatus::SchemaViolation { request, .. } if request == &new_request),
        );
        assert!(matches!(
            &failed.schema.status,
            XmlSchemaStatus::SchemaViolation { request, diagnostic }
                if request == &new_request
                    && diagnostic.line == Some(3)
                    && diagnostic.column == Some(9)
        ));
        assert_eq!(failed.parse, second.parse);
        assert_eq!(failed.tree, second.tree);
        assert_eq!(failed.output, second.output);
        assert_eq!(failed.xpath, second.xpath);
        assert_eq!(new_request.source_revision, second_revision);
        assert_ne!(old_request.schema_revision, new_request.schema_revision);
        assert_eq!(
            *validated_schemas
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![PathBuf::from("slow.xsd"), PathBuf::from("violation.xsd")]
        );

        session
            .select_schema_path(PathBuf::from("broken.xsd"))
            .unwrap();
        let broken = session.validate_selected_schema().unwrap();
        let schema_error = wait_for(
            &session,
            |snapshot| matches!(&snapshot.schema.status, XmlSchemaStatus::SchemaLoadParseResolveCompileError { request, .. } if request == &broken),
        );
        assert!(matches!(
            &schema_error.schema.status,
            XmlSchemaStatus::SchemaLoadParseResolveCompileError {
                error: XmlEngineError::SchemaLoad(diagnostic), ..
            } if diagnostic.line == Some(1)
        ));
        assert_eq!(schema_error.parse, second.parse);
        assert_eq!(schema_error.tree, second.tree);
        assert_eq!(schema_error.output, second.output);
        assert_eq!(schema_error.xpath, second.xpath);
        session.shutdown().unwrap();
        assert_eq!(
            *closed_documents
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![1, 2]
        );
    }

    #[test]
    fn m7e_shutdown_retains_owner_until_active_schema_validation_finishes() {
        let parsed_sources = Arc::new(Mutex::new(Vec::new()));
        let closed_documents = Arc::new(Mutex::new(Vec::new()));
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let mut engine = fake_engine(
            parsed_sources,
            Arc::clone(&closed_documents),
            None,
            None,
            None,
        );
        engine.blocked_schema_path = Some(PathBuf::from("held.xsd"));
        engine.schema_started = Some(started_sender);
        engine.schema_release = Some(release_receiver);
        let session = XmlEngineSession::start_with_engine(engine).unwrap();
        let revision = session.replace_source("valid".into()).unwrap();
        wait_for(&session, |snapshot| {
            snapshot.revision == Some(revision) && snapshot.parse == XmlParseStatus::Ready
        });
        session
            .select_schema_path(PathBuf::from("held.xsd"))
            .unwrap();
        session.validate_selected_schema().unwrap();
        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("the schema validation is active on the engine owner");

        session.request_shutdown();
        assert!(!session.try_reap().unwrap());
        release_sender.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !session.try_reap().unwrap() {
            assert!(
                Instant::now() < deadline,
                "schema owner did not become reapable"
            );
            thread::yield_now();
        }
        assert_eq!(
            *closed_documents
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![1]
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
