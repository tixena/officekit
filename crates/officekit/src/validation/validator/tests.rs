use core::iter;

use officekit_markup::common_simple_types::{
    BooleanValue, OnOffValue, UnsignedDecimalNumberValue, XMLName,
};
use officekit_markup::vml::DocumentBackground as VmlBackground;
use officekit_markup::wordprocessing::{
    AbstractNumberingDefinition, AssociatedHTMLDivID, AutomaticallyMergeUserFormatting,
    BlockContent, BlockLevelStructuredDocumentTag, BlockLevelStructuredDocumentTagContent,
    CellLevelStructuredDocumentTag, CellLevelStructuredDocumentTagContent, ColorValue,
    CompatibilitySettings, ComplexFieldCharacter, ComplexFieldCharacterContent,
    ComplexFieldCharacterType, ConditionalFormattingBitmask, ConditionalTableStyleFormattingTypes,
    ContentBlockContent, ContentCellContent, ContentRowContent, CustomCompatibilitySetting,
    CustomXMLAttribute, DecimalNumberValue, DefaultParagraphProperties,
    DoNotDisplayFramesetSplitters, Document, DocumentBackground, DocumentBody,
    DocumentDefaultParagraphAndRunProperties, DocumentSettings, DontWrapCellContent,
    EMailMessageCompositionStyle, EMailMessageReplyStyle, EMailMessageTextStyle, EmbeddedObject,
    EmbeddedObjectContent, EmbeddedVideo, FitTextWithinCell, FloatingTableAllowsOverlap,
    FloatingTablePositioning, FormFieldEnabled, FormFieldName, FormFieldNameValue,
    FormFieldProperties, FormFieldPropertiesContent, FourDigitHexadecimalValue,
    FrameCannotBeResized, FrameLongDescription, FrameOrFramesetTitle, FrameSize,
    FramesetDefinitionContent, FramesetSplitterBorderStyle, FramesetSplitterProperties,
    HiddenTableRowMarker, HideStyleFromMainUserInterface, HorizontalAlignmentType,
    IgnoreEndOfCellMarker, InlineLevelSmartTag, InlineLevelStructuredDocumentTag,
    InlineLevelStructuredDocumentTagContent, Justification, LeftAndRightMarginForFrame,
    MaintainLinkToExistingFile, MeasurementInPixels, MeasurementOrPercentageValue,
    NestedFramesetDefinition, NumberOfColumnsInColumnBand, NumberOfRowsInRowBand,
    NumberingDefinitions, NumberingLevelOverrideDefinition, Paragraph,
    ParagraphConditionalFormatting, ParagraphContent, ParagraphProperties,
    ParagraphPropertiesPPrGeneral, PlainTextStructuredDocumentTag, PreferredTableWidth,
    PreviousTableProperties, PrimaryStyle, PrimaryStyleName, ProgrammaticTag,
    ReferencedParagraphStyle, ReferencedTableStyle, RemoveSemiHiddenProperty, RepeatTableRow,
    RevisionInformationForParagraphProperties, RowLevelStructuredDocumentTag,
    RowLevelStructuredDocumentTagContent, SignedMeasurementInTwentiethsOfPoint,
    SingleFrameProperties, SmartTagProperties, StructuredDocumentTagLabel,
    StructuredDocumentTagNavigationOrderIndex, StructuredDocumentTagProperties,
    StructuredDocumentTagPropertiesContent, StyleCannotBeApplied,
    StyleConditionalTableFormattingProperties, StyleDefinition, StyleDefinitions, StyleTypes,
    SupplementarySmartTagInformation, Table, TableAlignmentException, TableAlignmentType,
    TableCaption, TableCell, TableCellProperties, TableDescription, TableLayout, TableLayoutType,
    TableOverlapSetting, TableProperties, TableRow, TableRowCannotBreakAcrossPages,
    TableRowProperties, TableRowPropertiesContent,
    TableStyleConditionalFormattingSettingsException, TableWidthUnits, Text, TextRun,
    TextRunContent, VisuallyRightToLeftTable, VmlElementContent, WebPageSettings, write,
};
use officekit_xml::element::{Document as XmlDocument, Element};
use officekit_xml::name::OfficeVersion;
use officekit_xml::packaging::package::{DECLARATION, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
use officekit_xml::write::{Scope, WriteOptions};

use super::{Validator, document_parts, reachable_parts, rule};
use crate::validation::error::{ErrorCode, ValidationError};

/// A node of the on and off element type `$node` whose `w:val` is `$value`.
macro_rules! on_off {
    ($node:ty, $value:expr) => {{
        let mut node = <$node>::default();
        node.val = Some(OnOffValue::from($value.to_owned()));
        node
    }};
}

const CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const CHART_CONTENT: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
const COMMENTS_EXTENDED: &str =
    "http://schemas.microsoft.com/office/2011/relationships/commentsExtended";
const COMMENTS_EXTENDED_CONTENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml";
const NA_AS_BLANK_CHART: &[u8] = br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:layout/></c:plotArea><c:extLst><c:ext xmlns:c16r3="http://schemas.microsoft.com/office/drawing/2017/03/chart" uri="{56B9EC1D-385E-4148-901F-78D8002777C0}"><c16r3:dataDisplayOptions16><c16r3:dispNaAsBlank val="1"/></c16r3:dataDisplayOptions16></c:ext></c:extLst></c:chart></c:chartSpace>"#;
const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const SMART_TAG: &str = r#"<w:smartTag w:uri="urn:schemas-microsoft-com:office:smarttags" w:element="PersonName"><w:smartTagPr><w:attr w:name="ProductID" w:val="LA VIVIENDA"/></w:smartTagPr><w:r><w:t>LA VIVIENDA</w:t></w:r></w:smartTag>"#;
const STRICT_OFFICE_DOCUMENT: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";
const STYLES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const STYLES_CONTENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
const NUMBERING: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";
const NUMBERING_CONTENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml";
const SETTINGS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
const SETTINGS_CONTENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";
const WEB_SETTINGS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/webSettings";
const WEB_SETTINGS_CONTENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.webSettings+xml";
const ON_OFF_ELEMENTS: [&str; 18] = [
    "cantSplit",
    "tblHeader",
    "hidden",
    "noWrap",
    "tcFitText",
    "hideMark",
    "noResizeAllowed",
    "linkedToFile",
    "noBorder",
    "flatBorders",
    "autoRedefine",
    "semiHidden",
    "unhideWhenUsed",
    "qFormat",
    "locked",
    "personal",
    "personalCompose",
    "personalReply",
];
const VERSIONS: [OfficeVersion; 6] = [
    OfficeVersion::Office2010,
    OfficeVersion::Office2013,
    OfficeVersion::Office2016,
    OfficeVersion::Office2019,
    OfficeVersion::Office2021,
    OfficeVersion::Microsoft365,
];
const WORD_NAMESPACES: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:v="urn:schemas-microsoft-com:vml""#;
const WNE: &str = "http://schemas.microsoft.com/office/word/2006/wordml";
const STRICT_SETTINGS: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/settings";
const STRICT_W: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const STRICT_A: &str = "http://purl.oclc.org/ooxml/drawingml/main";
const STRICT_WP: &str = "http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing";
const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const UNKNOWN: &str = "http://example.com/relationships/unknown";
const PARAGRAPH_STYLE: &str = r#"<w:style w:type="paragraph" w:styleId="S1"><w:name w:val="S1"/><w:pPr>PPR</w:pPr></w:style>"#;
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const BAND_SIZES: &str = r#"<w:tblStyleRowBandSize w:val="1"/><w:tblStyleColBandSize w:val="1"/>"#;
const PARAGRAPH_PROPERTIES_CHANGE: &str =
    r#"<w:pPrChange w:id="1" w:author="a"><w:pPr/></w:pPrChange>"#;

/// A Word document related by `office_document` whose main part relates to
/// one part by each of `related`: its name, relationship type, and content
/// type.
fn word(office_document: &str, related: &[(&str, &str, &str)]) -> Package {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    package.set_part_bytes(&main, MAIN, b"<a/>").unwrap();
    package.relate(None, &main, office_document).unwrap();
    for (name, relationship_type, content_type) in related {
        let part = PartName::new(name).unwrap();
        package
            .set_part_bytes(&part, content_type, b"<a/>")
            .unwrap();
        package
            .relate(Some(&main), &part, relationship_type)
            .unwrap();
    }
    package
}

fn chart_document(chart: &[u8]) -> Package {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    package
        .set_part_bytes(
            &main,
            MAIN,
            br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body/></w:document>"#,
        )
        .unwrap();
    package.relate(None, &main, OFFICE_DOCUMENT).unwrap();
    let part = PartName::new("/word/charts/chart1.xml").unwrap();
    package.set_part_bytes(&part, CHART_CONTENT, chart).unwrap();
    package.relate(Some(&main), &part, CHART).unwrap();
    package
}

fn word_document(before_body: &str, body: &str) -> Package {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    let document =
        format!("<w:document {WORD_NAMESPACES}>{before_body}<w:body>{body}</w:body></w:document>");
    package
        .set_part_bytes(&main, MAIN, document.as_bytes())
        .unwrap();
    package.relate(None, &main, OFFICE_DOCUMENT).unwrap();
    package
}

/// The part text of `root`, after the declaration Office writes.
fn xml_of(root: Element) -> String {
    String::from_utf8(XmlDocument::new(DECLARATION, root).to_xml(WriteOptions::default())).unwrap()
}

/// A Word document whose main part is `document`, written by the shared tree writer.
fn typed_word(document: &Document) -> Package {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    let xml = xml_of(write::document(document, &mut Scope::default()));
    package.set_part_bytes(&main, MAIN, xml.as_bytes()).unwrap();
    package.relate(None, &main, OFFICE_DOCUMENT).unwrap();
    package
}

/// A Word document whose body holds `blocks`.
fn typed_body(blocks: Vec<BlockContent>) -> Package {
    let mut body = DocumentBody::default();
    body.content = blocks;
    let mut document = Document::default();
    document.body = Some(Box::new(body));
    typed_word(&document)
}

/// A Word document whose body holds one paragraph of `content`.
fn typed_paragraph(content: Vec<ParagraphContent>) -> Package {
    let mut paragraph = Paragraph::default();
    paragraph.content = content;
    typed_body(vec![BlockContent::Paragraph(paragraph)])
}

/// An empty Word document whose numbering part holds `numbering`.
fn typed_numbering(numbering: &NumberingDefinitions) -> Package {
    with_part(
        typed_body(Vec::new()),
        "/word/numbering.xml",
        NUMBERING,
        NUMBERING_CONTENT,
        &xml_of(write::numbering_definitions(
            numbering,
            &mut Scope::default(),
        )),
    )
}

/// An empty Word document whose styles part holds `styles`.
fn typed_styles(styles: Vec<StyleDefinition>) -> Package {
    let mut definitions = StyleDefinitions::default();
    definitions.style_definition = styles;
    typed_style_definitions(&definitions)
}

/// An empty Word document whose styles part is `definitions`.
fn typed_style_definitions(definitions: &StyleDefinitions) -> Package {
    with_part(
        typed_body(Vec::new()),
        "/word/styles.xml",
        STYLES,
        STYLES_CONTENT,
        &xml_of(write::style_definitions(definitions, &mut Scope::default())),
    )
}

/// An empty Word document whose settings part holds `settings`.
fn typed_settings(settings: &DocumentSettings) -> Package {
    with_part(
        typed_body(Vec::new()),
        "/word/settings.xml",
        SETTINGS,
        SETTINGS_CONTENT,
        &xml_of(write::document_settings(settings, &mut Scope::default())),
    )
}

/// `package` with a web settings part holding `web_settings`.
fn with_web_settings(package: Package, web_settings: &WebPageSettings) -> Package {
    with_part(
        package,
        "/word/webSettings.xml",
        WEB_SETTINGS,
        WEB_SETTINGS_CONTENT,
        &xml_of(write::web_page_settings(
            web_settings,
            &mut Scope::default(),
        )),
    )
}

/// A table of `properties` and no row.
fn typed_table(properties: TableProperties) -> Package {
    let mut table = Table::default();
    table.properties = Box::new(properties);
    typed_body(vec![BlockContent::Table(table)])
}

/// A table of one row of `properties` holding one cell of `cell_properties` and an empty
/// paragraph.
fn table_of_one_cell(
    properties: TableRowProperties,
    cell_properties: Option<TableCellProperties>,
) -> Table {
    let mut cell = TableCell::default();
    cell.content = vec![BlockContent::Paragraph(Paragraph::default())];
    cell.properties = cell_properties.map(Box::new);
    let mut row = TableRow::default();
    row.content = vec![ContentCellContent::TableCell(cell)];
    row.properties = Some(Box::new(properties));
    let mut table = Table::default();
    table.content_row_content = vec![ContentRowContent::TableRow(row)];
    table
}

/// A style of `kind` whose id and name are `id`.
fn typed_style(kind: &str, id: &str) -> StyleDefinition {
    let mut name = PrimaryStyleName::default();
    name.val = Some(id.to_owned());
    let mut style = StyleDefinition::default();
    style.primary_style_name = Some(Box::new(name));
    style.style_id = Some(id.to_owned());
    style.r#type = Some(StyleTypes::from(kind.to_owned()));
    style
}

/// The table style `T1`, its `firstRow` condition holding `condition`.
fn table_style_condition(condition: StyleConditionalTableFormattingProperties) -> StyleDefinition {
    let mut first_row = condition;
    first_row.r#type = Some(ConditionalTableStyleFormattingTypes::from(
        "firstRow".to_owned(),
    ));
    let mut style = typed_style("table", "T1");
    style.style_conditional_table_formatting_properties = vec![first_row];
    style
}

/// The document defaults, their paragraph properties being `properties`.
fn document_defaults(properties: ParagraphPropertiesPPrGeneral) -> Package {
    let mut paragraph = DefaultParagraphProperties::default();
    paragraph.p_pr_general = Some(Box::new(properties));
    let mut defaults = DocumentDefaultParagraphAndRunProperties::default();
    defaults.default_paragraph_properties = Some(Box::new(paragraph));
    let mut definitions = StyleDefinitions::default();
    definitions.document_default_paragraph_and_run_properties = Some(Box::new(defaults));
    typed_style_definitions(&definitions)
}

fn number(value: &str) -> DecimalNumberValue {
    DecimalNumberValue::from(value.to_owned())
}

/// A run of the text `value`.
fn typed_run(value: &str) -> TextRun {
    let mut text = Text::default();
    text.value = value.to_owned();
    let mut run = TextRun::default();
    run.content = vec![TextRunContent::Text(text)];
    run
}

fn with_part(
    mut package: Package,
    name: &str,
    relationship: &str,
    content_type: &str,
    xml: &str,
) -> Package {
    let main = PartName::new("/word/document.xml").unwrap();
    let part = PartName::new(name).unwrap();
    package
        .set_part_bytes(&part, content_type, xml.as_bytes())
        .unwrap();
    package.relate(Some(&main), &part, relationship).unwrap();
    package
}

fn numbering_document(level_justification: &str) -> Package {
    let numbering = format!(
        "<w:numbering {WORD_NAMESPACES}><w:abstractNum w:abstractNumId=\"0\"><w:lvl w:ilvl=\"0\">{level_justification}</w:lvl></w:abstractNum></w:numbering>"
    );
    with_part(
        word_document("", ""),
        "/word/numbering.xml",
        NUMBERING,
        NUMBERING_CONTENT,
        &numbering,
    )
}

fn settings_document(settings: &str) -> Package {
    let part = format!("<w:settings {WORD_NAMESPACES}>{settings}</w:settings>");
    with_part(
        word_document("", ""),
        "/word/settings.xml",
        SETTINGS,
        SETTINGS_CONTENT,
        &part,
    )
}

fn web_settings_document(web_settings: &str) -> Package {
    let part = format!("<w:webSettings {WORD_NAMESPACES}>{web_settings}</w:webSettings>");
    with_part(
        word_document("", ""),
        "/word/webSettings.xml",
        WEB_SETTINGS,
        WEB_SETTINGS_CONTENT,
        &part,
    )
}

fn table_document(table_properties: &str) -> Package {
    word_document(
        "",
        &format!("<w:tbl><w:tblPr>{table_properties}</w:tblPr><w:tblGrid/></w:tbl>"),
    )
}

fn text_box(content: &str) -> Package {
    word_document(
        "",
        &format!(
            r#"<w:p><w:r><w:drawing><wp:inline xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><wp:extent cx="100" cy="100"/><wp:docPr id="1" name="Box"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="{WPS}"><wps:wsp xmlns:wps="{WPS}"><wps:cNvSpPr txBox="1"/><wps:spPr/><wps:txbx>{content}</wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#
        ),
    )
}

fn errors_at(package: &Package, version: OfficeVersion, element: &str) -> Vec<String> {
    Validator::new(version)
        .validate(package)
        .iter()
        .map(ValidationError::line)
        .filter(|line| line.contains(element))
        .collect()
}

fn accepted_at_every_version(package: &Package, element: &str) {
    for version in VERSIONS {
        assert_eq!(
            errors_at(package, version, element),
            Vec::<String>::new(),
            "{version:?}"
        );
    }
}

fn refused_with(package: &Package, element: &str, error: &str) {
    let errors = errors_at(package, OfficeVersion::Microsoft365, element);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains(error), "{errors:?}");
}

fn errors_in_the_extension_list(package: &Package, version: OfficeVersion) -> Vec<String> {
    Validator::new(version)
        .validate(package)
        .iter()
        .filter(|error| error.path_text().contains("c:extLst"))
        .map(ValidationError::line)
        .collect()
}

fn reached(package: &Package, target: OfficeVersion) -> Vec<(String, &'static str)> {
    reachable_parts(package, target)
        .into_iter()
        .map(|(part, part_rule)| (part.as_str().to_owned(), part_rule.name))
        .collect()
}

fn errors(count: usize) -> Vec<ValidationError> {
    iter::repeat_with(|| ValidationError::new(ErrorCode::ChoiceWithoutRequires, None, None, &[]))
        .take(count)
        .collect()
}

#[test]
fn a_new_validator_stops_at_1000_errors() {
    let validator = Validator::new(OfficeVersion::Office2016);
    assert_eq!(Validator::DEFAULT_MAX_ERRORS, 1000);
    assert_eq!(validator.max_errors(), 1000);
    assert_eq!(validator.version(), OfficeVersion::Office2016);
}

#[test]
fn the_error_that_reaches_the_cap_is_kept() {
    let validator = Validator::new(OfficeVersion::Microsoft365).with_max_errors(3);
    assert_eq!(validator.limit(errors(2)).len(), 2);
    assert_eq!(validator.limit(errors(3)).len(), 3);
    assert_eq!(validator.limit(errors(4)).len(), 3);
}

#[test]
fn zero_means_no_limit() {
    let validator = Validator::new(OfficeVersion::Microsoft365).with_max_errors(0);
    assert_eq!(validator.limit(errors(2500)).len(), 2500);
}

#[test]
fn a_package_with_no_document_has_no_errors() {
    let validator = Validator::new(OfficeVersion::Microsoft365);
    assert_eq!(validator.validate(&Package::new()).len(), 0);
}

#[test]
fn each_relationship_type_names_one_part_type_in_a_document() {
    for document in [
        "PresentationDocument",
        "SpreadsheetDocument",
        "WordprocessingDocument",
    ] {
        let parts = document_parts(rule(document).unwrap());
        assert!(!parts.is_empty(), "{document}");
        for part_rule in &parts {
            let same = parts
                .iter()
                .filter(|other| other.relationship_type == part_rule.relationship_type)
                .count();
            assert_eq!(same, 1, "{document}: {}", part_rule.name);
        }
    }
}

#[test]
fn the_parts_reached_are_typed_by_relationship_and_kept_from_their_version_on() {
    let package = word(
        OFFICE_DOCUMENT,
        &[
            ("/word/styles.xml", STYLES, STYLES_CONTENT),
            (
                "/word/commentsExtended.xml",
                COMMENTS_EXTENDED,
                COMMENTS_EXTENDED_CONTENT,
            ),
            ("/word/other.xml", UNKNOWN, "application/xml"),
        ],
    );
    assert_eq!(
        reached(&package, OfficeVersion::Office2013),
        [
            ("/word/document.xml".to_owned(), "MainDocumentPart"),
            ("/word/styles.xml".to_owned(), "StyleDefinitionsPart"),
            (
                "/word/commentsExtended.xml".to_owned(),
                "WordprocessingCommentsExPart"
            ),
        ]
    );
    assert_eq!(
        reached(&package, OfficeVersion::Office2010),
        [
            ("/word/document.xml".to_owned(), "MainDocumentPart"),
            ("/word/styles.xml".to_owned(), "StyleDefinitionsPart"),
        ]
    );
}

#[test]
fn a_part_whose_content_type_contradicts_its_part_type_is_not_reached() {
    let package = word(
        OFFICE_DOCUMENT,
        &[("/word/styles.xml", STYLES, "application/xml")],
    );
    assert_eq!(
        reached(&package, OfficeVersion::Microsoft365),
        [("/word/document.xml".to_owned(), "MainDocumentPart")]
    );
}

#[test]
fn a_strict_relationship_type_reaches_the_same_parts() {
    let package = word(STRICT_OFFICE_DOCUMENT, &[]);
    assert_eq!(
        reached(&package, OfficeVersion::Microsoft365),
        [("/word/document.xml".to_owned(), "MainDocumentPart")]
    );
}

#[test]
fn a_strict_extended_properties_type_reaches_the_extended_properties_part() {
    let mut package = word(OFFICE_DOCUMENT, &[]);
    let app = PartName::new("/docProps/app.xml").unwrap();
    package
        .set_part_bytes(
            &app,
            "application/vnd.openxmlformats-officedocument.extended-properties+xml",
            b"<a/>",
        )
        .unwrap();
    package
        .relate(None, &app, RelationshipType::EXTENDED_PROPERTIES.strict)
        .unwrap();
    assert!(
        reached(&package, OfficeVersion::Microsoft365)
            .contains(&("/docProps/app.xml".to_owned(), "ExtendedFilePropertiesPart"))
    );
}

#[test]
fn a_target_that_names_no_part_does_not_hide_the_other_parts_of_its_source() {
    let main = PartName::new("/word/document.xml").unwrap();
    let mut package = word(
        OFFICE_DOCUMENT,
        &[("/word/styles.xml", STYLES, STYLES_CONTENT)],
    );
    package
        .set_part_bytes(
            &PartName::new(&main.relationships_part()).unwrap(),
            "application/vnd.openxmlformats-package.relationships+xml",
            format!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{UNKNOWN}" Target="media/"/><Relationship Id="rId2" Type="{STYLES}" Target="styles.xml"/></Relationships>"#
            )
            .as_bytes(),
        )
        .unwrap();
    let reread = Package::from_bytes(package.to_bytes().unwrap()).unwrap();
    assert_eq!(
        reached(&reread, OfficeVersion::Office2013),
        [
            ("/word/document.xml".to_owned(), "MainDocumentPart"),
            ("/word/styles.xml".to_owned(), "StyleDefinitionsPart"),
        ]
    );
}

#[test]
fn a_chart_extension_list_accepts_the_extension_word_writes() {
    let package = chart_document(NA_AS_BLANK_CHART);
    for version in [
        OfficeVersion::Office2010,
        OfficeVersion::Office2013,
        OfficeVersion::Office2016,
        OfficeVersion::Office2019,
        OfficeVersion::Office2021,
        OfficeVersion::Microsoft365,
    ] {
        assert_eq!(
            errors_in_the_extension_list(&package, version),
            Vec::<String>::new(),
            "{version:?}"
        );
    }
}

#[test]
fn a_chart_extension_without_its_content_element_is_refused() {
    let misnested = br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:layout/></c:plotArea><c:extLst><c:ext xmlns:c16r3="http://schemas.microsoft.com/office/drawing/2017/03/chart" uri="{56B9EC1D-385E-4148-901F-78D8002777C0}"><c16r3:dispNaAsBlank val="1"/></c:ext></c:extLst></c:chart></c:chartSpace>"#;
    let errors =
        errors_in_the_extension_list(&chart_document(misnested), OfficeVersion::Microsoft365);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("InvalidChild"), "{errors:?}");
}

/// A document whose numbering level 0 is justified `value`.
fn numbering_with_justification(value: &str) -> Package {
    let mut justification = Justification::default();
    justification.val = Some(HorizontalAlignmentType::from(value.to_owned()));
    let mut level = NumberingLevelOverrideDefinition::default();
    level.ilvl = Some(number("0"));
    level.justification = Some(Box::new(justification));
    let mut definition = AbstractNumberingDefinition::default();
    definition.abstract_num_id = Some(number("0"));
    definition.numbering_level_override_definition = vec![level];
    let mut numbering = NumberingDefinitions::default();
    numbering.abstract_numbering_definition = vec![definition];
    typed_numbering(&numbering)
}

/// A table aligned `value`.
fn table_aligned(value: &str) -> Package {
    let mut alignment = TableAlignmentException::default();
    alignment.val = Some(TableAlignmentType::from(value.to_owned()));
    let mut properties = TableProperties::default();
    properties.table_alignment_exception = Some(Box::new(alignment));
    typed_table(properties)
}

/// A table whose `w:bidiVisual` is `value`.
fn right_to_left_table(value: &str) -> Package {
    let mut properties = TableProperties::default();
    properties.visually_right_to_left_table =
        Some(Box::new(on_off!(VisuallyRightToLeftTable, value)));
    typed_table(properties)
}

/// A paragraph in the HTML division `value`.
fn paragraph_in_div(value: &str) -> Package {
    let mut div_id = AssociatedHTMLDivID::default();
    div_id.val = Some(number(value));
    let mut properties = ParagraphProperties::default();
    properties.associated_html_div_id = Some(Box::new(div_id));
    let mut paragraph = Paragraph::default();
    paragraph.properties = Some(Box::new(properties));
    typed_body(vec![BlockContent::Paragraph(paragraph)])
}

/// A document holding each of the 18 on and off elements of [`ON_OFF_ELEMENTS`] at `value`: in a
/// table row and cell, in a paragraph style, and in the web settings' frameset.
fn on_off_document(value: &str) -> Package {
    let mut row = TableRowProperties::default();
    row.content = vec![
        TableRowPropertiesContent::TableRowCannotBreakAcrossPages(on_off!(
            TableRowCannotBreakAcrossPages,
            value
        )),
        TableRowPropertiesContent::RepeatTableRow(on_off!(RepeatTableRow, value)),
        TableRowPropertiesContent::HiddenTableRowMarker(on_off!(HiddenTableRowMarker, value)),
    ];
    let mut cell = TableCellProperties::default();
    cell.dont_wrap_cell_content = Some(Box::new(on_off!(DontWrapCellContent, value)));
    cell.fit_text_within_cell = Some(Box::new(on_off!(FitTextWithinCell, value)));
    cell.ignore_end_of_cell_marker = Some(Box::new(on_off!(IgnoreEndOfCellMarker, value)));
    let mut style = typed_style("paragraph", "Body");
    style.automatically_merge_user_formatting =
        Some(Box::new(on_off!(AutomaticallyMergeUserFormatting, value)));
    style.e_mail_message_composition_style =
        Some(Box::new(on_off!(EMailMessageCompositionStyle, value)));
    style.e_mail_message_reply_style = Some(Box::new(on_off!(EMailMessageReplyStyle, value)));
    style.e_mail_message_text_style = Some(Box::new(on_off!(EMailMessageTextStyle, value)));
    style.hidden_table_row_marker = Some(Box::new(on_off!(HiddenTableRowMarker, value)));
    style.hide_style_from_main_user_interface =
        Some(Box::new(on_off!(HideStyleFromMainUserInterface, value)));
    style.primary_style = Some(Box::new(on_off!(PrimaryStyle, value)));
    style.remove_semi_hidden_property = Some(Box::new(on_off!(RemoveSemiHiddenProperty, value)));
    style.style_cannot_be_applied = Some(Box::new(on_off!(StyleCannotBeApplied, value)));
    let mut splitter = FramesetSplitterProperties::default();
    splitter.do_not_display_frameset_splitters =
        Some(Box::new(on_off!(DoNotDisplayFramesetSplitters, value)));
    splitter.frameset_splitter_border_style =
        Some(Box::new(on_off!(FramesetSplitterBorderStyle, value)));
    let mut frame = SingleFrameProperties::default();
    frame.frame_cannot_be_resized = Some(Box::new(on_off!(FrameCannotBeResized, value)));
    frame.maintain_link_to_existing_file =
        Some(Box::new(on_off!(MaintainLinkToExistingFile, value)));
    let mut frameset = NestedFramesetDefinition::default();
    frameset.content = vec![FramesetDefinitionContent::SingleFrameProperties(frame)];
    frameset.frameset_splitter_properties = Some(Box::new(splitter));
    let mut web = WebPageSettings::default();
    web.nested_frameset_definition = Some(Box::new(frameset));
    let mut styles = StyleDefinitions::default();
    styles.style_definition = vec![style];
    let package = with_part(
        typed_body(vec![BlockContent::Table(table_of_one_cell(
            row,
            Some(cell),
        ))]),
        "/word/styles.xml",
        STYLES,
        STYLES_CONTENT,
        &xml_of(write::style_definitions(&styles, &mut Scope::default())),
    );
    with_web_settings(package, &web)
}

/// A content control tagged `a` with a label and a tab index, as a block, around a run, around a
/// cell and around a row.
fn content_controls() -> [Package; 4] {
    let properties = || {
        let mut tag = ProgrammaticTag::default();
        tag.val = Some("a".to_owned());
        let mut label = StructuredDocumentTagLabel::default();
        label.val = Some(number("1"));
        let mut tab_index = StructuredDocumentTagNavigationOrderIndex::default();
        tab_index.val = Some(UnsignedDecimalNumberValue::from("1".to_owned()));
        let mut properties = StructuredDocumentTagProperties::default();
        properties.content = Some(Box::new(
            StructuredDocumentTagPropertiesContent::PlainTextStructuredDocumentTag(
                PlainTextStructuredDocumentTag::default(),
            ),
        ));
        properties.programmatic_tag = Some(Box::new(tag));
        properties.structured_document_tag_label = Some(Box::new(label));
        properties.structured_document_tag_navigation_order_index = Some(Box::new(tab_index));
        Some(Box::new(properties))
    };
    let cell = || {
        let mut cell = TableCell::default();
        cell.content = vec![BlockContent::Paragraph(Paragraph::default())];
        cell
    };
    let table_of = |rows: Vec<ContentRowContent>| {
        let mut table = Table::default();
        table.content_row_content = rows;
        typed_body(vec![BlockContent::Table(table)])
    };
    let mut block_content = BlockLevelStructuredDocumentTagContent::default();
    block_content.content = vec![ContentBlockContent::Paragraph(Paragraph::default())];
    let mut block = BlockLevelStructuredDocumentTag::default();
    block.content = Some(Box::new(block_content));
    block.properties = properties();
    let mut run_content = InlineLevelStructuredDocumentTagContent::default();
    run_content.content = vec![ParagraphContent::TextRun(typed_run("a"))];
    let mut around_run = InlineLevelStructuredDocumentTag::default();
    around_run.content = Some(Box::new(run_content));
    around_run.properties = properties();
    let mut cell_content = CellLevelStructuredDocumentTagContent::default();
    cell_content.content = vec![ContentCellContent::TableCell(cell())];
    let mut around_cell = CellLevelStructuredDocumentTag::default();
    around_cell.content = Some(Box::new(cell_content));
    around_cell.properties = properties();
    let mut row_of_control = TableRow::default();
    row_of_control.content = vec![ContentCellContent::CellLevelStructuredDocumentTag(
        around_cell,
    )];
    let mut row = TableRow::default();
    row.content = vec![ContentCellContent::TableCell(cell())];
    let mut row_content = RowLevelStructuredDocumentTagContent::default();
    row_content.content = vec![ContentRowContent::TableRow(row)];
    let mut around_row = RowLevelStructuredDocumentTag::default();
    around_row.content = Some(Box::new(row_content));
    around_row.properties = properties();
    [
        typed_body(vec![BlockContent::BlockLevelStructuredDocumentTag(block)]),
        typed_paragraph(vec![ParagraphContent::InlineLevelStructuredDocumentTag(
            around_run,
        )]),
        table_of(vec![ContentRowContent::TableRow(row_of_control)]),
        table_of(vec![ContentRowContent::RowLevelStructuredDocumentTag(
            around_row,
        )]),
    ]
}

/// Table properties holding every child of `CT_TblPrBase`.
fn every_table_style_property() -> PreviousTableProperties {
    let mut style = ReferencedTableStyle::default();
    style.val = Some("T1".to_owned());
    let mut position = FloatingTablePositioning::default();
    position.tblp_x = Some(SignedMeasurementInTwentiethsOfPoint::from("1".to_owned()));
    let mut overlap = FloatingTableAllowsOverlap::default();
    overlap.val = Some(TableOverlapSetting::from("never".to_owned()));
    let mut row_band = NumberOfRowsInRowBand::default();
    row_band.val = Some(number("4"));
    let mut column_band = NumberOfColumnsInColumnBand::default();
    column_band.val = Some(number("4"));
    let mut width = PreferredTableWidth::default();
    width.r#type = Some(TableWidthUnits::from("auto".to_owned()));
    width.w = Some(MeasurementOrPercentageValue::from("0".to_owned()));
    let mut alignment = TableAlignmentException::default();
    alignment.val = Some(TableAlignmentType::from("center".to_owned()));
    let mut layout = TableLayout::default();
    layout.r#type = Some(TableLayoutType::from("fixed".to_owned()));
    let mut look = TableStyleConditionalFormattingSettingsException::default();
    look.val = Some(FourDigitHexadecimalValue::from("04A0".to_owned()));
    let mut caption = TableCaption::default();
    caption.val = Some("c".to_owned());
    let mut description = TableDescription::default();
    description.val = Some("d".to_owned());
    let mut properties = PreviousTableProperties::default();
    properties.floating_table_allows_overlap = Some(Box::new(overlap));
    properties.floating_table_positioning = Some(Box::new(position));
    properties.number_of_columns_in_column_band = Some(Box::new(column_band));
    properties.number_of_rows_in_row_band = Some(Box::new(row_band));
    properties.preferred_table_width = Some(Box::new(width));
    properties.referenced_table_style = Some(Box::new(style));
    properties.table_alignment_exception = Some(Box::new(alignment));
    properties.table_caption = Some(Box::new(caption));
    properties.table_description = Some(Box::new(description));
    properties.table_layout = Some(Box::new(layout));
    properties.table_style_conditional_formatting_settings_exception = Some(Box::new(look));
    properties.visually_right_to_left_table = Some(Box::default());
    properties
}

#[test]
fn a_numbering_level_accepts_start_and_end() {
    for value in ["start", "end"] {
        accepted_at_every_version(&numbering_with_justification(value), "w:lvlJc");
    }
}

#[test]
fn a_numbering_level_refuses_a_value_outside_st_jc() {
    refused_with(
        &numbering_with_justification("middle"),
        "w:lvlJc",
        "InvalidAttributeValue",
    );
}

#[test]
fn a_table_accepts_start_and_end_as_its_alignment() {
    for value in ["start", "end"] {
        accepted_at_every_version(&table_aligned(value), "w:jc");
    }
}

#[test]
fn a_table_refuses_an_alignment_outside_st_jc_table() {
    refused_with(&table_aligned("both"), "w:jc", "InvalidAttributeValue");
}

#[test]
fn a_table_accepts_every_on_off_value_for_bidi_visual() {
    for value in ["0", "1", "true", "false", "on", "off"] {
        accepted_at_every_version(&right_to_left_table(value), "w:bidiVisual");
    }
}

#[test]
fn a_table_refuses_a_value_outside_st_on_off_for_bidi_visual() {
    refused_with(
        &right_to_left_table("maybe"),
        "w:bidiVisual",
        "InvalidAttributeValue",
    );
}

#[test]
fn the_on_off_elements_accept_every_on_off_value() {
    for value in ["0", "1", "true", "false", "on", "off"] {
        let package = on_off_document(value);
        for element in ON_OFF_ELEMENTS {
            accepted_at_every_version(&package, &format!("w:{element}"));
        }
    }
}

#[test]
fn an_on_off_element_refuses_a_value_outside_st_on_off() {
    let mut row = TableRowProperties::default();
    row.content = vec![TableRowPropertiesContent::TableRowCannotBreakAcrossPages(
        on_off!(TableRowCannotBreakAcrossPages, "maybe"),
    )];
    let package = typed_body(vec![BlockContent::Table(table_of_one_cell(row, None))]);
    refused_with(&package, "w:cantSplit", "InvalidAttributeValue");
}

#[test]
fn a_document_background_accepts_filled() {
    let mut shape = VmlBackground::default();
    shape.filled = Some(BooleanValue::from("t".to_owned()));
    let mut background = DocumentBackground::default();
    background.content = vec![VmlElementContent::DocumentBackground(shape)];
    let mut document = Document::default();
    document.background = Some(Box::new(background));
    document.body = Some(Box::default());
    accepted_at_every_version(&typed_word(&document), "v:background");
}

#[test]
fn a_document_background_refuses_fill() {
    let package = word_document(
        r#"<w:background><v:background fill="t"/></w:background>"#,
        "",
    );
    refused_with(&package, "v:background", "UndeclaredAttribute");
}

#[test]
fn a_paragraph_and_the_settings_accept_smart_tags() {
    let mut attribute = CustomXMLAttribute::default();
    attribute.name = Some("ProductID".to_owned());
    attribute.val = Some("LA VIVIENDA".to_owned());
    let mut properties = SmartTagProperties::default();
    properties.custom_xml_attribute = vec![attribute];
    let mut tag = InlineLevelSmartTag::default();
    tag.content = vec![ParagraphContent::TextRun(typed_run("LA VIVIENDA"))];
    tag.element = Some(XMLName::from("PersonName".to_owned()));
    tag.properties = Some(Box::new(properties));
    tag.uri = Some("urn:schemas-microsoft-com:office:smarttags".to_owned());
    let paragraph = typed_paragraph(vec![ParagraphContent::InlineLevelSmartTag(tag)]);
    accepted_at_every_version(&paragraph, "w:smartTag");
    let mut tag_type = SupplementarySmartTagInformation::default();
    tag_type.name = Some("PersonName".to_owned());
    tag_type.namespaceuri = Some("urn:schemas-microsoft-com:office:smarttags".to_owned());
    tag_type.url = Some("http://example.com/".to_owned());
    let mut settings = DocumentSettings::default();
    settings.supplementary_smart_tag_information = vec![tag_type];
    accepted_at_every_version(&typed_settings(&settings), "w:smartTagType");
}

#[test]
fn the_settings_accept_do_not_embed_smart_tags_after_the_shape_defaults() {
    let package = settings_document(
        r#"<w:shapeDefaults/><w:doNotEmbedSmartTags/><w:decimalSymbol w:val="."/>"#,
    );
    accepted_at_every_version(&package, "w:doNotEmbedSmartTags");
}

#[test]
fn the_web_settings_accept_save_smart_tags_as_xml_after_the_target_screen_size() {
    let package =
        web_settings_document(r#"<w:targetScreenSz w:val="800x600"/><w:saveSmartTagsAsXml/>"#);
    accepted_at_every_version(&package, "w:saveSmartTagsAsXml");
}

#[test]
fn the_settings_refuse_do_not_embed_smart_tags_before_the_shape_defaults() {
    let package = settings_document("<w:doNotEmbedSmartTags/><w:shapeDefaults/>");
    let errors = errors_at(&package, OfficeVersion::Microsoft365, "/w:settings[1]");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn the_settings_refuse_a_second_do_not_embed_smart_tags() {
    let package = settings_document("<w:doNotEmbedSmartTags/><w:doNotEmbedSmartTags/>");
    let errors = errors_at(&package, OfficeVersion::Microsoft365, "/w:settings[1]");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn the_body_refuses_a_smart_tag() {
    let package = word_document("", SMART_TAG);
    refused_with(&package, "w:smartTag", "InvalidChild");
}

fn styles_document(styles: &str) -> Package {
    let part = format!("<w:styles {WORD_NAMESPACES}>{styles}</w:styles>");
    with_part(
        word_document("", ""),
        "/word/styles.xml",
        STYLES,
        STYLES_CONTENT,
        &part,
    )
}

fn paragraph_style(properties: &str) -> Package {
    styles_document(&PARAGRAPH_STYLE.replace("PPR", properties))
}

fn table_with_band_sizes(sizes: &str) -> String {
    format!(
        r#"<w:tbl><w:tblPr><w:bidiVisual/>{sizes}<w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid/></w:tbl>"#
    )
}

#[test]
fn a_numbering_level_accepts_div_id_and_cnf_style_after_the_outline_level() {
    let div_id =
        numbering_document(r#"<w:pPr><w:outlineLvl w:val="1"/><w:divId w:val="1"/></w:pPr>"#);
    accepted_at_every_version(&div_id, "holds w:divId");
    let cnf_style = numbering_document(
        r#"<w:pPr><w:divId w:val="1"/><w:cnfStyle w:val="000000000000"/></w:pPr>"#,
    );
    accepted_at_every_version(&cnf_style, "holds w:cnfStyle");
}

#[test]
fn a_style_accepts_div_id_and_cnf_style_after_the_outline_level() {
    let package = paragraph_style(
        r#"<w:outlineLvl w:val="1"/><w:divId w:val="1"/><w:cnfStyle w:val="000000000000"/>"#,
    );
    accepted_at_every_version(&package, "holds w:divId");
    accepted_at_every_version(&package, "holds w:cnfStyle");
}

#[test]
fn the_document_defaults_and_a_table_style_condition_accept_div_id_and_cnf_style() {
    let properties = || {
        let mut div_id = AssociatedHTMLDivID::default();
        div_id.val = Some(number("1"));
        let mut cnf_style = ParagraphConditionalFormatting::default();
        cnf_style.val = Some(ConditionalFormattingBitmask::from(
            "000000000000".to_owned(),
        ));
        let mut properties = ParagraphPropertiesPPrGeneral::default();
        properties.associated_html_div_id = Some(Box::new(div_id));
        properties.paragraph_conditional_formatting = Some(Box::new(cnf_style));
        properties
    };
    let mut first_row = StyleConditionalTableFormattingProperties::default();
    first_row.paragraph_properties_p_pr_general = Some(Box::new(properties()));
    let condition = typed_styles(vec![table_style_condition(first_row)]);
    for package in [document_defaults(properties()), condition] {
        accepted_at_every_version(&package, "holds w:divId");
        accepted_at_every_version(&package, "holds w:cnfStyle");
    }
}

#[test]
fn a_table_and_its_property_change_accept_the_band_sizes_after_bidi_visual() {
    let table = word_document("", &table_with_band_sizes(BAND_SIZES));
    let change = word_document(
        "",
        &table_with_band_sizes("").replace(
            "<w:bidiVisual/>",
            &format!(
                r#"<w:bidiVisual/><w:tblPrChange w:id="1" w:author="a"><w:tblPr><w:bidiVisual/>{BAND_SIZES}<w:tblW w:w="0" w:type="auto"/></w:tblPr></w:tblPrChange>"#
            ),
        ),
    );
    for package in [table, change] {
        accepted_at_every_version(&package, "holds w:tblStyleRowBandSize");
        accepted_at_every_version(&package, "holds w:tblStyleColBandSize");
    }
}

#[test]
fn a_table_style_condition_accepts_the_band_sizes_before_the_justification() {
    let package = styles_document(&format!(
        r#"<w:style w:type="table" w:styleId="T1"><w:name w:val="T1"/><w:tblStylePr w:type="firstRow"><w:tblPr>{BAND_SIZES}<w:jc w:val="center"/></w:tblPr></w:tblStylePr></w:style>"#
    ));
    accepted_at_every_version(&package, "holds w:tblStyleRowBandSize");
    accepted_at_every_version(&package, "holds w:tblStyleColBandSize");
}

#[test]
fn a_background_accepts_a_drawing() {
    let mut background = DocumentBackground::default();
    background.color = Some(ColorValue::from("FFFFFF".to_owned()));
    background.drawingml_object = Some(Box::default());
    let mut document = Document::default();
    document.background = Some(Box::new(background));
    document.body = Some(Box::default());
    accepted_at_every_version(&typed_word(&document), "holds w:drawing");
}

#[test]
fn an_object_accepts_a_movie() {
    let mut movie = EmbeddedVideo::default();
    movie.id = Some("rId9".to_owned());
    let mut object = EmbeddedObject::default();
    object.content = Some(Box::new(EmbeddedObjectContent::EmbeddedVideo(movie)));
    let mut run = TextRun::default();
    run.content = vec![TextRunContent::EmbeddedObject(object)];
    let package = typed_paragraph(vec![ParagraphContent::TextRun(run)]);
    accepted_at_every_version(&package, "holds w:movie");
}

#[test]
fn a_form_field_accepts_a_label_and_a_tab_index() {
    let mut name = FormFieldName::default();
    name.val = Some(FormFieldNameValue::from("a".to_owned()));
    let mut label = StructuredDocumentTagLabel::default();
    label.val = Some(number("1"));
    let mut tab_index = StructuredDocumentTagNavigationOrderIndex::default();
    tab_index.val = Some(UnsignedDecimalNumberValue::from("1".to_owned()));
    let mut form_field = FormFieldProperties::default();
    form_field.content = vec![
        FormFieldPropertiesContent::FormFieldName(name),
        FormFieldPropertiesContent::StructuredDocumentTagLabel(label),
        FormFieldPropertiesContent::StructuredDocumentTagNavigationOrderIndex(tab_index),
        FormFieldPropertiesContent::FormFieldEnabled(FormFieldEnabled::default()),
    ];
    let mut begin = ComplexFieldCharacter::default();
    begin.content = Some(Box::new(ComplexFieldCharacterContent::FormFieldProperties(
        form_field,
    )));
    begin.fld_char_type = Some(ComplexFieldCharacterType::from("begin".to_owned()));
    let mut run = TextRun::default();
    run.content = vec![TextRunContent::ComplexFieldCharacter(begin)];
    let package = typed_paragraph(vec![ParagraphContent::TextRun(run)]);
    accepted_at_every_version(&package, "holds w:label");
    accepted_at_every_version(&package, "holds w:tabIndex");
}

#[test]
fn a_content_control_accepts_a_label_and_a_tab_index() {
    for package in content_controls() {
        accepted_at_every_version(&package, "holds w:label");
        accepted_at_every_version(&package, "holds w:tabIndex");
    }
}

#[test]
fn a_frame_accepts_a_title_and_a_long_description_and_a_frameset_a_title() {
    let title = || {
        let mut title = FrameOrFramesetTitle::default();
        title.val = Some("t".to_owned());
        Some(Box::new(title))
    };
    let size = || {
        let mut size = FrameSize::default();
        size.val = Some("x".to_owned());
        Some(Box::new(size))
    };
    let mut name = PrimaryStyleName::default();
    name.val = Some("n".to_owned());
    let mut description = FrameLongDescription::default();
    description.id = Some("rId9".to_owned());
    let mut margin = LeftAndRightMarginForFrame::default();
    margin.val = Some(MeasurementInPixels::from("1".to_owned()));
    let mut frame = SingleFrameProperties::default();
    frame.frame_long_description = Some(Box::new(description));
    frame.frame_or_frameset_title = title();
    frame.frame_size = size();
    frame.left_and_right_margin_for_frame = Some(Box::new(margin));
    frame.primary_style_name = Some(Box::new(name));
    let framed = |frameset: NestedFramesetDefinition| {
        let mut web = WebPageSettings::default();
        web.nested_frameset_definition = Some(Box::new(frameset));
        with_web_settings(typed_body(Vec::new()), &web)
    };
    let mut holding_frame = NestedFramesetDefinition::default();
    holding_frame.content = vec![FramesetDefinitionContent::SingleFrameProperties(frame)];
    let frame_package = framed(holding_frame);
    accepted_at_every_version(&frame_package, "holds w:title");
    accepted_at_every_version(&frame_package, "holds w:longDesc");
    let mut titled = NestedFramesetDefinition::default();
    titled.content = vec![FramesetDefinitionContent::SingleFrameProperties(
        SingleFrameProperties::default(),
    )];
    titled.frame_or_frameset_title = title();
    titled.frame_size = size();
    accepted_at_every_version(&framed(titled), "holds w:title");
}

#[test]
fn a_style_refuses_div_id_before_the_outline_level() {
    let package = paragraph_style(r#"<w:divId w:val="1"/><w:outlineLvl w:val="1"/>"#);
    let errors = errors_at(
        &package,
        OfficeVersion::Microsoft365,
        "/w:style[1]/w:pPr[1]:",
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_table_refuses_a_band_size_after_the_table_width() {
    let package =
        table_document(r#"<w:tblW w:w="0" w:type="auto"/><w:tblStyleRowBandSize w:val="1"/>"#);
    let errors = errors_at(
        &package,
        OfficeVersion::Microsoft365,
        "/w:tbl[1]/w:tblPr[1]:",
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_paragraph_refuses_a_label() {
    let package = word_document("", r#"<w:p><w:pPr><w:label w:val="1"/></w:pPr></w:p>"#);
    let errors = errors_at(&package, OfficeVersion::Microsoft365, "/w:p[1]/w:pPr[1]:");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_frame_refuses_a_long_description_before_the_title() {
    let package = web_settings_document(&format!(
        r#"<w:frameset><w:frame xmlns:r="{RELATIONSHIPS}"><w:longDesc r:id="rId9"/><w:title w:val="t"/></w:frame></w:frameset>"#
    ));
    let errors = errors_at(&package, OfficeVersion::Microsoft365, "/w:frame[1]:");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_frameset_refuses_a_title_after_a_frame() {
    let package =
        web_settings_document(r#"<w:frameset><w:frame/><w:title w:val="t"/></w:frameset>"#);
    let errors = errors_at(&package, OfficeVersion::Microsoft365, "/w:frameset[1]:");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_table_style_and_its_conditions_accept_every_child_of_ct_tbl_pr_base() {
    let mut first_row = StyleConditionalTableFormattingProperties::default();
    first_row.previous_table_properties = Some(Box::new(every_table_style_property()));
    let mut style = table_style_condition(first_row);
    style.previous_table_properties = Some(Box::new(every_table_style_property()));
    let package = typed_styles(vec![style]);
    for version in VERSIONS {
        assert_eq!(
            errors_at(&package, version, "/w:tblPr[1]"),
            Vec::<String>::new(),
            "{version:?}"
        );
    }
}

#[test]
fn a_style_and_a_table_style_condition_accept_p_style_before_the_other_properties() {
    let style = paragraph_style(r#"<w:pStyle w:val="S2"/><w:keepNext/>"#);
    accepted_at_every_version(&style, "holds w:pStyle");
    let condition = styles_document(
        r#"<w:style w:type="table" w:styleId="T1"><w:name w:val="T1"/><w:tblStylePr w:type="firstRow"><w:pPr><w:pStyle w:val="S2"/><w:keepNext/></w:pPr></w:tblStylePr></w:style>"#,
    );
    accepted_at_every_version(&condition, "holds w:pStyle");
}

#[test]
fn the_document_defaults_accept_p_style_and_a_properties_change() {
    let mut style = ReferencedParagraphStyle::default();
    style.val = Some("S1".to_owned());
    let mut change = RevisionInformationForParagraphProperties::default();
    change.author = Some("a".to_owned());
    change.id = Some(number("1"));
    let mut properties = ParagraphPropertiesPPrGeneral::default();
    properties.keep_paragraph_with_next_paragraph = Some(Box::default());
    properties.referenced_paragraph_style = Some(Box::new(style));
    properties.revision_information_for_paragraph_properties = Some(Box::new(change));
    let package = document_defaults(properties);
    accepted_at_every_version(&package, "holds w:pStyle");
    accepted_at_every_version(&package, "holds w:pPrChange");
}

#[test]
fn a_numbering_level_accepts_a_properties_change_after_the_other_properties() {
    let package = numbering_document(&format!(
        "<w:pPr><w:keepNext/>{PARAGRAPH_PROPERTIES_CHANGE}</w:pPr>"
    ));
    accepted_at_every_version(&package, "holds w:pPrChange");
}

#[test]
fn a_paragraph_accepts_a_negative_div_id_and_refuses_zero_by_words_rule() {
    accepted_at_every_version(&paragraph_in_div("-5"), "w:divId");
    refused_with(
        &paragraph_in_div("0"),
        "w:divId",
        "AttributeValueOutsideConstraint",
    );
}

#[test]
fn a_table_style_refuses_the_table_width_before_the_table_style() {
    let package = styles_document(
        r#"<w:style w:type="table" w:styleId="T1"><w:name w:val="T1"/><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblStyle w:val="T1"/></w:tblPr></w:style>"#,
    );
    let errors = errors_at(
        &package,
        OfficeVersion::Microsoft365,
        "/w:style[1]/w:tblPr[1]:",
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_style_refuses_p_style_after_keep_next() {
    let package = paragraph_style(r#"<w:keepNext/><w:pStyle w:val="S2"/>"#);
    let errors = errors_at(
        &package,
        OfficeVersion::Microsoft365,
        "/w:style[1]/w:pPr[1]:",
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_band_size_that_is_not_an_integer_is_refused() {
    let mut band = NumberOfRowsInRowBand::default();
    band.val = Some(number("x"));
    let mut properties = PreviousTableProperties::default();
    properties.number_of_rows_in_row_band = Some(Box::new(band));
    let mut style = typed_style("table", "T1");
    style.previous_table_properties = Some(Box::new(properties));
    refused_with(
        &typed_styles(vec![style]),
        "w:tblStyleRowBandSize",
        "InvalidAttributeValue",
    );
}

#[test]
fn a_div_id_that_is_not_an_integer_is_refused() {
    refused_with(&paragraph_in_div("x"), "w:divId", "w:val");
}

#[test]
fn a_compatibility_setting_accepts_a_name_newer_than_the_target() {
    let mut setting = CustomCompatibilitySetting::default();
    setting.name = Some("useWord2013TrackBottomHyphenation".to_owned());
    setting.uri = Some("http://schemas.microsoft.com/office/word".to_owned());
    setting.val = Some("1".to_owned());
    let mut compatibility = CompatibilitySettings::default();
    compatibility.custom_compatibility_setting = vec![setting];
    let mut settings = DocumentSettings::default();
    settings.compatibility_settings = Some(Box::new(compatibility));
    accepted_at_every_version(&typed_settings(&settings), "w:compatSetting");
}

#[test]
fn a_text_box_accepts_wne_txbx_content_and_w_txbx_content() {
    accepted_at_every_version(
        &text_box(&format!(
            r#"<wne:txbxContent xmlns:wne="{WNE}"><w:p/></wne:txbxContent>"#
        )),
        "txbxContent",
    );
    accepted_at_every_version(
        &text_box("<w:txbxContent><w:p/></w:txbxContent>"),
        "txbxContent",
    );
}

#[test]
fn wne_txbx_content_outside_a_text_box_is_refused() {
    let package = word_document(
        "",
        &format!(r#"<wne:txbxContent xmlns:wne="{WNE}"><w:p/></wne:txbxContent>"#),
    );
    assert_eq!(
        errors_at(&package, OfficeVersion::Microsoft365, "wne:txbxContent").len(),
        1
    );
}

/// A Strict document holding `body`, declared as Word's Strict save declares it.
fn strict_document(body: &str) -> Package {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    let document = format!(
        r#"<w:document xmlns:w="{STRICT_W}" xmlns:wne="{WNE}" w:conformance="strict"><w:body>{body}</w:body></w:document>"#
    );
    package
        .set_part_bytes(&main, MAIN, document.as_bytes())
        .unwrap();
    package.relate(None, &main, STRICT_OFFICE_DOCUMENT).unwrap();
    package
}

fn all_errors(package: &Package) -> Vec<String> {
    Validator::new(OfficeVersion::Microsoft365)
        .with_max_errors(0)
        .validate(package)
        .iter()
        .map(ValidationError::line)
        .collect()
}

#[test]
fn a_strict_file_accepts_the_measures_word_writes() {
    let package = strict_document(
        r#"<w:p><w:pPr><w:tabs><w:tab w:val="start" w:pos="467.50pt"/></w:tabs></w:pPr></w:p><w:sectPr><w:pgSz w:w="612pt" w:h="792pt"/><w:pgMar w:top="72pt" w:right="72pt" w:bottom="-72pt" w:left="72pt" w:header="36pt" w:footer="36pt" w:gutter="0pt"/></w:sectPr>"#,
    );
    assert_eq!(all_errors(&package), Vec::<String>::new());
}

#[test]
fn a_strict_file_accepts_a_default_tab_stop_in_points() {
    let settings = format!(
        r#"<w:settings xmlns:w="{STRICT_W}"><w:defaultTabStop w:val="36pt"/></w:settings>"#
    );
    let package = with_part(
        strict_document(""),
        "/word/settings.xml",
        STRICT_SETTINGS,
        SETTINGS_CONTENT,
        &settings,
    );
    assert_eq!(all_errors(&package), Vec::<String>::new());
}

#[test]
fn a_strict_file_accepts_a_bare_number_of_twips_and_refuses_what_is_neither() {
    assert_eq!(
        all_errors(&strict_document(
            r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>"#
        )),
        Vec::<String>::new()
    );
    let errors = all_errors(&strict_document(
        r#"<w:sectPr><w:pgSz w:w="wide" w:h="-72pt"/></w:sectPr>"#,
    ));
    assert_eq!(errors.len(), 2, "{errors:?}");
}

#[test]
fn a_transitional_file_checks_a_measure_as_before() {
    let package = word_document("", r#"<w:sectPr><w:pgSz w:w="612pt"/></w:sectPr>"#);
    refused_with(&package, "w:pgSz", "InvalidAttributeValue");
}

#[test]
fn a_conditional_format_needs_no_val_in_either_class() {
    let body = r#"<w:p><w:pPr><w:cnfStyle w:firstRow="1"/></w:pPr></w:p>"#;
    assert_eq!(all_errors(&word_document("", body)), Vec::<String>::new());
    assert_eq!(all_errors(&strict_document(body)), Vec::<String>::new());
    let with_val = r#"<w:p><w:pPr><w:cnfStyle w:val="100000000000"/></w:pPr></w:p>"#;
    assert_eq!(
        all_errors(&word_document("", with_val)),
        Vec::<String>::new()
    );
    refused_with(
        &strict_document(with_val),
        "w:cnfStyle",
        "TransitionalOnlyAttribute",
    );
}

#[test]
fn run_properties_come_in_any_order_and_repeat() {
    let body = r#"<w:p><w:r><w:rPr><w:lang w:val="en-US"/><w:highlight w:val="yellow"/><w:b/><w:b/></w:rPr><w:t>x</w:t></w:r></w:p>"#;
    assert_eq!(all_errors(&word_document("", body)), Vec::<String>::new());
    assert_eq!(all_errors(&strict_document(body)), Vec::<String>::new());
    let mark = r#"<w:p><w:pPr><w:rPr><w:ins w:id="1" w:author="a"/><w:lang w:val="en-US"/><w:b/></w:rPr></w:pPr></w:p>"#;
    assert_eq!(all_errors(&word_document("", mark)), Vec::<String>::new());
}

#[test]
fn a_tracked_change_keeps_its_place_in_run_properties() {
    let last = r#"<w:p><w:r><w:rPr><w:rPrChange w:id="1" w:author="a"><w:rPr/></w:rPrChange><w:b/></w:rPr></w:r></w:p>"#;
    refused_with(&word_document("", last), "w:rPr", "UnexpectedChild");
    let first = r#"<w:p><w:pPr><w:rPr><w:b/><w:ins w:id="1" w:author="a"/></w:rPr></w:pPr></w:p>"#;
    refused_with(&word_document("", first), "w:rPr", "UnexpectedChild");
}

#[test]
fn a_style_still_refuses_the_run_properties_word_does_not_allow_there() {
    let styles = format!(
        r#"<w:styles {WORD_NAMESPACES}><w:style w:type="paragraph" w:styleId="S"><w:name w:val="S"/><w:rPr><w:highlight w:val="yellow"/></w:rPr></w:style></w:styles>"#
    );
    let package = with_part(
        word_document("", ""),
        "/word/styles.xml",
        STYLES,
        STYLES_CONTENT,
        &styles,
    );
    assert_eq!(all_errors(&package).len(), 1);
}

/// Word's Strict shape, `wp:wsp` under `graphicData`, in a Strict document.
fn strict_shape(shape: &str) -> Package {
    strict_document(&format!(
        r#"<w:p><w:r><w:drawing><wp:inline xmlns:wp="{STRICT_WP}"><wp:extent cx="100" cy="100"/><wp:docPr id="1" name="Box"/><a:graphic xmlns:a="{STRICT_A}"><a:graphicData uri="{WPS}">{shape}</a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#
    ))
}

#[test]
fn word_s_strict_shape_is_checked_as_its_microsoft_form_is() {
    let good = "<w:p><w:r><w:t>text</w:t></w:r></w:p>";
    let shape = |text: &str| {
        format!(
            r#"<wp:wsp xmlns:wp="{STRICT_WP}"><wp:cNvSpPr txBox="1"/><wp:spPr/><wp:txbx><wne:txbxContent>{text}</wne:txbxContent></wp:txbx><wp:bodyPr/></wp:wsp>"#
        )
    };
    assert_eq!(
        all_errors(&strict_shape(&shape(good))),
        Vec::<String>::new()
    );
    let bad = "<w:p><w:pPr><w:bogus/></w:pPr></w:p>";
    let errors = all_errors(&strict_shape(&shape(bad)));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("wne:txbxContent"), "{errors:?}");
}

#[test]
fn word_s_strict_shape_refuses_a_child_of_another_namespace() {
    let shape = format!(
        r#"<wp:wsp xmlns:wp="{STRICT_WP}" xmlns:wps="{WPS}"><wp:cNvSpPr/><wp:spPr/><wps:style/><wp:bodyPr/></wp:wsp>"#
    );
    let errors = all_errors(&strict_shape(&shape));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("wps:style"), "{errors:?}");
}

#[test]
fn word_s_strict_group_is_checked_down_to_its_shapes() {
    let group = format!(
        r#"<wp:wgp xmlns:wp="{STRICT_WP}"><wp:cNvGrpSpPr/><wp:grpSpPr/><wp:grpSp><wp:cNvGrpSpPr/><wp:grpSpPr/><wp:wsp><wp:bogus/></wp:wsp></wp:grpSp></wp:wgp>"#
    );
    let errors = all_errors(&strict_shape(&group));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("wp:bogus"), "{errors:?}");
}
