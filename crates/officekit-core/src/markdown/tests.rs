use alloc::string::String;
use alloc::vec::Vec;
use std::io::ErrorKind;
use std::path::Path;

use archie_core::parser::ast::{
    Block, Document, Heading, Inline, Line, MarkdownBlock, Paragraph as ArchieParagraph, Section,
    Text,
};
use officekit_markup::wordprocessing::{
    BlockContent, Paragraph, ParagraphContent, RunPropertiesContent, StyleDefinition, TextRun,
    TextRunContent,
};
use officekit_packaging::package::OpenOptions;
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::Target;
use officekit_parts::style_definitions::{self, CONTENT_TYPE, RELATIONSHIP_TYPE};
use os_shim::System as _;
use os_shim::mock::MemorySystem;

use super::{MarkdownError, MarkdownOptions, from_markdown};
use crate::docx::Docx;

/// A border as `val/sz/space/color`.
macro_rules! edge {
    ($border:expr) => {
        $border.as_ref().map_or_else(String::new, |border| {
            format!(
                "{}/{}/{}/{}",
                value(border.val.as_ref()),
                value(border.sz.as_ref()),
                value(border.space.as_ref()),
                value(border.color.as_ref())
            )
        })
    };
}

/// The sample of the design: a heading, a paragraph with strong and emphasis, a code block, a
/// block quote, and a thematic break.
const SAMPLE: &str =
    "# Results\n\nRevenue grew **12%** in *Q3*.\n\n```\nfirst\nsecond\n```\n\n> Quoted.\n\n***\n";

/// A YAML frontmatter no entry of reads, before the heading `T`.
const BROKEN_FRONTMATTER: &str = "---\n{a: 1, b: [}\n---\n# T\n";

/// The file `source` converts to, read from `/notes/report.md`.
fn converted(source: &str, options: MarkdownOptions) -> Result<Docx, MarkdownError> {
    from_markdown(&notes(source), Path::new("/notes/report.md"), options)
}

/// A section of `blocks` under no heading, as the only section of a document.
fn document_of(blocks: Vec<Block>) -> Document {
    Document::new(
        MarkdownBlock::new(alloc::vec![Section::new(None, blocks, Vec::new())]),
        None,
    )
}

/// The Word document `document` converts to.
fn from_archie(document: &Document) -> Docx {
    Docx::from_archie(
        &MemorySystem::new(),
        Path::new("/"),
        document,
        MarkdownOptions::default(),
    )
    .unwrap()
}

/// `text` as a heading of plain text.
fn heading(text: &str) -> Heading {
    Heading::new(alloc::vec![Inline::Text(Text::new(text.to_owned()))])
}

/// A memory filesystem holding `source` at `/notes/report.md`.
fn notes(source: &str) -> MemorySystem {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/notes")).unwrap();
    system
        .write(Path::new("/notes/report.md"), source.as_bytes())
        .unwrap();
    system
}

/// The paragraphs of the main document, each as its style (`-` for none), a colon, and its runs
/// joined by `|`.
fn outline(docx: &Docx) -> Vec<String> {
    let content = docx.main_document().unwrap().body.unwrap().content;
    let paragraphs: Vec<String> = content
        .iter()
        .filter_map(|block| {
            if let BlockContent::Paragraph(paragraph) = block {
                Some(paragraph_line(paragraph))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(paragraphs.len(), content.len());
    paragraphs
}

fn paragraph_line(paragraph: &Paragraph) -> String {
    let style = paragraph
        .properties
        .as_ref()
        .and_then(|properties| properties.referenced_paragraph_style.as_ref())
        .and_then(|style| style.val.as_deref())
        .unwrap_or("-");
    let runs: Vec<String> = paragraph
        .content
        .iter()
        .map(|content| {
            if let ParagraphContent::TextRun(run) = content {
                run_text(run)
            } else {
                format!("{content:?}")
            }
        })
        .collect();
    format!("{style}: {}", runs.join("|"))
}

/// `docx` written through a memory filesystem and opened again.
fn reopened(docx: &Docx) -> Docx {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/notes")).unwrap();
    docx.save_transitional(&system, Path::new("/notes/report.docx"))
        .unwrap();
    Docx::open(
        &system,
        Path::new("/notes/report.docx"),
        OpenOptions::default(),
    )
    .unwrap()
}

fn run_mark(property: &RunPropertiesContent) -> String {
    if let RunPropertiesContent::ReferencedCharacterStyle(style) = property {
        style.val.clone().unwrap_or_default()
    } else if matches!(property, RunPropertiesContent::Bold(_)) {
        "b".to_owned()
    } else if matches!(property, RunPropertiesContent::Italics(_)) {
        "i".to_owned()
    } else {
        format!("{property:?}")
    }
}

fn run_piece(content: &TextRunContent) -> String {
    if let TextRunContent::Text(characters) = content {
        assert_eq!(characters.space.as_deref(), Some("preserve"));
        characters.value.clone()
    } else if matches!(content, TextRunContent::TabCharacter(_)) {
        "<tab/>".to_owned()
    } else if matches!(content, TextRunContent::Break(_)) {
        "<br/>".to_owned()
    } else {
        format!("{content:?}")
    }
}

/// A run as its marks (`b`, `i`, a character style) joined by `,`, a colon, and its content, or
/// its content alone when it has no mark.
fn run_text(run: &TextRun) -> String {
    let marks: Vec<String> = run
        .properties
        .iter()
        .flat_map(|properties| &properties.content)
        .map(run_mark)
        .collect();
    let text: String = run.content.iter().map(run_piece).collect();
    if marks.is_empty() {
        text
    } else {
        format!("{}:{text}", marks.join(","))
    }
}

/// A style as its id, name, type and formatting, joined by `|`.
fn style_line(style: &StyleDefinition) -> String {
    let mut parts = alloc::vec![
        style.style_id.clone().unwrap_or_default(),
        style
            .primary_style_name
            .as_ref()
            .and_then(|name| name.val.clone())
            .unwrap_or_default(),
        value(style.r#type.as_ref()).to_owned(),
    ];
    if style.default.is_some() {
        parts.push(format!("default={}", value(style.default.as_ref())));
    }
    if let Some(parent) = style
        .parent_style_id
        .as_ref()
        .and_then(|id| id.val.as_ref())
    {
        parts.push(format!("basedOn={parent}"));
    }
    if let Some(paragraph) = &style.paragraph_properties_p_pr_general {
        if paragraph.keep_paragraph_with_next_paragraph.is_some() {
            parts.push("keepNext".to_owned());
        }
        if let Some(spacing) = &paragraph.spacing_between_lines_and_above_below_paragraph {
            parts.push(format!(
                "spacing={}/{}",
                value(spacing.before.as_ref()),
                value(spacing.after.as_ref())
            ));
        }
        if let Some(indentation) = &paragraph.paragraph_indentation {
            parts.push(format!("left={}", value(indentation.left.as_ref())));
        }
        if let Some(level) = &paragraph.associated_outline_level {
            parts.push(format!("outline={}", value(level.val.as_ref())));
        }
    }
    for property in style.run_properties.iter().flat_map(|run| &run.content) {
        parts.push(style_mark(property));
    }
    if let Some(borders) = style
        .previous_table_properties
        .as_ref()
        .and_then(|table| table.table_borders.as_ref())
    {
        parts.push(format!(
            "top={} left={} bottom={} right={} insideH={} insideV={}",
            edge!(borders.paragraph_border_above_identical_paragraphs),
            edge!(borders.left_paragraph_border),
            edge!(borders.paragraph_border_below_identical_paragraphs),
            edge!(borders.right_paragraph_border),
            edge!(borders.table_inside_horizontal_edges_border),
            edge!(borders.table_inside_vertical_edges_border),
        ));
    }
    parts.join("|")
}

fn style_mark(property: &RunPropertiesContent) -> String {
    if let RunPropertiesContent::RunFonts(fonts) = property {
        format!(
            "fonts={}/{}/{}/{}",
            fonts.ascii.as_deref().unwrap_or(""),
            fonts.h_ansi.as_deref().unwrap_or(""),
            fonts.cs.as_deref().unwrap_or(""),
            fonts.east_asia.as_deref().unwrap_or("")
        )
    } else if let RunPropertiesContent::NonComplexScriptFontSize(size) = property {
        format!("sz={}", value(size.val.as_ref()))
    } else if let RunPropertiesContent::ComplexScriptFontSize(size) = property {
        format!("szCs={}", value(size.val.as_ref()))
    } else if let RunPropertiesContent::RunContentColor(color) = property {
        format!("color={}", value(color.val.as_ref()))
    } else if let RunPropertiesContent::Underline(underline) = property {
        format!("u={}", value(underline.val.as_ref()))
    } else {
        run_mark(property)
    }
}

/// The text of an optional simple value, empty when it is absent.
fn value<T>(value: Option<&T>) -> &str
where
    T: AsRef<str>,
{
    value.map_or("", AsRef::as_ref)
}

#[test]
fn the_sample_converts_to_a_heading_a_paragraph_code_a_quote_and_a_break() {
    let docx = reopened(&converted(SAMPLE, MarkdownOptions::default()).unwrap());
    assert_eq!(
        outline(&docx),
        [
            "Heading1: Results",
            "-: Revenue grew |b:12%| in |i:Q3|.",
            "HTMLPreformatted: first",
            "HTMLPreformatted: second",
            "Quote: Quoted.",
            "-: ",
        ]
    );
    let content = docx.main_document().unwrap().body.unwrap().content;
    let borders = content
        .last()
        .and_then(|block| {
            if let BlockContent::Paragraph(paragraph) = block {
                paragraph.properties.as_ref()?.paragraph_borders.clone()
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        edge!(borders.paragraph_border_below_identical_paragraphs),
        "single/6/1/auto"
    );
    assert!(
        borders
            .paragraph_border_above_identical_paragraphs
            .is_none()
    );
    assert!(borders.left_paragraph_border.is_none());
    assert!(borders.right_paragraph_border.is_none());
}

#[test]
fn a_section_ten_deep_takes_the_ninth_heading_style() {
    let mut section: Section = Section::new(Some(heading("h10")), Vec::new(), Vec::new());
    for depth in (1..10_usize).rev() {
        section = Section::new(
            Some(heading(&format!("h{depth}"))),
            Vec::new(),
            alloc::vec![section],
        );
    }
    let document = Document::new(MarkdownBlock::new(alloc::vec![section]), None);
    assert_eq!(
        outline(&from_archie(&document)),
        [
            "Heading1: h1",
            "Heading2: h2",
            "Heading3: h3",
            "Heading4: h4",
            "Heading5: h5",
            "Heading6: h6",
            "Heading7: h7",
            "Heading8: h8",
            "Heading9: h9",
            "Heading9: h10",
        ]
    );
}

#[test]
fn the_lines_of_a_paragraph_join_with_a_space_or_after_a_hard_break_with_none() {
    let docx = converted(
        "first\nsecond\n\nbefore\\\nafter\n\n*a  \nb*\n\n[c  \nd](x)\n",
        MarkdownOptions::default(),
    )
    .unwrap();
    assert_eq!(
        outline(&docx),
        [
            "-: first| |second",
            "-: before|<br/>|after",
            "-: i:a|i:<br/>|i:b",
            "-: c|<br/>|d",
        ]
    );
}

#[test]
fn a_tab_and_a_line_ending_in_text_become_a_tab_and_a_break() {
    let line = Line::new(alloc::vec![Inline::Text(Text::new("a\tb\nc".to_owned()))]);
    let paragraph = Block::Paragraph(ArchieParagraph::new(alloc::vec![line]));
    assert_eq!(
        outline(&from_archie(&document_of(alloc::vec![paragraph]))),
        ["-: a<tab/>b<br/>c"]
    );
}

#[test]
fn inline_code_takes_the_code_style_and_nested_marks_add_up() {
    let docx = converted("Run `cargo` now, ***both***.\n", MarkdownOptions::default()).unwrap();
    assert_eq!(outline(&docx), ["-: Run |HTMLCode:cargo| now, |b,i:both|."]);
}

#[test]
fn each_line_of_a_block_quote_is_a_quote_paragraph_and_an_empty_one_is_empty() {
    let docx = converted("> a\n>\n> b\n", MarkdownOptions::default()).unwrap();
    assert_eq!(outline(&docx), ["Quote: a", "Quote: ", "Quote: b"]);
}

#[test]
fn a_frontmatter_that_does_not_read_is_passed_over_unless_it_is_to_be_carried() {
    let docx = converted(BROKEN_FRONTMATTER, MarkdownOptions::default()).unwrap();
    assert_eq!(outline(&docx), ["Heading1: T"]);
    let carried = MarkdownOptions {
        frontmatter: true,
        ..MarkdownOptions::default()
    };
    let error = converted(BROKEN_FRONTMATTER, carried).unwrap_err();
    assert!(matches!(
        error,
        MarkdownError::Frontmatter(frontmatter) if frontmatter.text == "{a: 1, b: [}"
    ));
}

#[test]
fn lists_tables_links_images_and_html_do_not_fail_and_a_link_keeps_its_text() {
    let docx = converted(
        "- a\n\n[x](https://example.com)\n\n<div>h</div>\n\n| a |\n|---|\n| b |\n\n![alt](pic.png)\n",
        MarkdownOptions::default(),
    )
    .unwrap();
    assert_eq!(outline(&docx), ["-: x", "-: "]);
}

#[test]
fn a_file_that_cannot_be_read_is_an_io_error_naming_it() {
    let failure = from_markdown(
        &MemorySystem::new(),
        Path::new("/missing.md"),
        MarkdownOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(
        failure,
        MarkdownError::Io { error, path }
            if error.kind() == ErrorKind::NotFound && path == Path::new("/missing.md")
    ));
}

#[test]
fn every_converted_document_has_the_styles_part_with_the_built_in_styles() {
    let docx = reopened(&converted("x\n", MarkdownOptions::default()).unwrap());
    let package = docx.package();
    let styles = PartName::new("/word/styles.xml").unwrap();
    assert_eq!(package.content_type(&styles), Some(CONTENT_TYPE));
    let related = package
        .related_by_type(Some(docx.main_document_part()), &RELATIONSHIP_TYPE)
        .unwrap();
    let targets: Vec<&Target> = related.iter().map(|part| &part.target).collect();
    assert_eq!(targets, [&Target::Part(styles.clone())]);
    let root = style_definitions::load(package, &styles, OpenOptions::default()).unwrap();
    let lines: Vec<String> = root.style_definition.iter().map(style_line).collect();
    let mut expected = alloc::vec![
        "Normal|Normal|paragraph|default=true|spacing=/160|fonts=Calibri/Calibri/Calibri/|sz=22|szCs=22"
            .to_owned(),
    ];
    for (index, size) in ["40", "32", "28", "26", "24", "22", "22", "22", "22"]
        .iter()
        .enumerate()
    {
        let level = index + 1;
        expected.push(format!(
            "Heading{level}|heading {level}|paragraph|basedOn=Normal|keepNext|spacing=240/120|outline={index}|b|sz={size}|szCs={size}"
        ));
    }
    let single = "single/4/0/auto";
    expected.extend([
        "Quote|Quote|paragraph|basedOn=Normal|left=720|i".to_owned(),
        "HTMLPreformatted|HTML Preformatted|paragraph|basedOn=Normal|spacing=/0|fonts=Consolas/Consolas/Consolas/|sz=20|szCs=20"
            .to_owned(),
        "ListParagraph|List Paragraph|paragraph|basedOn=Normal".to_owned(),
        "HTMLCode|HTML Code|character|fonts=Consolas/Consolas/Consolas/|sz=20|szCs=20".to_owned(),
        "Hyperlink|Hyperlink|character|color=0563C1|u=single".to_owned(),
        format!(
            "TableGrid|Table Grid|table|top={single} left={single} bottom={single} right={single} insideH={single} insideV={single}"
        ),
    ]);
    assert_eq!(lines, expected);
}
