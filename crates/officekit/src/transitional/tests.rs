use officekit_markup::common_simple_types::{
    DocumentConformanceClassValue, DocumentConformanceClassValueMember,
};
use officekit_markup::spreadsheet::{Cell, CellType, CellValue, Row, Worksheet};
use officekit_markup::wordprocessing::{
    BlockContent, Document, DocumentBody, HorizontalAlignmentType, Paragraph, ParagraphAlignment,
    ParagraphContent, ParagraphProperties, Text, TextRun, TextRunContent,
};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Document as Part, Element};
use officekit_xml::error::{ReadError, WriteError};
use officekit_xml::parse::parse;
use officekit_xml::write::{Layout, WriteOptions};

use super::{Transitional, spell};

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

/// The node `text` holds, written in Transitional and serialized compact.
fn transitional<T>(text: &str) -> String
where
    T: Transitional,
{
    written(&T::from_transitional(&element(text)).unwrap())
}

/// The refusal writing `node` in Transitional gives; `None` when the write succeeds.
fn refused<T>(node: &T) -> Option<&'static str>
where
    T: Transitional,
{
    if let Err(WriteError::NotInClass { class, tag }) = node.to_transitional() {
        (class == ConformanceClass::Transitional).then_some(tag)
    } else {
        None
    }
}

/// `node`, written in Transitional and serialized compact.
fn written<T>(node: &T) -> String
where
    T: Transitional,
{
    let mut part = Part::new("", node.to_transitional().unwrap());
    part.declaration = None;
    part.prolog.clear();
    String::from_utf8(part.to_xml(KEEP)).unwrap()
}

/// A paragraph justified `alignment`, holding the run of `value` when given.
fn justified(alignment: &str, value: Option<&str>) -> Paragraph {
    let mut justification = ParagraphAlignment::default();
    justification.val = Some(HorizontalAlignmentType::from(alignment.to_owned()));
    let mut properties = ParagraphProperties::default();
    properties.paragraph_alignment = Some(Box::new(justification));
    let mut paragraph = Paragraph::default();
    paragraph.properties = Some(Box::new(properties));
    if let Some(text) = value {
        let mut characters = Text::default();
        characters.value = text.to_owned();
        let mut run = TextRun::default();
        run.content = vec![TextRunContent::Text(characters)];
        paragraph.content = vec![ParagraphContent::TextRun(run)];
    }
    paragraph
}

/// A cell `A1` of the type `kind` holding `value`.
fn typed_cell(kind: &str, value: &str) -> Cell {
    let mut held = CellValue::default();
    held.value = value.to_owned();
    let mut cell = Cell::default();
    cell.r = Some("A1".to_owned());
    cell.t = Some(CellType::from(kind.to_owned()));
    cell.value = Some(Box::new(held));
    cell
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
    assert_eq!(
        written(&justified("left", Some("Hello"))),
        format!(
            "<w:p xmlns:w=\"{W}\"><w:pPr><w:jc w:val=\"left\"/></w:pPr><w:r><w:t>Hello</w:t></w:r></w:p>"
        )
    );
}

#[test]
fn a_strict_document_writes_the_transitional_uri_and_no_strict_conformance() {
    let mut body = DocumentBody::default();
    body.content = vec![BlockContent::Paragraph(justified("start", None))];
    let mut document = Document::default();
    document.body = Some(Box::new(body));
    document.conformance = Some(DocumentConformanceClassValue::from(
        DocumentConformanceClassValueMember::Strict,
    ));
    assert_eq!(
        written(&document),
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
    assert_eq!(
        refused(&typed_cell("d", "2026-10-14T00:00:00")),
        Some("c/@t")
    );
    let mut row = Row::default();
    row.cell = vec![typed_cell("d", "2026-10-14")];
    row.r = Some("1".to_owned());
    let mut sheet = Worksheet::default();
    sheet.sheet_data.row = vec![row];
    assert_eq!(refused(&sheet), Some("c/@t"));
    assert_eq!(refused(&typed_cell("n", "46309")), None);
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

/// `text` after a save from Strict to Transitional's walk, serialized compact, with whether the
/// walk changed it.
fn spelled(text: &str) -> (String, bool) {
    let mut root = element(text);
    let changed = spell(&mut root, None);
    let mut part = Part::new("", root);
    part.declaration = None;
    part.prolog.clear();
    (String::from_utf8(part.to_xml(KEEP)).unwrap(), changed)
}

#[test]
fn the_walk_back_writes_each_kind_of_row_in_transitional_s_spelling() {
    let (written, changed) = spelled(&format!(
        "<w:body xmlns:w=\"{W}\"><w:tbl><w:tblPr><w:tblW w:w=\"234pt\" w:type=\"dxa\"/><w:jc w:val=\"end\"/><w:tblBorders><w:start w:val=\"single\"/><w:end w:val=\"single\"/></w:tblBorders><w:tblLook w:firstRow=\"1\" w:lastRow=\"0\" w:firstColumn=\"true\" w:lastColumn=\"0\" w:noHBand=\"0\" w:noVBand=\"1\"/></w:tblPr></w:tbl><w:p><w:pPr><w:tabs><w:tab w:val=\"start\" w:pos=\"72pt\"/></w:tabs><w:ind w:start=\"36pt\" w:hanging=\"0.5in\"/><w:jc w:val=\"start\"/></w:pPr><w:r><w:rPr><w:w w:val=\"89%\"/></w:rPr></w:r></w:p><w:sectPr><w:pgSz w:w=\"612pt\" w:h=\"792pt\"/></w:sectPr></w:body>"
    ));
    assert!(changed);
    for expected in [
        "<w:tblW w:w=\"4680\" w:type=\"dxa\"/>",
        "<w:jc w:val=\"right\"/>",
        "<w:tblBorders><w:left w:val=\"single\"/><w:right w:val=\"single\"/></w:tblBorders>",
        "w:noVBand=\"1\" w:val=\"04A0\"/>",
        "<w:tab w:val=\"left\" w:pos=\"1440\"/>",
        "<w:ind w:left=\"720\" w:hanging=\"720\"/>",
        "<w:jc w:val=\"left\"/>",
        "<w:w w:val=\"89\"/>",
        "<w:pgSz w:w=\"12240\" w:h=\"15840\"/>",
    ] {
        assert!(written.contains(expected), "{expected} in {written}");
    }
}

#[test]
fn a_measure_rounds_half_away_from_zero_and_a_value_it_cannot_read_stays() {
    let (written, _) = spelled(&format!(
        "<w:tblPr xmlns:w=\"{W}\"><w:tblW w:w=\"233.33pt\" w:type=\"dxa\"/><w:tblInd w:w=\"96.08%\" w:type=\"pct\"/><w:tblCellSpacing w:w=\"abc\" w:type=\"dxa\"/></w:tblPr>"
    ));
    assert!(
        written.contains("<w:tblW w:w=\"4667\" w:type=\"dxa\"/>"),
        "{written}"
    );
    assert!(
        written.contains("<w:tblInd w:w=\"4804\" w:type=\"pct\"/>"),
        "{written}"
    );
    assert!(
        written.contains("<w:tblCellSpacing w:w=\"abc\" w:type=\"dxa\"/>"),
        "{written}"
    );
    let (negative, _) = spelled(&format!(
        "<w:ind xmlns:w=\"{W}\" w:start=\"-0.025pt\" w:end=\"2.54cm\"/>"
    ));
    assert_eq!(
        negative,
        format!("<w:ind xmlns:w=\"{W}\" w:left=\"-1\" w:right=\"1440\"/>")
    );
}

#[test]
fn the_walk_back_moves_codes_dates_and_children_back_into_their_attributes() {
    let (font, _) = spelled(&format!(
        "<w:fonts xmlns:w=\"{W}\"><w:font w:name=\"A\"><w:charset w:characterSet=\"iso-8859-1\"/></w:font><w:font w:name=\"B\"><w:charset w:characterSet=\"x-unknown\"/></w:font></w:fonts>"
    ));
    assert!(font.contains("<w:charset w:val=\"00\"/>"), "{font}");
    assert!(
        font.contains("<w:charset w:characterSet=\"x-unknown\"/>"),
        "{font}"
    );
    let (workbook, _) = spelled(&format!(
        "<x:root xmlns:x=\"{X}\"><x:textPr characterSet=\"iso-2022-jp\"/><x:dynamicFilter type=\"today\" maxValIso=\"2024-01-01T12:00:00\"/><x:protectedRange name=\"p\" sqref=\"A1\"><x:securityDescriptor>O:WD</x:securityDescriptor></x:protectedRange><x:dxf><x:border><x:start style=\"thin\"/><x:end/></x:border></x:dxf></x:root>"
    ));
    for expected in [
        "<x:textPr codePage=\"50220\"/>",
        "<x:dynamicFilter type=\"today\" maxVal=\"45292.5\"/>",
        "<x:protectedRange name=\"p\" sqref=\"A1\" securityDescriptor=\"O:WD\"/>",
        "<x:border><x:left style=\"thin\"/><x:right/></x:border>",
    ] {
        assert!(workbook.contains(expected), "{expected} in {workbook}");
    }
}

#[test]
fn the_walk_back_scopes_a_rule_to_the_parent_that_has_it() {
    let (written, _) = spelled(&format!(
        "<x:root xmlns:x=\"{X}\"><x:calcChain><x:c r=\"A1\" i=\"1\"/></x:calcChain><x:row><x:c r=\"A1\"/></x:row></x:root>"
    ));
    assert_eq!(
        written,
        format!(
            "<x:root xmlns:x=\"{X}\"><x:calcChain><x:c ref=\"A1\" i=\"1\"/></x:calcChain><x:row><x:c r=\"A1\"/></x:row></x:root>"
        )
    );
}

#[test]
fn the_walk_back_converts_percentages_in_text_and_attributes_and_keeps_a_chart_s_numbers() {
    let wp14 = "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing";
    let chart = "http://schemas.openxmlformats.org/drawingml/2006/chart";
    let (written, _) = spelled(&format!(
        "<a:root xmlns:a=\"{A}\" xmlns:wp14=\"{wp14}\" xmlns:c=\"{chart}\"><a:alpha val=\"93.999%\"/><a:gs pos=\"50%\"/><wp14:pctWidth>40%</wp14:pctWidth><c:gapWidth val=\"219\"/><c:overlap val=\"-27\"/></a:root>"
    ));
    for expected in [
        "<a:alpha val=\"93999\"/>",
        "<a:gs pos=\"50000\"/>",
        "<wp14:pctWidth>40000</wp14:pctWidth>",
        "<c:gapWidth val=\"219\"/>",
        "<c:overlap val=\"-27\"/>",
    ] {
        assert!(written.contains(expected), "{expected} in {written}");
    }
}

#[test]
fn the_walk_back_leaves_transitional_s_own_spelling_unchanged() {
    let text = format!(
        "<w:p xmlns:w=\"{W}\"><w:pPr><w:ind w:left=\"720\"/><w:jc w:val=\"left\"/></w:pPr></w:p>"
    );
    assert_eq!(spelled(&text), (text, false));
}
