use super::{PartEvent, PartReader};
use crate::element::{Element, Node};
use crate::name::Namespace;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// The element a `Start` event began.
fn started(event: Option<PartEvent>) -> Element {
    match event {
        Some(PartEvent::Start(element)) => Some(element),
        Some(PartEvent::Declaration(_) | PartEvent::End | PartEvent::Node(_)) | None => None,
    }
    .unwrap()
}

#[test]
fn a_part_reads_forward_and_loads_the_element_it_is_on() {
    let xml = format!(
        "<?xml version=\"1.0\"?><w:document xmlns:w=\"{W}\"><w:body><w:p><w:r><w:t>a &amp; b</w:t></w:r></w:p>\
         <!--c--><w:sectPr/></w:body></w:document>"
    );
    let mut reader = PartReader::new(xml.as_bytes());
    assert_eq!(
        reader.read_event().unwrap(),
        Some(PartEvent::Declaration("xml version=\"1.0\"".to_owned()))
    );
    let root = started(reader.read_event().unwrap());
    assert_eq!(
        (
            root.name.local.as_str(),
            root.namespace_declarations.len(),
            root.children.len()
        ),
        ("document", 1, 0)
    );
    let body = started(reader.read_event().unwrap());
    assert_eq!(
        (body.name.local.as_str(), &body.name.namespace),
        ("body", &Namespace::from_uri(W))
    );
    let start = started(reader.read_event().unwrap());
    let element = reader.read_element(start).unwrap();
    let text_element = element
        .child_elements()
        .flat_map(Element::child_elements)
        .next()
        .unwrap();
    assert_eq!(text_element.children, [Node::Text("a & b".to_owned())]);
    assert_eq!(
        reader.read_event().unwrap(),
        Some(PartEvent::Node(Node::Comment("c".to_owned())))
    );
    assert!(matches!(
        reader.read_event().unwrap(),
        Some(PartEvent::Node(Node::Element(section)))
            if section.name.local == "sectPr" && section.name.namespace == Namespace::from_uri(W)
    ));
    assert_eq!(reader.read_event().unwrap(), Some(PartEvent::End));
    assert_eq!(reader.read_event().unwrap(), Some(PartEvent::End));
    assert_eq!(reader.read_event().unwrap(), None);
}

#[test]
fn the_declaration_of_a_part_is_an_event_followed_by_its_line_break() {
    let mut reader = PartReader::new(
        &b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<a/>"[..],
    );
    assert_eq!(
        reader.read_event().unwrap(),
        Some(PartEvent::Declaration(
            "xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"".to_owned()
        ))
    );
    assert_eq!(
        reader.read_event().unwrap(),
        Some(PartEvent::Node(Node::Text("\r\n".to_owned())))
    );
    assert!(matches!(
        reader.read_event().unwrap(),
        Some(PartEvent::Node(Node::Element(element))) if element.name.local == "a"
    ));
    assert_eq!(reader.read_event().unwrap(), None);
}
