#![cfg(windows)]

use multi_launcher::xml_toolkit_engine::{
    XmlEngineError, XmlEngineWorker, XmlFormatMode, XmlXPathResult,
};

#[test]
fn formatting_uses_the_parsed_tree_and_preserves_xml_content() {
    let worker = XmlEngineWorker::start().expect("MSXML worker starts");
    let source = r#"<?xml version="1.0" encoding="UTF-8"?><?audit keep?><p:root xmlns:p="urn:format" label="one &amp; two"><p:empty/><p:mixed>left <p:b>bold</p:b> right</p:mixed><p:nested>left <p:b><p:i>bold</p:i></p:b> right</p:nested><p:preserved xml:space="preserve"> top <p:leaf>inner</p:leaf> bottom </p:preserved><p:code><![CDATA[<raw>]]></p:code><p:unicode>雪 café</p:unicode><!-- keep --></p:root><?after keep?>"#;
    let original = worker.parse(source.to_owned()).expect("UTF-8 XML parses");
    let namespaces = vec![("p".to_owned(), "urn:format".to_owned())];

    let pretty = worker
        .format(original, XmlFormatMode::Pretty)
        .expect("pretty output serializes from the parsed DOM");
    assert!(pretty.contains("<?xml"));
    assert!(pretty.contains("<?audit keep?>"));
    assert!(pretty.contains("<?after keep?>"));
    assert!(pretty.contains("\n  <p:empty"));
    assert!(pretty.contains("xmlns:p=\"urn:format\""));
    assert!(pretty.contains("label=\"one &amp; two\""));
    assert!(pretty.contains("<!-- keep -->"));
    assert!(pretty.contains("<![CDATA[<raw>]]>"));
    assert!(pretty.contains("雪 café"));
    assert!(pretty.contains(">left <p:b>bold</p:b> right</p:mixed>"));
    assert!(pretty.contains(">left <p:b><p:i>bold</p:i></p:b> right</p:nested>"));
    assert!(pretty.contains("> top <p:leaf>inner</p:leaf> bottom </p:preserved>"));

    let source_value = worker
        .query(original, "string(//p:mixed)", namespaces.clone())
        .expect("the source DOM remains queryable after formatting");
    let nested_source_value = worker
        .query(original, "string(//p:nested)", namespaces.clone())
        .expect("the nested mixed-content source remains queryable");
    let pretty_document = worker
        .parse(pretty)
        .expect("formatted output remains well formed");
    let pretty_value = worker
        .query(pretty_document, "string(//p:mixed)", namespaces.clone())
        .expect("formatted output can be queried");
    assert_eq!(
        source_value,
        XmlXPathResult::ScalarText("left bold right".into())
    );
    assert_eq!(pretty_value, source_value);
    assert_eq!(
        nested_source_value,
        XmlXPathResult::ScalarText("left bold right".into())
    );
    assert_eq!(
        worker
            .query(pretty_document, "string(//p:nested)", namespaces.clone())
            .expect("pretty nested mixed-content query succeeds"),
        nested_source_value
    );

    let minified = worker
        .format(original, XmlFormatMode::Minify)
        .expect("compact output serializes without mutating the source");
    assert!(minified.contains("left <p:b>bold</p:b> right"));
    let minified_document = worker
        .parse(minified)
        .expect("compact output remains well formed");
    assert_eq!(
        worker
            .query(minified_document, "string(//p:mixed)", namespaces)
            .expect("compact output query succeeds"),
        source_value
    );

    worker.close_document(minified_document).unwrap();
    worker.close_document(pretty_document).unwrap();
    worker.close_document(original).unwrap();
}

#[test]
fn malformed_and_non_utf8_declared_sources_have_parse_diagnostics() {
    let worker = XmlEngineWorker::start().expect("MSXML worker starts");
    let Err(XmlEngineError::Parse(malformed)) = worker.parse("<root>\n<child></root>".into())
    else {
        panic!("mismatched tags should report a parse error");
    };
    assert!(malformed.line.is_some_and(|line| line >= 2));
    assert!(malformed.column.is_some());

    let Err(XmlEngineError::Parse(encoding)) =
        worker.parse("<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><root>café</root>".into())
    else {
        panic!("String input with a non-UTF-8 declaration must be rejected");
    };
    assert!(encoding.message.contains("string input is UTF-8"));

    let utf8 = worker
        .parse("<?xml version='1.0' encoding='UTF-8'?><root>café</root>".into())
        .expect("UTF-8 declarations match the BSTR input representation");
    worker.close_document(utf8).unwrap();
}

#[test]
fn xpath_node_snapshots_have_an_aggregate_text_byte_limit() {
    let worker = XmlEngineWorker::start().expect("MSXML worker starts");
    let text = "x".repeat(1_500_000);
    let source = format!("<a><b><c>{text}</c></b></a>");
    let document = worker.parse(source).expect("bounded fixture parses");
    assert_eq!(
        worker.query(document, "//*", Vec::new()),
        Err(XmlEngineError::RetentionLimit("XPath result bytes"))
    );
    assert!(matches!(
        worker.query(document, "/a", Vec::new()),
        Ok(XmlXPathResult::Nodes(nodes)) if nodes.len() == 1
    ));
    worker.close_document(document).unwrap();
}
