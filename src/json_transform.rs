use std::collections::HashMap;
use std::fmt;

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Number;

/// A strict JSON parse or serialization error.
///
/// Parse failures include the line and column reported by `serde_json` so UI
/// callers can present a location without interpreting the display string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonTransformError {
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl fmt::Display for JsonTransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for JsonTransformError {}

impl From<serde_json::Error> for JsonTransformError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            message: error.to_string(),
            line: Some(error.line()),
            column: Some(error.column()),
        }
    }
}

impl JsonTransformError {
    fn serialization(error: serde_json::Error) -> Self {
        Self {
            message: error.to_string(),
            line: None,
            column: None,
        }
    }
}

/// A parsed JSON document that can be rendered in either supported format.
///
/// Object members retain their first-seen input order. Repeated member names
/// keep the last value, matching `serde_json::Value`'s existing parse
/// semantics while avoiding its default sorted-key map representation.
#[derive(Debug, Clone)]
pub struct JsonDocument {
    value: OrderedJsonValue,
}

impl JsonDocument {
    /// Parses one strict JSON value and rejects any non-whitespace trailing data.
    pub fn parse(input: &str) -> Result<Self, JsonTransformError> {
        let mut deserializer = serde_json::Deserializer::from_str(input);
        let value = OrderedJsonValue::deserialize(&mut deserializer)?;
        deserializer.end()?;
        Ok(Self { value })
    }

    /// Serializes the document with two-space indentation.
    pub fn pretty(&self) -> Result<String, JsonTransformError> {
        serde_json::to_string_pretty(&self.value).map_err(JsonTransformError::serialization)
    }

    /// Serializes the document without insignificant whitespace.
    pub fn minify(&self) -> Result<String, JsonTransformError> {
        serde_json::to_string(&self.value).map_err(JsonTransformError::serialization)
    }

    /// Builds a deterministic, read-only projection of this document for tree
    /// views. Values remain owned only by this document; the projection stores
    /// node metadata and bounded scalar previews rather than cloned subtrees.
    pub fn tree_projection(&self) -> JsonTreeProjection {
        let mut nodes = Vec::new();
        append_tree_node(&self.value, None, None, &mut nodes);
        JsonTreeProjection { nodes }
    }

    /// Searches decoded object keys and scalar values in deterministic
    /// document preorder. Matching is a case-sensitive literal substring
    /// search. Keys are object member names only; array indices and the root
    /// have no key text. Values include complete decoded strings and the
    /// serialized spelling of numbers, booleans, and null. Containers are not
    /// searched as summaries or serialized subtrees.
    pub fn search(&self, query: &str, fields: JsonSearchFields) -> Vec<JsonSearchMatch> {
        if query.is_empty() {
            return Vec::new();
        }
        let mut matches = Vec::new();
        let mut next_index = 0;
        append_json_search_matches(
            &self.value,
            None,
            query,
            fields,
            &mut next_index,
            &mut matches,
        );
        matches
    }

    /// Resolves a typed path against this parsed snapshot.
    pub fn resolve_path(&self, path: &JsonNodePath) -> Option<JsonValueRef<'_>> {
        value_at_path(&self.value, path).map(JsonValueRef::from)
    }

    /// Resolves a JSON Pointer against this document and returns its typed path.
    ///
    /// Object members are interpreted as keys even when their names look like
    /// array indices. Array members are interpreted as canonical decimal
    /// indices. A syntactically valid pointer to a missing value returns
    /// `Ok(None)`.
    pub fn resolve_pointer(&self, pointer: &str) -> Result<Option<JsonNodePath>, JsonPointerError> {
        let tokens = parse_pointer_tokens(pointer)?;
        let mut value = &self.value;
        let mut path = JsonNodePath::root();

        for token in tokens {
            let (segment, next) = match value {
                OrderedJsonValue::Object(members) => {
                    let Some((key, child)) = members.iter().find(|(key, _)| key == &token) else {
                        return Ok(None);
                    };
                    (JsonPathSegment::ObjectKey(key.clone()), child)
                }
                OrderedJsonValue::Array(values) => {
                    let Some(index) = parse_array_index(&token) else {
                        return Ok(None);
                    };
                    let Some(child) = values.get(index) else {
                        return Ok(None);
                    };
                    (JsonPathSegment::ArrayIndex(index), child)
                }
                _ => return Ok(None),
            };
            path.segments.push(segment);
            value = next;
        }

        Ok(Some(path))
    }

    /// Returns the copy-friendly value representation at `path`.
    ///
    /// Strings are copied as their decoded contents; all other JSON values use
    /// compact JSON serialization and retain object member order.
    pub fn copy_value(&self, path: &JsonNodePath) -> Result<Option<String>, JsonTransformError> {
        let Some(value) = value_at_path(&self.value, path) else {
            return Ok(None);
        };
        match value {
            OrderedJsonValue::String(value) => Ok(Some(value.clone())),
            value => serde_json::to_string(value)
                .map(Some)
                .map_err(JsonTransformError::serialization),
        }
    }

    /// Serializes the selected node as compact JSON, including string quotes.
    pub fn copy_subtree(&self, path: &JsonNodePath) -> Result<Option<String>, JsonTransformError> {
        value_at_path(&self.value, path)
            .map(serde_json::to_string)
            .transpose()
            .map_err(JsonTransformError::serialization)
    }
}

/// A typed segment in a path through one parsed JSON document.
///
/// Keeping object keys and array indices distinct avoids treating an object
/// member named `"0"` as the first array element.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum JsonPathSegment {
    ObjectKey(String),
    ArrayIndex(usize),
}

/// A path through one parsed JSON document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct JsonNodePath {
    segments: Vec<JsonPathSegment>,
}

impl JsonNodePath {
    pub fn root() -> Self {
        Self::default()
    }

    pub fn segments(&self) -> &[JsonPathSegment] {
        &self.segments
    }

    /// Returns the RFC 6901 JSON Pointer spelling for this path.
    pub fn to_json_pointer(&self) -> String {
        let mut pointer = String::new();
        for segment in &self.segments {
            pointer.push('/');
            match segment {
                JsonPathSegment::ObjectKey(key) => push_pointer_token(&mut pointer, key),
                JsonPathSegment::ArrayIndex(index) => {
                    pointer.push_str(&index.to_string());
                }
            }
        }
        pointer
    }

    /// Returns the display/copy label, using `(root)` for the root path.
    pub fn display_path(&self) -> String {
        let pointer = self.to_json_pointer();
        if pointer.is_empty() {
            "(root)".to_owned()
        } else {
            pointer
        }
    }
}

/// A JSON Pointer syntax error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonPointerError {
    MustStartWithSlash,
    InvalidEscape,
}

impl fmt::Display for JsonPointerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MustStartWithSlash => f.write_str("JSON Pointer must be empty or start with '/'"),
            Self::InvalidEscape => f.write_str("JSON Pointer contains an invalid '~' escape"),
        }
    }
}

impl std::error::Error for JsonPointerError {}

/// A JSON value's kind, suitable for read-only tree presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonNodeKind {
    Null,
    Boolean,
    Number,
    String,
    Array,
    Object,
}

/// A read-only borrowed view of a value in a parsed JSON snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonValueRef<'a> {
    Null,
    Boolean(bool),
    Number(&'a Number),
    String(&'a str),
    Array { len: usize },
    Object { len: usize },
}

impl JsonValueRef<'_> {
    pub fn kind(self) -> JsonNodeKind {
        match self {
            Self::Null => JsonNodeKind::Null,
            Self::Boolean(_) => JsonNodeKind::Boolean,
            Self::Number(_) => JsonNodeKind::Number,
            Self::String(_) => JsonNodeKind::String,
            Self::Array { .. } => JsonNodeKind::Array,
            Self::Object { .. } => JsonNodeKind::Object,
        }
    }
}

/// Which JSON text fields participate in literal search.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum JsonSearchFields {
    Keys,
    Values,
    #[default]
    Both,
}

/// One matched projected node. A single node is emitted once when its key,
/// scalar value, or both match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JsonSearchMatch {
    node: JsonTreeNodeId,
    key_matched: bool,
    value_matched: bool,
}

impl JsonSearchMatch {
    pub fn node(&self) -> JsonTreeNodeId {
        self.node
    }

    pub fn key_matched(&self) -> bool {
        self.key_matched
    }

    pub fn value_matched(&self) -> bool {
        self.value_matched
    }
}

fn append_json_search_matches(
    value: &OrderedJsonValue,
    object_key: Option<&str>,
    query: &str,
    fields: JsonSearchFields,
    next_index: &mut usize,
    matches: &mut Vec<JsonSearchMatch>,
) {
    let node = JsonTreeNodeId(*next_index);
    *next_index += 1;

    let key_matched = matches!(fields, JsonSearchFields::Keys | JsonSearchFields::Both)
        && object_key.is_some_and(|key| key.contains(query));
    let value_matched = matches!(fields, JsonSearchFields::Values | JsonSearchFields::Both)
        && match value {
            OrderedJsonValue::Null => "null".contains(query),
            OrderedJsonValue::Bool(value) => value.to_string().contains(query),
            OrderedJsonValue::Number(value) => value.to_string().contains(query),
            OrderedJsonValue::String(value) => value.contains(query),
            OrderedJsonValue::Array(_) | OrderedJsonValue::Object(_) => false,
        };

    if key_matched || value_matched {
        matches.push(JsonSearchMatch {
            node,
            key_matched,
            value_matched,
        });
    }

    match value {
        OrderedJsonValue::Array(values) => {
            for child in values {
                append_json_search_matches(child, None, query, fields, next_index, matches);
            }
        }
        OrderedJsonValue::Object(members) => {
            for (key, child) in members {
                append_json_search_matches(child, Some(key), query, fields, next_index, matches);
            }
        }
        _ => {}
    }
}

impl<'a> From<&'a OrderedJsonValue> for JsonValueRef<'a> {
    fn from(value: &'a OrderedJsonValue) -> Self {
        match value {
            OrderedJsonValue::Null => Self::Null,
            OrderedJsonValue::Bool(value) => Self::Boolean(*value),
            OrderedJsonValue::Number(value) => Self::Number(value),
            OrderedJsonValue::String(value) => Self::String(value),
            OrderedJsonValue::Array(values) => Self::Array { len: values.len() },
            OrderedJsonValue::Object(members) => Self::Object { len: members.len() },
        }
    }
}

/// Stable identity for one node within a deterministic tree projection.
///
/// IDs are pre-order indices and are scoped to the projection that produced
/// them. Use the associated [`JsonNodePath`] as the stable identity across a
/// rebuilt projection of an unchanged document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JsonTreeNodeId(usize);

impl JsonTreeNodeId {
    pub fn index(self) -> usize {
        self.0
    }
}

/// One row of a flattened, pre-order tree projection.
#[derive(Debug, Clone)]
pub struct JsonTreeNode {
    id: JsonTreeNodeId,
    parent: Option<JsonTreeNodeId>,
    segment: Option<JsonPathSegment>,
    kind: JsonNodeKind,
    preview: String,
    children: Vec<JsonTreeNodeId>,
}

impl JsonTreeNode {
    pub fn id(&self) -> JsonTreeNodeId {
        self.id
    }

    pub fn parent(&self) -> Option<JsonTreeNodeId> {
        self.parent
    }

    pub fn segment(&self) -> Option<&JsonPathSegment> {
        self.segment.as_ref()
    }

    pub fn kind(&self) -> JsonNodeKind {
        self.kind
    }

    /// A bounded scalar preview or a compact container summary.
    pub fn preview(&self) -> &str {
        &self.preview
    }

    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    pub fn children(&self) -> &[JsonTreeNodeId] {
        &self.children
    }
}

/// A deterministic, source-ordered, flattened projection of a JSON document.
#[derive(Debug, Clone)]
pub struct JsonTreeProjection {
    nodes: Vec<JsonTreeNode>,
}

impl JsonTreeProjection {
    pub fn root(&self) -> JsonTreeNodeId {
        JsonTreeNodeId(0)
    }

    pub fn nodes(&self) -> &[JsonTreeNode] {
        &self.nodes
    }

    pub fn node(&self, id: JsonTreeNodeId) -> Option<&JsonTreeNode> {
        self.nodes.get(id.0).filter(|node| node.id == id)
    }

    /// Reconstructs a typed path from parent links without storing a full path
    /// or serialized subtree on every ancestor node.
    pub fn path(&self, id: JsonTreeNodeId) -> Option<JsonNodePath> {
        let mut current = self.node(id)?;
        let mut segments = Vec::new();
        while let Some(parent) = current.parent {
            segments.push(current.segment.clone()?);
            current = self.node(parent)?;
        }
        segments.reverse();
        Some(JsonNodePath { segments })
    }
}

const JSON_TREE_PREVIEW_MAX_CHARS: usize = 80;

fn append_tree_node(
    value: &OrderedJsonValue,
    parent: Option<JsonTreeNodeId>,
    segment: Option<JsonPathSegment>,
    nodes: &mut Vec<JsonTreeNode>,
) -> JsonTreeNodeId {
    let id = JsonTreeNodeId(nodes.len());
    let (kind, preview) = match value {
        OrderedJsonValue::Null => (JsonNodeKind::Null, "null".to_owned()),
        OrderedJsonValue::Bool(value) => (JsonNodeKind::Boolean, value.to_string()),
        OrderedJsonValue::Number(value) => (JsonNodeKind::Number, value.to_string()),
        OrderedJsonValue::String(value) => (JsonNodeKind::String, bounded_preview(value)),
        OrderedJsonValue::Array(values) => (
            JsonNodeKind::Array,
            format!("Array ({} items)", values.len()),
        ),
        OrderedJsonValue::Object(members) => (
            JsonNodeKind::Object,
            format!("Object ({} items)", members.len()),
        ),
    };
    nodes.push(JsonTreeNode {
        id,
        parent,
        segment,
        kind,
        preview,
        children: Vec::new(),
    });

    match value {
        OrderedJsonValue::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                let child_id = append_tree_node(
                    child,
                    Some(id),
                    Some(JsonPathSegment::ArrayIndex(index)),
                    nodes,
                );
                nodes[id.0].children.push(child_id);
            }
        }
        OrderedJsonValue::Object(members) => {
            for (key, child) in members {
                let child_id = append_tree_node(
                    child,
                    Some(id),
                    Some(JsonPathSegment::ObjectKey(key.clone())),
                    nodes,
                );
                nodes[id.0].children.push(child_id);
            }
        }
        _ => {}
    }

    id
}

fn bounded_preview(value: &str) -> String {
    let mut chars = value.chars();
    let mut preview: String = chars.by_ref().take(JSON_TREE_PREVIEW_MAX_CHARS).collect();
    if chars.next().is_some() {
        preview.push('…');
    }
    preview
}

fn value_at_path<'a>(
    root: &'a OrderedJsonValue,
    path: &JsonNodePath,
) -> Option<&'a OrderedJsonValue> {
    let mut value = root;
    for segment in &path.segments {
        value = match (value, segment) {
            (OrderedJsonValue::Object(members), JsonPathSegment::ObjectKey(key)) => members
                .iter()
                .find(|(candidate, _)| candidate == key)
                .map(|(_, value)| value)?,
            (OrderedJsonValue::Array(values), JsonPathSegment::ArrayIndex(index)) => {
                values.get(*index)?
            }
            _ => return None,
        };
    }
    Some(value)
}

fn push_pointer_token(pointer: &mut String, token: &str) {
    for character in token.chars() {
        match character {
            '~' => pointer.push_str("~0"),
            '/' => pointer.push_str("~1"),
            character => pointer.push(character),
        }
    }
}

fn parse_pointer_tokens(pointer: &str) -> Result<Vec<String>, JsonPointerError> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    let Some(tokens) = pointer.strip_prefix('/') else {
        return Err(JsonPointerError::MustStartWithSlash);
    };

    tokens.split('/').map(decode_pointer_token).collect()
}

fn decode_pointer_token(token: &str) -> Result<String, JsonPointerError> {
    let mut decoded = String::with_capacity(token.len());
    let mut chars = token.chars();
    while let Some(character) = chars.next() {
        if character != '~' {
            decoded.push(character);
            continue;
        }
        match chars.next() {
            Some('0') => decoded.push('~'),
            Some('1') => decoded.push('/'),
            _ => return Err(JsonPointerError::InvalidEscape),
        }
    }
    Ok(decoded)
}

fn parse_array_index(token: &str) -> Option<usize> {
    if token.is_empty() || (token.len() > 1 && token.starts_with('0')) {
        return None;
    }
    if !token.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    token.parse().ok()
}

#[derive(Debug, Clone)]
enum OrderedJsonValue {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<OrderedJsonValue>),
    Object(Vec<(String, OrderedJsonValue)>),
}

impl<'de> Deserialize<'de> for OrderedJsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(OrderedJsonVisitor)
    }
}

struct OrderedJsonVisitor;

impl<'de> Visitor<'de> for OrderedJsonVisitor {
    type Value = OrderedJsonValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a strict JSON value")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(OrderedJsonValue::Null)
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(OrderedJsonValue::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(OrderedJsonValue::Number(Number::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(OrderedJsonValue::Number(Number::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Number::from_f64(value)
            .map(OrderedJsonValue::Number)
            .ok_or_else(|| E::custom("JSON numbers must be finite"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(OrderedJsonValue::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(OrderedJsonValue::String(value))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element()? {
            values.push(value);
        }
        Ok(OrderedJsonValue::Array(values))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut members: Vec<(String, OrderedJsonValue)> = Vec::new();
        let mut indices: HashMap<String, usize> = HashMap::new();
        while let Some((key, value)) = object.next_entry::<String, OrderedJsonValue>()? {
            if let Some(index) = indices.get(&key).copied() {
                members[index].1 = value;
            } else {
                indices.insert(key.clone(), members.len());
                members.push((key, value));
            }
        }
        Ok(OrderedJsonValue::Object(members))
    }
}

impl Serialize for OrderedJsonValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Number(value) => value.serialize(serializer),
            Self::String(value) => serializer.serialize_str(value),
            Self::Array(values) => {
                let mut sequence = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    sequence.serialize_element(value)?;
                }
                sequence.end()
            }
            Self::Object(members) => {
                let mut object = serializer.serialize_map(Some(members.len()))?;
                for (key, value) in members {
                    object.serialize_entry(key, value)?;
                }
                object.end()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search_paths(document: &JsonDocument, query: &str, fields: JsonSearchFields) -> Vec<String> {
        let projection = document.tree_projection();
        document
            .search(query, fields)
            .iter()
            .map(|matched| projection.path(matched.node()).unwrap().to_json_pointer())
            .collect()
    }

    #[test]
    fn json_search_modes_are_case_sensitive_union_once_and_preorder() {
        let document = JsonDocument::parse(
            r#"{"hit":"hit","arr":[{"hit":"value"}],"nested":{"hit":{"deep":"hit"}}}"#,
        )
        .unwrap();

        assert_eq!(
            search_paths(&document, "hit", JsonSearchFields::Keys),
            ["/hit", "/arr/0/hit", "/nested/hit"]
        );
        assert_eq!(
            search_paths(&document, "hit", JsonSearchFields::Values),
            ["/hit", "/nested/hit/deep"]
        );
        assert_eq!(
            search_paths(&document, "hit", JsonSearchFields::Both),
            ["/hit", "/arr/0/hit", "/nested/hit", "/nested/hit/deep"]
        );

        let projection = document.tree_projection();
        let matches = document.search("hit", JsonSearchFields::Both);
        let first = matches[0];
        assert_eq!(
            projection.path(first.node()).unwrap().to_json_pointer(),
            "/hit"
        );
        assert!(first.key_matched());
        assert!(first.value_matched());
        assert!(!matches[2].value_matched(), "container values are excluded");
        assert!(!matches[3].key_matched());
        assert!(matches[3].value_matched());
        assert!(document.search("HIT", JsonSearchFields::Both).is_empty());
        assert!(document.search("", JsonSearchFields::Both).is_empty());
    }

    #[test]
    fn json_search_uses_decoded_keys_and_full_unicode_scalar_values() {
        let long_value = format!("{}tail😀", "prefix-".repeat(30));
        let escaped_value = serde_json::to_string(&long_value).unwrap();
        let document = JsonDocument::parse(&format!(
            r#"{{"a\/b~x😀":{escaped_value},"array":["prefix-{}tail😀"]}}"#,
            "prefix-".repeat(30)
        ))
        .unwrap();
        let projection = document.tree_projection();

        assert_eq!(
            search_paths(&document, "b~x😀", JsonSearchFields::Keys),
            ["/a~1b~0x😀"]
        );
        assert_eq!(
            search_paths(&document, "tail😀", JsonSearchFields::Values),
            ["/a~1b~0x😀", "/array/0"]
        );
        assert!(
            projection
                .node(document.search("tail😀", JsonSearchFields::Values)[0].node())
                .unwrap()
                .preview()
                .len()
                < long_value.len(),
            "search must use full strings rather than bounded row previews"
        );
        assert!(
            document
                .search("TAIL😀", JsonSearchFields::Values)
                .is_empty()
        );
        assert!(document.search("0", JsonSearchFields::Keys).is_empty());
    }

    #[test]
    fn json_search_matches_canonical_primitive_spellings_but_not_containers() {
        let document = JsonDocument::parse(
            r#"{"number":-12.5,"truth":true,"falsehood":false,"nothing":null,"nested":{"needle":"hidden"}}"#,
        )
        .unwrap();

        for (query, path) in [
            ("-12.5", "/number"),
            ("tru", "/truth"),
            ("fals", "/falsehood"),
            ("nul", "/nothing"),
        ] {
            assert_eq!(
                search_paths(&document, query, JsonSearchFields::Values),
                [path]
            );
        }
        assert!(
            document
                .search("Object", JsonSearchFields::Values)
                .is_empty()
        );
        assert_eq!(
            search_paths(&document, "hidden", JsonSearchFields::Values),
            ["/nested/needle"]
        );
    }

    #[test]
    fn pretty_formats_objects_arrays_and_nested_values_in_input_order() {
        let document =
            JsonDocument::parse(r#"{"z":[1,{"second":true,"first":null}],"a":"text"}"#).unwrap();

        assert_eq!(
            document.pretty().unwrap(),
            "{\n  \"z\": [\n    1,\n    {\n      \"second\": true,\n      \"first\": null\n    }\n  ],\n  \"a\": \"text\"\n}"
        );
    }

    #[test]
    fn strict_json_primitives_and_escaped_strings_are_supported() {
        for (input, expected) in [
            ("\"\"", "\"\""),
            (r#""a\n\"b""#, r#""a\n\"b""#),
            ("-12", "-12"),
            ("true", "true"),
            ("false", "false"),
            ("null", "null"),
        ] {
            let document = JsonDocument::parse(input).unwrap();
            assert_eq!(document.minify().unwrap(), expected);
            assert!(document.pretty().is_ok(), "input {input:?}");
        }
    }

    #[test]
    fn minify_removes_whitespace_without_changing_order_or_values() {
        let document =
            JsonDocument::parse(" { \"third\" : [ true, null ], \"first\" : { \"n\" : 2.5 } } ")
                .unwrap();

        assert_eq!(
            document.minify().unwrap(),
            r#"{"third":[true,null],"first":{"n":2.5}}"#
        );
    }

    #[test]
    fn duplicate_object_members_keep_last_value_and_first_position() {
        let document = JsonDocument::parse(r#"{"b":1,"a":2,"b":3}"#).unwrap();
        assert_eq!(document.minify().unwrap(), r#"{"b":3,"a":2}"#);
    }

    #[test]
    fn malformed_input_exposes_structured_line_and_column() {
        let error = JsonDocument::parse("{\n  \"value\": }\n").unwrap_err();
        assert_eq!(error.line, Some(2));
        assert!(error.column.is_some_and(|column| column > 0));
        assert!(error.message.contains("line 2 column"));
    }

    #[test]
    fn valid_json_followed_by_trailing_data_is_rejected() {
        let error = JsonDocument::parse("{\"ok\":true} trailing").unwrap_err();
        assert_eq!(error.line, Some(1));
        assert!(error.column.is_some_and(|column| column > 0));
        assert!(error.message.contains("trailing characters"));
    }

    #[test]
    fn comments_and_trailing_commas_are_rejected() {
        for input in ["[1,]", "{\"a\":1,}", "{\"a\":1 // comment\n}"] {
            assert!(
                JsonDocument::parse(input).is_err(),
                "strict JSON parser accepted {input:?}"
            );
        }
    }

    #[test]
    fn tree_projection_is_flat_preorder_and_preserves_object_source_order() {
        let document =
            JsonDocument::parse(r#"{"first":[true,{"x":1}],"second":0,"first":[false,{"x":2}]}"#)
                .unwrap();
        let tree = document.tree_projection();

        assert_eq!(tree.root().index(), 0);
        assert_eq!(tree.nodes().len(), 6);
        assert_eq!(tree.node(tree.root()).unwrap().kind(), JsonNodeKind::Object);
        assert_eq!(tree.node(tree.root()).unwrap().child_count(), 2);

        let expected = [
            (None, JsonNodeKind::Object, "Object (2 items)"),
            (
                Some(JsonPathSegment::ObjectKey("first".into())),
                JsonNodeKind::Array,
                "Array (2 items)",
            ),
            (
                Some(JsonPathSegment::ArrayIndex(0)),
                JsonNodeKind::Boolean,
                "false",
            ),
            (
                Some(JsonPathSegment::ArrayIndex(1)),
                JsonNodeKind::Object,
                "Object (1 items)",
            ),
            (
                Some(JsonPathSegment::ObjectKey("x".into())),
                JsonNodeKind::Number,
                "2",
            ),
            (
                Some(JsonPathSegment::ObjectKey("second".into())),
                JsonNodeKind::Number,
                "0",
            ),
        ];
        for (index, (segment, kind, preview)) in expected.into_iter().enumerate() {
            let node = &tree.nodes()[index];
            assert_eq!(node.id().index(), index);
            assert_eq!(node.segment(), segment.as_ref());
            assert_eq!(node.kind(), kind);
            assert_eq!(node.preview(), preview);
            assert_eq!(
                tree.path(node.id()).unwrap().to_json_pointer(),
                match index {
                    0 => "",
                    1 => "/first",
                    2 => "/first/0",
                    3 => "/first/1",
                    4 => "/first/1/x",
                    5 => "/second",
                    _ => unreachable!(),
                }
            );
        }
    }

    #[test]
    fn tree_projection_supports_each_root_kind_and_empty_containers() {
        for (input, kind, preview, child_count) in [
            ("null", JsonNodeKind::Null, "null", 0),
            ("true", JsonNodeKind::Boolean, "true", 0),
            ("12.5", JsonNodeKind::Number, "12.5", 0),
            (r#""root""#, JsonNodeKind::String, "root", 0),
            ("[]", JsonNodeKind::Array, "Array (0 items)", 0),
            ("{}", JsonNodeKind::Object, "Object (0 items)", 0),
        ] {
            let document = JsonDocument::parse(input).unwrap();
            let tree = document.tree_projection();
            let root = tree.node(tree.root()).unwrap();
            assert_eq!(tree.nodes().len(), 1, "input {input:?}");
            assert_eq!(root.kind(), kind, "input {input:?}");
            assert_eq!(root.preview(), preview, "input {input:?}");
            assert_eq!(root.child_count(), child_count, "input {input:?}");
            assert_eq!(tree.path(root.id()).unwrap(), JsonNodePath::root());
        }
    }

    #[test]
    fn json_pointer_escaping_unicode_empty_keys_and_typed_zero_paths_resolve() {
        let document = JsonDocument::parse(
            r#"{"":{"a/b":{"~key":[{"0":"zero","😀":"smile"}]}} ,"0":"root zero","items":["array zero"]}"#,
        )
        .unwrap();

        let escaped_pointer = "//a~1b/~0key/0/0";
        let escaped_path = document.resolve_pointer(escaped_pointer).unwrap().unwrap();
        assert_eq!(escaped_path.to_json_pointer(), escaped_pointer);
        assert_eq!(
            document.resolve_path(&escaped_path),
            Some(JsonValueRef::String("zero"))
        );

        let unicode_path = document
            .resolve_pointer("//a~1b/~0key/0/😀")
            .unwrap()
            .unwrap();
        assert_eq!(
            document.resolve_path(&unicode_path),
            Some(JsonValueRef::String("smile"))
        );

        let object_zero = document.resolve_pointer("/0").unwrap().unwrap();
        assert_eq!(
            object_zero.segments(),
            &[JsonPathSegment::ObjectKey("0".to_owned())]
        );
        let array_zero = document.resolve_pointer("/items/0").unwrap().unwrap();
        assert_eq!(
            array_zero.segments(),
            &[
                JsonPathSegment::ObjectKey("items".to_owned()),
                JsonPathSegment::ArrayIndex(0)
            ]
        );
        assert_eq!(array_zero.display_path(), "/items/0");
        assert_eq!(JsonNodePath::root().to_json_pointer(), "");
        assert_eq!(JsonNodePath::root().display_path(), "(root)");
    }

    #[test]
    fn json_pointer_rejects_bad_escapes_and_reports_missing_or_invalid_paths() {
        let document = JsonDocument::parse(r#"{"arr":[1],"a~b":2}"#).unwrap();

        assert_eq!(
            document.resolve_pointer("relative"),
            Err(JsonPointerError::MustStartWithSlash)
        );
        for pointer in ["/bad~", "/bad~2"] {
            assert_eq!(
                document.resolve_pointer(pointer),
                Err(JsonPointerError::InvalidEscape),
                "pointer {pointer:?}"
            );
        }
        for pointer in ["/missing", "/arr/1", "/arr/01", "/arr/-", "/arr/nope"] {
            assert_eq!(
                document.resolve_pointer(pointer).unwrap(),
                None,
                "pointer {pointer:?}"
            );
        }
        assert!(document.resolve_pointer("/a~0b").unwrap().is_some());
    }

    #[test]
    fn copy_value_and_subtree_keep_distinct_string_semantics_and_json_order() {
        let document =
            JsonDocument::parse(r#"{"text":"line\nλ","number":2,"child":{"z":true,"a":[]}}"#)
                .unwrap();
        let string_path = document.resolve_pointer("/text").unwrap().unwrap();
        assert_eq!(
            document.copy_value(&string_path).unwrap().as_deref(),
            Some("line\nλ")
        );
        assert_eq!(
            document.copy_subtree(&string_path).unwrap().as_deref(),
            Some(r#""line\nλ""#)
        );

        let number_path = document.resolve_pointer("/number").unwrap().unwrap();
        assert_eq!(
            document.copy_value(&number_path).unwrap().as_deref(),
            Some("2")
        );
        let child_path = document.resolve_pointer("/child").unwrap().unwrap();
        assert_eq!(
            document.copy_value(&child_path).unwrap().as_deref(),
            Some(r#"{"z":true,"a":[]}"#)
        );
        assert_eq!(
            document
                .copy_subtree(&JsonNodePath::root())
                .unwrap()
                .as_deref(),
            Some(r#"{"text":"line\nλ","number":2,"child":{"z":true,"a":[]}}"#)
        );
    }

    #[test]
    fn string_tree_previews_are_bounded_by_unicode_characters() {
        let input = format!("\"{}\"", "😀".repeat(1000));
        let document = JsonDocument::parse(&input).unwrap();
        let tree = document.tree_projection();
        let preview = tree.node(tree.root()).unwrap().preview();

        assert_eq!(preview.chars().count(), JSON_TREE_PREVIEW_MAX_CHARS + 1);
        assert!(preview.ends_with('…'));
    }
}
