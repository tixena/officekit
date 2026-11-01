use officekit_markup::chart::{GapWidth, LabelOffset, Overlap};
use officekit_markup::drawing::Alpha;
use officekit_markup::office_drawing_2012_chart_style::DefRPr;
use officekit_markup::office_drawing_2017_03_chart::DispNaAsBlank;
use officekit_markup::office_word_2010_wordml::Shadow;
use officekit_markup::office_word_2010_wordprocessing_drawing::{
    PctHeight, PctPosHOffset, PctPosVOffset, PctWidth,
};
use officekit_markup::presentation::Presentation;
use officekit_markup::spreadsheet::{
    Border, CalculationChainInfo, Cell, DynamicFilter, PivotArea, ProtectedRange,
    TextImportSettings, Workbook,
};
use officekit_markup::wordprocessing::{
    CharacterSetSupportedByFont, CheckboxFormFieldProperties, CompatibilitySettings, Document,
    DocumentBackground, DocumentEditingRestrictions, DropDownListFormFieldProperties,
    FramesetSplitterProperties, Paragraph, ParagraphConditionalFormatting, ParagraphIndentation,
    PreferredTableWidth, RunProperties, SuggestedSorting, TableCellBorders,
    TableStyleConditionalFormattingSettingsException, TextRun, WriteProtection,
};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Document as Part, Element};
use officekit_xml::error::{ReadError, WriteError};
use officekit_xml::parse::parse;
use officekit_xml::write::{Layout, WriteOptions};

use super::{Refusal, Strict, iso_date_time, refusals};

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const C16R3: &str = "http://schemas.microsoft.com/office/drawing/2017/03/chart";
const C_STRICT: &str = "http://purl.oclc.org/ooxml/drawingml/chart";
const A_STRICT: &str = "http://purl.oclc.org/ooxml/drawingml/main";
const CS: &str = "http://schemas.microsoft.com/office/drawing/2012/chartStyle";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const P_STRICT: &str = "http://purl.oclc.org/ooxml/presentationml/main";
const V: &str = "urn:schemas-microsoft-com:vml";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const WP_STRICT: &str = "http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing";
const WP14: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing";
const WPC: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas";
const WPG: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup";
const WNE: &str = "http://schemas.microsoft.com/office/word/2006/wordml";
const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const X: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const X_STRICT: &str = "http://purl.oclc.org/ooxml/spreadsheetml/main";
/// A run's root declarations for the text box tests, Transitional URIs.
const RUN_DECLARATIONS: &str = "xmlns:w=\"{w}\" xmlns:mc=\"{mc}\" xmlns:wp=\"{wp}\" xmlns:wps=\"{wps}\" xmlns:a=\"{a}\" xmlns:v=\"{v}\"";
const KEEP: WriteOptions = WriteOptions::new(Layout::Compact, false);

/// The root element of `text`, with `{w}`, `{a}`, `{x}`, `{p}`, `{mc}`, `{v}`,
/// `{wp}`, `{wpg}`, and `{wps}` standing for the Transitional URIs.
fn element(text: &str) -> Element {
    let expanded = text
        .replace("{wp}", WP)
        .replace("{wpc}", WPC)
        .replace("{wpg}", WPG)
        .replace("{wps}", WPS)
        .replace("{w}", W)
        .replace("{a}", A)
        .replace("{x}", X)
        .replace("{p}", P)
        .replace("{mc}", MC)
        .replace("{v}", V);
    parse(expanded.as_bytes()).unwrap().root
}

/// The refusal writing the node `text` holds in Strict gives; `None` when the
/// write succeeds.
fn refusal<T>(text: &str) -> Option<&'static str>
where
    T: Strict,
{
    if let Err(WriteError::NotInClass { class, tag }) =
        T::from_strict(&element(text)).unwrap().to_strict()
    {
        (class == ConformanceClass::Strict).then_some(tag)
    } else {
        None
    }
}

/// The node `text` holds, written in Strict and serialized compact.
fn strict<T>(text: &str) -> String
where
    T: Strict,
{
    written(T::from_strict(&element(text)).unwrap().to_strict().unwrap())
}

fn written(root: Element) -> String {
    let mut part = Part::new("", root);
    part.declaration = None;
    part.prolog.clear();
    String::from_utf8(part.to_xml(KEEP)).unwrap()
}

#[test]
fn a_strict_and_a_transitional_paragraph_read_into_equal_structs() {
    let body = "<w:p><w:pPr><w:jc w:val=\"start\"/></w:pPr><w:r><w:t>Hello</w:t></w:r></w:p>";
    let transitional = element(&format!("<w:body xmlns:w=\"{W}\">{body}</w:body>"));
    let strict_body = element(&format!("<w:body xmlns:w=\"{W_STRICT}\">{body}</w:body>"));
    let first = |found: &Element| found.child_elements().next().unwrap().clone();
    assert_eq!(
        Paragraph::from_strict(&first(&transitional)).unwrap(),
        Paragraph::from_strict(&first(&strict_body)).unwrap()
    );
}

#[test]
fn a_transitional_paragraph_writes_as_strict() {
    assert_eq!(
        strict::<Paragraph>(
            "<w:p xmlns:w=\"{w}\"><w:pPr><w:jc w:val=\"left\"/></w:pPr><w:r><w:t>Hello</w:t></w:r></w:p>"
        ),
        format!(
            "<w:p xmlns:w=\"{W_STRICT}\"><w:pPr><w:jc w:val=\"start\"/></w:pPr><w:r><w:t>Hello</w:t></w:r></w:p>"
        )
    );
}

#[test]
fn the_roots_of_the_main_parts_declare_strict_conformance() {
    assert_eq!(
        strict::<Document>("<w:document xmlns:w=\"{w}\"><w:body/></w:document>"),
        format!(
            "<w:document xmlns:w=\"{W_STRICT}\" w:conformance=\"strict\"><w:body/></w:document>"
        )
    );
    assert_eq!(
        strict::<Document>(
            "<w:document xmlns:w=\"{w}\" w:conformance=\"transitional\"><w:body/></w:document>"
        ),
        format!(
            "<w:document xmlns:w=\"{W_STRICT}\" w:conformance=\"strict\"><w:body/></w:document>"
        )
    );
    let workbook = strict::<Workbook>("<x:workbook xmlns:x=\"{x}\"><x:sheets/></x:workbook>");
    assert!(
        workbook.contains(" conformance=\"strict\""),
        "{workbook} declares no conformance"
    );
    assert_eq!(
        strict::<Presentation>("<p:presentation xmlns:p=\"{p}\"/>"),
        format!("<p:presentation xmlns:p=\"{P_STRICT}\" conformance=\"strict\"/>")
    );
}

#[test]
fn reading_another_tag_is_an_error() {
    assert!(matches!(
        Paragraph::from_strict(&element("<w:r xmlns:w=\"{w}\"/>")),
        Err(ReadError::UnexpectedTag { expected, found }) if expected == "w:p" && found == "w:r"
    ));
}

#[test]
fn a_vml_picture_has_no_strict_form() {
    assert_eq!(
        refusal::<TextRun>(
            "<w:r xmlns:w=\"{w}\"><w:pict><v:shape xmlns:v=\"{v}\"/></w:pict></w:r>"
        ),
        Some("pict")
    );
}

#[test]
fn content_the_typed_tree_keeps_unknown_is_converted_and_checked() {
    let choice = "<mc:Choice Requires=\"w14\"><w:pPr><w:jc w:val=\"right\"/></w:pPr></mc:Choice>";
    assert_eq!(
        strict::<Paragraph>(&format!(
            "<w:p xmlns:w=\"{{w}}\" xmlns:mc=\"{{mc}}\"><mc:AlternateContent>{choice}</mc:AlternateContent></w:p>"
        )),
        format!(
            "<w:p xmlns:w=\"{W_STRICT}\" xmlns:mc=\"{MC}\"><mc:AlternateContent><mc:Choice Requires=\"w14\"><w:pPr><w:jc w:val=\"end\"/></w:pPr></mc:Choice></mc:AlternateContent></w:p>"
        )
    );
    assert_eq!(
        refusal::<TextRun>(
            "<w:r xmlns:w=\"{w}\" xmlns:mc=\"{mc}\"><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing/></mc:Choice><mc:Fallback><w:pict/></mc:Fallback></mc:AlternateContent></w:r>"
        ),
        Some("pict")
    );
    assert_eq!(
        refusal::<TextRun>(
            "<w:r xmlns:w=\"{w}\" xmlns:mc=\"{mc}\" xmlns:v=\"{v}\"><mc:AlternateContent><mc:Fallback><v:shape/></mc:Fallback></mc:AlternateContent></w:r>"
        ),
        Some("shape")
    );
}

#[test]
fn a_packed_bitmask_unpacks_into_its_attributes() {
    assert_eq!(
        strict::<TableStyleConditionalFormattingSettingsException>(
            "<w:tblLook xmlns:w=\"{w}\" w:val=\"04A0\"/>"
        ),
        format!(
            "<w:tblLook xmlns:w=\"{W_STRICT}\" w:firstRow=\"1\" w:lastRow=\"0\" w:firstColumn=\"1\" w:lastColumn=\"0\" w:noHBand=\"0\" w:noVBand=\"1\"/>"
        )
    );
    assert_eq!(
        strict::<TableStyleConditionalFormattingSettingsException>(
            "<w:tblLook xmlns:w=\"{w}\" w:val=\"04A0\" w:firstRow=\"0\"/>"
        ),
        format!(
            "<w:tblLook xmlns:w=\"{W_STRICT}\" w:firstRow=\"0\" w:lastRow=\"0\" w:firstColumn=\"1\" w:lastColumn=\"0\" w:noHBand=\"0\" w:noVBand=\"1\"/>"
        )
    );
    let unpacked = strict::<ParagraphConditionalFormatting>(
        "<w:cnfStyle xmlns:w=\"{w}\" w:val=\"101000000100\"/>",
    );
    for expected in [
        "w:firstRow=\"1\"",
        "w:lastRow=\"0\"",
        "w:firstColumn=\"1\"",
        "w:firstRowFirstColumn=\"1\"",
        "w:lastRowFirstColumn=\"0\"",
    ] {
        assert!(unpacked.contains(expected), "{unpacked} lacks {expected}");
    }
    assert!(!unpacked.contains("w:val="), "{unpacked} keeps w:val");
    assert_eq!(
        refusal::<ParagraphConditionalFormatting>("<w:cnfStyle xmlns:w=\"{w}\" w:val=\"1010\"/>"),
        Some("cnfStyle/@val")
    );
}

#[test]
fn a_bare_percentage_takes_its_unit() {
    assert_eq!(
        strict::<Alpha>("<a:alpha xmlns:a=\"{a}\" val=\"50000\"/>"),
        format!("<a:alpha xmlns:a=\"{A_STRICT}\" val=\"50%\"/>")
    );
    assert_eq!(
        strict::<Alpha>("<a:alpha xmlns:a=\"{a}\" val=\"12345\"/>"),
        format!("<a:alpha xmlns:a=\"{A_STRICT}\" val=\"12.345%\"/>")
    );
    assert_eq!(
        strict::<Alpha>("<a:alpha xmlns:a=\"{a}\" val=\"25%\"/>"),
        format!("<a:alpha xmlns:a=\"{A_STRICT}\" val=\"25%\"/>")
    );
}

#[test]
fn a_chart_s_percentages_stay_the_bare_numbers_word_writes() {
    assert_eq!(
        strict::<GapWidth>(&format!("<c:gapWidth xmlns:c=\"{C}\" val=\"219\"/>")),
        format!("<c:gapWidth xmlns:c=\"{C_STRICT}\" val=\"219\"/>")
    );
    assert_eq!(
        strict::<Overlap>(&format!("<c:overlap xmlns:c=\"{C}\" val=\"-27\"/>")),
        format!("<c:overlap xmlns:c=\"{C_STRICT}\" val=\"-27\"/>")
    );
    assert_eq!(
        strict::<LabelOffset>(&format!("<c:lblOffset xmlns:c=\"{C}\" val=\"100\"/>")),
        format!("<c:lblOffset xmlns:c=\"{C_STRICT}\" val=\"100\"/>")
    );
    assert_eq!(
        strict::<GapWidth>(&format!("<c:gapWidth xmlns:c=\"{C}\" val=\"219%\"/>")),
        format!("<c:gapWidth xmlns:c=\"{C_STRICT}\" val=\"219%\"/>")
    );
}

#[test]
fn a_table_width_counts_fiftieths_of_a_percent_or_twentieths_of_a_point() {
    assert_eq!(
        strict::<PreferredTableWidth>("<w:tblW xmlns:w=\"{w}\" w:w=\"2475\" w:type=\"pct\"/>"),
        format!("<w:tblW xmlns:w=\"{W_STRICT}\" w:w=\"49.5%\" w:type=\"pct\"/>")
    );
    assert_eq!(
        strict::<PreferredTableWidth>("<w:tblW xmlns:w=\"{w}\" w:w=\"2880\" w:type=\"dxa\"/>"),
        format!("<w:tblW xmlns:w=\"{W_STRICT}\" w:w=\"144pt\" w:type=\"dxa\"/>")
    );
}

#[test]
fn on_and_off_are_true_and_false() {
    assert_eq!(
        strict::<CompatibilitySettings>(
            "<w:compat xmlns:w=\"{w}\"><w:spaceForUL w:val=\"on\"/></w:compat>"
        ),
        format!("<w:compat xmlns:w=\"{W_STRICT}\"><w:spaceForUL w:val=\"true\"/></w:compat>")
    );
}

#[test]
fn a_tag_whose_types_differ_converts_by_its_parent() {
    let check_box = strict::<CheckboxFormFieldProperties>(
        "<w:checkBox xmlns:w=\"{w}\"><w:default w:val=\"on\"/></w:checkBox>",
    );
    assert!(
        check_box.contains("<w:default w:val=\"true\"/>"),
        "{check_box}"
    );
    let list = strict::<DropDownListFormFieldProperties>(
        "<w:ddList xmlns:w=\"{w}\"><w:default w:val=\"2\"/></w:ddList>",
    );
    assert!(list.contains("<w:default w:val=\"2\"/>"), "{list}");
    let run = strict::<RunProperties>("<w:rPr xmlns:w=\"{w}\"><w:w w:val=\"300\"/></w:rPr>");
    assert!(run.contains("<w:w w:val=\"300%\"/>"), "{run}");
    let frames = strict::<FramesetSplitterProperties>(
        "<w:framesetSplitbar xmlns:w=\"{w}\"><w:w w:val=\"300\"/></w:framesetSplitbar>",
    );
    assert!(frames.contains("<w:w w:val=\"300\"/>"), "{frames}");
}

#[test]
fn renamed_children_and_attributes_take_their_strict_names() {
    assert_eq!(
        strict::<TableCellBorders>(
            "<w:tcBorders xmlns:w=\"{w}\"><w:left w:val=\"single\"/><w:right w:val=\"nil\"/></w:tcBorders>"
        ),
        format!(
            "<w:tcBorders xmlns:w=\"{W_STRICT}\"><w:start w:val=\"single\"/><w:end w:val=\"nil\"/></w:tcBorders>"
        )
    );
    assert_eq!(
        strict::<TableCellBorders>(
            "<w:tcBorders xmlns:w=\"{w}\"><w:start w:val=\"double\"/><w:left w:val=\"single\"/></w:tcBorders>"
        ),
        format!("<w:tcBorders xmlns:w=\"{W_STRICT}\"><w:start w:val=\"double\"/></w:tcBorders>")
    );
    assert_eq!(
        strict::<Border>("<x:border xmlns:x=\"{x}\"><x:end/><x:left/></x:border>"),
        format!("<x:border xmlns:x=\"{X_STRICT}\"><x:start/><x:end/></x:border>")
    );
    assert_eq!(
        strict::<ParagraphIndentation>("<w:ind xmlns:w=\"{w}\" w:left=\"720\" w:hanging=\"360\"/>"),
        format!("<w:ind xmlns:w=\"{W_STRICT}\" w:start=\"720\" w:hanging=\"360\"/>")
    );
    assert_eq!(
        strict::<ParagraphIndentation>("<w:ind xmlns:w=\"{w}\" w:start=\"10\" w:left=\"720\"/>"),
        format!("<w:ind xmlns:w=\"{W_STRICT}\" w:start=\"10\"/>")
    );
    let chain = strict::<CalculationChainInfo>(
        "<x:calcChain xmlns:x=\"{x}\"><x:c ref=\"A1\" i=\"1\"/></x:calcChain>",
    );
    assert!(chain.contains("<x:c r=\"A1\" i=\"1\"/>"), "{chain}");
    assert_eq!(
        strict::<ProtectedRange>(
            "<x:protectedRange xmlns:x=\"{x}\" sqref=\"A1\" name=\"r\" securityDescriptor=\"O:WD\"/>"
        ),
        format!(
            "<x:protectedRange xmlns:x=\"{X_STRICT}\" sqref=\"A1\" name=\"r\"><x:securityDescriptor>O:WD</x:securityDescriptor></x:protectedRange>"
        )
    );
}

#[test]
fn enumeration_values_take_their_strict_spelling() {
    let sorting =
        strict::<SuggestedSorting>("<w:stylePaneSortMethod xmlns:w=\"{w}\" w:val=\"0000\"/>");
    assert!(sorting.contains("w:val=\"name\""), "{sorting}");
    let area = strict::<PivotArea>("<x:pivotArea xmlns:x=\"{x}\" type=\"topRight\"/>");
    assert!(area.contains("type=\"topEnd\""), "{area}");
}

#[test]
fn a_character_set_code_takes_its_iana_name() {
    assert_eq!(
        strict::<CharacterSetSupportedByFont>("<w:charset xmlns:w=\"{w}\" w:val=\"00\"/>"),
        format!("<w:charset xmlns:w=\"{W_STRICT}\" w:characterSet=\"iso-8859-1\"/>")
    );
    assert_eq!(
        refusal::<CharacterSetSupportedByFont>("<w:charset xmlns:w=\"{w}\" w:val=\"01\"/>"),
        Some("charset/@val")
    );
    assert_eq!(
        strict::<CharacterSetSupportedByFont>(
            "<w:charset xmlns:w=\"{w}\" w:val=\"02\" w:characterSet=\"windows-1252\"/>"
        ),
        format!("<w:charset xmlns:w=\"{W_STRICT}\" w:characterSet=\"windows-1252\"/>")
    );
    let import = strict::<TextImportSettings>("<x:textPr xmlns:x=\"{x}\" codePage=\"932\"/>");
    assert!(import.contains("characterSet=\"shift_jis\""), "{import}");
    assert_eq!(
        refusal::<TextImportSettings>("<x:textPr xmlns:x=\"{x}\" codePage=\"720\"/>"),
        Some("textPr/@codePage")
    );
}

#[test]
fn a_serial_date_takes_its_iso_attribute() {
    let filter = strict::<DynamicFilter>(
        "<x:dynamicFilter xmlns:x=\"{x}\" type=\"today\" maxVal=\"46309.5\"/>",
    );
    assert!(
        filter.contains("maxValIso=\"2026-10-14T12:00:00\""),
        "{filter}"
    );
    assert_eq!(
        iso_date_time("46309", true).as_deref(),
        Some("2030-10-15T00:00:00")
    );
    assert_eq!(
        iso_date_time("-4.25", false).as_deref(),
        Some("1899-12-25T18:00:00")
    );
    assert_eq!(
        iso_date_time("1.5E0", false).as_deref(),
        Some("1899-12-31T12:00:00")
    );
    assert_eq!(iso_date_time("-693594", false), None);
    assert_eq!(iso_date_time("today", false), None);
}

#[test]
fn legacy_content_has_no_strict_form() {
    assert_eq!(
        refusal::<CompatibilitySettings>("<w:compat xmlns:w=\"{w}\"><w:useFELayout/></w:compat>"),
        Some("useFELayout")
    );
    assert_eq!(
        refusal::<DocumentEditingRestrictions>(
            "<w:documentProtection xmlns:w=\"{w}\" w:hash=\"AAAA\" w:salt=\"AAAA\"/>"
        ),
        Some("documentProtection/@hash")
    );
}

#[test]
fn a_date_cell_is_written_as_it_is() {
    assert_eq!(
        strict::<Cell>(
            "<x:c xmlns:x=\"{x}\" r=\"A1\" t=\"d\"><x:v>2026-10-14T00:00:00</x:v></x:c>"
        ),
        format!(
            "<x:c xmlns:x=\"{X_STRICT}\" r=\"A1\" t=\"d\"><x:v>2026-10-14T00:00:00</x:v></x:c>"
        )
    );
}

/// The same declarations with the Strict URIs a Strict write gives them.
fn strict_run_declarations() -> String {
    format!(
        "xmlns:w=\"{W_STRICT}\" xmlns:mc=\"{MC}\" xmlns:wp=\"{WP_STRICT}\" xmlns:wps=\"{WPS}\" xmlns:a=\"{A_STRICT}\" xmlns:v=\"{V}\""
    )
}

#[test]
fn word_s_text_box_takes_wp_and_writes_its_text_in_wne_txbx_content() {
    let shape = "<a:graphic><a:graphicData uri=\"{wps}\"><wps:wsp normalEastAsianFlow=\"true\"><wps:cNvSpPr txBox=\"1\"/><wps:spPr/><wps:txbx id=\"3\"><w:txbxContent><w:p><w:r><w:t>Box</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic>";
    let run = format!(
        "<w:r {RUN_DECLARATIONS}><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing><wp:inline>{shape}</wp:inline></w:drawing></mc:Choice><mc:Fallback><w:pict><v:shape/></w:pict></mc:Fallback></mc:AlternateContent></w:r>"
    );
    assert_eq!(
        strict::<TextRun>(&run),
        format!(
            "<w:r {}><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{WPS}\"><wp:wsp normalEastAsianFlow=\"true\"><wp:cNvSpPr txBox=\"1\"/><wp:spPr/><wp:txbx id=\"3\"><wne:txbxContent xmlns:wne=\"{WNE}\"><w:p><w:r><w:t>Box</w:t></w:r></w:p></wne:txbxContent></wp:txbx><wp:bodyPr/></wp:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>",
            strict_run_declarations()
        )
    );
}

#[test]
fn word_s_shape_outside_alternate_content_takes_wp_a_linked_text_box_included() {
    let run = format!(
        "<w:r {RUN_DECLARATIONS}><w:drawing><wp:anchor><a:graphic><a:graphicData uri=\"{{wps}}\"><wps:wsp><wps:cNvSpPr/><wps:spPr/><wps:linkedTxbx id=\"3\" seq=\"1\"/><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"
    );
    assert_eq!(
        strict::<TextRun>(&run),
        format!(
            "<w:r {}><w:drawing><wp:anchor><a:graphic><a:graphicData uri=\"{WPS}\"><wp:wsp><wp:cNvSpPr/><wp:spPr/><wp:linkedTxbx id=\"3\" seq=\"1\"/><wp:bodyPr/></wp:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>",
            strict_run_declarations()
        )
    );
}

#[test]
fn a_drawing_inside_a_text_box_keeps_its_own_wp_names() {
    let run = format!(
        "<w:r {RUN_DECLARATIONS}><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{{wps}}\"><wps:wsp><wps:txbx><w:txbxContent><w:p><w:r><w:drawing><wp:inline><wp:extent cx=\"1\" cy=\"1\"/></wp:inline></w:drawing></w:r></w:p></w:txbxContent></wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    );
    assert_eq!(
        strict::<TextRun>(&run),
        format!(
            "<w:r {}><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{WPS}\"><wp:wsp><wp:txbx><wne:txbxContent xmlns:wne=\"{WNE}\"><w:p><w:r><w:drawing><wp:inline><wp:extent cx=\"1\" cy=\"1\"/></wp:inline></w:drawing></w:r></w:p></wne:txbxContent></wp:txbx></wp:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>",
            strict_run_declarations()
        )
    );
}

#[test]
fn word_s_group_takes_wp_and_writes_its_text_in_wne_txbx_content() {
    let run = format!(
        "<w:r {RUN_DECLARATIONS} xmlns:wpg=\"{{wpg}}\"><mc:AlternateContent><mc:Choice Requires=\"wpg\"><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{{wpg}}\"><wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr/><wps:wsp><wps:cNvSpPr txBox=\"1\"/><wps:spPr/><wps:txbx id=\"3\"><w:txbxContent><w:p><w:pPr><w:jc w:val=\"left\"/></w:pPr><w:r><w:t>Box</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp><wpg:grpSp><wpg:cNvGrpSpPr/><wpg:grpSpPr/></wpg:grpSp></wpg:wgp></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice><mc:Fallback><w:pict><v:group/></w:pict></mc:Fallback></mc:AlternateContent></w:r>"
    );
    assert_eq!(
        strict::<TextRun>(&run),
        format!(
            "<w:r {} xmlns:wpg=\"{WPG}\"><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{WPG}\"><wp:wgp><wp:cNvGrpSpPr/><wp:grpSpPr/><wp:wsp><wp:cNvSpPr txBox=\"1\"/><wp:spPr/><wp:txbx id=\"3\"><wne:txbxContent xmlns:wne=\"{WNE}\"><w:p><w:pPr><w:jc w:val=\"start\"/></w:pPr><w:r><w:t>Box</w:t></w:r></w:p></wne:txbxContent></wp:txbx><wp:bodyPr/></wp:wsp><wp:grpSp><wp:cNvGrpSpPr/><wp:grpSpPr/></wp:grpSp></wp:wgp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>",
            strict_run_declarations()
        )
    );
}

#[test]
fn word_s_canvas_takes_wp_and_writes_its_text_in_wne_txbx_content() {
    let run = format!(
        "<w:r {RUN_DECLARATIONS} xmlns:wpc=\"{{wpc}}\"><mc:AlternateContent><mc:Choice Requires=\"wpc\"><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{{wpc}}\"><wpc:wpc><wps:wsp><wps:cNvSpPr/><wps:spPr/><wps:txbx><w:txbxContent><w:p/></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></wpc:wpc></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice><mc:Fallback><w:pict><v:group/></w:pict></mc:Fallback></mc:AlternateContent></w:r>"
    );
    assert_eq!(
        strict::<TextRun>(&run),
        format!(
            "<w:r {} xmlns:wpc=\"{WPC}\"><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{WPC}\"><wp:wpc><wp:wsp><wp:cNvSpPr/><wp:spPr/><wp:txbx><wne:txbxContent xmlns:wne=\"{WNE}\"><w:p/></wne:txbxContent></wp:txbx><wp:bodyPr/></wp:wsp></wp:wpc></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>",
            strict_run_declarations()
        )
    );
}

#[test]
fn vml_fallback_of_any_other_choice_stays_refused() {
    assert_eq!(
        refusal::<TextRun>(&format!(
            "<w:r {RUN_DECLARATIONS}><mc:AlternateContent><mc:Choice Requires=\"wpi\"><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{{a}}\"/></a:graphic></wp:inline></w:drawing></mc:Choice><mc:Fallback><w:pict><v:shape/></w:pict></mc:Fallback></mc:AlternateContent></w:r>"
        )),
        Some("pict")
    );
}

#[test]
fn a_text_box_in_vml_has_no_strict_form() {
    assert_eq!(
        refusal::<TextRun>(&format!(
            "<w:r {RUN_DECLARATIONS}><w:pict><v:shape><v:textbox><w:txbxContent><w:p/></w:txbxContent></v:textbox></v:shape></w:pict></w:r>"
        )),
        Some("pict")
    );
}

#[test]
fn a_chart_na_as_blank_reads_and_writes_the_unprefixed_val_word_writes() {
    let text = format!("<c16r3:dispNaAsBlank xmlns:c16r3=\"{C16R3}\" val=\"1\"/>");
    let node = DispNaAsBlank::from_strict(&element(&text)).unwrap();
    assert_eq!(node.val.as_deref(), Some("1"));
    let mut set = DispNaAsBlank::default();
    set.val = Some("1".to_owned());
    assert_eq!(written(set.to_strict().unwrap()), text);
}

/// `<wp14:local>value</wp14:local>`.
fn relative(local: &str, value: &str) -> String {
    format!("<wp14:{local} xmlns:wp14=\"{WP14}\">{value}</wp14:{local}>")
}

#[test]
fn a_relative_size_or_position_is_a_percentage_in_strict() {
    let width = PctWidth::from_strict(&element(&relative("pctWidth", "40000"))).unwrap();
    assert_eq!(
        written(width.to_strict().unwrap()),
        relative("pctWidth", "40%")
    );
    let height = PctHeight::from_strict(&element(&relative("pctHeight", "0"))).unwrap();
    assert_eq!(
        written(height.to_strict().unwrap()),
        relative("pctHeight", "0%")
    );
    let across = PctPosHOffset::from_strict(&element(&relative("pctPosHOffset", "45500"))).unwrap();
    assert_eq!(
        written(across.to_strict().unwrap()),
        relative("pctPosHOffset", "45.5%")
    );
    let down = PctPosVOffset::from_strict(&element(&relative("pctPosVOffset", "-2500"))).unwrap();
    assert_eq!(
        written(down.to_strict().unwrap()),
        relative("pctPosVOffset", "-2.5%")
    );
    let spelled = PctWidth::from_strict(&element(&relative("pctWidth", "36%"))).unwrap();
    assert_eq!(
        written(spelled.to_strict().unwrap()),
        relative("pctWidth", "36%")
    );
}

#[test]
fn a_chart_style_baseline_is_a_percentage_in_strict() {
    let source = |baseline: &str| {
        format!("<cs:defRPr xmlns:cs=\"{CS}\" sz=\"1400\" baseline=\"{baseline}\"/>")
    };
    let node = DefRPr::from_strict(&element(&source("30000"))).unwrap();
    assert_eq!(written(node.to_strict().unwrap()), source("30%"));
}

#[test]
fn a_word_text_effect_keeps_its_whole_numbers_in_strict() {
    let source = format!("<w14:shadow xmlns:w14=\"{W14}\" w14:sx=\"100000\" w14:sy=\"100000\"/>");
    let node = Shadow::from_strict(&element(&source)).unwrap();
    assert_eq!(written(node.to_strict().unwrap()), source);
}

#[test]
fn refusals_names_every_refusal_in_document_order() {
    let run = element(&format!(
        "<w:r {RUN_DECLARATIONS}><w:pict><v:shape><v:textbox><w:txbxContent><w:p/></w:txbxContent></v:textbox></v:shape></w:pict><w:pict/></w:r>"
    ));
    assert_eq!(
        refusals(run),
        vec![
            Refusal {
                prefix: "w",
                tag: "pict"
            };
            2
        ]
    );
    assert_eq!(
        refusals(element(
            "<w:settings xmlns:w=\"{w}\" xmlns:o=\"urn:schemas-microsoft-com:office:office\"><w:compat><w:useFELayout/><w:shapeLayoutLikeWW8/></w:compat><w:shapeDefaults><o:shapelayout/></w:shapeDefaults></w:settings>"
        ))
        .iter()
        .map(|refusal| refusal.tag)
        .collect::<Vec<_>>(),
        ["shapeDefaults", "useFELayout", "shapeLayoutLikeWW8"]
    );
}

#[test]
fn refusals_names_the_namespace_of_a_vml_background() {
    assert_eq!(
        refusals(element(
            "<w:background xmlns:w=\"{w}\" xmlns:v=\"{v}\" w:color=\"ffffff\"><v:background fillcolor=\"yellow\"/></w:background>"
        )),
        [Refusal {
            prefix: "v",
            tag: "background"
        }]
    );
}

#[test]
fn the_first_of_refusals_is_what_a_write_refuses() {
    let text = format!("<w:r {RUN_DECLARATIONS}><w:pict/><w:object><v:shape/></w:object></w:r>");
    assert_eq!(
        refusals(element(&text)).first().map(|refusal| refusal.tag),
        refusal::<TextRun>(&text)
    );
}

#[test]
fn a_protection_with_no_password_takes_part_1_s_attributes() {
    assert_eq!(
        strict::<DocumentEditingRestrictions>(
            "<w:documentProtection xmlns:w=\"{w}\" w:cryptProviderType=\"rsaAES\" w:cryptAlgorithmClass=\"hash\" w:cryptAlgorithmType=\"typeAny\" w:cryptAlgorithmSid=\"14\" w:cryptSpinCount=\"100000\"/>"
        ),
        format!(
            "<w:documentProtection xmlns:w=\"{W_STRICT}\" w:algorithmName=\"SHA-512\" w:spinCount=\"100000\"/>"
        )
    );
    assert_eq!(
        strict::<DocumentEditingRestrictions>(
            "<w:documentProtection xmlns:w=\"{w}\" w:edit=\"readOnly\" w:enforcement=\"0\" w:cryptProviderType=\"rsaFull\" w:cryptAlgorithmClass=\"hash\" w:cryptAlgorithmType=\"typeAny\" w:cryptAlgorithmSid=\"4\" w:cryptSpinCount=\"50000\"/>"
        ),
        format!(
            "<w:documentProtection xmlns:w=\"{W_STRICT}\" w:edit=\"readOnly\" w:enforcement=\"0\" w:algorithmName=\"SHA-1\" w:spinCount=\"50000\"/>"
        )
    );
    assert_eq!(
        strict::<WriteProtection>(
            "<w:writeProtection xmlns:w=\"{w}\" w:cryptProviderType=\"rsaAES\" w:cryptAlgorithmClass=\"hash\" w:cryptAlgorithmType=\"typeAny\" w:cryptAlgorithmSid=\"12\" w:cryptSpinCount=\"100000\"/>"
        ),
        format!(
            "<w:writeProtection xmlns:w=\"{W_STRICT}\" w:algorithmName=\"SHA-256\" w:spinCount=\"100000\"/>"
        )
    );
}

#[test]
fn a_protection_part_1_has_no_slot_for_stays_refused() {
    for attributes in [
        "w:cryptAlgorithmSid=\"4\" w:hash=\"9oN7nWkCAyEZib1RomSJTjmPpCY=\"",
        "w:cryptAlgorithmSid=\"4\" w:salt=\"ZUdHa+D8F/OAKP3I7ssUnQ==\"",
        "w:cryptAlgorithmSid=\"5\"",
        "w:cryptProvider=\"Provider\"",
        "w:algIdExt=\"0000A5691D\"",
        "w:cryptProviderTypeExt=\"00A5691D\"",
        "w:cryptAlgorithmClass=\"custom\"",
        "w:cryptAlgorithmType=\"custom\"",
        "w:cryptProviderType=\"custom\"",
    ] {
        assert!(
            refusal::<DocumentEditingRestrictions>(&format!(
                "<w:documentProtection xmlns:w=\"{{w}}\" {attributes}/>"
            ))
            .is_some(),
            "{attributes}"
        );
    }
}

#[test]
fn a_vml_background_holding_only_a_color_becomes_the_background_s_attributes() {
    assert_eq!(
        strict::<DocumentBackground>(
            "<w:background xmlns:w=\"{w}\" xmlns:v=\"{v}\" w:color=\"ffffff\"><v:background id=\"_x0000_s1025\" w:themeColor=\"background1\" filled=\"t\"/></w:background>"
        ),
        format!(
            "<w:background xmlns:w=\"{W_STRICT}\" xmlns:v=\"{V}\" w:color=\"ffffff\" w:themeColor=\"background1\"/>"
        )
    );
}

#[test]
fn a_vml_background_with_a_fill_of_its_own_stays_refused() {
    for background in [
        "<v:background id=\"_x0000_s1025\" fillcolor=\"yellow\"/>",
        "<v:background id=\"_x0000_s1025\"><v:fill type=\"gradient\"/></v:background>",
        "<v:background id=\"_x0000_s1025\" filled=\"f\"/>",
    ] {
        assert_eq!(
            refusal::<DocumentBackground>(&format!(
                "<w:background xmlns:w=\"{{w}}\" xmlns:v=\"{{v}}\" w:color=\"ffffff\">{background}</w:background>"
            )),
            Some("background"),
            "{background}"
        );
    }
}
