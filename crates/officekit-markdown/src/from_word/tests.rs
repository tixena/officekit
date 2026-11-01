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
use officekit::parts::kinds::WordprocessingKind;
use officekit::parts::numbering_definitions;
use officekit::parts::style_definitions;
use officekit_markup::common_simple_types::{OnOffValue, XMLName};
use officekit_markup::math::{
    OfficeMath, OfficeMathContent, Run as MathRun, RunContent as MathRunContent, Text as MathText,
};
use officekit_markup::wordprocessing::{
    self, AbstractNumberingDefinition, AbstractNumberingDefinitionReference,
    AssociatedOutlineLevel, BlockContent, BlockLevelStructuredDocumentTag,
    BlockLevelStructuredDocumentTagContent, Bold, BorderStyles, Break, BreakTypes, CarriageReturn,
    ColorValue, ComplexFieldCharacter, ComplexFieldCharacterType, ContentBlockContent,
    DecimalNumberValue, DeletedRunContent, DeletedText, DocumentBody, FieldCode,
    InlineLevelCustomXMLElement, InlineLevelSmartTag, InlineLevelStructuredDocumentTag,
    InlineLevelStructuredDocumentTagContent, InsertedRunContent, Italics,
    MeasurementInEighthsOfAPoint, MeasurementInPoints, MoveDestinationRunContent,
    MoveSourceRunContent, NumberingDefinitionInstance, NumberingDefinitionInstanceReferenceNumId,
    NumberingDefinitionInstanceReferenceNumPr, NumberingDefinitions, NumberingFormat,
    NumberingLevelOverrideDefinition, NumberingLevelReference,
    ParagraphBorderBelowIdenticalParagraphs, ParagraphBorders, ParagraphContent,
    ParagraphIndentation, ParagraphProperties, ParagraphPropertiesPPrGeneral, ParentStyleID,
    PrimaryStyleName, ReferencedCharacterStyle, ReferencedParagraphStyle, RunFonts, RunProperties,
    RunPropertiesContent, SignedMeasurementInTwentiethsOfPoint, SimpleField, StNumberFormat,
    StartingValue, StyleDefinition, StyleDefinitions, StyleTypes, TabCharacter, TextRun,
    TextRunContent, TrackedRunContent,
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

/// The numbering part of the tests: instance 1 is bullets, instance 2 decimal numbers from 1
/// whose levels' text starts at 720 and 1440, instance 3 a level of format `none`, and instance 4
/// decimal numbers from 3 by a `w:startOverride`.
const NUMBERING: &str = concat!(
    r#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
    r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/></w:lvl>"#,
    r#"<w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum>"#,
    r#"<w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1)"/>"#,
    r#"<w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl>"#,
    r#"<w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:pPr><w:ind w:left="1440" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum>"#,
    r#"<w:abstractNum w:abstractNumId="2"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="none"/></w:lvl></w:abstractNum>"#,
    r#"<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#,
    r#"<w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>"#,
    r#"<w:num w:numId="3"><w:abstractNumId w:val="2"/></w:num>"#,
    r#"<w:num w:numId="4"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="3"/></w:lvlOverride></w:num>"#,
    "</w:numbering>"
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

    /// Add the numbering part [`NUMBERING`].
    fn numbering(&mut self) {
        self.part(
            "/word/numbering.xml",
            numbering_definitions::CONTENT_TYPE,
            &numbering_definitions::RELATIONSHIP_TYPE,
            NUMBERING.as_bytes(),
        );
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

/// A paragraph of the style `style` holding `runs`.
fn paragraph(style: &str, runs: Vec<TextRun>) -> BlockContent {
    let mut reference = ReferencedParagraphStyle::default();
    reference.val = Some(style.to_owned());
    let mut properties = ParagraphProperties::default();
    properties.referenced_paragraph_style = Some(Box::new(reference));
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

/// A new Word document whose body holds `blocks` and whose styles part holds [`styles`].
fn word(blocks: Vec<BlockContent>) -> Docx {
    Fixture::new().body(blocks)
}

fn bold_run_xml(text: &str) -> String {
    format!(r#"<w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
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

/// A paragraph of the run of `text`, numbered by `instance` at `level`.
fn numbered_xml(instance: u8, level: u8, text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="{instance}"/></w:numPr></w:pPr>{}</w:p>"#,
        run_xml(text)
    )
}

fn paragraph_xml(style: &str, runs: &str) -> String {
    format!(r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr>{runs}</w:p>"#)
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

fn run_xml(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

fn strong(children: Vec<Inline>) -> Inline {
    Inline::Strong(Strong::new(children))
}

/// A table of `rows`.
fn table_xml(rows: &[String]) -> String {
    format!("<w:tbl><w:tblPr/><w:tblGrid/>{}</w:tbl>", rows.concat())
}

/// A table cell of `properties`, the inside of its `w:tcPr`, holding a paragraph of the run of
/// `text`.
fn table_cell_xml(properties: &str, text: &str) -> String {
    format!(
        "<w:tc><w:tcPr>{properties}</w:tcPr><w:p>{}</w:p></w:tc>",
        run_xml(text)
    )
}

/// A table row of `cells`, marked `w:tblHeader` when `head` is set.
fn table_row_xml(head: bool, cells: &[String]) -> String {
    let properties = if head {
        "<w:trPr><w:tblHeader/></w:trPr>"
    } else {
        ""
    };
    format!("<w:tr>{properties}{}</w:tr>", cells.concat())
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
    let mut bottom = ParagraphBorderBelowIdenticalParagraphs::default();
    bottom.color = Some(ColorValue::from("auto".to_owned()));
    bottom.space = Some(MeasurementInPoints::from("1".to_owned()));
    bottom.sz = Some(MeasurementInEighthsOfAPoint::from("6".to_owned()));
    bottom.val = Some(BorderStyles::from("single".to_owned()));
    let mut borders = ParagraphBorders::default();
    borders.paragraph_border_below_identical_paragraphs = Some(Box::new(bottom));
    let mut bordered = ParagraphProperties::default();
    bordered.paragraph_borders = Some(Box::new(borders));
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
    let body = concat!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol/><w:gridCol/></w:tblGrid><w:tr>",
        "<w:tc><w:p><w:r><w:t>left</w:t></w:r></w:p></w:tc>",
        r#"<w:tc><w:p><w:hyperlink w:anchor="x"><w:r><w:t>right</w:t></w:r></w:hyperlink></w:p></w:tc>"#,
        "</w:tr></w:tbl>",
    );
    assert_eq!(
        markdown(&word_xml(body)),
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
    let link_id = fixture.external("https://example.com/q3", &RelationshipType::HYPERLINK);
    let rows = [
        table_row_xml(false, &[table_cell_xml("", "a"), table_cell_xml("", "b")]),
        table_row_xml(false, &[table_cell_xml("", "c"), table_cell_xml("", "d")]),
    ];
    let body = format!(
        r#"{}{}{}<w:p>{}<w:hyperlink r:id="{link_id}">{}</w:hyperlink>{}</w:p><w:p>{}</w:p>"#,
        numbered_xml(2, 0, "one"),
        numbered_xml(2, 0, "two"),
        table_xml(&rows),
        run_xml("See "),
        run_xml("the dashboard"),
        run_xml("."),
        picture_xml("Logo", &picture_id),
    );
    fixture.body_xml(&body)
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
    let body = [
        paragraph_xml("Numbered", &run_xml("styled")),
        numbered_xml(3, 0, "hidden"),
    ]
    .concat();
    let docx = fixture.body_xml(&body);
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
    let body = [
        numbered_xml(1, 0, "a"),
        numbered_xml(1, 1, "b"),
        numbered_xml(1, 0, "c"),
        numbered_xml(4, 0, "three"),
    ]
    .concat();
    let docx = fixture.body_xml(&body);
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
    let body = [
        numbered_xml(2, 0, "one"),
        numbered_xml(2, 1, "inner"),
        numbered_xml(2, 1, "inner"),
        numbered_xml(2, 0, "two"),
        numbered_xml(2, 1, "again"),
        paragraph_xml("Normal", &run_xml("between")),
        numbered_xml(2, 0, "three"),
    ]
    .concat();
    let docx = fixture.body_xml(&body);
    assert_eq!(
        markdown(&docx),
        "1. one\n   1. inner\n   2. inner\n2. two\n   1. again\n\nbetween\n\n3. three\n"
    );
}

#[test]
fn paragraphs_that_write_nothing_leave_the_items_of_one_word_list_in_one_list() {
    let mut fixture = Fixture::new();
    fixture.numbering();
    let body = [
        numbered_xml(2, 0, "one"),
        "<w:p/>".to_owned(),
        numbered_xml(2, 1, "inner"),
        paragraph_xml("Ttulo1", ""),
        numbered_xml(2, 0, "two"),
        "<w:p/>".to_owned(),
        numbered_xml(1, 0, "bullet"),
    ]
    .concat();
    let docx = fixture.body_xml(&body);
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

/// An indentation whose `w:start` is `start`.
fn indentation(start: &str) -> Box<ParagraphIndentation> {
    let mut indentation = ParagraphIndentation::default();
    indentation.start = Some(SignedMeasurementInTwentiethsOfPoint::from(start.to_owned()));
    Box::new(indentation)
}

/// Paragraph properties indented to `start`.
fn indented_to(start: &str) -> ParagraphProperties {
    let mut properties = ParagraphProperties::default();
    properties.paragraph_indentation = Some(indentation(start));
    properties
}

/// Paragraph properties numbering a paragraph at `level` of the numbering instance 1.
fn numbered_at(level: usize) -> ParagraphProperties {
    let mut level_reference = NumberingLevelReference::default();
    level_reference.val = Some(decimal(level));
    let mut instance = NumberingDefinitionInstanceReferenceNumId::default();
    instance.val = Some(decimal(1));
    let mut numbering = NumberingDefinitionInstanceReferenceNumPr::default();
    numbering.numbering_level_reference = Some(Box::new(level_reference));
    numbering.numbering_definition_instance_reference_num_id = Some(Box::new(instance));
    let mut properties = ParagraphProperties::default();
    properties.numbering_definition_instance_reference_num_pr = Some(Box::new(numbering));
    properties
}

/// Numbering instance 1: decimal numbers from 1, at one level per entry of `starts`, each level's
/// text starting at that indent.
fn numbering_starting_at(starts: &[&str]) -> NumberingDefinitions {
    let mut definition = AbstractNumberingDefinition::default();
    definition.abstract_num_id = Some(decimal(0));
    for (level, start) in starts.iter().enumerate() {
        let mut format = NumberingFormat::default();
        format.val = Some(StNumberFormat::from("decimal".to_owned()));
        let mut first = StartingValue::default();
        first.val = Some(decimal(1));
        let mut general = ParagraphPropertiesPPrGeneral::default();
        general.paragraph_indentation = Some(indentation(start));
        let mut defined = NumberingLevelOverrideDefinition::default();
        defined.ilvl = Some(decimal(level));
        defined.numbering_format = Some(Box::new(format));
        defined.starting_value = Some(Box::new(first));
        defined.paragraph_properties_p_pr_general = Some(Box::new(general));
        definition.numbering_level_override_definition.push(defined);
    }
    let mut reference = AbstractNumberingDefinitionReference::default();
    reference.val = Some(decimal(0));
    let mut instance = NumberingDefinitionInstance::default();
    instance.num_id = Some(decimal(1));
    instance.abstract_numbering_definition_reference = Box::new(reference);
    let mut numbering = NumberingDefinitions::default();
    numbering.abstract_numbering_definition.push(definition);
    numbering.numbering_definition_instance.push(instance);
    numbering
}

/// A paragraph of the run of `value`, with `properties`.
fn typed_paragraph(value: &str, properties: ParagraphProperties) -> wordprocessing::Paragraph {
    let mut text = wordprocessing::Text::default();
    text.value = value.to_owned();
    let mut run = TextRun::default();
    run.content.push(TextRunContent::Text(text));
    let mut paragraph = wordprocessing::Paragraph::default();
    paragraph.properties = Some(Box::new(properties));
    paragraph.content.push(ParagraphContent::TextRun(run));
    paragraph
}

/// A Word document of `paragraphs` whose numbering part holds `numbering`, built as a caller
/// builds one.
fn typed_word(
    paragraphs: Vec<wordprocessing::Paragraph>,
    numbering: &NumberingDefinitions,
) -> Docx {
    let mut body = DocumentBody::default();
    body.content = paragraphs
        .into_iter()
        .map(BlockContent::Paragraph)
        .collect();
    let mut document = wordprocessing::Document::default();
    document.body = Some(Box::new(body));
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    docx.set_main_document(&document).unwrap();
    let main = docx.main_document_part().clone();
    let part = PartName::new("/word/numbering.xml").unwrap();
    numbering_definitions::store_transitional(
        docx.package_mut(),
        &part,
        numbering,
        XmlWriteOptions::default(),
    )
    .unwrap();
    docx.package_mut()
        .relate(
            Some(&main),
            &part,
            numbering_definitions::RELATIONSHIP_TYPE.transitional,
        )
        .unwrap();
    docx
}

#[test]
fn an_indent_with_a_unit_finds_the_list_item_the_same_number_of_twips_finds() {
    let document = |indent: fn(u16) -> String| {
        typed_word(
            alloc::vec![
                typed_paragraph("one", numbered_at(0)),
                typed_paragraph("inner", numbered_at(1)),
                typed_paragraph("under inner", indented_to(&indent(1440))),
                typed_paragraph("under one", indented_to(&indent(720))),
                typed_paragraph("two", numbered_at(0)),
                typed_paragraph("elsewhere", indented_to(&indent(360))),
                typed_paragraph("three", numbered_at(0)),
            ],
            &numbering_starting_at(&[&indent(720), &indent(1440)]),
        )
    };
    let bare = arky(&document(|twips| twips.to_string()));
    let points = arky(&document(|twips| format!("{}pt", twips / 20)));
    let inner = Block::List(List::new(
        alloc::vec![item(alloc::vec![
            paragraph_of(alloc::vec![text("inner")]),
            paragraph_of(alloc::vec![text("under inner")]),
        ])],
        true,
        Some(1),
    ));
    assert_eq!(
        points.body.sections,
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
    );
    assert_eq!(points.body.sections, bare.body.sections);
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
    let mut fixture = Fixture::new();
    fixture.numbering();
    let indented = |left: u16, text: &str| {
        format!(
            r#"<w:p><w:pPr><w:ind w:left="{left}" w:firstLine="0"/></w:pPr>{}</w:p>"#,
            run_xml(text)
        )
    };
    let body = [
        numbered_xml(2, 0, "one"),
        numbered_xml(2, 1, "inner"),
        indented(1440, "under inner"),
        indented(720, "under one"),
        numbered_xml(2, 0, "two"),
        indented(360, "elsewhere"),
        numbered_xml(2, 0, "three"),
    ]
    .concat();
    let docx = fixture.body_xml(&body);
    let inner = Block::List(List::new(
        alloc::vec![item(alloc::vec![
            paragraph_of(alloc::vec![text("inner")]),
            paragraph_of(alloc::vec![text("under inner")]),
        ])],
        true,
        Some(1),
    ));
    assert_eq!(
        arky(&docx).body.sections,
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
    );
}

#[test]
fn a_list_paragraph_with_no_numbering_after_a_list_paragraph_is_a_later_block_of_its_item() {
    let mut fixture = Fixture::new();
    fixture.numbering();
    let body = [
        numbered_xml(2, 0, "one"),
        paragraph_xml("ListParagraph", &run_xml("more")),
        numbered_xml(2, 0, "two"),
        paragraph_xml("Normal", &run_xml("after")),
    ]
    .concat();
    let docx = fixture.body_xml(&body);
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
    let mut fixture = Fixture::new();
    fixture.numbering();
    let indented = |style: &str, text: &str| {
        format!(
            r#"<w:p><w:pPr><w:pStyle w:val="{style}"/><w:ind w:left="720"/></w:pPr>{}</w:p>"#,
            run_xml(text)
        )
    };
    let body = [
        numbered_xml(1, 0, "item"),
        indented("HTMLPreformatted", "let a = 1;"),
        indented("HTMLPreformatted", "let b = 2;"),
        indented("Quote", "quoted"),
        concat!(
            r#"<w:p><w:pPr><w:pBdr><w:bottom w:val="single" w:sz="6" w:space="1" w:color="auto"/>"#,
            r#"</w:pBdr><w:ind w:left="720"/></w:pPr></w:p>"#,
        )
        .to_owned(),
        numbered_xml(1, 0, "next"),
        paragraph_xml("HTMLPreformatted", &run_xml("outside")),
    ]
    .concat();
    let docx = fixture.body_xml(&body);
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
    let body = [
        numbered_xml(2, 0, "outer"),
        numbered_xml(1, 1, "inner"),
        r#"<w:p><w:pPr><w:pStyle w:val="HTMLPreformatted"/><w:ind w:left="720"/></w:pPr><w:r><w:t>code</w:t></w:r></w:p>"#.to_owned(),
    ]
    .concat();
    let docx = fixture.body_xml(&body);
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
    let aligned = |jc: &str, text: &str| {
        format!(
            r#"<w:tc><w:p><w:pPr><w:jc w:val="{jc}"/></w:pPr>{}</w:p></w:tc>"#,
            run_xml(text)
        )
    };
    let rows = [
        table_row_xml(true, &[aligned("center", "h1"), aligned("end", "h2")]),
        table_row_xml(false, &[table_cell_xml("", "a"), table_cell_xml("", "b")]),
    ];
    let docx = word_xml(&table_xml(&rows));
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
    let two_heads = [
        table_row_xml(true, &[table_cell_xml("", "h1")]),
        table_row_xml(true, &[table_cell_xml("", "h2")]),
        table_row_xml(false, &[table_cell_xml("", "a")]),
    ];
    let code = r#"<w:tc><w:p><w:pPr><w:pStyle w:val="HTMLPreformatted"/></w:pPr><w:r><w:t>let a = 1;</w:t></w:r></w:p></w:tc>"#;
    let code_cell = [
        table_row_xml(true, &[table_cell_xml("", "h")]),
        table_row_xml(false, &[code.to_owned()]),
    ];
    let docx = word_xml(&[table_xml(&two_heads), table_xml(&code_cell)].concat());
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
    let two = "<w:tc><w:p><w:r><w:t>x</w:t></w:r></w:p><w:p><w:r><w:t>y</w:t></w:r></w:p></w:tc>";
    let rows = [
        table_row_xml(true, &[table_cell_xml("", "h")]),
        table_row_xml(false, &[two.to_owned()]),
    ];
    let docx = word_xml(
        &[
            table_xml(&[table_row_xml(false, &[table_cell_xml("", "a")])]),
            table_xml(&rows),
        ]
        .concat(),
    );
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
    let rows = [
        table_row_xml(
            false,
            &[
                table_cell_xml(r#"<w:gridSpan w:val="2"/>"#, "wide"),
                table_cell_xml("", "c"),
            ],
        ),
        table_row_xml(
            false,
            &[
                table_cell_xml(r#"<w:vMerge w:val="restart"/>"#, "x"),
                table_cell_xml("", "y"),
                table_cell_xml("", "z"),
            ],
        ),
        table_row_xml(
            false,
            &[
                table_cell_xml("<w:vMerge/>", "under"),
                table_cell_xml("", "q"),
                table_cell_xml("", "r"),
            ],
        ),
    ];
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
        arky(&word_xml(&table_xml(&rows))).body.sections,
        no_heading(alloc::vec![Block::Table(expected)])
    );
}

#[test]
fn anchors_lose_their_underscores_and_hyperlink_fields_are_links() {
    let body = format!(
        concat!(
            r#"<w:p><w:hyperlink w:anchor="q3_results">{}</w:hyperlink>{}"#,
            r#"<w:hyperlink w:anchor="_2024_plan">{}</w:hyperlink></w:p>"#,
            r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r>"#,
            r#"<w:r><w:instrText xml:space="preserve"> HYPERLINK "https://x.example" </w:instrText></w:r>"#,
            r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r>{}"#,
            r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>{}"#,
            r#"<w:fldSimple w:instr=" HYPERLINK &quot;https://y.example&quot; ">{}</w:fldSimple></w:p>"#,
        ),
        run_xml("Q3"),
        run_xml(" and "),
        run_xml("plan"),
        run_xml("x"),
        run_xml(" or "),
        run_xml("y"),
    );
    let docx = word_xml(&body);
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
    let outer = fixture.external("https://outer.example", &RelationshipType::HYPERLINK);
    let inner = fixture.external("https://inner.example", &RelationshipType::HYPERLINK);
    let body = format!(
        r#"<w:p><w:hyperlink r:id="{outer}" w:tooltip="Tip">{}{}<w:hyperlink r:id="{inner}">{}</w:hyperlink></w:hyperlink></w:p>"#,
        run_xml("a "),
        bold_run_xml("b"),
        run_xml(" c"),
    );
    let docx = fixture.body_xml(&body);
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
    let body = format!(
        "<w:p>{}</w:p><w:p>{}</w:p>",
        picture_xml("A", &id),
        picture_xml("B", &id)
    );
    let result = exported(&fixture.body_xml(&body), MarkdownOptions::default());
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
    let blip = format!(r#"<a:blip r:link="{id}"/>"#);
    let body = format!(
        "<w:p><w:r>{}</w:r></w:p>",
        picture_drawing_xml("Logo", &blip)
    );
    assert_eq!(
        arky(&fixture.body_xml(&body)).body.sections,
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
    let id = fixture.part(
        "/word/htmlChunk1.html",
        "text/html",
        &RelationshipType::A_F_CHUNK,
        b"<html><BODY lang=\"es\"><div>hi</div></BODY></html>",
    );
    let body = format!(
        r#"{}<w:altChunk r:id="{id}"/>"#,
        paragraph_xml("Normal", &run_xml("before"))
    );
    let docx = fixture.body_xml(&body);
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
fn vml_picture(alt: &str, id: &str) -> String {
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
    let body = format!(
        "<w:p><w:r><w:pict>{}</w:pict></w:r></w:p><w:p><w:r><w:object>{}</w:object></w:r></w:p>",
        vml_picture("Company logo", &id),
        vml_picture("Sheet", &id),
    );
    let result = exported(&fixture.body_xml(&body), MarkdownOptions::default());
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
    let body = format!(
        "<w:p><w:r><w:pict>{}</w:pict></w:r></w:p>",
        vml_picture("Logo", &id)
    );
    assert_eq!(
        arky(&fixture.body_xml(&body)).body.sections,
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
        vml_picture("vml", &vml),
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
    let body = format!(
        concat!(
            r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r>"#,
            r#"<w:r><w:instrText xml:space="preserve"> HYPERLINK \l "q3_results" </w:instrText></w:r>"#,
            r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r>{}"#,
            r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>{}"#,
            r#"<w:fldSimple w:instr=" HYPERLINK \l &quot;_2024_plan&quot; ">{}</w:fldSimple>{}"#,
            r#"<w:fldSimple w:instr=" HYPERLINK &quot;https://x.example&quot; \l &quot;part_two&quot; ">{}</w:fldSimple>{}"#,
            r#"<w:fldSimple w:instr=" HYPERLINK \o &quot;tip&quot; ">{}</w:fldSimple></w:p>"#,
        ),
        run_xml("Q3"),
        run_xml(", "),
        run_xml("plan"),
        run_xml(", "),
        run_xml("part"),
        run_xml(", "),
        run_xml("plain"),
    );
    assert_eq!(
        markdown(&word_xml(&body)),
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
