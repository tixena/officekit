use officekit::docx::Docx;
use officekit::parts::font_table;
use officekit::parts::kinds::WordprocessingKind;
use officekit_markup::common_simple_types::OnOffValue;
use officekit_markup::wordprocessing::{
    BlockContent, BorderStyles, CharacterSetSupportedByFont, Document, DocumentBody,
    DocumentFinalSectionProperties, ExpandedCompressedText, FontTableRootElement,
    FourDigitHexadecimalValue, HorizontalAlignmentType, LeftParagraphBorder,
    MeasurementOrPercentageValue, PageSize, Paragraph, ParagraphAlignment, ParagraphContent,
    ParagraphIndentation, ParagraphProperties, PreferredTableWidth, PropertiesForASingleFont,
    RunProperties, RunPropertiesContent, StTextScale, Table, TableBorders,
    TableIndentFromLeadingMargin, TableStyleConditionalFormattingSettingsException,
    TableWidthUnits, Text, TextRun, TextRunContent, TwoDigitHexadecimalValue,
};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
use officekit_xml::write::WriteOptions;

/// One value of each kind of row a Word file holds, as Word writes it in Transitional.
const VALUES: [&str; 9] = [
    "<w:jc w:val=\"left\"/>",
    "<w:w w:val=\"89\"/>",
    "<w:tblW w:w=\"4680\" w:type=\"dxa\"/>",
    "<w:tblInd w:w=\"4804\" w:type=\"pct\"/>",
    "<w:tblBorders><w:left w:val=\"single\"/></w:tblBorders>",
    "w:val=\"04A0\"",
    "<w:ind w:left=\"720\" w:hanging=\"360\"/>",
    "<w:pgSz w:w=\"12240\" w:h=\"15840\"/>",
    "<w:charset w:val=\"00\"/>",
];

/// The text of the part `name` of the file `bytes`.
fn part_text(bytes: Vec<u8>, name: &str) -> String {
    let package = Package::from_bytes(bytes).unwrap();
    String::from_utf8(package.part_bytes(&PartName::new(name).unwrap()).unwrap()).unwrap()
}

fn on_off(value: &str) -> OnOffValue {
    OnOffValue::from(value.to_owned())
}

/// A table of no row: 4680 twips wide, indented 4804 in percent, a single left border, and
/// the look 04A0.
fn table() -> BlockContent {
    let mut preferred = PreferredTableWidth::default();
    preferred.w = Some(MeasurementOrPercentageValue::from("4680".to_owned()));
    preferred.r#type = Some(TableWidthUnits::from("dxa".to_owned()));
    let mut indent = TableIndentFromLeadingMargin::default();
    indent.w = Some(MeasurementOrPercentageValue::from("4804".to_owned()));
    indent.r#type = Some(TableWidthUnits::from("pct".to_owned()));
    let mut left = LeftParagraphBorder::default();
    left.val = Some(BorderStyles::from("single".to_owned()));
    let mut borders = TableBorders::default();
    borders.left_paragraph_border = Some(Box::new(left));
    let mut look = TableStyleConditionalFormattingSettingsException::default();
    look.val = Some(FourDigitHexadecimalValue::from("04A0".to_owned()));
    look.first_row = Some(on_off("1"));
    look.last_row = Some(on_off("0"));
    look.first_column = Some(on_off("1"));
    look.last_column = Some(on_off("0"));
    look.no_h_band = Some(on_off("0"));
    look.no_v_band = Some(on_off("1"));
    let mut table = Table::default();
    table.properties.preferred_table_width = Some(Box::new(preferred));
    table.properties.table_indent_from_leading_margin = Some(Box::new(indent));
    table.properties.table_borders = Some(Box::new(borders));
    table
        .properties
        .table_style_conditional_formatting_settings_exception = Some(Box::new(look));
    BlockContent::Table(table)
}

/// A left-justified paragraph indented 720 and hanging 360, of the run "x" scaled to 89%.
fn paragraph() -> BlockContent {
    let mut indentation = ParagraphIndentation::default();
    indentation.left = Some("720".to_owned().into());
    indentation.hanging = Some("360".to_owned().into());
    let mut justification = ParagraphAlignment::default();
    justification.val = Some(HorizontalAlignmentType::from("left".to_owned()));
    let mut properties = ParagraphProperties::default();
    properties.paragraph_indentation = Some(Box::new(indentation));
    properties.paragraph_alignment = Some(Box::new(justification));
    let mut scale = ExpandedCompressedText::default();
    scale.val = Some(StTextScale::from("89".to_owned()));
    let mut run_properties = RunProperties::default();
    run_properties.content = vec![RunPropertiesContent::ExpandedCompressedText(scale)];
    let mut characters = Text::default();
    characters.value = "x".to_owned();
    let mut run = TextRun::default();
    run.properties = Some(Box::new(run_properties));
    run.content = vec![TextRunContent::Text(characters)];
    let mut paragraph = Paragraph::default();
    paragraph.properties = Some(Box::new(properties));
    paragraph.content = vec![ParagraphContent::TextRun(run)];
    BlockContent::Paragraph(paragraph)
}

/// A document of `table` and `paragraph`, on a page 12240 by 15840.
fn document() -> Document {
    let mut size = PageSize::default();
    size.w = Some("12240".to_owned().into());
    size.h = Some("15840".to_owned().into());
    let mut section = DocumentFinalSectionProperties::default();
    section.page_size = Some(Box::new(size));
    let mut body = DocumentBody::default();
    body.content = vec![table(), paragraph()];
    body.document_final_section_properties = Some(Box::new(section));
    let mut document = Document::default();
    document.body = Some(Box::new(body));
    document
}

/// A font table of Calibri, whose character set is 00.
fn fonts() -> FontTableRootElement {
    let mut charset = CharacterSetSupportedByFont::default();
    charset.val = Some(TwoDigitHexadecimalValue::from("00".to_owned()));
    let mut font = PropertiesForASingleFont::default();
    font.character_set_supported_by_font = Some(Box::new(charset));
    font.name = Some("Calibri".to_owned());
    let mut fonts = FontTableRootElement::default();
    fonts.properties_for_a_single_font = vec![font];
    fonts
}

#[test]
fn a_save_to_strict_and_back_writes_each_value_as_the_first_transitional_file_held_it() {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    docx.set_main_document(&document()).unwrap();
    let main = docx.main_document_part().clone();
    let part = PartName::new("/word/fontTable1.xml").unwrap();
    let package = docx.package_mut();
    font_table::store_transitional(package, &part, &fonts(), WriteOptions::default()).unwrap();
    package
        .relate(
            Some(&main),
            &part,
            RelationshipType::FONT_TABLE.transitional,
        )
        .unwrap();
    let strict = Docx::from_bytes(docx.to_bytes_strict().unwrap(), OpenOptions::default()).unwrap();
    assert_eq!(strict.conformance_class(), ConformanceClass::Strict);
    let (bytes, warnings) = strict.to_bytes_transitional().unwrap();
    assert_eq!(warnings, []);
    let written = format!(
        "{}{}",
        part_text(bytes.clone(), "/word/document.xml"),
        part_text(bytes, "/word/fontTable1.xml")
    );
    for value in VALUES {
        assert!(written.contains(value), "{value} in {written}");
    }
}
