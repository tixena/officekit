use alloc::string::String;
use alloc::vec::Vec;
use std::path::Path;

use archie_core::parser::ast::{
    Args, Block, BlockQuote, CodeBlock, Document, Emphasis, Heading, Inline, Line, LineBreak,
    Mapping, Paragraph, RawDocument, Section, Strong, Text, ThematicBreak,
};
use archie_core::parser::parse;
use archie_core::writer::{WriteOptions, write};
use officekit_packaging::package::OpenOptions;
use officekit_packaging::part_name::PartName;
use officekit_parts::kinds::WordprocessingKind;
use officekit_parts::style_definitions::{CONTENT_TYPE, RELATIONSHIP_TYPE};
use os_shim::System as _;
use os_shim::mock::MemorySystem;

use crate::docx::Docx;
use crate::markdown::{MarkdownOptions, save_markdown};

/// The namespaces the main documents of the tests declare.
const NAMESPACES: &str = concat!(
    r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" "#,
    r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" "#,
    r#"xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" "#,
    r#"xmlns:v="urn:schemas-microsoft-com:vml" "#,
    r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" "#,
    r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#,
    r#"xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#
);

/// The styles part of the tests: `Ttulo1` is named `heading 1` and has no outline level,
/// `Subtitle` is based on `Heading2`, `Strongly` is a bold and italic paragraph style, `Cita` is
/// named `Quote`, `Emphatic` is an italic character style, and `Mono` a character style in
/// Courier New.
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
    r#"<w:style w:type="character" w:styleId="HTMLCode"><w:name w:val="HTML Code"/></w:style>"#,
    r#"<w:style w:type="character" w:styleId="Emphatic"><w:name w:val="Emphatic"/><w:rPr><w:i/></w:rPr></w:style>"#,
    r#"<w:style w:type="character" w:styleId="Mono"><w:name w:val="Mono"/><w:rPr><w:rFonts w:ascii="Courier New"/></w:rPr></w:style>"#,
    "</w:styles>"
);

/// The content of a text box holding one paragraph, `Boxed`.
const BOXED: &str = "<w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent>";

/// The archie document of `docx`, checked to read back from its markdown as the same tree.
fn archie(docx: &Docx) -> Document {
    let exported = docx
        .to_archie("report_media", MarkdownOptions::default())
        .unwrap();
    assert_eq!(exported.images, Vec::<(String, Vec<u8>)>::new());
    let text = write(
        &RawDocument::Valid(exported.document.clone()),
        &WriteOptions::default(),
    )
    .unwrap();
    assert_eq!(
        parse(&text).document,
        RawDocument::Valid(exported.document.clone()),
        "{text}"
    );
    exported.document
}

fn bold_run(text: &str) -> String {
    format!(r#"<w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

/// A heading of plain `value`.
fn heading(value: &str) -> Heading {
    Heading::new(alloc::vec![text(value)])
}

fn line_break() -> Inline {
    Inline::LineBreak(LineBreak::default())
}

/// The markdown of `docx`.
fn markdown(docx: &Docx) -> String {
    write(&RawDocument::Valid(archie(docx)), &WriteOptions::default()).unwrap()
}

/// One section with no heading, holding `blocks`.
fn no_heading(blocks: Vec<Block>) -> Vec<Section> {
    alloc::vec![Section::new(None, blocks, Vec::new())]
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

fn run(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

fn strong(children: Vec<Inline>) -> Inline {
    Inline::Strong(Strong::new(children))
}

fn text(value: &str) -> Inline {
    Inline::Text(Text::new(value.to_owned()))
}

/// A new Word document whose body is `body` and whose styles part is [`STYLES`].
fn word(body: &str) -> Docx {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let main = docx.main_document_part().clone();
    let content_type = docx.package().content_type(&main).unwrap().to_owned();
    let document = format!("<w:document {NAMESPACES}><w:body>{body}</w:body></w:document>");
    let styles = PartName::new("/word/styles.xml").unwrap();
    let package = docx.package_mut();
    package
        .set_part_bytes(&main, &content_type, document.as_bytes())
        .unwrap();
    package
        .set_part_bytes(&styles, CONTENT_TYPE, STYLES.as_bytes())
        .unwrap();
    package
        .relate(Some(&main), &styles, RELATIONSHIP_TYPE.transitional)
        .unwrap();
    docx
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
fn a_table_gives_its_cells_paragraphs_and_a_hyperlink_its_text() {
    let body = concat!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol/><w:gridCol/></w:tblGrid><w:tr>",
        "<w:tc><w:p><w:r><w:t>left</w:t></w:r></w:p></w:tc>",
        r#"<w:tc><w:p><w:hyperlink w:anchor="x"><w:r><w:t>right</w:t></w:r></w:hyperlink></w:p></w:tc>"#,
        "</w:tr></w:tbl>",
    );
    assert_eq!(markdown(&word(body)), "left\n\nright\n");
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
