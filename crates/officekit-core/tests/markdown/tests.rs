use std::io::Read as _;
use std::path::Path;

use archie_core::parser::parse;
use archie_core::writer::{WriteOptions, write};
use officekit_core::docx::Docx;
use officekit_core::markdown::{self, FRONTMATTER_RELATIONSHIP, MarkdownOptions};
use officekit_packaging::package::OpenOptions;
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::RelationshipType;
use officekit_parts::kinds::WordprocessingKind;
use officekit_parts::{numbering_definitions, style_definitions};
use officekit_validation::validator::Validator;
use officekit_xml::name::OfficeVersion;
use os_shim::System as _;
use os_shim::mock::MemorySystem;
use png::{BitDepth, ColorType, Encoder};

const SOURCE: &str = "+++\ntitle = \"Quarterly report\"\n+++\n# Results\n\nRevenue grew **12%** in *Q3*, see [the dashboard](https://example.com/q3) and [below](#notes).\n\n- North\n- South\n\n1. one\n2. two\n\n| Region | Revenue |\n| :----- | ------: |\n| North  | 1,200   |\n\n```\nfirst\nsecond\n```\n\n> Quoted.\n\n***\n\n## Notes\n\n<div align=\"center\">Signed</div>\n\n![Logo](logo.png)\n";

const NAMESPACES: &str = concat!(
    r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" "#,
    r#"xmlns:v="urn:schemas-microsoft-com:vml" "#,
    r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" "#,
    r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#,
    r#"xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" "#,
    r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#
);

/// The styles part of the Word file: the styles task 05 reads by name.
const STYLES: &str = concat!(
    r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
    r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="HTMLPreformatted"><w:name w:val="HTML Preformatted"/></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="ListParagraph"><w:name w:val="List Paragraph"/></w:style>"#,
    r#"<w:style w:type="character" w:styleId="HTMLCode"><w:name w:val="HTML Code"/></w:style>"#,
    "</w:styles>"
);

/// The numbering part: instance 1 is bullets, instance 2 decimal numbers from 1, instance 3
/// decimal numbers from 3, each of two levels.
const NUMBERING: &str = concat!(
    r#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
    r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/></w:lvl>"#,
    r#"<w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum>"#,
    r#"<w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/></w:lvl>"#,
    r#"<w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="decimal"/></w:lvl></w:abstractNum>"#,
    r#"<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#,
    r#"<w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>"#,
    r#"<w:num w:numId="3"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="3"/></w:lvlOverride></w:num>"#,
    "</w:numbering>"
);

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
    let mut bytes = Vec::new();
    system
        .open(Path::new(path))
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}

fn assert_valid(docx: &Docx) {
    let errors = Validator::new(OfficeVersion::Microsoft365).validate(docx.package());
    assert!(errors.is_empty(), "{errors:?}");
}

/// A folder `/n` holding `source` as `doc.md` and the picture `logo.png`.
fn notes(source: &str) -> MemorySystem {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/n")).unwrap();
    system
        .write(Path::new("/n/doc.md"), source.as_bytes())
        .unwrap();
    system.write(Path::new("/n/logo.png"), &tiny_png()).unwrap();
    system
}

/// The Word file of `/n/doc.md`, validated, saved, reopened and written as markdown to `back`.
fn through_word(system: &MemorySystem, back: &str, options: MarkdownOptions) -> String {
    let docx = markdown::from_markdown(system, Path::new("/n/doc.md"), options).unwrap();
    assert_valid(&docx);
    docx.save_transitional(system, Path::new("/n/doc.docx"))
        .unwrap();
    let reopened = Docx::open(system, Path::new("/n/doc.docx"), OpenOptions::default()).unwrap();
    markdown::save_markdown(&reopened, system, Path::new(back), options).unwrap();
    system.read_to_string(Path::new(back)).unwrap()
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
    assert!(!system.exists(Path::new("/n/back_media")).unwrap());
}

fn run(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

fn styled_run(properties: &str, text: &str) -> String {
    format!(r#"<w:r><w:rPr>{properties}</w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

fn paragraph(style: &str, content: &str) -> String {
    format!(r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr>{content}</w:p>"#)
}

fn listed(instance: u8, level: u8, content: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="{instance}"/></w:numPr></w:pPr>{content}</w:p>"#
    )
}

fn cell(content: &str) -> String {
    format!("<w:tc>{content}</w:tc>")
}

fn plain(text: &str) -> String {
    format!("<w:p>{}</w:p>", run(text))
}

fn table(rows: &[String]) -> String {
    format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol/><w:gridCol/></w:tblGrid>{}</w:tbl>",
        rows.concat()
    )
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

/// A Word file written by hand that holds each construct Word to markdown reads: headings, runs
/// of every style, hyperlinks, `HYPERLINK` fields, pictures in `DrawingML` and in `VML`, lists, a code
/// block, a quote, a thematic break, two tables, an HTML import, a text box and the frontmatter.
fn word_file() -> Docx {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let main = docx.main_document_part().clone();
    let content_type = docx.package().content_type(&main).unwrap().to_owned();
    add_part(
        &mut docx,
        &main,
        "/word/styles.xml",
        style_definitions::CONTENT_TYPE,
        style_definitions::RELATIONSHIP_TYPE.transitional,
        STYLES.as_bytes(),
    );
    add_part(
        &mut docx,
        &main,
        "/word/numbering.xml",
        numbering_definitions::CONTENT_TYPE,
        numbering_definitions::RELATIONSHIP_TYPE.transitional,
        NUMBERING.as_bytes(),
    );
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
    let package = docx.package_mut();
    let outer = package
        .relate_external(
            Some(&main),
            "https://example.com/q3",
            RelationshipType::HYPERLINK.transitional,
        )
        .unwrap();
    let frontmatter = PartName::new("/docProps/frontmatter.toml").unwrap();
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

    let body = word_body(&outer, &picture, &picture, &chunk);
    let document = format!("<w:document {NAMESPACES}><w:body>{body}</w:body></w:document>");
    docx.package_mut()
        .set_part_bytes(&main, &content_type, document.as_bytes())
        .unwrap();
    docx
}

/// The body of the Word file; `outer` is the id of the external hyperlink, `logo` and `image` those
/// of the pictures and `chunk` that of the HTML part.
fn word_body(outer: &str, logo: &str, image: &str, chunk: &str) -> String {
    let drawing = format!(
        r#"<w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="914400"/><wp:docPr id="1" name="Picture 1" descr="Chart"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="image"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="{image}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr/></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
    );
    let vml = format!(
        r#"<w:r><w:pict><v:shape alt="Logo"><v:imagedata r:id="{logo}"/></v:shape></w:pict></w:r>"#
    );
    let formatting = [
        run("Plain "),
        styled_run("<w:b/>", "bold"),
        run(" "),
        styled_run("<w:i/>", "italic"),
        run(" "),
        styled_run("<w:b/><w:i/>", "both"),
        run(" "),
        styled_run(r#"<w:rStyle w:val="HTMLCode"/>"#, "code"),
        r#"<w:r><w:tab/><w:t>tabbed</w:t><w:br w:type="textWrapping"/><w:t>wrapped</w:t></w:r>"#
            .to_owned(),
    ]
    .concat();
    let links = format!(
        concat!(
            r#"<w:hyperlink r:id="{outer}" w:tooltip="Tip">{dashboard}</w:hyperlink>{sep}"#,
            r#"<w:hyperlink w:anchor="notes">{below}</w:hyperlink>{sep}"#,
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r>"#,
            r#"<w:r><w:instrText xml:space="preserve"> HYPERLINK "https://example.com/site" </w:instrText></w:r>"#,
            r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r>{site}"#,
            r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>{sep}"#,
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r>"#,
            r#"<w:r><w:instrText xml:space="preserve"> HYPERLINK \l "q3_results" </w:instrText></w:r>"#,
            r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r>{q3}"#,
            r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>{sep}"#,
            r#"<w:fldSimple w:instr=" HYPERLINK &quot;https://x.example&quot; \l &quot;part_two&quot; ">{part}</w:fldSimple>"#,
        ),
        outer = outer,
        dashboard = run("the dashboard"),
        below = run("below"),
        site = run("site"),
        q3 = run("Q3"),
        part = run("part"),
        sep = run(", "),
    );
    let head_row = format!(
        "<w:tr><w:trPr><w:tblHeader/></w:trPr>{}{}</w:tr>",
        cell(&plain("A")),
        cell(&plain("B"))
    );
    let grid = table(&[
        format!("<w:tr>{}{}</w:tr>", cell(&plain("a")), cell(&plain("b"))),
        format!(
            "<w:tr>{}{}</w:tr>",
            cell(&format!("{}{}", plain("c"), plain("d"))),
            cell(&plain("e"))
        ),
    ]);
    let pipe = table(&[
        head_row,
        format!("<w:tr>{}{}</w:tr>", cell(&plain("1")), cell(&plain("2"))),
    ]);
    [
        paragraph("Heading1", &run("Results")),
        paragraph("Heading2", &run("Notes")),
        format!("<w:p>{formatting}</w:p>"),
        format!("<w:p>{links}</w:p>"),
        format!("<w:p>{vml}</w:p>"),
        format!("<w:p>{drawing}</w:p>"),
        listed(1, 0, &run("North")),
        listed(1, 1, &run("Inner")),
        listed(1, 0, &run("South")),
        paragraph("ListParagraph", &run("More about South")),
        listed(3, 0, &run("three")),
        listed(3, 0, &run("four")),
        paragraph("HTMLPreformatted", &run("first")),
        paragraph("HTMLPreformatted", &run("second")),
        paragraph("Quote", &run("Quoted")),
        concat!(
            r#"<w:p><w:pPr><w:pBdr><w:bottom w:val="single" w:sz="6" w:space="1" w:color="auto"/>"#,
            "</w:pBdr></w:pPr></w:p>"
        )
        .to_owned(),
        grid,
        pipe,
        format!(r#"<w:altChunk r:id="{chunk}"/>"#),
        format!(
            "<w:p><w:r><w:pict><v:shape><v:textbox><w:txbxContent>{}</w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p>",
            plain("Boxed")
        ),
    ]
    .concat()
}

#[test]
fn word_converted_to_markdown_and_back_gives_the_same_markdown_twice() {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/n/one")).unwrap();
    system.create_dir_all(Path::new("/n/two")).unwrap();
    let original = word_file();
    assert_valid(&original);
    original
        .save_transitional(&system, Path::new("/n/one/doc.docx"))
        .unwrap();
    let opened = Docx::open(
        &system,
        Path::new("/n/one/doc.docx"),
        OpenOptions::default(),
    )
    .unwrap();
    let options = options();
    markdown::save_markdown(&opened, &system, Path::new("/n/one/doc.md"), options).unwrap();
    let first = system.read_to_string(Path::new("/n/one/doc.md")).unwrap();
    assert_eq!(
        first,
        "---\ntitle: Quarterly report\n---\n# Results\n\n## Notes\n\nPlain **bold** *italic* ***both*** `code`\ttabbed\\\nwrapped\n\n[the dashboard](https://example.com/q3 \"Tip\"), [below](#notes), [site](https://example.com/site), [Q3](#q3-results), [part](https://x.example#part_two)\n\n![Logo](doc_media/image1.png)\n\n![Chart](doc_media/image1.png)\n\n- North\n\n  - Inner\n\n- South\n\n  More about South\n\n3. three\n4. four\n\n```\nfirst\nsecond\n```\n\n> Quoted\n\n***\n\n+-----+-----+\n| a   | b   |\n+-----+-----+\n| c   | e   |\n|     |     |\n| d   |     |\n+-----+-----+\n\n| A   | B   |\n| --- | --- |\n| 1   | 2   |\n\n<div>Imported</div>\n\nBoxed\n"
    );

    let again = markdown::from_markdown(&system, Path::new("/n/one/doc.md"), options).unwrap();
    assert_valid(&again);
    markdown::save_markdown(&again, &system, Path::new("/n/two/doc.md"), options).unwrap();
    assert_eq!(
        system.read_to_string(Path::new("/n/two/doc.md")).unwrap(),
        first
    );
    for folder in ["one", "two"] {
        assert_eq!(
            read_bytes(&system, &format!("/n/{folder}/doc_media/image1.png")),
            tiny_png()
        );
        assert!(
            !system
                .exists(Path::new(&format!("/n/{folder}/doc_media/image2.png")))
                .unwrap()
        );
    }
}
