use alloc::string::String;
use alloc::vec::Vec;
use std::path::Path;

use arky::parser::ast::{
    Alignment, Args, Block, BlockQuote, CodeBlock, Document, Emphasis, Heading, Html, Image,
    Inline, Item, Line, LineBreak, Link, LinkForm, LinkInline, LinkStrong, List, Mapping,
    MarkdownBlock, Paragraph, RawDocument, Section, Strong, Table, TableCell, TableFormat,
    TableRow, Text, ThematicBreak, Value,
};
use arky::parser::parse;
use arky::writer::{WriteOptions, write};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use officekit::docx::image::{PictureSource, inline_picture};
use officekit::docx::table::TableRows as _;
use officekit::parts::kinds::WordprocessingKind;
use officekit::parts::numbering_definitions;
use officekit::parts::style_definitions;
use officekit_markup::common_simple_types::{MeasurementInTwentiethsOfAPoint, OnOffValue, XMLName};
use officekit_markup::math::{
    OfficeMath, OfficeMathContent, Run as MathRun, RunContent as MathRunContent, Text as MathText,
};
use officekit_markup::vml::{ImageData, ShapeDefinition, ShapeDefinitionContent};
use officekit_markup::wordprocessing::{
    self, AbstractNumberingDefinition, AbstractNumberingDefinitionReference,
    AnchorForImportedExternalContent, AssociatedOutlineLevel, BlockContent,
    BlockLevelStructuredDocumentTag, BlockLevelStructuredDocumentTagContent, Bold, BorderStyles,
    Break, BreakTypes, CarriageReturn, ColorValue, ComplexFieldCharacter,
    ComplexFieldCharacterType, ContentBlockContent, ContentCellContent, ContentRowContent,
    DecimalNumberValue, DeletedRunContent, DeletedText, DocumentBody, EmbeddedObject, FieldCode,
    GridColumnsSpanned, HorizontalAlignmentType, Hyperlink, InlineLevelCustomXMLElement,
    InlineLevelSmartTag, InlineLevelStructuredDocumentTag, InlineLevelStructuredDocumentTagContent,
    InsertedRunContent, Italics, MeasurementInEighthsOfAPoint, MeasurementInPoints, MergedCellType,
    MoveDestinationRunContent, MoveSourceRunContent, NumberingDefinitionInstance,
    NumberingDefinitionInstanceReferenceNumId, NumberingDefinitionInstanceReferenceNumPr,
    NumberingDefinitions, NumberingFormat, NumberingLevelDefinitionOverride,
    NumberingLevelOverrideDefinition, NumberingLevelReference, NumberingLevelStartingValueOverride,
    NumberingLevelText, ParagraphAlignment, ParagraphBorderBelowIdenticalParagraphs,
    ParagraphBorders, ParagraphContent, ParagraphIndentation, ParagraphProperties,
    ParagraphPropertiesPPrGeneral, ParentStyleID, PrimaryStyleName, ReferencedCharacterStyle,
    ReferencedParagraphStyle, RunFonts, RunProperties, RunPropertiesContent,
    SignedMeasurementInTwentiethsOfPoint, SimpleField, StNumberFormat, StartingValue,
    StyleDefinition, StyleDefinitions, StyleTypes, TabCharacter, TableCellProperties, TextRun,
    TextRunContent, TrackedRunContent, VMLObject, VerticallyMergedCell, VmlElementContent,
};
use officekit_xml::packaging::package::OpenOptions;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
use officekit_xml::ready::ready_now;
use officekit_xml::write::WriteOptions as XmlWriteOptions;
use os_shim::System as _;
use os_shim::bucket::Condition;
use os_shim::mock::MemorySystem;

use crate::{
    DocxMarkdown as _, Exported, FRONTMATTER_RELATIONSHIP, MarkdownError, MarkdownOptions,
    save_markdown,
};
use officekit::docx::Docx;

/// The namespaces the main documents of the tests declare.
const NAMESPACES: &str = concat!(
    r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" "#,
    r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" "#,
    r#"xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" "#,
    r#"xmlns:v="urn:schemas-microsoft-com:vml" "#,
    r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" "#,
    r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#,
    r#"xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" "#,
    r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" "#,
    r#"xmlns:asvg="http://schemas.microsoft.com/office/drawing/2016/SVG/main" "#,
    r#"xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#
);

/// The first bytes of a PNG file, enough for a picture part of the tests.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";

/// The content of a text box holding one paragraph, `Boxed`.
const BOXED: &str = "<w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent>";

/// A Word document being built: its styles part holds [`styles`]; its other parts and
/// relationships are added before its body.
struct Fixture {
    docx: Docx,
    main: PartName,
}

impl Fixture {
    /// The document, its body holding `blocks`, built as a caller builds one.
    fn body(mut self, blocks: Vec<BlockContent>) -> Docx {
        let mut body = DocumentBody::default();
        body.content = blocks;
        let mut document = wordprocessing::Document::default();
        document.body = Some(Box::new(body));
        self.docx.set_main_document(&document).unwrap();
        self.docx
    }

    /// The document, its main part holding the XML text `body`.
    fn body_xml(mut self, body: &str) -> Docx {
        let content_type = self
            .docx
            .package()
            .content_type(&self.main)
            .unwrap()
            .to_owned();
        let document = format!("<w:document {NAMESPACES}><w:body>{body}</w:body></w:document>");
        self.docx
            .package_mut()
            .set_part_bytes(&self.main, &content_type, document.as_bytes())
            .unwrap();
        self.docx
    }

    /// Relate the main part to `target`, outside the package, by `relationship_type`; returns the
    /// relationship's id.
    fn external(&mut self, target: &str, relationship_type: &RelationshipType) -> String {
        self.docx
            .package_mut()
            .relate_external(Some(&self.main), target, relationship_type.transitional)
            .unwrap()
    }

    fn new() -> Self {
        let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
        let main = docx.main_document_part().clone();
        let part = PartName::new("/word/styles.xml").unwrap();
        let package = docx.package_mut();
        style_definitions::store_transitional(
            package,
            &part,
            &styles(),
            XmlWriteOptions::default(),
        )
        .unwrap();
        package
            .relate(
                Some(&main),
                &part,
                style_definitions::RELATIONSHIP_TYPE.transitional,
            )
            .unwrap();
        Self { docx, main }
    }

    /// Add the numbering part holding [`numbering`], its indents in twips.
    fn numbering(&mut self) {
        self.numbering_of(&numbering(|twips| twips.to_string()));
    }

    /// Add the numbering part holding `definitions`.
    fn numbering_of(&mut self, definitions: &NumberingDefinitions) {
        let part = PartName::new("/word/numbering.xml").unwrap();
        let package = self.docx.package_mut();
        numbering_definitions::store_transitional(
            package,
            &part,
            definitions,
            XmlWriteOptions::default(),
        )
        .unwrap();
        package
            .relate(
                Some(&self.main),
                &part,
                numbering_definitions::RELATIONSHIP_TYPE.transitional,
            )
            .unwrap();
    }

    /// Add the part `name` of `content_type` holding `bytes`, related from the main part by
    /// `relationship_type`; returns the relationship's id.
    fn part(
        &mut self,
        name: &str,
        content_type: &str,
        relationship_type: &RelationshipType,
        bytes: &[u8],
    ) -> String {
        let part = PartName::new(name).unwrap();
        let package = self.docx.package_mut();
        package.set_part_bytes(&part, content_type, bytes).unwrap();
        package
            .relate(Some(&self.main), &part, relationship_type.transitional)
            .unwrap()
    }
}

/// The arky document of `docx`, checked to read back from its markdown as the same tree; it
/// names no picture.
fn arky(docx: &Docx) -> Document {
    let result = exported(docx, MarkdownOptions::default());
    assert_eq!(result.images, Vec::<(String, Vec<u8>)>::new());
    result.document
}

/// A paragraph holding `content`.
fn block(content: Vec<ParagraphContent>) -> BlockContent {
    let mut paragraph = wordprocessing::Paragraph::default();
    paragraph.content = content;
    BlockContent::Paragraph(paragraph)
}

/// The text `value` of a run, its spaces kept.
fn characters(value: &str) -> TextRunContent {
    let mut characters = wordprocessing::Text::default();
    characters.space = Some("preserve".to_owned());
    characters.value = value.to_owned();
    TextRunContent::Text(characters)
}

/// A run holding a complex field's character of `kind`: `begin`, `separate` or `end`.
fn field_character(kind: &str) -> TextRun {
    let mut character = ComplexFieldCharacter::default();
    character.fld_char_type = Some(ComplexFieldCharacterType::from(kind.to_owned()));
    run_holding(alloc::vec![TextRunContent::ComplexFieldCharacter(
        character
    )])
}

/// A run of `value` in bold.
fn bold_run(value: &str) -> TextRun {
    marked(
        value,
        alloc::vec![RunPropertiesContent::Bold(Bold::default())],
    )
}

/// A run of `value` with the run properties `marks`.
fn marked(value: &str, marks: Vec<RunPropertiesContent>) -> TextRun {
    let mut properties = RunProperties::default();
    properties.content = marks;
    let mut marked = run(value);
    marked.properties = Some(Box::new(properties));
    marked
}

/// Paragraph borders of one bottom line, the horizontal rule Word writes.
fn bottom_border() -> Box<ParagraphBorders> {
    let mut bottom = ParagraphBorderBelowIdenticalParagraphs::default();
    bottom.color = Some(ColorValue::from("auto".to_owned()));
    bottom.space = Some(MeasurementInPoints::from("1".to_owned()));
    bottom.sz = Some(MeasurementInEighthsOfAPoint::from("6".to_owned()));
    bottom.val = Some(BorderStyles::from("single".to_owned()));
    let mut borders = ParagraphBorders::default();
    borders.paragraph_border_below_identical_paragraphs = Some(Box::new(bottom));
    Box::new(borders)
}

/// Paragraph properties of the style `style`, when given, indenting a paragraph to `left`.
fn indented(style: Option<&str>, left: &str) -> ParagraphProperties {
    let mut indentation = ParagraphIndentation::default();
    indentation.left = Some(SignedMeasurementInTwentiethsOfPoint::from(left.to_owned()));
    let mut properties = ParagraphProperties::default();
    properties.paragraph_indentation = Some(Box::new(indentation));
    properties.referenced_paragraph_style = style.map(|name| Box::new(style_reference(name)));
    properties
}

/// A list of instance 2 with paragraphs indented between its items; `measure` writes each
/// indent, the levels' too, from its twips.
fn indented_list(measure: fn(u16) -> String) -> Docx {
    let under = |twips: u16, value: &str| {
        styled_by(indented(None, &measure(twips)), alloc::vec![run(value)])
    };
    let mut fixture = Fixture::new();
    fixture.numbering_of(&numbering(measure));
    fixture.body(alloc::vec![
        numbered(2, 0, "one"),
        numbered(2, 1, "inner"),
        under(1440, "under inner"),
        under(720, "under one"),
        numbered(2, 0, "two"),
        under(360, "elsewhere"),
        numbered(2, 0, "three"),
    ])
}

/// What [`indented_list`] exports: a paragraph indented to where an item's text starts is a
/// later block of that item, and one indented elsewhere ends the list.
fn indented_list_sections() -> Vec<Section> {
    let inner = Block::List(List::new(
        alloc::vec![item(alloc::vec![
            paragraph_of(alloc::vec![text("inner")]),
            paragraph_of(alloc::vec![text("under inner")]),
        ])],
        true,
        Some(1),
    ));
    no_heading(alloc::vec![
        list(
            alloc::vec![
                item(alloc::vec![
                    paragraph_of(alloc::vec![text("one")]),
                    inner,
                    paragraph_of(alloc::vec![text("under one")]),
                ]),
                item(alloc::vec![paragraph_of(alloc::vec![text("two")])]),
            ],
            Some(1)
        ),
        paragraph_of(alloc::vec![text("elsewhere")]),
        list(
            alloc::vec![item(alloc::vec![paragraph_of(alloc::vec![text("three")])])],
            Some(3)
        ),
    ])
}

/// A paragraph of the run of `value`, numbered by `instance` at `level`.
fn numbered(instance: usize, level: usize, value: &str) -> BlockContent {
    let mut depth = NumberingLevelReference::default();
    depth.val = Some(decimal(level));
    let mut id = NumberingDefinitionInstanceReferenceNumId::default();
    id.val = Some(decimal(instance));
    let mut reference = NumberingDefinitionInstanceReferenceNumPr::default();
    reference.numbering_definition_instance_reference_num_id = Some(Box::new(id));
    reference.numbering_level_reference = Some(Box::new(depth));
    let mut properties = ParagraphProperties::default();
    properties.numbering_definition_instance_reference_num_pr = Some(Box::new(reference));
    styled_by(properties, alloc::vec![run(value)])
}

/// The numbering of the tests, `measure` writing each indent from its twips: instance 1 is
/// bullets, instance 2 decimal numbers from 1 whose levels' text starts at 720 and 1440, instance
/// 3 a level of format `none`, and instance 4 instance 2's numbers from 3 by a start override.
fn numbering(measure: fn(u16) -> String) -> NumberingDefinitions {
    let level = |index: usize, format: &str| {
        let mut kind = NumberingFormat::default();
        kind.val = Some(StNumberFormat::from(format.to_owned()));
        let mut first = StartingValue::default();
        first.val = Some(decimal(1));
        let mut defined = NumberingLevelOverrideDefinition::default();
        defined.ilvl = Some(decimal(index));
        defined.numbering_format = Some(Box::new(kind));
        defined.starting_value = Some(Box::new(first));
        defined
    };
    let indent = |mut defined: NumberingLevelOverrideDefinition, left: u16| {
        let mut indentation = ParagraphIndentation::default();
        indentation.hanging = Some(MeasurementInTwentiethsOfAPoint::from(measure(360)));
        indentation.left = Some(SignedMeasurementInTwentiethsOfPoint::from(measure(left)));
        let mut properties = ParagraphPropertiesPPrGeneral::default();
        properties.paragraph_indentation = Some(Box::new(indentation));
        defined.paragraph_properties_p_pr_general = Some(Box::new(properties));
        defined
    };
    let definition = |id: usize, levels: Vec<NumberingLevelOverrideDefinition>| {
        let mut abstract_definition = AbstractNumberingDefinition::default();
        abstract_definition.abstract_num_id = Some(decimal(id));
        abstract_definition.numbering_level_override_definition = levels;
        abstract_definition
    };
    let instance = |id: usize, abstract_id: usize| {
        let mut reference = AbstractNumberingDefinitionReference::default();
        reference.val = Some(decimal(abstract_id));
        let mut numbered_by = NumberingDefinitionInstance::default();
        numbered_by.abstract_numbering_definition_reference = Box::new(reference);
        numbered_by.num_id = Some(decimal(id));
        numbered_by
    };
    let mut parenthesis = NumberingLevelText::default();
    parenthesis.val = Some("%1)".to_owned());
    let mut decimals = indent(level(0, "decimal"), 720);
    decimals.numbering_level_text = Some(Box::new(parenthesis));
    let mut three = NumberingLevelStartingValueOverride::default();
    three.val = Some(decimal(3));
    let mut restart = NumberingLevelDefinitionOverride::default();
    restart.ilvl = Some(decimal(0));
    restart.numbering_level_starting_value_override = Some(Box::new(three));
    let mut from_three = instance(4, 1);
    from_three.numbering_level_definition_override = alloc::vec![restart];
    let mut definitions = NumberingDefinitions::default();
    definitions.abstract_numbering_definition = alloc::vec![
        definition(0, alloc::vec![level(0, "bullet"), level(1, "bullet")]),
        definition(1, alloc::vec![decimals, indent(level(1, "decimal"), 1440)]),
        definition(2, alloc::vec![level(0, "none")]),
    ];
    definitions.numbering_definition_instance =
        alloc::vec![instance(1, 0), instance(2, 1), instance(3, 2), from_three];
    definitions
}

/// A paragraph of the style `style` holding `runs`.
fn paragraph(style: &str, runs: Vec<TextRun>) -> BlockContent {
    let mut properties = ParagraphProperties::default();
    properties.referenced_paragraph_style = Some(Box::new(style_reference(style)));
    styled_by(properties, runs)
}

/// A paragraph of no style holding `runs`.
fn plain(runs: Vec<TextRun>) -> BlockContent {
    block(runs.into_iter().map(ParagraphContent::TextRun).collect())
}

/// A run of `value`, its text's spaces kept.
fn run(value: &str) -> TextRun {
    run_holding(alloc::vec![characters(value)])
}

/// A run holding `content`.
fn run_holding(content: Vec<TextRunContent>) -> TextRun {
    let mut run = TextRun::default();
    run.content = content;
    run
}

/// A style of `kind`, `id` and `name`.
fn style(kind: &str, id: &str, name: &str) -> StyleDefinition {
    let mut primary = PrimaryStyleName::default();
    primary.val = Some(name.to_owned());
    let mut style = StyleDefinition::default();
    style.primary_style_name = Some(Box::new(primary));
    style.style_id = Some(id.to_owned());
    style.r#type = Some(StyleTypes::from(kind.to_owned()));
    style
}

/// A reference to the paragraph style `style`.
fn style_reference(style: &str) -> ReferencedParagraphStyle {
    let mut reference = ReferencedParagraphStyle::default();
    reference.val = Some(style.to_owned());
    reference
}

/// A paragraph with `properties` holding `runs`.
fn styled_by(properties: ParagraphProperties, runs: Vec<TextRun>) -> BlockContent {
    let BlockContent::Paragraph(mut paragraph) = plain(runs) else {
        return BlockContent::Paragraph(wordprocessing::Paragraph::default());
    };
    paragraph.properties = Some(Box::new(properties));
    BlockContent::Paragraph(paragraph)
}

/// The styles of the tests: `Ttulo1` is named `heading 1` and has no outline level, `Subtitle`
/// is based on `Heading2`, `Strongly` is a bold and italic paragraph style, `Cita` is named
/// `Quote`, `Numbered` is numbered by instance 2, `ListParagraph` is named `List Paragraph`,
/// `Emphatic` is an italic character style, and `Mono` a character style in Courier New.
fn styles() -> StyleDefinitions {
    let based_on = |mut style: StyleDefinition, parent: &str| {
        let mut reference = ParentStyleID::default();
        reference.val = Some(parent.to_owned());
        style.parent_style_id = Some(Box::new(reference));
        style
    };
    let with_marks = |mut style: StyleDefinition, marks: Vec<RunPropertiesContent>| {
        let mut properties = RunProperties::default();
        properties.content = marks;
        style.run_properties = Some(Box::new(properties));
        style
    };
    let with_paragraph = |mut style: StyleDefinition, properties: ParagraphPropertiesPPrGeneral| {
        style.paragraph_properties_p_pr_general = Some(Box::new(properties));
        style
    };
    let mut normal = style("paragraph", "Normal", "Normal");
    normal.default = Some(OnOffValue::from("1".to_owned()));
    let mut level = AssociatedOutlineLevel::default();
    level.val = Some(decimal(1));
    let mut outline = ParagraphPropertiesPPrGeneral::default();
    outline.associated_outline_level = Some(Box::new(level));
    let mut instance = NumberingDefinitionInstanceReferenceNumId::default();
    instance.val = Some(decimal(2));
    let mut reference = NumberingDefinitionInstanceReferenceNumPr::default();
    reference.numbering_definition_instance_reference_num_id = Some(Box::new(instance));
    let mut numbered = ParagraphPropertiesPPrGeneral::default();
    numbered.numbering_definition_instance_reference_num_pr = Some(Box::new(reference));
    let mut fonts = RunFonts::default();
    fonts.ascii = Some("Courier New".to_owned());
    let mut definitions = StyleDefinitions::default();
    definitions.style_definition = alloc::vec![
        normal,
        with_marks(
            based_on(style("paragraph", "Ttulo1", "heading 1"), "Normal"),
            alloc::vec![RunPropertiesContent::Bold(Bold::default())],
        ),
        with_paragraph(style("paragraph", "Heading2", "heading 2"), outline),
        style("paragraph", "Heading3", "heading 3"),
        style("paragraph", "Heading7", "heading 7"),
        based_on(style("paragraph", "Subtitle", "Subtitle"), "Heading2"),
        with_marks(
            style("paragraph", "Strongly", "Strongly"),
            alloc::vec![
                RunPropertiesContent::Bold(Bold::default()),
                RunPropertiesContent::Italics(Italics::default()),
            ],
        ),
        style("paragraph", "HTMLPreformatted", "HTML Preformatted"),
        style("paragraph", "Cita", "Quote"),
        with_paragraph(style("paragraph", "Numbered", "Numbered"), numbered),
        style("paragraph", "ListParagraph", "List Paragraph"),
        style("character", "HTMLCode", "HTML Code"),
        with_marks(
            style("character", "Emphatic", "Emphatic"),
            alloc::vec![RunPropertiesContent::Italics(Italics::default())],
        ),
        with_marks(
            style("character", "Mono", "Mono"),
            alloc::vec![RunPropertiesContent::RunFonts(fonts)],
        ),
    ];
    definitions
}

/// A table of `rows`, its first `head` rows marked as head rows.
fn table(head: usize, rows: Vec<wordprocessing::TableRow>) -> BlockContent {
    let mut built = wordprocessing::Table::default();
    built.content_row_content = rows.into_iter().map(ContentRowContent::TableRow).collect();
    built.repeat_header_rows(head);
    BlockContent::Table(built)
}

/// A table cell holding `blocks`.
fn table_cell(blocks: Vec<BlockContent>) -> wordprocessing::TableCell {
    let mut cell = wordprocessing::TableCell::default();
    cell.content = blocks;
    cell
}

/// A table row of `cells`.
fn table_row(cells: Vec<wordprocessing::TableCell>) -> wordprocessing::TableRow {
    let mut row = wordprocessing::TableRow::default();
    row.content = cells
        .into_iter()
        .map(ContentCellContent::TableCell)
        .collect();
    row
}

/// A table cell of one paragraph of the run of `value`.
fn text_cell(value: &str) -> wordprocessing::TableCell {
    table_cell(alloc::vec![plain(alloc::vec![run(value)])])
}

/// A hyperlink to the bookmark `anchor` holding the run of `value`.
fn anchored(anchor: &str, value: &str) -> ParagraphContent {
    let mut hyperlink = Hyperlink::default();
    hyperlink.anchor = Some(anchor.to_owned());
    hyperlink.content = alloc::vec![text_run(value)];
    ParagraphContent::Hyperlink(hyperlink)
}

/// The runs of a complex field of `instruction` whose result is the run of `shown`.
fn complex_field(instruction: &str, shown: &str) -> Vec<ParagraphContent> {
    let mut code = FieldCode::default();
    code.space = Some("preserve".to_owned());
    code.value = instruction.to_owned();
    [
        field_character("begin"),
        run_holding(alloc::vec![TextRunContent::FieldCode(code)]),
        field_character("separate"),
        run(shown),
        field_character("end"),
    ]
    .into_iter()
    .map(ParagraphContent::TextRun)
    .collect()
}

/// A run holding a `w:pict` of [`vml_picture`].
fn pict(alt: &str, id: &str) -> TextRun {
    let mut object = VMLObject::default();
    object.content = alloc::vec![vml_picture(alt, id)];
    run_holding(alloc::vec![TextRunContent::VMLObject(object)])
}

/// A run holding a one-inch inline picture from `source`, of the drawing id `id` and the alt text
/// `descr`.
fn picture(source: PictureSource<'_>, id: u32, descr: &str) -> TextRun {
    run_holding(alloc::vec![TextRunContent::DrawingMLObject(
        inline_picture(
            source,
            id,
            &format!("Picture {id}"),
            Some(descr),
            (914_400, 914_400),
        )
    )])
}

/// A simple field of `instruction` whose result is the run of `shown`.
fn simple_field(instruction: &str, shown: &str) -> ParagraphContent {
    let mut field = SimpleField::default();
    field.content = alloc::vec![text_run(shown)];
    field.instr = Some(instruction.to_owned());
    ParagraphContent::SimpleField(field)
}

/// The run of `value` as paragraph content.
fn text_run(value: &str) -> ParagraphContent {
    ParagraphContent::TextRun(run(value))
}

/// A VML `v:shape` of the alt text `alt` whose `v:imagedata` names the relationship `id`.
fn vml_picture(alt: &str, id: &str) -> VmlElementContent {
    let mut data = ImageData::default();
    data.r_id = Some(id.to_owned());
    let mut shape = ShapeDefinition::default();
    shape.alt = Some(alt.to_owned());
    shape.content = alloc::vec![ShapeDefinitionContent::ImageData(data)];
    VmlElementContent::ShapeDefinition(shape)
}

/// A new Word document whose body holds `blocks` and whose styles part holds [`styles`].
fn word(blocks: Vec<BlockContent>) -> Docx {
    Fixture::new().body(blocks)
}

/// A table cell of one paragraph of plain `value`.
fn cell_of(value: &str) -> TableCell {
    TableCell::new(MarkdownBlock::new(no_heading(alloc::vec![paragraph_of(
        alloc::vec![text(value)]
    )])))
}

/// A table cell with no block.
fn empty_cell() -> TableCell {
    TableCell::new(MarkdownBlock::new(Vec::new()))
}

/// What `docx` exports with `options` to the folder `report_media`, its document checked to
/// read back from its markdown as the same tree.
fn exported(docx: &Docx, options: MarkdownOptions) -> Exported {
    let result = docx.to_arky("report_media", options).unwrap();
    let text = write(
        &RawDocument::Valid(result.document.clone()),
        &WriteOptions::default(),
    )
    .unwrap();
    assert_eq!(
        parse(&text).document,
        RawDocument::Valid(result.document.clone()),
        "{text}"
    );
    result
}

/// A heading of plain `value`.
fn heading(value: &str) -> Heading {
    Heading::new(alloc::vec![text(value)])
}

/// An image of `target` whose alt text is `alt`, with no title.
fn image(target: &str, alt: &str) -> Inline {
    Inline::Image(Image::new(
        LinkForm::Markdown,
        target.to_owned(),
        None,
        alt.to_owned(),
    ))
}

/// A list item of `blocks`.
fn item(blocks: Vec<Block>) -> Item {
    Item::new(MarkdownBlock::new(no_heading(blocks)))
}

fn line_break() -> Inline {
    Inline::LineBreak(LineBreak::default())
}

/// A link to `target`, with no title, whose text is `children`.
fn link(target: &str, children: Vec<LinkInline>) -> Inline {
    Inline::Link(Link::new(
        LinkForm::Markdown,
        target.to_owned(),
        None,
        children,
    ))
}

fn link_text(value: &str) -> LinkInline {
    LinkInline::Text(Text::new(value.to_owned()))
}

/// A tight list of `items`, ordered from `start` or bullets.
fn list(items: Vec<Item>, start: Option<u64>) -> Block {
    Block::List(List::new(items, false, start))
}

/// The markdown of `docx`.
fn markdown(docx: &Docx) -> String {
    write(&RawDocument::Valid(arky(docx)), &WriteOptions::default()).unwrap()
}

/// One section with no heading, holding `blocks`.
fn no_heading(blocks: Vec<Block>) -> Vec<Section> {
    alloc::vec![Section::new(None, blocks, Vec::new())]
}

/// A paragraph of one line of `children`.
fn paragraph_of(children: Vec<Inline>) -> Block {
    paragraph_lines(alloc::vec![children])
}

/// A paragraph of `lines`, each the inline nodes of one line.
fn paragraph_lines(lines: Vec<Vec<Inline>>) -> Block {
    Block::Paragraph(Paragraph::new(lines.into_iter().map(Line::new).collect()))
}

/// A run holding an inline picture whose `a:blip` names `id` and whose alt text is `descr`.
fn picture_xml(descr: &str, id: &str) -> String {
    format!(
        "<w:r>{}</w:r>",
        picture_drawing_xml(descr, &format!(r#"<a:blip r:embed="{id}"/>"#))
    )
}

/// A `w:drawing` of an inline picture whose `a:blip` is `blip` and whose alt text is `descr`.
fn picture_drawing_xml(descr: &str, blip: &str) -> String {
    format!(
        r#"<w:drawing><wp:inline><wp:extent cx="914400" cy="914400"/><wp:docPr id="1" name="Picture 1" descr="{descr}"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="image"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill>{blip}<a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr/></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing>"#
    )
}

/// The bytes of the file at `path`.
fn read_bytes(system: &MemorySystem, path: &str) -> Vec<u8> {
    ready_now(system.read(Path::new(path))).unwrap().0
}

fn strong(children: Vec<Inline>) -> Inline {
    Inline::Strong(Strong::new(children))
}

fn text(value: &str) -> Inline {
    Inline::Text(Text::new(value.to_owned()))
}

/// A new Word document whose body is the XML text `body` and whose styles part holds [`styles`].
fn word_xml(body: &str) -> Docx {
    Fixture::new().body_xml(body)
}

#[test]
fn the_design_sample_writes_its_markdown_and_no_media_folder() {
    let address = run_holding(alloc::vec![
        TextRunContent::Break(Break::default()),
        characters("Calle 5")
    ]);
    let document = word(alloc::vec![
        paragraph("Ttulo1", alloc::vec![run("Contrato")]),
        plain(alloc::vec![bold_run("Nombre: "), run("Ana"), address]),
    ]);
    let system = MemorySystem::new();
    ready_now(system.create_dir_all(Path::new("/n"))).unwrap();
    ready_now(document.save_transitional(&system, Path::new("/n/contract.docx"))).unwrap();
    let docx = ready_now(Docx::open(
        &system,
        Path::new("/n/contract.docx"),
        OpenOptions::default(),
    ))
    .unwrap();
    arky(&docx);
    ready_now(save_markdown(
        &docx,
        &system,
        Path::new("/n/contract.md"),
        MarkdownOptions::default(),
    ))
    .unwrap();
    assert_eq!(
        ready_now(system.read_to_string(Path::new("/n/contract.md"))).unwrap(),
        "# Contrato\n\n**Nombre:** Ana\\\nCalle 5\n"
    );
    assert!(!ready_now(system.exists(Path::new("/n/contract_media"))).unwrap());
}

#[test]
fn save_markdown_leaves_a_media_folder_already_there_as_it_is() {
    let system = MemorySystem::new();
    ready_now(system.create_dir_all(Path::new("/n/contract_media"))).unwrap();
    ready_now(system.write(
        Path::new("/n/contract_media/kept.png"),
        b"png",
        Condition::None,
    ))
    .unwrap();
    ready_now(save_markdown(
        &word(alloc::vec![paragraph("Normal", alloc::vec![run("Hola")])]),
        &system,
        Path::new("/n/contract.md"),
        MarkdownOptions::default(),
    ))
    .unwrap();
    assert_eq!(
        ready_now(system.read_to_string(Path::new("/n/contract.md"))).unwrap(),
        "Hola\n"
    );
    assert_eq!(
        ready_now(system.read_to_string(Path::new("/n/contract_media/kept.png"))).unwrap(),
        "png"
    );
}

#[test]
fn a_style_named_heading_1_and_an_own_outline_level_0_both_give_depth_1_headings() {
    let mut level = AssociatedOutlineLevel::default();
    level.val = Some(decimal(0));
    let mut outlined = ParagraphProperties::default();
    outlined.associated_outline_level = Some(Box::new(level));
    let docx = word(alloc::vec![
        paragraph("Ttulo1", alloc::vec![run("One")]),
        styled_by(outlined, alloc::vec![run("Two")]),
    ]);
    assert_eq!(
        arky(&docx).body.sections,
        [
            Section::new(Some(heading("One")), Vec::new(), Vec::new()),
            Section::new(Some(heading("Two")), Vec::new(), Vec::new()),
        ]
    );
    assert_eq!(markdown(&docx), "# One\n\n# Two\n");
}

#[test]
fn a_style_based_on_a_heading_style_takes_its_level() {
    let docx = word(alloc::vec![
        paragraph("Ttulo1", alloc::vec![run("Top")]),
        paragraph("Subtitle", alloc::vec![run("Under")]),
    ]);
    assert_eq!(markdown(&docx), "# Top\n\n## Under\n");
}

#[test]
fn a_level_3_heading_after_a_level_1_heading_sits_in_a_depth_2_section_with_no_heading() {
    let docx = word(alloc::vec![
        paragraph("Ttulo1", alloc::vec![run("One")]),
        paragraph("Heading3", alloc::vec![run("Three")]),
    ]);
    let three = Section::new(Some(heading("Three")), Vec::new(), Vec::new());
    let skipped = Section::new(None, Vec::new(), alloc::vec![three]);
    assert_eq!(
        arky(&docx).body.sections,
        [Section::new(
            Some(heading("One")),
            Vec::new(),
            alloc::vec![skipped]
        )]
    );
    assert_eq!(markdown(&docx), "# One\n\n### Three\n");
}

#[test]
fn a_level_7_heading_is_a_bold_paragraph_of_the_open_section() {
    let docx = word(alloc::vec![
        paragraph("Ttulo1", alloc::vec![run("Top")]),
        paragraph("Heading7", alloc::vec![run("Deep")]),
    ]);
    let deep = paragraph_of(alloc::vec![strong(alloc::vec![text("Deep")])]);
    assert_eq!(
        arky(&docx).body.sections,
        [Section::new(
            Some(heading("Top")),
            alloc::vec![deep],
            Vec::new()
        )]
    );
    assert_eq!(markdown(&docx), "# Top\n\n**Deep**\n");
}

#[test]
fn a_heading_with_no_text_gives_nothing_and_a_line_break_in_a_heading_is_a_space() {
    let broken = run_holding(alloc::vec![
        characters("a"),
        TextRunContent::Break(Break::default()),
        characters("b"),
    ]);
    let docx = word(alloc::vec![
        paragraph("Ttulo1", Vec::new()),
        paragraph("Heading3", alloc::vec![broken]),
    ]);
    assert_eq!(markdown(&docx), "### a b\n");
}

#[test]
fn three_bold_runs_become_one_strong_node() {
    let docx = word(alloc::vec![plain(alloc::vec![
        bold_run("Re"),
        bold_run("ve"),
        bold_run("nue")
    ])]);
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![paragraph_of(alloc::vec![strong(alloc::vec![
            text("Revenue")
        ])])])
    );
}

#[test]
fn the_space_at_the_end_of_a_bold_stretch_goes_outside_it() {
    let docx = word(alloc::vec![plain(alloc::vec![
        bold_run("Nombre: "),
        run("Ana")
    ])]);
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![paragraph_of(alloc::vec![
            strong(alloc::vec![text("Nombre:")]),
            text(" Ana"),
        ])])
    );
    assert_eq!(markdown(&docx), "**Nombre:** Ana\n");
}

#[test]
fn a_run_both_bold_and_italic_is_emphasis_holding_strong() {
    let docx = word(alloc::vec![plain(alloc::vec![marked(
        "x",
        alloc::vec![
            RunPropertiesContent::Bold(Bold::default()),
            RunPropertiesContent::Italics(Italics::default()),
        ],
    )])]);
    let both = Inline::Emphasis(Emphasis::new(alloc::vec![strong(alloc::vec![text("x")])]));
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![paragraph_of(alloc::vec![both])])
    );
    assert_eq!(markdown(&docx), "***x***\n");
}

#[test]
fn character_styles_and_code_fonts_mark_runs_and_a_paragraph_style_does_not() {
    let character_style = |id: &str| {
        let mut reference = ReferencedCharacterStyle::default();
        reference.val = Some(id.to_owned());
        RunPropertiesContent::ReferencedCharacterStyle(reference)
    };
    let mut consolas = RunFonts::default();
    consolas.ascii = Some("Consolas".to_owned());
    let mut not_bold = Bold::default();
    not_bold.val = Some(OnOffValue::from("0".to_owned()));
    let docx = word(alloc::vec![
        plain(alloc::vec![
            marked("says", alloc::vec![character_style("Emphatic")]),
            run(" "),
            marked("a", alloc::vec![character_style("HTMLCode")]),
            marked(" ", alloc::vec![RunPropertiesContent::RunFonts(consolas)]),
            marked(
                "b",
                alloc::vec![
                    character_style("Mono"),
                    RunPropertiesContent::Bold(not_bold)
                ],
            ),
        ]),
        paragraph("Strongly", alloc::vec![run("plain")]),
    ]);
    assert_eq!(markdown(&docx), "*says* `a b`\n\nplain\n");
}

#[test]
fn inserted_text_stays_deleted_text_goes_and_a_field_gives_its_shown_text() {
    let tracked =
        |id: usize, runs: Vec<TextRun>| -> (Option<DecimalNumberValue>, Vec<TrackedRunContent>) {
            (
                Some(decimal(id)),
                runs.into_iter().map(TrackedRunContent::TextRun).collect(),
            )
        };
    let mut inserted = InsertedRunContent::default();
    (inserted.id, inserted.content) = tracked(1, alloc::vec![run("new")]);
    inserted.author = Some("a".to_owned());
    let mut old = DeletedText::default();
    old.value = "old".to_owned();
    let mut deleted = DeletedRunContent::default();
    (deleted.id, deleted.content) = tracked(
        2,
        alloc::vec![run_holding(alloc::vec![TextRunContent::DeletedText(old)])],
    );
    deleted.author = Some("a".to_owned());
    let mut instruction = FieldCode::default();
    instruction.space = Some("preserve".to_owned());
    instruction.value = r#" DATE \@ "d/M/yyyy" "#.to_owned();
    let mut simple = SimpleField::default();
    simple.instr = Some(" AUTHOR ".to_owned());
    simple.content = alloc::vec![ParagraphContent::TextRun(run("Ana"))];
    let mut moved_from = MoveSourceRunContent::default();
    (moved_from.id, moved_from.content) = tracked(3, alloc::vec![run("gone")]);
    moved_from.author = Some("a".to_owned());
    let mut moved_to = MoveDestinationRunContent::default();
    (moved_to.id, moved_to.content) = tracked(4, alloc::vec![run(".")]);
    moved_to.author = Some("a".to_owned());
    let docx = word(alloc::vec![block(alloc::vec![
        ParagraphContent::TextRun(run("Kept ")),
        ParagraphContent::InsertedRunContent(inserted),
        ParagraphContent::DeletedRunContent(deleted),
        ParagraphContent::TextRun(run(" on ")),
        ParagraphContent::TextRun(field_character("begin")),
        ParagraphContent::TextRun(run_holding(alloc::vec![TextRunContent::FieldCode(
            instruction
        )])),
        ParagraphContent::TextRun(field_character("separate")),
        ParagraphContent::TextRun(run("19/10/2026")),
        ParagraphContent::TextRun(field_character("end")),
        ParagraphContent::TextRun(run(" by ")),
        ParagraphContent::SimpleField(simple),
        ParagraphContent::MoveSourceRunContent(moved_from),
        ParagraphContent::MoveDestinationRunContent(moved_to),
    ])]);
    assert_eq!(markdown(&docx), "Kept new on 19/10/2026 by Ana.\n");
}

#[test]
fn consecutive_code_and_quote_paragraphs_become_one_code_block_and_one_block_quote() {
    let docx = word(alloc::vec![
        paragraph("HTMLPreformatted", alloc::vec![run("fn main() {")]),
        paragraph("HTMLPreformatted", alloc::vec![run("}")]),
        paragraph("Cita", alloc::vec![run("a")]),
        paragraph("Cita", alloc::vec![run("b")]),
    ]);
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![
            Block::CodeBlock(CodeBlock::new(
                None,
                Args::Toml(Mapping::new()),
                "fn main() {\n}".to_owned()
            )),
            Block::BlockQuote(BlockQuote::new("a\nb".to_owned())),
        ])
    );
    assert_eq!(markdown(&docx), "```\nfn main() {\n}\n```\n\n> a\n> b\n");
}

#[test]
fn tabs_and_text_wrapping_breaks_stay_and_a_page_break_goes() {
    let typed_break = |kind: &str| {
        let mut found = Break::default();
        found.r#type = Some(BreakTypes::from(kind.to_owned()));
        TextRunContent::Break(found)
    };
    let docx = word(alloc::vec![plain(alloc::vec![run_holding(alloc::vec![
        characters("a"),
        TextRunContent::TabCharacter(TabCharacter::default()),
        characters("b"),
        TextRunContent::CarriageReturn(CarriageReturn::default()),
        characters("c"),
        typed_break("page"),
        characters("d"),
        typed_break("textWrapping"),
        characters("e"),
        TextRunContent::Break(Break::default()),
    ])])]);
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![paragraph_lines(alloc::vec![
            alloc::vec![text("a\tb"), line_break()],
            alloc::vec![text("cd"), line_break()],
            alloc::vec![text("e")],
        ])])
    );
}

#[test]
fn an_empty_paragraph_gives_nothing_and_one_with_a_bottom_border_alone_a_thematic_break() {
    let mut bordered = ParagraphProperties::default();
    bordered.paragraph_borders = Some(bottom_border());
    let docx = word(alloc::vec![
        plain(Vec::new()),
        plain(alloc::vec![run("a")]),
        styled_by(bordered, Vec::new()),
        plain(alloc::vec![run("  ")]),
        plain(alloc::vec![run("b")]),
    ]);
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![
            paragraph_of(alloc::vec![text("a")]),
            Block::ThematicBreak(ThematicBreak::default()),
            paragraph_of(alloc::vec![text("b")]),
        ])
    );
}

#[test]
fn content_controls_smart_tags_and_custom_xml_give_their_content() {
    let mut inner_content = InlineLevelStructuredDocumentTagContent::default();
    inner_content.content = alloc::vec![ParagraphContent::TextRun(run("b"))];
    let mut inner = InlineLevelStructuredDocumentTag::default();
    inner.content = Some(Box::new(inner_content));
    let mut smart_tag = InlineLevelSmartTag::default();
    smart_tag.content = alloc::vec![ParagraphContent::TextRun(run(" c"))];
    smart_tag.element = Some(XMLName::from("e".to_owned()));
    smart_tag.uri = Some("u".to_owned());
    let mut custom = InlineLevelCustomXMLElement::default();
    custom.content = alloc::vec![ParagraphContent::TextRun(run(" d"))];
    custom.element = Some(XMLName::from("f".to_owned()));
    let mut held = wordprocessing::Paragraph::default();
    held.content = alloc::vec![
        ParagraphContent::TextRun(run("a ")),
        ParagraphContent::InlineLevelStructuredDocumentTag(inner),
        ParagraphContent::InlineLevelSmartTag(smart_tag),
        ParagraphContent::InlineLevelCustomXMLElement(custom),
    ];
    let mut outer_content = BlockLevelStructuredDocumentTagContent::default();
    outer_content.content = alloc::vec![ContentBlockContent::Paragraph(held)];
    let mut outer = BlockLevelStructuredDocumentTag::default();
    outer.content = Some(Box::new(outer_content));
    let docx = word(alloc::vec![BlockContent::BlockLevelStructuredDocumentTag(
        outer
    )]);
    assert_eq!(markdown(&docx), "a b c d\n");
}

// XML input: the typed tree keeps mc:AlternateContent's branches and Word's wps shape as unknown content.
#[test]
fn a_text_box_gives_its_paragraphs_once_after_the_paragraph_that_holds_it() {
    let choice = format!(
        r#"<mc:Choice Requires="wps"><w:drawing><wp:anchor><a:graphic><a:graphicData><wps:wsp><wps:txbx>{BOXED}</wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice>"#
    );
    let fallback = format!(
        "<mc:Fallback><w:pict><v:shape><v:textbox>{BOXED}</v:textbox></v:shape></w:pict></mc:Fallback>"
    );
    let body = format!(
        "<w:p><w:r><w:t>Before</w:t></w:r><w:r><mc:AlternateContent>{choice}{fallback}</mc:AlternateContent></w:r></w:p><w:p><w:r><w:pict><v:shape><v:textbox>{BOXED}</v:textbox></v:shape></w:pict></w:r></w:p>"
    );
    assert_eq!(markdown(&word_xml(&body)), "Before\n\nBoxed\n\nBoxed\n");
}

/// A run holding a text box as Word writes it in a Strict file, `content` inside `element`.
fn strict_text_box(element: &str, content: &str) -> String {
    format!(
        concat!(
            r#"<w:r><w:drawing><wp:anchor><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">"#,
            r#"<wp:wsp><wp:cNvSpPr txBox="1"/><wp:txbx>"#,
            r#"<{element} xmlns:wne="http://schemas.microsoft.com/office/word/2006/wordml">{content}</{element}>"#,
            "</wp:txbx><wp:bodyPr/></wp:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>",
        ),
        element = element,
        content = content,
    )
}

// XML input: the typed tree keeps a:graphicData's shape as unknown content, and its spelling is the point.
#[test]
fn a_text_box_in_word_s_strict_form_gives_its_paragraphs_after_the_paragraph_that_holds_it() {
    let boxed = strict_text_box("wne:txbxContent", "<w:p><w:r><w:t>Boxed</w:t></w:r></w:p>");
    let body = format!(
        "<w:p><w:r><w:t>Before</w:t></w:r>{boxed}</w:p><w:p><w:r><w:t>After</w:t></w:r></w:p>"
    );
    assert_eq!(markdown(&word_xml(&body)), "Before\n\nBoxed\n\nAfter\n");
}

// XML input: the typed tree keeps a:graphicData's shape as unknown content, and its spelling is the point.
#[test]
fn a_text_box_in_the_form_the_strict_schema_declares_gives_its_paragraphs() {
    let boxed = strict_text_box("wp:txbxContent", "<w:p><w:r><w:t>Boxed</w:t></w:r></w:p>");
    let body = format!("<w:p><w:r><w:t>Before</w:t></w:r>{boxed}</w:p>");
    assert_eq!(markdown(&word_xml(&body)), "Before\n\nBoxed\n");
}

// XML input: the typed tree keeps a:graphicData's shape as unknown content, and its spelling is the point.
#[test]
fn a_picture_in_a_text_box_in_word_s_strict_form_comes_once_with_the_box() {
    let mut fixture = Fixture::new();
    let id = fixture.part(
        "/word/media/image1.png",
        "image/png",
        &RelationshipType::IMAGE,
        PNG,
    );
    let inside = format!("<w:p>{}</w:p>", picture_xml("boxed", &id));
    let boxed = strict_text_box("wne:txbxContent", &inside);
    let body = format!("<w:p><w:r><w:t>Before</w:t></w:r>{boxed}</w:p>");
    let result = exported(&fixture.body_xml(&body), MarkdownOptions::default());
    assert_eq!(
        result.document.body.sections,
        no_heading(alloc::vec![
            paragraph_of(alloc::vec![text("Before")]),
            paragraph_of(alloc::vec![image("report_media/image1.png", "boxed")]),
        ])
    );
}

#[test]
fn an_inline_picture_built_through_the_model_exports_with_its_alt_text() {
    let mut fixture = Fixture::new();
    let id = fixture.part(
        "/word/media/image1.png",
        "image/png",
        &RelationshipType::IMAGE,
        PNG,
    );
    let picture = run_holding(alloc::vec![TextRunContent::DrawingMLObject(
        inline_picture(
            PictureSource::Embedded(&id),
            1,
            "Picture 1",
            Some("Logo"),
            (914_400, 914_400),
        )
    )]);
    let result = exported(
        &fixture.body(alloc::vec![plain(alloc::vec![picture])]),
        MarkdownOptions::default(),
    );
    assert_eq!(
        result.document.body.sections,
        no_heading(alloc::vec![paragraph_of(alloc::vec![image(
            "report_media/image1.png",
            "Logo"
        )])])
    );
}

#[test]
fn an_equation_gives_its_text() {
    let mut solution = MathText::default();
    solution.value = "x=1".to_owned();
    let mut math_run = MathRun::default();
    math_run.content = alloc::vec![MathRunContent::Text(solution)];
    let mut equation = OfficeMath::default();
    equation.content = alloc::vec![OfficeMathContent::Run(math_run)];
    let docx = word(alloc::vec![block(alloc::vec![
        ParagraphContent::TextRun(run("Solve ")),
        ParagraphContent::OfficeMath(equation),
    ])]);
    assert_eq!(markdown(&docx), "Solve x=1\n");
}

#[test]
fn to_arky_leaves_the_main_part_as_it_was() {
    let docx = word(alloc::vec![
        paragraph("Ttulo1", alloc::vec![run("Contrato")]),
        plain(alloc::vec![bold_run("x")]),
    ]);
    let main = docx.main_document_part().clone();
    let before = docx.package().part_bytes(&main).unwrap();
    arky(&docx);
    assert_eq!(docx.package().part_bytes(&main).unwrap(), before);
}

/// A table of one row, `left` and a link to the bookmark `x` reading `right`.
#[test]
fn a_table_cell_is_converted_by_the_same_rules_and_a_hyperlink_is_a_link() {
    let mut hyperlink = Hyperlink::default();
    hyperlink.anchor = Some("x".to_owned());
    hyperlink.content = alloc::vec![ParagraphContent::TextRun(run("right"))];
    let linked = block(alloc::vec![ParagraphContent::Hyperlink(hyperlink)]);
    let docx = word(alloc::vec![table(
        0,
        alloc::vec![table_row(alloc::vec![
            text_cell("left"),
            table_cell(alloc::vec![linked]),
        ])],
    )]);
    assert_eq!(
        markdown(&docx),
        "+------+-------------+\n| left | [right](#x) |\n+------+-------------+\n"
    );
}

/// The sample: a numbered list `one`, `two`; a table of `a`, `b` / `c`, `d` with no head row; a
/// hyperlink to `https://example.com/q3`; and the picture `word/media/image1.png`, its alt text
/// `Logo`.
fn sample() -> Docx {
    let mut fixture = Fixture::new();
    fixture.numbering();
    let picture_id = fixture.part(
        "/word/media/image1.png",
        "image/png",
        &RelationshipType::IMAGE,
        PNG,
    );
    let mut dashboard = Hyperlink::default();
    dashboard.content = alloc::vec![text_run("the dashboard")];
    dashboard.id = Some(fixture.external("https://example.com/q3", &RelationshipType::HYPERLINK));
    fixture.body(alloc::vec![
        numbered(2, 0, "one"),
        numbered(2, 0, "two"),
        table(
            0,
            alloc::vec![
                table_row(alloc::vec![text_cell("a"), text_cell("b")]),
                table_row(alloc::vec![text_cell("c"), text_cell("d")]),
            ],
        ),
        block(alloc::vec![
            text_run("See "),
            ParagraphContent::Hyperlink(dashboard),
            text_run("."),
        ]),
        plain(alloc::vec![picture(
            PictureSource::Embedded(&picture_id),
            1,
            "Logo"
        )]),
    ])
}

#[test]
fn the_list_table_link_and_picture_sample_writes_its_markdown_and_one_image_file() {
    let system = MemorySystem::new();
    ready_now(system.create_dir_all(Path::new("/n"))).unwrap();
    ready_now(sample().save_transitional(&system, Path::new("/n/contract.docx"))).unwrap();
    let docx = ready_now(Docx::open(
        &system,
        Path::new("/n/contract.docx"),
        OpenOptions::default(),
    ))
    .unwrap();
    assert_eq!(
        exported(&docx, MarkdownOptions::default()).images,
        [("image1.png".to_owned(), PNG.to_vec())]
    );
    ready_now(save_markdown(
        &docx,
        &system,
        Path::new("/n/contract.md"),
        MarkdownOptions::default(),
    ))
    .unwrap();
    assert_eq!(
        ready_now(system.read_to_string(Path::new("/n/contract.md"))).unwrap(),
        concat!(
            "1. one\n2. two\n\n",
            "+-----+-----+\n| a   | b   |\n+-----+-----+\n| c   | d   |\n+-----+-----+\n\n",
            "See [the dashboard](https://example.com/q3).\n\n",
            "![Logo](contract_media/image1.png)\n",
        )
    );
    assert_eq!(read_bytes(&system, "/n/contract_media/image1.png"), PNG);
}

#[test]
fn with_embedded_pictures_the_sample_writes_a_data_uri_and_no_media_folder() {
    let system = MemorySystem::new();
    ready_now(system.create_dir_all(Path::new("/n"))).unwrap();
    let docx = sample();
    let options = MarkdownOptions {
        embed_images: true,
        frontmatter: false,
    };
    assert_eq!(
        exported(&docx, options).images,
        Vec::<(String, Vec<u8>)>::new()
    );
    ready_now(save_markdown(
        &docx,
        &system,
        Path::new("/n/contract.md"),
        options,
    ))
    .unwrap();
    let encoded = STANDARD.encode(PNG);
    assert!(encoded.starts_with("iVBORw0KGgo"));
    let written = ready_now(system.read_to_string(Path::new("/n/contract.md"))).unwrap();
    assert!(
        written.ends_with(&format!(
            "See [the dashboard](https://example.com/q3).\n\n![Logo](data:image/png;base64,{encoded})\n"
        )),
        "{written}"
    );
    assert!(!ready_now(system.exists(Path::new("/n/contract_media"))).unwrap());
}

#[test]
fn a_paragraph_numbered_by_its_style_is_a_list_item_and_a_level_of_format_none_a_paragraph() {
    let mut fixture = Fixture::new();
    fixture.numbering();
    let docx = fixture.body(alloc::vec![
        paragraph("Numbered", alloc::vec![run("styled")]),
        numbered(3, 0, "hidden"),
    ]);
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![
            list(
                alloc::vec![item(alloc::vec![paragraph_of(alloc::vec![text("styled")])])],
                Some(1)
            ),
            paragraph_of(alloc::vec![text("hidden")]),
        ])
    );
    assert_eq!(markdown(&docx), "1. styled\n\nhidden\n");
}

#[test]
fn a_deeper_level_nests_bullets_have_no_start_and_a_start_override_gives_the_first_number() {
    let mut fixture = Fixture::new();
    fixture.numbering();
    let docx = fixture.body(alloc::vec![
        numbered(1, 0, "a"),
        numbered(1, 1, "b"),
        numbered(1, 0, "c"),
        numbered(4, 0, "three"),
    ]);
    let nested = list(
        alloc::vec![item(alloc::vec![paragraph_of(alloc::vec![text("b")])])],
        None,
    );
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![
            list(
                alloc::vec![
                    item(alloc::vec![paragraph_of(alloc::vec![text("a")]), nested]),
                    item(alloc::vec![paragraph_of(alloc::vec![text("c")])]),
                ],
                None
            ),
            list(
                alloc::vec![item(alloc::vec![paragraph_of(alloc::vec![text("three")])])],
                Some(3)
            ),
        ])
    );
}

#[test]
fn a_list_a_paragraph_interrupts_goes_on_from_its_last_number_and_a_deeper_level_starts_again() {
    let mut fixture = Fixture::new();
    fixture.numbering();
    let docx = fixture.body(alloc::vec![
        numbered(2, 0, "one"),
        numbered(2, 1, "inner"),
        numbered(2, 1, "inner"),
        numbered(2, 0, "two"),
        numbered(2, 1, "again"),
        paragraph("Normal", alloc::vec![run("between")]),
        numbered(2, 0, "three"),
    ]);
    assert_eq!(
        markdown(&docx),
        "1. one\n   1. inner\n   2. inner\n2. two\n   1. again\n\nbetween\n\n3. three\n"
    );
}

#[test]
fn paragraphs_that_write_nothing_leave_the_items_of_one_word_list_in_one_list() {
    let mut fixture = Fixture::new();
    fixture.numbering();
    let docx = fixture.body(alloc::vec![
        numbered(2, 0, "one"),
        plain(Vec::new()),
        numbered(2, 1, "inner"),
        paragraph("Ttulo1", Vec::new()),
        numbered(2, 0, "two"),
        plain(Vec::new()),
        numbered(1, 0, "bullet"),
    ]);
    let inner = list(
        alloc::vec![item(alloc::vec![paragraph_of(alloc::vec![text("inner")])])],
        Some(1),
    );
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![
            list(
                alloc::vec![
                    item(alloc::vec![paragraph_of(alloc::vec![text("one")]), inner]),
                    item(alloc::vec![paragraph_of(alloc::vec![text("two")])]),
                ],
                Some(1)
            ),
            list(
                alloc::vec![item(alloc::vec![paragraph_of(alloc::vec![text("bullet")])])],
                None
            ),
        ])
    );
}

/// A decimal number of the typed tree.
fn decimal(value: usize) -> DecimalNumberValue {
    DecimalNumberValue::from(value.to_string())
}

#[test]
fn an_indent_with_a_unit_finds_the_list_item_the_same_number_of_twips_finds() {
    let points = indented_list(|twips| format!("{}pt", twips / 20));
    assert_eq!(arky(&points).body.sections, indented_list_sections());
}

#[test]
fn a_measure_with_a_unit_gives_its_twips() {
    for (measure, twips) in [
        ("36pt", 720_i64),
        ("451.15pt", 9023),
        ("-18pt", -360),
        ("0.5in", 720),
        ("3pc", 720),
        ("3pi", 720),
        ("1.27cm", 720),
        ("12.7mm", 720),
    ] {
        assert_eq!(super::twips_of(measure), Some(twips), "{measure}");
    }
    assert_eq!(super::twips_of("36"), None);
}

#[test]
fn a_paragraph_indented_to_where_an_items_text_starts_is_a_later_block_of_that_item() {
    let twips = indented_list(|twips| twips.to_string());
    assert_eq!(arky(&twips).body.sections, indented_list_sections());
}

#[test]
fn a_list_paragraph_with_no_numbering_after_a_list_paragraph_is_a_later_block_of_its_item() {
    let mut fixture = Fixture::new();
    fixture.numbering();
    let docx = fixture.body(alloc::vec![
        numbered(2, 0, "one"),
        paragraph("ListParagraph", alloc::vec![run("more")]),
        numbered(2, 0, "two"),
        paragraph("Normal", alloc::vec![run("after")]),
    ]);
    let items = alloc::vec![
        item(alloc::vec![
            paragraph_of(alloc::vec![text("one")]),
            paragraph_of(alloc::vec![text("more")]),
        ]),
        item(alloc::vec![paragraph_of(alloc::vec![text("two")])]),
    ];
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![
            Block::List(List::new(items, true, Some(1))),
            paragraph_of(alloc::vec![text("after")]),
        ])
    );
    assert_eq!(markdown(&docx), "1. one\n\n   more\n\n2. two\n\nafter\n");
}

#[test]
fn a_code_block_a_block_quote_and_a_rule_inside_a_list_item_come_back_inside_it() {
    let under =
        |style: &str, value: &str| styled_by(indented(Some(style), "720"), alloc::vec![run(value)]);
    let mut ruled = indented(None, "720");
    ruled.paragraph_borders = Some(bottom_border());
    let mut fixture = Fixture::new();
    fixture.numbering();
    let docx = fixture.body(alloc::vec![
        numbered(1, 0, "item"),
        under("HTMLPreformatted", "let a = 1;"),
        under("HTMLPreformatted", "let b = 2;"),
        under("Quote", "quoted"),
        styled_by(ruled, Vec::new()),
        numbered(1, 0, "next"),
        paragraph("HTMLPreformatted", alloc::vec![run("outside")]),
    ]);
    let inside = alloc::vec![
        paragraph_of(alloc::vec![text("item")]),
        Block::CodeBlock(CodeBlock::new(
            None,
            Args::Toml(Mapping::new()),
            "let a = 1;\nlet b = 2;".to_owned()
        )),
        Block::BlockQuote(BlockQuote::new("quoted".to_owned())),
        Block::ThematicBreak(ThematicBreak::default()),
    ];
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![
            list(
                alloc::vec![
                    item(inside),
                    item(alloc::vec![paragraph_of(alloc::vec![text("next")])]),
                ],
                None
            ),
            Block::CodeBlock(CodeBlock::new(
                None,
                Args::Toml(Mapping::new()),
                "outside".to_owned()
            )),
        ])
    );
}

#[test]
fn a_later_block_indented_to_an_outer_level_goes_into_that_level_s_item() {
    let mut fixture = Fixture::new();
    fixture.numbering();
    let docx = fixture.body(alloc::vec![
        numbered(2, 0, "outer"),
        numbered(1, 1, "inner"),
        styled_by(
            indented(Some("HTMLPreformatted"), "720"),
            alloc::vec![run("code")]
        ),
    ]);
    let inner = list(
        alloc::vec![item(alloc::vec![paragraph_of(alloc::vec![text("inner")])])],
        None,
    );
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![list(
            alloc::vec![item(alloc::vec![
                paragraph_of(alloc::vec![text("outer")]),
                inner,
                Block::CodeBlock(CodeBlock::new(
                    None,
                    Args::Toml(Mapping::new()),
                    "code".to_owned()
                )),
            ])],
            Some(1)
        )])
    );
}

#[test]
fn a_table_with_a_head_row_and_one_line_cells_is_a_pipe_table_aligned_by_its_first_row() {
    let aligned = |alignment: &str, value: &str| {
        let mut justification = ParagraphAlignment::default();
        justification.val = Some(HorizontalAlignmentType::from(alignment.to_owned()));
        let mut properties = ParagraphProperties::default();
        properties.paragraph_alignment = Some(Box::new(justification));
        table_cell(alloc::vec![styled_by(properties, alloc::vec![run(value)])])
    };
    let docx = word(alloc::vec![table(
        1,
        alloc::vec![
            table_row(alloc::vec![aligned("center", "h1"), aligned("end", "h2")]),
            table_row(alloc::vec![text_cell("a"), text_cell("b")]),
        ],
    )]);
    let expected = Table::new(
        TableFormat::Pipe,
        alloc::vec![Alignment::Center, Alignment::Right],
        alloc::vec![TableRow::new(alloc::vec![cell_of("h1"), cell_of("h2")])],
        alloc::vec![TableRow::new(alloc::vec![cell_of("a"), cell_of("b")])],
    );
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![Block::Table(expected)])
    );
    assert_eq!(
        markdown(&docx),
        "| h1  | h2  |\n| :-: | --: |\n| a   | b   |\n"
    );
}

#[test]
fn a_table_with_two_head_rows_or_a_one_line_code_cell_is_a_grid_table() {
    let two_heads = table(
        2,
        alloc::vec![
            table_row(alloc::vec![text_cell("h1")]),
            table_row(alloc::vec![text_cell("h2")]),
            table_row(alloc::vec![text_cell("a")]),
        ],
    );
    let code = table_cell(alloc::vec![paragraph(
        "HTMLPreformatted",
        alloc::vec![run("let a = 1;")]
    )]);
    let code_cell = table(
        1,
        alloc::vec![
            table_row(alloc::vec![text_cell("h")]),
            table_row(alloc::vec![code]),
        ],
    );
    let docx = word(alloc::vec![two_heads, code_cell]);
    let formats: Vec<TableFormat> = arky(&docx)
        .body
        .sections
        .iter()
        .flat_map(|section| &section.blocks)
        .filter_map(|block| {
            if let Block::Table(table) = block {
                Some(table.format)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(formats, [TableFormat::Grid, TableFormat::Grid]);
}

#[test]
fn a_table_with_no_head_row_or_with_a_cell_of_two_paragraphs_is_a_grid_table() {
    let two = table_cell(alloc::vec![
        plain(alloc::vec![run("x")]),
        plain(alloc::vec![run("y")]),
    ]);
    let docx = word(alloc::vec![
        table(0, alloc::vec![table_row(alloc::vec![text_cell("a")])]),
        table(
            1,
            alloc::vec![
                table_row(alloc::vec![text_cell("h")]),
                table_row(alloc::vec![two]),
            ],
        ),
    ]);
    let two_paragraphs = TableCell::new(MarkdownBlock::new(no_heading(alloc::vec![
        paragraph_of(alloc::vec![text("x")]),
        paragraph_of(alloc::vec![text("y")]),
    ])));
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![
            Block::Table(Table::new(
                TableFormat::Grid,
                alloc::vec![Alignment::Default],
                Vec::new(),
                alloc::vec![TableRow::new(alloc::vec![cell_of("a")])],
            )),
            Block::Table(Table::new(
                TableFormat::Grid,
                alloc::vec![Alignment::Default],
                alloc::vec![TableRow::new(alloc::vec![cell_of("h")])],
                alloc::vec![TableRow::new(alloc::vec![two_paragraphs])],
            )),
        ])
    );
}

#[test]
fn a_spanning_cell_and_a_vertical_merge_continuation_are_padded_with_empty_cells() {
    let with = |properties: TableCellProperties, value: &str| {
        let mut cell = text_cell(value);
        cell.properties = Some(Box::new(properties));
        cell
    };
    let mut span = GridColumnsSpanned::default();
    span.val = Some(decimal(2));
    let mut spanning = TableCellProperties::default();
    spanning.grid_columns_spanned = Some(Box::new(span));
    let mut restart = VerticallyMergedCell::default();
    restart.val = Some(MergedCellType::from("restart".to_owned()));
    let mut merge_start = TableCellProperties::default();
    merge_start.vertically_merged_cell = Some(Box::new(restart));
    let mut merge_continuation = TableCellProperties::default();
    merge_continuation.vertically_merged_cell = Some(Box::new(VerticallyMergedCell::default()));
    let docx = word(alloc::vec![table(
        0,
        alloc::vec![
            table_row(alloc::vec![with(spanning, "wide"), text_cell("c")]),
            table_row(alloc::vec![
                with(merge_start, "x"),
                text_cell("y"),
                text_cell("z"),
            ]),
            table_row(alloc::vec![
                with(merge_continuation, "under"),
                text_cell("q"),
                text_cell("r"),
            ]),
        ],
    )]);
    let expected = Table::new(
        TableFormat::Grid,
        alloc::vec![Alignment::Default; 3],
        Vec::new(),
        alloc::vec![
            TableRow::new(alloc::vec![cell_of("wide"), empty_cell(), cell_of("c")]),
            TableRow::new(alloc::vec![cell_of("x"), cell_of("y"), cell_of("z")]),
            TableRow::new(alloc::vec![empty_cell(), cell_of("q"), cell_of("r")]),
        ],
    );
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![Block::Table(expected)])
    );
}

#[test]
fn anchors_lose_their_underscores_and_hyperlink_fields_are_links() {
    let fields = [
        complex_field(r#" HYPERLINK "https://x.example" "#, "x"),
        alloc::vec![
            text_run(" or "),
            simple_field(r#" HYPERLINK "https://y.example" "#, "y"),
        ],
    ]
    .concat();
    let docx = word(alloc::vec![
        block(alloc::vec![
            anchored("q3_results", "Q3"),
            text_run(" and "),
            anchored("_2024_plan", "plan"),
        ]),
        block(fields),
    ]);
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![
            paragraph_of(alloc::vec![
                link("#q3-results", alloc::vec![link_text("Q3")]),
                text(" and "),
                link("#2024-plan", alloc::vec![link_text("plan")]),
            ]),
            paragraph_of(alloc::vec![
                link("https://x.example", alloc::vec![link_text("x")]),
                text(" or "),
                link("https://y.example", alloc::vec![link_text("y")]),
            ]),
        ])
    );
    assert_eq!(
        markdown(&docx),
        "[Q3](#q3-results) and [plan](#2024-plan)\n\n[x](https://x.example) or [y](https://y.example)\n"
    );
}

#[test]
fn a_hyperlink_inside_a_hyperlink_joins_its_text_and_bold_inside_a_link_is_link_strong() {
    let mut fixture = Fixture::new();
    let mut nested = Hyperlink::default();
    nested.content = alloc::vec![text_run(" c")];
    nested.id = Some(fixture.external("https://inner.example", &RelationshipType::HYPERLINK));
    let mut outer = Hyperlink::default();
    outer.content = alloc::vec![
        text_run("a "),
        ParagraphContent::TextRun(bold_run("b")),
        ParagraphContent::Hyperlink(nested),
    ];
    outer.id = Some(fixture.external("https://outer.example", &RelationshipType::HYPERLINK));
    outer.tooltip = Some("Tip".to_owned());
    let docx = fixture.body(alloc::vec![block(alloc::vec![
        ParagraphContent::Hyperlink(outer)
    ])]);
    let expected = Inline::Link(Link::new(
        LinkForm::Markdown,
        "https://outer.example".to_owned(),
        Some("Tip".to_owned()),
        alloc::vec![
            link_text("a "),
            LinkInline::Strong(LinkStrong::new(alloc::vec![link_text("b")])),
            link_text(" c"),
        ],
    ));
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![paragraph_of(alloc::vec![expected])])
    );
    assert_eq!(
        markdown(&docx),
        "[a **b** c](https://outer.example \"Tip\")\n"
    );
}

#[test]
fn an_image_part_named_twice_is_exported_once() {
    let mut fixture = Fixture::new();
    let id = fixture.part(
        "/word/media/image1.png",
        "image/png",
        &RelationshipType::IMAGE,
        PNG,
    );
    let docx = fixture.body(alloc::vec![
        plain(alloc::vec![picture(PictureSource::Embedded(&id), 1, "A")]),
        plain(alloc::vec![picture(PictureSource::Embedded(&id), 2, "B")]),
    ]);
    let result = exported(&docx, MarkdownOptions::default());
    assert_eq!(result.images, [("image1.png".to_owned(), PNG.to_vec())]);
    assert_eq!(
        result.document.body.sections,
        no_heading(alloc::vec![
            paragraph_of(alloc::vec![image("report_media/image1.png", "A")]),
            paragraph_of(alloc::vec![image("report_media/image1.png", "B")]),
        ])
    );
}

// XML input: the typed blip keeps asvg:svgBlip in a:ext as unknown content, and reading it from there is the behavior.
#[test]
fn a_picture_with_an_svg_extension_exports_the_svg_part_and_not_the_png() {
    let mut fixture = Fixture::new();
    let png = fixture.part(
        "/word/media/image1.png",
        "image/png",
        &RelationshipType::IMAGE,
        PNG,
    );
    let svg = fixture.part(
        "/word/media/image2.svg",
        "image/svg+xml",
        &RelationshipType::IMAGE,
        b"<svg/>",
    );
    let blip = format!(
        r#"<a:blip r:embed="{png}"><a:extLst><a:ext uri="{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}"><asvg:svgBlip r:embed="{svg}"/></a:ext></a:extLst></a:blip>"#
    );
    let body = format!(
        "<w:p><w:r>{}</w:r></w:p>",
        picture_drawing_xml("Chart", &blip)
    );
    let result = exported(&fixture.body_xml(&body), MarkdownOptions::default());
    assert_eq!(
        result.images,
        [("image2.svg".to_owned(), b"<svg/>".to_vec())]
    );
    assert_eq!(
        result.document.body.sections,
        no_heading(alloc::vec![paragraph_of(alloc::vec![image(
            "report_media/image2.svg",
            "Chart"
        )])])
    );
}

#[test]
fn a_linked_picture_is_an_image_of_its_address() {
    let mut fixture = Fixture::new();
    let id = fixture.external("https://example.com/logo.png", &RelationshipType::IMAGE);
    let docx = fixture.body(alloc::vec![plain(alloc::vec![picture(
        PictureSource::Linked(&id),
        1,
        "Logo"
    )])]);
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![paragraph_of(alloc::vec![image(
            "https://example.com/logo.png",
            "Logo"
        )])])
    );
}

// XML input: which mc:AlternateContent branch is read is the behavior, and the typed tree keeps the branches as unknown content.
#[test]
fn a_picture_inside_alternate_content_is_read_from_the_first_branch_that_holds_one() {
    let mut fixture = Fixture::new();
    let mut add = |name: &str| fixture.part(name, "image/png", &RelationshipType::IMAGE, PNG);
    let first = add("/word/media/image1.png");
    let second = add("/word/media/image2.png");
    let unused = add("/word/media/image3.png");
    let blip = |id: &str| format!(r#"<a:blip r:embed="{id}"/>"#);
    let text_box = format!(
        "<w:drawing><wp:anchor><a:graphic><a:graphicData><wps:wsp><wps:txbx>{BOXED}</wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing>"
    );
    let body = format!(
        concat!(
            r#"<w:p><w:r><mc:AlternateContent><mc:Choice Requires="wps">{}</mc:Choice>"#,
            "<mc:Fallback>{}</mc:Fallback></mc:AlternateContent></w:r></w:p>",
            r#"<w:p><w:r><mc:AlternateContent><mc:Choice Requires="wps">{}</mc:Choice>"#,
            "<mc:Fallback>{}</mc:Fallback></mc:AlternateContent></w:r></w:p>",
        ),
        picture_drawing_xml("first", &blip(&first)),
        picture_drawing_xml("unused", &blip(&unused)),
        text_box,
        picture_drawing_xml("second", &blip(&second)),
    );
    let result = exported(&fixture.body_xml(&body), MarkdownOptions::default());
    assert_eq!(
        result.images,
        [
            ("image1.png".to_owned(), PNG.to_vec()),
            ("image2.png".to_owned(), PNG.to_vec()),
        ]
    );
    assert_eq!(
        result.document.body.sections,
        no_heading(alloc::vec![
            paragraph_of(alloc::vec![image("report_media/image1.png", "first")]),
            paragraph_of(alloc::vec![image("report_media/image2.png", "second")]),
            paragraph_of(alloc::vec![text("Boxed")]),
        ])
    );
}

#[test]
fn an_alt_chunk_naming_an_html_part_is_an_html_block_of_its_body() {
    let mut fixture = Fixture::new();
    let mut chunk = AnchorForImportedExternalContent::default();
    chunk.id = Some(fixture.part(
        "/word/htmlChunk1.html",
        "text/html",
        &RelationshipType::A_F_CHUNK,
        b"<html><BODY lang=\"es\"><div>hi</div></BODY></html>",
    ));
    let docx = fixture.body(alloc::vec![
        paragraph("Normal", alloc::vec![run("before")]),
        BlockContent::AnchorForImportedExternalContent(chunk),
    ]);
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![
            paragraph_of(alloc::vec![text("before")]),
            Block::Html(Html::new("<div>hi</div>".to_owned())),
        ])
    );
}

#[test]
fn the_frontmatter_part_is_the_frontmatter_only_when_asked_and_one_that_does_not_read_fails() {
    let with_part = |toml: &[u8]| {
        let mut docx = word(alloc::vec![paragraph("Normal", alloc::vec![run("Hola")])]);
        let name = PartName::new("/docProps/frontmatter.toml").unwrap();
        let package = docx.package_mut();
        package
            .set_part_bytes(&name, "application/toml", toml)
            .unwrap();
        package
            .relate(None, &name, FRONTMATTER_RELATIONSHIP)
            .unwrap();
        docx
    };
    let asked = MarkdownOptions {
        embed_images: false,
        frontmatter: true,
    };
    let docx = with_part(b"title = \"Plan\"\n");
    let mut mapping = Mapping::new();
    mapping.insert("title".to_owned(), Value::String("Plan".to_owned()));
    assert_eq!(exported(&docx, asked).document.frontmatter, Some(mapping));
    assert_eq!(
        exported(&docx, MarkdownOptions::default())
            .document
            .frontmatter,
        None
    );
    let broken = with_part(b"a = [\n");
    assert!(matches!(
        broken.to_arky("report_media", asked),
        Err(MarkdownError::Frontmatter(_))
    ));
    assert_eq!(
        broken
            .to_arky("report_media", MarkdownOptions::default())
            .unwrap()
            .document
            .frontmatter,
        None
    );
}

/// A VML `v:shape` of alt text `alt` whose `v:imagedata` names the relationship `id`.
fn vml_picture_xml(alt: &str, id: &str) -> String {
    format!(r#"<v:shape alt="{alt}"><v:imagedata r:id="{id}"/></v:shape>"#)
}

#[test]
fn a_vml_picture_in_a_pict_or_an_object_is_an_image_with_the_shape_s_alt_text() {
    let mut fixture = Fixture::new();
    let id = fixture.part(
        "/word/media/image1.png",
        "image/png",
        &RelationshipType::IMAGE,
        PNG,
    );
    let mut object = EmbeddedObject::default();
    object.vml_element_content = alloc::vec![vml_picture("Sheet", &id)];
    let docx = fixture.body(alloc::vec![
        plain(alloc::vec![pict("Company logo", &id)]),
        plain(alloc::vec![run_holding(alloc::vec![
            TextRunContent::EmbeddedObject(object)
        ])]),
    ]);
    let result = exported(&docx, MarkdownOptions::default());
    assert_eq!(result.images, [("image1.png".to_owned(), PNG.to_vec())]);
    assert_eq!(
        result.document.body.sections,
        no_heading(alloc::vec![
            paragraph_of(alloc::vec![image(
                "report_media/image1.png",
                "Company logo"
            )]),
            paragraph_of(alloc::vec![image("report_media/image1.png", "Sheet")]),
        ])
    );
}

#[test]
fn a_vml_picture_of_an_external_relationship_is_an_image_of_its_address() {
    let mut fixture = Fixture::new();
    let id = fixture.external("https://example.com/logo.png", &RelationshipType::IMAGE);
    let docx = fixture.body(alloc::vec![plain(alloc::vec![pict("Logo", &id)])]);
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![paragraph_of(alloc::vec![image(
            "https://example.com/logo.png",
            "Logo"
        )])])
    );
}

// XML input: which mc:AlternateContent branch is read is the behavior, and the typed tree keeps the branches as unknown content.
#[test]
fn a_drawing_in_the_choice_wins_over_a_vml_picture_in_the_fallback() {
    let mut fixture = Fixture::new();
    let mut add = |name: &str| fixture.part(name, "image/png", &RelationshipType::IMAGE, PNG);
    let drawn = add("/word/media/image1.png");
    let vml = add("/word/media/image2.png");
    let body = format!(
        r#"<w:p><w:r><mc:AlternateContent><mc:Choice Requires="wps">{}</mc:Choice><mc:Fallback><w:pict>{}</w:pict></mc:Fallback></mc:AlternateContent></w:r></w:p>"#,
        picture_drawing_xml("drawn", &format!(r#"<a:blip r:embed="{drawn}"/>"#)),
        vml_picture_xml("vml", &vml),
    );
    let result = exported(&fixture.body_xml(&body), MarkdownOptions::default());
    assert_eq!(result.images, [("image1.png".to_owned(), PNG.to_vec())]);
    assert_eq!(
        result.document.body.sections,
        no_heading(alloc::vec![paragraph_of(alloc::vec![image(
            "report_media/image1.png",
            "drawn"
        )])])
    );
}

#[test]
fn a_hyperlink_field_s_l_switch_names_a_bookmark_or_an_address_s_fragment() {
    let content = [
        complex_field(r#" HYPERLINK \l "q3_results" "#, "Q3"),
        alloc::vec![
            text_run(", "),
            simple_field(r#" HYPERLINK \l "_2024_plan" "#, "plan"),
            text_run(", "),
            simple_field(r#" HYPERLINK "https://x.example" \l "part_two" "#, "part"),
            text_run(", "),
            simple_field(r#" HYPERLINK \o "tip" "#, "plain"),
        ],
    ]
    .concat();
    assert_eq!(
        markdown(&word(alloc::vec![block(content)])),
        "[Q3](#q3-results), [plan](#2024-plan), [part](https://x.example#part_two), plain\n"
    );
}

#[test]
fn a_bold_stretch_next_to_punctuation_comes_back_bold() {
    let docx = word(alloc::vec![
        plain(alloc::vec![run("MONTO SOLICITUD"), bold_run(":")]),
        plain(alloc::vec![run("a"), bold_run("(bold)"), run("b")]),
        plain(alloc::vec![bold_run("bold."), run("b")]),
    ]);
    assert_eq!(
        arky(&docx).body.sections,
        no_heading(alloc::vec![
            paragraph_of(alloc::vec![
                text("MONTO SOLICITUD"),
                strong(alloc::vec![text(":")])
            ]),
            paragraph_of(alloc::vec![
                text("a"),
                strong(alloc::vec![text("(bold)")]),
                text("b")
            ]),
            paragraph_of(alloc::vec![strong(alloc::vec![text("bold.")]), text("b")]),
        ])
    );
}
