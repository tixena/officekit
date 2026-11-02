use officekit_markup::chart::{GapWidth, LabelOffset, Overlap};
use officekit_markup::common_simple_types::{
    BooleanValue, CryptographicAlgorithmClasses, CryptographicAlgorithmTypes,
    CryptographicProviderTypes, DocumentConformanceClassValue, DocumentConformanceClassValueMember,
    MeasurementInTwentiethsOfAPoint, OnOffValue,
};
use officekit_markup::drawing::Alpha;
use officekit_markup::office_drawing_2012_chart_style::DefRPr;
use officekit_markup::office_drawing_2017_03_chart::DispNaAsBlank;
use officekit_markup::office_word_2010_wordml::Shadow;
use officekit_markup::office_word_2010_wordprocessing_drawing::{
    PctHeight, PctPosHOffset, PctPosVOffset, PctWidth,
};
use officekit_markup::presentation::Presentation;
use officekit_markup::spreadsheet::{
    Border, CalculationChainInfo, Cell, CellCalcCell, CellValue, DynamicFilter, PivotArea,
    ProtectedRange, ReferenceSequence, Text as SpreadsheetText, TextImportSettings, Workbook,
};
use officekit_markup::vml::{
    DocumentBackground as VmlBackground, ShapeDefinition, ShapeFillProperties, ShapeFillType,
};
use officekit_markup::vml_office_drawing::{EmbeddedOLEObject, OLEType, ShapeLayoutProperties};
use officekit_markup::wordprocessing::{
    BorderStyles, CharacterSetSupportedByFont, CheckboxFormFieldProperties, ColorValue,
    CompatibilitySettings, ConditionalFormattingBitmask, DecimalNumberValue,
    DefaultPropertiesForVMLObjectsInHeaderAndFooter, DefaultPropertiesForVMLObjectsInMainDocument,
    Document, DocumentBackground, DocumentEditingRestrictions, DocumentProtectionTypes,
    DocumentSettings, DropDownListFormFieldProperties, EightDigitHexadecimalValue, EmbeddedObject,
    EmbeddedObjectContent, EmbeddedObjectUpdateModes, FontFamily, FontFamilyValue,
    FourDigitHexadecimalValue, FramesetSplitterProperties, HorizontalAlignmentType,
    HorizontalAlignmentTypeMember, LeftParagraphBorder, LinkedObjectProperties, Paragraph,
    ParagraphAlignment, ParagraphConditionalFormatting, ParagraphContent, ParagraphIndentation,
    ParagraphProperties, PreferredTableWidth, PropertiesForASingleFont, RightParagraphBorder,
    RunProperties, SignedMeasurementInTwentiethsOfPoint, SuggestedSorting, TableCellBorders,
    TableCellLeadingEdgeBorder, TableStyleConditionalFormattingSettingsException, Text as WordText,
    TextRun, TextRunContent, TwoDigitHexadecimalValue, VMLObject, VmlElementContent,
    VmlOfficeDrawingElementContent, WebPageSettings, WriteProtection, write,
};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Document as Part, Element};
use officekit_xml::error::{ReadError, WriteError};
use officekit_xml::parse::parse;
use officekit_xml::write::{Layout, Scope, WriteOptions};

use super::{Refusal, Strict, iso_date_time, refusals};

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const C16R3: &str = "http://schemas.microsoft.com/office/drawing/2017/03/chart";
const C_STRICT: &str = "http://purl.oclc.org/ooxml/drawingml/chart";
const A_STRICT: &str = "http://purl.oclc.org/ooxml/drawingml/main";
const CS: &str = "http://schemas.microsoft.com/office/drawing/2012/chartStyle";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
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

/// What an on and off value holds; `None` when it is absent or holds no such value.
fn flag(value: Option<&OnOffValue>) -> Option<bool> {
    value?.value().ok()
}

fn number(value: &str) -> DecimalNumberValue {
    DecimalNumberValue::from(value.to_owned())
}

/// The refusal writing `node` in Strict gives; `None` when the write succeeds.
fn refused<T>(node: &T) -> Option<&'static str>
where
    T: Strict,
{
    if let Err(WriteError::NotInClass { class, tag }) = node.to_strict() {
        (class == ConformanceClass::Strict).then_some(tag)
    } else {
        None
    }
}

/// A run holding `content`.
fn run_of(content: TextRunContent) -> TextRun {
    let mut run = TextRun::default();
    run.content = vec![content];
    run
}

/// `node` written in Strict and read back.
fn through_strict<T>(node: &T) -> T
where
    T: Strict,
{
    T::from_strict(&node.to_strict().unwrap()).unwrap()
}

fn twips(value: &str) -> MeasurementInTwentiethsOfAPoint {
    MeasurementInTwentiethsOfAPoint::from(value.to_owned())
}

/// A run holding a `w:pict` of an empty VML shape.
fn vml_run() -> TextRun {
    let mut picture = VMLObject::default();
    picture.content = vec![VmlElementContent::ShapeDefinition(
        ShapeDefinition::default(),
    )];
    run_of(TextRunContent::VMLObject(picture))
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
    let mut alignment = ParagraphAlignment::default();
    alignment.val = Some(HorizontalAlignmentType::from(
        HorizontalAlignmentTypeMember::Left,
    ));
    let mut properties = ParagraphProperties::default();
    properties.paragraph_alignment = Some(Box::new(alignment));
    let mut characters = WordText::default();
    characters.value = "Hello".to_owned();
    let mut paragraph = Paragraph::default();
    paragraph.content = vec![ParagraphContent::TextRun(run_of(TextRunContent::Text(
        characters,
    )))];
    paragraph.properties = Some(Box::new(properties));
    assert_eq!(
        written(paragraph.to_strict().unwrap()),
        format!(
            "<w:p xmlns:w=\"{W_STRICT}\"><w:pPr><w:jc w:val=\"start\"/></w:pPr><w:r><w:t>Hello</w:t></w:r></w:p>"
        )
    );
}

#[test]
fn spreadsheet_string_elements_read_back_with_their_space() {
    let mut text = SpreadsheetText::default();
    text.space = Some("preserve".to_owned());
    text.value = " two".to_owned();
    assert_eq!(
        written(text.to_strict().unwrap()),
        format!("<x:t xmlns:x=\"{X_STRICT}\" xml:space=\"preserve\"> two</x:t>")
    );
    let read_text = through_strict(&text);
    assert_eq!(read_text.space, text.space);
    assert_eq!(read_text.value, text.value);
    let mut value = CellValue::default();
    value.space = Some("preserve".to_owned());
    value.value = " 1".to_owned();
    let read_value = through_strict(&value);
    assert_eq!(read_value.space, value.space);
    assert_eq!(read_value.value, value.value);
}

#[test]
fn spreadsheet_text_with_no_space_writes_no_attribute() {
    let mut text = SpreadsheetText::default();
    text.value = "two".to_owned();
    assert_eq!(
        written(text.to_strict().unwrap()),
        format!("<x:t xmlns:x=\"{X_STRICT}\">two</x:t>")
    );
}

#[test]
fn a_read_holds_xml_space_on_the_field_of_spreadsheet_text() {
    for uri in [X, X_STRICT] {
        let text = SpreadsheetText::from_strict(&element(&format!(
            "<x:t xmlns:x=\"{uri}\" xml:space=\"preserve\"> two</x:t>"
        )))
        .unwrap();
        assert_eq!(text.space.as_deref(), Some("preserve"));
        assert_eq!(text.value, " two");
        assert_eq!(text.unknown.attributes, []);
    }
}

#[test]
fn the_roots_of_the_main_parts_declare_strict_conformance() {
    let declared = |member| Some(DocumentConformanceClassValue::from(member));
    let mut document = Document::default();
    document.body = Some(Box::default());
    assert_eq!(
        through_strict(&document).conformance,
        declared(DocumentConformanceClassValueMember::Strict)
    );
    document.conformance = declared(DocumentConformanceClassValueMember::Transitional);
    assert_eq!(
        through_strict(&document).conformance,
        declared(DocumentConformanceClassValueMember::Strict)
    );
    assert_eq!(
        through_strict(&Workbook::default()).conformance,
        declared(DocumentConformanceClassValueMember::Strict)
    );
    assert_eq!(
        through_strict(&Presentation::default()).conformance,
        declared(DocumentConformanceClassValueMember::Strict)
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
fn a_vml_picture_takes_the_wrapper_word_writes() {
    assert_eq!(
        written(vml_run().to_strict().unwrap()),
        format!(
            "<w:r xmlns:w=\"{W_STRICT}\"><mc:AlternateContent xmlns:mc=\"{MC}\" xmlns:v=\"{V}\"><mc:Choice Requires=\"v\"><w:pict><v:shape xmlns:v=\"{V}\"/></w:pict></mc:Choice></mc:AlternateContent></w:r>"
        )
    );
}

#[test]
fn a_choice_that_requires_vml_stays_as_word_writes_it() {
    let written = strict::<TextRun>(&format!(
        "<w:r {RUN_DECLARATIONS}><mc:AlternateContent><mc:Choice Requires=\"v\"><w:pict><v:shape/></w:pict></mc:Choice></mc:AlternateContent></w:r>"
    ));
    assert_eq!(
        written,
        format!(
            "<w:r {}><mc:AlternateContent><mc:Choice Requires=\"v\"><w:pict><v:shape/></w:pict></mc:Choice></mc:AlternateContent></w:r>",
            strict_run_declarations()
        )
    );
}

#[test]
fn a_choice_matches_the_prefix_it_requires_by_its_namespace() {
    assert_eq!(
        refusal::<TextRun>(&format!(
            "<w:r {RUN_DECLARATIONS} xmlns:vml=\"{{v}}\"><mc:AlternateContent><mc:Choice Requires=\"vml\"><w:pict><vml:shape/></w:pict></mc:Choice></mc:AlternateContent></w:r>"
        )),
        None
    );
    assert_eq!(
        refusal::<TextRun>(&format!(
            "<w:r {RUN_DECLARATIONS}><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:pict><v:shape/></w:pict></mc:Choice></mc:AlternateContent></w:r>"
        )),
        Some("pict")
    );
}

#[test]
fn a_loose_vml_picture_takes_the_wrapper_with_no_fallback() {
    let text = written(vml_run().to_strict().unwrap());
    assert!(
        text.contains("<mc:Choice Requires=\"v\"><w:pict>"),
        "{text}"
    );
    assert!(text.contains(&format!("xmlns:v=\"{V}\"")), "{text}");
    assert!(!text.contains("Fallback"), "{text}");
}

#[test]
fn an_embedded_object_takes_the_wrapper_and_its_attribute_rules() {
    let mut ole = EmbeddedOLEObject::default();
    ole.prog_id = Some("PBrush".to_owned());
    ole.r#type = Some(OLEType::from("Embed".to_owned()));
    let mut link = LinkedObjectProperties::default();
    link.locked_field = Some(OnOffValue::from("on".to_owned()));
    link.update_mode = Some(EmbeddedObjectUpdateModes::from("always".to_owned()));
    let mut object = EmbeddedObject::default();
    object.content = Some(Box::new(EmbeddedObjectContent::LinkedObjectProperties(
        link,
    )));
    object.dxa_orig = Some(twips("3630"));
    object.dya_orig = Some(twips("1935"));
    object.vml_element_content = vec![
        VmlElementContent::ShapeDefinition(ShapeDefinition::default()),
        VmlElementContent::EmbeddedOLEObject(ole),
    ];
    let text = written(
        run_of(TextRunContent::EmbeddedObject(object))
            .to_strict()
            .unwrap(),
    );
    assert!(
        text.contains("<mc:Choice Requires=\"v\"><w:object w:dxaOrig=\"3630\" w:dyaOrig=\"1935\">"),
        "{text}"
    );
    assert!(text.contains("ProgID=\"PBrush\""), "{text}");
    assert!(text.contains("w:lockedField=\"true\""), "{text}");
}

#[test]
fn an_object_holding_no_vml_stays_in_the_run() {
    let mut object = EmbeddedObject::default();
    object.drawingml_object = Some(Box::default());
    object.dxa_orig = Some(twips("3630"));
    let text = written(
        run_of(TextRunContent::EmbeddedObject(object))
            .to_strict()
            .unwrap(),
    );
    assert!(
        text.ends_with("><w:object w:dxaOrig=\"3630\"><w:drawing/></w:object></w:r>"),
        "{text}"
    );
}

#[test]
fn a_vml_picture_bullet_stays_refused() {
    assert_eq!(
        refusals(element(
            "<w:numbering xmlns:w=\"{w}\" xmlns:v=\"{v}\"><w:numPicBullet w:numPicBulletId=\"0\"><w:pict><v:shape/></w:pict></w:numPicBullet></w:numbering>"
        )),
        [Refusal {
            prefix: "w",
            tag: "pict"
        }]
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
    let look = |first_row: Option<&str>| {
        let mut look = TableStyleConditionalFormattingSettingsException::default();
        look.first_row = first_row.map(|value| OnOffValue::from(value.to_owned()));
        look.val = Some(FourDigitHexadecimalValue::from("04A0".to_owned()));
        let unpacked = through_strict(&look);
        assert_eq!(unpacked.val, None);
        [
            unpacked.first_row,
            unpacked.last_row,
            unpacked.first_column,
            unpacked.last_column,
            unpacked.no_h_band,
            unpacked.no_v_band,
        ]
        .map(|value| flag(value.as_ref()))
    };
    let (yes, no) = (Some(true), Some(false));
    assert_eq!(look(None), [yes, no, yes, no, no, yes]);
    assert_eq!(look(Some("0")), [no, no, yes, no, no, yes]);
    let style = |val: &str| {
        let mut style = ParagraphConditionalFormatting::default();
        style.val = Some(ConditionalFormattingBitmask::from(val.to_owned()));
        style
    };
    let unpacked = through_strict(&style("101000000100"));
    assert_eq!(unpacked.val, None);
    assert_eq!(
        [
            unpacked.first_row,
            unpacked.last_row,
            unpacked.first_column,
            unpacked.first_row_first_column,
            unpacked.last_row_first_column,
        ]
        .map(|value| flag(value.as_ref())),
        [yes, no, yes, yes, no]
    );
    assert_eq!(refused(&style("1010")), Some("cnfStyle/@val"));
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
    let style = |value: &str| Some(BorderStyles::from(value.to_owned()));
    let mut left = LeftParagraphBorder::default();
    left.val = style("single");
    let mut right = RightParagraphBorder::default();
    right.val = style("nil");
    let mut sides = TableCellBorders::default();
    sides.left_paragraph_border = Some(Box::new(left.clone()));
    sides.right_paragraph_border = Some(Box::new(right));
    let renamed = through_strict(&sides);
    assert_eq!(renamed.left_paragraph_border, None);
    assert_eq!(renamed.right_paragraph_border, None);
    assert_eq!(
        renamed
            .table_cell_leading_edge_border
            .and_then(|border| border.val),
        style("single")
    );
    assert_eq!(
        renamed
            .table_cell_trailing_margin_exception
            .and_then(|border| border.val),
        style("nil")
    );
    let mut start = TableCellLeadingEdgeBorder::default();
    start.val = style("double");
    let mut both = TableCellBorders::default();
    both.left_paragraph_border = Some(Box::new(left));
    both.table_cell_leading_edge_border = Some(Box::new(start));
    let kept = through_strict(&both);
    assert_eq!(kept.left_paragraph_border, None);
    assert_eq!(
        kept.table_cell_leading_edge_border
            .and_then(|border| border.val),
        style("double")
    );
    let mut border = Border::default();
    border.leading_edge_border_left = Some(Box::default());
    border.trailing_edge_border_end = Some(Box::default());
    let cell_border = through_strict(&border);
    assert_eq!(cell_border.leading_edge_border_left, None);
    assert!(cell_border.leading_edge_border_start.is_some());
    assert!(cell_border.trailing_edge_border_end.is_some());
    let measure = |value: &str| Some(SignedMeasurementInTwentiethsOfPoint::from(value.to_owned()));
    let mut indentation = ParagraphIndentation::default();
    indentation.hanging = Some(twips("360"));
    indentation.left = measure("720");
    let moved = through_strict(&indentation);
    assert_eq!((moved.start, moved.left), (measure("720"), None));
    assert_eq!(moved.hanging, Some(twips("360")));
    indentation.start = measure("10");
    let started = through_strict(&indentation);
    assert_eq!((started.start, started.left), (measure("10"), None));
    let mut cell = CellCalcCell::default();
    cell.i = Some("1".to_owned());
    cell.r#ref = Some("A1".to_owned());
    let mut chain = CalculationChainInfo::default();
    chain.cell_calc_cell = vec![cell];
    let cells = through_strict(&chain).cell_calc_cell;
    let first = cells.first().unwrap();
    assert_eq!(
        (first.r.as_deref(), first.r#ref.as_deref()),
        (Some("A1"), None)
    );
    assert_eq!(first.i.as_deref(), Some("1"));
    let mut range = ProtectedRange::default();
    range.name = Some("r".to_owned());
    range.security_descriptor_attribute = Some("O:WD".to_owned());
    range.sqref = Some(ReferenceSequence::from("A1".to_owned()));
    let protected = through_strict(&range);
    assert_eq!(protected.security_descriptor_attribute, None);
    assert_eq!(
        protected
            .security_descriptor
            .iter()
            .map(|descriptor| descriptor.value.as_str())
            .collect::<Vec<_>>(),
        ["O:WD"]
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
fn a_font_drops_a_character_set_code_with_no_name() {
    let font = |name: &str, code: &str| {
        let mut charset = CharacterSetSupportedByFont::default();
        charset.val = Some(TwoDigitHexadecimalValue::from(code.to_owned()));
        let mut font = PropertiesForASingleFont::default();
        font.character_set_supported_by_font = Some(Box::new(charset));
        font.name = Some(name.to_owned());
        font
    };
    let character_set =
        |held: PropertiesForASingleFont| held.character_set_supported_by_font?.character_set;
    for code in ["01", "02"] {
        let mut family = FontFamily::default();
        family.val = Some(FontFamilyValue::from("auto".to_owned()));
        let mut wingdings = font("Wingdings", code);
        wingdings.family = Some(Box::new(family));
        let dropped = through_strict(&wingdings);
        assert_eq!(dropped.character_set_supported_by_font, None, "{code}");
        assert!(dropped.family.is_some(), "{code}");
    }
    assert_eq!(
        character_set(through_strict(&font("Times New Roman", "00"))).as_deref(),
        Some("iso-8859-1")
    );
    let mut given = font("Wingdings", "02");
    if let Some(charset) = given.character_set_supported_by_font.as_mut() {
        charset.character_set = Some("windows-1252".to_owned());
    }
    assert_eq!(
        character_set(through_strict(&given)).as_deref(),
        Some("windows-1252")
    );
    assert_eq!(refused(&font("Terminal", "FF")), Some("charset/@val"));
    let mut loose = CharacterSetSupportedByFont::default();
    loose.val = Some(TwoDigitHexadecimalValue::from("01".to_owned()));
    assert_eq!(
        refused(&loose),
        Some("charset/@val"),
        "a character set outside a font stays refused"
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
    let mut compatibility = CompatibilitySettings::default();
    compatibility.ignore_page_break_from_continuous_section_break = Some(Box::default());
    assert_eq!(refused(&compatibility), Some("footnoteLayoutLikeWW8"));
    let mut protection = DocumentEditingRestrictions::default();
    protection.hash = Some("AAAA".to_owned());
    protection.salt = Some("AAAA".to_owned());
    assert_eq!(refused(&protection), Some("documentProtection/@hash"));
}

#[test]
fn a_strict_save_removes_the_settings_word_leaves_out() {
    let mut compatibility = CompatibilitySettings::default();
    compatibility.do_not_bypass_east_asian_complex_script_layout_code = Some(Box::default());
    compatibility.ignore_text_wrapping_around_objects_at_bottom_of_page = Some(Box::default());
    let removed = through_strict(&compatibility);
    assert_eq!(
        removed.do_not_bypass_east_asian_complex_script_layout_code,
        None
    );
    assert_eq!(
        removed.ignore_text_wrapping_around_objects_at_bottom_of_page,
        None
    );
    let layout = || {
        vec![VmlOfficeDrawingElementContent::ShapeLayoutProperties(
            ShapeLayoutProperties::default(),
        )]
    };
    let mut main_defaults = DefaultPropertiesForVMLObjectsInMainDocument::default();
    main_defaults.content = layout();
    let mut header_defaults = DefaultPropertiesForVMLObjectsInHeaderAndFooter::default();
    header_defaults.content = layout();
    let mut settings = DocumentSettings::default();
    settings.default_properties_for_vml_objects_in_header_and_footer =
        Some(Box::new(header_defaults));
    settings.default_properties_for_vml_objects_in_main_document = Some(Box::new(main_defaults));
    assert_eq!(
        refusals(write::document_settings(&settings, &mut Scope::default())),
        []
    );
    let without = through_strict(&settings);
    assert_eq!(
        without.default_properties_for_vml_objects_in_header_and_footer,
        None
    );
    assert_eq!(
        without.default_properties_for_vml_objects_in_main_document,
        None
    );
    let mut web = WebPageSettings::default();
    web.utilize_vml_when_saving_as_web_page = Some(Box::default());
    assert_eq!(
        refusals(write::web_page_settings(&web, &mut Scope::default())),
        []
    );
    assert_eq!(
        through_strict(&web).utilize_vml_when_saving_as_web_page,
        None
    );
    compatibility.ignore_page_break_from_continuous_section_break = Some(Box::default());
    assert_eq!(
        refused(&compatibility),
        Some("footnoteLayoutLikeWW8"),
        "what else the part holds stays refused"
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
fn a_text_box_in_vml_takes_the_wrapper_and_writes_its_text_in_wne_txbx_content() {
    assert_eq!(
        strict::<TextRun>(&format!(
            "<w:r {RUN_DECLARATIONS}><w:pict><v:shape><v:textbox><w:txbxContent><w:p><w:pPr><w:jc w:val=\"left\"/></w:pPr></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r>"
        )),
        format!(
            "<w:r {}><mc:AlternateContent><mc:Choice Requires=\"v\"><w:pict><v:shape><v:textbox><wne:txbxContent xmlns:wne=\"{WNE}\"><w:p><w:pPr><w:jc w:val=\"start\"/></w:pPr></w:p></wne:txbxContent></v:textbox></v:shape></w:pict></mc:Choice></mc:AlternateContent></w:r>",
            strict_run_declarations()
        )
    );
}

#[test]
fn what_a_vml_text_box_holds_is_still_refused_with_its_own_label() {
    assert_eq!(
        refusal::<TextRun>(&format!(
            "<w:r {RUN_DECLARATIONS}><w:pict><v:shape><v:textbox><w:txbxContent><w:p><w:r><mc:AlternateContent><mc:Fallback><v:oval/></mc:Fallback></mc:AlternateContent></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r>"
        )),
        Some("oval")
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
        "<w:r {RUN_DECLARATIONS}><mc:AlternateContent><mc:Fallback><w:pict><v:shape><v:textbox><w:txbxContent><w:p/></w:txbxContent></v:textbox></v:shape></w:pict></mc:Fallback></mc:AlternateContent><mc:AlternateContent><mc:Fallback><w:pict/></mc:Fallback></mc:AlternateContent></w:r>"
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
            "<w:settings xmlns:w=\"{w}\"><w:compat><w:footnoteLayoutLikeWW8/><w:lineWrapLikeWord6/></w:compat></w:settings>"
        ))
        .iter()
        .map(|refusal| refusal.tag)
        .collect::<Vec<_>>(),
        ["footnoteLayoutLikeWW8", "lineWrapLikeWord6"]
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
    let text = format!(
        "<w:r {RUN_DECLARATIONS}><mc:AlternateContent><mc:Fallback><w:pict/></mc:Fallback></mc:AlternateContent><mc:AlternateContent><mc:Fallback><w:object><v:shape/></w:object></mc:Fallback></mc:AlternateContent></w:r>"
    );
    assert_eq!(
        refusals(element(&text)).first().map(|refusal| refusal.tag),
        refusal::<TextRun>(&text)
    );
}

#[test]
fn a_protection_with_no_password_takes_part_1_s_attributes() {
    let crypt = |provider: &str, sid: &str, spin: &str| {
        let mut protection = DocumentEditingRestrictions::default();
        protection.crypt_algorithm_class =
            Some(CryptographicAlgorithmClasses::from("hash".to_owned()));
        protection.crypt_algorithm_sid = Some(number(sid));
        protection.crypt_algorithm_type =
            Some(CryptographicAlgorithmTypes::from("typeAny".to_owned()));
        protection.crypt_provider_type =
            Some(CryptographicProviderTypes::from(provider.to_owned()));
        protection.crypt_spin_count = Some(number(spin));
        protection
    };
    let sha_512 = through_strict(&crypt("rsaAES", "14", "100000"));
    assert_eq!(sha_512.algorithm_name.as_deref(), Some("SHA-512"));
    assert_eq!(sha_512.spin_count, Some(number("100000")));
    assert_eq!(
        (sha_512.crypt_algorithm_sid, sha_512.crypt_spin_count),
        (None, None)
    );
    let mut read_only = crypt("rsaFull", "4", "50000");
    read_only.edit = Some(DocumentProtectionTypes::from("readOnly".to_owned()));
    read_only.enforcement = Some(OnOffValue::from("0".to_owned()));
    let sha_1 = through_strict(&read_only);
    assert_eq!(sha_1.algorithm_name.as_deref(), Some("SHA-1"));
    assert_eq!(sha_1.spin_count, Some(number("50000")));
    assert_eq!(sha_1.edit, read_only.edit);
    assert_eq!(flag(sha_1.enforcement.as_ref()), Some(false));
    let mut write_protection = WriteProtection::default();
    write_protection.crypt_algorithm_class =
        Some(CryptographicAlgorithmClasses::from("hash".to_owned()));
    write_protection.crypt_algorithm_sid = Some(number("12"));
    write_protection.crypt_algorithm_type =
        Some(CryptographicAlgorithmTypes::from("typeAny".to_owned()));
    write_protection.crypt_provider_type =
        Some(CryptographicProviderTypes::from("rsaAES".to_owned()));
    write_protection.crypt_spin_count = Some(number("100000"));
    let sha_256 = through_strict(&write_protection);
    assert_eq!(sha_256.algorithm_name.as_deref(), Some("SHA-256"));
    assert_eq!(sha_256.spin_count, Some(number("100000")));
}

#[test]
fn a_protection_part_1_has_no_slot_for_stays_refused() {
    let protection = |set: fn(&mut DocumentEditingRestrictions)| {
        let mut node = DocumentEditingRestrictions::default();
        set(&mut node);
        node
    };
    let cases = [
        (
            "SHA-1 with a hash",
            protection(|node| {
                node.crypt_algorithm_sid = Some(number("4"));
                node.hash = Some("9oN7nWkCAyEZib1RomSJTjmPpCY=".to_owned());
            }),
        ),
        (
            "SHA-1 with a salt",
            protection(|node| {
                node.crypt_algorithm_sid = Some(number("4"));
                node.salt = Some("ZUdHa+D8F/OAKP3I7ssUnQ==".to_owned());
            }),
        ),
        (
            "an algorithm with no name",
            protection(|node| {
                node.crypt_algorithm_sid = Some(number("5"));
            }),
        ),
        (
            "a provider",
            protection(|node| {
                node.crypt_provider = Some("Provider".to_owned());
            }),
        ),
        (
            "an algorithm extension",
            protection(|node| {
                node.alg_id_ext = Some(EightDigitHexadecimalValue::from("0000A5691D".to_owned()));
            }),
        ),
        (
            "a provider type extension",
            protection(|node| {
                node.crypt_provider_type_ext =
                    Some(EightDigitHexadecimalValue::from("00A5691D".to_owned()));
            }),
        ),
        (
            "a custom algorithm class",
            protection(|node| {
                node.crypt_algorithm_class =
                    Some(CryptographicAlgorithmClasses::from("custom".to_owned()));
            }),
        ),
        (
            "a custom algorithm type",
            protection(|node| {
                node.crypt_algorithm_type =
                    Some(CryptographicAlgorithmTypes::from("custom".to_owned()));
            }),
        ),
        (
            "a custom provider type",
            protection(|node| {
                node.crypt_provider_type =
                    Some(CryptographicProviderTypes::from("custom".to_owned()));
            }),
        ),
    ];
    for (case, protected) in cases {
        assert!(refused(&protected).is_some(), "{case}");
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
    let background = |content: VmlBackground| {
        let mut background = DocumentBackground::default();
        background.color = Some(ColorValue::from("ffffff".to_owned()));
        background.content = vec![VmlElementContent::DocumentBackground(content)];
        background
    };
    let shape = || {
        let mut shape = VmlBackground::default();
        shape.id = Some("_x0000_s1025".to_owned());
        shape
    };
    let mut colored = shape();
    colored.fillcolor = Some("yellow".to_owned());
    let mut fill = ShapeFillProperties::default();
    fill.r#type = Some(ShapeFillType::from("gradient".to_owned()));
    let mut gradient = shape();
    gradient.shape_fill_properties = Some(Box::new(fill));
    let mut unfilled = shape();
    unfilled.filled = Some(BooleanValue::from("f".to_owned()));
    for (case, content) in [
        ("a fill color", colored),
        ("a fill of its own", gradient),
        ("no fill", unfilled),
    ] {
        assert_eq!(refused(&background(content)), Some("background"), "{case}");
    }
}
