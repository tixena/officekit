use officekit_markup::spreadsheet::{Cell, Worksheet};
use officekit_markup::wordprocessing::{Document, Paragraph, TextRun};
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
const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const WP_STRICT: &str = "http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const A_STRICT: &str = "http://purl.oclc.org/ooxml/drawingml/main";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const WPG: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup";
const WPC: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas";
const WNE: &str = "http://schemas.microsoft.com/office/word/2006/wordml";

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

/// A run holding `payload` in a Strict drawing, in the form Word's own Strict file has.
fn strict_run(uri: &str, payload: &str) -> String {
    format!(
        "<w:r xmlns:w=\"{W_STRICT}\" xmlns:wp=\"{WP_STRICT}\" xmlns:a=\"{A_STRICT}\"><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{uri}\">{payload}</a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    )
}

/// `drawing` written in a Transitional run, in the Choice `required` names.
fn transitional_run(required: &str, uri: &str, declarations: &str, payload: &str) -> String {
    format!(
        "<w:r xmlns:w=\"{W}\" xmlns:wp=\"{WP}\" xmlns:a=\"{A}\"><mc:AlternateContent xmlns:mc=\"{MC}\"><mc:Choice xmlns:{required}=\"{uri}\" Requires=\"{required}\"><w:drawing><wp:inline><a:graphic><a:graphicData{declarations} uri=\"{uri}\">{payload}</a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice></mc:AlternateContent></w:r>"
    )
}

#[test]
fn word_s_strict_shape_takes_word_s_transitional_names_in_a_choice() {
    let payload = format!(
        "<wp:wsp normalEastAsianFlow=\"true\"><wp:cNvSpPr txBox=\"1\"/><wp:spPr/><wp:style/><wp:txbx id=\"3\"><wne:txbxContent xmlns:wne=\"{WNE}\"><w:p><w:r><w:t>Box</w:t></w:r></w:p></wne:txbxContent></wp:txbx><wp:bodyPr/></wp:wsp>"
    );
    assert_eq!(
        transitional::<TextRun>(&strict_run(WPS, &payload)),
        transitional_run(
            "wps",
            WPS,
            &format!(" xmlns:wps=\"{WPS}\""),
            &format!(
                "<wps:wsp normalEastAsianFlow=\"true\"><wps:cNvSpPr txBox=\"1\"/><wps:spPr/><wps:style/><wps:txbx id=\"3\"><w:txbxContent xmlns:w=\"{W}\"><w:p><w:r><w:t>Box</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp>"
            )
        )
    );
}

#[test]
fn a_wps_shape_holding_wne_text_takes_w_text_in_a_choice() {
    let payload = format!(
        "<wps:wsp xmlns:wps=\"{WPS}\"><wps:txbx><wne:txbxContent xmlns:wne=\"{WNE}\"><w:p/></wne:txbxContent></wps:txbx></wps:wsp>"
    );
    assert_eq!(
        transitional::<TextRun>(&strict_run(WPS, &payload)),
        transitional_run(
            "wps",
            WPS,
            "",
            &format!(
                "<wps:wsp xmlns:wps=\"{WPS}\"><wps:txbx><w:txbxContent xmlns:w=\"{W}\"><w:p/></w:txbxContent></wps:txbx></wps:wsp>"
            )
        )
    );
}

#[test]
fn a_group_takes_wpg_and_its_shapes_wps() {
    let payload = "<wp:wgp><wp:cNvGrpSpPr/><wp:grpSpPr/><wp:wsp><wp:cNvPr id=\"1\" name=\"s\"/><wp:spPr/><wp:bodyPr/></wp:wsp><wp:grpSp><wp:cNvGrpSpPr/><wp:grpSpPr/></wp:grpSp></wp:wgp>";
    assert_eq!(
        transitional::<TextRun>(&strict_run(WPG, payload)),
        transitional_run(
            "wpg",
            WPG,
            &format!(" xmlns:wpg=\"{WPG}\" xmlns:wps=\"{WPS}\""),
            "<wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr/><wps:wsp><wps:cNvPr id=\"1\" name=\"s\"/><wps:spPr/><wps:bodyPr/></wps:wsp><wpg:grpSp><wpg:cNvGrpSpPr/><wpg:grpSpPr/></wpg:grpSp></wpg:wgp>"
        )
    );
}

#[test]
fn a_canvas_takes_wpc_and_its_frame_s_children_wpg() {
    let payload = "<wp:wpc><wp:bg/><wp:whole/><wp:wsp><wp:spPr/><wp:bodyPr/></wp:wsp><wp:wgp><wp:grpSpPr/></wp:wgp><wp:graphicFrame><wp:cNvPr id=\"1\" name=\"f\"/></wp:graphicFrame></wp:wpc>";
    assert_eq!(
        transitional::<TextRun>(&strict_run(WPC, payload)),
        transitional_run(
            "wpc",
            WPC,
            &format!(" xmlns:wpc=\"{WPC}\" xmlns:wps=\"{WPS}\" xmlns:wpg=\"{WPG}\""),
            "<wpc:wpc><wpc:bg/><wpc:whole/><wps:wsp><wps:spPr/><wps:bodyPr/></wps:wsp><wpg:wgp><wpg:grpSpPr/></wpg:wgp><wpc:graphicFrame><wpg:cNvPr id=\"1\" name=\"f\"/></wpc:graphicFrame></wpc:wpc>"
        )
    );
}

#[test]
fn a_shape_in_a_fallback_stays_in_its_branch() {
    let run = format!(
        "<w:r xmlns:w=\"{W_STRICT}\" xmlns:wp=\"{WP_STRICT}\" xmlns:a=\"{A_STRICT}\" xmlns:mc=\"{MC}\"><mc:AlternateContent><mc:Choice Requires=\"v\"><w:t/></mc:Choice><mc:Fallback><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{WPS}\"><wp:wsp><wp:spPr/><wp:bodyPr/></wp:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Fallback></mc:AlternateContent></w:r>"
    );
    assert_eq!(
        transitional::<TextRun>(&run),
        format!(
            "<w:r xmlns:w=\"{W}\" xmlns:wp=\"{WP}\" xmlns:a=\"{A}\" xmlns:mc=\"{MC}\"><mc:AlternateContent><mc:Choice Requires=\"v\"><w:t/></mc:Choice><mc:Fallback><w:drawing><wp:inline><a:graphic><a:graphicData xmlns:wps=\"{WPS}\" uri=\"{WPS}\"><wps:wsp><wps:spPr/><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Fallback></mc:AlternateContent></w:r>"
        )
    );
}

#[test]
fn a_drawing_inside_a_text_box_keeps_its_wp_anchor() {
    let picture = "http://schemas.openxmlformats.org/drawingml/2006/picture";
    let payload = format!(
        "<wp:wsp><wp:txbx><wne:txbxContent xmlns:wne=\"{WNE}\"><w:p><w:r><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{picture}\"/></a:graphic></wp:inline></w:drawing></w:r></w:p></wne:txbxContent></wp:txbx></wp:wsp>"
    );
    let written = transitional::<TextRun>(&strict_run(WPS, &payload));
    assert!(
        written.contains(&format!(
            "<w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{picture}\"/></a:graphic></wp:inline></w:drawing>"
        )),
        "{written}"
    );
    assert_eq!(
        written.matches("mc:AlternateContent ").count(),
        1,
        "{written}"
    );
}

#[test]
fn a_shape_already_in_word_s_transitional_form_stays_as_it_is() {
    let bare = format!(
        "<w:r xmlns:w=\"{W}\" xmlns:wp=\"{WP}\" xmlns:a=\"{A}\" xmlns:wps=\"{WPS}\"><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{WPS}\"><wps:wsp><wps:txbx><w:txbxContent><w:p/></w:txbxContent></wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    );
    assert_eq!(transitional::<TextRun>(&bare), bare);
    let block = format!(
        "<w:r xmlns:w=\"{W}\" xmlns:wp=\"{WP}\" xmlns:a=\"{A}\" xmlns:wps=\"{WPS}\" xmlns:mc=\"{MC}\"><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{WPS}\"><wps:wsp><wps:txbx><w:txbxContent><w:p/></w:txbxContent></wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice><mc:Fallback><w:pict/></mc:Fallback></mc:AlternateContent></w:r>"
    );
    assert_eq!(transitional::<TextRun>(&block), block);
}

#[test]
fn an_ecma_wsp_under_another_address_stays_as_it_is() {
    let uri = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
    let run = format!(
        "<w:r xmlns:w=\"{W}\" xmlns:wp=\"{WP}\" xmlns:a=\"{A}\"><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{uri}\"><wp:wsp><wp:bodyPr/></wp:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    );
    assert_eq!(transitional::<TextRun>(&run), run);
}
