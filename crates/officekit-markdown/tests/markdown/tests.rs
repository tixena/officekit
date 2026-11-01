use std::path::Path;

use arky::parser::parse;
use arky::writer::{WriteOptions, write};
use officekit::docx::Docx;
use officekit::docx::image::{PictureSource, inline_picture};
use officekit::docx::table::TableRows as _;
use officekit::parts::kinds::WordprocessingKind;
use officekit::parts::{numbering_definitions, style_definitions};
use officekit::validation::validator::Validator;
use officekit_markdown::{FRONTMATTER_RELATIONSHIP, MarkdownOptions, from_markdown, save_markdown};
use officekit_markup::common_simple_types::OnOffValue;
use officekit_markup::vml::{
    ImageData, ShapeDefinition, ShapeDefinitionContent, TextBox, TextBoxContent,
};
use officekit_markup::wordprocessing::{
    AbstractNumberingDefinition, AbstractNumberingDefinitionReference,
    AnchorForImportedExternalContent, BlockContent, Bold, BorderStyles, Break, BreakTypes,
    ColorValue, ComplexFieldCharacter, ComplexFieldCharacterType, ContentCellContent,
    ContentRowContent, DecimalNumberValue, Document, DocumentBody, FieldCode, Hyperlink, Italics,
    MeasurementInEighthsOfAPoint, MeasurementInPoints, NumberingDefinitionInstance,
    NumberingDefinitionInstanceReferenceNumId, NumberingDefinitionInstanceReferenceNumPr,
    NumberingDefinitions, NumberingFormat, NumberingLevelDefinitionOverride,
    NumberingLevelOverrideDefinition, NumberingLevelReference, NumberingLevelStartingValueOverride,
    Paragraph, ParagraphBorderBelowIdenticalParagraphs, ParagraphBorders, ParagraphContent,
    ParagraphProperties, PrimaryStyleName, ReferencedCharacterStyle, ReferencedParagraphStyle,
    RichTextBoxContentContainer, RunProperties, RunPropertiesContent, SimpleField, StNumberFormat,
    StartingValue, StyleDefinition, StyleDefinitions, StyleTypes, TabCharacter, Table, TableCell,
    TableRow, Text as WordText, TextRun, TextRunContent, VMLObject, VmlElementContent,
};
use officekit_xml::name::OfficeVersion;
use officekit_xml::packaging::package::OpenOptions;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
use officekit_xml::ready::ready_now;
use officekit_xml::write::WriteOptions as XmlWriteOptions;
use os_shim::System as _;
use os_shim::bucket::Condition;
use os_shim::mock::MemorySystem;
use png::{BitDepth, ColorType, Encoder};

const SOURCE: &str = "+++\ntitle = \"Quarterly report\"\n+++\n# Results\n\nRevenue grew **12%** in *Q3*, see [the dashboard](https://example.com/q3) and [below](#notes).\n\n- North\n- South\n\n1. one\n2. two\n\n| Region | Revenue |\n| :----- | ------: |\n| North  | 1,200   |\n\n```\nfirst\nsecond\n```\n\n> Quoted.\n\n***\n\n## Notes\n\n<div align=\"center\">Signed</div>\n\n![Logo](logo.png)\n";

fn options() -> MarkdownOptions {
    let mut options = MarkdownOptions::default();
    options.frontmatter = true;
    options
}

fn tiny_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, 2, 2);
    encoder.set_color(ColorType::Rgb);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[200_u8; 12]).unwrap();
    writer.finish().unwrap();
    bytes
}

fn read_bytes(system: &MemorySystem, path: &str) -> Vec<u8> {
    ready_now(system.read(Path::new(path))).unwrap().0
}

fn assert_valid(docx: &Docx) {
    let errors = Validator::new(OfficeVersion::Microsoft365).validate(docx.package());
    assert!(errors.is_empty(), "{errors:?}");
}

/// A folder `/n` holding `source` as `doc.md` and the picture `logo.png`.
fn notes(source: &str) -> MemorySystem {
    let system = MemorySystem::new();
    ready_now(system.create_dir_all(Path::new("/n"))).unwrap();
    ready_now(system.write(Path::new("/n/doc.md"), source.as_bytes(), Condition::None)).unwrap();
    ready_now(system.write(Path::new("/n/logo.png"), &tiny_png(), Condition::None)).unwrap();
    system
}

/// The Word file of `/n/doc.md`, validated, saved, reopened and written as markdown to `back`.
fn through_word(system: &MemorySystem, back: &str, options: MarkdownOptions) -> String {
    let docx = ready_now(from_markdown(system, Path::new("/n/doc.md"), options)).unwrap();
    assert_valid(&docx);
    ready_now(docx.save_transitional(system, Path::new("/n/doc.docx"))).unwrap();
    let reopened = ready_now(Docx::open(
        system,
        Path::new("/n/doc.docx"),
        OpenOptions::default(),
    ))
    .unwrap();
    ready_now(save_markdown(&reopened, system, Path::new(back), options)).unwrap();
    ready_now(system.read_to_string(Path::new(back))).unwrap()
}

fn after_word(source: &str) -> String {
    through_word(&notes(source), "/n/back.md", MarkdownOptions::default())
}

#[test]
fn markdown_comes_back_from_word_as_written_but_for_the_picture_folder() {
    let system = notes(SOURCE);
    let back = through_word(&system, "/n/back.md", options());
    let expected = write(&parse(SOURCE).document, &WriteOptions::default())
        .unwrap()
        .replace("![Logo](logo.png)", "![Logo](back_media/image1.png)");
    assert_eq!(back, expected);
    assert_eq!(read_bytes(&system, "/n/back_media/image1.png"), tiny_png());
}

#[test]
fn a_code_block_loses_its_language_and_arguments() {
    assert_eq!(after_word("```rust\nx\n```\n"), "```\nx\n```\n");
    assert_eq!(after_word("```rust title=a\nx\n```\n"), "```\nx\n```\n");
}

#[test]
fn a_wiki_link_comes_back_as_a_markdown_link() {
    assert_eq!(after_word("[[target|text]]\n"), "[text](target)\n");
}

#[test]
fn a_loose_list_comes_back_tight_unless_an_item_holds_two_paragraphs() {
    assert_eq!(after_word("- a\n\n- b\n"), "- a\n- b\n");
    assert_eq!(after_word("- a\n\n  b\n\n- c\n"), "- a\n\n  b\n\n- c\n");
}

#[test]
fn a_later_paragraph_of_an_item_comes_back_inside_that_item_at_its_level() {
    let source = "1. one\n\n   under one\n\n   1. inner\n\n      under inner\n\n2. two\n";
    assert_eq!(after_word(source), source);
}

#[test]
fn a_simple_table_or_a_grid_table_of_one_head_row_of_one_line_cells_comes_back_as_a_pipe_table() {
    let pipe = "| A   | B   |\n| --- | --- |\n| 1   | 2   |\n";
    assert_eq!(after_word("A | B\n--|--\n1 | 2\n"), pipe);
    assert_eq!(
        after_word("+---+---+\n| A | B |\n+===+===+\n| 1 | 2 |\n+---+---+\n"),
        pipe
    );
}

#[test]
fn a_paragraph_comes_back_as_one_line() {
    assert_eq!(after_word("one\ntwo\n"), "one two\n");
}

#[test]
fn embedded_pictures_write_no_folder() {
    let system = notes("![Logo](logo.png)\n");
    let mut embed = MarkdownOptions::default();
    embed.embed_images = true;
    let back = through_word(&system, "/n/back.md", embed);
    assert!(back.starts_with("![Logo](data:image/png;base64,"), "{back}");
    assert!(!ready_now(system.exists(Path::new("/n/back_media"))).unwrap());
}

/// Add the part `name` of `content_type` holding `bytes`, related from `main` by
/// `relationship_type`; returns the relationship's id.
fn add_part(
    docx: &mut Docx,
    main: &PartName,
    name: &str,
    content_type: &str,
    relationship_type: &str,
    bytes: &[u8],
) -> String {
    let part = PartName::new(name).unwrap();
    let package = docx.package_mut();
    package.set_part_bytes(&part, content_type, bytes).unwrap();
    package
        .relate(Some(main), &part, relationship_type)
        .unwrap()
}

/// A paragraph holding `content`.
fn block(content: Vec<ParagraphContent>) -> BlockContent {
    let mut paragraph = Paragraph::default();
    paragraph.content = content;
    BlockContent::Paragraph(paragraph)
}

/// The text `value` of a run, its spaces kept.
fn characters(value: &str) -> TextRunContent {
    let mut characters = WordText::default();
    characters.space = Some("preserve".to_owned());
    characters.value = value.to_owned();
    TextRunContent::Text(characters)
}

/// A table cell of one paragraph per entry of `values`.
fn cell(values: &[&str]) -> TableCell {
    let mut cell = TableCell::default();
    cell.content = values.iter().map(|value| plain(value)).collect();
    cell
}

/// The runs of a complex field of `instruction` whose result is the run of `shown`.
fn complex_field(instruction: &str, shown: &str) -> Vec<ParagraphContent> {
    let character = |kind: &str| {
        let mut character = ComplexFieldCharacter::default();
        character.fld_char_type = Some(ComplexFieldCharacterType::from(kind.to_owned()));
        run_holding(vec![TextRunContent::ComplexFieldCharacter(character)])
    };
    let mut code = FieldCode::default();
    code.space = Some("preserve".to_owned());
    code.value = instruction.to_owned();
    [
        character("begin"),
        run_holding(vec![TextRunContent::FieldCode(code)]),
        character("separate"),
        run(shown),
        character("end"),
    ]
    .into_iter()
    .map(ParagraphContent::TextRun)
    .collect()
}

/// A paragraph of the run of `value`, numbered by `instance` at `level`.
fn listed(instance: usize, level: usize, value: &str) -> BlockContent {
    let mut depth = NumberingLevelReference::default();
    depth.val = Some(number(level));
    let mut id = NumberingDefinitionInstanceReferenceNumId::default();
    id.val = Some(number(instance));
    let mut reference = NumberingDefinitionInstanceReferenceNumPr::default();
    reference.numbering_definition_instance_reference_num_id = Some(Box::new(id));
    reference.numbering_level_reference = Some(Box::new(depth));
    let mut properties = ParagraphProperties::default();
    properties.numbering_definition_instance_reference_num_pr = Some(Box::new(reference));
    with_properties(properties, plain(value))
}

/// A run of `value` with the run properties `marks`.
fn marked(value: &str, marks: Vec<RunPropertiesContent>) -> TextRun {
    let mut properties = RunProperties::default();
    properties.content = marks;
    let mut marked = run(value);
    marked.properties = Some(Box::new(properties));
    marked
}

fn number(value: usize) -> DecimalNumberValue {
    DecimalNumberValue::from(value.to_string())
}

/// The numbering part: instance 1 is bullets, instance 2 decimal numbers from 1, instance 3
/// decimal numbers from 3, each of two levels.
fn numbering() -> NumberingDefinitions {
    let level = |index: usize, format: &str| {
        let mut kind = NumberingFormat::default();
        kind.val = Some(StNumberFormat::from(format.to_owned()));
        let mut first = StartingValue::default();
        first.val = Some(number(1));
        let mut defined = NumberingLevelOverrideDefinition::default();
        defined.ilvl = Some(number(index));
        defined.numbering_format = Some(Box::new(kind));
        defined.starting_value = Some(Box::new(first));
        defined
    };
    let definition = |id: usize, format: &str| {
        let mut abstract_definition = AbstractNumberingDefinition::default();
        abstract_definition.abstract_num_id = Some(number(id));
        abstract_definition.numbering_level_override_definition =
            vec![level(0, format), level(1, format)];
        abstract_definition
    };
    let instance = |id: usize, abstract_id: usize| {
        let mut reference = AbstractNumberingDefinitionReference::default();
        reference.val = Some(number(abstract_id));
        let mut numbered_by = NumberingDefinitionInstance::default();
        numbered_by.abstract_numbering_definition_reference = Box::new(reference);
        numbered_by.num_id = Some(number(id));
        numbered_by
    };
    let mut three = NumberingLevelStartingValueOverride::default();
    three.val = Some(number(3));
    let mut restart = NumberingLevelDefinitionOverride::default();
    restart.ilvl = Some(number(0));
    restart.numbering_level_starting_value_override = Some(Box::new(three));
    let mut from_three = instance(3, 1);
    from_three.numbering_level_definition_override = vec![restart];
    let mut definitions = NumberingDefinitions::default();
    definitions.abstract_numbering_definition =
        vec![definition(0, "bullet"), definition(1, "decimal")];
    definitions.numbering_definition_instance = vec![instance(1, 0), instance(2, 1), from_three];
    definitions
}

/// A paragraph of no style holding the run of `value`.
fn plain(value: &str) -> BlockContent {
    block(vec![ParagraphContent::TextRun(run(value))])
}

/// A run of `value`, its text's spaces kept.
fn run(value: &str) -> TextRun {
    run_holding(vec![characters(value)])
}

/// A run holding `content`.
fn run_holding(content: Vec<TextRunContent>) -> TextRun {
    let mut run = TextRun::default();
    run.content = content;
    run
}

/// A simple field of `instruction` whose result is the run of `shown`.
fn simple_field(instruction: &str, shown: &str) -> ParagraphContent {
    let mut field = SimpleField::default();
    field.content = vec![ParagraphContent::TextRun(run(shown))];
    field.instr = Some(instruction.to_owned());
    ParagraphContent::SimpleField(field)
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

/// A paragraph of the style `style` holding the run of `value`.
fn styled(style: &str, value: &str) -> BlockContent {
    let mut reference = ReferencedParagraphStyle::default();
    reference.val = Some(style.to_owned());
    let mut properties = ParagraphProperties::default();
    properties.referenced_paragraph_style = Some(Box::new(reference));
    with_properties(properties, plain(value))
}

/// The styles part of the Word file: the styles Word to markdown reads by name.
fn styles() -> StyleDefinitions {
    let mut normal = style("paragraph", "Normal", "Normal");
    normal.default = Some(OnOffValue::from("1".to_owned()));
    let mut definitions = StyleDefinitions::default();
    definitions.style_definition = vec![
        normal,
        style("paragraph", "Heading1", "heading 1"),
        style("paragraph", "Heading2", "heading 2"),
        style("paragraph", "Quote", "Quote"),
        style("paragraph", "HTMLPreformatted", "HTML Preformatted"),
        style("paragraph", "ListParagraph", "List Paragraph"),
        style("character", "HTMLCode", "HTML Code"),
    ];
    definitions
}

/// A table of `rows`, its first `head` rows marked as head rows.
fn table(head: usize, rows: Vec<Vec<TableCell>>) -> BlockContent {
    let mut built = Table::default();
    built.content_row_content = rows
        .into_iter()
        .map(|cells| {
            let mut row = TableRow::default();
            row.content = cells
                .into_iter()
                .map(ContentCellContent::TableCell)
                .collect();
            ContentRowContent::TableRow(row)
        })
        .collect();
    built.repeat_header_rows(head);
    BlockContent::Table(built)
}

/// `paragraph`, a paragraph, with `properties`.
fn with_properties(properties: ParagraphProperties, paragraph: BlockContent) -> BlockContent {
    let BlockContent::Paragraph(mut held) = paragraph else {
        return paragraph;
    };
    held.properties = Some(Box::new(properties));
    BlockContent::Paragraph(held)
}

/// The paragraph of runs of every style: plain, bold, italic, both, the `HTML Code` character
/// style, and a tab and a text wrapping break.
fn formatting() -> BlockContent {
    let mut wrapping = Break::default();
    wrapping.r#type = Some(BreakTypes::from("textWrapping".to_owned()));
    let mut tabbed = WordText::default();
    tabbed.value = "tabbed".to_owned();
    let mut wrapped = WordText::default();
    wrapped.value = "wrapped".to_owned();
    let mut code = ReferencedCharacterStyle::default();
    code.val = Some("HTMLCode".to_owned());
    let bold = || RunPropertiesContent::Bold(Bold::default());
    let italic = || RunPropertiesContent::Italics(Italics::default());
    block(
        [
            run("Plain "),
            marked("bold", vec![bold()]),
            run(" "),
            marked("italic", vec![italic()]),
            run(" "),
            marked("both", vec![bold(), italic()]),
            run(" "),
            marked(
                "code",
                vec![RunPropertiesContent::ReferencedCharacterStyle(code)],
            ),
            run_holding(vec![
                TextRunContent::TabCharacter(TabCharacter::default()),
                TextRunContent::Text(tabbed),
                TextRunContent::Break(wrapping),
                TextRunContent::Text(wrapped),
            ]),
        ]
        .into_iter()
        .map(ParagraphContent::TextRun)
        .collect(),
    )
}

/// The paragraph of links: a hyperlink of the relationship `outer` with a tooltip, one to a
/// bookmark, two `HYPERLINK` complex fields and a `HYPERLINK` simple field.
fn links(outer: &str) -> BlockContent {
    let mut dashboard = Hyperlink::default();
    dashboard.content = vec![ParagraphContent::TextRun(run("the dashboard"))];
    dashboard.id = Some(outer.to_owned());
    dashboard.tooltip = Some("Tip".to_owned());
    let mut below = Hyperlink::default();
    below.anchor = Some("notes".to_owned());
    below.content = vec![ParagraphContent::TextRun(run("below"))];
    let separator = || ParagraphContent::TextRun(run(", "));
    block(
        [
            vec![
                ParagraphContent::Hyperlink(dashboard),
                separator(),
                ParagraphContent::Hyperlink(below),
                separator(),
            ],
            complex_field(r#" HYPERLINK "https://example.com/site" "#, "site"),
            vec![separator()],
            complex_field(r#" HYPERLINK \l "q3_results" "#, "Q3"),
            vec![
                separator(),
                simple_field(r#" HYPERLINK "https://x.example" \l "part_two" "#, "part"),
            ],
        ]
        .concat(),
    )
}

/// An empty paragraph with a single bottom border, the horizontal rule Word writes.
fn rule() -> BlockContent {
    let mut bottom = ParagraphBorderBelowIdenticalParagraphs::default();
    bottom.color = Some(ColorValue::from("auto".to_owned()));
    bottom.space = Some(MeasurementInPoints::from("1".to_owned()));
    bottom.sz = Some(MeasurementInEighthsOfAPoint::from("6".to_owned()));
    bottom.val = Some(BorderStyles::from("single".to_owned()));
    let mut borders = ParagraphBorders::default();
    borders.paragraph_border_below_identical_paragraphs = Some(Box::new(bottom));
    let mut properties = ParagraphProperties::default();
    properties.paragraph_borders = Some(Box::new(borders));
    with_properties(properties, block(Vec::new()))
}

/// A paragraph holding a `w:pict` of the VML `v:shape` holding `content`, with the alt text
/// `alt` when given.
fn vml_shape(alt: Option<&str>, content: ShapeDefinitionContent) -> BlockContent {
    let mut shape = ShapeDefinition::default();
    shape.alt = alt.map(str::to_owned);
    shape.content = vec![content];
    let mut picture = VMLObject::default();
    picture.content = vec![VmlElementContent::ShapeDefinition(shape)];
    block(vec![ParagraphContent::TextRun(run_holding(vec![
        TextRunContent::VMLObject(picture),
    ]))])
}

/// A Word file built through the typed model that holds each construct Word to markdown reads:
/// headings, runs of every style, hyperlinks, `HYPERLINK` fields, pictures in `DrawingML` and in
/// `VML`, lists, a code block, a quote, a thematic break, two tables, an HTML import, a text box
/// and the frontmatter.
fn word_file() -> Docx {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let main = docx.main_document_part().clone();
    let picture = add_part(
        &mut docx,
        &main,
        "/word/media/image1.png",
        "image/png",
        RelationshipType::IMAGE.transitional,
        &tiny_png(),
    );
    let chunk = add_part(
        &mut docx,
        &main,
        "/word/htmlChunk1.html",
        "text/html",
        RelationshipType::A_F_CHUNK.transitional,
        b"<html><body><div>Imported</div></body></html>",
    );
    let styles_part = PartName::new("/word/styles.xml").unwrap();
    let numbering_part = PartName::new("/word/numbering.xml").unwrap();
    let frontmatter = PartName::new("/docProps/frontmatter.toml").unwrap();
    let package = docx.package_mut();
    style_definitions::store_transitional(
        package,
        &styles_part,
        &styles(),
        XmlWriteOptions::default(),
    )
    .unwrap();
    package
        .relate(
            Some(&main),
            &styles_part,
            style_definitions::RELATIONSHIP_TYPE.transitional,
        )
        .unwrap();
    numbering_definitions::store_transitional(
        package,
        &numbering_part,
        &numbering(),
        XmlWriteOptions::default(),
    )
    .unwrap();
    package
        .relate(
            Some(&main),
            &numbering_part,
            numbering_definitions::RELATIONSHIP_TYPE.transitional,
        )
        .unwrap();
    let outer = package
        .relate_external(
            Some(&main),
            "https://example.com/q3",
            RelationshipType::HYPERLINK.transitional,
        )
        .unwrap();
    package
        .set_part_bytes(
            &frontmatter,
            "application/toml",
            b"title = \"Quarterly report\"\n",
        )
        .unwrap();
    package
        .relate(None, &frontmatter, FRONTMATTER_RELATIONSHIP)
        .unwrap();

    let mut body = DocumentBody::default();
    body.content = word_body(&outer, &picture, &picture, &chunk);
    let mut document = Document::default();
    document.body = Some(Box::new(body));
    docx.set_main_document(&document).unwrap();
    docx
}

/// The body of the Word file; `outer` is the id of the external hyperlink, `logo` and `image` those
/// of the pictures and `chunk` that of the HTML part.
fn word_body(outer: &str, logo: &str, image: &str, chunk: &str) -> Vec<BlockContent> {
    let drawing = run_holding(vec![TextRunContent::DrawingMLObject(inline_picture(
        PictureSource::Embedded(image),
        1,
        "Picture 1",
        Some("Chart"),
        (914_400, 914_400),
    ))]);
    let mut data = ImageData::default();
    data.r_id = Some(logo.to_owned());
    let mut alt_chunk = AnchorForImportedExternalContent::default();
    alt_chunk.id = Some(chunk.to_owned());
    let mut boxed = RichTextBoxContentContainer::default();
    boxed.content = vec![plain("Boxed")];
    let mut text_box = TextBox::default();
    text_box.content = Some(Box::new(TextBoxContent::RichTextBoxContentContainer(boxed)));
    vec![
        styled("Heading1", "Results"),
        styled("Heading2", "Notes"),
        formatting(),
        links(outer),
        vml_shape(Some("Logo"), ShapeDefinitionContent::ImageData(data)),
        block(vec![ParagraphContent::TextRun(drawing)]),
        listed(1, 0, "North"),
        listed(1, 1, "Inner"),
        listed(1, 0, "South"),
        styled("ListParagraph", "More about South"),
        listed(3, 0, "three"),
        listed(3, 0, "four"),
        styled("HTMLPreformatted", "first"),
        styled("HTMLPreformatted", "second"),
        styled("Quote", "Quoted"),
        rule(),
        table(
            0,
            vec![
                vec![cell(&["a"]), cell(&["b"])],
                vec![cell(&["c", "d"]), cell(&["e"])],
            ],
        ),
        table(
            1,
            vec![
                vec![cell(&["A"]), cell(&["B"])],
                vec![cell(&["1"]), cell(&["2"])],
            ],
        ),
        BlockContent::AnchorForImportedExternalContent(alt_chunk),
        vml_shape(None, ShapeDefinitionContent::TextBox(text_box)),
    ]
}

#[test]
fn word_converted_to_markdown_and_back_gives_the_same_markdown_twice() {
    let system = MemorySystem::new();
    ready_now(system.create_dir_all(Path::new("/n/one"))).unwrap();
    ready_now(system.create_dir_all(Path::new("/n/two"))).unwrap();
    let original = word_file();
    assert_valid(&original);
    ready_now(original.save_transitional(&system, Path::new("/n/one/doc.docx"))).unwrap();
    let opened = ready_now(Docx::open(
        &system,
        Path::new("/n/one/doc.docx"),
        OpenOptions::default(),
    ))
    .unwrap();
    let options = options();
    ready_now(save_markdown(
        &opened,
        &system,
        Path::new("/n/one/doc.md"),
        options,
    ))
    .unwrap();
    let first = ready_now(system.read_to_string(Path::new("/n/one/doc.md"))).unwrap();
    assert_eq!(
        first,
        "---\ntitle: Quarterly report\n---\n# Results\n\n## Notes\n\nPlain **bold** *italic* ***both*** `code`\ttabbed\\\nwrapped\n\n[the dashboard](https://example.com/q3 \"Tip\"), [below](#notes), [site](https://example.com/site), [Q3](#q3-results), [part](https://x.example#part_two)\n\n![Logo](doc_media/image1.png)\n\n![Chart](doc_media/image1.png)\n\n- North\n\n  - Inner\n\n- South\n\n  More about South\n\n3. three\n4. four\n\n```\nfirst\nsecond\n```\n\n> Quoted\n\n***\n\n+-----+-----+\n| a   | b   |\n+-----+-----+\n| c   | e   |\n|     |     |\n| d   |     |\n+-----+-----+\n\n| A   | B   |\n| --- | --- |\n| 1   | 2   |\n\n<div>Imported</div>\n\nBoxed\n"
    );

    let again = ready_now(from_markdown(&system, Path::new("/n/one/doc.md"), options)).unwrap();
    assert_valid(&again);
    ready_now(save_markdown(
        &again,
        &system,
        Path::new("/n/two/doc.md"),
        options,
    ))
    .unwrap();
    assert_eq!(
        ready_now(system.read_to_string(Path::new("/n/two/doc.md"))).unwrap(),
        first
    );
    for folder in ["one", "two"] {
        assert_eq!(
            read_bytes(&system, &format!("/n/{folder}/doc_media/image1.png")),
            tiny_png()
        );
        assert!(
            !ready_now(system.exists(Path::new(&format!("/n/{folder}/doc_media/image2.png"))))
                .unwrap()
        );
    }
}
