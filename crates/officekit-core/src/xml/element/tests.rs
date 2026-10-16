use super::{Attribute, Document, Element, NamespaceDeclaration, Node};
use crate::xml::conformance::ConformanceClass;
use crate::xml::name::{Namespace, QName};
use crate::xml::parse::parse;
use crate::xml::write::{Layout, WriteOptions};

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
