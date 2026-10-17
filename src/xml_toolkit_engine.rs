//! The XML engine boundary. COM objects are created, used, and dropped on the
//! dedicated worker thread; only owned Rust values cross its channel.

use std::path::PathBuf;

pub const MAX_XML_INPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_SCHEMA_FILES: usize = 32;
const MAX_SCHEMA_DEPTH: usize = 16;
const MAX_SCHEMA_FILE_BYTES: usize = 1024 * 1024;
const MAX_SCHEMA_GRAPH_BYTES: usize = 4 * 1024 * 1024;
const MAX_XPATH_RESULTS: usize = 10_000;
const MAX_XPATH_RESULT_BYTES: usize = 4 * 1024 * 1024;
const MAX_NAMESPACE_BINDINGS: usize = 64;
const MAX_NAMESPACE_BYTES: usize = 32 * 1024;
const MAX_RETAINED_DOCUMENTS: usize = 16;
const MAX_RETAINED_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
const MAX_FORMAT_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_FORMAT_INDENT_BYTES: usize = 4 * 1024 * 1024;
const MAX_FORMAT_DEPTH: usize = 256;
const MAX_FORMAT_NODES: usize = 200_000;

/// Formatting runs over a clone of the already parsed document. It never
/// changes the source tree; callers must explicitly apply returned text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XmlFormatMode {
    Pretty,
    Minify,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct XmlDocumentId(u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct XmlNodeId {
    document: XmlDocumentId,
    ordinal: u64,
}

impl XmlNodeId {
    pub fn document(self) -> XmlDocumentId {
        self.document
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlDiagnostic {
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XmlEngineError {
    UnsupportedPlatform,
    WorkerUnavailable,
    InputTooLarge { limit: usize },
    RetentionLimit(&'static str),
    UnknownDocument,
    Parse(XmlDiagnostic),
    Format(String),
    XPath(String),
    SchemaPolicy(String),
    SchemaLoad(XmlDiagnostic),
    SchemaValidation(XmlDiagnostic),
    Io(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlNodeSnapshot {
    pub id: XmlNodeId,
    pub kind: String,
    pub qualified_name: String,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XmlXPathResult {
    Nodes(Vec<XmlNodeSnapshot>),
    /// MSXML's XPath API returns node sets directly. Scalar XPath 1.0 values
    /// are evaluated through a fixed internal XSLT and intentionally remain
    /// text, because their lexical form does not reveal the XPath type.
    ScalarText(String),
}

fn validate_utf8_declaration(source: &str) -> Result<(), XmlEngineError> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let Some(after_start) = source.strip_prefix("<?xml") else {
        return Ok(());
    };
    if !after_start.chars().next().is_some_and(char::is_whitespace) {
        // This includes processing instructions such as xml-stylesheet; the
        // XML parser remains responsible for reporting malformed declarations.
        return Ok(());
    }
    let Some(end) = source.find("?>") else {
        return Ok(());
    };
    let declaration = &source[5..end];
    let bytes = declaration.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let start = cursor;
        while bytes.get(cursor).is_some_and(|byte| {
            byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b':' || *byte == b'-'
        }) {
            cursor += 1;
        }
        if start == cursor {
            break;
        }
        let name = &declaration[start..cursor];
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'=') {
            break;
        }
        cursor += 1;
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let Some(quote @ (b'\'' | b'"')) = bytes.get(cursor).copied() else {
            break;
        };
        cursor += 1;
        let value_start = cursor;
        while bytes.get(cursor).is_some_and(|byte| *byte != quote) {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&quote) {
            break;
        }
        let value = &declaration[value_start..cursor];
        cursor += 1;
        if name == "encoding" && !value.eq_ignore_ascii_case("UTF-8") {
            return Err(XmlEngineError::Parse(XmlDiagnostic {
                line: Some(1),
                column: Some(u32::try_from(value_start + 6).unwrap_or(u32::MAX)),
                message:
                    "string input is UTF-8; the XML declaration must use UTF-8 or omit encoding"
                        .into(),
            }));
        }
    }
    Ok(())
}

#[cfg(windows)]
mod windows_backend {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    use std::fs::{self, File, OpenOptions};
    use std::io::Read;
    use std::os::windows::io::AsRawHandle;
    use std::path::{Component, Path, Prefix};
    use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
    use std::thread::{self, JoinHandle};
    use windows::Win32::Data::Xml::MsXml::{
        DOMDocument60, IXMLDOMDocument2, IXMLDOMDocument3, IXMLDOMElement, IXMLDOMNode,
        IXMLDOMNodeList, IXMLDOMSchemaCollection2, NODE_CDATA_SECTION, NODE_ELEMENT, NODE_TEXT,
        XMLSchemaCache60,
    };
    use windows::Win32::Foundation::{HANDLE, VARIANT_BOOL};
    use windows::Win32::Storage::FileSystem::{GetFinalPathNameByHandleW, VOLUME_NAME_DOS};
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows::core::{BSTR, IUnknown, Interface, VARIANT};

    enum Request {
        Parse {
            source: String,
            reply: Sender<Result<XmlDocumentId, XmlEngineError>>,
        },
        Format {
            document: XmlDocumentId,
            mode: XmlFormatMode,
            reply: Sender<Result<String, XmlEngineError>>,
        },
        Query {
            document: XmlDocumentId,
            expression: String,
            namespaces: Vec<(String, String)>,
            reply: Sender<Result<XmlXPathResult, XmlEngineError>>,
        },
        Validate {
            document: XmlDocumentId,
            selected_schema: PathBuf,
            reply: Sender<Result<(), XmlEngineError>>,
        },
        CloseDocument {
            document: XmlDocumentId,
            reply: Sender<Result<(), XmlEngineError>>,
        },
        Stop,
    }

    /// A synchronous request handle backed by one COM apartment thread.
    /// Calls return owned snapshots; no COM interface leaves that thread.
    /// UI callers should submit work through `XmlEngineSession` instead.
    pub struct XmlEngineWorker {
        sender: Sender<Request>,
        join: Option<JoinHandle<()>>,
    }

    impl XmlEngineWorker {
        pub fn start() -> Result<Self, XmlEngineError> {
            let (sender, receiver) = mpsc::channel();
            let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
            let join = thread::Builder::new()
                .name("xml-engine-msxml".to_owned())
                .spawn(move || worker_main(receiver, ready_sender))
                .map_err(|_| XmlEngineError::WorkerUnavailable)?;
            if ready_receiver.recv().ok() != Some(Ok(())) {
                let _ = join.join();
                return Err(XmlEngineError::WorkerUnavailable);
            }
            Ok(Self {
                sender,
                join: Some(join),
            })
        }

        pub fn parse(&self, source: String) -> Result<XmlDocumentId, XmlEngineError> {
            self.request(|reply| Request::Parse { source, reply })
        }

        /// Formats a clone of the parsed document on the COM apartment thread.
        /// The retained source document stays available for queries/validation.
        pub fn format(
            &self,
            document: XmlDocumentId,
            mode: XmlFormatMode,
        ) -> Result<String, XmlEngineError> {
            self.request(|reply| Request::Format {
                document,
                mode,
                reply,
            })
        }

        pub fn query(
            &self,
            document: XmlDocumentId,
            expression: impl Into<String>,
            namespaces: Vec<(String, String)>,
        ) -> Result<XmlXPathResult, XmlEngineError> {
            self.request(|reply| Request::Query {
                document,
                expression: expression.into(),
                namespaces,
                reply,
            })
        }

        pub fn validate_selected_schema(
            &self,
            document: XmlDocumentId,
            selected_schema: PathBuf,
        ) -> Result<(), XmlEngineError> {
            self.request(|reply| Request::Validate {
                document,
                selected_schema,
                reply,
            })
        }

        pub fn close_document(&self, document: XmlDocumentId) -> Result<(), XmlEngineError> {
            self.request(|reply| Request::CloseDocument { document, reply })
        }

        fn request<T>(
            &self,
            build: impl FnOnce(Sender<Result<T, XmlEngineError>>) -> Request,
        ) -> Result<T, XmlEngineError> {
            let (reply, response) = mpsc::channel();
            self.sender
                .send(build(reply))
                .map_err(|_| XmlEngineError::WorkerUnavailable)?;
            response
                .recv()
                .map_err(|_| XmlEngineError::WorkerUnavailable)?
        }
    }

    impl Drop for XmlEngineWorker {
        fn drop(&mut self) {
            let _ = self.sender.send(Request::Stop);
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    struct ComApartment;

    impl ComApartment {
        fn initialize() -> Result<Self, XmlEngineError> {
            // SAFETY: this is called once at the start of the worker thread,
            // and the matching CoUninitialize runs on that same thread.
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
                .ok()
                .map_err(|_| XmlEngineError::WorkerUnavailable)?;
            Ok(Self)
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            // SAFETY: construction only succeeds after CoInitializeEx on this
            // thread, and the worker drops all COM documents before this guard.
            unsafe { CoUninitialize() };
        }
    }

    struct DocumentState {
        dom: IXMLDOMDocument2,
        source: String,
    }

    struct Engine {
        documents: BTreeMap<XmlDocumentId, DocumentState>,
        nodes: HashMap<XmlNodeId, IXMLDOMNode>,
        next_document: u64,
        next_node: u64,
        retained_source_bytes: usize,
    }

    impl Engine {
        fn new() -> Self {
            Self {
                documents: BTreeMap::new(),
                nodes: HashMap::new(),
                next_document: 1,
                next_node: 1,
                retained_source_bytes: 0,
            }
        }

        fn parse(&mut self, source: String) -> Result<XmlDocumentId, XmlEngineError> {
            check_input_size(&source)?;
            validate_utf8_declaration(&source)?;
            if self.documents.len() >= MAX_RETAINED_DOCUMENTS {
                return Err(XmlEngineError::RetentionLimit("document count"));
            }
            if self.retained_source_bytes.saturating_add(source.len()) > MAX_RETAINED_DOCUMENT_BYTES
            {
                return Err(XmlEngineError::RetentionLimit("document storage"));
            }
            let dom = secure_document(&source, XmlErrorContext::Xml)?;
            let id = XmlDocumentId(self.next_document);
            self.next_document = self.next_document.saturating_add(1);
            self.retained_source_bytes = self.retained_source_bytes.saturating_add(source.len());
            self.documents.insert(id, DocumentState { dom, source });
            Ok(id)
        }

        fn format(
            &self,
            document: XmlDocumentId,
            mode: XmlFormatMode,
        ) -> Result<String, XmlEngineError> {
            let state = self
                .documents
                .get(&document)
                .ok_or(XmlEngineError::UnknownDocument)?;
            format_existing_document(&state.dom, mode)
        }

        fn close_document(&mut self, document: XmlDocumentId) -> Result<(), XmlEngineError> {
            let state = self
                .documents
                .remove(&document)
                .ok_or(XmlEngineError::UnknownDocument)?;
            self.retained_source_bytes = self
                .retained_source_bytes
                .saturating_sub(state.source.len());
            self.nodes.retain(|id, _| id.document != document);
            Ok(())
        }

        fn query(
            &mut self,
            document: XmlDocumentId,
            expression: &str,
            namespaces: &[(String, String)],
        ) -> Result<XmlXPathResult, XmlEngineError> {
            if expression.len() > 16 * 1024 {
                return Err(XmlEngineError::XPath(
                    "expression exceeds the size limit".into(),
                ));
            }
            if contains_document_function(expression) {
                return Err(XmlEngineError::SchemaPolicy(
                    "the XSLT document() function is disabled".into(),
                ));
            }
            let state = self
                .documents
                .get(&document)
                .ok_or(XmlEngineError::UnknownDocument)?;
            set_xpath_namespaces(&state.dom, namespaces)?;
            let list = match unsafe { state.dom.selectNodes(&BSTR::from(expression)) } {
                Ok(list) => list,
                Err(_) => {
                    return scalar_xpath(&state.dom, expression, namespaces)
                        .map(XmlXPathResult::ScalarText);
                }
            };
            let count = unsafe { list.length() }
                .map_err(|_| XmlEngineError::XPath("MSXML could not count XPath results".into()))?;
            if self.nodes.len().saturating_add(count.max(0) as usize) > MAX_XPATH_RESULTS {
                return Err(XmlEngineError::RetentionLimit("XPath node identities"));
            }
            let nodes = snapshot_nodes(document, &list, &mut self.next_node, &mut self.nodes)?;
            Ok(XmlXPathResult::Nodes(nodes))
        }

        fn validate(
            &self,
            document: XmlDocumentId,
            selected_schema: &Path,
        ) -> Result<(), XmlEngineError> {
            let state = self
                .documents
                .get(&document)
                .ok_or(XmlEngineError::UnknownDocument)?;
            let mut snapshot = SecureSchemaSnapshot::read(selected_schema)?;
            let (root_schema, root_namespace, imports) = snapshot.prepare()?;
            let cache = unsafe {
                CoCreateInstance::<_, IXMLDOMSchemaCollection2>(
                    &XMLSchemaCache60,
                    None,
                    CLSCTX_INPROC_SERVER,
                )
            }
            .map_err(|_| {
                XmlEngineError::SchemaLoad(XmlDiagnostic {
                    line: None,
                    column: None,
                    message: "MSXML 6 schema cache could not be created".into(),
                })
            })?;
            unsafe { cache.SetvalidateOnLoad(VARIANT_BOOL(0)) }.map_err(|_| {
                XmlEngineError::SchemaPolicy(
                    "MSXML could not disable schema validation on load".into(),
                )
            })?;
            for (namespace, schema) in imports {
                add_schema_dom(&cache, &namespace, &schema)?;
            }
            add_schema_dom(&cache, &root_namespace, &root_schema)?;

            let validation_dom = secure_document(&state.source, XmlErrorContext::Xml)?;
            let unknown = IUnknown::from(cache.clone());
            unsafe { validation_dom.putref_schemas(&VARIANT::from(unknown)) }.map_err(|_| {
                XmlEngineError::SchemaPolicy(
                    "MSXML could not attach the selected local schema".into(),
                )
            })?;
            let parse_error = unsafe { validation_dom.validate() }.map_err(|_| {
                XmlEngineError::SchemaLoad(XmlDiagnostic {
                    line: None,
                    column: None,
                    message: "MSXML could not validate against the selected local XSD".into(),
                })
            })?;
            let code = unsafe { parse_error.errorCode() }.unwrap_or(1);
            if code == 0 {
                Ok(())
            } else {
                Err(XmlEngineError::SchemaValidation(XmlDiagnostic {
                    line: u32::try_from(unsafe { parse_error.line() }.unwrap_or(0))
                        .ok()
                        .filter(|value| *value > 0),
                    column: u32::try_from(unsafe { parse_error.linepos() }.unwrap_or(0))
                        .ok()
                        .filter(|value| *value > 0),
                    message: unsafe { parse_error.reason() }
                        .map(|reason| reason.to_string())
                        .unwrap_or_else(|_| "XML does not satisfy the selected XSD".into()),
                }))
            }
        }
    }

    fn worker_main(receiver: Receiver<Request>, ready: SyncSender<Result<(), ()>>) {
        let apartment = match ComApartment::initialize() {
            Ok(apartment) => apartment,
            Err(_) => {
                let _ = ready.send(Err(()));
                return;
            }
        };
        let _ = ready.send(Ok(()));
        let mut engine = Engine::new();
        while let Ok(request) = receiver.recv() {
            match request {
                Request::Parse { source, reply } => {
                    let _ = reply.send(engine.parse(source));
                }
                Request::Format {
                    document,
                    mode,
                    reply,
                } => {
                    let _ = reply.send(engine.format(document, mode));
                }
                Request::Query {
                    document,
                    expression,
                    namespaces,
                    reply,
                } => {
                    let _ = reply.send(engine.query(document, &expression, &namespaces));
                }
                Request::Validate {
                    document,
                    selected_schema,
                    reply,
                } => {
                    let _ = reply.send(engine.validate(document, &selected_schema));
                }
                Request::CloseDocument { document, reply } => {
                    let _ = reply.send(engine.close_document(document));
                }
                Request::Stop => break,
            }
        }
        // Drop all apartment-bound interfaces before balancing CoInitializeEx.
        drop(engine);
        drop(apartment);
    }

    #[derive(Clone, Copy)]
    enum XmlErrorContext {
        Xml,
        Schema,
    }

    fn secure_document(
        source: &str,
        context: XmlErrorContext,
    ) -> Result<IXMLDOMDocument2, XmlEngineError> {
        check_input_size(source)?;
        let document = unsafe {
            CoCreateInstance::<_, IXMLDOMDocument2>(&DOMDocument60, None, CLSCTX_INPROC_SERVER)
        }
        .map_err(|_| XmlEngineError::WorkerUnavailable)?;

        // Every security setting is mandatory. Failure to apply any setting
        // aborts the operation before any XML text is parsed.
        unsafe { document.Setasync(VARIANT_BOOL(0)) }.map_err(|_| policy_error(context))?;
        unsafe { document.SetvalidateOnParse(VARIANT_BOOL(0)) }
            .map_err(|_| policy_error(context))?;
        unsafe { document.SetresolveExternals(VARIANT_BOOL(0)) }
            .map_err(|_| policy_error(context))?;
        unsafe { document.SetpreserveWhiteSpace(VARIANT_BOOL(-1)) }
            .map_err(|_| policy_error(context))?;
        unsafe { document.setProperty(&BSTR::from("ProhibitDTD"), &VARIANT::from(true)) }
            .map_err(|_| policy_error(context))?;
        unsafe { document.setProperty(&BSTR::from("AllowXsltScript"), &VARIANT::from(false)) }
            .map_err(|_| policy_error(context))?;
        unsafe {
            document.setProperty(&BSTR::from("AllowDocumentFunction"), &VARIANT::from(false))
        }
        .map_err(|_| policy_error(context))?;

        let loaded = unsafe { document.loadXML(&BSTR::from(source)) }
            .map_err(|_| parse_error(&document, context))?;
        if loaded.0 == 0 {
            return Err(parse_error(&document, context));
        }
        Ok(document)
    }

    fn policy_error(context: XmlErrorContext) -> XmlEngineError {
        let message = "MSXML refused a required secure parser setting".to_owned();
        match context {
            XmlErrorContext::Xml => XmlEngineError::SchemaPolicy(message),
            XmlErrorContext::Schema => XmlEngineError::SchemaLoad(XmlDiagnostic {
                line: None,
                column: None,
                message,
            }),
        }
    }

    fn parse_error(document: &IXMLDOMDocument2, context: XmlErrorContext) -> XmlEngineError {
        let diagnostic = unsafe { document.parseError() }
            .ok()
            .map(|error| XmlDiagnostic {
                line: u32::try_from(unsafe { error.line() }.unwrap_or(0))
                    .ok()
                    .filter(|value| *value > 0),
                column: u32::try_from(unsafe { error.linepos() }.unwrap_or(0))
                    .ok()
                    .filter(|value| *value > 0),
                message: unsafe { error.reason() }
                    .map(|reason| reason.to_string())
                    .unwrap_or_else(|_| "MSXML could not parse the XML input".into()),
            })
            .unwrap_or(XmlDiagnostic {
                line: None,
                column: None,
                message: "MSXML could not parse the XML input".into(),
            });
        match context {
            XmlErrorContext::Xml => XmlEngineError::Parse(diagnostic),
            XmlErrorContext::Schema => XmlEngineError::SchemaLoad(diagnostic),
        }
    }

    fn check_input_size(source: &str) -> Result<(), XmlEngineError> {
        if source.len() > MAX_XML_INPUT_BYTES {
            Err(XmlEngineError::InputTooLarge {
                limit: MAX_XML_INPUT_BYTES,
            })
        } else {
            Ok(())
        }
    }

    fn set_xpath_namespaces(
        document: &IXMLDOMDocument2,
        namespaces: &[(String, String)],
    ) -> Result<(), XmlEngineError> {
        let mut bindings = String::new();
        if namespaces.len() > MAX_NAMESPACE_BINDINGS {
            return Err(XmlEngineError::XPath(
                "namespace binding count limit exceeded".into(),
            ));
        }
        for (prefix, uri) in namespaces {
            if !valid_prefix(prefix) || prefix == "xml" || prefix == "xmlns" {
                return Err(XmlEngineError::XPath(
                    "invalid XPath namespace prefix".into(),
                ));
            }
            bindings.push_str("xmlns:");
            bindings.push_str(prefix);
            bindings.push_str("=\"");
            bindings.push_str(&escape_xml_attribute(uri));
            bindings.push_str("\" ");
            if bindings.len() > MAX_NAMESPACE_BYTES {
                return Err(XmlEngineError::XPath(
                    "namespace binding size limit exceeded".into(),
                ));
            }
        }
        unsafe { document.setProperty(&BSTR::from("SelectionLanguage"), &VARIANT::from("XPath")) }
            .map_err(|_| XmlEngineError::XPath("MSXML XPath 1.0 is unavailable".into()))?;
        unsafe {
            document.setProperty(
                &BSTR::from("SelectionNamespaces"),
                &VARIANT::from(bindings.as_str()),
            )
        }
        .map_err(|_| XmlEngineError::XPath("MSXML rejected the namespace map".into()))
    }

    fn valid_prefix(prefix: &str) -> bool {
        let mut chars = prefix.chars();
        chars
            .next()
            .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
            && chars.all(|ch| ch == '_' || ch == '-' || ch == '.' || ch.is_ascii_alphanumeric())
    }

    fn escape_xml_attribute(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
    }

    fn contains_document_function(expression: &str) -> bool {
        let mut quote = None;
        let mut chars = expression.char_indices().peekable();
        while let Some((index, ch)) = chars.next() {
            if let Some(active) = quote {
                if ch == active {
                    quote = None;
                }
                continue;
            }
            if ch == '\'' || ch == '"' {
                quote = Some(ch);
                continue;
            }
            if ch.is_ascii_alphabetic() || ch == '_' {
                let start = index;
                let mut end = index + ch.len_utf8();
                while let Some((next_index, next)) = chars.peek().copied() {
                    if next.is_ascii_alphanumeric() || next == '_' || next == '-' || next == '.' {
                        chars.next();
                        end = next_index + next.len_utf8();
                    } else {
                        break;
                    }
                }
                if expression[start..end].eq_ignore_ascii_case("document") {
                    let rest = expression[end..].trim_start();
                    if rest.starts_with('(') {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn scalar_xpath(
        document: &IXMLDOMDocument2,
        expression: &str,
        namespaces: &[(String, String)],
    ) -> Result<String, XmlEngineError> {
        let mut stylesheet_namespaces = String::new();
        for (prefix, uri) in namespaces {
            if !valid_prefix(prefix) || prefix == "xml" || prefix == "xmlns" {
                return Err(XmlEngineError::XPath(
                    "invalid XPath namespace prefix".into(),
                ));
            }
            stylesheet_namespaces.push_str(" xmlns:");
            stylesheet_namespaces.push_str(prefix);
            stylesheet_namespaces.push_str("=\"");
            stylesheet_namespaces.push_str(&escape_xml_attribute(uri));
            stylesheet_namespaces.push('"');
        }
        let stylesheet = format!(
            "<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"{stylesheet_namespaces}><xsl:output method=\"text\"/><xsl:template match=\"/\"><xsl:value-of select=\"{}\"/></xsl:template></xsl:stylesheet>",
            escape_xml_attribute(expression)
        );
        let stylesheet = secure_document(&stylesheet, XmlErrorContext::Xml).map_err(|_| {
            XmlEngineError::XPath("MSXML rejected the scalar XPath expression".into())
        })?;
        let root = unsafe { stylesheet.documentElement() }
            .map_err(|_| XmlEngineError::XPath("MSXML could not prepare scalar XPath".into()))?;
        unsafe { document.transformNode(&root) }
            .map(|value| value.to_string())
            .map_err(|_| XmlEngineError::XPath("MSXML rejected the scalar XPath expression".into()))
    }

    fn snapshot_nodes(
        document: XmlDocumentId,
        list: &IXMLDOMNodeList,
        next_node: &mut u64,
        identities: &mut HashMap<XmlNodeId, IXMLDOMNode>,
    ) -> Result<Vec<XmlNodeSnapshot>, XmlEngineError> {
        let count = unsafe { list.length() }
            .map_err(|_| XmlEngineError::XPath("MSXML could not count XPath results".into()))?;
        if usize::try_from(count).unwrap_or(0) > MAX_XPATH_RESULTS {
            return Err(XmlEngineError::XPath(
                "XPath result count limit exceeded".into(),
            ));
        }
        let mut result = Vec::with_capacity(count.max(0) as usize);
        let mut pending_identities = Vec::with_capacity(count.max(0) as usize);
        let mut next_ordinal = *next_node;
        let mut result_bytes = 0usize;
        for index in 0..count {
            let node = unsafe { list.get_item(index) }
                .map_err(|_| XmlEngineError::XPath("MSXML could not read XPath result".into()))?;
            let id = XmlNodeId {
                document,
                ordinal: next_ordinal,
            };
            next_ordinal = next_ordinal.saturating_add(1);
            let value = unsafe { node.text() }
                .map(|value| value.to_string())
                .unwrap_or_default();
            result_bytes = result_bytes.saturating_add(value.len());
            if result_bytes > MAX_XPATH_RESULT_BYTES {
                return Err(XmlEngineError::RetentionLimit("XPath result bytes"));
            }
            let kind = unsafe { node.nodeTypeString() }
                .map(|value| value.to_string())
                .unwrap_or_default();
            result_bytes = result_bytes.saturating_add(kind.len());
            if result_bytes > MAX_XPATH_RESULT_BYTES {
                return Err(XmlEngineError::RetentionLimit("XPath result bytes"));
            }
            let qualified_name = unsafe { node.nodeName() }
                .map(|value| value.to_string())
                .unwrap_or_default();
            result_bytes = result_bytes.saturating_add(qualified_name.len());
            if result_bytes > MAX_XPATH_RESULT_BYTES {
                return Err(XmlEngineError::RetentionLimit("XPath result bytes"));
            }
            result.push(XmlNodeSnapshot {
                id,
                kind,
                qualified_name,
                value,
            });
            pending_identities.push((id, node));
        }
        *next_node = next_ordinal;
        identities.extend(pending_identities);
        Ok(result)
    }

    fn format_existing_document(
        document: &IXMLDOMDocument2,
        mode: XmlFormatMode,
    ) -> Result<String, XmlEngineError> {
        // Clone the parsed tree instead of parsing its retained source again.
        // Formatting only mutates this clone, so XPath and schema requests keep
        // addressing the authoritative source document.
        let clone = unsafe { document.cloneNode(VARIANT_BOOL(-1)) }
            .map_err(|_| XmlEngineError::Format("MSXML could not clone the XML document".into()))?
            .cast::<IXMLDOMDocument2>()
            .map_err(|_| {
                XmlEngineError::Format("MSXML returned an invalid document clone".into())
            })?;
        let root: IXMLDOMNode = clone.cast().map_err(|_| {
            XmlEngineError::Format("MSXML returned an invalid document clone".into())
        })?;
        let mut visited = 0usize;
        let children = format_children(&root, &mut visited)?;
        let mut indent_bytes = 0usize;
        for child in children {
            if unsafe { child.nodeType() }.map_err(|_| {
                XmlEngineError::Format("MSXML could not identify an XML node".into())
            })? == NODE_ELEMENT
            {
                format_element(
                    &clone,
                    &child,
                    0,
                    false,
                    false,
                    mode,
                    &mut visited,
                    &mut indent_bytes,
                )?;
            }
        }
        let output = unsafe { clone.xml() }
            .map_err(|_| {
                XmlEngineError::Format("MSXML could not serialize the XML document".into())
            })?
            .to_string();
        if output.len() > MAX_FORMAT_OUTPUT_BYTES {
            return Err(XmlEngineError::Format(format!(
                "formatted output exceeds the {MAX_FORMAT_OUTPUT_BYTES}-byte limit"
            )));
        }
        Ok(output)
    }

    fn format_element(
        document: &IXMLDOMDocument2,
        element: &IXMLDOMNode,
        depth: usize,
        inherited_space_preserve: bool,
        protected_by_ancestor: bool,
        mode: XmlFormatMode,
        visited: &mut usize,
        indent_bytes: &mut usize,
    ) -> Result<(), XmlEngineError> {
        if depth > MAX_FORMAT_DEPTH {
            return Err(XmlEngineError::Format(format!(
                "element nesting exceeds the {MAX_FORMAT_DEPTH}-level formatting limit"
            )));
        }
        let preserve = xml_space_preserve(element, inherited_space_preserve)?;
        let children = format_children(element, visited)?;
        let mut has_character_data = false;
        for child in &children {
            let node_type = unsafe { child.nodeType() }.map_err(|_| {
                XmlEngineError::Format("MSXML could not identify an XML node".into())
            })?;
            has_character_data |= node_type == NODE_TEXT || node_type == NODE_CDATA_SECTION;
        }
        // A mixed-content ancestor must protect every nested subtree: adding
        // indentation several levels below its direct text would still alter
        // that ancestor's string value. `xml:space` is carried independently
        // so a descendant's explicit `default` can resume formatting.
        let protect_descendants = protected_by_ancestor || has_character_data;
        for child in &children {
            if unsafe { child.nodeType() }.map_err(|_| {
                XmlEngineError::Format("MSXML could not identify an XML node".into())
            })? == NODE_ELEMENT
            {
                format_element(
                    document,
                    child,
                    depth + 1,
                    preserve,
                    protect_descendants,
                    mode,
                    visited,
                    indent_bytes,
                )?;
            }
        }

        if mode == XmlFormatMode::Pretty
            && !protected_by_ancestor
            && !preserve
            && !children.is_empty()
            && !has_character_data
        {
            // Existing text and CDATA nodes are never trimmed or rewritten.
            // New indentation is added only where the parsed parent has no
            // character data, so mixed content and whitespace-only text remain
            // byte-for-byte represented in the cloned tree.
            let before_each = 1usize.saturating_add(2usize.saturating_mul(depth + 1));
            let trailing = 1usize.saturating_add(2usize.saturating_mul(depth));
            let added = before_each
                .saturating_mul(children.len())
                .saturating_add(trailing);
            *indent_bytes = (*indent_bytes).saturating_add(added);
            if *indent_bytes > MAX_FORMAT_INDENT_BYTES {
                return Err(XmlEngineError::Format(format!(
                    "pretty indentation exceeds the {MAX_FORMAT_INDENT_BYTES}-byte limit"
                )));
            }
            for child in &children {
                let whitespace = format!("\n{}", "  ".repeat(depth + 1));
                let text =
                    unsafe { document.createTextNode(&BSTR::from(whitespace)) }.map_err(|_| {
                        XmlEngineError::Format("MSXML could not create indentation".into())
                    })?;
                let reference = VARIANT::from(IUnknown::from(child.clone()));
                unsafe { element.insertBefore(&text, &reference) }.map_err(|_| {
                    XmlEngineError::Format("MSXML could not insert indentation".into())
                })?;
            }
            let trailing_text = format!("\n{}", "  ".repeat(depth));
            let text = unsafe { document.createTextNode(&BSTR::from(trailing_text)) }
                .map_err(|_| XmlEngineError::Format("MSXML could not create indentation".into()))?;
            unsafe { element.appendChild(&text) }
                .map_err(|_| XmlEngineError::Format("MSXML could not finish indentation".into()))?;
        }
        Ok(())
    }

    fn format_children(
        parent: &IXMLDOMNode,
        visited: &mut usize,
    ) -> Result<Vec<IXMLDOMNode>, XmlEngineError> {
        let children = unsafe { parent.childNodes() }
            .map_err(|_| XmlEngineError::Format("MSXML could not inspect XML children".into()))?;
        let count = unsafe { children.length() }
            .map_err(|_| XmlEngineError::Format("MSXML could not count XML children".into()))?;
        let count = usize::try_from(count)
            .map_err(|_| XmlEngineError::Format("MSXML returned an invalid child count".into()))?;
        *visited = (*visited).saturating_add(count);
        if *visited > MAX_FORMAT_NODES {
            return Err(XmlEngineError::Format(format!(
                "document exceeds the {MAX_FORMAT_NODES}-node formatting limit"
            )));
        }
        let mut result = Vec::with_capacity(count);
        for index in 0..count {
            result.push(
                unsafe { children.get_item(index as i32) }.map_err(|_| {
                    XmlEngineError::Format("MSXML could not read an XML child".into())
                })?,
            );
        }
        Ok(result)
    }

    fn xml_space_preserve(element: &IXMLDOMNode, inherited: bool) -> Result<bool, XmlEngineError> {
        let attributes = unsafe { element.attributes() }
            .map_err(|_| XmlEngineError::Format("MSXML could not inspect xml:space".into()))?;
        let count = unsafe { attributes.length() }.map_err(|_| {
            XmlEngineError::Format("MSXML could not count element attributes".into())
        })?;
        for index in 0..count {
            let attribute = unsafe { attributes.get_item(index) }.map_err(|_| {
                XmlEngineError::Format("MSXML could not read an element attribute".into())
            })?;
            if unsafe { attribute.nodeName() }
                .map(|name| name.to_string())
                .ok()
                .as_deref()
                != Some("xml:space")
            {
                continue;
            }
            let value = unsafe { attribute.text() }
                .map(|value| value.to_string())
                .map_err(|_| XmlEngineError::Format("MSXML could not read xml:space".into()))?;
            return Ok(match value.as_str() {
                "preserve" => true,
                "default" => false,
                _ => inherited,
            });
        }
        Ok(inherited)
    }

    struct SecureSchemaSnapshot {
        root: PathBuf,
        root_relative: PathBuf,
        files: BTreeMap<PathBuf, IXMLDOMDocument2>,
        total_bytes: usize,
    }

    impl SecureSchemaSnapshot {
        fn read(selected_schema: &Path) -> Result<Self, XmlEngineError> {
            reject_network_or_device_path(selected_schema)?;
            reject_reparse_components(selected_schema)?;
            let canonical_schema = selected_schema
                .canonicalize()
                .map_err(|error| XmlEngineError::Io(error.to_string()))?;
            let root = canonical_schema
                .parent()
                .ok_or_else(|| XmlEngineError::SchemaPolicy("XSD must be a local file".into()))?
                .to_path_buf();
            reject_network_or_device_path(&root)?;
            reject_reparse_components(&root)?;
            let root_relative = canonical_schema
                .strip_prefix(&root)
                .map_err(|_| {
                    XmlEngineError::SchemaPolicy("selected XSD is outside its root".into())
                })?
                .to_path_buf();
            let mut snapshot = Self {
                root,
                root_relative: root_relative.clone(),
                files: BTreeMap::new(),
                total_bytes: 0,
            };
            snapshot.read_file(root_relative, 0)?;
            Ok(snapshot)
        }

        fn read_file(&mut self, relative: PathBuf, depth: usize) -> Result<(), XmlEngineError> {
            if depth > MAX_SCHEMA_DEPTH {
                return Err(XmlEngineError::SchemaPolicy(
                    "XSD dependency depth limit exceeded".into(),
                ));
            }
            if self.files.contains_key(&relative) {
                return Ok(());
            }
            if self.files.len() >= MAX_SCHEMA_FILES {
                return Err(XmlEngineError::SchemaPolicy(
                    "XSD file count limit exceeded".into(),
                ));
            }
            let requested = self.root.join(&relative);
            reject_reparse_components(&requested)?;
            let canonical = requested
                .canonicalize()
                .map_err(|error| XmlEngineError::Io(error.to_string()))?;
            reject_network_or_device_path(&canonical)?;
            if !canonical.starts_with(&self.root) {
                return Err(XmlEngineError::SchemaPolicy(
                    "XSD reference escapes the selected folder".into(),
                ));
            }
            let bytes = read_verified_schema_file(&canonical, &self.root)?;
            self.total_bytes = self.total_bytes.saturating_add(bytes.len());
            if self.total_bytes > MAX_SCHEMA_GRAPH_BYTES {
                return Err(XmlEngineError::SchemaPolicy(
                    "XSD graph size limit exceeded".into(),
                ));
            }
            let source = std::str::from_utf8(&bytes)
                .map_err(|_| XmlEngineError::SchemaPolicy("XSD files must use UTF-8".into()))?;
            let dom = secure_document(source, XmlErrorContext::Schema)?;
            let root = schema_root(&dom)?;
            if unsafe { root.namespaceURI() }
                .map(|namespace| namespace.to_string())
                .ok()
                .as_deref()
                != Some("http://www.w3.org/2001/XMLSchema")
                || unsafe { root.baseName() }
                    .map(|name| name.to_string())
                    .ok()
                    .as_deref()
                    != Some("schema")
            {
                return Err(XmlEngineError::SchemaPolicy(
                    "XSD root must be an XML Schema document".into(),
                ));
            }
            let base_query =
                unsafe { dom.selectNodes(&BSTR::from("//*[@xml:base]")) }.map_err(|_| {
                    XmlEngineError::SchemaPolicy("MSXML could not inspect XSD base URIs".into())
                })?;
            if unsafe { base_query.length() }.unwrap_or(0) > 0 {
                return Err(XmlEngineError::SchemaPolicy(
                    "XSD xml:base is not allowed".into(),
                ));
            }
            set_xpath_namespaces(
                &dom,
                &[("xs".into(), "http://www.w3.org/2001/XMLSchema".into())],
            )?;
            let references = unsafe {
                dom.selectNodes(&BSTR::from("//xs:include/@schemaLocation | //xs:import/@schemaLocation | //xs:redefine/@schemaLocation"))
            }
            .map_err(|_| XmlEngineError::SchemaPolicy("MSXML could not inspect XSD dependencies".into()))?;
            self.files.insert(relative.clone(), dom);
            let count = unsafe { references.length() }.map_err(|_| {
                XmlEngineError::SchemaPolicy("MSXML could not count XSD dependencies".into())
            })?;
            for index in 0..count {
                let node = unsafe { references.get_item(index) }.map_err(|_| {
                    XmlEngineError::SchemaPolicy("MSXML could not read XSD dependency".into())
                })?;
                let location = unsafe { node.text() }
                    .map(|value| value.to_string())
                    .map_err(|_| {
                        XmlEngineError::SchemaPolicy("XSD dependency location is unreadable".into())
                    })?;
                let child = local_schema_reference(&relative, &location)?;
                self.read_file(child, depth + 1)?;
            }
            Ok(())
        }

        fn prepare(
            &mut self,
        ) -> Result<(IXMLDOMDocument2, String, Vec<(String, IXMLDOMDocument2)>), XmlEngineError>
        {
            let mut active = BTreeSet::new();
            let mut imports = Vec::new();
            let mut imported_namespaces = HashMap::new();
            let root_schema = self.compile_file(
                &self.root_relative.clone(),
                &mut active,
                &mut imports,
                &mut imported_namespaces,
                0,
            )?;
            let namespace = schema_target_namespace(&root_schema)?;
            Ok((root_schema, namespace, imports))
        }

        fn compile_file(
            &mut self,
            relative: &Path,
            active: &mut BTreeSet<PathBuf>,
            imports: &mut Vec<(String, IXMLDOMDocument2)>,
            imported_namespaces: &mut HashMap<String, PathBuf>,
            depth: usize,
        ) -> Result<IXMLDOMDocument2, XmlEngineError> {
            if depth > MAX_SCHEMA_DEPTH || !active.insert(relative.to_path_buf()) {
                return Err(XmlEngineError::SchemaPolicy(
                    "cyclic or overdeep XSD dependencies are not supported".into(),
                ));
            }
            let document = self.files.get(relative).cloned().ok_or_else(|| {
                XmlEngineError::SchemaPolicy(
                    "XSD dependency was not read into the secure graph".into(),
                )
            })?;
            let root = schema_root(&document)?;
            let namespace = schema_target_namespace(&document)?;
            let references = unsafe {
                document.selectNodes(&BSTR::from("/*/xs:include | /*/xs:import | /*/xs:redefine"))
            }
            .map_err(|_| {
                XmlEngineError::SchemaPolicy("MSXML could not inspect XSD declarations".into())
            })?;
            let count = unsafe { references.length() }.map_err(|_| {
                XmlEngineError::SchemaPolicy("MSXML could not count XSD declarations".into())
            })?;
            let mut declarations = Vec::with_capacity(count.max(0) as usize);
            for index in 0..count {
                declarations.push(unsafe { references.get_item(index) }.map_err(|_| {
                    XmlEngineError::SchemaPolicy("MSXML could not read XSD declaration".into())
                })?);
            }
            let mut included_files = BTreeSet::new();
            for declaration in declarations {
                let kind = unsafe { declaration.baseName() }
                    .map(|name| name.to_string())
                    .map_err(|_| {
                        XmlEngineError::SchemaPolicy("XSD declaration name is unreadable".into())
                    })?;
                let element = declaration.cast::<IXMLDOMElement>().map_err(|_| {
                    XmlEngineError::SchemaPolicy("XSD declaration is not an element".into())
                })?;
                match kind.as_str() {
                    "include" => {
                        let location =
                            element_attribute(&element, "schemaLocation")?.ok_or_else(|| {
                                XmlEngineError::SchemaPolicy(
                                    "XSD include requires a local schemaLocation".into(),
                                )
                            })?;
                        let child_path = local_schema_reference(relative, &location)?;
                        if !included_files.insert(child_path.clone()) {
                            return Err(XmlEngineError::SchemaPolicy(
                                "duplicate XSD include is not supported".into(),
                            ));
                        }
                        let child_document = self.compile_file(
                            &child_path,
                            active,
                            imports,
                            imported_namespaces,
                            depth + 1,
                        )?;
                        let child_root = schema_root(&child_document)?;
                        if schema_target_namespace(&child_document)? != namespace {
                            return Err(XmlEngineError::SchemaPolicy(
                                "included XSD must use the same target namespace".into(),
                            ));
                        }
                        if has_direct_schema_declaration(&child_document, "import")? {
                            return Err(XmlEngineError::SchemaPolicy("imports inside included XSDs must be declared at the selected schema root".into()));
                        }
                        merge_include_settings(&root, &child_root)?;
                        let children = unsafe { child_root.selectNodes(&BSTR::from("*")) }
                            .map_err(|_| {
                                XmlEngineError::SchemaPolicy(
                                    "could not inspect included XSD declarations".into(),
                                )
                            })?;
                        let child_count = unsafe { children.length() }.map_err(|_| {
                            XmlEngineError::SchemaPolicy(
                                "could not count included XSD declarations".into(),
                            )
                        })?;
                        for child_index in 0..child_count {
                            let child =
                                unsafe { children.get_item(child_index) }.map_err(|_| {
                                    XmlEngineError::SchemaPolicy(
                                        "could not read included XSD declaration".into(),
                                    )
                                })?;
                            let local_name = unsafe { child.baseName() }
                                .map(|name| name.to_string())
                                .unwrap_or_default();
                            if local_name == "annotation" {
                                continue;
                            }
                            if matches!(local_name.as_str(), "include" | "import" | "redefine") {
                                return Err(XmlEngineError::SchemaPolicy(
                                    "nested XSD composition could not be flattened safely".into(),
                                ));
                            }
                            let import_document =
                                document.cast::<IXMLDOMDocument3>().map_err(|_| {
                                    XmlEngineError::SchemaPolicy(
                                        "MSXML cannot securely import an XSD declaration".into(),
                                    )
                                })?;
                            let imported =
                                unsafe { import_document.importNode(&child, VARIANT_BOOL(-1)) }
                                    .map_err(|_| {
                                        XmlEngineError::SchemaPolicy(
                                            "could not import a secure XSD declaration".into(),
                                        )
                                    })?;
                            unsafe { root.appendChild(&imported) }.map_err(|_| {
                                XmlEngineError::SchemaPolicy(
                                    "could not combine a local XSD include".into(),
                                )
                            })?;
                        }
                        unsafe { root.removeChild(&declaration) }.map_err(|_| {
                            XmlEngineError::SchemaPolicy(
                                "could not remove a resolved XSD include".into(),
                            )
                        })?;
                    }
                    "import" => {
                        let location = element_attribute(&element, "schemaLocation")?
                            .ok_or_else(|| XmlEngineError::SchemaPolicy("XSD import requires an explicitly selected local schemaLocation".into()))?;
                        let expected_namespace =
                            element_attribute(&element, "namespace")?.unwrap_or_default();
                        let child_path = local_schema_reference(relative, &location)?;
                        let child_document = self.compile_file(
                            &child_path,
                            active,
                            imports,
                            imported_namespaces,
                            depth + 1,
                        )?;
                        let child_namespace = schema_target_namespace(&child_document)?;
                        if child_namespace != expected_namespace || child_namespace == namespace {
                            return Err(XmlEngineError::SchemaPolicy(
                                "XSD import namespace does not match its local schema".into(),
                            ));
                        }
                        match imported_namespaces.get(&child_namespace) {
                            Some(existing) if existing != &child_path => {
                                return Err(XmlEngineError::SchemaPolicy(
                                    "multiple local XSD files declare the same imported namespace"
                                        .into(),
                                ));
                            }
                            Some(_) => {}
                            None => {
                                imported_namespaces.insert(child_namespace.clone(), child_path);
                                imports.push((child_namespace, child_document));
                            }
                        }
                        unsafe { element.removeAttribute(&BSTR::from("schemaLocation")) }.map_err(
                            |_| {
                                XmlEngineError::SchemaPolicy(
                                    "could not remove the resolved XSD import location".into(),
                                )
                            },
                        )?;
                    }
                    "redefine" => {
                        return Err(XmlEngineError::SchemaPolicy(
                            "XSD redefine is not supported by the secure local resolver".into(),
                        ));
                    }
                    _ => {}
                }
            }
            active.remove(relative);
            Ok(document)
        }
    }

    fn read_verified_schema_file(
        path: &Path,
        approved_root: &Path,
    ) -> Result<Vec<u8>, XmlEngineError> {
        if !windows_path_is_within(path, approved_root) {
            return Err(XmlEngineError::SchemaPolicy(
                "XSD file is outside the selected schema root".into(),
            ));
        }
        reject_reparse_components(path)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let file = options
            .open(path)
            .map_err(|error| XmlEngineError::Io(error.to_string()))?;
        let metadata = file
            .metadata()
            .map_err(|error| XmlEngineError::Io(error.to_string()))?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || windows_reparse_point(&metadata)
        {
            return Err(XmlEngineError::SchemaPolicy(
                "XSD handle is not a regular local file".into(),
            ));
        }
        let opened_path = final_path_for_handle(&file)?;
        reject_network_or_device_path(&opened_path)?;
        reject_reparse_components(&opened_path)?;
        if !windows_path_equals(&opened_path, path)
            || !windows_path_is_within(&opened_path, approved_root)
        {
            return Err(XmlEngineError::SchemaPolicy(
                "opened XSD handle resolved outside the approved schema path".into(),
            ));
        }
        if metadata.len() > MAX_SCHEMA_FILE_BYTES as u64 {
            return Err(XmlEngineError::SchemaPolicy(
                "XSD file exceeds its size limit".into(),
            ));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take((MAX_SCHEMA_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| XmlEngineError::Io(error.to_string()))?;
        if bytes.len() > MAX_SCHEMA_FILE_BYTES {
            return Err(XmlEngineError::SchemaPolicy(
                "XSD file grew beyond its size limit while reading".into(),
            ));
        }
        Ok(bytes)
    }

    fn final_path_for_handle(file: &File) -> Result<PathBuf, XmlEngineError> {
        let handle = HANDLE(file.as_raw_handle());
        let mut buffer = vec![0_u16; 512];
        loop {
            let length = unsafe { GetFinalPathNameByHandleW(handle, &mut buffer, VOLUME_NAME_DOS) };
            if length == 0 {
                return Err(XmlEngineError::Io(
                    std::io::Error::last_os_error().to_string(),
                ));
            }
            let length = length as usize;
            if length < buffer.len() {
                buffer.truncate(length);
                let path = String::from_utf16(&buffer)
                    .map_err(|error| XmlEngineError::Io(error.to_string()))?;
                return Ok(strip_windows_extended_prefix(path));
            }
            buffer.resize(length.saturating_add(1), 0);
        }
    }

    fn strip_windows_extended_prefix(path: String) -> PathBuf {
        if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
            PathBuf::from(format!(r"\\{unc}"))
        } else if let Some(local) = path.strip_prefix(r"\\?\") {
            PathBuf::from(local)
        } else {
            PathBuf::from(path)
        }
    }

    fn windows_path_text(path: &Path) -> String {
        strip_windows_extended_prefix(path.to_string_lossy().into_owned())
            .to_string_lossy()
            .into_owned()
    }

    fn windows_path_equals(left: &Path, right: &Path) -> bool {
        windows_path_text(left).eq_ignore_ascii_case(&windows_path_text(right))
    }

    fn windows_path_is_within(path: &Path, root: &Path) -> bool {
        let path = windows_path_text(path);
        let root = windows_path_text(root);
        let root = root.trim_end_matches(['\\', '/']);
        path.eq_ignore_ascii_case(root)
            || path
                .get(..root.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(root))
                && path
                    .as_bytes()
                    .get(root.len())
                    .is_some_and(|separator| *separator == b'\\' || *separator == b'/')
    }

    fn schema_root(document: &IXMLDOMDocument2) -> Result<IXMLDOMElement, XmlEngineError> {
        unsafe { document.documentElement() }
            .map_err(|_| XmlEngineError::SchemaPolicy("XSD has no document element".into()))
    }

    fn schema_target_namespace(document: &IXMLDOMDocument2) -> Result<String, XmlEngineError> {
        let root = schema_root(document)?;
        element_attribute(&root, "targetNamespace").map(|value| value.unwrap_or_default())
    }

    fn element_attribute(
        element: &IXMLDOMElement,
        name: &str,
    ) -> Result<Option<String>, XmlEngineError> {
        let value = unsafe { element.getAttribute(&BSTR::from(name)) }
            .map_err(|_| XmlEngineError::SchemaPolicy("could not read an XSD attribute".into()))?
            .to_string();
        Ok((!value.is_empty()).then_some(value))
    }

    fn has_direct_schema_declaration(
        document: &IXMLDOMDocument2,
        declaration: &str,
    ) -> Result<bool, XmlEngineError> {
        let expression = format!("/*/xs:{declaration}");
        let matches = unsafe { document.selectNodes(&BSTR::from(expression)) }.map_err(|_| {
            XmlEngineError::SchemaPolicy("could not inspect XSD composition".into())
        })?;
        Ok(unsafe { matches.length() }.unwrap_or(0) > 0)
    }

    fn merge_include_settings(
        parent: &IXMLDOMElement,
        included: &IXMLDOMElement,
    ) -> Result<(), XmlEngineError> {
        let parent_namespace = element_attribute(parent, "targetNamespace")?.unwrap_or_default();
        let included_namespace =
            element_attribute(included, "targetNamespace")?.unwrap_or_default();
        if parent_namespace != included_namespace {
            return Err(XmlEngineError::SchemaPolicy(
                "included XSD must use the same target namespace".into(),
            ));
        }
        for name in [
            "elementFormDefault",
            "attributeFormDefault",
            "blockDefault",
            "finalDefault",
            "xpathDefaultNamespace",
        ] {
            if element_attribute(parent, name)? != element_attribute(included, name)? {
                return Err(XmlEngineError::SchemaPolicy(
                    "included XSD has incompatible schema defaults".into(),
                ));
            }
        }
        let attributes = unsafe { included.attributes() }.map_err(|_| {
            XmlEngineError::SchemaPolicy("could not inspect included XSD namespaces".into())
        })?;
        let count = unsafe { attributes.length() }.map_err(|_| {
            XmlEngineError::SchemaPolicy("could not count included XSD namespaces".into())
        })?;
        for index in 0..count {
            let attribute = unsafe { attributes.get_item(index) }.map_err(|_| {
                XmlEngineError::SchemaPolicy("could not read included XSD namespace".into())
            })?;
            let name = unsafe { attribute.nodeName() }
                .map(|value| value.to_string())
                .map_err(|_| {
                    XmlEngineError::SchemaPolicy("included XSD namespace name is unreadable".into())
                })?;
            if name != "xmlns" && !name.starts_with("xmlns:") {
                continue;
            }
            let value = unsafe { attribute.text() }
                .map(|value| value.to_string())
                .map_err(|_| {
                    XmlEngineError::SchemaPolicy(
                        "included XSD namespace value is unreadable".into(),
                    )
                })?;
            match element_attribute(parent, &name)? {
                Some(existing) if existing != value => {
                    return Err(XmlEngineError::SchemaPolicy(
                        "included XSD namespace prefix conflicts with the selected schema".into(),
                    ));
                }
                Some(_) => {}
                None => unsafe {
                    parent.setAttribute(&BSTR::from(name), &VARIANT::from(value.as_str()))
                }
                .map_err(|_| {
                    XmlEngineError::SchemaPolicy(
                        "could not preserve an included XSD namespace".into(),
                    )
                })?,
            }
        }
        Ok(())
    }

    fn add_schema_dom(
        cache: &IXMLDOMSchemaCollection2,
        namespace: &str,
        document: &IXMLDOMDocument2,
    ) -> Result<(), XmlEngineError> {
        let root = schema_root(document)?;
        let unknown = IUnknown::from(root);
        unsafe { cache.add(&BSTR::from(namespace), &VARIANT::from(unknown)) }.map_err(|_| {
            XmlEngineError::SchemaLoad(XmlDiagnostic {
                line: None,
                column: None,
                message: "MSXML could not compile the securely parsed local XSD".into(),
            })
        })
    }

    fn reject_network_or_device_path(path: &Path) -> Result<(), XmlEngineError> {
        let prefix_is_local = match path.components().next() {
            Some(Component::Prefix(prefix)) => {
                matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
            }
            _ => true,
        };
        let value = path.to_string_lossy();
        if !prefix_is_local || value.starts_with("//") || value.contains("://") {
            return Err(XmlEngineError::SchemaPolicy(
                "network and device paths are not allowed".into(),
            ));
        }
        if path.components().any(|component| {
            matches!(component, Component::Normal(value) if value.to_string_lossy().contains(':'))
        }) {
            return Err(XmlEngineError::SchemaPolicy("alternate data stream paths are not allowed".into()));
        }
        Ok(())
    }

    fn reject_reparse_components(path: &Path) -> Result<(), XmlEngineError> {
        let mut current = PathBuf::new();
        for component in path.components() {
            if matches!(component, Component::Prefix(_)) {
                current.push(component.as_os_str());
                continue;
            }
            current.push(component);
            let metadata = fs::symlink_metadata(&current)
                .map_err(|error| XmlEngineError::Io(error.to_string()))?;
            let is_reparse = metadata.file_type().is_symlink() || windows_reparse_point(&metadata);
            if is_reparse {
                return Err(XmlEngineError::SchemaPolicy(
                    "reparse points are not allowed in XSD paths".into(),
                ));
            }
        }
        Ok(())
    }

    #[cfg(windows)]
    fn windows_reparse_point(metadata: &fs::Metadata) -> bool {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }

    #[cfg(not(windows))]
    fn windows_reparse_point(_: &fs::Metadata) -> bool {
        false
    }

    fn local_schema_reference(parent: &Path, location: &str) -> Result<PathBuf, XmlEngineError> {
        if location.is_empty()
            || location.contains(':')
            || location.contains('%')
            || location.contains('?')
            || location.contains('#')
            || location.starts_with('\\')
            || location.starts_with('/')
        {
            return Err(XmlEngineError::SchemaPolicy(
                "XSD import must use a confined relative path".into(),
            ));
        }
        let path = Path::new(location);
        if path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(XmlEngineError::SchemaPolicy(
                "XSD import must stay within the selected folder".into(),
            ));
        }
        Ok(parent.parent().unwrap_or_else(|| Path::new("")).join(path))
    }
}

#[cfg(windows)]
pub use windows_backend::XmlEngineWorker;

#[cfg(not(windows))]
pub struct XmlEngineWorker;

#[cfg(not(windows))]
impl XmlEngineWorker {
    pub fn start() -> Result<Self, XmlEngineError> {
        Err(XmlEngineError::UnsupportedPlatform)
    }
}
