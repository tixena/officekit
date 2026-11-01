use alloc::collections::BTreeSet;

use officekit_markup::common_simple_types::MeasurementInTwentiethsOfAPoint;
use officekit_markup::spreadsheet::{
    BorderId, Cell, CellFormats, CellValue, ColumnInformation, ColumnWidthFormatting, FillId,
    FontId, Format, FormulaF, FormulaType, HexUnsignedInteger, NumberFormatId, Row,
    SheetInformation, SheetProperties, SheetTabColor, StyleSheet, Table, TableColumn,
    TotalsRowFunctionTypes, WebPublishingItem, WebPublishingItems, WebSourceType, Workbook,
    Worksheet, write as spreadsheet_write,
};
use officekit_markup::wordprocessing::{
    AssociatedHTMLDivID, AssociatedOutlineLevel, BlockContent, BookmarkStart,
    CompatibilitySettings, ContentCellContent, ContentRowContent, CustomCompatibilitySetting,
    DecimalNumberValue, Document, DocumentBody, DocumentFinalSectionProperties, DocumentFootnotes,
    DocumentSettings, Footer, FootnoteContent, FootnoteReference, FourDigitHexadecimalValue,
    GridColumnDefinition, Header, HeaderOrFooterType, HeaderReference, Hyperlink, Paragraph,
    ParagraphBorderBelowIdenticalParagraphsTblWidth, ParagraphContent, ParagraphProperties,
    RadixPointForFieldCodeEvaluation, ReferencedParagraphStyle, SectionPropertiesContent,
    SuggestedFiltering, Table as WordTable, TableCell, TableCellMarginExceptions, TableRow,
    TableWidthUnits, Text, TextRun, TextRunContent, write,
};
use officekit_xml::element::{Document as XmlDocument, Element};
use officekit_xml::name::OfficeVersion;
use officekit_xml::packaging::package::{DECLARATION, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::write::{Scope, WriteOptions};
use regex::Regex;

use super::constraint::Constraint;
use super::registry::registrations;
use crate::validation::error::{ErrorCode, Finding};
use crate::validation::generated::{SCHEMA_TYPES, SEMANTIC_CONSTRAINTS, type_id};
use crate::validation::validator::Validator;

const FOOTER: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";
const FOOTNOTES: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml";
const HEADER: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const NAMESPACES: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
     xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
     xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" \
     xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"w14\"";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const SETTINGS: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";
const SHEET: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
const SPREADSHEET_STYLES: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";
const TABLE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml";
const WORKBOOK: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";

/// One part of a package: its name, content type, the part it is related
/// from (`None`: the package), the last segment of its relationship type, and
/// its XML.
type Part<'part> = (
    &'part str,
    &'part str,
    Option<&'part str>,
    &'part str,
    String,
);

/// The path of `steps` under `/w:document[1]/w:body[1]` in the main part.
fn in_body(steps: &str) -> String {
    format!("/word/document.xml /w:document[1]/w:body[1]{steps}")
}

/// A package of `parts`, each related from its source in order.
fn package(parts: &[Part<'_>]) -> Package {
    let mut package = Package::new();
    for (name, content_type, source, relationship, xml) in parts {
        let part = PartName::new(name).unwrap();
        package
            .set_part_bytes(&part, content_type, xml.as_bytes())
            .unwrap();
        let from = source.map(|found| PartName::new(found).unwrap());
        package
            .relate(
                from.as_ref(),
                &part,
                &format!("{RELATIONSHIPS}{relationship}"),
            )
            .unwrap();
    }
    package
}

/// Each semantic error at `target`: its code, and its part and path.
fn semantic(package: &Package, target: OfficeVersion) -> Vec<(ErrorCode, String)> {
    Validator::new(target)
        .with_max_errors(0)
        .validate(package)
        .into_iter()
        .filter(|error| {
            error.code.kind() == Finding::Semantic
                || matches!(
                    error.code,
                    ErrorCode::AttributeCannotBeOmitted
                        | ErrorCode::AttributeValueOutsideConstraint
                )
        })
        .map(|error| {
            let part = error
                .part
                .as_ref()
                .map_or_else(String::new, |name| name.as_str().to_owned());
            (error.code, format!("{part} {}", error.path_text()))
        })
        .collect()
}

/// A Word document whose body holds `body`, its main part related to each of
/// `parts`: name, content type, the last segment of the relationship type,
/// and XML.
fn word_xml(body: &str, parts: &[(&str, &str, &str, String)]) -> Package {
    let mut all: Vec<Part<'_>> = vec![(
        "/word/document.xml",
        MAIN,
        None,
        "officeDocument",
        format!("<w:document {NAMESPACES}><w:body>{body}</w:body></w:document>"),
    )];
    for (name, content_type, relationship, xml) in parts {
        all.push((
            name,
            content_type,
            Some("/word/document.xml"),
            relationship,
            xml.clone(),
        ));
    }
    package(&all)
}

/// The part text of `root`, after the declaration Office writes.
fn xml_of(root: Element) -> String {
    String::from_utf8(XmlDocument::new(DECLARATION, root).to_xml(WriteOptions::default())).unwrap()
}

/// A Word document whose main part is `document`, related to each of `parts`: name, content type,
/// the last segment of the relationship type, and XML.
fn word_document(document: &Document, parts: &[(&str, &str, &str, String)]) -> Package {
    let mut all: Vec<Part<'_>> = vec![(
        "/word/document.xml",
        MAIN,
        None,
        "officeDocument",
        xml_of(write::document(document, &mut Scope::default())),
    )];
    for (name, content_type, relationship, xml) in parts {
        all.push((
            name,
            content_type,
            Some("/word/document.xml"),
            relationship,
            xml.clone(),
        ));
    }
    package(&all)
}

/// A Word document whose body holds `blocks`, its main part related to each of `parts`.
fn word(blocks: Vec<BlockContent>, parts: &[(&str, &str, &str, String)]) -> Package {
    let mut body = DocumentBody::default();
    body.content = blocks;
    let mut document = Document::default();
    document.body = Some(Box::new(body));
    word_document(&document, parts)
}

/// A paragraph with `properties` and no content.
fn paragraph_with(properties: ParagraphProperties) -> BlockContent {
    let mut paragraph = Paragraph::default();
    paragraph.properties = Some(Box::new(properties));
    BlockContent::Paragraph(paragraph)
}

/// A paragraph holding `content`.
fn paragraph_of(content: Vec<ParagraphContent>) -> BlockContent {
    let mut paragraph = Paragraph::default();
    paragraph.content = content;
    BlockContent::Paragraph(paragraph)
}

/// A run holding `content`.
fn run_of(content: TextRunContent) -> ParagraphContent {
    let mut run = TextRun::default();
    run.content = vec![content];
    ParagraphContent::TextRun(run)
}

/// A Word document of an empty paragraph whose settings part holds `settings`.
fn with_settings(settings: &DocumentSettings) -> Package {
    word(
        vec![BlockContent::Paragraph(Paragraph::default())],
        &[(
            "/word/settings.xml",
            SETTINGS,
            "settings",
            xml_of(write::document_settings(settings, &mut Scope::default())),
        )],
    )
}

fn number(value: &str) -> DecimalNumberValue {
    DecimalNumberValue::from(value.to_owned())
}

/// A spreadsheet whose one worksheet is `sheet`, with a styles part of one cell format, and a
/// table part of `column` when given.
fn spreadsheet(sheet: &Worksheet, column: Option<TableColumn>) -> Package {
    let mut information = SheetInformation::default();
    information.id = Some("rId1".to_owned());
    information.name = Some("S".to_owned());
    information.sheet_id = Some("1".to_owned());
    let mut workbook = Workbook::default();
    workbook.sheets.sheet_information = vec![information];
    let mut format = Format::default();
    format.border_id = Some(BorderId::from("0".to_owned()));
    format.fill_id = Some(FillId::from("0".to_owned()));
    format.font_id = Some(FontId::from("0".to_owned()));
    format.num_fmt_id = Some(NumberFormatId::from("0".to_owned()));
    let mut formats = CellFormats::default();
    formats.count = Some("1".to_owned());
    formats.format = vec![format];
    let mut styles = StyleSheet::default();
    styles.cell_formats = Some(Box::new(formats));
    let mut parts: Vec<Part<'_>> = vec![
        (
            "/xl/workbook.xml",
            WORKBOOK,
            None,
            "officeDocument",
            xml_of(spreadsheet_write::workbook(
                &workbook,
                &mut Scope::default(),
            )),
        ),
        (
            "/xl/worksheets/sheet1.xml",
            SHEET,
            Some("/xl/workbook.xml"),
            "worksheet",
            xml_of(spreadsheet_write::worksheet(sheet, &mut Scope::default())),
        ),
        (
            "/xl/styles.xml",
            SPREADSHEET_STYLES,
            Some("/xl/workbook.xml"),
            "styles",
            xml_of(spreadsheet_write::style_sheet(
                &styles,
                &mut Scope::default(),
            )),
        ),
    ];
    if let Some(table_column) = column {
        let mut table = Table::default();
        table.columns.count = Some("1".to_owned());
        table.columns.table_column = vec![table_column];
        table.display_name = Some("T".to_owned());
        table.id = Some("1".to_owned());
        table.name = Some("T".to_owned());
        table.r#ref = Some("A1:A2".to_owned());
        parts.push((
            "/xl/tables/table1.xml",
            TABLE,
            Some("/xl/worksheets/sheet1.xml"),
            "table",
            xml_of(spreadsheet_write::table(&table, &mut Scope::default())),
        ));
    }
    package(&parts)
}

/// A worksheet whose one row, row 1, holds `cells`.
fn sheet_of_cells(cells: Vec<Cell>) -> Worksheet {
    let mut row = Row::default();
    row.cell = cells;
    row.r = Some("1".to_owned());
    let mut sheet = Worksheet::default();
    sheet.sheet_data.row = vec![row];
    sheet
}

#[test]
fn a_value_one_attribute_must_not_take_is_outside_the_constraint() {
    let in_div = |value: &str| {
        let mut div_id = AssociatedHTMLDivID::default();
        div_id.val = Some(number(value));
        let mut properties = ParagraphProperties::default();
        properties.associated_html_div_id = Some(Box::new(div_id));
        word(vec![paragraph_with(properties)], &[])
    };
    assert_eq!(
        semantic(&in_div("0"), OfficeVersion::Microsoft365),
        [(
            ErrorCode::AttributeValueOutsideConstraint,
            in_body("/w:p[1]/w:pPr[1]/w:divId[1]")
        )]
    );
    assert_eq!(semantic(&in_div("7"), OfficeVersion::Microsoft365), []);
}

#[test]
fn a_hexadecimal_value_is_compared_by_its_number() {
    let mut filter = SuggestedFiltering::default();
    filter.val = Some(FourDigitHexadecimalValue::from("0040".to_owned()));
    let mut settings = DocumentSettings::default();
    settings.suggested_filtering = Some(Box::new(filter));
    assert_eq!(
        semantic(&with_settings(&settings), OfficeVersion::Office2016),
        [(
            ErrorCode::AttributeValueOutsideConstraint,
            "/word/settings.xml /w:settings[1]/w:stylePaneFormatFilter[1]".to_owned()
        )]
    );
}

#[test]
fn a_value_out_of_range_is_outside_the_constraint() {
    let mut level = AssociatedOutlineLevel::default();
    level.val = Some(number("10"));
    let mut properties = ParagraphProperties::default();
    properties.associated_outline_level = Some(Box::new(level));
    let mut identified = Paragraph::default();
    identified.para_id = Some("80000001".to_owned());
    identified.text_id = Some("7FFFFFFF".to_owned());
    let package = word(
        vec![
            paragraph_with(properties),
            BlockContent::Paragraph(identified),
        ],
        &[],
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2010),
        [
            (
                ErrorCode::AttributeValueOutsideConstraint,
                in_body("/w:p[1]/w:pPr[1]/w:outlineLvl[1]")
            ),
            (
                ErrorCode::AttributeValueOutsideConstraint,
                in_body("/w:p[2]")
            ),
        ]
    );
}

#[test]
fn a_value_too_long_is_outside_the_constraint() {
    let styled = |length: usize| {
        let mut style = ReferencedParagraphStyle::default();
        style.val = Some("s".repeat(length));
        let mut properties = ParagraphProperties::default();
        properties.referenced_paragraph_style = Some(Box::new(style));
        word(vec![paragraph_with(properties)], &[])
    };
    assert_eq!(semantic(&styled(253), OfficeVersion::Office2013), []);
    assert_eq!(
        semantic(&styled(254), OfficeVersion::Office2013),
        [(
            ErrorCode::AttributeValueOutsideConstraint,
            in_body("/w:p[1]/w:pPr[1]/w:pStyle[1]")
        )]
    );
}

#[test]
fn a_value_off_its_pattern_is_outside_the_constraint() {
    let mut symbol = RadixPointForFieldCodeEvaluation::default();
    symbol.val = Some("ab".to_owned());
    let mut settings = DocumentSettings::default();
    settings.radix_point_for_field_code_evaluation = Some(Box::new(symbol));
    assert_eq!(
        semantic(&with_settings(&settings), OfficeVersion::Office2019),
        [(
            ErrorCode::AttributeValueOutsideConstraint,
            "/word/settings.xml /w:settings[1]/w:decimalSymbol[1]".to_owned()
        )]
    );
}

#[test]
fn a_bookmark_id_held_twice_is_reported_once_on_the_last() {
    let bookmark = |id: &str, name: &str| {
        let mut start = BookmarkStart::default();
        start.id = Some(number(id));
        start.name = Some(name.to_owned());
        paragraph_of(vec![ParagraphContent::BookmarkStart(start)])
    };
    let package = word(
        vec![bookmark("1", "a"), bookmark("1", "b"), bookmark("2", "c")],
        &[],
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2013),
        [(
            ErrorCode::DuplicateAttributeValue,
            in_body("/w:p[2]/w:bookmarkStart[1]")
        )]
    );
}

#[test]
fn a_footnote_reference_to_a_missing_footnote_is_reported() {
    let footnote = |id: &str| {
        let mut footnote = FootnoteContent::default();
        footnote.content = vec![BlockContent::Paragraph(Paragraph::default())];
        footnote.id = Some(number(id));
        footnote
    };
    let mut footnotes = DocumentFootnotes::default();
    footnotes.footnote_content = vec![footnote("-1"), footnote("0")];
    let reference = |id: &str| {
        let mut reference = FootnoteReference::default();
        reference.id = Some(number(id));
        run_of(TextRunContent::FootnoteReference(reference))
    };
    let package = word(
        vec![paragraph_of(vec![reference("9"), reference("0")])],
        &[(
            "/word/footnotes.xml",
            FOOTNOTES,
            "footnotes",
            xml_of(write::document_footnotes(&footnotes, &mut Scope::default())),
        )],
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Microsoft365),
        [(
            ErrorCode::ReferencedElementNotFound,
            in_body("/w:p[1]/w:r[1]/w:footnoteReference[1]")
        )]
    );
}

#[test]
fn an_id_naming_no_relationship_is_reported() {
    let mut text = Text::default();
    text.value = "x".to_owned();
    let mut hyperlink = Hyperlink::default();
    hyperlink.content = vec![run_of(TextRunContent::Text(text))];
    hyperlink.id = Some("rId99".to_owned());
    let package = word(
        vec![paragraph_of(vec![ParagraphContent::Hyperlink(hyperlink)])],
        &[],
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2016),
        [(
            ErrorCode::RelationshipNotFound,
            in_body("/w:p[1]/w:hyperlink[1]")
        )]
    );
}

#[test]
fn a_relationship_whose_target_names_no_part_hides_no_other_id() {
    let body = "<w:p><w:hyperlink r:id=\"rId1\"><w:r><w:t>x</w:t></w:r></w:hyperlink></w:p>";
    let mut written = word_xml(body, &[]);
    let main = PartName::new("/word/document.xml").unwrap();
    written
        .set_part_bytes(
            &PartName::new(&main.relationships_part()).unwrap(),
            "application/vnd.openxmlformats-package.relationships+xml",
            format!(
                "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 <Relationship Id=\"rId1\" Type=\"{RELATIONSHIPS}hyperlink\" Target=\"https://example.org/\" TargetMode=\"External\"/>\
                 <Relationship Id=\"rId2\" Type=\"{RELATIONSHIPS}image\" Target=\"media/\"/>\
                 </Relationships>"
            )
            .as_bytes(),
        )
        .unwrap();
    let package = Package::from_bytes(written.to_bytes().unwrap()).unwrap();
    assert_eq!(semantic(&package, OfficeVersion::Microsoft365), []);
}

#[test]
fn a_header_reference_naming_a_footer_relationship_is_reported() {
    let mut footer = Footer::default();
    footer.content = vec![BlockContent::Paragraph(Paragraph::default())];
    let mut header = Header::default();
    header.content = vec![BlockContent::Paragraph(Paragraph::default())];
    let reference = |kind: &str, id: &str| {
        let mut reference = HeaderReference::default();
        reference.id = Some(id.to_owned());
        reference.r#type = Some(HeaderOrFooterType::from(kind.to_owned()));
        SectionPropertiesContent::HeaderReference(reference)
    };
    let mut section = DocumentFinalSectionProperties::default();
    section.content = vec![reference("default", "rId1"), reference("first", "rId2")];
    let mut body = DocumentBody::default();
    body.document_final_section_properties = Some(Box::new(section));
    let mut document = Document::default();
    document.body = Some(Box::new(body));
    let parts = [
        (
            "/word/footer1.xml",
            FOOTER,
            "footer",
            xml_of(write::footer(&footer, &mut Scope::default())),
        ),
        (
            "/word/header1.xml",
            HEADER,
            "header",
            xml_of(write::header(&header, &mut Scope::default())),
        ),
    ];
    assert_eq!(
        semantic(&word_document(&document, &parts), OfficeVersion::Office2021),
        [(
            ErrorCode::WrongRelationshipType,
            in_body("/w:sectPr[1]/w:headerReference[1]")
        )]
    );
}

#[test]
fn a_required_attribute_left_out_cannot_be_omitted() {
    let mut bottom = ParagraphBorderBelowIdenticalParagraphsTblWidth::default();
    bottom.r#type = Some(TableWidthUnits::from("dxa".to_owned()));
    let mut margins = TableCellMarginExceptions::default();
    margins.paragraph_border_below_identical_paragraphs_tbl_width = Some(Box::new(bottom));
    let mut column = GridColumnDefinition::default();
    column.w = Some(MeasurementInTwentiethsOfAPoint::from("10".to_owned()));
    let mut cell = TableCell::default();
    cell.content = vec![BlockContent::Paragraph(Paragraph::default())];
    let mut row = TableRow::default();
    row.content = vec![ContentCellContent::TableCell(cell)];
    let mut table = WordTable::default();
    table.content_row_content = vec![ContentRowContent::TableRow(row)];
    table.grid.column_definition = vec![column];
    table.properties.table_cell_margin_exceptions = Some(Box::new(margins));
    assert_eq!(
        semantic(
            &word(vec![BlockContent::Table(table)], &[]),
            OfficeVersion::Office2010
        ),
        [(
            ErrorCode::AttributeCannotBeOmitted,
            in_body("/w:tbl[1]/w:tblPr[1]/w:tblCellMar[1]/w:bottom[1]")
        )]
    );
}

#[test]
fn a_value_the_other_attribute_does_not_allow_conflicts_with_it() {
    let mut setting = CustomCompatibilitySetting::default();
    setting.name = Some("compatibilityMode".to_owned());
    setting.uri = Some("http://schemas.microsoft.com/office/word".to_owned());
    setting.val = Some("13".to_owned());
    let mut compatibility = CompatibilitySettings::default();
    compatibility.custom_compatibility_setting = vec![setting];
    let mut settings = DocumentSettings::default();
    settings.compatibility_settings = Some(Box::new(compatibility));
    assert_eq!(
        semantic(&with_settings(&settings), OfficeVersion::Office2013),
        [(
            ErrorCode::AttributeValueConflictsWithAnother,
            "/word/settings.xml /w:settings[1]/w:compat[1]/w:compatSetting[1]".to_owned()
        )]
    );
}

#[test]
fn two_mutually_exclusive_attributes_are_reported() {
    let mut color = SheetTabColor::default();
    color.rgb = Some(HexUnsignedInteger::from("FF0000FF".to_owned()));
    color.theme = Some("1".to_owned());
    let mut properties = SheetProperties::default();
    properties.sheet_tab_color = Some(Box::new(color));
    let mut sheet = Worksheet::default();
    sheet.sheet_properties = Some(Box::new(properties));
    assert_eq!(
        semantic(&spreadsheet(&sheet, None), OfficeVersion::Microsoft365),
        [(
            ErrorCode::MutuallyExclusiveAttributes,
            "/xl/worksheets/sheet1.xml /x:worksheet[1]/x:sheetPr[1]/x:tabColor[1]".to_owned()
        )]
    );
}

#[test]
fn a_minimum_above_its_maximum_exceeds_it() {
    let mut column = ColumnWidthFormatting::default();
    column.max = Some("2".to_owned());
    column.min = Some("5".to_owned());
    column.width = Some("9".to_owned());
    let mut columns = ColumnInformation::default();
    columns.column_width_formatting = vec![column];
    let mut sheet = Worksheet::default();
    sheet.column_information = vec![columns];
    assert_eq!(
        semantic(&spreadsheet(&sheet, None), OfficeVersion::Office2016),
        [(
            ErrorCode::AttributeValueExceedsAnother,
            "/xl/worksheets/sheet1.xml /x:worksheet[1]/x:cols[1]/x:col[1]".to_owned()
        )]
    );
}

#[test]
fn a_shared_formula_requires_its_index() {
    let mut formula = FormulaF::default();
    formula.r#ref = Some("B1:B2".to_owned());
    formula.t = Some(FormulaType::from("shared".to_owned()));
    formula.value = "A1".to_owned();
    let mut cell = Cell::default();
    cell.formula_f = Some(Box::new(formula));
    cell.r = Some("B1".to_owned());
    assert_eq!(
        semantic(
            &spreadsheet(&sheet_of_cells(vec![cell]), None),
            OfficeVersion::Office2019
        ),
        [(
            ErrorCode::AttributeRequiredForValue,
            "/xl/worksheets/sheet1.xml /x:worksheet[1]/x:sheetData[1]/x:row[1]/x:c[1]/x:f[1]"
                .to_owned()
        )]
    );
}

#[test]
fn a_source_range_must_be_absent_unless_the_source_is_a_range() {
    let mut item = WebPublishingItem::default();
    item.destination_file = Some("f.htm".to_owned());
    item.div_id = Some("d".to_owned());
    item.id = Some("1".to_owned());
    item.source_ref = Some("A1".to_owned());
    item.source_type = Some(WebSourceType::from("sheet".to_owned()));
    let mut items = WebPublishingItems::default();
    items.count = Some("1".to_owned());
    items.web_publishing_item = vec![item];
    let mut sheet = Worksheet::default();
    sheet.web_publishing_items = Some(Box::new(items));
    assert_eq!(
        semantic(&spreadsheet(&sheet, None), OfficeVersion::Office2021),
        [(
            ErrorCode::AttributeMustBeAbsentUnlessValue,
            "/xl/worksheets/sheet1.xml /x:worksheet[1]/x:webPublishItems[1]/x:webPublishItem[1]"
                .to_owned()
        )]
    );
}

#[test]
fn a_totals_label_must_be_absent_for_a_custom_function() {
    let mut column = TableColumn::default();
    column.id = Some("1".to_owned());
    column.name = Some("C".to_owned());
    column.totals_row_function = Some(TotalsRowFunctionTypes::from("custom".to_owned()));
    column.totals_row_label = Some("x".to_owned());
    assert_eq!(
        semantic(
            &spreadsheet(&Worksheet::default(), Some(column)),
            OfficeVersion::Office2010
        ),
        [(
            ErrorCode::AttributeMustBeAbsentForValue,
            "/xl/tables/table1.xml /x:table[1]/x:tableColumns[1]/x:tableColumn[1]".to_owned()
        )]
    );
}

#[test]
fn a_style_index_past_the_cell_formats_is_reported() {
    let styled = |reference: &str, style: &str| {
        let mut value = CellValue::default();
        value.value = "1".to_owned();
        let mut cell = Cell::default();
        cell.r = Some(reference.to_owned());
        cell.s = Some(style.to_owned());
        cell.value = Some(Box::new(value));
        cell
    };
    let sheet = sheet_of_cells(vec![styled("A1", "5"), styled("B1", "0")]);
    assert_eq!(
        semantic(&spreadsheet(&sheet, None), OfficeVersion::Microsoft365),
        [(
            ErrorCode::IndexedElementNotFound,
            "/xl/worksheets/sheet1.xml /x:worksheet[1]/x:sheetData[1]/x:row[1]/x:c[1]".to_owned()
        )]
    );
}

#[test]
fn the_table_holds_every_registration_sorted_by_type() {
    assert_eq!(SEMANTIC_CONSTRAINTS.len(), 988);
    let types: BTreeSet<u16> = SEMANTIC_CONSTRAINTS.iter().map(|row| row.type_id).collect();
    assert_eq!(types.len(), 466);
    assert!(
        SEMANTIC_CONSTRAINTS
            .windows(2)
            .all(|pair| matches!(pair, [left, right] if left.type_id <= right.type_id))
    );
    for row in SEMANTIC_CONSTRAINTS {
        assert!(SCHEMA_TYPES.get(usize::from(row.type_id)).is_some());
        if let Constraint::ValuePattern {
            attribute: _attribute,
            pattern,
        } = row.constraint
        {
            assert!(Regex::new(pattern).is_ok(), "{pattern}");
        }
    }
    let bookmark = type_id("w:CT_Bookmark/w:bookmarkStart").unwrap();
    assert_eq!(registrations(bookmark).len(), 1);
}
