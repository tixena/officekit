use alloc::string::String;
use alloc::vec::Vec;
use std::io::Read as _;
use std::path::Path;

use archie_core::parser::ast::{
    Alignment, Args, Block, BlockQuote, CodeBlock, Document, Emphasis, Heading, Html, Image,
    Inline, Item, Line, LineBreak, Link, LinkForm, LinkInline, LinkStrong, List, Mapping,
    MarkdownBlock, Paragraph, RawDocument, Section, Strong, Table, TableCell, TableFormat,
    TableRow, Text, ThematicBreak, Value,
};
use archie_core::parser::parse;
use archie_core::writer::{WriteOptions, write};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use officekit_packaging::package::OpenOptions;
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::RelationshipType;
use officekit_parts::kinds::WordprocessingKind;
use officekit_parts::numbering_definitions;
use officekit_parts::style_definitions::{CONTENT_TYPE, RELATIONSHIP_TYPE};
use os_shim::System as _;
use os_shim::mock::MemorySystem;

use crate::docx::Docx;
use crate::markdown::{
    Exported, FRONTMATTER_RELATIONSHIP, MarkdownError, MarkdownOptions, save_markdown,
};

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

/// The numbering part of the tests: instance 1 is bullets, instance 2 decimal numbers from 1,
/// instance 3 a level of format `none`, and instance 4 decimal numbers from 3 by a
/// `w:startOverride`.
const NUMBERING: &str = concat!(
    r#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
    r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/></w:lvl>"#,
    r#"<w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum>"#,
    r#"<w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1)"/></w:lvl>"#,
    r#"<w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="decimal"/></w:lvl></w:abstractNum>"#,
    r#"<w:abstractNum w:abstractNumId="2"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="none"/></w:lvl></w:abstractNum>"#,
    r#"<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#,
    r#"<w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>"#,
    r#"<w:num w:numId="3"><w:abstractNumId w:val="2"/></w:num>"#,
    r#"<w:num w:numId="4"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="3"/></w:lvlOverride></w:num>"#,
    "</w:numbering>"
);

/// The first bytes of a PNG file, enough for a picture part of the tests.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";

/// The styles part of the tests: `Ttulo1` is named `heading 1` and has no outline level,
/// `Subtitle` is based on `Heading2`, `Strongly` is a bold and italic paragraph style, `Cita` is
/// named `Quote`, `Numbered` is numbered by instance 2, `ListParagraph` is named `List
/// Paragraph`, `Emphatic` is an italic character style, and `Mono` a character style in Courier
/// New.
const STYLES: &str = concat!(
    r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
    r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="Ttulo1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:rPr><w:b/></w:rPr></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="Heading7"><w:name w:val="heading 7"/></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="Subtitle"><w:name w:val="Subtitle"/><w:basedOn w:val="Heading2"/></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="Strongly"><w:name w:val="Strongly"/><w:rPr><w:b/><w:i/></w:rPr></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="HTMLPreformatted"><w:name w:val="HTML Preformatted"/></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="Cita"><w:name w:val="Quote"/></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="Numbered"><w:name w:val="Numbered"/><w:pPr><w:numPr><w:numId w:val="2"/></w:numPr></w:pPr></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="ListParagraph"><w:name w:val="List Paragraph"/></w:style>"#,
    r#"<w:style w:type="character" w:styleId="HTMLCode"><w:name w:val="HTML Code"/></w:style>"#,
    r#"<w:style w:type="character" w:styleId="Emphatic"><w:name w:val="Emphatic"/><w:rPr><w:i/></w:rPr></w:style>"#,
    r#"<w:style w:type="character" w:styleId="Mono"><w:name w:val="Mono"/><w:rPr><w:rFonts w:ascii="Courier New"/></w:rPr></w:style>"#,
    "</w:styles>"
);

/// The content of a text box holding one paragraph, `Boxed`.
const BOXED: &str = "<w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent>";

/// A Word document being built: its styles part is [`STYLES`]; its other parts and relationships
/// are added before its body.
struct Fixture {
    docx: Docx,
    main: PartName,
}

impl Fixture {
    /// The document, its main part holding `body`.
    fn body(mut self, body: &str) -> Docx {
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
        let styles = PartName::new("/word/styles.xml").unwrap();
        let package = docx.package_mut();
        package
            .set_part_bytes(&styles, CONTENT_TYPE, STYLES.as_bytes())
            .unwrap();
        package
            .relate(Some(&main), &styles, RELATIONSHIP_TYPE.transitional)
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

/// The archie document of `docx`, checked to read back from its markdown as the same tree; it
/// names no picture.
fn archie(docx: &Docx) -> Document {
    let result = exported(docx, MarkdownOptions::default());
    assert_eq!(result.images, Vec::<(String, Vec<u8>)>::new());
    result.document
}

fn bold_run(text: &str) -> String {
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
    let result = docx.to_archie("report_media", options).unwrap();
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
    write(&RawDocument::Valid(archie(docx)), &WriteOptions::default()).unwrap()
}

/// One section with no heading, holding `blocks`.
fn no_heading(blocks: Vec<Block>) -> Vec<Section> {
    alloc::vec![Section::new(None, blocks, Vec::new())]
}

/// A paragraph of the run of `text`, numbered by `instance` at `level`.
fn numbered(instance: u8, level: u8, text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="{instance}"/></w:numPr></w:pPr>{}</w:p>"#,
        run(text)
    )
}

fn paragraph(style: &str, runs: &str) -> String {
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
fn picture(descr: &str, id: &str) -> String {
    format!(
        "<w:r>{}</w:r>",
        picture_drawing(descr, &format!(r#"<a:blip r:embed="{id}"/>"#))
    )
}

/// A `w:drawing` of an inline picture whose `a:blip` is `blip` and whose alt text is `descr`.
fn picture_drawing(descr: &str, blip: &str) -> String {
    format!(
        r#"<w:drawing><wp:inline><wp:extent cx="914400" cy="914400"/><wp:docPr id="1" name="Picture 1" descr="{descr}"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="image"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill>{blip}<a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr/></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing>"#
    )
}

/// The bytes of the file at `path`.
fn read_bytes(system: &MemorySystem, path: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    system
        .open(Path::new(path))
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}

fn run(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

fn strong(children: Vec<Inline>) -> Inline {
    Inline::Strong(Strong::new(children))
}

/// A table of `rows`.
fn table(rows: &[String]) -> String {
    format!("<w:tbl><w:tblPr/><w:tblGrid/>{}</w:tbl>", rows.concat())
}

/// A table cell of `properties`, the inside of its `w:tcPr`, holding a paragraph of the run of
/// `text`.
fn table_cell(properties: &str, text: &str) -> String {
    format!(
        "<w:tc><w:tcPr>{properties}</w:tcPr><w:p>{}</w:p></w:tc>",
        run(text)
    )
}

/// A table row of `cells`, marked `w:tblHeader` when `head` is set.
fn table_row(head: bool, cells: &[String]) -> String {
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

/// A new Word document whose body is `body` and whose styles part is [`STYLES`].
fn word(body: &str) -> Docx {
    Fixture::new().body(body)
}

#[test]
fn the_design_sample_writes_its_markdown_and_no_media_folder() {
    let body = format!(
        "{}<w:p>{}{}<w:r><w:br/><w:t>Calle 5</w:t></w:r></w:p>",
        paragraph("Ttulo1", &run("Contrato")),
        bold_run("Nombre: "),
        run("Ana"),
    );
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/n")).unwrap();
    word(&body)
        .save_transitional(&system, Path::new("/n/contract.docx"))
        .unwrap();
    let docx = Docx::open(
        &system,
        Path::new("/n/contract.docx"),
        OpenOptions::default(),
    )
    .unwrap();
    archie(&docx);
    save_markdown(
        &docx,
        &system,
        Path::new("/n/contract.md"),
        MarkdownOptions::default(),
    )
    .unwrap();
    assert_eq!(
        system.read_to_string(Path::new("/n/contract.md")).unwrap(),
        "# Contrato\n\n**Nombre:** Ana\\\nCalle 5\n"
    );
    assert!(!system.exists(Path::new("/n/contract_media")).unwrap());
}

#[test]
fn save_markdown_leaves_a_media_folder_already_there_as_it_is() {
    let system = MemorySystem::new();
    system
        .create_dir_all(Path::new("/n/contract_media"))
        .unwrap();
    system
        .write(Path::new("/n/contract_media/kept.png"), b"png")
        .unwrap();
    save_markdown(
        &word(&paragraph("Normal", &run("Hola"))),
        &system,
        Path::new("/n/contract.md"),
        MarkdownOptions::default(),
    )
    .unwrap();
    assert_eq!(
        system.read_to_string(Path::new("/n/contract.md")).unwrap(),
        "Hola\n"
    );
    assert_eq!(
        system
            .read_to_string(Path::new("/n/contract_media/kept.png"))
            .unwrap(),
        "png"
    );
}

#[test]
fn a_style_named_heading_1_and_an_own_outline_level_0_both_give_depth_1_headings() {
    let body = format!(
        r#"{}<w:p><w:pPr><w:outlineLvl w:val="0"/></w:pPr>{}</w:p>"#,
        paragraph("Ttulo1", &run("One")),
        run("Two"),
    );
    let docx = word(&body);
    assert_eq!(
        archie(&docx).body.sections,
        [
            Section::new(Some(heading("One")), Vec::new(), Vec::new()),
            Section::new(Some(heading("Two")), Vec::new(), Vec::new()),
        ]
    );
    assert_eq!(markdown(&docx), "# One\n\n# Two\n");
}

#[test]
fn a_style_based_on_a_heading_style_takes_its_level() {
    let body = [
        paragraph("Ttulo1", &run("Top")),
        paragraph("Subtitle", &run("Under")),
    ]
    .concat();
    assert_eq!(markdown(&word(&body)), "# Top\n\n## Under\n");
}

#[test]
fn a_level_3_heading_after_a_level_1_heading_sits_in_a_depth_2_section_with_no_heading() {
    let body = [
        paragraph("Ttulo1", &run("One")),
        paragraph("Heading3", &run("Three")),
    ]
    .concat();
    let docx = word(&body);
    let three = Section::new(Some(heading("Three")), Vec::new(), Vec::new());
    let skipped = Section::new(None, Vec::new(), alloc::vec![three]);
    assert_eq!(
        archie(&docx).body.sections,
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
    let body = [
        paragraph("Ttulo1", &run("Top")),
        paragraph("Heading7", &run("Deep")),
    ]
    .concat();
    let docx = word(&body);
    let deep = paragraph_of(alloc::vec![strong(alloc::vec![text("Deep")])]);
    assert_eq!(
        archie(&docx).body.sections,
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
    let body = [
        paragraph("Ttulo1", ""),
        paragraph("Heading3", "<w:r><w:t>a</w:t><w:br/><w:t>b</w:t></w:r>"),
    ]
    .concat();
    assert_eq!(markdown(&word(&body)), "### a b\n");
}

#[test]
fn three_bold_runs_become_one_strong_node() {
    let body = format!(
        "<w:p>{}{}{}</w:p>",
        bold_run("Re"),
        bold_run("ve"),
        bold_run("nue")
    );
    assert_eq!(
        archie(&word(&body)).body.sections,
        no_heading(alloc::vec![paragraph_of(alloc::vec![strong(alloc::vec![
            text("Revenue")
        ])])])
    );
}

#[test]
fn the_space_at_the_end_of_a_bold_stretch_goes_outside_it() {
    let body = format!("<w:p>{}{}</w:p>", bold_run("Nombre: "), run("Ana"));
    let docx = word(&body);
    assert_eq!(
        archie(&docx).body.sections,
        no_heading(alloc::vec![paragraph_of(alloc::vec![
            strong(alloc::vec![text("Nombre:")]),
            text(" Ana"),
        ])])
    );
    assert_eq!(markdown(&docx), "**Nombre:** Ana\n");
}

#[test]
fn a_run_both_bold_and_italic_is_emphasis_holding_strong() {
    let docx = word("<w:p><w:r><w:rPr><w:b/><w:i/></w:rPr><w:t>x</w:t></w:r></w:p>");
    let both = Inline::Emphasis(Emphasis::new(alloc::vec![strong(alloc::vec![text("x")])]));
    assert_eq!(
        archie(&docx).body.sections,
        no_heading(alloc::vec![paragraph_of(alloc::vec![both])])
    );
    assert_eq!(markdown(&docx), "***x***\n");
}

#[test]
fn character_styles_and_code_fonts_mark_runs_and_a_paragraph_style_does_not() {
    let body = format!(
        "<w:p>{}{}{}{}{}</w:p>{}",
        r#"<w:r><w:rPr><w:rStyle w:val="Emphatic"/></w:rPr><w:t>says</w:t></w:r>"#,
        run(" "),
        r#"<w:r><w:rPr><w:rStyle w:val="HTMLCode"/></w:rPr><w:t>a</w:t></w:r>"#,
        r#"<w:r><w:rPr><w:rFonts w:ascii="Consolas"/></w:rPr><w:t xml:space="preserve"> </w:t></w:r>"#,
        r#"<w:r><w:rPr><w:rStyle w:val="Mono"/><w:b w:val="0"/></w:rPr><w:t>b</w:t></w:r>"#,
        paragraph("Strongly", &run("plain")),
    );
    assert_eq!(markdown(&word(&body)), "*says* `a b`\n\nplain\n");
}

#[test]
fn inserted_text_stays_deleted_text_goes_and_a_field_gives_its_shown_text() {
    let body = concat!(
        r#"<w:p><w:r><w:t xml:space="preserve">Kept </w:t></w:r>"#,
        r#"<w:ins w:id="1" w:author="a"><w:r><w:t>new</w:t></w:r></w:ins>"#,
        r#"<w:del w:id="2" w:author="a"><w:r><w:delText>old</w:delText></w:r></w:del>"#,
        r#"<w:r><w:t xml:space="preserve"> on </w:t></w:r>"#,
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r>"#,
        r#"<w:r><w:instrText xml:space="preserve"> DATE \@ "d/M/yyyy" </w:instrText></w:r>"#,
        r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r>"#,
        r#"<w:r><w:t>19/10/2026</w:t></w:r>"#,
        r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
        r#"<w:r><w:t xml:space="preserve"> by </w:t></w:r>"#,
        r#"<w:fldSimple w:instr=" AUTHOR "><w:r><w:t>Ana</w:t></w:r></w:fldSimple>"#,
        r#"<w:moveFrom w:id="3" w:author="a"><w:r><w:t>gone</w:t></w:r></w:moveFrom>"#,
        r#"<w:moveTo w:id="4" w:author="a"><w:r><w:t>.</w:t></w:r></w:moveTo></w:p>"#,
    );
    assert_eq!(markdown(&word(body)), "Kept new on 19/10/2026 by Ana.\n");
}

#[test]
fn consecutive_code_and_quote_paragraphs_become_one_code_block_and_one_block_quote() {
    let body = [
        paragraph("HTMLPreformatted", &run("fn main() {")),
        paragraph("HTMLPreformatted", &run("}")),
        paragraph("Cita", &run("a")),
        paragraph("Cita", &run("b")),
    ]
    .concat();
    let docx = word(&body);
    assert_eq!(
        archie(&docx).body.sections,
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
    let body = concat!(
        "<w:p><w:r><w:t>a</w:t><w:tab/><w:t>b</w:t><w:cr/><w:t>c</w:t>",
        r#"<w:br w:type="page"/><w:t>d</w:t><w:br w:type="textWrapping"/><w:t>e</w:t>"#,
        "<w:br/></w:r></w:p>",
    );
    assert_eq!(
        archie(&word(body)).body.sections,
        no_heading(alloc::vec![paragraph_lines(alloc::vec![
            alloc::vec![text("a\tb"), line_break()],
            alloc::vec![text("cd"), line_break()],
            alloc::vec![text("e")],
        ])])
    );
}

#[test]
fn an_empty_paragraph_gives_nothing_and_one_with_a_bottom_border_alone_a_thematic_break() {
    let body = concat!(
        "<w:p/>",
        "<w:p><w:r><w:t>a</w:t></w:r></w:p>",
        r#"<w:p><w:pPr><w:pBdr><w:bottom w:val="single" w:sz="6" w:space="1" w:color="auto"/>"#,
        "</w:pBdr></w:pPr></w:p>",
        r#"<w:p><w:r><w:t xml:space="preserve">  </w:t></w:r></w:p>"#,
        "<w:p><w:r><w:t>b</w:t></w:r></w:p>",
    );
    assert_eq!(
        archie(&word(body)).body.sections,
        no_heading(alloc::vec![
            paragraph_of(alloc::vec![text("a")]),
            Block::ThematicBreak(ThematicBreak::default()),
            paragraph_of(alloc::vec![text("b")]),
        ])
    );
}

#[test]
fn content_controls_smart_tags_and_custom_xml_give_their_content() {
    let body = concat!(
        r#"<w:sdt><w:sdtContent><w:p><w:r><w:t xml:space="preserve">a </w:t></w:r>"#,
        "<w:sdt><w:sdtContent><w:r><w:t>b</w:t></w:r></w:sdtContent></w:sdt>",
        r#"<w:smartTag w:uri="u" w:element="e"><w:r><w:t xml:space="preserve"> c</w:t></w:r></w:smartTag>"#,
        r#"<w:customXml w:element="f"><w:r><w:t xml:space="preserve"> d</w:t></w:r></w:customXml>"#,
        "</w:p></w:sdtContent></w:sdt>",
    );
    assert_eq!(markdown(&word(body)), "a b c d\n");
}

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
    assert_eq!(markdown(&word(&body)), "Before\n\nBoxed\n\nBoxed\n");
}

#[test]
fn an_equation_gives_its_text() {
    let body = concat!(
        r#"<w:p><w:r><w:t xml:space="preserve">Solve </w:t></w:r>"#,
        "<m:oMath><m:r><m:t>x=1</m:t></m:r></m:oMath></w:p>",
    );
    assert_eq!(markdown(&word(body)), "Solve x=1\n");
}

#[test]
fn to_archie_leaves_the_main_part_as_it_was() {
    let body = format!(
        "{}<w:p>{}</w:p>",
        paragraph("Ttulo1", &run("Contrato")),
        bold_run("x")
    );
    let docx = word(&body);
    let main = docx.main_document_part().clone();
    let before = docx.package().part_bytes(&main).unwrap();
    archie(&docx);
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
        markdown(&word(body)),
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
        table_row(false, &[table_cell("", "a"), table_cell("", "b")]),
        table_row(false, &[table_cell("", "c"), table_cell("", "d")]),
    ];
    let body = format!(
        r#"{}{}{}<w:p>{}<w:hyperlink r:id="{link_id}">{}</w:hyperlink>{}</w:p><w:p>{}</w:p>"#,
        numbered(2, 0, "one"),
        numbered(2, 0, "two"),
        table(&rows),
        run("See "),
        run("the dashboard"),
        run("."),
        picture("Logo", &picture_id),
    );
    fixture.body(&body)
}

#[test]
fn the_list_table_link_and_picture_sample_writes_its_markdown_and_one_image_file() {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/n")).unwrap();
    sample()
        .save_transitional(&system, Path::new("/n/contract.docx"))
        .unwrap();
    let docx = Docx::open(
        &system,
        Path::new("/n/contract.docx"),
        OpenOptions::default(),
    )
    .unwrap();
    assert_eq!(
        exported(&docx, MarkdownOptions::default()).images,
        [("image1.png".to_owned(), PNG.to_vec())]
    );
    save_markdown(
        &docx,
        &system,
        Path::new("/n/contract.md"),
        MarkdownOptions::default(),
    )
    .unwrap();
    assert_eq!(
        system.read_to_string(Path::new("/n/contract.md")).unwrap(),
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
    system.create_dir_all(Path::new("/n")).unwrap();
    let docx = sample();
    let options = MarkdownOptions {
        embed_images: true,
        frontmatter: false,
    };
    assert_eq!(
        exported(&docx, options).images,
        Vec::<(String, Vec<u8>)>::new()
    );
    save_markdown(&docx, &system, Path::new("/n/contract.md"), options).unwrap();
    let encoded = STANDARD.encode(PNG);
    assert!(encoded.starts_with("iVBORw0KGgo"));
    let written = system.read_to_string(Path::new("/n/contract.md")).unwrap();
    assert!(
        written.ends_with(&format!(
            "See [the dashboard](https://example.com/q3).\n\n![Logo](data:image/png;base64,{encoded})\n"
        )),
        "{written}"
    );
    assert!(!system.exists(Path::new("/n/contract_media")).unwrap());
}

#[test]
fn a_paragraph_numbered_by_its_style_is_a_list_item_and_a_level_of_format_none_a_paragraph() {
    let mut fixture = Fixture::new();
    fixture.numbering();
    let body = [
        paragraph("Numbered", &run("styled")),
        numbered(3, 0, "hidden"),
    ]
    .concat();
    let docx = fixture.body(&body);
    assert_eq!(
        archie(&docx).body.sections,
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
        numbered(1, 0, "a"),
        numbered(1, 1, "b"),
        numbered(1, 0, "c"),
        numbered(4, 0, "three"),
    ]
    .concat();
    let docx = fixture.body(&body);
    let nested = list(
        alloc::vec![item(alloc::vec![paragraph_of(alloc::vec![text("b")])])],
        None,
    );
    assert_eq!(
        archie(&docx).body.sections,
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
fn a_list_paragraph_with_no_numbering_after_a_list_paragraph_is_a_later_block_of_its_item() {
    let mut fixture = Fixture::new();
    fixture.numbering();
    let body = [
        numbered(2, 0, "one"),
        paragraph("ListParagraph", &run("more")),
        numbered(2, 0, "two"),
        paragraph("Normal", &run("after")),
    ]
    .concat();
    let docx = fixture.body(&body);
    let items = alloc::vec![
        item(alloc::vec![
            paragraph_of(alloc::vec![text("one")]),
            paragraph_of(alloc::vec![text("more")]),
        ]),
        item(alloc::vec![paragraph_of(alloc::vec![text("two")])]),
    ];
    assert_eq!(
        archie(&docx).body.sections,
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
            run(text)
        )
    };
    let body = [
        numbered(1, 0, "item"),
        indented("HTMLPreformatted", "let a = 1;"),
        indented("HTMLPreformatted", "let b = 2;"),
        indented("Quote", "quoted"),
        concat!(
            r#"<w:p><w:pPr><w:pBdr><w:bottom w:val="single" w:sz="6" w:space="1" w:color="auto"/>"#,
            r#"</w:pBdr><w:ind w:left="720"/></w:pPr></w:p>"#,
        )
        .to_owned(),
        numbered(1, 0, "next"),
        paragraph("HTMLPreformatted", &run("outside")),
    ]
    .concat();
    let docx = fixture.body(&body);
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
        archie(&docx).body.sections,
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
        numbered(2, 0, "outer"),
        numbered(1, 1, "inner"),
        r#"<w:p><w:pPr><w:pStyle w:val="HTMLPreformatted"/><w:ind w:left="720"/></w:pPr><w:r><w:t>code</w:t></w:r></w:p>"#.to_owned(),
    ]
    .concat();
    let docx = fixture.body(&body);
    let inner = list(
        alloc::vec![item(alloc::vec![paragraph_of(alloc::vec![text("inner")])])],
        None,
    );
    assert_eq!(
        archie(&docx).body.sections,
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
            run(text)
        )
    };
    let rows = [
        table_row(true, &[aligned("center", "h1"), aligned("end", "h2")]),
        table_row(false, &[table_cell("", "a"), table_cell("", "b")]),
    ];
    let docx = word(&table(&rows));
    let expected = Table::new(
        TableFormat::Pipe,
        alloc::vec![Alignment::Center, Alignment::Right],
        alloc::vec![TableRow::new(alloc::vec![cell_of("h1"), cell_of("h2")])],
        alloc::vec![TableRow::new(alloc::vec![cell_of("a"), cell_of("b")])],
    );
    assert_eq!(
        archie(&docx).body.sections,
        no_heading(alloc::vec![Block::Table(expected)])
    );
    assert_eq!(
        markdown(&docx),
        "| h1  | h2  |\n| :-: | --: |\n| a   | b   |\n"
    );
}

#[test]
fn a_table_with_no_head_row_or_with_a_cell_of_two_paragraphs_is_a_grid_table() {
    let two = "<w:tc><w:p><w:r><w:t>x</w:t></w:r></w:p><w:p><w:r><w:t>y</w:t></w:r></w:p></w:tc>";
    let rows = [
        table_row(true, &[table_cell("", "h")]),
        table_row(false, &[two.to_owned()]),
    ];
    let docx = word(
        &[
            table(&[table_row(false, &[table_cell("", "a")])]),
            table(&rows),
        ]
        .concat(),
    );
    let two_paragraphs = TableCell::new(MarkdownBlock::new(no_heading(alloc::vec![
        paragraph_of(alloc::vec![text("x")]),
        paragraph_of(alloc::vec![text("y")]),
    ])));
    assert_eq!(
        archie(&docx).body.sections,
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
        table_row(
            false,
            &[
                table_cell(r#"<w:gridSpan w:val="2"/>"#, "wide"),
                table_cell("", "c"),
            ],
        ),
        table_row(
            false,
            &[
                table_cell(r#"<w:vMerge w:val="restart"/>"#, "x"),
                table_cell("", "y"),
                table_cell("", "z"),
            ],
        ),
        table_row(
            false,
            &[
                table_cell("<w:vMerge/>", "under"),
                table_cell("", "q"),
                table_cell("", "r"),
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
        archie(&word(&table(&rows))).body.sections,
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
        run("Q3"),
        run(" and "),
        run("plan"),
        run("x"),
        run(" or "),
        run("y"),
    );
    let docx = word(&body);
    assert_eq!(
        archie(&docx).body.sections,
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
        run("a "),
        bold_run("b"),
        run(" c"),
    );
    let docx = fixture.body(&body);
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
        archie(&docx).body.sections,
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
        picture("A", &id),
        picture("B", &id)
    );
    let result = exported(&fixture.body(&body), MarkdownOptions::default());
    assert_eq!(result.images, [("image1.png".to_owned(), PNG.to_vec())]);
    assert_eq!(
        result.document.body.sections,
        no_heading(alloc::vec![
            paragraph_of(alloc::vec![image("report_media/image1.png", "A")]),
            paragraph_of(alloc::vec![image("report_media/image1.png", "B")]),
        ])
    );
}

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
    let body = format!("<w:p><w:r>{}</w:r></w:p>", picture_drawing("Chart", &blip));
    let result = exported(&fixture.body(&body), MarkdownOptions::default());
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
    let body = format!("<w:p><w:r>{}</w:r></w:p>", picture_drawing("Logo", &blip));
    assert_eq!(
        archie(&fixture.body(&body)).body.sections,
        no_heading(alloc::vec![paragraph_of(alloc::vec![image(
            "https://example.com/logo.png",
            "Logo"
        )])])
    );
}

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
        picture_drawing("first", &blip(&first)),
        picture_drawing("unused", &blip(&unused)),
        text_box,
        picture_drawing("second", &blip(&second)),
    );
    let result = exported(&fixture.body(&body), MarkdownOptions::default());
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
        paragraph("Normal", &run("before"))
    );
    let docx = fixture.body(&body);
    assert_eq!(
        archie(&docx).body.sections,
        no_heading(alloc::vec![
            paragraph_of(alloc::vec![text("before")]),
            Block::Html(Html::new("<div>hi</div>".to_owned())),
        ])
    );
}

#[test]
fn the_frontmatter_part_is_the_frontmatter_only_when_asked_and_one_that_does_not_read_fails() {
    let with_part = |toml: &[u8]| {
        let mut docx = word(&paragraph("Normal", &run("Hola")));
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
        broken.to_archie("report_media", asked),
        Err(MarkdownError::Frontmatter(_))
    ));
    assert_eq!(
        broken
            .to_archie("report_media", MarkdownOptions::default())
            .unwrap()
            .document
            .frontmatter,
        None
    );
}
