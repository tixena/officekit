use super::{Attribute, Document, Element, NamespaceDeclaration, Node};
use crate::conformance::ConformanceClass;
use crate::name::{Namespace, QName};
use crate::parse::parse;
use crate::write::{Layout, WriteOptions};

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const BIBLIOGRAPHY: &str = "http://schemas.openxmlformats.org/officeDocument/2006/bibliography";
const BIBLIOGRAPHY_STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/bibliography";
const DS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/customXml";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const KEEP: WriteOptions = WriteOptions {
    layout: Layout::Compact,
    namespace_cleanup: false,
};

fn written(document: &Document) -> String {
    String::from_utf8(document.to_xml(KEEP)).unwrap()
}

#[test]
fn a_new_element_holds_only_its_name() {
    let element = Element::new(QName::prefixed("w", "p", W));
    assert_eq!(element.name, QName::prefixed("w", "p", W));
    assert_eq!(element.attributes, []);
    assert_eq!(element.children, []);
    assert_eq!(element.namespace_declarations, []);
}

#[test]
fn child_elements_skips_the_other_nodes() {
    let document = parse(b"<a><b/>text<!--c--><d/><?pi x?><![CDATA[e]]></a>").unwrap();
    let names: Vec<&str> = document
        .root
        .child_elements()
        .map(|element| element.name.local.as_str())
        .collect();
    assert_eq!(names, ["b", "d"]);
}

#[test]
fn an_attribute_is_found_by_namespace_and_local_name() {
    let mut element = Element::new(QName::prefixed("w", "p", W));
    element.attributes.push(Attribute {
        name: QName::prefixed("w", "rsidR", W_STRICT),
        value: "00A1".to_owned(),
    });
    element.attributes.push(Attribute {
        name: QName::unprefixed("rsidR", ""),
        value: "plain".to_owned(),
    });
    let wordprocessing = Namespace::from_uri(W);
    assert_eq!(element.attribute(&wordprocessing, "rsidR"), Some("00A1"));
    assert_eq!(element.attribute(&Namespace::None, "rsidR"), Some("plain"));
    assert_eq!(element.attribute(&wordprocessing, "rsidP"), None);
}

#[test]
fn set_class_declares_every_namespace_of_the_subtree_with_the_class_uri() {
    let transitional = format!(
        r#"<w:document xmlns:w="{W}" xmlns:w14="{W14}" xmlns:x="urn:x"><w:body><w:p xmlns:w="{W}"/></w:body></w:document>"#
    );
    let mut document = parse(transitional.as_bytes()).unwrap();
    document.root.set_class(ConformanceClass::Strict);
    assert_eq!(
        written(&document),
        format!(
            r#"<w:document xmlns:w="{W_STRICT}" xmlns:w14="{W14}" xmlns:x="urn:x"><w:body><w:p xmlns:w="{W_STRICT}"/></w:body></w:document>"#
        )
    );
    document.root.set_class(ConformanceClass::Transitional);
    assert_eq!(written(&document), transitional);
}

#[test]
fn a_new_document_has_a_line_break_after_its_declaration() {
    let mut root = Element::new(QName::prefixed("w", "document", W));
    root.namespace_declarations.push(NamespaceDeclaration {
        prefix: "w".to_owned(),
        uri: W.to_owned(),
    });
    let document = Document::new(
        r#"xml version="1.0" encoding="UTF-8" standalone="yes""#,
        root,
    );
    assert_eq!(document.prolog, [Node::Text("\r\n".to_owned())]);
    assert_eq!(
        String::from_utf8(document.to_xml(WriteOptions::default())).unwrap(),
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:document xmlns:w=\"{W}\"/>"
        )
    );
}

/// The value of the unprefixed `uri` of an `a:graphicData`, after `class` is set.
fn graphic_address(uri: &str, class: ConformanceClass) -> String {
    let source = format!(r#"<a:graphicData xmlns:a="{A}" uri="{uri}"/>"#);
    let mut document = parse(source.as_bytes()).unwrap();
    document.root.set_class(class);
    document
        .root
        .attribute(&Namespace::None, "uri")
        .unwrap()
        .to_owned()
}

#[test]
fn set_class_writes_a_graphic_data_address_in_the_class() {
    for (transitional, strict) in [
        (
            "http://schemas.openxmlformats.org/drawingml/2006/picture",
            "http://purl.oclc.org/ooxml/drawingml/picture",
        ),
        (
            "http://schemas.openxmlformats.org/drawingml/2006/chart",
            "http://purl.oclc.org/ooxml/drawingml/chart",
        ),
        (
            "http://schemas.openxmlformats.org/drawingml/2006/diagram",
            "http://purl.oclc.org/ooxml/drawingml/diagram",
        ),
        (
            "http://schemas.openxmlformats.org/drawingml/2006/lockedCanvas",
            "http://purl.oclc.org/ooxml/drawingml/lockedCanvas",
        ),
        (
            "http://schemas.openxmlformats.org/presentationml/2006/ole",
            "http://purl.oclc.org/ooxml/presentationml/ole",
        ),
    ] {
        assert_eq!(
            graphic_address(transitional, ConformanceClass::Strict),
            strict
        );
        assert_eq!(
            graphic_address(strict, ConformanceClass::Transitional),
            transitional
        );
    }
}

#[test]
fn set_class_leaves_a_graphic_data_address_with_one_spelling() {
    let shape = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
    assert_eq!(graphic_address(shape, ConformanceClass::Strict), shape);
    assert_eq!(
        graphic_address(shape, ConformanceClass::Transitional),
        shape
    );
}

#[test]
fn set_class_writes_a_schema_reference_in_the_class() {
    let source = format!(
        r#"<ds:schemaRef xmlns:ds="{DS}" ds:uri="http://schemas.openxmlformats.org/officeDocument/2006/bibliography"/>"#
    );
    let mut document = parse(source.as_bytes()).unwrap();
    document.root.set_class(ConformanceClass::Strict);
    let namespace = document.root.name.namespace.clone();
    assert_eq!(
        document.root.attribute(&namespace, "uri"),
        Some("http://purl.oclc.org/ooxml/officeDocument/bibliography")
    );
}

#[test]
fn set_class_leaves_the_minimum_version_of_a_diagram_extension() {
    let source = r#"<dsp:dataModelExt xmlns:dsp="http://schemas.microsoft.com/office/drawing/2008/diagram" minVer="http://schemas.openxmlformats.org/drawingml/2006/diagram"/>"#;
    let mut document = parse(source.as_bytes()).unwrap();
    document.root.set_class(ConformanceClass::Strict);
    assert_eq!(
        document.root.attribute(&Namespace::None, "minVer"),
        Some("http://schemas.openxmlformats.org/drawingml/2006/diagram")
    );
}

/// The prefix mappings of a binding element after `class` is set.
fn mappings_in(element: &str, mappings: &str, class: ConformanceClass) -> String {
    let source = format!(
        r#"<{element} xmlns:w="{W}" xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml" w:prefixMappings="{mappings}"/>"#
    );
    let mut document = parse(source.as_bytes()).unwrap();
    document.root.set_class(class);
    document
        .root
        .attribute(&Namespace::from_uri(W), "prefixMappings")
        .unwrap()
        .to_owned()
}

#[test]
fn set_class_writes_the_address_of_a_moved_part_in_a_binding() {
    for element in ["w:dataBinding", "w15:dataBinding"] {
        let transitional = format!("xmlns:ns0='{BIBLIOGRAPHY}' ");
        let strict = format!("xmlns:ns0='{BIBLIOGRAPHY_STRICT}' ");
        assert_eq!(
            mappings_in(element, &transitional, ConformanceClass::Strict),
            strict
        );
        assert_eq!(
            mappings_in(element, &strict, ConformanceClass::Transitional),
            transitional
        );
    }
}

#[test]
fn set_class_leaves_every_other_address_in_a_binding() {
    for address in [
        "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
        "http://schemas.microsoft.com/office/2006/coverPageProps",
        "http://example.org/data",
        "http://schemas.openxmlformats.org/officeDocument/2006/bibliography2",
        W,
    ] {
        let mappings =
            format!("xmlns:ns0='{address}' xmlns:ns1='http://purl.org/dc/elements/1.1/'");
        assert_eq!(
            mappings_in("w:dataBinding", &mappings, ConformanceClass::Strict),
            mappings
        );
    }
}

#[test]
fn set_class_writes_each_mapping_of_a_binding() {
    let mappings = format!("xmlns:a='{BIBLIOGRAPHY}' xmlns:b='urn:x' xmlns:c='{BIBLIOGRAPHY}'");
    assert_eq!(
        mappings_in("w:dataBinding", &mappings, ConformanceClass::Strict),
        format!("xmlns:a='{BIBLIOGRAPHY_STRICT}' xmlns:b='urn:x' xmlns:c='{BIBLIOGRAPHY_STRICT}'")
    );
}
