#![cfg(windows)]

use multi_launcher::xml_toolkit_engine::{XmlEngineError, XmlEngineWorker, XmlXPathResult};
use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

#[test]
fn secure_msxml_gate_proves_parse_xpath_xsd_and_local_resource_policy() {
    let worker = XmlEngineWorker::start().expect("MSXML 6 COM worker starts");
    let directory = tempfile::tempdir().expect("temporary schema root");
    let probe = TcpListener::bind(("127.0.0.1", 0)).expect("loopback-only request probe");
    probe.set_nonblocking(true).expect("nonblocking probe");
    let probe_url = format!(
        "http://127.0.0.1:{}/remote.xsd",
        probe.local_addr().unwrap().port()
    );
    let probe_hits = Arc::new(AtomicUsize::new(0));
    let stop_probe = Arc::new(AtomicBool::new(false));
    let server_hits = Arc::clone(&probe_hits);
    let server_stop = Arc::clone(&stop_probe);
    let probe_thread = thread::spawn(move || {
        while !server_stop.load(Ordering::Acquire) {
            match probe.accept() {
                Ok((mut stream, _)) => {
                    server_hits.fetch_add(1, Ordering::Relaxed);
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                }
                Err(_) => break,
            }
        }
    });

    let common_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:gate" xmlns:t="urn:gate" elementFormDefault="qualified">
  <xs:simpleType name="PositiveCount"><xs:restriction base="xs:int"><xs:minInclusive value="1"/></xs:restriction></xs:simpleType>
</xs:schema>"#;
    let imported_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:labels" xmlns:l="urn:labels">
  <xs:simpleType name="Label"><xs:restriction base="xs:string"><xs:minLength value="1"/></xs:restriction></xs:simpleType>
</xs:schema>"#;
    let root_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:gate" xmlns:t="urn:gate" xmlns:l="urn:labels" elementFormDefault="qualified">
  <xs:import namespace="urn:labels" schemaLocation="labels.xsd"/>
  <xs:include schemaLocation="common.xsd"/>
  <xs:element name="catalog"><xs:complexType><xs:sequence>
    <xs:element name="entry" maxOccurs="unbounded"><xs:complexType mixed="true">
      <xs:attribute name="id" type="l:Label" use="required"/>
      <xs:attribute name="count" type="t:PositiveCount" use="required"/>
    </xs:complexType></xs:element>
  </xs:sequence></xs:complexType></xs:element>
</xs:schema>"#;
    fs::write(directory.path().join("common.xsd"), common_xsd).unwrap();
    fs::write(directory.path().join("labels.xsd"), imported_xsd).unwrap();
    let root_xsd_path = directory.path().join("root.xsd");
    fs::write(&root_xsd_path, root_xsd).unwrap();

    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<t:catalog xmlns:t="urn:gate" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="urn:gate {probe_url}">
  <t:entry id="café" count="2">雪</t:entry>
</t:catalog>"#
    );
    let document = worker.parse(xml.clone()).expect("Unicode XML parses");

    let namespaces = vec![("g".to_owned(), "urn:gate".to_owned())];
    let selected = worker
        .query(document, "//g:entry", namespaces.clone())
        .expect("namespaced XPath node selection");
    let XmlXPathResult::Nodes(selected) = selected else {
        panic!("node query must return nodes");
    };
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].qualified_name, "t:entry");
    assert!(selected[0].value.contains("雪"));
    assert_eq!(selected[0].id.document(), document);
    assert!(matches!(
        worker
            .query(document, "(//g:entry)", namespaces.clone())
            .expect("parenthesized node-set XPath"),
        XmlXPathResult::Nodes(_)
    ));

    let attribute = worker
        .query(document, "//g:entry/@id", namespaces.clone())
        .expect("attribute XPath node selection");
    let XmlXPathResult::Nodes(attribute) = attribute else {
        panic!("attribute query must return nodes");
    };
    assert_eq!(attribute.len(), 1);
    assert_eq!(attribute[0].value, "café");

    for (expression, expected) in [
        ("count(//g:entry)", "1"),
        ("sum(//g:entry/@count)", "2"),
        ("string(//g:entry/@id)", "café"),
        ("boolean(//g:entry)", "true"),
        ("string-length(//g:entry/@id)", "4"),
        ("count(//g:entry) > 0", "true"),
        ("sum(//g:entry/@count) + 2", "4"),
    ] {
        let result = worker
            .query(document, expression, namespaces.clone())
            .expect("scalar XPath result");
        assert_eq!(result, XmlXPathResult::ScalarText(expected.to_owned()));
    }
    assert!(matches!(
        worker.query(document, "document('http://127.0.0.1/a')", Vec::new()),
        Err(XmlEngineError::XPath(message)) if message == "the XSLT document() function is disabled"
    ));
    assert!(matches!(
        worker.query(document, "//g:entry[", namespaces.clone()),
        Err(XmlEngineError::XPath(_))
    ));
    assert!(
        worker
            .validate_selected_schema(document, root_xsd_path.clone())
            .is_ok()
    );

    let invalid_xml = worker
        .parse(xml.replace("count=\"2\"", "count=\"0\""))
        .expect("well-formed but invalid XML still parses");
    assert!(matches!(
        worker.validate_selected_schema(invalid_xml, root_xsd_path),
        Err(XmlEngineError::SchemaValidation(_))
    ));

    let malformed = worker.parse("<root>\n<child></root>".to_owned());
    let Err(XmlEngineError::Parse(diagnostic)) = malformed else {
        panic!("malformed XML must produce a parse diagnostic");
    };
    assert!(diagnostic.line.is_some_and(|line| line >= 2));
    assert!(diagnostic.column.is_some());

    let external_xsd_path = directory.path().join("external.xsd");
    fs::write(
        &external_xsd_path,
        format!(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:import namespace="urn:remote" schemaLocation="{probe_url}"/></xs:schema>"#
        ),
    )
    .unwrap();
    assert!(matches!(
        worker.validate_selected_schema(document, external_xsd_path),
        Err(XmlEngineError::SchemaPolicy(_))
    ));
    let traversal_xsd_path = directory.path().join("traversal.xsd");
    fs::write(
        &traversal_xsd_path,
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:include schemaLocation="../outside.xsd"/></xs:schema>"#,
    )
    .unwrap();
    assert!(matches!(
        worker.validate_selected_schema(document, traversal_xsd_path),
        Err(XmlEngineError::SchemaPolicy(_))
    ));

    let doctype =
        format!(r#"<!DOCTYPE root [<!ENTITY remote SYSTEM "{probe_url}">]><root>&remote;</root>"#);
    assert!(matches!(
        worker.parse(doctype),
        Err(XmlEngineError::Parse(_))
    ));
    stop_probe.store(true, Ordering::Release);
    probe_thread.join().expect("probe server exits");
    assert_eq!(
        probe_hits.load(Ordering::Relaxed),
        0,
        "XML and XSD paths must not fetch loopback URLs"
    );
    worker
        .close_document(invalid_xml)
        .expect("invalid fixture closes");
    worker
        .close_document(document)
        .expect("parsed fixture closes");
    assert!(matches!(
        worker.query(document, "//g:entry", namespaces),
        Err(XmlEngineError::UnknownDocument)
    ));
}
