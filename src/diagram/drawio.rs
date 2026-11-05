//! Bounded XML event intake for the documented draw.io subset.

use quick_xml::Reader;
use quick_xml::events::{BytesDecl, BytesStart, Event};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::str;

use super::visual::{
    DEFAULT_LAYER_ID, MAX_VISUAL_CONNECTOR_LABEL_BYTES, MAX_VISUAL_COORDINATE_ABS,
    MAX_VISUAL_DIMENSION, MAX_VISUAL_LAYERS, MAX_VISUAL_NAME_BYTES, MAX_VISUAL_OBJECTS,
    MAX_VISUAL_TEXT_BYTES, MIN_VISUAL_DIMENSION, TITLE_OBJECT_ID, VisualAnchor, VisualColor,
    VisualConnector, VisualConnectorDirection, VisualConnectorEndpoint, VisualConnectorRoute,
    VisualDiagramDocument, VisualDocumentError, VisualLayer, VisualLayerId, VisualLineStyle,
    VisualObject, VisualObjectId, VisualObjectKind, VisualPrimitiveShape, VisualScene, VisualStyle,
    VisualTransform,
};

pub const MAX_DRAWIO_XML_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_DRAWIO_XML_DEPTH: usize = 16;
pub const MAX_DRAWIO_ATTRIBUTES_PER_ELEMENT: usize = 32;
pub const MAX_DRAWIO_ATTRIBUTE_NAME_BYTES: usize = 128;
pub const MAX_DRAWIO_ATTRIBUTE_VALUE_BYTES: usize = MAX_VISUAL_TEXT_BYTES;
pub const MAX_DRAWIO_ENCODED_ATTRIBUTE_BYTES: usize = MAX_VISUAL_TEXT_BYTES * 6;
pub const MAX_DRAWIO_STYLE_BYTES: usize = 4 * 1024;
pub const MAX_DRAWIO_ID_BYTES: usize = 128;
pub const MAX_DRAWIO_ISSUES: usize = 64;
pub const MAX_DRAWIO_CELLS: usize = MAX_VISUAL_OBJECTS + MAX_VISUAL_LAYERS + 1;
const MAX_DRAWIO_XML_EVENTS: usize = MAX_DRAWIO_CELLS * 8 + 128;
const MAX_DRAWIO_ELEMENT_NAME_BYTES: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DrawioWireError {
    InputTooLarge { max_bytes: usize },
    InvalidUtf8,
    MalformedXml,
    DuplicateAttribute,
    DuplicateId,
    MalformedLink,
    LimitExceeded(&'static str),
    Unsupported(&'static str),
    UnsupportedContent(DrawioReport),
    InvalidVisualDocument(VisualDocumentError),
}

impl fmt::Display for DrawioWireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge { max_bytes } => {
                write!(formatter, "draw.io XML exceeds the {max_bytes}-byte limit")
            }
            Self::InvalidUtf8 => formatter.write_str("draw.io XML must be UTF-8"),
            Self::MalformedXml => formatter.write_str("draw.io XML is malformed"),
            Self::DuplicateAttribute => {
                formatter.write_str("draw.io XML contains a duplicate attribute")
            }
            Self::DuplicateId => formatter.write_str("draw.io XML contains a duplicate cell ID"),
            Self::MalformedLink => {
                formatter.write_str("draw.io connector has a missing or unsupported endpoint")
            }
            Self::LimitExceeded(limit) => {
                write!(formatter, "draw.io XML exceeds the {limit} limit")
            }
            Self::Unsupported(reason) => write!(formatter, "unsupported draw.io XML: {reason}"),
            Self::UnsupportedContent(report) => {
                write!(
                    formatter,
                    "draw.io input contains unsupported content: {}",
                    report.total_issues
                )
            }
            Self::InvalidVisualDocument(error) => {
                write!(
                    formatter,
                    "draw.io content is not a valid visual document: {error}"
                )
            }
        }
    }
}

impl std::error::Error for DrawioWireError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawioIssue {
    pub source_id: Option<String>,
    pub property: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DrawioReport {
    pub issues: Vec<DrawioIssue>,
    pub total_issues: usize,
}

impl DrawioReport {
    fn add(&mut self, source_id: Option<&str>, property: &'static str) {
        self.total_issues = self.total_issues.saturating_add(1);
        if self.issues.len() < MAX_DRAWIO_ISSUES {
            self.issues.push(DrawioIssue {
                source_id: source_id.map(str::to_owned),
                property,
            });
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrawioImportResult {
    pub document: VisualDiagramDocument,
    pub report: DrawioReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawioExportResult {
    /// Complete output bytes. Export returns this only if every scene property
    /// was represented; unsupported-content failures contain a report without
    /// exposing a partially publishable buffer.
    pub bytes: Vec<u8>,
    pub report: DrawioReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct XmlElement {
    pub name: String,
    pub attributes: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum XmlEvent {
    Start(XmlElement),
    Empty(XmlElement),
    End(String),
    Text(String),
    CData(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct XmlNode {
    name: String,
    attributes: Vec<(String, String)>,
    children: Vec<XmlNode>,
    text: String,
}

impl XmlNode {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find_map(|(key, value)| (key == name).then_some(value.as_str()))
    }

    fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a XmlNode> {
        self.children.iter().filter(move |child| child.name == name)
    }
}

fn parse_xml_tree(bytes: &[u8]) -> Result<XmlNode, DrawioWireError> {
    let events = read_xml_events(bytes)?;
    let mut stack = Vec::<XmlNode>::new();
    let mut root = None;
    for event in events {
        match event {
            XmlEvent::Start(element) => stack.push(XmlNode {
                name: element.name,
                attributes: element.attributes,
                children: Vec::new(),
                text: String::new(),
            }),
            XmlEvent::Empty(element) => {
                let node = XmlNode {
                    name: element.name,
                    attributes: element.attributes,
                    children: Vec::new(),
                    text: String::new(),
                };
                append_xml_node(&mut stack, &mut root, node)?;
            }
            XmlEvent::Text(text) | XmlEvent::CData(text) => {
                let Some(parent) = stack.last_mut() else {
                    if text.chars().all(char::is_whitespace) {
                        continue;
                    }
                    return Err(DrawioWireError::MalformedXml);
                };
                parent.text.push_str(&text);
            }
            XmlEvent::End(name) => {
                let Some(node) = stack.pop() else {
                    return Err(DrawioWireError::MalformedXml);
                };
                if node.name != name {
                    return Err(DrawioWireError::MalformedXml);
                }
                append_xml_node(&mut stack, &mut root, node)?;
            }
        }
    }
    if !stack.is_empty() {
        return Err(DrawioWireError::MalformedXml);
    }
    root.ok_or(DrawioWireError::MalformedXml)
}

fn append_xml_node(
    stack: &mut [XmlNode],
    root: &mut Option<XmlNode>,
    node: XmlNode,
) -> Result<(), DrawioWireError> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else if root.replace(node).is_some() {
        return Err(DrawioWireError::MalformedXml);
    }
    Ok(())
}

pub(super) fn read_xml_events(bytes: &[u8]) -> Result<Vec<XmlEvent>, DrawioWireError> {
    if bytes.len() > MAX_DRAWIO_XML_BYTES {
        return Err(DrawioWireError::InputTooLarge {
            max_bytes: MAX_DRAWIO_XML_BYTES,
        });
    }
    let source = str::from_utf8(bytes).map_err(|_| DrawioWireError::InvalidUtf8)?;
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_comments = true;

    let mut events = Vec::new();
    let mut stack = Vec::<String>::new();
    let mut root_name: Option<String> = None;
    let mut root_closed = false;
    let mut declaration_seen = false;
    let mut raw_event_count = 0usize;
    let mut declaration_must_be_first = true;

    loop {
        let event = reader
            .read_event()
            .map_err(|_| DrawioWireError::MalformedXml)?;
        if !matches!(event, Event::Eof) {
            raw_event_count = raw_event_count.saturating_add(1);
            if raw_event_count > MAX_DRAWIO_XML_EVENTS {
                return Err(DrawioWireError::LimitExceeded("event-count"));
            }
        }
        match event {
            Event::Start(start) => {
                if root_closed {
                    return Err(DrawioWireError::MalformedXml);
                }
                if stack.len() >= MAX_DRAWIO_XML_DEPTH {
                    return Err(DrawioWireError::LimitExceeded("nesting-depth"));
                }
                let element = decode_start(&start, reader.decoder())?;
                if stack.is_empty() {
                    if root_name.is_some() {
                        return Err(DrawioWireError::MalformedXml);
                    }
                    ensure_supported_root(&element.name)?;
                    root_name = Some(element.name.clone());
                }
                stack.push(element.name.clone());
                events.push(XmlEvent::Start(element));
            }
            Event::Empty(start) => {
                if root_closed {
                    return Err(DrawioWireError::MalformedXml);
                }
                if stack.len() >= MAX_DRAWIO_XML_DEPTH {
                    return Err(DrawioWireError::LimitExceeded("nesting-depth"));
                }
                let element = decode_start(&start, reader.decoder())?;
                if stack.is_empty() {
                    if root_name.is_some() {
                        return Err(DrawioWireError::MalformedXml);
                    }
                    ensure_supported_root(&element.name)?;
                    root_name = Some(element.name.clone());
                    root_closed = true;
                }
                events.push(XmlEvent::Empty(element));
            }
            Event::End(end) => {
                let qname = end.name();
                let name =
                    str::from_utf8(qname.as_ref()).map_err(|_| DrawioWireError::MalformedXml)?;
                let Some(open) = stack.pop() else {
                    return Err(DrawioWireError::MalformedXml);
                };
                if open != name {
                    return Err(DrawioWireError::MalformedXml);
                }
                if stack.is_empty() {
                    root_closed = true;
                }
                events.push(XmlEvent::End(name.to_owned()));
            }
            Event::Text(text) => {
                let decoded = text.decode().map_err(|_| DrawioWireError::MalformedXml)?;
                let unescaped = quick_xml::escape::unescape_with(
                    decoded.as_ref(),
                    quick_xml::escape::resolve_xml_entity,
                )
                .map_err(|_| DrawioWireError::Unsupported("custom entity reference"))?;
                if !unescaped.chars().all(is_xml_10_char) {
                    return Err(DrawioWireError::MalformedXml);
                }
                if stack.is_empty() {
                    if !unescaped.is_empty() {
                        declaration_must_be_first = false;
                    }
                    if !unescaped.chars().all(char::is_whitespace) {
                        return Err(DrawioWireError::MalformedXml);
                    }
                } else {
                    declaration_must_be_first = false;
                    events.push(XmlEvent::Text(unescaped.into_owned()));
                }
            }
            Event::CData(text) => {
                if stack.is_empty() || root_closed {
                    return Err(DrawioWireError::MalformedXml);
                }
                let decoded = text.decode().map_err(|_| DrawioWireError::MalformedXml)?;
                if !decoded.chars().all(is_xml_10_char) {
                    return Err(DrawioWireError::MalformedXml);
                }
                declaration_must_be_first = false;
                events.push(XmlEvent::CData(decoded.into_owned()));
            }
            Event::Comment(comment) => {
                let decoded = comment
                    .decode()
                    .map_err(|_| DrawioWireError::MalformedXml)?;
                if !decoded.chars().all(is_xml_10_char) {
                    return Err(DrawioWireError::MalformedXml);
                }
                declaration_must_be_first = false;
            }
            Event::Decl(declaration) => {
                if declaration_seen || root_name.is_some() || !declaration_must_be_first {
                    return Err(DrawioWireError::MalformedXml);
                }
                declaration_seen = true;
                validate_xml_declaration(&declaration, reader.decoder())?;
            }
            Event::DocType(_) => {
                return Err(DrawioWireError::Unsupported("DTD declarations"));
            }
            Event::PI(_) => {
                return Err(DrawioWireError::Unsupported("processing instructions"));
            }
            Event::GeneralRef(reference) => {
                if stack.is_empty() || root_closed {
                    return Err(DrawioWireError::MalformedXml);
                }
                let decoded = reference
                    .decode()
                    .map_err(|_| DrawioWireError::MalformedXml)?;
                let character = decode_xml_reference(&decoded)?;
                if !is_xml_10_char(character) {
                    return Err(DrawioWireError::MalformedXml);
                }
                declaration_must_be_first = false;
                events.push(XmlEvent::Text(character.to_string()));
            }
            Event::Eof => break,
        }
    }

    if root_name.is_none() || !root_closed || !stack.is_empty() {
        return Err(DrawioWireError::MalformedXml);
    }
    Ok(events)
}

pub fn import_drawio_xml(bytes: &[u8]) -> Result<DrawioImportResult, DrawioWireError> {
    let root = parse_xml_tree(bytes)?;
    let (graph, page_name) = select_graph_model(&root)?;
    let cells_root = graph
        .children
        .iter()
        .find(|child| child.name == "root")
        .ok_or(DrawioWireError::Unsupported("missing mxGraph root"))?;
    ensure_whitespace(&cells_root.text)?;

    let cell_nodes = cells_root
        .children
        .iter()
        .filter(|child| child.name == "mxCell")
        .collect::<Vec<_>>();
    if cell_nodes.len() != cells_root.children.len() {
        return Err(DrawioWireError::Unsupported(
            "non-cell content in mxGraph root",
        ));
    }
    if cell_nodes.len() > MAX_DRAWIO_CELLS {
        return Err(DrawioWireError::LimitExceeded("cell-count"));
    }

    let mut report = DrawioReport::default();
    if page_name.is_some_and(|name| !name.is_empty()) {
        report.add(None, "page-name-not-retained");
    }

    let mut cell_ids = HashSet::<String>::with_capacity(cell_nodes.len());
    let mut root_cell_seen = false;
    let mut layer_id_by_source = HashMap::<String, VisualLayerId>::new();
    let mut layers = Vec::<VisualLayer>::new();
    let mut parsed = Vec::<ParsedCell<'_>>::new();

    for node in &cell_nodes {
        ensure_whitespace(&node.text)?;
        let id = node
            .attr("id")
            .ok_or(DrawioWireError::Unsupported("cell without ID"))?;
        if id.is_empty() || id.len() > MAX_DRAWIO_ID_BYTES {
            return Err(DrawioWireError::LimitExceeded("cell-ID-bytes"));
        }
        if !cell_ids.insert(id.to_owned()) {
            return Err(DrawioWireError::DuplicateId);
        }
        let parent = node.attr("parent");
        let vertex = parse_optional_boolean(node.attr("vertex"))?;
        let edge = parse_optional_boolean(node.attr("edge"))?;
        if vertex && edge {
            return Err(DrawioWireError::Unsupported(
                "cell marked as both vertex and edge",
            ));
        }
        let style = parse_style_text(node.attr("style"))?;

        if id == "0" {
            ensure_cell_attributes(node, &["id"])?;
            if root_cell_seen
                || !layers.is_empty()
                || !parsed.is_empty()
                || !node.children.is_empty()
            {
                return Err(DrawioWireError::Unsupported("invalid root cell"));
            }
            root_cell_seen = true;
            continue;
        }
        if !root_cell_seen {
            return Err(DrawioWireError::Unsupported("root cell must be first"));
        }

        if parent == Some("0") && !vertex && !edge {
            ensure_cell_attributes(node, &["id", "parent", "value", "style"])?;
            if !node.children.is_empty() || !style.is_empty() {
                return Err(DrawioWireError::Unsupported("styled or nested layer cell"));
            }
            if layers.len() >= MAX_VISUAL_LAYERS {
                return Err(DrawioWireError::LimitExceeded("layer-count"));
            }
            let layer_id = VisualLayerId::from_raw(
                u64::try_from(layers.len() + 1)
                    .map_err(|_| DrawioWireError::LimitExceeded("layer-count"))?,
            );
            let name = match node.attr("value") {
                Some(value) if !value.is_empty() => value.to_owned(),
                Some(_) => {
                    report.add(Some(id), "empty-layer-name-defaulted");
                    format!("Layer {}", layers.len() + 1)
                }
                None => {
                    report.add(Some(id), "missing-layer-name-defaulted");
                    format!("Layer {}", layers.len() + 1)
                }
            };
            if name.len() > MAX_VISUAL_NAME_BYTES {
                return Err(DrawioWireError::LimitExceeded("layer-name-bytes"));
            }
            layer_id_by_source.insert(id.to_owned(), layer_id);
            layers.push(VisualLayer { id: layer_id, name });
            continue;
        }

        let kind = match (vertex, edge) {
            (true, false) => ParsedCellKind::Vertex,
            (false, true) => ParsedCellKind::Edge,
            _ => return Err(DrawioWireError::Unsupported("nested or untyped graph cell")),
        };
        let allowed = match kind {
            ParsedCellKind::Vertex => &[
                "id",
                "parent",
                "value",
                "style",
                "vertex",
                "visible",
                "connectable",
            ][..],
            ParsedCellKind::Edge => &[
                "id",
                "parent",
                "value",
                "style",
                "edge",
                "source",
                "target",
                "visible",
                "connectable",
            ][..],
        };
        ensure_cell_attributes(node, allowed)?;
        if node.attr("visible").is_some_and(|value| value != "1")
            || node.attr("connectable").is_some_and(|value| value != "1")
        {
            return Err(DrawioWireError::Unsupported(
                "hidden or non-connectable cell",
            ));
        }
        parsed.push(ParsedCell {
            id: id.to_owned(),
            parent: parent.map(str::to_owned),
            value: node.attr("value").unwrap_or("").to_owned(),
            kind,
            style,
            node,
        });
    }
    if !root_cell_seen {
        return Err(DrawioWireError::Unsupported("missing root cell"));
    }
    if layers.is_empty() {
        layers.push(VisualLayer {
            id: DEFAULT_LAYER_ID,
            name: "Layer 1".into(),
        });
    }
    for cell in &parsed {
        let Some(parent) = cell.parent.as_deref() else {
            return Err(DrawioWireError::Unsupported(
                "graph cell is missing a layer parent",
            ));
        };
        if !layer_id_by_source.contains_key(parent) {
            return Err(DrawioWireError::Unsupported(
                "cell parent is not a direct layer",
            ));
        }
    }

    let mut scene = VisualScene {
        layers,
        objects: Vec::new(),
        presets: Vec::new(),
        assets: Vec::new(),
    };
    let mut next_object_id = 2u64;
    let mut source_object_ids = HashMap::<String, VisualObjectId>::new();
    let mut source_transforms = HashMap::<String, VisualTransform>::new();
    let mut pending = Vec::<PendingVisualObject>::new();

    for cell in &parsed {
        let layer_source = cell.parent.as_deref().ok_or(DrawioWireError::Unsupported(
            "graph cell is missing a layer parent",
        ))?;
        let layer_id =
            *layer_id_by_source
                .get(layer_source)
                .ok_or(DrawioWireError::Unsupported(
                    "cell parent is not a direct layer",
                ))?;
        let style = parse_cell_style(&cell.style, cell.kind, &cell.value)?;
        match cell.kind {
            ParsedCellKind::Vertex => {
                let transform = parse_vertex_transform(cell.node, style.rotation)?;
                let shape = style
                    .shape
                    .ok_or(DrawioWireError::Unsupported("missing vertex shape"))?;
                if shape == VertexShape::Text {
                    let id = allocate_object_id(&mut next_object_id)?;
                    let object = VisualObject {
                        id,
                        layer_id,
                        transform,
                        style: style.text_style(),
                        kind: VisualObjectKind::Text {
                            text: cell.value.clone(),
                        },
                    };
                    source_object_ids.insert(cell.id.clone(), id);
                    source_transforms.insert(cell.id.clone(), transform);
                    pending.push(PendingVisualObject::Ready(object));
                } else {
                    let shape_id = allocate_object_id(&mut next_object_id)?;
                    let primitive = VisualObject {
                        id: shape_id,
                        layer_id,
                        transform,
                        style: style.visual_style.clone(),
                        kind: VisualObjectKind::Primitive {
                            shape: shape
                                .primitive()
                                .ok_or(DrawioWireError::Unsupported("unsupported vertex shape"))?,
                        },
                    };
                    source_object_ids.insert(cell.id.clone(), shape_id);
                    source_transforms.insert(cell.id.clone(), transform);
                    pending.push(PendingVisualObject::Ready(primitive));
                    if !cell.value.is_empty() {
                        report.add(Some(&cell.id), "vertex-label-split-to-text");
                        let label_id = allocate_object_id(&mut next_object_id)?;
                        let label = VisualObject {
                            id: label_id,
                            layer_id,
                            transform,
                            style: style.text_style(),
                            kind: VisualObjectKind::Text {
                                text: cell.value.clone(),
                            },
                        };
                        pending.push(PendingVisualObject::Ready(label));
                    }
                }
            }
            ParsedCellKind::Edge => {
                let connector_id = allocate_object_id(&mut next_object_id)?;
                if style.source_anchor.is_none() {
                    report.add(Some(&cell.id), "source-anchor-inferred");
                }
                if style.target_anchor.is_none() {
                    report.add(Some(&cell.id), "target-anchor-inferred");
                }
                pending.push(PendingVisualObject::Connector(PendingConnector {
                    id: connector_id,
                    layer_id,
                    style,
                    label: cell.value.clone(),
                    source: cell.node.attr("source").map(str::to_owned),
                    target: cell.node.attr("target").map(str::to_owned),
                    node: cell.node,
                }));
            }
        }
        if pending.len() > MAX_VISUAL_OBJECTS {
            return Err(DrawioWireError::LimitExceeded("visual-object-count"));
        }
    }

    {
        pending.push(PendingVisualObject::Ready(VisualObject {
            id: TITLE_OBJECT_ID,
            layer_id: DEFAULT_LAYER_ID,
            transform: inert_title_transform(),
            style: inert_title_style(),
            kind: VisualObjectKind::Text {
                text: String::new(),
            },
        }));
    }

    for item in pending {
        let object = match item {
            PendingVisualObject::Ready(object) => object,
            PendingVisualObject::Connector(pending_connector) => {
                let connector =
                    resolve_connector(pending_connector, &source_object_ids, &source_transforms)?;
                connector
            }
        };
        if scene.objects.len() >= MAX_VISUAL_OBJECTS {
            return Err(DrawioWireError::LimitExceeded("visual-object-count"));
        }
        scene.objects.push(object);
    }

    let document = VisualDiagramDocument::from_fresh_scene(scene)
        .map_err(DrawioWireError::InvalidVisualDocument)?;
    Ok(DrawioImportResult { document, report })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParsedCellKind {
    Vertex,
    Edge,
}

struct ParsedCell<'a> {
    id: String,
    parent: Option<String>,
    value: String,
    kind: ParsedCellKind,
    style: HashMap<String, String>,
    node: &'a XmlNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VertexShape {
    Rectangle,
    RoundRect,
    Diamond,
    Ellipse,
    Text,
}

impl VertexShape {
    fn primitive(self) -> Option<VisualPrimitiveShape> {
        match self {
            Self::Rectangle => Some(VisualPrimitiveShape::Rectangle),
            Self::RoundRect => Some(VisualPrimitiveShape::RoundRect),
            Self::Diamond => Some(VisualPrimitiveShape::Diamond),
            Self::Ellipse => Some(VisualPrimitiveShape::Ellipse),
            Self::Text => None,
        }
    }
}

#[derive(Clone)]
struct ParsedCellStyle {
    visual_style: VisualStyle,
    font_color: VisualColor,
    shape: Option<VertexShape>,
    rotation: f64,
    route: VisualConnectorRoute,
    direction: VisualConnectorDirection,
    source_anchor: Option<VisualAnchor>,
    target_anchor: Option<VisualAnchor>,
}

impl ParsedCellStyle {
    fn text_style(&self) -> VisualStyle {
        VisualStyle {
            fill: VisualColor::rgba(0, 0, 0, 0),
            border: self.font_color,
            border_width: 0.0,
            line_style: VisualLineStyle::Solid,
            font_family: self.visual_style.font_family.clone(),
            font_size: self.visual_style.font_size,
            opacity: self.visual_style.opacity,
            shadow: None,
        }
    }
}

enum PendingVisualObject<'a> {
    Ready(VisualObject),
    Connector(PendingConnector<'a>),
}

struct PendingConnector<'a> {
    id: VisualObjectId,
    layer_id: VisualLayerId,
    style: ParsedCellStyle,
    label: String,
    source: Option<String>,
    target: Option<String>,
    node: &'a XmlNode,
}

fn select_graph_model(root: &XmlNode) -> Result<(&XmlNode, Option<&str>), DrawioWireError> {
    match root.name.as_str() {
        "mxGraphModel" => {
            ensure_attributes(
                root,
                &[
                    "dx",
                    "dy",
                    "grid",
                    "gridSize",
                    "guides",
                    "tooltips",
                    "connect",
                    "arrows",
                    "fold",
                    "page",
                    "pageScale",
                    "pageWidth",
                    "pageHeight",
                    "math",
                    "shadow",
                ],
            )?;
            ensure_default_graph_options(root)?;
            ensure_whitespace(&root.text)?;
            let roots = root.children_named("root").collect::<Vec<_>>();
            if roots.len() != 1 || roots.len() != root.children.len() {
                return Err(DrawioWireError::Unsupported("expected one mxGraph root"));
            }
            Ok((root, None))
        }
        "mxfile" => {
            ensure_attributes(
                root,
                &["host", "modified", "agent", "version", "type", "compressed"],
            )?;
            if root
                .attr("compressed")
                .is_some_and(|value| value != "false" && value != "0")
            {
                return Err(DrawioWireError::Unsupported("compressed diagram pages"));
            }
            ensure_whitespace(&root.text)?;
            let diagrams = root.children_named("diagram").collect::<Vec<_>>();
            if diagrams.len() != 1 || diagrams.len() != root.children.len() {
                return Err(DrawioWireError::Unsupported(
                    "multiple or unsupported diagram pages",
                ));
            }
            let diagram = diagrams[0];
            ensure_attributes(diagram, &["id", "name"])?;
            if !diagram.text.chars().all(char::is_whitespace) {
                return Err(DrawioWireError::Unsupported("compressed diagram page"));
            }
            let models = diagram.children_named("mxGraphModel").collect::<Vec<_>>();
            if models.len() != 1 || models.len() != diagram.children.len() {
                return Err(DrawioWireError::Unsupported(
                    "expected one uncompressed diagram page",
                ));
            }
            let graph = models[0];
            ensure_attributes(
                graph,
                &[
                    "dx",
                    "dy",
                    "grid",
                    "gridSize",
                    "guides",
                    "tooltips",
                    "connect",
                    "arrows",
                    "fold",
                    "page",
                    "pageScale",
                    "pageWidth",
                    "pageHeight",
                    "math",
                    "shadow",
                ],
            )?;
            ensure_default_graph_options(graph)?;
            ensure_whitespace(&graph.text)?;
            let roots = graph.children_named("root").collect::<Vec<_>>();
            if roots.len() != 1 || roots.len() != graph.children.len() {
                return Err(DrawioWireError::Unsupported("expected one mxGraph root"));
            }
            Ok((graph, diagram.attr("name")))
        }
        _ => Err(DrawioWireError::Unsupported("root element")),
    }
}

fn ensure_attributes(node: &XmlNode, allowed: &[&str]) -> Result<(), DrawioWireError> {
    if node
        .attributes
        .iter()
        .any(|(name, _)| !allowed.contains(&name.as_str()))
    {
        Err(DrawioWireError::Unsupported("unknown XML attribute"))
    } else {
        Ok(())
    }
}

fn ensure_default_graph_options(node: &XmlNode) -> Result<(), DrawioWireError> {
    for attribute in ["math", "shadow"] {
        if node
            .attr(attribute)
            .is_some_and(|value| value != "0" && value != "false")
        {
            return Err(DrawioWireError::Unsupported(
                "graph-wide math or shadow effects",
            ));
        }
    }
    Ok(())
}

fn ensure_cell_attributes(node: &XmlNode, allowed: &[&str]) -> Result<(), DrawioWireError> {
    ensure_attributes(node, allowed)
}

fn ensure_whitespace(text: &str) -> Result<(), DrawioWireError> {
    if text.chars().all(char::is_whitespace) {
        Ok(())
    } else {
        Err(DrawioWireError::Unsupported("unexpected XML text content"))
    }
}

fn parse_optional_boolean(value: Option<&str>) -> Result<bool, DrawioWireError> {
    match value {
        None | Some("0") => Ok(false),
        Some("1") => Ok(true),
        Some(_) => Err(DrawioWireError::Unsupported("invalid boolean cell flag")),
    }
}

fn parse_style_text(value: Option<&str>) -> Result<HashMap<String, String>, DrawioWireError> {
    let Some(value) = value else {
        return Ok(HashMap::new());
    };
    if value.len() > MAX_DRAWIO_STYLE_BYTES {
        return Err(DrawioWireError::LimitExceeded("style-bytes"));
    }
    let mut style = HashMap::new();
    for item in value.split(';').filter(|item| !item.is_empty()) {
        let Some((key, value)) = item.split_once('=') else {
            return Err(DrawioWireError::Unsupported("unrecognized style token"));
        };
        if key.is_empty() || key.len() > MAX_DRAWIO_ATTRIBUTE_NAME_BYTES {
            return Err(DrawioWireError::LimitExceeded("style-key-bytes"));
        }
        if style.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(DrawioWireError::Unsupported("duplicate style property"));
        }
        if style.len() > MAX_DRAWIO_ATTRIBUTES_PER_ELEMENT {
            return Err(DrawioWireError::LimitExceeded("style-property-count"));
        }
    }
    Ok(style)
}

fn parse_cell_style(
    values: &HashMap<String, String>,
    kind: ParsedCellKind,
    label: &str,
) -> Result<ParsedCellStyle, DrawioWireError> {
    const COMMON: &[&str] = &[
        "fillColor",
        "strokeColor",
        "strokeWidth",
        "dashed",
        "dashPattern",
        "opacity",
        "fillOpacity",
        "strokeOpacity",
        "fontFamily",
        "fontSize",
        "fontColor",
        "fontStyle",
        "html",
        "whiteSpace",
        "align",
        "verticalAlign",
    ];
    const VERTEX: &[&str] = &["shape", "rounded", "rotation"];
    const EDGE: &[&str] = &[
        "rounded",
        "edgeStyle",
        "curved",
        "startArrow",
        "endArrow",
        "startFill",
        "endFill",
        "exitX",
        "exitY",
        "entryX",
        "entryY",
        "orthogonalLoop",
        "jettySize",
    ];
    if values.keys().any(|key| {
        !COMMON.contains(&key.as_str())
            && match kind {
                ParsedCellKind::Vertex => !VERTEX.contains(&key.as_str()),
                ParsedCellKind::Edge => !EDGE.contains(&key.as_str()),
            }
    }) {
        return Err(DrawioWireError::Unsupported("unsupported style property"));
    }
    if values
        .get("html")
        .is_some_and(|html| html != "0" && html != "1")
        || values.get("html").is_some_and(|html| {
            html == "1"
                && (label.contains('<')
                    || label.contains('>')
                    || contains_html_entity_reference(label))
        })
    {
        return Err(DrawioWireError::Unsupported("active HTML label"));
    }
    if values
        .get("whiteSpace")
        .is_some_and(|value| value != "wrap")
        || values.get("align").is_some_and(|value| value != "center")
        || values
            .get("verticalAlign")
            .is_some_and(|value| value != "middle")
        || values.get("fontStyle").is_some_and(|value| value != "0")
    {
        return Err(DrawioWireError::Unsupported("unsupported text style"));
    }

    let mut visual_style = VisualStyle::default();
    visual_style.font_size = 12.0;
    visual_style.border_width = 1.0;
    visual_style.fill = parse_color(
        values.get("fillColor").map(String::as_str),
        visual_style.fill,
    )?;
    visual_style.border = parse_color(
        values.get("strokeColor").map(String::as_str),
        visual_style.border,
    )?;
    visual_style.border_width = parse_number(
        values.get("strokeWidth").map(String::as_str),
        visual_style.border_width,
        "stroke width",
    )?;
    visual_style.line_style = match values.get("dashed").map(String::as_str) {
        None | Some("0") => VisualLineStyle::Solid,
        Some("1") => match values.get("dashPattern").map(String::as_str) {
            None | Some("3 3") => VisualLineStyle::Dashed,
            Some("1 1") => VisualLineStyle::Dotted,
            _ => return Err(DrawioWireError::Unsupported("unsupported dash pattern")),
        },
        Some(_) => return Err(DrawioWireError::Unsupported("unsupported dashed style")),
    };
    let opacity = parse_percent(values.get("opacity").map(String::as_str), 100.0)? / 100.0;
    let fill_opacity = parse_percent(values.get("fillOpacity").map(String::as_str), 100.0)?;
    let stroke_opacity = parse_percent(values.get("strokeOpacity").map(String::as_str), 100.0)?;
    visual_style.fill.alpha = (f64::from(visual_style.fill.alpha) * fill_opacity / 100.0)
        .round()
        .clamp(0.0, 255.0) as u8;
    visual_style.border.alpha = (f64::from(visual_style.border.alpha) * stroke_opacity / 100.0)
        .round()
        .clamp(0.0, 255.0) as u8;
    visual_style.font_family = values
        .get("fontFamily")
        .cloned()
        .unwrap_or_else(|| visual_style.font_family.clone());
    if visual_style.font_family.len() > MAX_VISUAL_NAME_BYTES {
        return Err(DrawioWireError::LimitExceeded("font-family-bytes"));
    }
    visual_style.font_size = parse_number(
        values.get("fontSize").map(String::as_str),
        visual_style.font_size,
        "font size",
    )?;
    visual_style.opacity = opacity;
    let font_color = parse_color(
        values.get("fontColor").map(String::as_str),
        VisualColor::rgba(0, 0, 0, 255),
    )?;

    let (shape, rotation, route, direction, source_anchor, target_anchor) = match kind {
        ParsedCellKind::Vertex => {
            let shape_name = values
                .get("shape")
                .map(String::as_str)
                .unwrap_or("rectangle");
            let rounded = values.get("rounded").map(String::as_str);
            let shape = match shape_name {
                "rectangle" | "rect" => match rounded {
                    None | Some("0") => VertexShape::Rectangle,
                    Some("1") => VertexShape::RoundRect,
                    _ => return Err(DrawioWireError::Unsupported("unsupported rounded style")),
                },
                "roundRect" if rounded.is_none_or(|value| value == "1") => VertexShape::RoundRect,
                "ellipse" if rounded.is_none_or(|value| value == "0") => VertexShape::Ellipse,
                "rhombus" if rounded.is_none_or(|value| value == "0") => VertexShape::Diamond,
                "text" if rounded.is_none_or(|value| value == "0") => VertexShape::Text,
                _ => return Err(DrawioWireError::Unsupported("unsupported vertex geometry")),
            };
            if shape == VertexShape::Text
                && (values.get("fillColor").is_some_and(|value| value != "none")
                    || values
                        .get("strokeColor")
                        .is_some_and(|value| value != "none")
                    || values.get("strokeWidth").is_some_and(|value| value != "0")
                    || values.get("dashed").is_some_and(|value| value != "0")
                    || values.contains_key("dashPattern"))
            {
                return Err(DrawioWireError::Unsupported("text box fill or outline"));
            }
            let rotation =
                parse_number(values.get("rotation").map(String::as_str), 0.0, "rotation")?;
            (
                Some(shape),
                rotation,
                VisualConnectorRoute::Straight,
                VisualConnectorDirection::None,
                None,
                None,
            )
        }
        ParsedCellKind::Edge => {
            if values.get("fontColor").is_some() && font_color != visual_style.border {
                return Err(DrawioWireError::Unsupported(
                    "distinct connector label color",
                ));
            }
            if values.get("rounded").is_some_and(|value| value != "0") {
                return Err(DrawioWireError::Unsupported("rounded connector path"));
            }
            let curved = parse_optional_boolean(values.get("curved").map(String::as_str))?;
            let edge_style = values.get("edgeStyle").map(String::as_str);
            let route = if curved {
                if edge_style.is_some_and(|value| value != "none") {
                    return Err(DrawioWireError::Unsupported("conflicting connector route"));
                }
                VisualConnectorRoute::Curved
            } else {
                match edge_style {
                    None | Some("none") | Some("straightEdgeStyle") => {
                        VisualConnectorRoute::Straight
                    }
                    Some("elbowEdgeStyle") | Some("orthogonalEdgeStyle") => {
                        VisualConnectorRoute::Elbow
                    }
                    _ => return Err(DrawioWireError::Unsupported("unsupported connector route")),
                }
            };
            if values
                .get("orthogonalLoop")
                .is_some_and(|value| value != "1")
                || values.get("jettySize").is_some_and(|value| value != "auto")
            {
                return Err(DrawioWireError::Unsupported("custom connector routing"));
            }
            let start_arrow = parse_arrow(
                values
                    .get("startArrow")
                    .map(String::as_str)
                    .unwrap_or("none"),
            )?;
            let end_arrow = parse_arrow(
                values
                    .get("endArrow")
                    .map(String::as_str)
                    .unwrap_or("classic"),
            )?;
            if (start_arrow && values.get("startFill").is_some_and(|value| value != "1"))
                || (end_arrow && values.get("endFill").is_some_and(|value| value != "1"))
            {
                return Err(DrawioWireError::Unsupported("unfilled arrowhead"));
            }
            let direction = match (start_arrow, end_arrow) {
                (false, false) => VisualConnectorDirection::None,
                (false, true) => VisualConnectorDirection::Forward,
                (true, false) => VisualConnectorDirection::Reverse,
                (true, true) => VisualConnectorDirection::Both,
            };
            (
                None,
                0.0,
                route,
                direction,
                parse_anchor_pair(values, "exit")?,
                parse_anchor_pair(values, "entry")?,
            )
        }
    };
    Ok(ParsedCellStyle {
        visual_style,
        font_color,
        shape,
        rotation,
        route,
        direction,
        source_anchor,
        target_anchor,
    })
}

fn contains_html_entity_reference(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut cursor = 0usize;
    while cursor + 1 < bytes.len() {
        if bytes[cursor] != b'&' {
            cursor += 1;
            continue;
        }
        let start = cursor + 1;
        let mut end = start;
        if bytes.get(end) == Some(&b'#') {
            end += 1;
            if matches!(bytes.get(end), Some(b'x' | b'X')) {
                end += 1;
                while bytes.get(end).is_some_and(u8::is_ascii_hexdigit) {
                    end += 1;
                }
            } else {
                while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                    end += 1;
                }
            }
        } else if bytes.get(end).is_some_and(u8::is_ascii_alphabetic) {
            while bytes.get(end).is_some_and(u8::is_ascii_alphanumeric) {
                end += 1;
            }
        } else {
            cursor += 1;
            continue;
        }
        if end > start && (bytes.get(end) == Some(&b';') || bytes[start] != b'#') {
            return true;
        }
        cursor += 1;
    }
    false
}

fn parse_arrow(value: &str) -> Result<bool, DrawioWireError> {
    match value {
        "none" => Ok(false),
        "classic" => Ok(true),
        _ => Err(DrawioWireError::Unsupported("unsupported arrowhead")),
    }
}

fn parse_anchor_pair(
    style: &HashMap<String, String>,
    prefix: &str,
) -> Result<Option<VisualAnchor>, DrawioWireError> {
    let x = style.get(&format!("{prefix}X"));
    let y = style.get(&format!("{prefix}Y"));
    match (x, y) {
        (None, None) => Ok(None),
        (Some(x), Some(y)) => anchor_from_fraction(
            parse_number(Some(x), 0.5, "anchor x")?,
            parse_number(Some(y), 0.5, "anchor y")?,
        )
        .map(Some),
        _ => Err(DrawioWireError::Unsupported("incomplete connector anchor")),
    }
}

fn anchor_from_fraction(x: f64, y: f64) -> Result<VisualAnchor, DrawioWireError> {
    const EPS: f64 = 1.0e-6;
    let close = |left: f64, right: f64| (left - right).abs() <= EPS;
    match (
        close(x, 0.5),
        close(y, 0.0),
        close(x, 1.0),
        close(y, 0.5),
        close(y, 1.0),
        close(x, 0.0),
    ) {
        (_, true, true, _, _, _) => Ok(VisualAnchor::NorthEast),
        (true, true, _, _, _, _) => Ok(VisualAnchor::North),
        (_, true, _, _, _, true) => Ok(VisualAnchor::NorthWest),
        (_, _, true, true, _, _) => Ok(VisualAnchor::East),
        (_, _, true, _, true, _) => Ok(VisualAnchor::SouthEast),
        (true, _, _, _, true, _) => Ok(VisualAnchor::South),
        (_, _, _, true, _, true) => Ok(VisualAnchor::West),
        (_, _, _, _, true, true) => Ok(VisualAnchor::SouthWest),
        _ => Err(DrawioWireError::Unsupported(
            "connector anchors must use compass points",
        )),
    }
}

fn parse_color(value: Option<&str>, fallback: VisualColor) -> Result<VisualColor, DrawioWireError> {
    let Some(value) = value else {
        return Ok(fallback);
    };
    if value == "none" {
        return Ok(VisualColor::rgba(
            fallback.red,
            fallback.green,
            fallback.blue,
            0,
        ));
    }
    let Some(hex) = value.strip_prefix('#') else {
        return Err(DrawioWireError::Unsupported("unsupported color syntax"));
    };
    let bytes = hex.as_bytes();
    if bytes.len() != 6 || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return Err(DrawioWireError::Unsupported("unsupported color syntax"));
    }
    let channel = |high: u8, low: u8| {
        let high = hex_digit(high).ok_or(DrawioWireError::Unsupported("invalid color"))?;
        let low = hex_digit(low).ok_or(DrawioWireError::Unsupported("invalid color"))?;
        Ok::<u8, DrawioWireError>((high << 4) | low)
    };
    Ok(VisualColor::rgba(
        channel(bytes[0], bytes[1])?,
        channel(bytes[2], bytes[3])?,
        channel(bytes[4], bytes[5])?,
        255,
    ))
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_percent(value: Option<&str>, fallback: f64) -> Result<f64, DrawioWireError> {
    let value = parse_number(value, fallback, "opacity")?;
    if !(0.0..=100.0).contains(&value) {
        return Err(DrawioWireError::Unsupported("opacity out of range"));
    }
    Ok(value)
}

fn parse_number(
    value: Option<&str>,
    fallback: f64,
    _name: &'static str,
) -> Result<f64, DrawioWireError> {
    let Some(value) = value else {
        return Ok(fallback);
    };
    let parsed = value
        .parse::<f64>()
        .map_err(|_| DrawioWireError::Unsupported("invalid numeric value"))?;
    if !parsed.is_finite() {
        return Err(DrawioWireError::Unsupported("non-finite numeric value"));
    }
    Ok(parsed)
}

fn parse_vertex_transform(
    node: &XmlNode,
    rotation: f64,
) -> Result<VisualTransform, DrawioWireError> {
    let geometry = geometry_child(node)?;
    ensure_attributes(geometry, &["as", "x", "y", "width", "height"])?;
    if geometry.attr("as") != Some("geometry") || !geometry.children.is_empty() {
        return Err(DrawioWireError::Unsupported("unsupported vertex geometry"));
    }
    ensure_whitespace(&geometry.text)?;
    let x = parse_number(geometry.attr("x"), 0.0, "x")?;
    let y = parse_number(geometry.attr("y"), 0.0, "y")?;
    let width = parse_number(geometry.attr("width"), f64::NAN, "width")?;
    let height = parse_number(geometry.attr("height"), f64::NAN, "height")?;
    let center_x = x + width / 2.0;
    let center_y = y + height / 2.0;
    if !center_x.is_finite()
        || !center_y.is_finite()
        || center_x.abs() > MAX_VISUAL_COORDINATE_ABS
        || center_y.abs() > MAX_VISUAL_COORDINATE_ABS
        || !(MIN_VISUAL_DIMENSION..=MAX_VISUAL_DIMENSION).contains(&width)
        || !(MIN_VISUAL_DIMENSION..=MAX_VISUAL_DIMENSION).contains(&height)
        || !rotation.is_finite()
        || rotation.abs() > 36_000.0
    {
        return Err(DrawioWireError::LimitExceeded("visual geometry"));
    }
    Ok(VisualTransform {
        x: center_x,
        y: center_y,
        width,
        height,
        rotation_degrees: rotation,
    })
}

fn geometry_child(node: &XmlNode) -> Result<&XmlNode, DrawioWireError> {
    if node.children.len() != 1 || node.children[0].name != "mxGeometry" {
        return Err(DrawioWireError::Unsupported("expected one mxGeometry"));
    }
    Ok(&node.children[0])
}

fn validate_edge_geometry(node: &XmlNode) -> Result<(), DrawioWireError> {
    let geometry = geometry_child(node)?;
    ensure_attributes(geometry, &["as", "relative", "x", "y", "width", "height"])?;
    if geometry.attr("as") != Some("geometry")
        || geometry.attr("relative") != Some("1")
        || !geometry.children.is_empty()
    {
        return Err(DrawioWireError::Unsupported("custom connector waypoints"));
    }
    ensure_whitespace(&geometry.text)?;
    for dimension in ["x", "y", "width", "height"] {
        if parse_number(geometry.attr(dimension), 0.0, dimension)? != 0.0 {
            return Err(DrawioWireError::Unsupported("connector label offset"));
        }
    }
    Ok(())
}

fn allocate_object_id(next: &mut u64) -> Result<VisualObjectId, DrawioWireError> {
    let id = VisualObjectId::from_raw(*next);
    *next = next
        .checked_add(1)
        .ok_or(DrawioWireError::LimitExceeded("visual-object-ID"))?;
    Ok(id)
}

fn resolve_connector(
    pending: PendingConnector<'_>,
    source_object_ids: &HashMap<String, VisualObjectId>,
    source_transforms: &HashMap<String, VisualTransform>,
) -> Result<VisualObject, DrawioWireError> {
    validate_edge_geometry(pending.node)?;
    if pending.label.len() > MAX_VISUAL_CONNECTOR_LABEL_BYTES {
        return Err(DrawioWireError::LimitExceeded("connector-label-bytes"));
    }
    let source = pending.source.ok_or(DrawioWireError::MalformedLink)?;
    let target = pending.target.ok_or(DrawioWireError::MalformedLink)?;
    if source.len() > MAX_DRAWIO_ID_BYTES || target.len() > MAX_DRAWIO_ID_BYTES {
        return Err(DrawioWireError::LimitExceeded("endpoint-ID-bytes"));
    }
    let source_id = *source_object_ids
        .get(&source)
        .ok_or(DrawioWireError::MalformedLink)?;
    let target_id = *source_object_ids
        .get(&target)
        .ok_or(DrawioWireError::MalformedLink)?;
    let source_transform = *source_transforms
        .get(&source)
        .ok_or(DrawioWireError::MalformedLink)?;
    let target_transform = *source_transforms
        .get(&target)
        .ok_or(DrawioWireError::MalformedLink)?;
    let source_anchor = pending
        .style
        .source_anchor
        .unwrap_or_else(|| infer_compass_anchor(source_transform, target_transform));
    let target_anchor = pending
        .style
        .target_anchor
        .unwrap_or_else(|| infer_compass_anchor(target_transform, source_transform));
    Ok(VisualObject {
        id: pending.id,
        layer_id: pending.layer_id,
        transform: VisualTransform::default(),
        style: pending.style.visual_style,
        kind: VisualObjectKind::Connector {
            connector: VisualConnector {
                from: VisualConnectorEndpoint {
                    object_id: source_id,
                    anchor: source_anchor,
                },
                to: VisualConnectorEndpoint {
                    object_id: target_id,
                    anchor: target_anchor,
                },
                route: pending.style.route,
                direction: pending.style.direction,
                label: pending.label,
            },
        },
    })
}

fn infer_compass_anchor(from: VisualTransform, to: VisualTransform) -> VisualAnchor {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let radians = from.rotation_degrees.to_radians();
    let local_x = dx * radians.cos() + dy * radians.sin();
    let local_y = -dx * radians.sin() + dy * radians.cos();
    if local_x == 0.0 && local_y == 0.0 {
        return VisualAnchor::East;
    }
    let octant =
        ((local_y.atan2(local_x) / std::f64::consts::FRAC_PI_4).round() as i32).rem_euclid(8);
    match octant {
        0 => VisualAnchor::East,
        1 => VisualAnchor::SouthEast,
        2 => VisualAnchor::South,
        3 => VisualAnchor::SouthWest,
        4 => VisualAnchor::West,
        5 => VisualAnchor::NorthWest,
        6 => VisualAnchor::North,
        _ => VisualAnchor::NorthEast,
    }
}

pub fn export_drawio_xml(
    document: &VisualDiagramDocument,
) -> Result<DrawioExportResult, DrawioWireError> {
    export_drawio_scene(document.scene())
}

/// Export one committed visual scene without copying its controller history
/// or saved baseline into a temporary document.
pub(crate) fn export_drawio_scene(
    scene: &VisualScene,
) -> Result<DrawioExportResult, DrawioWireError> {
    scene
        .validate()
        .map_err(DrawioWireError::InvalidVisualDocument)?;
    let mut result = DrawioExportResult {
        bytes: Vec::new(),
        report: DrawioReport::default(),
    };
    let exportable_vertices = scene
        .objects
        .iter()
        .filter_map(|object| is_exportable_vertex(object).then_some(object.id))
        .collect::<HashSet<_>>();
    let mut writer = BoundedXmlWriter::default();
    writer.push("<?xml version=\"1.0\" encoding=\"UTF-8\"?>")?;
    writer.push("<mxfile><diagram id=\"page-1\" name=\"Page-1\"><mxGraphModel><root>")?;
    writer.push("<mxCell id=\"0\"/>")?;
    for layer in &scene.layers {
        writer.push("<mxCell id=\"")?;
        writer.push(&layer_wire_id(layer.id))?;
        writer.push("\" parent=\"0\" value=\"")?;
        writer.push_attribute(&layer.name)?;
        writer.push("\"/>")?;
    }

    // mxCell document order is the scene-wide stable stacking order. The parent
    // layer is an independent attribute, so do not regroup objects by layer.
    for object in &scene.objects {
        if object.id == TITLE_OBJECT_ID && is_inert_title(object) {
            continue;
        }
        if object.style.shadow.is_some() {
            result.report.add(
                Some(&object_wire_id(object.id)),
                "shadow-style-not-exported",
            );
        }
        match &object.kind {
            VisualObjectKind::Primitive { shape } => {
                let Some(shape) = primitive_to_wire(*shape) else {
                    result.report.add(
                        Some(&object_wire_id(object.id)),
                        "primitive-shape-not-supported",
                    );
                    continue;
                };
                write_vertex(&mut writer, object, shape, None, &mut result.report)?;
            }
            VisualObjectKind::Text { text } => {
                write_vertex(&mut writer, object, "text", Some(text), &mut result.report)?;
            }
            VisualObjectKind::Connector { connector } => {
                if !exportable_vertices.contains(&connector.from.object_id)
                    || !exportable_vertices.contains(&connector.to.object_id)
                {
                    result.report.add(
                        Some(&object_wire_id(object.id)),
                        "connector-endpoint-not-exported",
                    );
                    continue;
                }
                write_connector(&mut writer, object, connector, &mut result.report)?;
            }
            VisualObjectKind::PresetInstance { .. } => {
                result.report.add(
                    Some(&object_wire_id(object.id)),
                    "preset-instance-not-exported",
                );
            }
            VisualObjectKind::ImageReference { .. } => {
                result.report.add(
                    Some(&object_wire_id(object.id)),
                    "image-reference-not-exported",
                );
            }
        }
    }
    if !scene.presets.is_empty() {
        result.report.add(None, "preset-definitions-not-exported");
    }
    writer.push("</root></mxGraphModel></diagram></mxfile>")?;
    if result.report.total_issues != 0 {
        return Err(DrawioWireError::UnsupportedContent(result.report));
    }
    result.bytes = writer.output.into_bytes();
    Ok(result)
}

fn is_exportable_vertex(object: &VisualObject) -> bool {
    if object.id == TITLE_OBJECT_ID && is_inert_title(object) {
        return false;
    }
    match &object.kind {
        VisualObjectKind::Primitive { shape } => primitive_to_wire(*shape).is_some(),
        VisualObjectKind::Text { .. } => true,
        VisualObjectKind::PresetInstance { .. }
        | VisualObjectKind::ImageReference { .. }
        | VisualObjectKind::Connector { .. } => false,
    }
}

fn is_inert_title(object: &VisualObject) -> bool {
    object.id == TITLE_OBJECT_ID
        && object.layer_id == DEFAULT_LAYER_ID
        && matches!(&object.kind, VisualObjectKind::Text { text } if text.is_empty())
        && object.transform == inert_title_transform()
        && object.style == inert_title_style()
}

fn inert_title_transform() -> VisualTransform {
    VisualTransform {
        x: 0.0,
        y: 0.0,
        width: MIN_VISUAL_DIMENSION,
        height: MIN_VISUAL_DIMENSION,
        rotation_degrees: 0.0,
    }
}

fn inert_title_style() -> VisualStyle {
    let mut style = VisualStyle::default();
    style.fill = VisualColor::rgba(0, 0, 0, 0);
    style.border = VisualColor::rgba(0, 0, 0, 0);
    style.border_width = 0.0;
    style.font_size = 4.0;
    style.opacity = 0.0;
    style
}

fn primitive_to_wire(shape: VisualPrimitiveShape) -> Option<&'static str> {
    match shape {
        VisualPrimitiveShape::Rectangle => Some("rectangle"),
        VisualPrimitiveShape::RoundRect => Some("roundRect"),
        VisualPrimitiveShape::Diamond => Some("rhombus"),
        VisualPrimitiveShape::Ellipse => Some("ellipse"),
        VisualPrimitiveShape::Circle => None,
        VisualPrimitiveShape::Cylinder
        | VisualPrimitiveShape::Parallelogram
        | VisualPrimitiveShape::Triangle => None,
    }
}

fn write_vertex(
    writer: &mut BoundedXmlWriter,
    object: &VisualObject,
    shape: &str,
    text: Option<&str>,
    report: &mut DrawioReport,
) -> Result<(), DrawioWireError> {
    writer.push("<mxCell id=\"")?;
    writer.push(&object_wire_id(object.id))?;
    if let Some(text) = text {
        writer.push("\" value=\"")?;
        writer.push_attribute(text)?;
    }
    writer.push("\" style=\"")?;
    writer.push_attribute(&vertex_style(object, shape, text.is_some(), report)?)?;
    writer.push("\" vertex=\"1\" parent=\"")?;
    writer.push(&layer_wire_id(object.layer_id))?;
    writer.push("\"><mxGeometry x=\"")?;
    writer.push_number(object.transform.x - object.transform.width / 2.0)?;
    writer.push("\" y=\"")?;
    writer.push_number(object.transform.y - object.transform.height / 2.0)?;
    writer.push("\" width=\"")?;
    writer.push_number(object.transform.width)?;
    writer.push("\" height=\"")?;
    writer.push_number(object.transform.height)?;
    writer.push("\" as=\"geometry\"/></mxCell>")
}

fn vertex_style(
    object: &VisualObject,
    shape: &str,
    text: bool,
    report: &mut DrawioReport,
) -> Result<String, DrawioWireError> {
    let opacity = if text {
        if object.style.border.alpha != 255 {
            report.add(
                Some(&object_wire_id(object.id)),
                "text-alpha-folded-into-opacity",
            );
        }
        object.style.opacity * f64::from(object.style.border.alpha) / 255.0
    } else {
        object.style.opacity
    };
    let mut properties = vec![
        ("shape", shape.to_owned()),
        ("fillColor", color_value(object.style.fill)),
        ("fillOpacity", percent_for_alpha(object.style.fill.alpha)),
        (
            "strokeColor",
            if text {
                "none".to_owned()
            } else {
                color_value(object.style.border)
            },
        ),
        (
            "strokeOpacity",
            if text {
                "0".to_owned()
            } else {
                percent_for_alpha(object.style.border.alpha)
            },
        ),
        (
            "strokeWidth",
            if text {
                "0".to_owned()
            } else {
                object.style.border_width.to_string()
            },
        ),
        ("opacity", (opacity * 100.0).to_string()),
        (
            "dashed",
            if object.style.line_style == VisualLineStyle::Solid {
                "0".to_owned()
            } else {
                "1".to_owned()
            },
        ),
        ("html", "0".to_owned()),
        ("whiteSpace", "wrap".to_owned()),
        ("align", "center".to_owned()),
        ("verticalAlign", "middle".to_owned()),
    ];
    if !text && object.style.line_style == VisualLineStyle::Dashed {
        properties.push(("dashPattern", "3 3".to_owned()));
    } else if !text && object.style.line_style == VisualLineStyle::Dotted {
        properties.push(("dashPattern", "1 1".to_owned()));
    }
    if object.transform.rotation_degrees != 0.0 {
        properties.push(("rotation", object.transform.rotation_degrees.to_string()));
    }
    let font_family = if font_family_uses_style_delimiter(&object.style.font_family) {
        report.add(
            Some(&object_wire_id(object.id)),
            "font-family-not-representable",
        );
        "sans-serif"
    } else {
        &object.style.font_family
    };
    properties.push(("fontFamily", font_family.to_owned()));
    properties.push(("fontSize", object.style.font_size.to_string()));
    properties.push(("fontColor", color_value(object.style.border)));
    properties.push(("fontStyle", "0".to_owned()));
    if text {
        for (key, value) in &mut properties {
            if *key == "fillColor" {
                *value = "none".to_owned();
            } else if *key == "fillOpacity" {
                *value = "0".to_owned();
            } else if *key == "dashed" {
                *value = "0".to_owned();
            }
        }
    }
    let style = properties
        .into_iter()
        .map(|(key, value)| format!("{key}={value};"))
        .collect::<String>();
    if style.len() > MAX_DRAWIO_STYLE_BYTES {
        return Err(DrawioWireError::LimitExceeded("style-bytes"));
    }
    Ok(style)
}

fn write_connector(
    writer: &mut BoundedXmlWriter,
    object: &VisualObject,
    connector: &VisualConnector,
    report: &mut DrawioReport,
) -> Result<(), DrawioWireError> {
    writer.push("<mxCell id=\"")?;
    writer.push(&object_wire_id(object.id))?;
    writer.push("\" value=\"")?;
    writer.push_attribute(&connector.label)?;
    writer.push("\" style=\"")?;
    writer.push_attribute(&connector_style(object, connector, report)?)?;
    writer.push("\" edge=\"1\" parent=\"")?;
    writer.push(&layer_wire_id(object.layer_id))?;
    writer.push("\" source=\"")?;
    writer.push(&object_wire_id(connector.from.object_id))?;
    writer.push("\" target=\"")?;
    writer.push(&object_wire_id(connector.to.object_id))?;
    writer.push("\"><mxGeometry relative=\"1\" as=\"geometry\"/></mxCell>")
}

fn connector_style(
    object: &VisualObject,
    connector: &VisualConnector,
    report: &mut DrawioReport,
) -> Result<String, DrawioWireError> {
    let route = match connector.route {
        VisualConnectorRoute::Straight => "edgeStyle=none;curved=0;",
        VisualConnectorRoute::Elbow => {
            "edgeStyle=orthogonalEdgeStyle;curved=0;orthogonalLoop=1;jettySize=auto;"
        }
        VisualConnectorRoute::Curved => "edgeStyle=none;curved=1;",
    };
    let (start_arrow, end_arrow) = match connector.direction {
        VisualConnectorDirection::None => ("none", "none"),
        VisualConnectorDirection::Forward => ("none", "classic"),
        VisualConnectorDirection::Reverse => ("classic", "none"),
        VisualConnectorDirection::Both => ("classic", "classic"),
    };
    let (exit_x, exit_y) = anchor_fraction(connector.from.anchor);
    let (entry_x, entry_y) = anchor_fraction(connector.to.anchor);
    let mut style = format!(
        "{route}startArrow={start_arrow};endArrow={end_arrow};exitX={exit_x};exitY={exit_y};entryX={entry_x};entryY={entry_y};strokeColor={};strokeOpacity={};strokeWidth={};opacity={};dashed={};",
        color_value(object.style.border),
        "100",
        object.style.border_width,
        object.style.opacity * f64::from(object.style.border.alpha) / 255.0 * 100.0,
        u8::from(object.style.line_style != VisualLineStyle::Solid),
    );
    if object.style.line_style == VisualLineStyle::Dashed {
        style.push_str("dashPattern=3 3;");
    } else if object.style.line_style == VisualLineStyle::Dotted {
        style.push_str("dashPattern=1 1;");
    }
    let font_family = if font_family_uses_style_delimiter(&object.style.font_family) {
        report.add(
            Some(&object_wire_id(object.id)),
            "font-family-not-representable",
        );
        "sans-serif"
    } else {
        &object.style.font_family
    };
    if object.style.border.alpha != 255 {
        report.add(
            Some(&object_wire_id(object.id)),
            "connector-alpha-folded-into-opacity",
        );
    }
    style.push_str(&format!(
        "fontFamily={};fontSize={};fontColor={};fontStyle=0;html=0;whiteSpace=wrap;align=center;verticalAlign=middle;",
        font_family,
        object.style.font_size,
        color_value(object.style.border),
    ));
    if style.len() > MAX_DRAWIO_STYLE_BYTES {
        return Err(DrawioWireError::LimitExceeded("style-bytes"));
    }
    Ok(style)
}

fn font_family_uses_style_delimiter(font_family: &str) -> bool {
    font_family
        .chars()
        .any(|character| matches!(character, ';' | '=') || character.is_control())
}

fn anchor_fraction(anchor: VisualAnchor) -> (&'static str, &'static str) {
    match anchor {
        VisualAnchor::North => ("0.5", "0"),
        VisualAnchor::NorthEast => ("1", "0"),
        VisualAnchor::East => ("1", "0.5"),
        VisualAnchor::SouthEast => ("1", "1"),
        VisualAnchor::South => ("0.5", "1"),
        VisualAnchor::SouthWest => ("0", "1"),
        VisualAnchor::West => ("0", "0.5"),
        VisualAnchor::NorthWest => ("0", "0"),
    }
}

fn color_value(color: VisualColor) -> String {
    format!("#{:02x}{:02x}{:02x}", color.red, color.green, color.blue)
}

fn percent_for_alpha(alpha: u8) -> String {
    (f64::from(alpha) * 100.0 / 255.0).to_string()
}

fn object_wire_id(id: VisualObjectId) -> String {
    format!("v{}", id.get())
}

fn layer_wire_id(id: VisualLayerId) -> String {
    format!("l{}", id.get())
}

#[derive(Default)]
struct BoundedXmlWriter {
    output: String,
}

impl BoundedXmlWriter {
    fn push(&mut self, value: &str) -> Result<(), DrawioWireError> {
        if self
            .output
            .len()
            .checked_add(value.len())
            .is_none_or(|size| size > MAX_DRAWIO_XML_BYTES)
        {
            return Err(DrawioWireError::LimitExceeded("output-bytes"));
        }
        self.output.push_str(value);
        Ok(())
    }

    fn push_number(&mut self, value: f64) -> Result<(), DrawioWireError> {
        if !value.is_finite() {
            return Err(DrawioWireError::Unsupported("non-finite output geometry"));
        }
        self.push(&value.to_string())
    }

    fn push_attribute(&mut self, value: &str) -> Result<(), DrawioWireError> {
        for character in value.chars() {
            if !is_xml_10_char(character) {
                return Err(DrawioWireError::Unsupported(
                    "text contains non-XML character",
                ));
            }
            let escaped = match character {
                '&' => "&amp;",
                '<' => "&lt;",
                '>' => "&gt;",
                '"' => "&quot;",
                '\n' => "&#10;",
                '\r' => "&#13;",
                '\t' => "&#9;",
                _ => {
                    let mut buffer = [0; 4];
                    self.push(character.encode_utf8(&mut buffer))?;
                    continue;
                }
            };
            self.push(escaped)?;
        }
        Ok(())
    }
}

fn ensure_supported_root(name: &str) -> Result<(), DrawioWireError> {
    if matches!(name, "mxGraphModel" | "mxfile") {
        Ok(())
    } else {
        Err(DrawioWireError::Unsupported("root element"))
    }
}

fn decode_start(
    start: &BytesStart<'_>,
    decoder: quick_xml::encoding::Decoder,
) -> Result<XmlElement, DrawioWireError> {
    let name = str::from_utf8(start.name().as_ref())
        .map_err(|_| DrawioWireError::MalformedXml)?
        .to_owned();
    if name.is_empty() || name.len() > MAX_DRAWIO_ELEMENT_NAME_BYTES || name.contains(':') {
        return Err(DrawioWireError::Unsupported(
            "qualified or oversized element name",
        ));
    }
    validate_start_lexical(start.as_ref())?;

    let mut attributes = Vec::new();
    let mut seen = HashSet::new();
    for attribute in start.attributes().with_checks(false) {
        let attribute = attribute.map_err(|_| DrawioWireError::MalformedXml)?;
        if attributes.len() >= MAX_DRAWIO_ATTRIBUTES_PER_ELEMENT {
            return Err(DrawioWireError::LimitExceeded("attributes-per-element"));
        }
        let key =
            str::from_utf8(attribute.key.as_ref()).map_err(|_| DrawioWireError::MalformedXml)?;
        if key.is_empty() || key.len() > MAX_DRAWIO_ATTRIBUTE_NAME_BYTES || key.contains(':') {
            return Err(DrawioWireError::Unsupported(
                "qualified or oversized attribute name",
            ));
        }
        if !seen.insert(key.to_owned()) {
            return Err(DrawioWireError::DuplicateAttribute);
        }
        if attribute.value.len() > MAX_DRAWIO_ENCODED_ATTRIBUTE_BYTES {
            return Err(DrawioWireError::LimitExceeded("attribute-value-bytes"));
        }
        if key == "xmlns" {
            return Err(DrawioWireError::Unsupported("namespace declarations"));
        }
        let value = attribute
            .decoded_and_normalized_value_with(
                quick_xml::XmlVersion::Explicit1_0,
                decoder,
                1,
                quick_xml::escape::resolve_xml_entity,
            )
            .map_err(|_| DrawioWireError::MalformedXml)?;
        if value.len() > MAX_DRAWIO_ATTRIBUTE_VALUE_BYTES {
            return Err(DrawioWireError::LimitExceeded(
                "decoded-attribute-value-bytes",
            ));
        }
        if !value.chars().all(is_xml_10_char) {
            return Err(DrawioWireError::MalformedXml);
        }
        attributes.push((key.to_owned(), value.into_owned()));
    }

    Ok(XmlElement { name, attributes })
}

fn validate_xml_declaration(
    declaration: &BytesDecl<'_>,
    decoder: quick_xml::encoding::Decoder,
) -> Result<(), DrawioWireError> {
    let raw = str::from_utf8(&*declaration).map_err(|_| DrawioWireError::MalformedXml)?;
    let start = BytesStart::from_content(raw.to_owned(), 3);
    let mut seen = HashSet::new();
    let mut position = 0usize;
    for attribute in start.attributes().with_checks(true) {
        let attribute = attribute.map_err(|_| DrawioWireError::MalformedXml)?;
        let key =
            str::from_utf8(attribute.key.as_ref()).map_err(|_| DrawioWireError::MalformedXml)?;
        if !seen.insert(key.to_owned()) {
            return Err(DrawioWireError::DuplicateAttribute);
        }
        let expected_position = match key {
            "version" => 0,
            "encoding" => 1,
            "standalone" => 2,
            _ => return Err(DrawioWireError::MalformedXml),
        };
        if expected_position < position || (expected_position > position && key != "standalone") {
            return Err(DrawioWireError::MalformedXml);
        }
        let value = attribute
            .decoded_and_normalized_value_with(
                quick_xml::XmlVersion::Explicit1_0,
                decoder,
                1,
                quick_xml::escape::resolve_xml_entity,
            )
            .map_err(|_| DrawioWireError::MalformedXml)?;
        match key {
            "version" if value == "1.0" && position == 0 => {}
            "encoding" if position <= 1 && value.eq_ignore_ascii_case("utf-8") => {}
            "standalone" if value == "yes" || value == "no" => {}
            "version" => return Err(DrawioWireError::Unsupported("non-1.0 XML declaration")),
            "encoding" => return Err(DrawioWireError::Unsupported("non-UTF-8 XML declaration")),
            "standalone" => return Err(DrawioWireError::MalformedXml),
            _ => return Err(DrawioWireError::MalformedXml),
        }
        position = expected_position + 1;
    }
    if position == 0 || !seen.contains("version") {
        return Err(DrawioWireError::MalformedXml);
    }
    Ok(())
}

fn validate_start_lexical(raw: &[u8]) -> Result<(), DrawioWireError> {
    let mut cursor = 0usize;
    while cursor < raw.len() && !raw[cursor].is_ascii_whitespace() && raw[cursor] != b'/' {
        cursor += 1;
    }
    if cursor == 0 {
        return Err(DrawioWireError::MalformedXml);
    }

    while cursor < raw.len() {
        let before_space = cursor;
        while cursor < raw.len() && raw[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor == raw.len() {
            break;
        }
        if raw[cursor] == b'/' {
            cursor += 1;
            if raw[cursor..].iter().any(|byte| !byte.is_ascii_whitespace()) {
                return Err(DrawioWireError::MalformedXml);
            }
            break;
        }
        if cursor == before_space {
            return Err(DrawioWireError::MalformedXml);
        }

        let name_start = cursor;
        while cursor < raw.len()
            && !raw[cursor].is_ascii_whitespace()
            && !matches!(raw[cursor], b'=' | b'/' | b'<' | b'>' | b'\'' | b'"')
        {
            cursor += 1;
        }
        if cursor == name_start {
            return Err(DrawioWireError::MalformedXml);
        }
        while cursor < raw.len() && raw[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if raw.get(cursor) != Some(&b'=') {
            return Err(DrawioWireError::MalformedXml);
        }
        cursor += 1;
        while cursor < raw.len() && raw[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let Some(&quote @ (b'\'' | b'"')) = raw.get(cursor) else {
            return Err(DrawioWireError::MalformedXml);
        };
        cursor += 1;
        while cursor < raw.len() && raw[cursor] != quote {
            if raw[cursor] == b'<' {
                return Err(DrawioWireError::MalformedXml);
            }
            cursor += 1;
        }
        if raw.get(cursor) != Some(&quote) {
            return Err(DrawioWireError::MalformedXml);
        }
        cursor += 1;
        if cursor < raw.len() && !raw[cursor].is_ascii_whitespace() && raw[cursor] != b'/' {
            return Err(DrawioWireError::MalformedXml);
        }
    }
    Ok(())
}

fn decode_xml_reference(reference: &str) -> Result<char, DrawioWireError> {
    let character = match reference {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "apos" => '\'',
        "quot" => '"',
        numeric if numeric.starts_with("#x") || numeric.starts_with("#X") => {
            let value = u32::from_str_radix(&numeric[2..], 16)
                .map_err(|_| DrawioWireError::MalformedXml)?;
            char::from_u32(value).ok_or(DrawioWireError::MalformedXml)?
        }
        numeric if numeric.starts_with('#') => {
            let value = numeric[1..]
                .parse::<u32>()
                .map_err(|_| DrawioWireError::MalformedXml)?;
            char::from_u32(value).ok_or(DrawioWireError::MalformedXml)?
        }
        _ => return Err(DrawioWireError::Unsupported("custom entity references")),
    };
    Ok(character)
}

fn is_xml_10_char(character: char) -> bool {
    matches!(character, '\u{9}' | '\u{A}' | '\u{D}')
        || matches!(character as u32, 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN_DRAWIO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<mxfile compressed="false"><diagram id="page" name="Starter"><mxGraphModel><root>
<mxCell id="0"/>
<mxCell id="background" parent="0" value="Background"/>
<mxCell id="labels" parent="0" value="Labels"/>
<mxCell id="start" value="A &amp; one" style="shape=rectangle;rounded=0;fillColor=#e0f0ff;strokeColor=#123456;strokeWidth=2;opacity=85;rotation=30;" vertex="1" parent="background"><mxGeometry x="10" y="20" width="120" height="60" as="geometry"/></mxCell>
<mxCell id="finish" value="" style="shape=ellipse;fillColor=#ffffff;strokeColor=#222222;" vertex="1" parent="background"><mxGeometry x="240" y="20" width="100" height="80" as="geometry"/></mxCell>
<mxCell id="caption" value="First&#13;&#10;東京 &amp; more &#60;&quot;" style="shape=text;fillColor=none;strokeColor=none;fontColor=#654321;fontFamily=A&amp;B;fontSize=12;opacity=80;rotation=-15;" vertex="1" parent="labels"><mxGeometry x="30" y="160" width="180" height="48" as="geometry"/></mxCell>
<mxCell id="route" value="flow &amp; data" style="edgeStyle=orthogonalEdgeStyle;curved=0;startArrow=none;endArrow=classic;exitX=1;exitY=0.5;entryX=0;entryY=0.5;strokeColor=#336699;strokeWidth=2;opacity=100;" edge="1" parent="background" source="start" target="finish"><mxGeometry relative="1" as="geometry"/></mxCell>
</root></mxGraphModel></diagram></mxfile>"#;

    fn import_minimal(cells: &str) -> Result<DrawioImportResult, DrawioWireError> {
        let xml = format!(
            "<mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"layer\" parent=\"0\" value=\"Layer\"/>{cells}</root></mxGraphModel>"
        );
        import_drawio_xml(xml.as_bytes())
    }

    fn primitive(id: u64, shape: VisualPrimitiveShape) -> VisualObject {
        VisualObject {
            id: VisualObjectId::from_raw(id),
            layer_id: DEFAULT_LAYER_ID,
            transform: VisualTransform {
                x: 140.0 + id as f64 * 180.0,
                y: 120.0,
                width: 120.0,
                height: 72.0,
                rotation_degrees: 0.0,
            },
            style: VisualStyle::default(),
            kind: VisualObjectKind::Primitive { shape },
        }
    }

    fn document_with_object(object: VisualObject) -> VisualDiagramDocument {
        let mut scene = VisualScene::new("Wire title");
        scene.objects.push(object);
        VisualDiagramDocument::from_fresh_scene(scene).unwrap()
    }

    #[test]
    fn diagram_drawio_wire_event_intake_bounds_structure_and_refuses_active_markup() {
        let valid = br#"<?xml version="1.0" encoding="UTF-8"?><mxGraphModel><root><mxCell id="0"/></root></mxGraphModel>"#;
        let events = read_xml_events(valid).unwrap();
        assert!(
            matches!(events.first(), Some(XmlEvent::Start(element)) if element.name == "mxGraphModel")
        );

        assert_eq!(
            read_xml_events(b"<mxGraphModel><root></mxGraphModel>"),
            Err(DrawioWireError::MalformedXml)
        );
        assert_eq!(
            read_xml_events(b"<!DOCTYPE mxGraphModel [<!ENTITY x 'y'>]><mxGraphModel/>"),
            Err(DrawioWireError::Unsupported("DTD declarations"))
        );
        assert_eq!(
            read_xml_events(b"<?xml-stylesheet href='x'?><mxGraphModel/>"),
            Err(DrawioWireError::Unsupported("processing instructions"))
        );
        assert_eq!(
            read_xml_events(b"<mxGraphModel dx='0' dx='1'/>"),
            Err(DrawioWireError::DuplicateAttribute)
        );
    }

    #[test]
    fn diagram_drawio_wire_event_intake_enforces_input_depth_and_attribute_limits() {
        let oversized = vec![b' '; MAX_DRAWIO_XML_BYTES + 1];
        assert_eq!(
            read_xml_events(&oversized),
            Err(DrawioWireError::InputTooLarge {
                max_bytes: MAX_DRAWIO_XML_BYTES
            })
        );

        let mut too_deep = String::from("<mxGraphModel>");
        for _ in 0..MAX_DRAWIO_XML_DEPTH {
            too_deep.push_str("<x>");
        }
        for _ in 0..MAX_DRAWIO_XML_DEPTH {
            too_deep.push_str("</x>");
        }
        too_deep.push_str("</mxGraphModel>");
        assert_eq!(
            read_xml_events(too_deep.as_bytes()),
            Err(DrawioWireError::LimitExceeded("nesting-depth"))
        );

        let attributes = (0..=MAX_DRAWIO_ATTRIBUTES_PER_ELEMENT)
            .map(|index| format!(" a{index}=\"x\""))
            .collect::<String>();
        let too_many = format!("<mxGraphModel{attributes}/>");
        assert_eq!(
            read_xml_events(too_many.as_bytes()),
            Err(DrawioWireError::LimitExceeded("attributes-per-element"))
        );

        let comments = format!(
            "{}<mxGraphModel/>",
            "<!--x-->".repeat(MAX_DRAWIO_XML_EVENTS + 1)
        );
        assert_eq!(
            read_xml_events(comments.as_bytes()),
            Err(DrawioWireError::LimitExceeded("event-count"))
        );
    }

    #[test]
    fn diagram_drawio_wire_imports_layered_shapes_text_and_anchored_connectors() {
        let imported = import_drawio_xml(GOLDEN_DRAWIO.as_bytes()).unwrap();
        let scene = imported.document.scene();

        assert_eq!(scene.layers.len(), 2);
        assert_eq!(scene.layers[0].name, "Background");
        assert_eq!(scene.layers[1].name, "Labels");
        assert!(imported.report.issues.iter().any(|issue| {
            issue.source_id.as_deref() == Some("start")
                && issue.property == "vertex-label-split-to-text"
        }));
        assert!(
            imported
                .report
                .issues
                .iter()
                .any(|issue| { issue.property == "page-name-not-retained" })
        );

        let start = scene
            .objects
            .iter()
            .find(|object| {
                matches!(
                    &object.kind,
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle
                    }
                )
            })
            .unwrap();
        assert_eq!(start.transform.x, 70.0);
        assert_eq!(start.transform.y, 50.0);
        assert_eq!(start.transform.rotation_degrees, 30.0);
        assert_eq!(start.style.fill, VisualColor::rgba(224, 240, 255, 255));
        assert_eq!(start.style.border, VisualColor::rgba(18, 52, 86, 255));
        assert_eq!(start.style.opacity, 0.85);

        let free_text = scene
            .objects
            .iter()
            .find_map(|object| match &object.kind {
                VisualObjectKind::Text { text } if text.starts_with("First") => {
                    Some((object, text))
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(free_text.1, "First\r\n東京 & more <\"");
        assert_eq!(free_text.0.style.font_family, "A&B");
        assert_eq!(free_text.0.transform.rotation_degrees, -15.0);

        let connector = scene
            .objects
            .iter()
            .find_map(|object| match &object.kind {
                VisualObjectKind::Connector { connector } => Some(connector),
                _ => None,
            })
            .unwrap();
        assert_eq!(connector.route, VisualConnectorRoute::Elbow);
        assert_eq!(connector.direction, VisualConnectorDirection::Forward);
        assert_eq!(connector.from.object_id, start.id);
        assert_eq!(connector.from.anchor, VisualAnchor::East);
        assert_eq!(connector.to.anchor, VisualAnchor::West);
        assert_eq!(connector.label, "flow & data");
    }

    #[test]
    fn diagram_drawio_wire_export_is_a_lossless_escaped_canonical_roundtrip() {
        let imported = import_drawio_xml(GOLDEN_DRAWIO.as_bytes()).unwrap();
        let first = export_drawio_xml(&imported.document).unwrap();
        assert_eq!(first.report.total_issues, 0);
        let xml = std::str::from_utf8(&first.bytes).unwrap();
        assert!(xml.contains("fontFamily=A&amp;B"));
        assert!(xml.contains("First&#13;&#10;東京 &amp; more &lt;&quot;"));
        assert!(xml.contains("value=\"A &amp; one\""));
        assert!(xml.contains("source=\"v2\" target=\"v4\""));

        let second_document = import_drawio_xml(&first.bytes).unwrap().document;
        let second = export_drawio_xml(&second_document).unwrap();
        assert_eq!(first.bytes, second.bytes);

        let direct_model = b"<mxGraphModel><root><mxCell id='0'/></root></mxGraphModel>";
        assert_eq!(
            import_drawio_xml(direct_model)
                .unwrap()
                .document
                .scene()
                .layers
                .len(),
            1
        );
    }

    #[test]
    fn diagram_drawio_wire_reports_inferred_anchors_in_rotated_local_space() {
        let imported = import_minimal(
            "<mxCell id=\"source\" style=\"shape=rectangle;rotation=90;\" vertex=\"1\" parent=\"layer\"><mxGeometry x=\"0\" y=\"0\" width=\"100\" height=\"50\" as=\"geometry\"/></mxCell><mxCell id=\"target\" style=\"shape=ellipse;\" vertex=\"1\" parent=\"layer\"><mxGeometry x=\"200\" y=\"0\" width=\"100\" height=\"50\" as=\"geometry\"/></mxCell><mxCell id=\"link\" edge=\"1\" parent=\"layer\" source=\"source\" target=\"target\"><mxGeometry relative=\"1\" as=\"geometry\"/></mxCell>",
        )
        .unwrap();
        assert_eq!(imported.report.total_issues, 2);
        assert!(imported.report.issues.iter().any(|issue| {
            issue.source_id.as_deref() == Some("link") && issue.property == "source-anchor-inferred"
        }));
        let connector = imported
            .document
            .scene()
            .objects
            .iter()
            .find_map(|object| match &object.kind {
                VisualObjectKind::Connector { connector } => Some(connector),
                _ => None,
            })
            .unwrap();
        assert_eq!(connector.from.anchor, VisualAnchor::North);
        assert_eq!(connector.to.anchor, VisualAnchor::West);
    }

    #[test]
    fn diagram_drawio_wire_refuses_hostile_and_unrepresentable_xml() {
        let cases = [
            (
                "duplicate cell IDs",
                "<mxCell id=\"1\" parent=\"0\"/><mxCell id=\"1\" parent=\"0\"/>",
            ),
            (
                "remote or image-backed styles",
                "<mxCell id=\"shape\" value=\"x\" style=\"shape=rectangle;image=https://example.invalid/a.png;\" vertex=\"1\" parent=\"layer\"><mxGeometry x=\"0\" y=\"0\" width=\"20\" height=\"20\" as=\"geometry\"/></mxCell>",
            ),
            (
                "non-ASCII color syntax",
                "<mxCell id=\"shape\" value=\"x\" style=\"shape=rectangle;fillColor=#€abc;\" vertex=\"1\" parent=\"layer\"><mxGeometry x=\"0\" y=\"0\" width=\"20\" height=\"20\" as=\"geometry\"/></mxCell>",
            ),
            (
                "active HTML entity labels",
                "<mxCell id=\"shape\" value=\"A&amp;nbsp;B\" style=\"shape=text;html=1;\" vertex=\"1\" parent=\"layer\"><mxGeometry x=\"0\" y=\"0\" width=\"20\" height=\"20\" as=\"geometry\"/></mxCell>",
            ),
            (
                "text boxes with unsupported fill",
                "<mxCell id=\"shape\" value=\"x\" style=\"shape=text;fillColor=#ffffff;\" vertex=\"1\" parent=\"layer\"><mxGeometry x=\"0\" y=\"0\" width=\"20\" height=\"20\" as=\"geometry\"/></mxCell>",
            ),
            (
                "nested and swimlane geometry",
                "<mxCell id=\"group\" parent=\"0\" style=\"swimlane;\"/><mxCell id=\"shape\" value=\"x\" style=\"shape=rectangle;\" vertex=\"1\" parent=\"group\"><mxGeometry x=\"0\" y=\"0\" width=\"20\" height=\"20\" as=\"geometry\"/></mxCell>",
            ),
            (
                "missing connector endpoint",
                "<mxCell id=\"edge\" edge=\"1\" parent=\"layer\" source=\"missing\" target=\"missing\"><mxGeometry relative=\"1\" as=\"geometry\"/></mxCell>",
            ),
        ];
        for (reason, cells) in cases {
            assert!(import_minimal(cells).is_err(), "accepted {reason}");
        }

        for xml in [
            "<mxGraphModel dx=\"0\"dx=\"1\"/>",
            "<mxGraphModel dx=\"0<1\"/>",
            "<mxGraphModel><root><mxCell id=\"0\"/></root></mxGraphModel><mxGraphModel/>",
            "<mxfile compressed=\"true\"><diagram><mxGraphModel/></diagram></mxfile>",
            "<mxfile><diagram/><diagram/></mxfile>",
            "<!DOCTYPE mxGraphModel [<!ENTITY x 'y'>]><mxGraphModel/>",
            "<mxGraphModel><root>&custom;</root></mxGraphModel>",
        ] {
            assert!(import_drawio_xml(xml.as_bytes()).is_err(), "accepted {xml}");
        }
        let namespaced =
            b"<mxGraphModel xmlns=\"urn:external\"><root><mxCell id=\"0\"/></root></mxGraphModel>";
        assert!(import_drawio_xml(namespaced).is_err());

        let mut too_many_cells = String::from("<mxGraphModel><root><mxCell id=\"0\"/>");
        for index in 0..MAX_DRAWIO_CELLS {
            too_many_cells.push_str(&format!(
                "<mxCell id=\"l{index}\" parent=\"0\" value=\"Layer\"/>"
            ));
        }
        too_many_cells.push_str("</root></mxGraphModel>");
        assert_eq!(
            import_drawio_xml(too_many_cells.as_bytes()).unwrap_err(),
            DrawioWireError::LimitExceeded("cell-count")
        );

        let long_endpoint = "e".repeat(MAX_DRAWIO_ID_BYTES + 1);
        let dangling = format!(
            "<mxCell id=\"shape\" style=\"shape=rectangle;\" vertex=\"1\" parent=\"layer\"><mxGeometry x=\"0\" y=\"0\" width=\"20\" height=\"20\" as=\"geometry\"/></mxCell><mxCell id=\"edge\" edge=\"1\" parent=\"layer\" source=\"{long_endpoint}\" target=\"shape\"><mxGeometry relative=\"1\" as=\"geometry\"/></mxCell>"
        );
        assert_eq!(
            import_minimal(&dangling).unwrap_err(),
            DrawioWireError::LimitExceeded("endpoint-ID-bytes")
        );
    }

    #[test]
    fn diagram_drawio_wire_export_fails_closed_without_partial_bytes() {
        let cylinder = primitive(2, VisualPrimitiveShape::Cylinder);
        let document = document_with_object(cylinder);
        match export_drawio_xml(&document) {
            Err(DrawioWireError::UnsupportedContent(report)) => {
                assert!(
                    report
                        .issues
                        .iter()
                        .any(|issue| { issue.property == "primitive-shape-not-supported" })
                );
            }
            Ok(partial) => panic!(
                "unsupported scene returned publishable bytes: {} bytes",
                partial.bytes.len()
            ),
            Err(other) => panic!("unexpected export result: {other:?}"),
        }

        for delimiter in [";", "="] {
            let mut unsafe_font = primitive(2, VisualPrimitiveShape::Rectangle);
            unsafe_font.style.font_family = format!("A{delimiter}shape=ellipse");
            let document = document_with_object(unsafe_font);
            assert!(matches!(
                export_drawio_xml(&document),
                Err(DrawioWireError::UnsupportedContent(report))
                    if report.issues.iter().any(|issue| issue.property == "font-family-not-representable")
            ));
        }

        let mut scene = VisualScene::new("Wire title");
        scene.objects.extend(
            (2..=MAX_DRAWIO_ISSUES as u64 + 2)
                .map(|id| primitive(id, VisualPrimitiveShape::Cylinder)),
        );
        let document = VisualDiagramDocument::from_fresh_scene(scene).unwrap();
        match export_drawio_xml(&document) {
            Err(DrawioWireError::UnsupportedContent(report)) => {
                assert_eq!(report.total_issues, MAX_DRAWIO_ISSUES + 1);
                assert_eq!(report.issues.len(), MAX_DRAWIO_ISSUES);
            }
            other => panic!("expected bounded unsupported report, got {other:?}"),
        }
    }
}
