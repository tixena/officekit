use alloc::string::String;
use alloc::vec::Vec;
use std::io::{Cursor, ErrorKind};
use std::path::Path;

use arky::parser::ast::{
    Alignment, Block, Document, Heading, Inline, Item, Line, List, MarkdownBlock,
    Paragraph as ArkyParagraph, Section, Table as ArkyTable, TableCell as ArkyTableCell,
    TableFormat, TableRow as ArkyTableRow, Text,
};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use image_webp::{ColorType as WebPColor, WebPEncoder};
use officekit::parts::numbering_definitions;
use officekit::parts::style_definitions::{self, CONTENT_TYPE, RELATIONSHIP_TYPE};
use officekit::validation::validator::Validator;
use officekit_markup::wordprocessing::{
    BlockContent, ContentCellContent, ContentRowContent, Paragraph, ParagraphContent,
    RunPropertiesContent, StyleDefinition, Table, TableCell, TableRow, TableRowPropertiesContent,
    TextRun, TextRunContent,
};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::Element;
use officekit_xml::name::{Namespace, OfficeVersion};
use officekit_xml::packaging::package::OpenOptions;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::{Relationship, RelationshipType, Target, TargetMode};
use os_shim::System as _;
use os_shim::WriteMode;
use os_shim::mock::MemorySystem;
use png::{BitDepth, ColorType, Decoder, Encoder};

use super::{
    DocxMarkdown as _, FRONTMATTER_RELATIONSHIP, MarkdownError, MarkdownOptions, from_markdown,
    from_markdown_text,
};
use officekit::docx::Docx;

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

/// The sample of the design of links: a TOML frontmatter, a heading, a link to a web address with
/// a title, a link to the heading's anchor, an HTML block, and HTML inside a sentence.
const LINK_SAMPLE: &str = "+++\ntitle = \"Q3\"\n+++\n# Q3 results\n\nSee [the dashboard](https://example.com/q3 \"Live\") and [above](#q3-results).\n\n<div align=\"center\">Signed</div>\n\nWater is H<sub>2</sub>O.\n";

/// The sample of the design of lists and tables: an ordered list holding a bullet list, and a
/// table of a head row and a row, aligned left and right.
const LIST_SAMPLE: &str = "1. one\n   - inner\n2. two\n\n| Region | Revenue |\n| :----- | ------: |\n| North  | 1,200   |\n";

const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const PICTURE: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const SVG: &str = "http://schemas.microsoft.com/office/drawing/2016/SVG/main";
const WORDPROCESSING: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const WORDPROCESSING_DRAWING: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";

/// The SVG of the design's sample of pictures: a blue rectangle 120 by 80.
const DIAGRAM: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80"><rect width="120" height="80" fill="#2b6cb0"/></svg>"##;

/// [`DIAGRAM`] percent-encoded in a `data:` address.
const DIAGRAM_ADDRESS: &str = "data:image/svg+xml,%3Csvg%20xmlns=%22http://www.w3.org/2000/svg%22%20width=%22120%22%20height=%2280%22%3E%3Crect%20width=%22120%22%20height=%2280%22%20fill=%22%232b6cb0%22/%3E%3C/svg%3E";

/// The design's sample of pictures: a PNG 300 by 100, an SVG, a WebP 4 by 3, and a picture on the
/// web, read from `/n/p.md`.
const PICTURE_SAMPLE: &str = "![Logo](logo.png) ![Diagram](diagram.svg) ![Team](team.webp) ![Web](https://example.com/a.png)\n";

/// Each block as a line: a paragraph as [`paragraph_line`] writes it; a table as `tbl`, its style,
/// its width type and its column count, then a line per row, `tr` (`tr header` for a row marked
/// `w:tblHeader`) and its cells' blocks, the blocks of a cell joined by ` / ` and the cells by
/// ` || `; a `w:altChunk` as `altChunk` and the target of its relationship.
fn block_lines(content: &[BlockContent], relationships: &[Relationship]) -> Vec<String> {
    let mut lines = Vec::new();
    for block in content {
        if let BlockContent::Paragraph(paragraph) = block {
            lines.push(paragraph_line(paragraph, relationships));
        } else if let BlockContent::Table(table) = block {
            lines.extend(table_lines(table, relationships));
        } else if let BlockContent::AnchorForImportedExternalContent(chunk) = block {
            lines.push(format!(
                "altChunk {}",
                target_of(relationships, chunk.id.as_deref())
            ));
        } else {
            lines.push(format!("{block:?}"));
        }
    }
    lines
}

/// The bookmark of each paragraph that opens with a `w:bookmarkStart` and closes with the
/// `w:bookmarkEnd` of the same id, as its id and its name.
fn bookmarks(docx: &Docx) -> Vec<String> {
    let content = docx.main_document().unwrap().body.unwrap().content;
    content
        .iter()
        .filter_map(|block| {
            if let BlockContent::Paragraph(paragraph) = block
                && let Some(ParagraphContent::BookmarkStart(start)) = paragraph.content.first()
                && let Some(ParagraphContent::BookmarkEnd(end)) = paragraph.content.last()
            {
                assert_eq!(start.id, end.id);
                Some(format!(
                    "{} {}",
                    value(start.id.as_ref()),
                    value(start.name.as_ref())
                ))
            } else {
                None
            }
        })
        .collect()
}

/// A table cell of one paragraph of plain `text`.
fn cell_of(text: &str) -> ArkyTableCell {
    ArkyTableCell::new(MarkdownBlock::new(alloc::vec![Section::new(
        None,
        alloc::vec![plain(text)],
        Vec::new()
    )]))
}

/// A paragraph's content as text: a run as [`run_text`] writes it, a hyperlink as `<`, its
/// target (`#` and its anchor, or its relationship's target), its tooltip in quotes, `>`, its runs
/// joined by `|`, and `</>`; `None` for a bookmark's start or end.
fn content_text(content: &ParagraphContent, relationships: &[Relationship]) -> Option<String> {
    if let ParagraphContent::TextRun(run) = content {
        Some(run_text(run))
    } else if let ParagraphContent::Hyperlink(hyperlink) = content {
        let target = hyperlink.anchor.as_ref().map_or_else(
            || target_of(relationships, hyperlink.id.as_deref()),
            |anchor| format!("#{anchor}"),
        );
        let tooltip = hyperlink
            .tooltip
            .as_ref()
            .map_or_else(String::new, |tooltip| format!(" \"{tooltip}\""));
        let runs: Vec<String> = hyperlink
            .content
            .iter()
            .filter_map(|inner| content_text(inner, relationships))
            .collect();
        Some(format!("<{target}{tooltip}>{}</>", runs.join("|")))
    } else if matches!(
        content,
        ParagraphContent::BookmarkStart(_) | ParagraphContent::BookmarkEnd(_)
    ) {
        None
    } else {
        Some(format!("{content:?}"))
    }
}

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
fn from_arky(document: &Document) -> Docx {
    Docx::from_arky(
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

/// The relationships of the main document part, as its relationship part holds them.
fn main_relationships(docx: &Docx) -> Vec<Relationship> {
    docx.package()
        .relationship_sets()
        .into_iter()
        .find(|(source, _)| source.as_ref() == Some(docx.main_document_part()))
        .map_or_else(Vec::new, |(_, relationships)| relationships)
}

/// A memory filesystem holding `source` at `/notes/report.md`.
fn notes(source: &str) -> MemorySystem {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/notes")).unwrap();
    system
        .write(
            Path::new("/notes/report.md"),
            source.as_bytes(),
            WriteMode::Replace,
        )
        .unwrap();
    system
}

/// The numbering part: each level of each abstract numbering as `abstract`, the numbering's id,
/// the level, its format, its text, its start and its indents; then each instance as `num`, its
/// id, `->` and its abstract numbering, and each start override as the level, `=` and the start.
fn numbering_lines(docx: &Docx) -> Vec<String> {
    let name = PartName::new("/word/numbering.xml").unwrap();
    let root = numbering_definitions::load(docx.package(), &name, OpenOptions::default()).unwrap();
    let mut lines = Vec::new();
    for definition in &root.abstract_numbering_definition {
        for level in &definition.numbering_level_override_definition {
            let indentation = level
                .paragraph_properties_p_pr_general
                .as_ref()
                .and_then(|paragraph| paragraph.paragraph_indentation.as_ref());
            lines.push(format!(
                "abstract {} {}: {} {} start={} left={} hanging={}",
                value(definition.abstract_num_id.as_ref()),
                value(level.ilvl.as_ref()),
                value(
                    level
                        .numbering_format
                        .as_ref()
                        .and_then(|format| format.val.as_ref())
                ),
                value(
                    level
                        .numbering_level_text
                        .as_ref()
                        .and_then(|text| text.val.as_ref())
                ),
                value(
                    level
                        .starting_value
                        .as_ref()
                        .and_then(|start| start.val.as_ref())
                ),
                value(indentation.and_then(|indent| indent.left.as_ref())),
                value(indentation.and_then(|indent| indent.hanging.as_ref())),
            ));
        }
    }
    for instance in &root.numbering_definition_instance {
        let overrides: Vec<String> = instance
            .numbering_level_definition_override
            .iter()
            .map(|level| {
                format!(
                    " {}={}",
                    value(level.ilvl.as_ref()),
                    value(
                        level
                            .numbering_level_starting_value_override
                            .as_ref()
                            .and_then(|start| start.val.as_ref())
                    )
                )
            })
            .collect();
        lines.push(format!(
            "num {} -> {}{}",
            value(instance.num_id.as_ref()),
            value(
                instance
                    .abstract_numbering_definition_reference
                    .val
                    .as_ref()
            ),
            overrides.concat()
        ));
    }
    lines
}

/// The numbering instances of the numbering part, as [`numbering_lines`] writes them.
fn numbering_instances(docx: &Docx) -> Vec<String> {
    numbering_lines(docx)
        .into_iter()
        .filter(|line| line.starts_with("num "))
        .collect()
}

/// The blocks of the main document, as [`block_lines`] writes them.
fn outline(docx: &Docx) -> Vec<String> {
    let relationships = main_relationships(docx);
    let content = docx.main_document().unwrap().body.unwrap().content;
    block_lines(&content, &relationships)
}

/// A paragraph as its style (`-` for none); its numbering as `num=` and its instance and level, its
/// left indent as `left=`, its alignment as `jc=`; a colon; and its content, as [`content_text`]
/// writes it, joined by `|`.
fn paragraph_line(paragraph: &Paragraph, relationships: &[Relationship]) -> String {
    let own = paragraph.properties.as_deref();
    let mut marks = alloc::vec![
        own.and_then(|properties| properties.referenced_paragraph_style.as_ref())
            .and_then(|style| style.val.clone())
            .unwrap_or_else(|| "-".to_owned())
    ];
    if let Some(numbering) = own.and_then(|properties| {
        properties
            .numbering_definition_instance_reference_num_pr
            .as_ref()
    }) {
        marks.push(format!(
            "num={}/{}",
            value(
                numbering
                    .numbering_definition_instance_reference_num_id
                    .as_ref()
                    .and_then(|instance| instance.val.as_ref())
            ),
            value(
                numbering
                    .numbering_level_reference
                    .as_ref()
                    .and_then(|level| level.val.as_ref())
            )
        ));
    }
    if let Some(indentation) = own.and_then(|properties| properties.paragraph_indentation.as_ref())
    {
        marks.push(format!("left={}", value(indentation.left.as_ref())));
    }
    if let Some(alignment) = own.and_then(|properties| properties.paragraph_alignment.as_ref()) {
        marks.push(format!("jc={}", value(alignment.val.as_ref())));
    }
    let content: Vec<String> = paragraph
        .content
        .iter()
        .filter_map(|content| content_text(content, relationships))
        .collect();
    format!("{}: {}", marks.join(" "), content.join("|"))
}

/// The bytes and the content type of the part at `path`.
fn part(docx: &Docx, path: &str) -> (String, String) {
    let name = PartName::new(path).unwrap();
    let package = docx.package();
    (
        String::from_utf8(package.part_bytes(&name).unwrap()).unwrap(),
        package.content_type(&name).unwrap_or_default().to_owned(),
    )
}

/// A paragraph of plain `text`.
fn plain(text: &str) -> Block {
    Block::Paragraph(ArkyParagraph::new(alloc::vec![Line::new(alloc::vec![
        Inline::Text(Text::new(text.to_owned()))
    ])]))
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

/// A table as `tbl` and its rows, as [`block_lines`] writes it.
fn table_lines(table: &Table, relationships: &[Relationship]) -> Vec<String> {
    let mut lines = alloc::vec![format!(
        "tbl {} {} {}",
        value(
            table
                .properties
                .referenced_table_style
                .as_ref()
                .and_then(|style| style.val.as_ref())
        ),
        value(
            table
                .properties
                .preferred_table_width
                .as_ref()
                .and_then(|width| width.r#type.as_ref())
        ),
        table.grid.column_definition.len()
    )];
    for row in &table.content_row_content {
        if let ContentRowContent::TableRow(written) = row {
            let header = written
                .properties
                .iter()
                .flat_map(|properties| &properties.content)
                .any(|item| matches!(item, TableRowPropertiesContent::RepeatTableRow(_)));
            let cells: Vec<String> = written
                .content
                .iter()
                .map(|content| {
                    if let ContentCellContent::TableCell(cell) = content {
                        block_lines(&cell.content, relationships).join(" / ")
                    } else {
                        format!("{content:?}")
                    }
                })
                .collect();
            let kind = if header { "tr header" } else { "tr" };
            lines.push(format!("{kind}: {}", cells.join(" || ")));
        } else {
            lines.push(format!("{row:?}"));
        }
    }
    lines
}

/// The target of the relationship `id` names, or `id` itself when none does.
fn target_of(relationships: &[Relationship], id: Option<&str>) -> String {
    relationships
        .iter()
        .find(|relationship| Some(relationship.id.as_str()) == id)
        .map_or_else(
            || format!("{id:?}"),
            |relationship| relationship.target.clone(),
        )
}

/// The table rows of the main document, each as its cells.
fn table_rows(docx: &Docx) -> Vec<Vec<TableCell>> {
    let content = docx.main_document().unwrap().body.unwrap().content;
    content
        .iter()
        .filter_map(|block| {
            if let BlockContent::Table(table) = block {
                Some(&table.content_row_content)
            } else {
                None
            }
        })
        .flatten()
        .filter_map(|item| {
            if let ContentRowContent::TableRow(row) = item {
                Some(row_cells(row))
            } else {
                None
            }
        })
        .collect()
}

/// The cells of `row`.
fn row_cells(row: &TableRow) -> Vec<TableCell> {
    row.content
        .iter()
        .filter_map(|content| {
            if let ContentCellContent::TableCell(cell) = content {
                Some(cell.clone())
            } else {
                None
            }
        })
        .collect()
}

/// The value of the unqualified attribute `local` of `element`, `-` when it has none.
fn attribute<'tree>(element: &'tree Element, local: &str) -> &'tree str {
    element.attribute(&Namespace::None, local).unwrap_or("-")
}

/// Every element of the namespace `uri` named `local` at `element` and below it, in document
/// order, added to `found`.
fn descendants<'tree>(
    element: &'tree Element,
    uri: &str,
    local: &str,
    found: &mut Vec<&'tree Element>,
) {
    if element.name.local == local && element.name.namespace == Namespace::from_uri(uri) {
        found.push(element);
    }
    for child in element.child_elements() {
        descendants(child, uri, local, found);
    }
}

/// The first element [`descendants`] finds.
fn first<'tree>(element: &'tree Element, uri: &str, local: &str) -> Option<&'tree Element> {
    let mut found = Vec::new();
    descendants(element, uri, local, &mut found);
    found.first().copied()
}

/// A memory filesystem holding `source` at `/n/p.md`, and each of `files`, a name and its bytes,
/// in `/n`.
fn picture_notes(source: &str, files: &[(&str, &[u8])]) -> MemorySystem {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/n")).unwrap();
    system
        .write(Path::new("/n/p.md"), source.as_bytes(), WriteMode::Replace)
        .unwrap();
    for &(name, bytes) in files {
        system
            .write(&Path::new("/n").join(name), bytes, WriteMode::Replace)
            .unwrap();
    }
    system
}

/// The Word document `/n/p.md` of `system` converts to, written and opened again.
fn picture_docx(system: &MemorySystem) -> Docx {
    reopened(&from_markdown(system, Path::new("/n/p.md"), MarkdownOptions::default()).unwrap())
}

/// A picture as its `wp:docPr` id, name, description and title; its frame, `cx` by `cy`; and the
/// target of each relationship its `a:blip` names, as `embed=` or `link=`, and of its
/// `asvg:svgBlip`, as `svg=`. Its `pic:cNvPr` must match its `wp:docPr`, and its `a:xfrm` extent
/// its `wp:extent`.
fn picture_line(drawing: &Element, relationships: &[Relationship]) -> String {
    let properties = first(drawing, WORDPROCESSING_DRAWING, "docPr").unwrap();
    let non_visual = first(drawing, PICTURE, "cNvPr").unwrap();
    assert_eq!(
        (attribute(non_visual, "id"), attribute(non_visual, "name")),
        (attribute(properties, "id"), attribute(properties, "name"))
    );
    let extent = first(drawing, WORDPROCESSING_DRAWING, "extent").unwrap();
    let transform = first(first(drawing, DRAWING, "xfrm").unwrap(), DRAWING, "ext").unwrap();
    assert_eq!(
        (attribute(transform, "cx"), attribute(transform, "cy")),
        (attribute(extent, "cx"), attribute(extent, "cy"))
    );
    let relationship = Namespace::from_uri(RELATIONSHIPS);
    let blip = first(drawing, DRAWING, "blip").unwrap();
    let named: Vec<String> = blip
        .attributes
        .iter()
        .filter(|named| named.name.namespace == relationship)
        .map(|named| {
            format!(
                "{}={}",
                named.name.local,
                target_of(relationships, Some(&named.value))
            )
        })
        .collect();
    let svg = first(blip, SVG, "svgBlip").map_or_else(String::new, |svg_blip| {
        format!(
            " svg={}",
            target_of(relationships, svg_blip.attribute(&relationship, "embed"))
        )
    });
    format!(
        "{} {:?} {:?} {:?} {}x{} {}{svg}",
        attribute(properties, "id"),
        attribute(properties, "name"),
        attribute(properties, "descr"),
        attribute(properties, "title"),
        attribute(extent, "cx"),
        attribute(extent, "cy"),
        named.join(" ")
    )
}

/// Each picture of the main document, as [`picture_line`] writes it.
fn pictures(docx: &Docx) -> Vec<String> {
    let relationships = main_relationships(docx);
    let root = docx
        .package()
        .tree_at(docx.main_document_part())
        .unwrap()
        .root;
    let mut drawings = Vec::new();
    descendants(&root, WORDPROCESSING, "drawing", &mut drawings);
    drawings
        .iter()
        .map(|drawing| picture_line(drawing, &relationships))
        .collect()
}

/// An opaque grey PNG `width` by `height`.
fn png_of_size(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, width, height);
    encoder.set_color(ColorType::Rgb);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let samples = alloc::vec![128_u8; usize::try_from(width * height * 3).unwrap()];
    writer.write_image_data(&samples).unwrap();
    writer.finish().unwrap();
    bytes
}

/// The samples of the PNG `bytes`, with its width, its height and its color type.
fn png_samples(bytes: &[u8]) -> (u32, u32, ColorType, Vec<u8>) {
    let mut reader = Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut samples = alloc::vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut samples).unwrap();
    samples.truncate(info.buffer_size());
    (info.width, info.height, info.color_type, samples)
}

/// Twelve pixels of red, green, blue and alpha, a picture 4 by 3.
fn team_pixels() -> Vec<u8> {
    (0..12_u8)
        .flat_map(|index| [index * 20, 255 - index * 20, index * 7, 128 + index])
        .collect()
}

/// `pixels`, red, green, blue and alpha, `width` by `height`, as a lossless WebP.
fn webp_of(pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    WebPEncoder::new(&mut bytes)
        .encode(pixels, width, height, WebPColor::Rgba8)
        .unwrap();
    bytes
}

/// The validator's errors for `docx`, for Microsoft 365.
fn validation_errors(docx: &Docx) -> Vec<String> {
    Validator::new(OfficeVersion::Microsoft365)
        .validate(docx.package())
        .iter()
        .map(|error| format!("{error:?}"))
        .collect()
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
        outline(&from_arky(&document)),
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
            "-: <x>Hyperlink:c|Hyperlink:<br/>|Hyperlink:d</>",
        ]
    );
}

#[test]
fn a_tab_and_a_line_ending_in_text_become_a_tab_and_a_break() {
    let line = Line::new(alloc::vec![Inline::Text(Text::new("a\tb\nc".to_owned()))]);
    let paragraph = Block::Paragraph(ArkyParagraph::new(alloc::vec![line]));
    assert_eq!(
        outline(&from_arky(&document_of(alloc::vec![paragraph]))),
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
fn a_picture_whose_file_is_missing_fails_the_conversion_naming_its_target() {
    let failure = converted("![alt](pic.png)\n", MarkdownOptions::default()).unwrap_err();
    assert!(matches!(
        failure,
        MarkdownError::Image { target, error } if target == "pic.png" && !error.is_empty()
    ));
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

#[test]
fn the_list_and_table_sample_converts_to_numbered_paragraphs_and_a_table() {
    let docx = reopened(&converted(LIST_SAMPLE, MarkdownOptions::default()).unwrap());
    assert_eq!(
        outline(&docx),
        [
            "- num=2/0: one",
            "- num=1/1: inner",
            "- num=2/0: two",
            "tbl TableGrid auto 2",
            "tr header: - jc=left: Region || - jc=right: Revenue",
            "tr: - jc=left: North || - jc=right: 1,200",
        ]
    );
    assert_eq!(numbering_instances(&docx), ["num 1 -> 0", "num 2 -> 1 0=1"]);
    let numbering = PartName::new("/word/numbering.xml").unwrap();
    let related = docx
        .package()
        .related_by_type(
            Some(docx.main_document_part()),
            &numbering_definitions::RELATIONSHIP_TYPE,
        )
        .unwrap();
    let targets: Vec<&Target> = related.iter().map(|part| &part.target).collect();
    assert_eq!(targets, [&Target::Part(numbering.clone())]);
    assert_eq!(
        docx.package().content_type(&numbering),
        Some(numbering_definitions::CONTENT_TYPE)
    );
}

#[test]
fn two_ordered_lists_get_an_instance_each_and_two_bullet_lists_share_one() {
    let ordered = converted("1. a\n\ntext\n\n3. b\n", MarkdownOptions::default()).unwrap();
    assert_eq!(
        outline(&ordered),
        ["- num=2/0: a", "-: text", "- num=3/0: b"]
    );
    assert_eq!(
        numbering_instances(&ordered),
        ["num 1 -> 0", "num 2 -> 1 0=1", "num 3 -> 1 0=3"]
    );
    let bullets = converted("- a\n\ntext\n\n- b\n", MarkdownOptions::default()).unwrap();
    assert_eq!(
        outline(&bullets),
        ["- num=1/0: a", "-: text", "- num=1/0: b"]
    );
    assert_eq!(numbering_instances(&bullets), ["num 1 -> 0"]);
}

#[test]
fn a_list_nested_ten_deep_takes_the_ninth_level_for_its_two_deepest() {
    let innermost = MarkdownBlock::new(alloc::vec![Section::new(
        None,
        alloc::vec![plain("l10")],
        Vec::new()
    )]);
    let mut list: List = List::new(alloc::vec![Item::new(innermost)], false, None);
    for depth in (1..10_usize).rev() {
        let body = MarkdownBlock::new(alloc::vec![Section::new(
            None,
            alloc::vec![plain(&format!("l{depth}")), Block::List(list)],
            Vec::new()
        )]);
        list = List::new(alloc::vec![Item::new(body)], false, None);
    }
    let lines = outline(&from_arky(&document_of(alloc::vec![Block::List(list)])));
    let mut expected: Vec<String> = (0..9_usize)
        .map(|level| format!("- num=1/{level}: l{}", level + 1))
        .collect();
    expected.push("- num=1/8: l10".to_owned());
    assert_eq!(lines, expected);
}

#[test]
fn the_later_blocks_of_an_item_are_list_paragraphs_indented_to_its_level() {
    let docx = converted(
        "- first\n\n  second\n\n  - inner\n\n    third\n",
        MarkdownOptions::default(),
    )
    .unwrap();
    assert_eq!(
        outline(&docx),
        [
            "- num=1/0: first",
            "ListParagraph left=720: second",
            "- num=1/1: inner",
            "ListParagraph left=1440: third",
        ]
    );
}

#[test]
fn a_later_paragraph_of_an_item_is_indented_to_where_its_levels_numbering_puts_the_text() {
    let docx = reopened(
        &converted(
            "1. one\n\n   1. inner\n\n      under inner\n\n   under one\n\n2. two\n",
            MarkdownOptions::default(),
        )
        .unwrap(),
    );
    assert_eq!(
        outline(&docx),
        [
            "- num=2/0: one",
            "- num=3/1: inner",
            "ListParagraph left=1440: under inner",
            "ListParagraph left=720: under one",
            "- num=2/0: two",
        ]
    );
    let levels = numbering_lines(&docx);
    for level in [
        "abstract 1 0: decimal %1. start=1 left=720 hanging=360",
        "abstract 1 1: decimal %2. start=1 left=1440 hanging=360",
    ] {
        assert!(levels.iter().any(|line| line == level), "{levels:?}");
    }
}

#[test]
fn an_item_that_opens_with_another_block_gets_an_empty_numbered_paragraph() {
    let docx = converted("- ```\n  code\n  ```\n", MarkdownOptions::default()).unwrap();
    assert_eq!(
        outline(&docx),
        ["- num=1/0: ", "HTMLPreformatted left=720: code"]
    );
}

#[test]
fn a_nested_ordered_list_starts_at_its_first_number() {
    let docx = converted("1. a\n\n   3. b\n", MarkdownOptions::default()).unwrap();
    assert_eq!(outline(&docx), ["- num=2/0: a", "- num=3/1: b"]);
    assert_eq!(
        numbering_instances(&docx),
        ["num 1 -> 0", "num 2 -> 1 0=1", "num 3 -> 1 1=3"]
    );
}

#[test]
fn an_ordered_list_nested_ten_deep_overrides_the_ninth_level() {
    let innermost = MarkdownBlock::new(alloc::vec![Section::new(
        None,
        alloc::vec![plain("l10")],
        Vec::new()
    )]);
    let mut list: List = List::new(alloc::vec![Item::new(innermost)], true, Some(7));
    for depth in (1..10_usize).rev() {
        let body = MarkdownBlock::new(alloc::vec![Section::new(
            None,
            alloc::vec![plain(&format!("l{depth}")), Block::List(list)],
            Vec::new()
        )]);
        list = List::new(alloc::vec![Item::new(body)], false, None);
    }
    let docx = from_arky(&document_of(alloc::vec![Block::List(list)]));
    assert_eq!(numbering_instances(&docx), ["num 1 -> 0", "num 2 -> 1 8=7"]);
}

#[test]
fn a_top_level_ordered_list_starting_at_three_overrides_the_first_level() {
    let docx = converted("3. a\n", MarkdownOptions::default()).unwrap();
    assert_eq!(numbering_instances(&docx), ["num 1 -> 0", "num 2 -> 1 0=3"]);
}

#[test]
fn blocks_inside_a_list_item_keep_their_style_and_get_the_item_indent() {
    let docx = converted(
        "- item\n\n  ```\n  code\n  ```\n\n  > quoted\n\n  ***\n\n  plain\n",
        MarkdownOptions::default(),
    )
    .unwrap();
    assert_eq!(
        outline(&docx),
        [
            "- num=1/0: item",
            "HTMLPreformatted left=720: code",
            "Quote left=720: quoted",
            "ListParagraph left=720: ",
            "ListParagraph left=720: plain",
        ]
    );
    let content = docx.main_document().unwrap().body.unwrap().content;
    let bordered = content.iter().any(|block| {
        matches!(block, BlockContent::Paragraph(paragraph)
            if paragraph.content.is_empty()
                && paragraph
                    .properties
                    .as_ref()
                    .is_some_and(|properties| properties.paragraph_borders.is_some()))
    });
    assert!(bordered);
}

#[test]
fn a_heading_inside_a_list_item_is_a_bold_list_paragraph_with_no_bookmark() {
    let docx = converted("- item\n\n  ### Inner\n", MarkdownOptions::default()).unwrap();
    assert_eq!(
        outline(&docx),
        ["- num=1/0: item", "ListParagraph left=720: b:Inner"]
    );
    let content = docx.main_document().unwrap().body.unwrap().content;
    assert!(content.iter().all(|block| !matches!(block,
        BlockContent::Paragraph(paragraph)
            if paragraph.content.iter().any(|inner| matches!(inner, ParagraphContent::BookmarkStart(_))))));
}

#[test]
fn the_numbering_part_holds_nine_bullet_and_nine_decimal_levels() {
    let docx = reopened(&converted(LIST_SAMPLE, MarkdownOptions::default()).unwrap());
    let mut expected = Vec::new();
    for level in 0..9_usize {
        let bullet = ["\u{2022}", "\u{25e6}", "\u{25aa}"][level % 3];
        expected.push(format!(
            "abstract 0 {level}: bullet {bullet} start=1 left={} hanging=360",
            720 * (level + 1)
        ));
    }
    for level in 0..9_usize {
        expected.push(format!(
            "abstract 1 {level}: decimal %{}. start=1 left={} hanging=360",
            level + 1,
            720 * (level + 1)
        ));
    }
    expected.extend(["num 1 -> 0".to_owned(), "num 2 -> 1 0=1".to_owned()]);
    assert_eq!(numbering_lines(&docx), expected);
}

#[test]
fn a_document_with_no_list_has_no_numbering_part() {
    let docx = converted("# T\n", MarkdownOptions::default()).unwrap();
    let package = docx.package();
    assert!(!package.contains(&PartName::new("/word/numbering.xml").unwrap()));
    let related = package
        .related_by_type(
            Some(docx.main_document_part()),
            &numbering_definitions::RELATIONSHIP_TYPE,
        )
        .unwrap();
    assert_eq!(related.len(), 0);
}

#[test]
fn a_short_row_gets_empty_cells_holding_one_paragraph() {
    let table = ArkyTable::new(
        TableFormat::Pipe,
        alloc::vec![Alignment::Center, Alignment::Default],
        alloc::vec![ArkyTableRow::new(alloc::vec![cell_of("a"), cell_of("b")])],
        alloc::vec![ArkyTableRow::new(alloc::vec![cell_of("c")])],
    );
    let docx = from_arky(&document_of(alloc::vec![Block::Table(table)]));
    assert_eq!(
        outline(&docx),
        [
            "tbl TableGrid auto 2",
            "tr header: - jc=center: a || -: b",
            "tr: - jc=center: c || -: ",
        ]
    );
    let rows = table_rows(&docx);
    let short = rows.last().unwrap();
    assert_eq!(short.len(), 2);
    assert_eq!(
        short.last().unwrap().content,
        [BlockContent::Paragraph(Paragraph::default())]
    );
}

#[test]
fn the_converted_list_and_table_sample_validates() {
    let docx = converted(LIST_SAMPLE, MarkdownOptions::default()).unwrap();
    assert_eq!(validation_errors(&docx), Vec::<String>::new());
}

#[test]
fn the_link_sample_converts_to_bookmarks_hyperlinks_an_html_part_and_a_frontmatter_part() {
    let carried = MarkdownOptions {
        frontmatter: true,
        ..MarkdownOptions::default()
    };
    let docx = reopened(&converted(LINK_SAMPLE, carried).unwrap());
    assert_eq!(
        outline(&docx),
        [
            "Heading1: Q3 results",
            "-: See |<https://example.com/q3 \"Live\">Hyperlink:the dashboard</>| and |<#q3_results>Hyperlink:above</>|.",
            "altChunk htmlChunk1.html",
            "-: Water is H|2|O.",
        ]
    );
    assert_eq!(bookmarks(&docx), ["0 q3_results"]);
    assert_eq!(
        part(&docx, "/word/htmlChunk1.html"),
        (
            "<html><body><div align=\"center\">Signed</div></body></html>".to_owned(),
            "text/html".to_owned()
        )
    );
    assert_eq!(
        part(&docx, "/docProps/frontmatter.toml"),
        ("title = \"Q3\"\n".to_owned(), "application/toml".to_owned())
    );
    let related = docx.package().related(None).unwrap();
    let frontmatter: Vec<&Target> = related
        .iter()
        .filter(|part| part.type_uri == FRONTMATTER_RELATIONSHIP)
        .map(|part| &part.target)
        .collect();
    assert_eq!(
        frontmatter,
        [&Target::Part(
            PartName::new("/docProps/frontmatter.toml").unwrap()
        )]
    );
}

#[test]
fn an_external_link_is_an_external_hyperlink_relationship_to_its_target_as_written() {
    let docx = reopened(
        &converted(
            "[w](https://example.com/q3) [n](other.md) [m](mailto:sales@example.com)\n",
            MarkdownOptions::default(),
        )
        .unwrap(),
    );
    let hyperlink = RelationshipType::HYPERLINK.in_class(ConformanceClass::Transitional);
    let links: Vec<(String, TargetMode)> = main_relationships(&docx)
        .into_iter()
        .filter(|relationship| relationship.type_uri == hyperlink)
        .map(|relationship| (relationship.target, relationship.target_mode))
        .collect();
    assert_eq!(
        links,
        [
            ("https://example.com/q3".to_owned(), TargetMode::External),
            ("other.md".to_owned(), TargetMode::External),
            ("mailto:sales@example.com".to_owned(), TargetMode::External),
        ]
    );
    assert_eq!(
        outline(&docx),
        [
            "-: <https://example.com/q3>Hyperlink:w</>| |<other.md>Hyperlink:n</>| |<mailto:sales@example.com>Hyperlink:m</>"
        ]
    );
}

#[test]
fn a_link_holding_bold_writes_a_bold_run_inside_the_hyperlink() {
    let docx = converted("[a **b**](https://x.example)\n", MarkdownOptions::default()).unwrap();
    assert_eq!(
        outline(&docx),
        ["-: <https://x.example>Hyperlink:a |Hyperlink,b:b</>"]
    );
}

#[test]
fn a_link_written_across_lines_is_one_hyperlink_holding_the_space_between_them() {
    let docx = converted(
        "See [the quarterly\n**dash** board](https://example.com/q3) now.\n",
        MarkdownOptions::default(),
    )
    .unwrap();
    assert_eq!(
        outline(&docx),
        [
            "-: See |<https://example.com/q3>Hyperlink:the quarterly|Hyperlink: |Hyperlink,b:dash|Hyperlink: board</>| now."
        ]
    );
    assert_eq!(main_relationships(&docx).len(), 2);
}

#[test]
fn two_headings_of_one_name_get_numbered_bookmarks_and_a_long_one_is_cut_to_forty() {
    let repeated = converted(
        "# Q3 results\n\n# Q3 results\n\n# Q3 results\n",
        MarkdownOptions::default(),
    )
    .unwrap();
    assert_eq!(
        bookmarks(&repeated),
        ["0 q3_results", "1 q3_results_1", "2 q3_results_2"]
    );
    let long = "a".repeat(60);
    let cut = converted(
        &format!("# {long}\n\n# {long}\n"),
        MarkdownOptions::default(),
    )
    .unwrap();
    assert_eq!(
        bookmarks(&cut),
        [
            format!("0 {}", "a".repeat(40)),
            format!("1 {}_1", "a".repeat(38))
        ]
    );
}

#[test]
fn html_inside_a_sentence_is_dropped_and_the_text_around_it_kept() {
    let docx = converted("Water is H<sub>2</sub>O.\n", MarkdownOptions::default()).unwrap();
    assert_eq!(outline(&docx), ["-: Water is H|2|O."]);
}

#[test]
fn each_html_block_gets_a_part_and_an_alt_chunk_of_its_own() {
    let docx = reopened(
        &converted(
            "<div>a</div>\n\ntext\n\n<p>b</p>\n",
            MarkdownOptions::default(),
        )
        .unwrap(),
    );
    assert_eq!(
        outline(&docx),
        [
            "altChunk htmlChunk1.html",
            "-: text",
            "altChunk htmlChunk2.html"
        ]
    );
    assert_eq!(
        part(&docx, "/word/htmlChunk1.html"),
        (
            "<html><body><div>a</div></body></html>".to_owned(),
            "text/html".to_owned()
        )
    );
    assert_eq!(
        part(&docx, "/word/htmlChunk2.html"),
        (
            "<html><body><p>b</p></body></html>".to_owned(),
            "text/html".to_owned()
        )
    );
    let chunk = RelationshipType::A_F_CHUNK.in_class(ConformanceClass::Transitional);
    let types: Vec<String> = main_relationships(&docx)
        .into_iter()
        .filter(|relationship| relationship.target.starts_with("htmlChunk"))
        .map(|relationship| relationship.type_uri)
        .collect();
    assert_eq!(types, [chunk, chunk]);
}

#[test]
fn the_frontmatter_part_is_written_only_when_asked_and_a_null_fails_the_conversion() {
    let docx = converted(LINK_SAMPLE, MarkdownOptions::default()).unwrap();
    let package = docx.package();
    assert!(!package.contains(&PartName::new("/docProps/frontmatter.toml").unwrap()));
    assert!(
        package
            .related(None)
            .unwrap()
            .iter()
            .all(|part| part.type_uri != FRONTMATTER_RELATIONSHIP)
    );
    let carried = MarkdownOptions {
        frontmatter: true,
        ..MarkdownOptions::default()
    };
    let error = converted("---\nkey: null\n---\n# T\n", carried).unwrap_err();
    assert!(matches!(
        error,
        MarkdownError::Write(written) if written.message == "TOML cannot hold a null"
    ));
}

#[test]
fn the_converted_link_sample_validates() {
    let carried = MarkdownOptions {
        frontmatter: true,
        ..MarkdownOptions::default()
    };
    let docx = converted(LINK_SAMPLE, carried).unwrap();
    assert_eq!(validation_errors(&docx), Vec::<String>::new());
}

#[test]
fn the_picture_sample_stores_its_parts_links_the_web_picture_and_sizes_each_frame() {
    let logo = png_of_size(300, 100);
    let team = webp_of(&team_pixels(), 4, 3);
    let system = picture_notes(
        PICTURE_SAMPLE,
        &[
            ("logo.png", &logo),
            ("diagram.svg", DIAGRAM.as_bytes()),
            ("team.webp", &team),
        ],
    );
    let docx = picture_docx(&system);
    assert_eq!(
        pictures(&docx),
        [
            r#"1 "Picture 1" "Logo" "-" 2857500x952500 embed=media/image1.png"#,
            r#"2 "Picture 2" "Diagram" "-" 1143000x762000 embed=media/image2.png svg=media/image3.svg"#,
            r#"3 "Picture 3" "Team" "-" 38100x28575 embed=media/image4.png"#,
            r#"4 "Picture 4" "Web" "-" 5943600x5943600 link=https://example.com/a.png"#,
        ]
    );
    let package = docx.package();
    let mut media: Vec<(String, String)> = package
        .part_names()
        .unwrap()
        .iter()
        .filter(|name| name.as_str().starts_with("/word/media/"))
        .map(|name| {
            (
                name.as_str().to_owned(),
                package.content_type(name).unwrap_or_default().to_owned(),
            )
        })
        .collect();
    media.sort();
    assert_eq!(
        media,
        [
            ("/word/media/image1.png".to_owned(), "image/png".to_owned()),
            ("/word/media/image2.png".to_owned(), "image/png".to_owned()),
            (
                "/word/media/image3.svg".to_owned(),
                "image/svg+xml".to_owned()
            ),
            ("/word/media/image4.png".to_owned(), "image/png".to_owned()),
        ]
    );
    let bytes = |path: &str| package.part_bytes(&PartName::new(path).unwrap()).unwrap();
    assert_eq!(bytes("/word/media/image1.png"), logo);
    assert_eq!(bytes("/word/media/image3.svg"), DIAGRAM.as_bytes());
    let (width, height, _, _) = png_samples(&bytes("/word/media/image2.png"));
    assert_eq!((width, height), (120, 80));
    assert_eq!(
        png_samples(&bytes("/word/media/image4.png")),
        (4, 3, ColorType::Rgba, team_pixels())
    );
    let image = RelationshipType::IMAGE.in_class(ConformanceClass::Transitional);
    let images: Vec<(String, TargetMode)> = main_relationships(&docx)
        .into_iter()
        .filter(|relationship| relationship.type_uri == image)
        .map(|relationship| (relationship.target, relationship.target_mode))
        .collect();
    assert_eq!(
        images,
        [
            ("media/image1.png".to_owned(), TargetMode::Internal),
            ("media/image2.png".to_owned(), TargetMode::Internal),
            ("media/image3.svg".to_owned(), TargetMode::Internal),
            ("media/image4.png".to_owned(), TargetMode::Internal),
            ("https://example.com/a.png".to_owned(), TargetMode::External),
        ]
    );
}

#[test]
fn a_picture_wider_than_the_text_takes_its_width_and_a_height_scaled_by_the_same_factor() {
    let wide = png_of_size(1000, 500);
    let system = picture_notes("![Wide](wide.png)\n", &[("wide.png", &wide)]);
    assert_eq!(
        pictures(&picture_docx(&system)),
        [r#"1 "Picture 1" "Wide" "-" 5943600x2971800 embed=media/image1.png"#]
    );
}

#[test]
fn each_picture_is_numbered_by_its_place_among_the_documents_pictures_and_described_by_its_alt() {
    let dot = png_of_size(1, 1);
    let source = "![first](dot.png \"The dot\")\n\n# Next\n\n[![linked](dot.png)](https://example.com/page) ![](https://example.com/b.png)\n";
    let docx = picture_docx(&picture_notes(source, &[("dot.png", &dot)]));
    assert_eq!(
        pictures(&docx),
        [
            r#"1 "Picture 1" "first" "The dot" 9525x9525 embed=media/image1.png"#,
            r#"2 "Picture 2" "linked" "-" 9525x9525 embed=media/image1.png"#,
            r#"3 "Picture 3" "" "-" 5943600x5943600 link=https://example.com/b.png"#,
        ]
    );
    let content = docx.main_document().unwrap().body.unwrap().content;
    let in_link = content.iter().any(|block| {
        matches!(block, BlockContent::Paragraph(paragraph) if paragraph.content.iter().any(|item| {
            matches!(item, ParagraphContent::Hyperlink(hyperlink) if hyperlink.content.iter().any(|inner| {
                matches!(inner, ParagraphContent::TextRun(run) if run.content.iter().any(|piece| {
                    matches!(piece, TextRunContent::DrawingMLObject(_))
                }))
            }))
        }))
    });
    assert!(in_link);
}

#[test]
fn two_pictures_of_one_file_share_one_image_part() {
    let dot = png_of_size(1, 1);
    let system = picture_notes(
        "![a](dot.png)\n\n![b](dot.png)\n\n![c](other.png)\n",
        &[("dot.png", &dot), ("other.png", &dot)],
    );
    let docx = picture_docx(&system);
    assert_eq!(
        pictures(&docx),
        [
            r#"1 "Picture 1" "a" "-" 9525x9525 embed=media/image1.png"#,
            r#"2 "Picture 2" "b" "-" 9525x9525 embed=media/image1.png"#,
            r#"3 "Picture 3" "c" "-" 9525x9525 embed=media/image2.png"#,
        ]
    );
}

#[test]
fn two_pictures_of_one_svg_share_one_png_part_and_one_svg_part() {
    let system = picture_notes(
        "![a](diagram.svg)\n\n![b](diagram.svg)\n",
        &[("diagram.svg", DIAGRAM.as_bytes())],
    );
    let docx = picture_docx(&system);
    assert_eq!(
        pictures(&docx),
        [
            r#"1 "Picture 1" "a" "-" 1143000x762000 embed=media/image1.png svg=media/image2.svg"#,
            r#"2 "Picture 2" "b" "-" 1143000x762000 embed=media/image1.png svg=media/image2.svg"#,
        ]
    );
}

#[test]
fn a_picture_whose_alt_text_spans_two_lines_is_one_picture() {
    let dot = png_of_size(1, 1);
    let docx = picture_docx(&picture_notes(
        "![first\nsecond](dot.png)\n",
        &[("dot.png", &dot)],
    ));
    assert_eq!(
        pictures(&docx),
        [r#"1 "Picture 1" "first second" "-" 9525x9525 embed=media/image1.png"#]
    );
    let images = docx
        .package()
        .part_names()
        .unwrap()
        .iter()
        .filter(|name| name.as_str().starts_with("/word/media/"))
        .count();
    assert_eq!(images, 1);
}

#[test]
fn a_picture_whose_alt_text_spans_three_lines_is_one_picture() {
    let dot = png_of_size(1, 1);
    let docx = picture_docx(&picture_notes(
        "![a\nb\nc](dot.png)\n",
        &[("dot.png", &dot)],
    ));
    assert_eq!(
        pictures(&docx),
        [r#"1 "Picture 1" "a b c" "-" 9525x9525 embed=media/image1.png"#]
    );
}

#[test]
fn a_picture_inside_a_link_whose_alt_text_spans_two_lines_is_one_picture() {
    let dot = png_of_size(1, 1);
    let source = "[![a\nb](dot.png)](https://example.com/page)\n";
    let docx = picture_docx(&picture_notes(source, &[("dot.png", &dot)]));
    assert_eq!(
        pictures(&docx),
        [r#"1 "Picture 1" "a b" "-" 9525x9525 embed=media/image1.png"#]
    );
}

#[test]
fn a_converted_document_with_each_kind_of_picture_validates() {
    let logo = png_of_size(300, 100);
    let team = webp_of(&team_pixels(), 4, 3);
    let source = format!(
        "{PICTURE_SAMPLE}\n![d](data:image/png;base64,{}) ![s]({DIAGRAM_ADDRESS})\n\n[![l](logo.png)](https://example.com)\n",
        STANDARD.encode(&logo)
    );
    let system = picture_notes(
        &source,
        &[
            ("logo.png", &logo),
            ("diagram.svg", DIAGRAM.as_bytes()),
            ("team.webp", &team),
        ],
    );
    let docx = from_markdown(&system, Path::new("/n/p.md"), MarkdownOptions::default()).unwrap();
    assert_eq!(pictures(&docx).len(), 7);
    assert_eq!(validation_errors(&docx), Vec::<String>::new());
}

#[test]
fn a_link_with_no_text_shows_its_target() {
    let docx = converted(
        "[[Other page]] and [](https://example.com)\n",
        MarkdownOptions::default(),
    )
    .unwrap();
    assert_eq!(
        outline(&docx),
        [
            "-: <Other page>Hyperlink:Other page</>| and |<https://example.com>Hyperlink:https://example.com</>"
        ]
    );
}

#[test]
fn a_bookmark_name_starts_with_a_letter_or_an_underscore() {
    let docx = converted(
        "# 2024 plan\n\n# !!!\n\n# ???\n\n[x](#2024-plan)\n",
        MarkdownOptions::default(),
    )
    .unwrap();
    assert_eq!(
        bookmarks(&docx),
        ["0 _2024_plan", "1 _heading", "2 _heading_1"]
    );
    assert!(outline(&docx).contains(&"-: <#_2024_plan>Hyperlink:x</>".to_owned()));
}

#[test]
fn markdown_text_with_no_folder_fails_on_a_relative_picture_naming_it() {
    let error = from_markdown_text(
        &MemorySystem::new(),
        "![Logo](logo.png)\n",
        None,
        MarkdownOptions::default(),
    )
    .unwrap_err();
    assert!(
        matches!(&error, MarkdownError::Image { target, error: cause }
            if target == "logo.png" && !cause.is_empty()),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "logo.png: a relative path, and markdown from standard input has no folder to find it from"
    );
}

#[test]
fn markdown_text_with_no_folder_stores_a_data_and_an_absolute_picture_and_links_a_web_one() {
    let dot = png_of_size(1, 1);
    let system = picture_notes("", &[("dot.png", &dot)]);
    let source = format!(
        "![Data](data:image/png;base64,{}) ![Web](https://example.com/a.png) ![Dot](/n/dot.png)\n",
        STANDARD.encode(&dot)
    );
    let docx =
        reopened(&from_markdown_text(&system, &source, None, MarkdownOptions::default()).unwrap());
    assert_eq!(
        pictures(&docx),
        [
            r#"1 "Picture 1" "Data" "-" 9525x9525 embed=media/image1.png"#,
            r#"2 "Picture 2" "Web" "-" 5943600x5943600 link=https://example.com/a.png"#,
            r#"3 "Picture 3" "Dot" "-" 9525x9525 embed=media/image2.png"#,
        ]
    );
}
