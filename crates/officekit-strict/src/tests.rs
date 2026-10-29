use officekit_markup::drawing::Alpha;
use officekit_markup::office_drawing_2017_03_chart::DispNaAsBlank;
use officekit_markup::presentation::Presentation;
use officekit_markup::spreadsheet::{
    Border, CalculationChainInfo, Cell, DynamicFilter, PivotArea, ProtectedRange,
    TextImportSettings, Workbook,
};
use officekit_markup::wordprocessing::{
    CharacterSetSupportedByFont, CheckboxFormFieldProperties, CompatibilitySettings, Document,
    DocumentEditingRestrictions, DropDownListFormFieldProperties, FramesetSplitterProperties,
    Paragraph, ParagraphConditionalFormatting, ParagraphIndentation, PreferredTableWidth,
    RunProperties, SuggestedSorting, TableCellBorders,
    TableStyleConditionalFormattingSettingsException, TextRun,
};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Document as Part, Element};
use officekit_xml::error::{ReadError, WriteError};
use officekit_xml::parse::parse;
use officekit_xml::write::{Layout, WriteOptions};

use super::{Strict, iso_date_time};

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const C16R3: &str = "http://schemas.microsoft.com/office/drawing/2017/03/chart";
const A_STRICT: &str = "http://purl.oclc.org/ooxml/drawingml/main";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const P_STRICT: &str = "http://purl.oclc.org/ooxml/presentationml/main";
const V: &str = "urn:schemas-microsoft-com:vml";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const WP_STRICT: &str = "http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing";
const WPG: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup";
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
        format!("<a:alpha xmlns:a=\"{A_STRICT}\" val=\"12.35%\"/>")
    );
    assert_eq!(
        strict::<Alpha>("<a:alpha xmlns:a=\"{a}\" val=\"25%\"/>"),
        format!("<a:alpha xmlns:a=\"{A_STRICT}\" val=\"25%\"/>")
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
fn word_s_text_box_writes_as_the_ecma_text_box_in_place_of_its_alternate_content() {
    let shape = "<a:graphic><a:graphicData uri=\"{wps}\"><wps:wsp normalEastAsianFlow=\"true\"><wps:cNvSpPr txBox=\"1\"/><wps:spPr/><wps:txbx id=\"3\"><w:txbxContent><w:p><w:r><w:t>Box</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic>";
    let run = format!(
        "<w:r {RUN_DECLARATIONS}><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing><wp:inline>{shape}</wp:inline></w:drawing></mc:Choice><mc:Fallback><w:pict><v:shape/></w:pict></mc:Fallback></mc:AlternateContent></w:r>"
    );
    assert_eq!(
        strict::<TextRun>(&run),
        format!(
            "<w:r {}><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{WP_STRICT}\"><wp:wsp normalEastAsianFlow=\"true\"><wp:cNvSpPr txBox=\"1\"/><wp:spPr/><wp:txbx id=\"3\"><wp:txbxContent><w:p><w:r><w:t>Box</w:t></w:r></w:p></wp:txbxContent></wp:txbx><wp:bodyPr/></wp:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>",
            strict_run_declarations()
        )
    );
}

#[test]
fn word_s_shape_outside_alternate_content_converts_in_place_a_linked_text_box_included() {
    let run = format!(
        "<w:r {RUN_DECLARATIONS}><w:drawing><wp:anchor><a:graphic><a:graphicData uri=\"{{wps}}\"><wps:wsp><wps:cNvSpPr/><wps:spPr/><wps:linkedTxbx id=\"3\" seq=\"1\"/><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"
    );
    assert_eq!(
        strict::<TextRun>(&run),
        format!(
            "<w:r {}><w:drawing><wp:anchor><a:graphic><a:graphicData uri=\"{WP_STRICT}\"><wp:wsp><wp:cNvSpPr/><wp:spPr/><wp:linkedTxbx id=\"3\" seq=\"1\"/><wp:bodyPr/></wp:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>",
            strict_run_declarations()
        )
    );
}

#[test]
fn a_text_box_in_a_group_or_in_vml_has_no_strict_form() {
    assert_eq!(
        refusal::<TextRun>(&format!(
            "<w:r {RUN_DECLARATIONS} xmlns:wpg=\"{{wpg}}\"><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{{wpg}}\"><wpg:wgp><wps:wsp><wps:txbx><w:txbxContent><w:p/></w:txbxContent></wps:txbx></wps:wsp></wpg:wgp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
        )),
        Some("txbxContent")
    );
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
