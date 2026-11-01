use alloc::sync::Arc;

use super::{parse, root_name};
use crate::element::{Node, XmlError};
use crate::name::{Namespace, QName};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const XML: &str = "http://www.w3.org/XML/1998/namespace";

fn malformed(bytes: &[u8]) -> bool {
    matches!(parse(bytes), Err(XmlError::Malformed(_)))
}

#[test]
fn every_node_kind_is_kept() {
    let xml = format!(
        "<?xml version=\"1.0\"?>\r\n<!--before--><?app go?>\n<w:r xmlns:w=\"{W}\" w:rsidR=\"1\">\
         <w:t>a &amp; b</w:t><![CDATA[x < y]]><!-- note --><?pi data?></w:r>\r\n<!--after-->"
    );
    let document = parse(xml.as_bytes()).unwrap();
    assert_eq!(document.declaration.as_deref(), Some("xml version=\"1.0\""));
    assert_eq!(
        document.prolog,
        [
            Node::Text("\r\n".to_owned()),
            Node::Comment("before".to_owned()),
            Node::ProcessingInstruction {
                content: "go".to_owned(),
                target: "app".to_owned(),
            },
            Node::Text("\n".to_owned()),
        ]
    );
    let root = &document.root;
    assert_eq!(root.name, QName::prefixed("w", "r", W));
    assert_eq!(root.namespace_declarations.len(), 1);
    assert_eq!(root.namespace_declarations[0].prefix, "w");
    assert_eq!(root.namespace_declarations[0].uri, W);
    assert_eq!(root.attributes.len(), 1);
    assert_eq!(root.attributes[0].name, QName::prefixed("w", "rsidR", W));
    assert_eq!(root.attributes[0].value, "1");
    assert_eq!(root.children.len(), 4);
    assert_eq!(
        root.child_elements().next().map(|text| &text.children),
        Some(&vec![Node::Text("a & b".to_owned())])
    );
    assert_eq!(
        root.children[1..],
        [
            Node::CData("x < y".to_owned()),
            Node::Comment(" note ".to_owned()),
            Node::ProcessingInstruction {
                content: "data".to_owned(),
                target: "pi".to_owned(),
            },
        ]
    );
    assert_eq!(
        document.epilog,
        [
            Node::Text("\r\n".to_owned()),
            Node::Comment("after".to_owned()),
        ]
    );
}

#[test]
fn text_outside_the_root_keeps_its_line_endings() {
    for line_break in ["\r\n", "\r", "\n"] {
        let xml = format!("<?xml version=\"1.0\"?>{line_break}<a/>{line_break}");
        let document = parse(xml.as_bytes()).unwrap();
        assert_eq!(document.prolog, [Node::Text(line_break.to_owned())]);
        assert_eq!(document.epilog, [Node::Text(line_break.to_owned())]);
    }
    let bare = parse(b"<?xml version=\"1.0\"?><a/>").unwrap();
    assert_eq!(bare.prolog, []);
    assert_eq!(bare.epilog, []);
}

#[test]
fn text_inside_the_root_is_unescaped_and_its_line_endings_normalized() {
    let document = parse(b"<a>1 &lt; 2 &gt; 0&#xD;&#x9;x&apos;y&quot;\r\nz\rw</a>").unwrap();
    assert_eq!(
        document.root.children,
        [Node::Text("1 < 2 > 0\r\tx'y\"\nz\nw".to_owned())]
    );
}

#[test]
fn attribute_values_are_normalized_and_their_references_resolved() {
    let document = parse(b"<a b=\"x&#xA;y\tz\r\nw&quot;&lt;&amp;&apos;\"/>").unwrap();
    assert_eq!(document.root.attributes[0].value, "x\ny z w\"<&'");
}

#[test]
fn a_document_type_declaration_is_an_error() {
    assert!(malformed(b"<?xml version=\"1.0\"?><!DOCTYPE a><a/>"));
}

#[test]
fn an_undeclared_prefix_is_an_error() {
    assert_eq!(
        parse(b"<w:document/>"),
        Err(XmlError::UndeclaredPrefix("w".to_owned()))
    );
    assert_eq!(
        parse(format!("<w:p xmlns:w=\"{W}\" w14:paraId=\"1\"/>").as_bytes()),
        Err(XmlError::UndeclaredPrefix("w14".to_owned()))
    );
}

#[test]
fn a_declaration_is_in_scope_only_inside_its_element() {
    let xml = format!("<a><w:p xmlns:w=\"{W}\"/><w:p/></a>");
    assert_eq!(
        parse(xml.as_bytes()),
        Err(XmlError::UndeclaredPrefix("w".to_owned()))
    );
}

#[test]
fn the_xml_prefix_needs_no_declaration() {
    let document = parse(b"<t xml:space=\"preserve\"> </t>").unwrap();
    assert_eq!(
        document.root.attributes[0].name.namespace,
        Namespace::from_uri(XML)
    );
}

#[test]
fn a_name_without_a_prefix_takes_the_default_namespace_and_an_attribute_none() {
    let xml = format!("<document xmlns=\"{W}\" val=\"1\"><body xmlns=\"\"/></document>");
    let document = parse(xml.as_bytes()).unwrap();
    assert_eq!(document.root.name.namespace, Namespace::from_uri(W));
    assert_eq!(document.root.attributes[0].name.namespace, Namespace::None);
    assert_eq!(
        document
            .root
            .child_elements()
            .next()
            .map(|body| &body.name.namespace),
        Some(&Namespace::None)
    );
}

#[test]
fn a_namespace_outside_the_table_is_kept_with_its_uri() {
    let uri = "http://documentgeneration.prodoctivity.com/2011/";
    let xml = format!("<pd:tag xmlns:pd=\"{uri}\" pd:key=\"a\"/>");
    let document = parse(xml.as_bytes()).unwrap();
    assert_eq!(
        document.root.name.namespace,
        Namespace::Other(Arc::from(uri))
    );
    assert_eq!(
        document.root.attributes[0].name.namespace,
        document.root.name.namespace
    );
}

#[test]
fn a_strict_uri_has_the_id_of_its_transitional_twin_and_is_kept_as_written() {
    let xml = format!("<w:document xmlns:w=\"{W_STRICT}\"/>");
    let document = parse(xml.as_bytes()).unwrap();
    assert_eq!(document.root.name.namespace, Namespace::from_uri(W));
    assert_eq!(document.root.namespace_declarations[0].uri, W_STRICT);
}

#[test]
fn markup_that_is_not_one_well_formed_element_is_malformed() {
    assert!(malformed(b"<a/><b/>"));
    assert!(malformed(b"<a></a><b></b>"));
    assert!(malformed(b"<a><b></a>"));
    assert!(malformed(b"<a>"));
    assert!(malformed(b""));
    assert!(malformed(b"<a>&unknown;</a>"));
}

/// ASCII `text` in UTF-16 with its byte order mark, little-endian.
fn utf16_little_endian(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(text.bytes().flat_map(|byte| [byte, 0]));
    bytes
}

/// ASCII `text` in UTF-16 with its byte order mark, big-endian.
fn utf16_big_endian(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFE, 0xFF];
    bytes.extend(text.bytes().flat_map(|byte| [0, byte]));
    bytes
}

#[test]
fn a_part_in_utf16_reads_as_the_same_part_in_utf8() {
    let body = format!("<w:t xmlns:w=\"{W}\" w:rsidR=\"1\">x y</w:t>");
    let expected = parse(body.as_bytes()).unwrap();
    assert_eq!(
        parse(&utf16_little_endian(&body)).unwrap().root,
        expected.root
    );
    assert_eq!(parse(&utf16_big_endian(&body)).unwrap().root, expected.root);
}

#[test]
fn a_character_outside_ascii_survives_utf16_in_both_byte_orders() {
    // <a>U+00E9 U+1F600</a>; the second character is the surrogate pair D83D DE00.
    let little = [
        0xFF, 0xFE, b'<', 0, b'a', 0, b'>', 0, 0xE9, 0, 0x3D, 0xD8, 0x00, 0xDE, b'<', 0, b'/', 0,
        b'a', 0, b'>', 0,
    ];
    let big = [
        0xFE, 0xFF, 0, b'<', 0, b'a', 0, b'>', 0, 0xE9, 0xD8, 0x3D, 0xDE, 0x00, 0, b'<', 0, b'/',
        0, b'a', 0, b'>',
    ];
    for bytes in [little, big] {
        assert_eq!(
            parse(&bytes).unwrap().root.children,
            [Node::Text("\u{e9}\u{1F600}".to_owned())]
        );
    }
}

#[test]
fn a_part_read_from_utf16_declares_utf8_because_it_writes_as_utf8() {
    let body = "<?xml version=\"1.0\" encoding=\"UTF-16\" standalone=\"yes\"?><a/>";
    let document = parse(&utf16_little_endian(body)).unwrap();
    assert_eq!(
        document.declaration.as_deref(),
        Some("xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"")
    );
    let bare = parse(&utf16_little_endian("<?xml version=\"1.0\"?><a/>")).unwrap();
    assert_eq!(bare.declaration.as_deref(), Some("xml version=\"1.0\""));
}

#[test]
fn a_part_that_only_declares_utf16_keeps_its_bytes_and_its_declaration() {
    let body = "<?xml version=\"1.0\" encoding=\"utf-16\"?><a/>";
    let document = parse(body.as_bytes()).unwrap();
    assert_eq!(
        document.declaration.as_deref(),
        Some("xml version=\"1.0\" encoding=\"utf-16\"")
    );
}

#[test]
fn a_utf16_part_with_half_a_character_or_a_lone_surrogate_is_malformed() {
    let mut odd = utf16_little_endian("<a/>");
    odd.push(b' ');
    assert!(malformed(&odd));
    let mut lone = utf16_little_endian("<a>");
    lone.extend([0x00, 0xD8]);
    lone.extend(utf16_little_endian("</a>").split_off(2));
    assert!(malformed(&lone));
}

#[test]
fn a_root_name_needs_only_the_start_tag() {
    let source = format!("<?xml version=\"1.0\"?><!--c--><w:r xmlns:w=\"{W}\"><x:y/><unclosed>");
    assert_eq!(
        root_name(source.as_bytes()),
        Some(QName::prefixed("w", "r", W))
    );
    assert_eq!(
        root_name(b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><root><v>\xE9</v></root>"),
        Some(QName::unprefixed("root", ""))
    );
}

#[test]
fn bytes_that_show_no_root_have_no_root_name() {
    assert_eq!(root_name(b""), None);
    assert_eq!(root_name(b"<?xml version=\"1.0\"?>"), None);
    assert_eq!(root_name(b"<!DOCTYPE a><a/>"), None);
    assert_eq!(root_name(b"<x:a/>"), None);
}
