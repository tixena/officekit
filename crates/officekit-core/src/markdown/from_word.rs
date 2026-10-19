//! A Word body as an archie document: headings as sections, paragraphs and their runs, code
//! blocks, block quotes, thematic breaks, and the paragraphs of text boxes.

#[cfg(test)]
mod tests;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::{iter, mem, ptr};

use archie_core::parser::ast::{
    Args, Block, BlockQuote, Code, CodeBlock, Document as ArchieDocument, Emphasis, Heading,
    Inline, Line, LineBreak, Mapping, MarkdownBlock, Paragraph as ArchieParagraph, Section, Strong,
    Text, ThematicBreak,
};
use officekit_markup::wordprocessing::{
    AssociatedOutlineLevel, BlockContent, BreakTypesMember, ComplexFieldCharacter,
    ComplexFieldCharacterType, ComplexFieldCharacterTypeMember, ContentBlockContent,
    ContentCellContent, ContentRowContent, Document, Paragraph, ParagraphContent,
    ParagraphProperties, RichTextBoxContentContainer, RunProperties, RunPropertiesContent,
    StyleDefinition, StyleTypesMember, TextRun, TextRunContent, TrackedRunContent,
};
use officekit_packaging::error::PackageError;
use officekit_packaging::package::{OpenOptions, Package};
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::Target;
use officekit_parts::style_definitions::{self, RELATIONSHIP_TYPE};
use officekit_xml::element::{Element, Node};
use officekit_xml::mc;
use officekit_xml::name::Namespace;
use officekit_xml::name::table::NAMESPACES;

use super::parts::{CODE, PREFORMATTED, QUOTE};

/// The name of the character style of inline code.
const CODE_NAME: &str = "HTML Code";

/// The fonts whose text is code.
const CODE_FONTS: [&str; 9] = [
    "Cascadia Code",
    "Cascadia Mono",
    "Consolas",
    "Courier",
    "Courier New",
    "Lucida Console",
    "Menlo",
    "Monaco",
    "Source Code Pro",
];

/// The deepest heading level markdown has; a heading at level 7, 8 or 9 is a bold paragraph.
const DEEPEST: usize = 6;

/// The deepest heading level Word has.
const DEEPEST_WORD: usize = 9;

const NON_BREAKING_HYPHEN: &str = "\u{2011}";

/// The name of the paragraph style of each line of a code block.
const PREFORMATTED_NAME: &str = "HTML Preformatted";

/// Consecutive paragraphs that become one block.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Gathering {
    Code,
    Quote,
}

/// What a paragraph is, by its style and its own properties.
enum Kind {
    Code,
    Heading(usize),
    Plain,
    Quote,
}

/// What a paragraph's runs give, in order.
enum Piece {
    Break,
    Text(Marks, String),
}

/// The lines of a paragraph as they are built: the stretch of text of the same marks still to
/// write, the line it goes on, and the lines before.
#[derive(Default)]
struct Lines {
    done: Vec<Line>,
    line: Vec<Inline>,
    marks: Marks,
    stretch: String,
}

/// Whether a stretch of text is bold, code, or italic.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Marks {
    bold: bool,
    code: bool,
    italic: bool,
}

/// What the runs of one paragraph give: its pieces, and the text boxes they hold.
#[derive(Default)]
struct Runs {
    boxes: Vec<RichTextBoxContentContainer>,
    pieces: Vec<Piece>,
}

/// The sections built so far: the open ones, one per depth from 1, and the closed sections at
/// depth 1.
#[derive(Default)]
struct Sections {
    done: Vec<Section>,
    open: Vec<Section>,
}

/// The styles of the styles part, by id, and the id of the default paragraph style.
#[derive(Default)]
pub struct Styles {
    by_id: BTreeMap<String, StyleDefinition>,
    default_paragraph: Option<String>,
}

/// The walk of a body in document order.
struct Walk<'styles> {
    /// Each complex field open, innermost last: whether its result has started.
    fields: Vec<bool>,
    gathered: Vec<String>,
    gathering: Option<Gathering>,
    sections: Sections,
    styles: &'styles Styles,
}

impl Lines {
    /// The lines, without the hard line breaks that end the paragraph.
    fn finish(mut self) -> Vec<Line> {
        self.flush();
        self.done.push(Line::new(mem::take(&mut self.line)));
        while self
            .done
            .last()
            .is_some_and(|line| line.children.is_empty())
        {
            self.done.pop();
            if let Some(last) = self.done.last_mut()
                && matches!(last.children.last(), Some(Inline::LineBreak(_)))
            {
                last.children.pop();
            }
        }
        self.done
    }

    /// The stretch as its node: bold as strong, italic as emphasis holding it, code inside both;
    /// the spaces at the ends of a bold or italic stretch go outside it.
    fn flush(&mut self) {
        let text = mem::take(&mut self.stretch);
        if text.is_empty() {
            return;
        }
        let marks = self.marks;
        if !marks.bold && !marks.italic {
            if marks.code {
                self.push(Inline::Code(Code::new(text)));
            } else {
                self.push_text(&text);
            }
            return;
        }
        let core = text.trim_matches(char::is_whitespace);
        if core.is_empty() {
            self.push_text(&text);
            return;
        }
        let start = text.len() - text.trim_start_matches(char::is_whitespace).len();
        let end = start + core.len();
        self.push_text(&text[..start]);
        let mut node = if marks.code {
            Inline::Code(Code::new(core.to_owned()))
        } else {
            Inline::Text(Text::new(core.to_owned()))
        };
        if marks.bold {
            node = Inline::Strong(Strong::new(alloc::vec![node]));
        }
        if marks.italic {
            node = Inline::Emphasis(Emphasis::new(alloc::vec![node]));
        }
        self.push(node);
        self.push_text(&text[end..]);
    }

    /// End the line with a hard line break; the text after it starts the next line.
    fn line_break(&mut self) {
        self.flush();
        self.line.push(Inline::LineBreak(LineBreak::default()));
        self.done.push(Line::new(mem::take(&mut self.line)));
    }

    fn push(&mut self, node: Inline) {
        self.line.push(node);
    }

    /// Plain `text`, joined to the plain text before it.
    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(Inline::Text(last)) = self.line.last_mut() {
            last.value.push_str(text);
        } else {
            self.push(Inline::Text(Text::new(text.to_owned())));
        }
    }

    /// `text` of `marks`, in the stretch when its marks are the same, else starting a new one.
    fn text(&mut self, marks: Marks, text: &str) {
        if marks != self.marks {
            self.flush();
            self.marks = marks;
        }
        self.stretch.push_str(text);
    }
}

impl Runs {
    /// What a piece of a shown run gives: text, a tab, a hard line break, or the text boxes a
    /// drawing holds.
    fn add(&mut self, item: &TextRunContent, marks: Marks) {
        match item {
            TextRunContent::Break(line) => {
                if line
                    .r#type
                    .as_ref()
                    .is_none_or(|kind| matches!(kind.value(), Ok(BreakTypesMember::TextWrapping)))
                {
                    self.pieces.push(Piece::Break);
                }
            }
            TextRunContent::CarriageReturn(_) => self.pieces.push(Piece::Break),
            TextRunContent::DrawingMLObject(_)
            | TextRunContent::EmbeddedObject(_)
            | TextRunContent::Other(_)
            | TextRunContent::VMLObject(_) => {
                text_boxes(&officekit_markup::generic(item), &mut self.boxes);
            }
            TextRunContent::NonBreakingHyphenCharacter(_) => self.text(marks, NON_BREAKING_HYPHEN),
            TextRunContent::TabCharacter(_) => self.text(marks, "\t"),
            TextRunContent::Text(text) => self.text(marks, &text.value),
            TextRunContent::AbsolutePositionTabCharacter(_)
            | TextRunContent::CommentContentReferenceMark(_)
            | TextRunContent::CommentInformationBlock(_)
            | TextRunContent::ComplexFieldCharacter(_)
            | TextRunContent::ContentPart(_)
            | TextRunContent::ContinuationSeparatorMark(_)
            | TextRunContent::DateBlockLongDayFormat(_)
            | TextRunContent::DateBlockLongMonthFormat(_)
            | TextRunContent::DateBlockLongYearFormat(_)
            | TextRunContent::DateBlockShortDayFormat(_)
            | TextRunContent::DateBlockShortMonthFormat(_)
            | TextRunContent::DateBlockShortYearFormat(_)
            | TextRunContent::DeletedFieldCode(_)
            | TextRunContent::DeletedText(_)
            | TextRunContent::EndnoteReference(_)
            | TextRunContent::EndnoteReferenceMark(_)
            | TextRunContent::FieldCode(_)
            | TextRunContent::FootnoteEndnoteSeparatorMark(_)
            | TextRunContent::FootnoteReference(_)
            | TextRunContent::FootnoteReferenceMark(_)
            | TextRunContent::OptionalHyphenCharacter(_)
            | TextRunContent::PageNumberBlock(_)
            | TextRunContent::PhoneticGuide(_)
            | TextRunContent::PositionOfLastCalculatedPageBreak(_)
            | TextRunContent::SymbolCharacter(_)
            | _ => {}
        }
    }

    /// Whether the pieces hold a character other than white space.
    fn has_text(&self) -> bool {
        self.pieces.iter().any(|piece| match piece {
            Piece::Break => false,
            Piece::Text(_, text) => text.chars().any(|character| !character.is_whitespace()),
        })
    }

    /// The text of an equation, plain.
    fn math(&mut self, item: &Node) {
        let mut text = String::new();
        math_text(item, &mut text);
        self.text(Marks::default(), &text);
    }

    /// `text` of `marks`, joined to the text before it when that has the same marks.
    fn text(&mut self, marks: Marks, text: &str) {
        if let Some(Piece::Text(before, last)) = self.pieces.last_mut()
            && *before == marks
        {
            last.push_str(text);
        } else {
            self.pieces.push(Piece::Text(marks, text.to_owned()));
        }
    }
}

impl Sections {
    /// `block`, last in the deepest open section, or in a new section with no heading.
    fn block(&mut self, block: Block) {
        if let Some(section) = self.open.last_mut() {
            section.blocks.push(block);
        } else {
            self.open
                .push(Section::new(None, alloc::vec![block], Vec::new()));
        }
    }

    /// Close the open sections at `depth` or deeper.
    fn close(&mut self, depth: usize) {
        while self.open.len() >= depth
            && let Some(section) = self.open.pop()
        {
            if let Some(parent) = self.open.last_mut() {
                parent.subsections.push(section);
            } else {
                self.done.push(section);
            }
        }
    }

    /// Every section at depth 1, each closed.
    fn finish(mut self) -> Vec<Section> {
        self.close(1);
        self.done
    }

    /// A new section of `heading` at `depth`, after the open ones at its depth or deeper close; a
    /// skipped level above it is a section with no heading.
    fn heading(&mut self, depth: usize, heading: Heading) {
        self.close(depth);
        while self.open.len() + 1 < depth {
            self.open.push(Section::new(None, Vec::new(), Vec::new()));
        }
        self.open
            .push(Section::new(Some(heading), Vec::new(), Vec::new()));
    }
}

impl Styles {
    /// The style `id`, then the style it is based on, and so on, until a style is missing or one
    /// comes again.
    fn chain(&self, id: Option<&str>) -> Vec<&StyleDefinition> {
        let mut chain: Vec<&StyleDefinition> = Vec::new();
        let mut next = id;
        while let Some(style) = next.and_then(|current| self.by_id.get(current))
            && !chain.iter().any(|seen| ptr::eq(*seen, style))
        {
            chain.push(style);
            next = style
                .parent_style_id
                .as_ref()
                .and_then(|parent| parent.val.as_deref());
        }
        chain
    }

    /// The heading level of a paragraph of `properties`, from 1 to 9: its own outline level, else
    /// the first its style's chain gives, by a style named `heading N` or by an outline level.
    fn heading_level(&self, properties: Option<&ParagraphProperties>) -> Option<usize> {
        if let Some(level) = properties.and_then(|own| own.associated_outline_level.as_deref()) {
            return outline_level(level);
        }
        self.chain(self.paragraph_style(properties))
            .into_iter()
            .find_map(|style| {
                let named = style
                    .primary_style_name
                    .as_ref()
                    .and_then(|name| name.val.as_deref())
                    .and_then(heading_named);
                named.map(Some).or_else(|| {
                    style
                        .paragraph_properties_p_pr_general
                        .as_ref()
                        .and_then(|general| general.associated_outline_level.as_deref())
                        .map(outline_level)
                })
            })
            .flatten()
    }

    /// Whether the style `id` is the built-in style of id `builtin` and name `name`.
    fn is(&self, id: Option<&str>, builtin: &str, name: &str) -> bool {
        id.is_some_and(|style| {
            style == builtin
                || self
                    .by_id
                    .get(style)
                    .and_then(|definition| definition.primary_style_name.as_ref())
                    .and_then(|primary| primary.val.as_deref())
                    .is_some_and(|named| named.eq_ignore_ascii_case(name))
        })
    }

    /// What a paragraph of `properties` is: a heading, a line of code or of a quote, or plain.
    fn kind(&self, properties: Option<&ParagraphProperties>) -> Kind {
        self.heading_level(properties).map_or_else(
            || {
                let style = self.paragraph_style(properties);
                if self.is(style, PREFORMATTED, PREFORMATTED_NAME) {
                    Kind::Code
                } else if self.is(style, QUOTE, QUOTE) {
                    Kind::Quote
                } else {
                    Kind::Plain
                }
            },
            Kind::Heading,
        )
    }

    /// A run's marks: bold and italic set on the run or by its character style's chain, code by
    /// the style `HTMLCode` or a code font.
    fn marks(&self, properties: Option<&RunProperties>) -> Marks {
        let own = properties.map_or(&[][..], |run| run.content.as_slice());
        let style = own.iter().find_map(|property| {
            if let RunPropertiesContent::ReferencedCharacterStyle(reference) = property {
                reference.val.as_deref()
            } else {
                None
            }
        });
        let chain = self.chain(style);
        let lists: Vec<&[RunPropertiesContent]> = iter::once(own)
            .chain(chain.iter().map(|definition| {
                definition
                    .run_properties
                    .as_ref()
                    .map_or(&[][..], |run| run.content.as_slice())
            }))
            .collect();
        let first = |pick: fn(&RunPropertiesContent) -> Option<bool>| {
            lists
                .iter()
                .find_map(|list| list.iter().find_map(pick))
                .unwrap_or(false)
        };
        let font = lists
            .iter()
            .find_map(|list| list.iter().find_map(ascii_font));
        Marks {
            bold: first(bold),
            code: self.is(style, CODE, CODE_NAME)
                || font.is_some_and(|name| {
                    CODE_FONTS
                        .iter()
                        .any(|code| code.eq_ignore_ascii_case(name))
                }),
            italic: first(italic),
        }
    }

    /// The paragraph style of a paragraph of `properties`: its own, else the default one.
    fn paragraph_style<'style>(
        &'style self,
        properties: Option<&'style ParagraphProperties>,
    ) -> Option<&'style str> {
        properties
            .and_then(|own| own.referenced_paragraph_style.as_ref())
            .and_then(|style| style.val.as_deref())
            .or(self.default_paragraph.as_deref())
    }

    /// The styles of the styles part `main` relates to, read with `options`; none when it relates
    /// to none.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::related_by_type`] and [`style_definitions::load`].
    pub fn read(
        package: &Package,
        main: &PartName,
        options: OpenOptions,
    ) -> Result<Self, PackageError> {
        let part = package
            .related_by_type(Some(main), &RELATIONSHIP_TYPE)?
            .into_iter()
            .find_map(|related| {
                if let Target::Part(name) = related.target {
                    Some(name)
                } else {
                    None
                }
            });
        let Some(name) = part else {
            return Ok(Self::default());
        };
        let mut styles = Self::default();
        for style in style_definitions::load(package, &name, options)?.style_definition {
            let Some(id) = style.style_id.clone() else {
                continue;
            };
            if styles.default_paragraph.is_none() && is_default_paragraph(&style) {
                styles.default_paragraph = Some(id.clone());
            }
            styles.by_id.entry(id).or_insert(style);
        }
        Ok(styles)
    }
}

impl Walk<'_> {
    fn blocks(&mut self, blocks: &[BlockContent]) {
        for block in blocks {
            match block {
                BlockContent::BlockLevelCustomXMLElement(element) => {
                    self.content_blocks(&element.content);
                }
                BlockContent::BlockLevelStructuredDocumentTag(tag) => {
                    if let Some(inner) = &tag.content {
                        self.content_blocks(&inner.content);
                    }
                }
                BlockContent::Paragraph(paragraph) => self.paragraph(paragraph),
                BlockContent::Table(table) => self.rows(&table.content_row_content),
                BlockContent::AnchorForImportedExternalContent(_)
                | BlockContent::BookmarkEnd(_)
                | BlockContent::BookmarkStart(_)
                | BlockContent::CommentAnchorRangeEnd(_)
                | BlockContent::CommentAnchorRangeStart(_)
                | BlockContent::ConflictDelRunTrackChange(_)
                | BlockContent::ConflictInsRunTrackChange(_)
                | BlockContent::CustomXMLMarkupDeletionEnd(_)
                | BlockContent::CustomXMLMarkupDeletionStart(_)
                | BlockContent::CustomXMLMarkupInsertionEnd(_)
                | BlockContent::CustomXMLMarkupInsertionStart(_)
                | BlockContent::CustomXMLMarkupMoveSourceEnd(_)
                | BlockContent::CustomXMLMarkupMoveSourceStart(_)
                | BlockContent::CustomXmlConflictDelRangeEnd(_)
                | BlockContent::CustomXmlConflictDelRangeStart(_)
                | BlockContent::CustomXmlConflictInsRangeEnd(_)
                | BlockContent::CustomXmlConflictInsRangeStart(_)
                | BlockContent::CustomXmlMoveToRangeEnd(_)
                | BlockContent::CustomXmlMoveToRangeStart(_)
                | BlockContent::DeletedRunContent(_)
                | BlockContent::InsertedRunContent(_)
                | BlockContent::MoveDestinationLocationContainerEnd(_)
                | BlockContent::MoveDestinationLocationContainerStart(_)
                | BlockContent::MoveDestinationRunContent(_)
                | BlockContent::MoveSourceLocationContainerEnd(_)
                | BlockContent::MoveSourceLocationContainerStart(_)
                | BlockContent::MoveSourceRunContent(_)
                | BlockContent::OfficeMath(_)
                | BlockContent::OfficeMathParagraph(_)
                | BlockContent::Other(_)
                | BlockContent::ProofingErrorAnchor(_)
                | BlockContent::RangePermissionEnd(_)
                | BlockContent::RangePermissionStart(_)
                | _ => {}
            }
        }
    }

    /// The cells of a table row, each cell's blocks in order.
    fn cells(&mut self, cells: &[ContentCellContent]) {
        for item in cells {
            match item {
                ContentCellContent::CellLevelCustomXMLElement(element) => {
                    self.cells(&element.content);
                }
                ContentCellContent::CellLevelStructuredDocumentTag(tag) => {
                    if let Some(inner) = &tag.content {
                        self.cells(&inner.content);
                    }
                }
                ContentCellContent::TableCell(cell) => self.blocks(&cell.content),
                ContentCellContent::BookmarkEnd(_)
                | ContentCellContent::BookmarkStart(_)
                | ContentCellContent::CommentAnchorRangeEnd(_)
                | ContentCellContent::CommentAnchorRangeStart(_)
                | ContentCellContent::ConflictDelRunTrackChange(_)
                | ContentCellContent::ConflictInsRunTrackChange(_)
                | ContentCellContent::CustomXMLMarkupDeletionEnd(_)
                | ContentCellContent::CustomXMLMarkupDeletionStart(_)
                | ContentCellContent::CustomXMLMarkupInsertionEnd(_)
                | ContentCellContent::CustomXMLMarkupInsertionStart(_)
                | ContentCellContent::CustomXMLMarkupMoveSourceEnd(_)
                | ContentCellContent::CustomXMLMarkupMoveSourceStart(_)
                | ContentCellContent::CustomXmlConflictDelRangeEnd(_)
                | ContentCellContent::CustomXmlConflictDelRangeStart(_)
                | ContentCellContent::CustomXmlConflictInsRangeEnd(_)
                | ContentCellContent::CustomXmlConflictInsRangeStart(_)
                | ContentCellContent::CustomXmlMoveToRangeEnd(_)
                | ContentCellContent::CustomXmlMoveToRangeStart(_)
                | ContentCellContent::DeletedRunContent(_)
                | ContentCellContent::InsertedRunContent(_)
                | ContentCellContent::MoveDestinationLocationContainerEnd(_)
                | ContentCellContent::MoveDestinationLocationContainerStart(_)
                | ContentCellContent::MoveDestinationRunContent(_)
                | ContentCellContent::MoveSourceLocationContainerEnd(_)
                | ContentCellContent::MoveSourceLocationContainerStart(_)
                | ContentCellContent::MoveSourceRunContent(_)
                | ContentCellContent::OfficeMath(_)
                | ContentCellContent::OfficeMathParagraph(_)
                | ContentCellContent::Other(_)
                | ContentCellContent::ProofingErrorAnchor(_)
                | ContentCellContent::RangePermissionEnd(_)
                | ContentCellContent::RangePermissionStart(_)
                | _ => {}
            }
        }
    }

    /// The blocks of a content control or a custom XML element.
    fn content_blocks(&mut self, blocks: &[ContentBlockContent]) {
        for block in blocks {
            match block {
                ContentBlockContent::BlockLevelCustomXMLElement(element) => {
                    self.content_blocks(&element.content);
                }
                ContentBlockContent::BlockLevelStructuredDocumentTag(tag) => {
                    if let Some(inner) = &tag.content {
                        self.content_blocks(&inner.content);
                    }
                }
                ContentBlockContent::Paragraph(paragraph) => self.paragraph(paragraph),
                ContentBlockContent::Table(table) => self.rows(&table.content_row_content),
                ContentBlockContent::BookmarkEnd(_)
                | ContentBlockContent::BookmarkStart(_)
                | ContentBlockContent::CommentAnchorRangeEnd(_)
                | ContentBlockContent::CommentAnchorRangeStart(_)
                | ContentBlockContent::ConflictDelRunTrackChange(_)
                | ContentBlockContent::ConflictInsRunTrackChange(_)
                | ContentBlockContent::CustomXMLMarkupDeletionEnd(_)
                | ContentBlockContent::CustomXMLMarkupDeletionStart(_)
                | ContentBlockContent::CustomXMLMarkupInsertionEnd(_)
                | ContentBlockContent::CustomXMLMarkupInsertionStart(_)
                | ContentBlockContent::CustomXMLMarkupMoveSourceEnd(_)
                | ContentBlockContent::CustomXMLMarkupMoveSourceStart(_)
                | ContentBlockContent::CustomXmlConflictDelRangeEnd(_)
                | ContentBlockContent::CustomXmlConflictDelRangeStart(_)
                | ContentBlockContent::CustomXmlConflictInsRangeEnd(_)
                | ContentBlockContent::CustomXmlConflictInsRangeStart(_)
                | ContentBlockContent::CustomXmlMoveToRangeEnd(_)
                | ContentBlockContent::CustomXmlMoveToRangeStart(_)
                | ContentBlockContent::DeletedRunContent(_)
                | ContentBlockContent::InsertedRunContent(_)
                | ContentBlockContent::MoveDestinationLocationContainerEnd(_)
                | ContentBlockContent::MoveDestinationLocationContainerStart(_)
                | ContentBlockContent::MoveDestinationRunContent(_)
                | ContentBlockContent::MoveSourceLocationContainerEnd(_)
                | ContentBlockContent::MoveSourceLocationContainerStart(_)
                | ContentBlockContent::MoveSourceRunContent(_)
                | ContentBlockContent::OfficeMath(_)
                | ContentBlockContent::OfficeMathParagraph(_)
                | ContentBlockContent::Other(_)
                | ContentBlockContent::ProofingErrorAnchor(_)
                | ContentBlockContent::RangePermissionEnd(_)
                | ContentBlockContent::RangePermissionStart(_)
                | _ => {}
            }
        }
    }

    /// A `w:fldChar`: `begin` opens a field, `separate` starts its result, `end` closes it.
    fn field_character(&mut self, character: &ComplexFieldCharacter) {
        let Some(Ok(kind)) = character
            .fld_char_type
            .as_ref()
            .map(ComplexFieldCharacterType::value)
        else {
            return;
        };
        match kind {
            ComplexFieldCharacterTypeMember::Begin => self.fields.push(false),
            ComplexFieldCharacterTypeMember::End => {
                self.fields.pop();
            }
            ComplexFieldCharacterTypeMember::Separate => {
                if let Some(result) = self.fields.last_mut() {
                    *result = true;
                }
            }
            _ => {}
        }
    }

    /// The code block or block quote of the paragraphs gathered, if any.
    fn flush(&mut self) {
        let Some(gathering) = self.gathering.take() else {
            return;
        };
        let value = mem::take(&mut self.gathered).join("\n");
        let block = match gathering {
            Gathering::Code => {
                Block::CodeBlock(CodeBlock::new(None, Args::Toml(Mapping::new()), value))
            }
            Gathering::Quote => Block::BlockQuote(BlockQuote::new(value)),
        };
        self.sections.block(block);
    }

    /// `line`, the text of a code or quote paragraph, after the lines of the paragraphs of its
    /// kind right before it.
    fn gather(&mut self, gathering: Gathering, line: String) {
        if self.gathering != Some(gathering) {
            self.flush();
            self.gathering = Some(gathering);
        }
        self.gathered.push(line);
    }

    /// A paragraph as the node its kind gives, then the paragraphs of its text boxes.
    fn paragraph(&mut self, paragraph: &Paragraph) {
        let mut runs = Runs::default();
        self.paragraph_content(&paragraph.content, &mut runs);
        let has_text = runs.has_text();
        let Runs { boxes, pieces } = runs;
        let properties = paragraph.properties.as_deref();
        match self.styles.kind(properties) {
            Kind::Code => self.gather(Gathering::Code, plain(pieces)),
            Kind::Heading(level) if level <= DEEPEST => {
                self.flush();
                if has_text {
                    self.sections
                        .heading(level, Heading::new(heading_inlines(pieces)));
                }
            }
            Kind::Heading(_) => {
                self.flush();
                if has_text {
                    let bold = pieces.into_iter().map(|piece| match piece {
                        Piece::Break => Piece::Break,
                        Piece::Text(marks, text) => Piece::Text(
                            Marks {
                                bold: true,
                                ..marks
                            },
                            text,
                        ),
                    });
                    self.sections
                        .block(Block::Paragraph(ArchieParagraph::new(lines(bold))));
                }
            }
            Kind::Plain => {
                self.flush();
                let block = if has_text {
                    Some(Block::Paragraph(ArchieParagraph::new(lines(pieces))))
                } else {
                    is_rule(properties).then(|| Block::ThematicBreak(ThematicBreak::default()))
                };
                if let Some(rule_or_paragraph) = block {
                    self.sections.block(rule_or_paragraph);
                }
            }
            Kind::Quote => self.gather(Gathering::Quote, plain(pieces)),
        }
        for text_box in boxes {
            self.blocks(&text_box.content);
        }
    }

    fn paragraph_content(&mut self, content: &[ParagraphContent], runs: &mut Runs) {
        for item in content {
            match item {
                ParagraphContent::BidirectionalEmbeddingLevel(level) => {
                    self.paragraph_content(&level.content, runs);
                }
                ParagraphContent::BidirectionalOverride(direction) => {
                    self.paragraph_content(&direction.content, runs);
                }
                ParagraphContent::Hyperlink(link) => self.paragraph_content(&link.content, runs),
                ParagraphContent::InlineLevelCustomXMLElement(element) => {
                    self.paragraph_content(&element.content, runs);
                }
                ParagraphContent::InlineLevelSmartTag(tag) => {
                    self.paragraph_content(&tag.content, runs);
                }
                ParagraphContent::InlineLevelStructuredDocumentTag(tag) => {
                    if let Some(inner) = &tag.content {
                        self.paragraph_content(&inner.content, runs);
                    }
                }
                ParagraphContent::InsertedRunContent(insertion) => {
                    self.tracked(&insertion.content, runs);
                }
                ParagraphContent::MoveDestinationRunContent(destination) => {
                    self.tracked(&destination.content, runs);
                }
                ParagraphContent::OfficeMath(_) | ParagraphContent::OfficeMathParagraph(_) => {
                    if self.shown() {
                        runs.math(&officekit_markup::generic(item));
                    }
                }
                ParagraphContent::SimpleField(field) => {
                    self.paragraph_content(&field.content, runs);
                }
                ParagraphContent::TextRun(run) => self.run(run, runs),
                ParagraphContent::AnchorForSubdocumentLocation(_)
                | ParagraphContent::BookmarkEnd(_)
                | ParagraphContent::BookmarkStart(_)
                | ParagraphContent::CommentAnchorRangeEnd(_)
                | ParagraphContent::CommentAnchorRangeStart(_)
                | ParagraphContent::ConflictDelRunTrackChange(_)
                | ParagraphContent::ConflictInsRunTrackChange(_)
                | ParagraphContent::CustomXMLMarkupDeletionEnd(_)
                | ParagraphContent::CustomXMLMarkupDeletionStart(_)
                | ParagraphContent::CustomXMLMarkupInsertionEnd(_)
                | ParagraphContent::CustomXMLMarkupInsertionStart(_)
                | ParagraphContent::CustomXMLMarkupMoveSourceEnd(_)
                | ParagraphContent::CustomXMLMarkupMoveSourceStart(_)
                | ParagraphContent::CustomXmlConflictDelRangeEnd(_)
                | ParagraphContent::CustomXmlConflictDelRangeStart(_)
                | ParagraphContent::CustomXmlConflictInsRangeEnd(_)
                | ParagraphContent::CustomXmlConflictInsRangeStart(_)
                | ParagraphContent::CustomXmlMoveToRangeEnd(_)
                | ParagraphContent::CustomXmlMoveToRangeStart(_)
                | ParagraphContent::DeletedRunContent(_)
                | ParagraphContent::MoveDestinationLocationContainerEnd(_)
                | ParagraphContent::MoveDestinationLocationContainerStart(_)
                | ParagraphContent::MoveSourceLocationContainerEnd(_)
                | ParagraphContent::MoveSourceLocationContainerStart(_)
                | ParagraphContent::MoveSourceRunContent(_)
                | ParagraphContent::Other(_)
                | ParagraphContent::ProofingErrorAnchor(_)
                | ParagraphContent::RangePermissionEnd(_)
                | ParagraphContent::RangePermissionStart(_)
                | _ => {}
            }
        }
    }

    /// The rows of a table, each row's cells in order.
    fn rows(&mut self, rows: &[ContentRowContent]) {
        self.flush();
        for item in rows {
            match item {
                ContentRowContent::RowLevelCustomXMLElement(element) => {
                    self.rows(&element.content);
                }
                ContentRowContent::RowLevelStructuredDocumentTag(tag) => {
                    if let Some(inner) = &tag.content {
                        self.rows(&inner.content);
                    }
                }
                ContentRowContent::TableRow(row) => self.cells(&row.content),
                ContentRowContent::BookmarkEnd(_)
                | ContentRowContent::BookmarkStart(_)
                | ContentRowContent::CommentAnchorRangeEnd(_)
                | ContentRowContent::CommentAnchorRangeStart(_)
                | ContentRowContent::ConflictDelRunTrackChange(_)
                | ContentRowContent::ConflictInsRunTrackChange(_)
                | ContentRowContent::CustomXMLMarkupDeletionEnd(_)
                | ContentRowContent::CustomXMLMarkupDeletionStart(_)
                | ContentRowContent::CustomXMLMarkupInsertionEnd(_)
                | ContentRowContent::CustomXMLMarkupInsertionStart(_)
                | ContentRowContent::CustomXMLMarkupMoveSourceEnd(_)
                | ContentRowContent::CustomXMLMarkupMoveSourceStart(_)
                | ContentRowContent::CustomXmlConflictDelRangeEnd(_)
                | ContentRowContent::CustomXmlConflictDelRangeStart(_)
                | ContentRowContent::CustomXmlConflictInsRangeEnd(_)
                | ContentRowContent::CustomXmlConflictInsRangeStart(_)
                | ContentRowContent::CustomXmlMoveToRangeEnd(_)
                | ContentRowContent::CustomXmlMoveToRangeStart(_)
                | ContentRowContent::DeletedRunContent(_)
                | ContentRowContent::InsertedRunContent(_)
                | ContentRowContent::MoveDestinationLocationContainerEnd(_)
                | ContentRowContent::MoveDestinationLocationContainerStart(_)
                | ContentRowContent::MoveDestinationRunContent(_)
                | ContentRowContent::MoveSourceLocationContainerEnd(_)
                | ContentRowContent::MoveSourceLocationContainerStart(_)
                | ContentRowContent::MoveSourceRunContent(_)
                | ContentRowContent::OfficeMath(_)
                | ContentRowContent::OfficeMathParagraph(_)
                | ContentRowContent::Other(_)
                | ContentRowContent::ProofingErrorAnchor(_)
                | ContentRowContent::RangePermissionEnd(_)
                | ContentRowContent::RangePermissionStart(_)
                | _ => {}
            }
        }
        self.flush();
    }

    /// A run's pieces, of the run's marks, and the text boxes of the nodes the schemas do not
    /// declare in it, such as `mc:AlternateContent`; a field's instruction, and a result its field
    /// does not show, give none.
    fn run(&mut self, run: &TextRun, runs: &mut Runs) {
        let marks = self.styles.marks(run.properties.as_deref());
        for item in &run.content {
            if let TextRunContent::ComplexFieldCharacter(character) = item {
                self.field_character(character);
                continue;
            }
            if self.shown() {
                runs.add(item, marks);
            }
        }
        if self.shown() {
            for extra in &run.unknown.children {
                text_boxes(&extra.node, &mut runs.boxes);
            }
        }
    }

    /// Whether the text read now shows: every field open is in its result.
    fn shown(&self) -> bool {
        self.fields.iter().all(|result| *result)
    }

    /// The content of an insertion or a move's destination, as if the change were accepted.
    fn tracked(&mut self, content: &[TrackedRunContent], runs: &mut Runs) {
        for item in content {
            match item {
                TrackedRunContent::BidirectionalEmbeddingLevel(level) => {
                    self.paragraph_content(&level.content, runs);
                }
                TrackedRunContent::BidirectionalOverride(direction) => {
                    self.paragraph_content(&direction.content, runs);
                }
                TrackedRunContent::InlineLevelCustomXMLElement(element) => {
                    self.paragraph_content(&element.content, runs);
                }
                TrackedRunContent::InlineLevelSmartTag(tag) => {
                    self.paragraph_content(&tag.content, runs);
                }
                TrackedRunContent::InlineLevelStructuredDocumentTag(tag) => {
                    if let Some(inner) = &tag.content {
                        self.paragraph_content(&inner.content, runs);
                    }
                }
                TrackedRunContent::InsertedRunContent(insertion) => {
                    self.tracked(&insertion.content, runs);
                }
                TrackedRunContent::MoveDestinationRunContent(destination) => {
                    self.tracked(&destination.content, runs);
                }
                TrackedRunContent::OfficeMath(_) | TrackedRunContent::OfficeMathParagraph(_) => {
                    if self.shown() {
                        runs.math(&officekit_markup::generic(item));
                    }
                }
                TrackedRunContent::TextRun(run) => self.run(run, runs),
                TrackedRunContent::Accent(_)
                | TrackedRunContent::ArrayObject(_)
                | TrackedRunContent::Bar(_)
                | TrackedRunContent::BookmarkEnd(_)
                | TrackedRunContent::BookmarkStart(_)
                | TrackedRunContent::BorderBoxObject(_)
                | TrackedRunContent::BoxObject(_)
                | TrackedRunContent::CommentAnchorRangeEnd(_)
                | TrackedRunContent::CommentAnchorRangeStart(_)
                | TrackedRunContent::ConflictDelRunTrackChange(_)
                | TrackedRunContent::ConflictInsRunTrackChange(_)
                | TrackedRunContent::CustomXMLMarkupDeletionEnd(_)
                | TrackedRunContent::CustomXMLMarkupDeletionStart(_)
                | TrackedRunContent::CustomXMLMarkupInsertionEnd(_)
                | TrackedRunContent::CustomXMLMarkupInsertionStart(_)
                | TrackedRunContent::CustomXMLMarkupMoveSourceEnd(_)
                | TrackedRunContent::CustomXMLMarkupMoveSourceStart(_)
                | TrackedRunContent::CustomXmlConflictDelRangeEnd(_)
                | TrackedRunContent::CustomXmlConflictDelRangeStart(_)
                | TrackedRunContent::CustomXmlConflictInsRangeEnd(_)
                | TrackedRunContent::CustomXmlConflictInsRangeStart(_)
                | TrackedRunContent::CustomXmlMoveToRangeEnd(_)
                | TrackedRunContent::CustomXmlMoveToRangeStart(_)
                | TrackedRunContent::DeletedRunContent(_)
                | TrackedRunContent::DelimiterObject(_)
                | TrackedRunContent::FractionObject(_)
                | TrackedRunContent::FunctionApplyObject(_)
                | TrackedRunContent::GroupCharacterObject(_)
                | TrackedRunContent::LowerLimitObject(_)
                | TrackedRunContent::MatrixObject(_)
                | TrackedRunContent::MoveDestinationLocationContainerEnd(_)
                | TrackedRunContent::MoveDestinationLocationContainerStart(_)
                | TrackedRunContent::MoveSourceLocationContainerEnd(_)
                | TrackedRunContent::MoveSourceLocationContainerStart(_)
                | TrackedRunContent::MoveSourceRunContent(_)
                | TrackedRunContent::Nary(_)
                | TrackedRunContent::Other(_)
                | TrackedRunContent::PhantomObject(_)
                | TrackedRunContent::PreSubSuperscriptObject(_)
                | TrackedRunContent::ProofingErrorAnchor(_)
                | TrackedRunContent::RadicalObject(_)
                | TrackedRunContent::RangePermissionEnd(_)
                | TrackedRunContent::RangePermissionStart(_)
                | TrackedRunContent::Run(_)
                | TrackedRunContent::SubSuperscriptObject(_)
                | TrackedRunContent::SubscriptObject(_)
                | TrackedRunContent::SuperscriptObject(_)
                | TrackedRunContent::UpperLimitObject(_)
                | _ => {}
            }
        }
    }
}

/// The font of a run's `w:rFonts/@w:ascii`, when `property` gives one.
fn ascii_font(property: &RunPropertiesContent) -> Option<&str> {
    if let RunPropertiesContent::RunFonts(fonts) = property {
        fonts.ascii.as_deref()
    } else {
        None
    }
}

/// Whether `property` is `w:b`, and turns bold on.
fn bold(property: &RunPropertiesContent) -> Option<bool> {
    if let RunPropertiesContent::Bold(bold) = property {
        Some(
            bold.val
                .as_ref()
                .is_none_or(|value| value.value().unwrap_or(true)),
        )
    } else {
        None
    }
}

/// The archie document of `root`, its styles read from `styles`.
pub fn document(root: &Document, styles: &Styles) -> ArchieDocument {
    let mut walk = Walk {
        fields: Vec::new(),
        gathered: Vec::new(),
        gathering: None,
        sections: Sections::default(),
        styles,
    };
    if let Some(body) = &root.body {
        walk.blocks(&body.content);
    }
    walk.flush();
    ArchieDocument::new(MarkdownBlock::new(walk.sections.finish()), None)
}

/// The inline nodes of a heading: one line, each hard line break a space.
fn heading_inlines(pieces: Vec<Piece>) -> Vec<Inline> {
    let spaced = pieces.into_iter().map(|piece| match piece {
        Piece::Break => Piece::Text(Marks::default(), " ".to_owned()),
        Piece::Text(marks, text) => Piece::Text(marks, text),
    });
    lines(spaced)
        .into_iter()
        .next()
        .map_or_else(Vec::new, |line| line.children)
}

/// The level of a style named `heading N`, `N` from 1 to 9, in any case.
fn heading_named(name: &str) -> Option<usize> {
    let lower = name.to_ascii_lowercase();
    lower
        .strip_prefix("heading ")
        .and_then(|number| number.parse::<usize>().ok())
        .filter(|level| (1..=DEEPEST_WORD).contains(level))
}

/// Whether `style` is the default paragraph style.
fn is_default_paragraph(style: &StyleDefinition) -> bool {
    let paragraph = style
        .r#type
        .as_ref()
        .is_none_or(|kind| matches!(kind.value(), Ok(StyleTypesMember::Paragraph)));
    let default = style
        .default
        .as_ref()
        .is_some_and(|value| value.value().unwrap_or(false));
    paragraph && default
}

/// Whether a paragraph of `properties` has a bottom border and no other.
fn is_rule(properties: Option<&ParagraphProperties>) -> bool {
    properties
        .and_then(|own| own.paragraph_borders.as_deref())
        .is_some_and(|borders| {
            borders
                .paragraph_border_below_identical_paragraphs
                .as_ref()
                .is_some_and(|bottom| {
                    bottom
                        .val
                        .as_ref()
                        .is_none_or(|style| !matches!(style.as_ref(), "nil" | "none"))
                })
                && borders.left_paragraph_border.is_none()
                && borders
                    .paragraph_border_above_identical_paragraphs
                    .is_none()
                && borders.paragraph_border_between_facing_pages.is_none()
                && borders
                    .paragraph_border_between_identical_paragraphs
                    .is_none()
                && borders.right_paragraph_border.is_none()
        })
}

/// Whether `element` is named `local` in the namespace the namespace table names `prefix`.
fn is_named(element: &Element, prefix: &str, local: &str) -> bool {
    element.name.local == local && table_prefix(&element.name.namespace) == Some(prefix)
}

/// Whether `property` is `w:i`, and turns italic on.
fn italic(property: &RunPropertiesContent) -> Option<bool> {
    if let RunPropertiesContent::Italics(italics) = property {
        Some(
            italics
                .val
                .as_ref()
                .is_none_or(|value| value.value().unwrap_or(true)),
        )
    } else {
        None
    }
}

/// The lines of `pieces`: each hard line break ends one, and the breaks that end the paragraph go.
fn lines<I>(pieces: I) -> Vec<Line>
where
    I: IntoIterator<Item = Piece>,
{
    let mut lines = Lines::default();
    for piece in pieces {
        match piece {
            Piece::Break => lines.line_break(),
            Piece::Text(marks, text) => lines.text(marks, &text),
        }
    }
    lines.finish()
}

/// The text of every `m:t` under `node`, in document order.
fn math_text(node: &Node, text: &mut String) {
    let Node::Element(element) = node else {
        return;
    };
    for child in &element.children {
        if let Node::Text(value) = child
            && is_named(element, "m", "t")
        {
            text.push_str(value);
        } else {
            math_text(child, text);
        }
    }
}

/// The level of an outline level `N - 1`, from 1 to 9; `None` for body text.
fn outline_level(level: &AssociatedOutlineLevel) -> Option<usize> {
    level
        .val
        .as_ref()
        .and_then(|value| value.value().ok())
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value < DEEPEST_WORD)
        .map(|value| value + 1)
}

/// The text of a code or quote paragraph: its pieces' text, a line ending at each hard line break.
fn plain(pieces: Vec<Piece>) -> String {
    let mut text = String::new();
    for piece in pieces {
        match piece {
            Piece::Break => text.push('\n'),
            Piece::Text(_, value) => text.push_str(&value),
        }
    }
    text
}

/// The prefix the namespace table gives `namespace`, when it knows it.
fn table_prefix(namespace: &Namespace) -> Option<&'static str> {
    if let Namespace::Known(id) = *namespace {
        NAMESPACES.get(usize::from(id)).map(|row| row.prefix)
    } else {
        None
    }
}

/// Every `w:txbxContent` under `node`, read; of an `mc:AlternateContent`, those of its first
/// branch that holds any.
fn text_boxes(node: &Node, boxes: &mut Vec<RichTextBoxContentContainer>) {
    let Node::Element(element) = node else {
        return;
    };
    if is_named(element, "w", "txbxContent") {
        boxes.push(officekit_markup::read(element));
    } else if mc::is_mc(&element.name.namespace) && element.name.local == "AlternateContent" {
        for branch in &element.children {
            let before = boxes.len();
            text_boxes(branch, boxes);
            if boxes.len() > before {
                break;
            }
        }
    } else {
        for child in &element.children {
            text_boxes(child, boxes);
        }
    }
}
