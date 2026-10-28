use officekit_markup::spreadsheet::{Cell, Worksheet};
use officekit_markup::wordprocessing::{Document, Paragraph};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Document as Part, Element};
use officekit_xml::error::{ReadError, WriteError};
use officekit_xml::parse::parse;
use officekit_xml::write::{Layout, WriteOptions};

use super::Transitional;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const X: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const KEEP: WriteOptions = WriteOptions::new(Layout::Compact, false);

fn element(text: &str) -> Element {
    parse(text.as_bytes()).unwrap().root
}

/// The refusal writing the node `text` holds in Transitional gives; `None`
/// when the write succeeds.
fn refusal<T>(text: &str) -> Option<&'static str>
where
    T: Transitional,
{
    if let Err(WriteError::NotInClass { class, tag }) = T::from_transitional(&element(text))
        .unwrap()
        .to_transitional()
    {
        (class == ConformanceClass::Transitional).then_some(tag)
    } else {
        None
    }
}

/// The node `text` holds, written in Transitional and serialized compact.
fn transitional<T>(text: &str) -> String
where
    T: Transitional,
{
    let root = T::from_transitional(&element(text))
        .unwrap()
        .to_transitional()
        .unwrap();
    let mut part = Part::new("", root);
    part.declaration = None;
    part.prolog.clear();
    String::from_utf8(part.to_xml(KEEP)).unwrap()
}

#[test]
fn a_strict_and_a_transitional_paragraph_read_into_equal_structs() {
    let body = "<w:p><w:pPr><w:jc w:val=\"left\"/></w:pPr><w:r><w:t>Hello</w:t></w:r></w:p>";
    let first = |uri: &str| {
        element(&format!("<w:body xmlns:w=\"{uri}\">{body}</w:body>"))
            .child_elements()
            .next()
            .unwrap()
            .clone()
    };
    assert_eq!(
        Paragraph::from_transitional(&first(W)).unwrap(),
        Paragraph::from_transitional(&first(W_STRICT)).unwrap()
    );
}

#[test]
fn a_transitional_paragraph_writes_back_unchanged() {
    let text = format!(
        "<w:p xmlns:w=\"{W}\"><w:pPr><w:jc w:val=\"left\"/></w:pPr><w:r><w:t>Hello</w:t></w:r></w:p>"
    );
    assert_eq!(transitional::<Paragraph>(&text), text);
}

#[test]
fn a_strict_document_writes_the_transitional_uri_and_no_strict_conformance() {
    assert_eq!(
        transitional::<Document>(&format!(
            "<w:document xmlns:w=\"{W_STRICT}\" w:conformance=\"strict\"><w:body><w:p><w:pPr><w:jc w:val=\"start\"/></w:pPr></w:p></w:body></w:document>"
        )),
        format!(
            "<w:document xmlns:w=\"{W}\"><w:body><w:p><w:pPr><w:jc w:val=\"start\"/></w:pPr></w:p></w:body></w:document>"
        )
    );
}

#[test]
fn reading_another_tag_is_an_error() {
    assert!(matches!(
        Paragraph::from_transitional(&element(&format!("<w:r xmlns:w=\"{W}\"/>"))),
        Err(ReadError::UnexpectedTag { expected, found }) if expected == "w:p" && found == "w:r"
    ));
}

#[test]
fn a_date_cell_has_no_transitional_form() {
    let cell =
        format!("<x:c xmlns:x=\"{X}\" r=\"A1\" t=\"d\"><x:v>2026-10-14T00:00:00</x:v></x:c>");
    assert_eq!(refusal::<Cell>(&cell), Some("c/@t"));
    assert_eq!(
        refusal::<Worksheet>(&format!(
            "<x:worksheet xmlns:x=\"{X}\"><x:sheetData><x:row r=\"1\"><x:c r=\"A1\" t=\"d\"><x:v>2026-10-14</x:v></x:c></x:row></x:sheetData></x:worksheet>"
        )),
        Some("c/@t")
    );
    assert_eq!(
        refusal::<Cell>(&format!(
            "<x:c xmlns:x=\"{X}\" r=\"A1\" t=\"n\"><x:v>46309</x:v></x:c>"
        )),
        None
    );
}
