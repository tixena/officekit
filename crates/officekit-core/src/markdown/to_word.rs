//! arky's blocks and inline nodes as Word paragraphs, runs, lists, tables, hyperlinks and
//! pictures.

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::{iter, mem};
use std::path::Path;

use arky::parser::ast::{
    Alignment, Block, Continues, Inline, Item, Line, Link, LinkInline, List, MarkdownBlock,
    Paragraph as ArkyParagraph, Section, Table as ArkyTable, TableRow as ArkyTableRow,
};
use arky::parser::extension::Extension;
use officekit_markup::Read as _;
use officekit_markup::wordprocessing::{
    AnchorForImportedExternalContent, BlockContent, Bold, BookmarkEnd, BookmarkStart,
    BorderStylesMember, Break, ContentCellContent, ContentRowContent, GridColumnDefinition,
    HorizontalAlignmentTypeMember, Hyperlink, Italics, NumberingDefinitionInstanceReferenceNumId,
    NumberingDefinitionInstanceReferenceNumPr, NumberingLevelReference, Paragraph,
    ParagraphAlignment, ParagraphBorderBelowIdenticalParagraphs, ParagraphBorders,
    ParagraphContent, ParagraphIndentation, ParagraphProperties, PreferredTableWidth,
    ReferencedCharacterStyle, ReferencedParagraphStyle, ReferencedTableStyle, RepeatTableRow,
    RunProperties, RunPropertiesContent, TabCharacter, Table, TableCell, TableGrid,
    TableProperties, TableRow, TableRowProperties, TableRowPropertiesContent,
    TableWidthUnitsMember, Text, TextRun, TextRunContent,
};
use officekit_packaging::error::PackageError;
use officekit_packaging::image::Image;
use officekit_packaging::package::Package;
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::RelationshipType;
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Attribute, Element, NamespaceDeclaration, Node};
use officekit_xml::name::QName;
use os_shim::System;

use super::MarkdownError;
use super::parts::{
    self, CODE, DEEPEST_HEADING, DEEPEST_LEVEL, HYPERLINK, LIST_PARAGRAPH, PREFORMATTED, QUOTE,
    TABLE_GRID,
};
use super::picture::{self, Picture};

/// The longest bookmark name Word takes.
const BOOKMARK_LENGTH: usize = 40;

const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

/// The EMU in a pixel at 96 pixels an inch.
const EMU_PER_PIXEL: u64 = 9_525;

const HTML_CONTENT_TYPE: &str = "text/html";

/// The names the part of an HTML block takes, from `/word/htmlChunk1.html`.
const HTML_PART: &str = "/word/htmlChunk{}.html";

const PICTURE: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";

const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

const SVG: &str = "http://schemas.microsoft.com/office/drawing/2016/SVG/main";

/// The `a:ext` URI of the extension that names a picture's SVG.
const SVG_EXTENSION: &str = "{96DAC541-7B7A-43D3-8B79-37D633B846F1}";

/// The text width of a page, 6.5 inches, in EMU: the widest frame a picture gets, and the side of
/// the square frame of a picture whose size is not known.
const TEXT_WIDTH: u64 = 5_943_600;

const WORDPROCESSING: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

const WORDPROCESSING_DRAWING: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";

/// The Word blocks written so far, and what the walk keeps from one block to the next.
struct Body<'package, S>
where
    S: ?Sized,
{
    /// The folder a relative picture path is found from.
    base: Option<&'package Path>,
    blocks: Vec<BlockContent>,
    bookmarks: Bookmarks,
    /// The depth of the list item whose later blocks are being written.
    item: Option<usize>,
    main: &'package PartName,
    /// The level and the numbering instance of the next paragraph, the first of a list item.
    number: Option<(usize, usize)>,
    /// The first number and the level of each ordered list so far; `None` until the walk meets a list.
    numbering: Option<Vec<(u64, usize)>>,
    package: &'package mut Package,
    pictures: Pictures,
    system: &'package S,
}

impl<S> Body<'_, S>
where
    S: System + ?Sized,
{
    fn block<E>(&mut self, block: &Block<E>) -> Result<(), MarkdownError>
    where
        E: Extension,
    {
        match block {
            Block::BlockQuote(quote) => self.lines_styled(QUOTE, &quote.value),
            Block::CodeBlock(code) => self.lines_styled(PREFORMATTED, &code.value),
            Block::Html(html) => self.html(&html.value)?,
            Block::List(list) => self.list(list)?,
            Block::Paragraph(paragraph) => self.paragraph(paragraph)?,
            Block::Table(table) => self.table(table)?,
            Block::ThematicBreak(_) => self.thematic_break(),
            _ => {}
        }
        Ok(())
    }

    /// A table cell of `body`, its paragraphs aligned by `alignment`; a cell with no paragraph
    /// gets an empty one.
    fn cell<E>(
        &mut self,
        body: &MarkdownBlock<E>,
        alignment: Option<Alignment>,
    ) -> Result<TableCell, MarkdownError>
    where
        E: Extension,
    {
        let outer = mem::take(&mut self.blocks);
        let item = self.item.take();
        self.markdown_block(body)?;
        self.item = item;
        let mut content = mem::replace(&mut self.blocks, outer);
        if !content
            .iter()
            .any(|block| matches!(block, BlockContent::Paragraph(_)))
        {
            content.push(BlockContent::Paragraph(Paragraph::default()));
        }
        if let Some(jc) = alignment.and_then(justification) {
            for block in &mut content {
                if let BlockContent::Paragraph(paragraph) = block {
                    paragraph
                        .properties
                        .get_or_insert_with(Box::default)
                        .paragraph_alignment = Some(Box::new(jc.clone()));
                }
            }
        }
        let mut cell = TableCell::default();
        cell.content = content;
        Ok(cell)
    }

    /// A `w:altChunk` naming a new part that holds `html` in a page.
    fn html(&mut self, html: &str) -> Result<(), PackageError> {
        let page = format!("<html><body>{html}</body></html>");
        let (_, id) = self.package.add_part(
            Some(self.main),
            RelationshipType::A_F_CHUNK.in_class(ConformanceClass::Transitional),
            HTML_CONTENT_TYPE,
            HTML_PART,
            page.as_bytes(),
        )?;
        let mut chunk = AnchorForImportedExternalContent::default();
        chunk.id = Some(id);
        self.blocks
            .push(BlockContent::AnchorForImportedExternalContent(chunk));
        Ok(())
    }

    /// A list item at `depth`, numbered by `instance`: its first paragraph numbered, or an empty
    /// numbered paragraph when it starts with another block, then its later blocks.
    fn item<E>(
        &mut self,
        item: &Item<E>,
        depth: usize,
        instance: usize,
    ) -> Result<(), MarkdownError>
    where
        E: Extension,
    {
        self.number = Some((depth.min(DEEPEST_LEVEL), instance));
        let opening = item
            .body
            .sections
            .first()
            .filter(|section| section.heading.is_none())
            .and_then(|section| section.blocks.first());
        if !matches!(opening, Some(Block::Paragraph(_))) {
            self.push(None, Vec::new());
        }
        let outer = self.item.replace(depth);
        self.markdown_block(&item.body)?;
        self.item = outer;
        Ok(())
    }

    /// One paragraph styled `style` per line of `text`; an empty line is an empty paragraph.
    fn lines_styled(&mut self, style: &str, text: &str) {
        for line in text.split('\n') {
            let mut runs = self.runs();
            runs.text(line.strip_suffix('\r').unwrap_or(line), Format::default());
            let content = runs.into_content();
            self.push(Some(style), content);
        }
    }

    /// A list one level deeper than the item it is in: numbering instance
    /// [`parts::BULLET_INSTANCE`] for bullets, an instance of its own when ordered.
    fn list<E>(&mut self, list: &List<E>) -> Result<(), MarkdownError>
    where
        E: Extension,
    {
        let depth = self.item.map_or(0, |outer| outer + 1);
        let ordered = self.numbering.get_or_insert_with(Vec::new);
        let instance = list.start.map_or(parts::BULLET_INSTANCE, |first| {
            ordered.push((first, depth.min(DEEPEST_LEVEL)));
            parts::ordered_instance(ordered.len() - 1)
        });
        for item in &list.items {
            self.item(item, depth, instance)?;
        }
        Ok(())
    }

    /// The sections of `block`, from depth 1.
    fn markdown_block<E>(&mut self, block: &MarkdownBlock<E>) -> Result<(), MarkdownError>
    where
        E: Extension,
    {
        for section in &block.sections {
            self.section(section, 1)?;
        }
        Ok(())
    }

    /// One paragraph of `paragraph`'s lines, each after the one before and a space, or no space
    /// when the one before ends in a hard line break.
    fn paragraph<E>(&mut self, paragraph: &ArkyParagraph<E>) -> Result<(), MarkdownError>
    where
        E: Extension,
    {
        let mut runs = self.runs();
        let mut previous: Option<&Line<E>> = None;
        for line in &paragraph.lines {
            if previous.is_some_and(|before| !ends_in_break(&before.children)) {
                runs.text(" ", Format::default());
            }
            runs.inlines(&line.children, Format::default())?;
            previous = Some(line);
        }
        let content = runs.into_content();
        self.push(None, content);
        Ok(())
    }

    /// The properties of the next paragraph: its numbering when it opens a list item,
    /// the item's indent and `style` (`ListParagraph` for none) when it continues one, `style`
    /// otherwise.
    fn properties(&mut self, style: Option<&str>) -> Option<Box<ParagraphProperties>> {
        let mut properties = ParagraphProperties::default();
        if let Some((level, instance)) = self.number.take() {
            properties.numbering_definition_instance_reference_num_pr =
                Some(Box::new(numbering_reference(level, instance)));
        } else if let Some(depth) = self.item {
            let mut indentation = ParagraphIndentation::default();
            indentation.left = Some(parts::level_indent(depth.min(DEEPEST_LEVEL)).into());
            properties.paragraph_indentation = Some(Box::new(indentation));
            properties.referenced_paragraph_style =
                Some(Box::new(paragraph_style(style.unwrap_or(LIST_PARAGRAPH))));
        } else {
            properties.referenced_paragraph_style = Some(Box::new(paragraph_style(style?)));
        }
        Some(Box::new(properties))
    }

    /// A paragraph of `content`, styled `style`.
    fn push(&mut self, style: Option<&str>, content: Vec<ParagraphContent>) {
        let mut paragraph = Paragraph::default();
        paragraph.content = content;
        paragraph.properties = self.properties(style);
        self.blocks.push(BlockContent::Paragraph(paragraph));
    }

    /// An empty writer of runs, relating links and pictures from the main part.
    const fn runs(&mut self) -> Runs<'_, S> {
        Runs {
            base: self.base,
            content: Vec::new(),
            image: None,
            link: None,
            main: self.main,
            package: &mut *self.package,
            pictures: &mut self.pictures,
            system: self.system,
        }
    }

    /// The section's heading at `depth`, between the start and the end of its bookmark, then its
    /// blocks, then its subsections one level deeper.
    fn section<E>(&mut self, section: &Section<E>, depth: usize) -> Result<(), MarkdownError>
    where
        E: Extension,
    {
        if let Some(heading) = &section.heading {
            if self.item.is_some() {
                let bold = Format {
                    bold: true,
                    ..Format::default()
                };
                let mut runs = self.runs();
                runs.inlines(&heading.children, bold)?;
                let content = runs.into_content();
                self.push(None, content);
            } else {
                let (id, name) = self.bookmarks.add(&heading_text(&heading.children));
                let mut start = BookmarkStart::default();
                start.id = Some(id.to_string().into());
                start.name = Some(name);
                let mut end = BookmarkEnd::default();
                end.id = Some(id.to_string().into());
                let mut runs = self.runs();
                runs.content.push(ParagraphContent::BookmarkStart(start));
                runs.inlines(&heading.children, Format::default())?;
                let mut content = runs.into_content();
                content.push(ParagraphContent::BookmarkEnd(end));
                let style = parts::heading(depth.min(DEEPEST_HEADING));
                self.push(Some(&style), content);
            }
        }
        for block in &section.blocks {
            self.block(block)?;
        }
        for subsection in &section.subsections {
            self.section(subsection, depth + 1)?;
        }
        Ok(())
    }

    /// A table styled `TableGrid` of automatic width, as many columns wide as its widest row: its
    /// head rows, then its other rows.
    fn table<E>(&mut self, table: &ArkyTable<E>) -> Result<(), MarkdownError>
    where
        E: Extension,
    {
        let columns = table
            .head
            .iter()
            .chain(&table.rows)
            .map(|row| row.cells.len())
            .max()
            .unwrap_or(0);
        let mut rows = Vec::new();
        for row in &table.head {
            let written = self.table_row(row, true, columns, &table.alignments)?;
            rows.push(ContentRowContent::TableRow(written));
        }
        for row in &table.rows {
            let written = self.table_row(row, false, columns, &table.alignments)?;
            rows.push(ContentRowContent::TableRow(written));
        }
        let mut style = ReferencedTableStyle::default();
        style.val = Some(TABLE_GRID.to_owned());
        let mut width = PreferredTableWidth::default();
        width.r#type = Some(TableWidthUnitsMember::Auto.into());
        let mut properties = TableProperties::default();
        properties.preferred_table_width = Some(Box::new(width));
        properties.referenced_table_style = Some(Box::new(style));
        let mut grid = TableGrid::default();
        grid.column_definition = iter::repeat_with(GridColumnDefinition::default)
            .take(columns)
            .collect();
        let mut written = Table::default();
        written.content_row_content = rows;
        written.grid = Box::new(grid);
        written.properties = Box::new(properties);
        self.blocks.push(BlockContent::Table(written));
        Ok(())
    }

    /// A row of `row`'s cells, `columns` wide: each cell it lacks is an empty one. A head row is
    /// marked `w:tblHeader`.
    fn table_row<E>(
        &mut self,
        row: &ArkyTableRow<E>,
        head: bool,
        columns: usize,
        alignments: &[Alignment],
    ) -> Result<TableRow, MarkdownError>
    where
        E: Extension,
    {
        let mut written = TableRow::default();
        if head {
            let mut properties = TableRowProperties::default();
            properties.content = alloc::vec![TableRowPropertiesContent::RepeatTableRow(
                RepeatTableRow::default()
            )];
            written.properties = Some(Box::new(properties));
        }
        for column in 0..columns {
            let cell = if let Some(cell) = row.cells.get(column) {
                self.cell(&cell.body, alignments.get(column).copied())?
            } else {
                let mut empty = TableCell::default();
                empty.content = alloc::vec![BlockContent::Paragraph(Paragraph::default())];
                empty
            };
            written.content.push(ContentCellContent::TableCell(cell));
        }
        Ok(written)
    }

    /// An empty paragraph with a single bottom border.
    fn thematic_break(&mut self) {
        let mut bottom = ParagraphBorderBelowIdenticalParagraphs::default();
        bottom.color = Some("auto".to_owned().into());
        bottom.space = Some("1".to_owned().into());
        bottom.sz = Some("6".to_owned().into());
        bottom.val = Some(BorderStylesMember::Single.into());
        let mut borders = ParagraphBorders::default();
        borders.paragraph_border_below_identical_paragraphs = Some(Box::new(bottom));
        let mut properties = self.properties(None).unwrap_or_default();
        properties.paragraph_borders = Some(Box::new(borders));
        let mut paragraph = Paragraph::default();
        paragraph.properties = Some(properties);
        self.blocks.push(BlockContent::Paragraph(paragraph));
    }
}

/// The names of the heading bookmarks written so far.
#[derive(Default)]
struct Bookmarks {
    names: BTreeSet<String>,
}

impl Bookmarks {
    /// The id and the name of the bookmark of a heading reading `text`. The name is
    /// [`bookmark_name`] of its anchor, with `_1` added for the second heading of that name and
    /// `_2` for the third, cut so that it stays within [`BOOKMARK_LENGTH`]. Ids count from 0.
    fn add(&mut self, text: &str) -> (usize, String) {
        let id = self.names.len();
        let base = bookmark_name(&anchor(text));
        let mut name = base.clone();
        let mut repeat = 0_usize;
        while self.names.contains(&name) {
            repeat += 1;
            let suffix = format!("_{repeat}");
            let kept: String = base.chars().take(BOOKMARK_LENGTH - suffix.len()).collect();
            name = format!("{kept}{suffix}");
        }
        self.names.insert(name.clone());
        (id, name)
    }
}

/// The formatting the inline nodes around a run give it.
#[derive(Clone, Copy, Default)]
struct Format {
    bold: bool,
    /// `HTMLCode` for code. A run inside a link with no character style takes `Hyperlink`;
    /// code inside a link keeps `HTMLCode`, since a run has one character style.
    character_style: Option<&'static str>,
    italic: bool,
}

impl Format {
    /// The run's properties, `None` when it has no formatting.
    fn properties(self) -> Option<Box<RunProperties>> {
        let mut content = Vec::new();
        if let Some(id) = self.character_style {
            let mut reference = ReferencedCharacterStyle::default();
            reference.val = Some(id.to_owned());
            content.push(RunPropertiesContent::ReferencedCharacterStyle(reference));
        }
        if self.bold {
            content.push(RunPropertiesContent::Bold(Bold::default()));
        }
        if self.italic {
            content.push(RunPropertiesContent::Italics(Italics::default()));
        }
        (!content.is_empty()).then(|| {
            let mut properties = RunProperties::default();
            properties.content = content;
            Box::new(properties)
        })
    }
}

/// What the walk counts of the pictures it writes.
#[derive(Default)]
struct Pictures {
    /// The pictures written so far: the next one's `wp:docPr` id is one more.
    count: usize,
    /// The image parts stored so far: the next one is `media/image{n}`, `n` one more, whatever its
    /// extension.
    parts: usize,
    /// Each stored picture's target, as written, with the ids of its image part and of its SVG
    /// part and its frame size, so a later picture of the same target reuses them.
    stored: BTreeMap<String, Stored>,
}

/// The parts one stored picture took: the ids `a:blip r:embed` and `asvg:svgBlip r:embed` name,
/// and the frame size.
#[derive(Clone)]
struct Stored {
    id: String,
    size: Option<(u32, u32)>,
    svg_id: Option<String>,
}

/// The runs of one paragraph, written so far, and the package its links and pictures are related
/// in.
struct Runs<'package, S>
where
    S: ?Sized,
{
    /// The folder a relative picture path is found from.
    base: Option<&'package Path>,
    content: Vec<ParagraphContent>,
    /// The alt text so far of an image that goes on into the next line.
    image: Option<String>,
    /// The hyperlink the runs go into while a link is written, kept open while the link goes on
    /// into the next line.
    link: Option<Hyperlink>,
    main: &'package PartName,
    package: &'package mut Package,
    pictures: &'package mut Pictures,
    system: &'package S,
}

impl<S> Runs<'_, S>
where
    S: System + ?Sized,
{
    /// An empty `w:hyperlink` for `link`, its title as the tooltip: to the bookmark its anchor
    /// names when its target starts with `#`, else to its target, as written, through an external
    /// relationship from the main part.
    fn hyperlink<E>(&mut self, link: &Link<E>) -> Result<Hyperlink, PackageError>
    where
        E: Extension,
    {
        let mut hyperlink = Hyperlink::default();
        hyperlink.tooltip.clone_from(&link.title);
        if let Some(target) = link.target.strip_prefix('#') {
            hyperlink.anchor = Some(bookmark_name(target));
        } else {
            hyperlink.id = Some(self.package.relate_external(
                Some(self.main),
                &link.target,
                RelationshipType::HYPERLINK.in_class(ConformanceClass::Transitional),
            )?);
        }
        Ok(hyperlink)
    }

    /// The picture of one piece of an image: an image cut across lines is written once, at its
    /// last piece, its alt text the pieces' joined by spaces.
    fn image(
        &mut self,
        target: &str,
        alt: &str,
        title: Option<&str>,
        continues: Continues,
        format: Format,
    ) -> Result<(), MarkdownError> {
        match continues {
            Continues::Next => self.image = Some(alt.to_owned()),
            Continues::Both => {
                let gathered = self.image.get_or_insert_with(String::new);
                gathered.push(' ');
                gathered.push_str(alt);
            }
            Continues::Previous => {
                let mut gathered = self.image.take().unwrap_or_default();
                gathered.push(' ');
                gathered.push_str(alt);
                self.picture(target, &gathered, title, format)?;
            }
            Continues::Neither | _ => self.picture(target, alt, title, format)?,
        }
        Ok(())
    }

    fn inline<E>(&mut self, inline: &Inline<E>, format: Format) -> Result<(), MarkdownError>
    where
        E: Extension,
    {
        match inline {
            Inline::Code(code) => self.text(
                &code.value,
                Format {
                    character_style: Some(CODE),
                    ..format
                },
            ),
            Inline::Emphasis(emphasis) => self.inlines(
                &emphasis.children,
                Format {
                    italic: true,
                    ..format
                },
            )?,
            Inline::Image(image) => {
                self.image(
                    &image.target,
                    &image.alt,
                    image.title.as_deref(),
                    image.continues,
                    format,
                )?;
            }
            Inline::LineBreak(_) => self.line_break(format),
            Inline::Link(link) => self.link(link, format)?,
            Inline::Strong(strong) => self.inlines(
                &strong.children,
                Format {
                    bold: true,
                    ..format
                },
            )?,
            Inline::Text(text) => self.text(&text.value, format),
            Inline::Html(_) | _ => {}
        }
        Ok(())
    }

    fn inlines<E>(&mut self, inlines: &[Inline<E>], format: Format) -> Result<(), MarkdownError>
    where
        E: Extension,
    {
        for inline in inlines {
            self.inline(inline, format)?;
        }
        Ok(())
    }

    /// The runs written, with the hyperlink of a link still open closed.
    fn into_content(mut self) -> Vec<ParagraphContent> {
        self.content
            .extend(self.link.take().map(ParagraphContent::Hyperlink));
        self.content
    }

    fn line_break(&mut self, format: Format) {
        self.run(alloc::vec![TextRunContent::Break(Break::default())], format);
    }

    /// A `w:hyperlink` around the link's runs. A link written across lines, which arky cuts into
    /// one piece per line, is one hyperlink: its first piece opens it, the space that joins the
    /// lines and the later pieces go into it, and its last piece closes it.
    fn link<E>(&mut self, link: &Link<E>, format: Format) -> Result<(), MarkdownError>
    where
        E: Extension,
    {
        let goes_on_from_previous = matches!(link.continues, Continues::Previous | Continues::Both);
        let hyperlink = match self.link.take() {
            Some(open) if goes_on_from_previous => open,
            unfinished => {
                self.content
                    .extend(unfinished.map(ParagraphContent::Hyperlink));
                self.hyperlink(link)?
            }
        };
        self.link = Some(hyperlink);
        if link.children.is_empty() {
            self.text(&link.target, format);
        } else {
            for child in &link.children {
                self.link_inline(child, format)?;
            }
        }
        if !matches!(link.continues, Continues::Next | Continues::Both) {
            self.content
                .extend(self.link.take().map(ParagraphContent::Hyperlink));
        }
        Ok(())
    }

    /// A node of a link's text, as the runs the same node outside a link gives.
    fn link_inline<E>(
        &mut self,
        inline: &LinkInline<E>,
        format: Format,
    ) -> Result<(), MarkdownError>
    where
        E: Extension,
    {
        match inline {
            LinkInline::Code(code) => self.text(
                &code.value,
                Format {
                    character_style: Some(CODE),
                    ..format
                },
            ),
            LinkInline::Emphasis(emphasis) => {
                for child in &emphasis.children {
                    self.link_inline(
                        child,
                        Format {
                            italic: true,
                            ..format
                        },
                    )?;
                }
            }
            LinkInline::Image(image) => {
                self.image(
                    &image.target,
                    &image.alt,
                    image.title.as_deref(),
                    image.continues,
                    format,
                )?;
            }
            LinkInline::LineBreak(_) => self.line_break(format),
            LinkInline::Strong(strong) => {
                for child in &strong.children {
                    self.link_inline(
                        child,
                        Format {
                            bold: true,
                            ..format
                        },
                    )?;
                }
            }
            LinkInline::Text(text) => self.text(&text.value, format),
            LinkInline::Html(_) | _ => {}
        }
        Ok(())
    }

    /// A run holding a `w:drawing` of the picture `target` names, `alt` its description and
    /// `title` its title: an image part named through `r:embed`, beside an SVG part for an SVG, or
    /// a web address named through `r:link`; a stored picture whose target an earlier one had
    /// reuses its parts.
    fn picture(
        &mut self,
        target: &str,
        alt: &str,
        title: Option<&str>,
        format: Format,
    ) -> Result<(), MarkdownError> {
        let (blip, size) = self.stored_picture(target)?;
        self.pictures.count += 1;
        let drawing = drawing_run(self.pictures.count, alt, title, blip, frame(size));
        self.run(TextRun::read(&drawing).content, format);
        Ok(())
    }

    /// A run of `content`; inside a link it goes into the hyperlink, styled `Hyperlink` unless it
    /// is code.
    fn run(&mut self, content: Vec<TextRunContent>, format: Format) {
        let mut run = TextRun::default();
        run.content = content;
        if let Some(hyperlink) = &mut self.link {
            let linked = Format {
                character_style: format.character_style.or(Some(HYPERLINK)),
                ..format
            };
            run.properties = linked.properties();
            hyperlink.content.push(ParagraphContent::TextRun(run));
        } else {
            run.properties = format.properties();
            self.content.push(ParagraphContent::TextRun(run));
        }
    }

    /// Store `image` as the next image part, `media/image{n}` beside the main part and related
    /// from it; returns the relationship id.
    fn store(&mut self, image: &Image) -> Result<String, PackageError> {
        self.pictures.parts += 1;
        let name = PartName::new(&format!(
            "{}/media/image{}.{}",
            self.main.directory(),
            self.pictures.parts,
            image.format().extension()
        ))?;
        self.package
            .set_part_bytes(&name, image.content_type(), image.bytes())?;
        self.package.relate(
            Some(self.main),
            &name,
            RelationshipType::IMAGE.in_class(ConformanceClass::Transitional),
        )
    }

    /// The `a:blip` of the picture `target` names and its frame size; a target stored before
    /// reads no file and stores no part.
    fn stored_picture(
        &mut self,
        target: &str,
    ) -> Result<(Element, Option<(u32, u32)>), MarkdownError> {
        if let Some(known) = self.pictures.stored.get(target).cloned() {
            return Ok((blip("embed", known.id, known.svg_id), known.size));
        }
        Ok(match picture::read(self.system, self.base, target)? {
            Picture::Linked(address) => {
                let id = self.package.relate_external(
                    Some(self.main),
                    &address,
                    RelationshipType::IMAGE.in_class(ConformanceClass::Transitional),
                )?;
                (blip("link", id, None), None)
            }
            Picture::Stored { shown, size, svg } => {
                let id = self.store(&shown)?;
                let svg_id = svg.map(|image| self.store(&image)).transpose()?;
                self.pictures.stored.insert(
                    target.to_owned(),
                    Stored {
                        id: id.clone(),
                        size,
                        svg_id: svg_id.clone(),
                    },
                );
                (blip("embed", id, svg_id), size)
            }
        })
    }

    /// One run of `value`: a tab in it is a `w:tab`, a line ending a `w:br`. Empty text writes
    /// nothing.
    fn text(&mut self, value: &str, format: Format) {
        let mut content = Vec::new();
        for (row, raw) in value.split('\n').enumerate() {
            if row > 0 {
                content.push(TextRunContent::Break(Break::default()));
            }
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            for (column, piece) in line.split('\t').enumerate() {
                if column > 0 {
                    content.push(TextRunContent::TabCharacter(TabCharacter::default()));
                }
                if !piece.is_empty() {
                    let mut characters = Text::default();
                    characters.space = Some("preserve".to_owned());
                    piece.clone_into(&mut characters.value);
                    content.push(TextRunContent::Text(characters));
                }
            }
        }
        if !content.is_empty() {
            self.run(content, format);
        }
    }
}

/// The anchor a markdown viewer gives a heading reading `text`: lower case, punctuation dropped,
/// each space a `-`.
fn anchor(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|character| character.is_alphanumeric() || matches!(character, ' ' | '-' | '_'))
        .map(|character| if character == ' ' { '-' } else { character })
        .collect()
}

/// `a:blip`, naming its image through `r:{reference}`, `r:embed` or `r:link`, of relationship
/// `id`, and for an SVG the SVG part through the `asvg:svgBlip` extension of relationship `svg`.
/// The extension is kept as written, so `asvg:svgBlip` declares the prefixes it uses.
fn blip(reference: &str, id: String, svg: Option<String>) -> Element {
    let mut blip = element(DRAWING, "a", "blip", &[], Vec::new());
    blip.attributes.push(Attribute::new(
        QName::prefixed("r", reference, RELATIONSHIPS),
        id,
    ));
    if let Some(svg_id) = svg {
        let mut svg_blip = element(SVG, "asvg", "svgBlip", &[], Vec::new());
        svg_blip.namespace_declarations = alloc::vec![
            NamespaceDeclaration::new("asvg".to_owned(), SVG.to_owned()),
            NamespaceDeclaration::new("r".to_owned(), RELATIONSHIPS.to_owned()),
        ];
        svg_blip.attributes.push(Attribute::new(
            QName::prefixed("r", "embed", RELATIONSHIPS),
            svg_id,
        ));
        let extension = element(
            DRAWING,
            "a",
            "ext",
            &[("uri", SVG_EXTENSION)],
            alloc::vec![svg_blip],
        );
        blip.children.push(Node::Element(element(
            DRAWING,
            "a",
            "extLst",
            &[],
            alloc::vec![extension],
        )));
    }
    blip
}

/// The Word blocks of `body`, its sections from depth 1. Its links, pictures and HTML blocks are
/// related from `main` in `package`, local pictures read through `system`, a relative path from
/// `base`, and the numbering part is added when it holds a list.
///
/// # Errors
///
/// The errors of [`picture::read`], [`Package::relate_external`], [`Package::add_part`] and
/// [`parts::add_numbering`].
pub fn blocks<S, E>(
    system: &S,
    base: Option<&Path>,
    body: &MarkdownBlock<E>,
    package: &mut Package,
    main: &PartName,
) -> Result<Vec<BlockContent>, MarkdownError>
where
    S: System + ?Sized,
    E: Extension,
{
    let mut writer = Body {
        base,
        blocks: Vec::new(),
        bookmarks: Bookmarks::default(),
        item: None,
        main,
        number: None,
        numbering: None,
        package,
        pictures: Pictures::default(),
        system,
    };
    writer.markdown_block(body)?;
    if let Some(starts) = &writer.numbering {
        parts::add_numbering(writer.package, main, starts)?;
    }
    Ok(writer.blocks)
}

/// The bookmark name `anchor` gives: each character but a letter, a digit or `_` written `_`;
/// `_` in front when it would start with anything but a letter, `_heading` when it is empty; at
/// most [`BOOKMARK_LENGTH`] characters. Word takes only a name starting with a letter, and a
/// leading `_` makes a hidden bookmark.
fn bookmark_name(anchor: &str) -> String {
    let kept: String = anchor
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let named = match kept.chars().next() {
        None => String::from("_heading"),
        Some(first) if first.is_alphabetic() || first == '_' => kept,
        Some(_) => format!("_{kept}"),
    };
    named.chars().take(BOOKMARK_LENGTH).collect()
}

/// A `w:r` holding the `w:drawing` of picture `number`, counting from 1: an inline picture of
/// `blip`, in a frame `cx` by `cy` EMU, whose `wp:docPr` holds `alt` as its description and
/// `title`, when there is one, as its title.
fn drawing_run(
    number: usize,
    alt: &str,
    title: Option<&str>,
    blip: Element,
    (cx, cy): (u64, u64),
) -> Element {
    let id = number.to_string();
    let name = format!("Picture {number}");
    let (width, height) = (cx.to_string(), cy.to_string());
    let extent = [("cx", width.as_str()), ("cy", height.as_str())];
    let mut properties = alloc::vec![("id", id.as_str()), ("name", name.as_str()), ("descr", alt)];
    properties.extend(title.map(|text| ("title", text)));
    let inline = element(
        WORDPROCESSING_DRAWING,
        "wp",
        "inline",
        &[],
        alloc::vec![
            element(WORDPROCESSING_DRAWING, "wp", "extent", &extent, Vec::new()),
            element(
                WORDPROCESSING_DRAWING,
                "wp",
                "docPr",
                &properties,
                Vec::new()
            ),
            graphic(&id, &name, blip, &extent),
        ],
    );
    element(
        WORDPROCESSING,
        "w",
        "r",
        &[],
        alloc::vec![element(
            WORDPROCESSING,
            "w",
            "drawing",
            &[],
            alloc::vec![inline]
        )],
    )
}

/// The element `prefix:local` of the namespace `uri`, holding `attributes`, each an unqualified
/// name and its value, and `children`.
fn element(
    uri: &str,
    prefix: &str,
    local: &str,
    attributes: &[(&str, &str)],
    children: Vec<Element>,
) -> Element {
    let mut element = Element::new(QName::prefixed(prefix, local, uri));
    element.attributes = attributes
        .iter()
        .map(|&(name, value)| Attribute::new(QName::unprefixed(name, ""), value.to_owned()))
        .collect();
    element.children = children.into_iter().map(Node::Element).collect();
    element
}

/// Whether the last of `inlines`, or the last node inside it, is a hard line break.
fn ends_in_break<E>(inlines: &[Inline<E>]) -> bool
where
    E: Extension,
{
    inlines.last().is_some_and(|last| match last {
        Inline::Emphasis(emphasis) => ends_in_break(&emphasis.children),
        Inline::LineBreak(_) => true,
        Inline::Link(link) => link_ends_in_break(&link.children),
        Inline::Strong(strong) => ends_in_break(&strong.children),
        Inline::Code(_) | Inline::Html(_) | Inline::Image(_) | Inline::Text(_) | _ => false,
    })
}

/// `a:graphic` holding the `pic:pic` of `blip`, its `pic:cNvPr` of `id` and `name`, stretched
/// over a rectangle of `extent`, its `cx` and `cy`.
fn graphic(id: &str, name: &str, blip: Element, extent: &[(&str, &str)]) -> Element {
    let non_visual = element(
        PICTURE,
        "pic",
        "nvPicPr",
        &[],
        alloc::vec![
            element(
                PICTURE,
                "pic",
                "cNvPr",
                &[("id", id), ("name", name)],
                Vec::new()
            ),
            element(PICTURE, "pic", "cNvPicPr", &[], Vec::new()),
        ],
    );
    let fill = element(
        PICTURE,
        "pic",
        "blipFill",
        &[],
        alloc::vec![
            blip,
            element(
                DRAWING,
                "a",
                "stretch",
                &[],
                alloc::vec![element(DRAWING, "a", "fillRect", &[], Vec::new())]
            ),
        ],
    );
    let shape = element(
        PICTURE,
        "pic",
        "spPr",
        &[],
        alloc::vec![
            element(
                DRAWING,
                "a",
                "xfrm",
                &[],
                alloc::vec![
                    element(DRAWING, "a", "off", &[("x", "0"), ("y", "0")], Vec::new()),
                    element(DRAWING, "a", "ext", extent, Vec::new()),
                ]
            ),
            element(
                DRAWING,
                "a",
                "prstGeom",
                &[("prst", "rect")],
                alloc::vec![element(DRAWING, "a", "avLst", &[], Vec::new())]
            ),
        ],
    );
    let picture = element(
        PICTURE,
        "pic",
        "pic",
        &[],
        alloc::vec![non_visual, fill, shape],
    );
    element(
        DRAWING,
        "a",
        "graphic",
        &[],
        alloc::vec![element(
            DRAWING,
            "a",
            "graphicData",
            &[("uri", PICTURE)],
            alloc::vec![picture]
        )],
    )
}

/// The frame of a picture `size` pixels big, width then height, `cx` and `cy` in EMU: 9,525 EMU a
/// pixel, scaled down to [`TEXT_WIDTH`] keeping its proportions when wider; a square
/// [`TEXT_WIDTH`] wide when its size is not known.
fn frame(size: Option<(u32, u32)>) -> (u64, u64) {
    let Some((across, down)) = size else {
        return (TEXT_WIDTH, TEXT_WIDTH);
    };
    let (width, height) = (u64::from(across), u64::from(down));
    if width * EMU_PER_PIXEL > TEXT_WIDTH {
        (TEXT_WIDTH, height * TEXT_WIDTH / width)
    } else {
        (width * EMU_PER_PIXEL, height * EMU_PER_PIXEL)
    }
}

/// The text of a heading's `inlines`, without their formatting.
fn heading_text<E>(inlines: &[Inline<E>]) -> String
where
    E: Extension,
{
    let mut text = String::new();
    push_text(&mut text, inlines);
    text
}

/// The `w:jc` of the cells of a column aligned `alignment`, `None` for [`Alignment::Default`].
fn justification(alignment: Alignment) -> Option<ParagraphAlignment> {
    let member = match alignment {
        Alignment::Center => HorizontalAlignmentTypeMember::Center,
        Alignment::Left => HorizontalAlignmentTypeMember::Left,
        Alignment::Right => HorizontalAlignmentTypeMember::Right,
        Alignment::Default | _ => return None,
    };
    let mut jc = ParagraphAlignment::default();
    jc.val = Some(member.into());
    Some(jc)
}

/// Whether the last of a link's `inlines`, or the last node inside it, is a hard line break.
fn link_ends_in_break<E>(inlines: &[LinkInline<E>]) -> bool
where
    E: Extension,
{
    inlines.last().is_some_and(|last| match last {
        LinkInline::Emphasis(emphasis) => link_ends_in_break(&emphasis.children),
        LinkInline::LineBreak(_) => true,
        LinkInline::Strong(strong) => link_ends_in_break(&strong.children),
        LinkInline::Code(_)
        | LinkInline::Html(_)
        | LinkInline::Image(_)
        | LinkInline::Text(_)
        | _ => false,
    })
}

/// `w:numPr` of list level `level` in numbering instance `instance`.
fn numbering_reference(level: usize, instance: usize) -> NumberingDefinitionInstanceReferenceNumPr {
    let mut level_reference = NumberingLevelReference::default();
    level_reference.val = Some(level.to_string().into());
    let mut instance_reference = NumberingDefinitionInstanceReferenceNumId::default();
    instance_reference.val = Some(instance.to_string().into());
    let mut reference = NumberingDefinitionInstanceReferenceNumPr::default();
    reference.numbering_definition_instance_reference_num_id = Some(Box::new(instance_reference));
    reference.numbering_level_reference = Some(Box::new(level_reference));
    reference
}

/// `w:pStyle` of style `id`.
fn paragraph_style(id: &str) -> ReferencedParagraphStyle {
    let mut reference = ReferencedParagraphStyle::default();
    reference.val = Some(id.to_owned());
    reference
}

/// The text of a link's `inlines`, without their formatting, added to `text`.
fn push_link_text<E>(text: &mut String, inlines: &[LinkInline<E>])
where
    E: Extension,
{
    for inline in inlines {
        match inline {
            LinkInline::Code(code) => text.push_str(&code.value),
            LinkInline::Emphasis(emphasis) => push_link_text(text, &emphasis.children),
            LinkInline::Strong(strong) => push_link_text(text, &strong.children),
            LinkInline::Text(characters) => text.push_str(&characters.value),
            LinkInline::Html(_) | LinkInline::Image(_) | LinkInline::LineBreak(_) | _ => {}
        }
    }
}

/// The text of `inlines`, without their formatting, added to `text`.
fn push_text<E>(text: &mut String, inlines: &[Inline<E>])
where
    E: Extension,
{
    for inline in inlines {
        match inline {
            Inline::Code(code) => text.push_str(&code.value),
            Inline::Emphasis(emphasis) => push_text(text, &emphasis.children),
            Inline::Link(link) => push_link_text(text, &link.children),
            Inline::Strong(strong) => push_text(text, &strong.children),
            Inline::Text(characters) => text.push_str(&characters.value),
            Inline::Html(_) | Inline::Image(_) | Inline::LineBreak(_) | _ => {}
        }
    }
}
