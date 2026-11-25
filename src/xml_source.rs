//! Source admission shared by the XML editor and file loader.
use crate::xml_toolkit_engine::MAX_XML_INPUT_BYTES;

/// Validate a protected Unicode clipboard read before replacing XML source.
pub fn validate_clipboard_source(source: &str) -> Result<(), String> {
    if source.len() > MAX_XML_INPUT_BYTES {
        return Err(format!(
            "Clipboard XML exceeds the {MAX_XML_INPUT_BYTES}-byte source limit."
        ));
    }
    validate_utf8_declaration(source)
}

pub(crate) fn validate_utf8_declaration(source: &str) -> Result<(), String> {
    let Some(after_open) = source.strip_prefix("<?xml") else {
        return Ok(());
    };
    if !after_open.starts_with("?>") && !after_open.chars().next().is_some_and(char::is_whitespace)
    {
        // Processing-instruction targets such as `xml-stylesheet` are not an
        // XML declaration and remain the XML parser's responsibility.
        return Ok(());
    }
    let end = source
        .find("?>")
        .ok_or_else(|| "XML declaration is incomplete.".to_owned())?;
    let declaration = &source[5..end];
    let Some(encoding) = declaration_encoding(declaration)? else {
        return Ok(());
    };
    if !encoding.eq_ignore_ascii_case("utf-8") {
        return Err(format!(
            "XML declares unsupported encoding {encoding:?}; only UTF-8 is supported."
        ));
    }
    Ok(())
}

fn declaration_encoding(declaration: &str) -> Result<Option<&str>, String> {
    let bytes = declaration.as_bytes();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor == bytes.len() {
            break;
        }
        let name_start = cursor;
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_alphanumeric()
                || bytes[cursor] == b'_'
                || bytes[cursor] == b'-')
        {
            cursor += 1;
        }
        if cursor == name_start {
            cursor += 1;
            continue;
        }
        let name = &declaration[name_start..cursor];
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'=' {
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let Some(quote @ (b'\'' | b'"')) = bytes.get(cursor).copied() else {
            continue;
        };
        cursor += 1;
        let value_start = cursor;
        while cursor < bytes.len() && bytes[cursor] != quote {
            cursor += 1;
        }
        if cursor == bytes.len() {
            return Err("XML declaration has an unterminated attribute.".into());
        }
        let value = &declaration[value_start..cursor];
        cursor += 1;
        if name == "encoding" {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// Format XML diagnostics consistently across utility frontends.
pub fn xml_error_message(error: &crate::xml_toolkit_engine::XmlEngineError) -> String {
    use crate::xml_toolkit_engine::XmlEngineError;
    match error {
        XmlEngineError::Parse(diagnostic) => {
            let location = match (diagnostic.line, diagnostic.column) {
                (Some(line), Some(column)) => format!("line {line}, column {column}: "),
                (Some(line), None) => format!("line {line}: "),
                (None, Some(column)) => format!("column {column}: "),
                (None, None) => String::new(),
            };
            format!("{location}{}", diagnostic.message)
        }
        XmlEngineError::InputTooLarge { limit } => {
            format!("source exceeds the {limit}-byte input limit")
        }
        XmlEngineError::UnsupportedPlatform => {
            "the XML backend is supported on Windows only".into()
        }
        other => format!("{other:?}"),
    }
}
