//! archie's blocks and inline nodes as Word paragraphs, runs, lists, tables and hyperlinks.

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;
use core::{iter, mem};

use archie_core::parser::ast::{
    Alignment, Block, Continues, Inline, Item, Line, Link, LinkInline, List, MarkdownBlock,
    Paragraph as ArchieParagraph, Section, Table as ArchieTable, TableRow as ArchieTableRow,
};
use archie_core::parser::extension::Extension;
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
use officekit_packaging::package::Package;
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::RelationshipType;
use officekit_xml::conformance::ConformanceClass;

use super::parts::{
    self, CODE, DEEPEST_HEADING, DEEPEST_LEVEL, HYPERLINK, LIST_PARAGRAPH, PREFORMATTED, QUOTE,
    TABLE_GRID,
};

/// The longest bookmark name Word takes.
const BOOKMARK_LENGTH: usize = 40;

const HTML_CONTENT_TYPE: &str = "text/html";

/// The names the part of an HTML block takes, from `/word/htmlChunk1.html`.
const HTML_PART: &str = "/word/htmlChunk{}.html";

/// The Word blocks written so far, and what the walk keeps from one block to the next.
struct Body<'package> {
    blocks: Vec<BlockContent>,
    bookmarks: Bookmarks,
    /// The depth of the list item whose later blocks are being written.
    item: Option<usize>,
    main: &'package PartName,
    /// The level and the numbering instance of the next paragraph, the first of a list item.
    number: Option<(usize, usize)>,
    /// The first number of each ordered list so far; `None` until the walk meets a list.
    numbering: Option<Vec<u64>>,
    package: &'package mut Package,
}

impl Body<'_> {
    fn block<E>(&mut self, block: &Block<E>) -> Result<(), PackageError>
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
    ) -> Result<TableCell, PackageError>
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
    fn item<E>(&mut self, item: &Item<E>, depth: usize, instance: usize) -> Result<(), PackageError>
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
    fn list<E>(&mut self, list: &List<E>) -> Result<(), PackageError>
    where
        E: Extension,
    {
        let depth = self.item.map_or(0, |outer| outer + 1);
        let ordered = self.numbering.get_or_insert_with(Vec::new);
        let instance = list.start.map_or(parts::BULLET_INSTANCE, |first| {
            ordered.push(first);
            parts::ordered_instance(ordered.len() - 1)
        });
        for item in &list.items {
            self.item(item, depth, instance)?;
        }
        Ok(())
    }

    /// The sections of `block`, from depth 1.
    fn markdown_block<E>(&mut self, block: &MarkdownBlock<E>) -> Result<(), PackageError>
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
    fn paragraph<E>(&mut self, paragraph: &ArchieParagraph<E>) -> Result<(), PackageError>
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
    /// `ListParagraph` and the item's indent when it continues one, `style` otherwise.
    fn properties(&mut self, style: Option<&str>) -> Option<Box<ParagraphProperties>> {
        let mut properties = ParagraphProperties::default();
        if let Some((level, instance)) = self.number.take() {
            properties.numbering_definition_instance_reference_num_pr =
                Some(Box::new(numbering_reference(level, instance)));
        } else if let Some(depth) = self.item {
            let mut indentation = ParagraphIndentation::default();
            indentation.left = Some(parts::level_indent(depth.min(DEEPEST_LEVEL)).into());
            properties.paragraph_indentation = Some(Box::new(indentation));
            properties.referenced_paragraph_style = Some(Box::new(paragraph_style(LIST_PARAGRAPH)));
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

    /// An empty writer of runs, relating links from the main part.
    const fn runs(&mut self) -> Runs<'_> {
        Runs {
            content: Vec::new(),
            link: None,
            main: self.main,
            package: &mut *self.package,
        }
    }

    /// The section's heading at `depth`, between the start and the end of its bookmark, then its
    /// blocks, then its subsections one level deeper.
    fn section<E>(&mut self, section: &Section<E>, depth: usize) -> Result<(), PackageError>
    where
        E: Extension,
    {
        if let Some(heading) = &section.heading {
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
    fn table<E>(&mut self, table: &ArchieTable<E>) -> Result<(), PackageError>
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
        row: &ArchieTableRow<E>,
        head: bool,
        columns: usize,
        alignments: &[Alignment],
    ) -> Result<TableRow, PackageError>
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

/// The runs of one paragraph, written so far, and the package its links are related in.
struct Runs<'package> {
    content: Vec<ParagraphContent>,
    /// The hyperlink the runs go into while a link is written, kept open while the link goes on
    /// into the next line.
    link: Option<Hyperlink>,
    main: &'package PartName,
    package: &'package mut Package,
}

impl Runs<'_> {
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

    fn inline<E>(&mut self, inline: &Inline<E>, format: Format) -> Result<(), PackageError>
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
            Inline::Html(_) | Inline::Image(_) | _ => {}
        }
        Ok(())
    }

    fn inlines<E>(&mut self, inlines: &[Inline<E>], format: Format) -> Result<(), PackageError>
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

    /// A `w:hyperlink` around the link's runs. A link written across lines, which archie cuts into
    /// one piece per line, is one hyperlink: its first piece opens it, the space that joins the
    /// lines and the later pieces go into it, and its last piece closes it.
    fn link<E>(&mut self, link: &Link<E>, format: Format) -> Result<(), PackageError>
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
        for child in &link.children {
            self.link_inline(child, format);
        }
        if !matches!(link.continues, Continues::Next | Continues::Both) {
            self.content
                .extend(self.link.take().map(ParagraphContent::Hyperlink));
        }
        Ok(())
    }

    /// A node of a link's text, as the runs the same node outside a link gives.
    fn link_inline<E>(&mut self, inline: &LinkInline<E>, format: Format)
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
                    );
                }
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
                    );
                }
            }
            LinkInline::Text(text) => self.text(&text.value, format),
            LinkInline::Html(_) | LinkInline::Image(_) | _ => {}
        }
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

/// The Word blocks of `body`, its sections from depth 1. Its links and HTML blocks are related
/// from `main` in `package`, and the numbering part is added when it holds a list.
///
/// # Errors
///
/// The errors of [`Package::relate_external`], [`Package::add_part`] and
/// [`parts::add_numbering`].
pub fn blocks<E>(
    body: &MarkdownBlock<E>,
    package: &mut Package,
    main: &PartName,
) -> Result<Vec<BlockContent>, PackageError>
where
    E: Extension,
{
    let mut writer = Body {
        blocks: Vec::new(),
        bookmarks: Bookmarks::default(),
        item: None,
        main,
        number: None,
        numbering: None,
        package,
    };
    writer.markdown_block(body)?;
    if let Some(starts) = &writer.numbering {
        parts::add_numbering(writer.package, main, starts)?;
    }
    Ok(writer.blocks)
}

/// The bookmark name `anchor` gives: each character but a letter, a digit or `_` written `_`,
/// cut to [`BOOKMARK_LENGTH`] characters.
fn bookmark_name(anchor: &str) -> String {
    anchor
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .take(BOOKMARK_LENGTH)
        .collect()
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
