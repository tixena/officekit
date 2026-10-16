use super::{PartEvent, PartReader};
use crate::markup::wordprocessing::{ParagraphContent, TextRunContent, read};
use crate::packaging::package::Package;
use crate::packaging::part_name::PartName;
use crate::stream::writer::PartWriter;
use crate::xml::element::{Element, Node};
use crate::xml::name::Namespace;
use crate::xml::write::Layout;

const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const W15: &str = "http://schemas.microsoft.com/office/word/2012/wordml";

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
    let paragraph = read::paragraph(&element);
    assert!(matches!(
        paragraph.content.as_slice(),
        [ParagraphContent::TextRun(run)]
            if matches!(run.content.as_slice(), [TextRunContent::Text(text)] if text.value == "a & b")
    ));
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
fn a_part_read_from_its_package_streams_a_root_that_declares_only_the_prefixes_it_uses() {
    let main = PartName::new("/word/document.xml").unwrap();
    let declarations =
        format!("xmlns:w=\"{W}\" xmlns:mc=\"{MC}\" xmlns:w14=\"{W14}\" xmlns:w15=\"{W15}\"");
    let mut package = Package::new();
    package
        .set_part_bytes(
            &main,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
            format!(
                "<w:document {declarations} mc:Ignorable=\"w14 w15\"><w:body>\n  <w:p w14:paraId=\"00000001\"/>\n</w:body></w:document>"
            )
            .as_bytes(),
        )
        .unwrap();
    let mut reader = PartReader::from_part(&package, &main).unwrap();
    let mut sink = Vec::new();
    let mut out = PartWriter::new(&mut sink, Layout::Compact);
    while let Some(event) = reader.read_event().unwrap() {
        out.write_event(event).unwrap();
    }
    assert_eq!(
        String::from_utf8(sink).unwrap(),
        format!(
            "<w:document xmlns:w=\"{W}\" xmlns:mc=\"{MC}\" xmlns:w14=\"{W14}\" mc:Ignorable=\"w14\">\
             <w:body><w:p w14:paraId=\"00000001\"/></w:body></w:document>"
        )
    );
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
