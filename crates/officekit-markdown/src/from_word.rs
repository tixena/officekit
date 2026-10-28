//! A Word body as an arky document: headings as sections, paragraphs and their runs, lists,
//! tables, links, pictures, code blocks, block quotes, thematic breaks, HTML imports, and the
//! paragraphs of text boxes.

#[cfg(test)]
mod tests;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::{iter, mem, ptr};

use arky::parser::ast::{
    Alignment, Args, Block, BlockQuote, Code, CodeBlock, Document as ArkyDocument, Emphasis,
    Heading, Html, Image, Inline, Item, Line, LineBreak, Link, LinkEmphasis, LinkForm, LinkInline,
    LinkStrong, List, Mapping, MarkdownBlock, Paragraph as ArkyParagraph, RawDocument, Section,
    Strong, Table as ArkyTable, TableCell as ArkyTableCell, TableFormat, TableRow as ArkyTableRow,
    Text, ThematicBreak,
};
use arky::writer::{self, WriteOptions};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use officekit::parts::{numbering_definitions, style_definitions};
use officekit_markup::wordprocessing::{
    AnchorForImportedExternalContent, AssociatedOutlineLevel, BlockContent, BreakTypesMember,
    ComplexFieldCharacter, ComplexFieldCharacterType, ComplexFieldCharacterTypeMember,
    ContentBlockContent, ContentCellContent, ContentRowContent, DecimalNumberValue, Document,
    HorizontalAlignmentType, HorizontalAlignmentTypeMember, Hyperlink, MergedCellType,
    MergedCellTypeMember, NumberingDefinitionInstanceReferenceNumPr, NumberingDefinitions,
    NumberingLevelDefinitionOverride, NumberingLevelOverrideDefinition, Paragraph,
    ParagraphContent, ParagraphIndentation, ParagraphProperties, RichTextBoxContentContainer,
    RunProperties, RunPropertiesContent, SignedMeasurementInTwentiethsOfPointMember,
    StNumberFormat, StNumberFormatMember, StyleDefinition, StyleTypesMember, Table, TableCell,
    TableRow, TableRowPropertiesContent, TextRun, TextRunContent, TrackedRunContent,
};
use officekit_xml::element::{Element, Node};
use officekit_xml::mc;
use officekit_xml::name::Namespace;
use officekit_xml::name::table::NAMESPACES;
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::{RelatedPart, RelationshipType, Target};

use super::Exported;
use super::parts::{self, CODE, DEEPEST_LEVEL, LIST_PARAGRAPH, PREFORMATTED, QUOTE};

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

/// The content type of an HTML part a `w:altChunk` names.
const HTML_CONTENT_TYPE: &str = "text/html";

/// The name of the paragraph style of the later blocks of a list item.
const LIST_PARAGRAPH_NAME: &str = "List Paragraph";

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
#[derive(Clone, Copy)]
enum Kind {
    Code,
    Heading(usize),
    Plain,
    Quote,
}

/// What the numbering of a paragraph makes it.
enum Numbered {
    /// A level whose `w:numFmt` is `none`: a plain paragraph.
    Hidden,
    /// An item of the list of numbering instance `instance` at level `level`, ordered from `start`
    /// or bullets when `start` is `None`.
    Item {
        instance: i64,
        level: usize,
        start: Option<u64>,
    },
    /// No numbering, or numbering that names no instance or level.
    Unnumbered,
}

/// What a paragraph's runs give, in order.
enum Piece {
    Break,
    Image(Option<usize>, Image),
    Text(Marks, String),
}

/// A picture a `w:drawing` or a VML `v:shape` holds: the relationship ids its `a:blip` or
/// `v:imagedata` names, embedded or linked, and of the SVG part its extension names; its alt text
/// and its title.
struct Drawing {
    alt: String,
    embed: Option<String>,
    link: Option<String>,
    svg: Option<String>,
    title: Option<String>,
}

/// A complex field open: its instruction so far, whether its result has started, and the link
/// its result is when its instruction is `HYPERLINK`.
struct Field {
    instruction: String,
    link: Option<usize>,
    result: bool,
}

/// The lines of a paragraph as they are built: the stretch of text of the same marks still to
/// write, the link it goes in, the line they go on, and the lines before.
struct Lines<'links> {
    done: Vec<Line>,
    line: Vec<Inline>,
    /// The link open on the line: its index in `links`, and its text so far.
    link: Option<(usize, Vec<LinkInline>)>,
    links: &'links Links,
    marks: Marks,
    stretch: String,
}

/// The target and the title of every link met, each once.
#[derive(Default)]
struct Links {
    targets: Vec<(String, Option<String>)>,
}

/// Whether a stretch of text is bold, code, or italic, and the link it is the text of, by its
/// index in [`Links`].
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Marks {
    bold: bool,
    code: bool,
    italic: bool,
    link: Option<usize>,
}

/// The numbering part's instances and abstract numberings.
#[derive(Default)]
pub struct Numbering {
    root: NumberingDefinitions,
}

/// A list still open: its numbering instance and level, its first number when ordered, and the
/// blocks of each of its items.
struct OpenList {
    instance: i64,
    items: Vec<Vec<Block>>,
    level: usize,
    start: Option<u64>,
}

/// The blocks of one stretch of markdown, the body or a table cell's, as they are built: the
/// paragraphs gathered into a code block or a block quote and the list they go into, by its
/// index in `lists`, the lists open, and the sections.
#[derive(Default)]
struct Output {
    /// The lists open close before the next block written outside them; an item of the
    /// numbering instance of an open list comes first keeps them open.
    closing: bool,
    gathered: Vec<String>,
    gathering: Option<(Gathering, Option<usize>)>,
    lists: Vec<OpenList>,
    sections: Sections,
}

/// The parts the main part relates to, by relationship id, and the pictures exported so far.
pub struct Parts<'package> {
    /// Write each picture as a data URI.
    embed: bool,
    /// The folded names of the image parts in `images`.
    exported: BTreeSet<String>,
    images: Vec<(String, Vec<u8>)>,
    media: &'package str,
    package: &'package Package,
    related: BTreeMap<String, RelatedPart>,
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
struct Walk<'walk> {
    /// Each complex field open, innermost last.
    fields: Vec<Field>,
    /// The link of the `w:hyperlink` or `HYPERLINK` simple field being read, and how many complex
    /// fields were open when it started.
    hyperlink: Option<(usize, usize)>,
    links: Links,
    numbering: &'walk Numbering,
    /// The last number each level of each numbering instance showed, by instance and level.
    numbers: BTreeMap<(i64, usize), u64>,
    output: Output,
    parts: Parts<'walk>,
    styles: &'walk Styles,
}

impl<'links> Lines<'links> {
    /// The open link, as a link node at the end of the line.
    fn close_link(&mut self) {
        if let Some((index, children)) = self.link.take() {
            self.line.extend(self.links.node(index, children));
        }
    }

    /// The lines, without the hard line breaks that end the paragraph.
    fn finish(mut self) -> Vec<Line> {
        self.flush();
        self.close_link();
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

    /// The stretch as its node, inside its link: bold as strong, italic as emphasis holding it,
    /// code inside both; the spaces at the ends of a bold or italic stretch go outside it.
    fn flush(&mut self) {
        let text = mem::take(&mut self.stretch);
        if text.is_empty() {
            return;
        }
        let marks = self.marks;
        self.open(marks.link);
        if !marks.bold && !marks.italic {
            if marks.code {
                self.push_marked(marks, &text);
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
        self.push_marked(marks, core);
        self.push_text(&text[end..]);
    }

    /// `image`, inside `link`.
    fn image(&mut self, link: Option<usize>, image: Image) {
        self.flush();
        self.open(link);
        if let Some((_, children)) = &mut self.link {
            children.push(LinkInline::Image(image));
        } else {
            self.line.push(Inline::Image(image));
        }
    }

    /// End the line with a hard line break, outside any link; the text after it starts the next
    /// line.
    fn line_break(&mut self) {
        self.flush();
        self.close_link();
        self.line.push(Inline::LineBreak(LineBreak::default()));
        self.done.push(Line::new(mem::take(&mut self.line)));
    }

    const fn new(links: &'links Links) -> Self {
        Self {
            done: Vec::new(),
            line: Vec::new(),
            link: None,
            links,
            marks: Marks {
                bold: false,
                code: false,
                italic: false,
                link: None,
            },
            stretch: String::new(),
        }
    }

    /// Make `link` the open link, closing the one open when it is another.
    fn open(&mut self, link: Option<usize>) {
        if self.link.as_ref().map(|(index, _)| *index) == link {
            return;
        }
        self.close_link();
        self.link = link.map(|index| (index, Vec::new()));
    }

    /// `core`, bold, code or italic as `marks` say, inside the open link if any.
    fn push_marked(&mut self, marks: Marks, core: &str) {
        if let Some((_, children)) = &mut self.link {
            let mut node = if marks.code {
                LinkInline::Code(Code::new(core.to_owned()))
            } else {
                LinkInline::Text(Text::new(core.to_owned()))
            };
            if marks.bold {
                node = LinkInline::Strong(LinkStrong::new(alloc::vec![node]));
            }
            if marks.italic {
                node = LinkInline::Emphasis(LinkEmphasis::new(alloc::vec![node]));
            }
            children.push(node);
            return;
        }
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
        self.line.push(node);
    }

    /// Plain `text`, joined to the plain text before it, inside the open link if any.
    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some((_, children)) = &mut self.link {
            if let Some(LinkInline::Text(last)) = children.last_mut() {
                last.value.push_str(text);
            } else {
                children.push(LinkInline::Text(Text::new(text.to_owned())));
            }
            return;
        }
        if let Some(Inline::Text(last)) = self.line.last_mut() {
            last.value.push_str(text);
        } else {
            self.line.push(Inline::Text(Text::new(text.to_owned())));
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

impl Links {
    /// The index of the link to `target` titled `title`, added when it is new.
    fn index(&mut self, target: String, title: Option<String>) -> usize {
        if let Some(found) = self
            .targets
            .iter()
            .position(|(known, known_title)| *known == target && *known_title == title)
        {
            return found;
        }
        self.targets.push((target, title));
        self.targets.len() - 1
    }

    /// The node of link `index`, whose text is `children`.
    fn node(&self, index: usize, children: Vec<LinkInline>) -> Option<Inline> {
        self.targets.get(index).map(|(target, title)| {
            Inline::Link(Link::new(
                LinkForm::Markdown,
                target.clone(),
                title.clone(),
                children,
            ))
        })
    }
}

impl Numbering {
    /// The left indent, in twentieths of a point, of the text of level `level` of numbering
    /// instance `instance`: the `w:ind` of the level's `w:pPr`.
    fn indent(&self, instance: i64, level: usize) -> Option<i64> {
        let (_, definition) = self.level(instance, level)?;
        let properties = definition.paragraph_properties_p_pr_general.as_deref()?;
        left_indent(properties.paragraph_indentation.as_deref()?)
    }

    /// Level `level` of numbering instance `instance`: the instance's `w:lvlOverride` of it, if
    /// any, and the level that override gives, else its abstract numbering's.
    fn level(
        &self,
        instance: i64,
        level: usize,
    ) -> Option<(
        Option<&NumberingLevelDefinitionOverride>,
        &NumberingLevelOverrideDefinition,
    )> {
        let definition = self
            .root
            .numbering_definition_instance
            .iter()
            .find(|candidate| number(candidate.num_id.as_ref()) == Some(instance))?;
        let level_override = definition
            .numbering_level_definition_override
            .iter()
            .find(|candidate| level_number(candidate.ilvl.as_ref()) == Some(level));
        let abstract_id = number(
            definition
                .abstract_numbering_definition_reference
                .val
                .as_ref(),
        );
        let level_definition = level_override
            .and_then(|found| found.numbering_level_override_definition.as_deref())
            .or_else(|| {
                self.root
                    .abstract_numbering_definition
                    .iter()
                    .find(|candidate| {
                        abstract_id.is_some()
                            && number(candidate.abstract_num_id.as_ref()) == abstract_id
                    })?
                    .numbering_level_override_definition
                    .iter()
                    .find(|candidate| level_number(candidate.ilvl.as_ref()) == Some(level))
            })?;
        Some((level_override, level_definition))
    }

    /// What level `level` of numbering instance `instance` makes a paragraph: the level the
    /// instance's `w:lvlOverride` gives, else its abstract numbering's; a bullet item, a hidden
    /// number, or an ordered item from the override's `w:startOverride` or the level's `w:start`.
    fn of(&self, instance: i64, level: usize) -> Numbered {
        let Some((level_override, level_definition)) = self.level(instance, level) else {
            return Numbered::Unnumbered;
        };
        let format = level_definition
            .numbering_format
            .as_ref()
            .and_then(|format| format.val.as_ref())
            .map(StNumberFormat::value)
            .and_then(Result::ok);
        match format {
            Some(StNumberFormatMember::Bullet) => Numbered::Item {
                instance,
                level,
                start: None,
            },
            Some(StNumberFormatMember::None) => Numbered::Hidden,
            Some(_) | None => {
                let first = level_override
                    .and_then(|found| found.numbering_level_starting_value_override.as_ref())
                    .and_then(|start| number(start.val.as_ref()))
                    .or_else(|| {
                        let start = level_definition.starting_value.as_ref()?;
                        number(start.val.as_ref())
                    });
                Numbered::Item {
                    instance,
                    level,
                    start: Some(
                        first
                            .and_then(|value| u64::try_from(value).ok())
                            .unwrap_or(0),
                    ),
                }
            }
        }
    }

    /// The numbering part `main` relates to, read with `options`; none when it relates to none.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::related_by_type`] and [`numbering_definitions::load`].
    pub fn read(
        package: &Package,
        main: &PartName,
        options: OpenOptions,
    ) -> Result<Self, PackageError> {
        let Some(name) = related_part(package, main, &numbering_definitions::RELATIONSHIP_TYPE)?
        else {
            return Ok(Self::default());
        };
        Ok(Self {
            root: numbering_definitions::load(package, &name, options)?,
        })
    }
}

impl OpenList {
    /// The list, each item one section with no heading holding its blocks, or no section when it
    /// holds none. It is tight unless an item holds two paragraphs one after the other, which
    /// markdown has no tight form for.
    fn into_block(self) -> Block {
        let loose = self.items.iter().any(|blocks| {
            blocks
                .windows(2)
                .any(|pair| matches!(pair, [Block::Paragraph(_), Block::Paragraph(_)]))
        });
        let items = self
            .items
            .into_iter()
            .map(|blocks| {
                let sections = if blocks.is_empty() {
                    Vec::new()
                } else {
                    alloc::vec![Section::new(None, blocks, Vec::new())]
                };
                Item::new(MarkdownBlock::new(sections))
            })
            .collect();
        Block::List(List::new(items, loose, self.start))
    }
}

impl Output {
    /// A block outside every list: the lists open close first.
    fn block(&mut self, block: Block) {
        self.flush();
        self.close(0);
        self.sections.block(block);
    }

    /// Close the lists open past the first `keep`, each into the last item of the list before it,
    /// or into the sections.
    fn close(&mut self, keep: usize) {
        if self.lists.len() > keep {
            self.flush();
        }
        if keep == 0 {
            self.closing = false;
        }
        while self.lists.len() > keep
            && let Some(list) = self.lists.pop()
        {
            self.put(list.into_block(), self.lists.len().checked_sub(1));
        }
    }

    /// The blocks, every list closed.
    fn finish(mut self) -> MarkdownBlock {
        self.flush();
        self.close(0);
        MarkdownBlock::new(self.sections.finish())
    }

    /// The code block or block quote of the paragraphs gathered, if any, where they go.
    fn flush(&mut self) {
        let Some((gathering, place)) = self.gathering.take() else {
            return;
        };
        let value = mem::take(&mut self.gathered).join("\n");
        let block = match gathering {
            Gathering::Code => {
                Block::CodeBlock(CodeBlock::new(None, Args::Toml(Mapping::new()), value))
            }
            Gathering::Quote => Block::BlockQuote(BlockQuote::new(value)),
        };
        self.put(block, place);
    }

    /// `line`, the text of a code or quote paragraph going into the list at `place`, or the
    /// sections when `None`, after the lines of the paragraphs of its kind and place right before
    /// it.
    fn gather(&mut self, gathering: Gathering, place: Option<usize>, line: String) {
        if self.gathering != Some((gathering, place)) {
            self.flush();
            self.gathering = Some((gathering, place));
        }
        self.gathered.push(line);
    }

    /// A new section of `heading` at `depth`, outside every list.
    fn heading(&mut self, depth: usize, heading: Heading) {
        self.flush();
        self.close(0);
        self.sections.heading(depth, heading);
    }

    /// A new item of the list of `instance` at `level`, ordered from `start` or bullets: in the
    /// list open at that level when it is of the same instance, else in a new list, nested in the
    /// item before when that is at a shallower level. Lists that are closing stay open when one
    /// of them is of `instance`. Returns the list's index.
    fn item(&mut self, instance: i64, level: usize, start: Option<u64>) -> usize {
        self.flush();
        if self.closing && !self.lists.iter().any(|list| list.instance == instance) {
            self.close(0);
        }
        self.closing = false;
        let deeper = self
            .lists
            .iter()
            .position(|list| list.level > level)
            .unwrap_or(self.lists.len());
        self.close(deeper);
        if let Some(last) = self.lists.last_mut()
            && last.level == level
            && last.instance == instance
        {
            last.items.push(Vec::new());
        } else {
            if self.lists.last().is_some_and(|last| last.level == level) {
                self.close(self.lists.len() - 1);
            }
            self.lists.push(OpenList {
                instance,
                items: alloc::vec![Vec::new()],
                level,
                start,
            });
        }
        self.lists.len() - 1
    }

    /// The list whose last item a later block goes into: the one open at `level` when there is
    /// one, else the innermost; the lists inside it close.
    fn later(&mut self, level: Option<usize>) -> usize {
        self.closing = false;
        let index = level
            .and_then(|wanted| self.lists.iter().position(|list| list.level == wanted))
            .unwrap_or_else(|| self.lists.len().saturating_sub(1));
        self.close(index + 1);
        index
    }

    /// `block`, last in the last item of the list at `place`, or in the sections after the lists
    /// that are closing.
    fn put(&mut self, block: Block, place: Option<usize>) {
        if let Some(blocks) = place
            .and_then(|index| self.lists.get_mut(index))
            .and_then(|list| list.items.last_mut())
        {
            blocks.push(block);
            return;
        }
        if self.closing {
            self.close(0);
        }
        self.sections.block(block);
    }
}

impl<'package> Parts<'package> {
    /// The address relationship `id` names when its target is outside the package, or the name
    /// of the part it names.
    fn external(&self, id: &str) -> Option<String> {
        let related = self.related.get(id)?;
        match &related.target {
            Target::External(address) => Some(address.clone()),
            Target::Part(name) => Some(name.as_str().to_owned()),
            _ => None,
        }
    }

    /// The HTML between `<body>` and `</body>` of the HTML part relationship `id` names; `None`
    /// when it names no HTML part or the body is empty.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::part_bytes`].
    fn html(&self, id: &str) -> Result<Option<String>, PackageError> {
        let Some((name, content_type)) = self.part(id) else {
            return Ok(None);
        };
        if !content_type.eq_ignore_ascii_case(HTML_CONTENT_TYPE) {
            return Ok(None);
        }
        let bytes = self.package.part_bytes(name)?;
        let page = String::from_utf8_lossy(&bytes);
        let body = html_body(&page);
        Ok((!body.trim().is_empty()).then(|| body.to_owned()))
    }

    /// The part relationship `id` names and its content type, when the package holds it.
    fn part(&self, id: &str) -> Option<(&PartName, &str)> {
        let related = self.related.get(id)?;
        if let Target::Part(name) = &related.target {
            self.package
                .content_type(name)
                .map(|content_type| (name, content_type))
        } else {
            None
        }
    }

    /// The target of the picture `drawing` names: `{media}/{file name}` of its SVG part, else of
    /// its embedded part, or that part as a data URI when the pictures are embedded; else the
    /// address of its linked picture. `None` when it names none the package holds.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::part_bytes`].
    fn picture(&mut self, drawing: Drawing) -> Result<Option<Image>, PackageError> {
        let part = drawing
            .svg
            .as_deref()
            .and_then(|id| self.part(id))
            .or_else(|| {
                let id = drawing.embed.as_deref()?;
                self.part(id)
            })
            .map(|(name, content_type)| (name.clone(), content_type.to_owned()));
        let target = if let Some((name, content_type)) = part {
            self.target_of(&name, &content_type)?
        } else if let Some(address) = drawing.link.as_deref().and_then(|id| self.external(id)) {
            address
        } else {
            return Ok(None);
        };
        Ok(Some(Image::new(
            LinkForm::Markdown,
            target,
            drawing.title,
            drawing.alt,
        )))
    }

    /// The parts `main` relates to in `package`; each picture's target is in the folder `media`,
    /// or a data URI when `embed` is set.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::related`].
    pub fn read(
        package: &'package Package,
        main: &PartName,
        media: &'package str,
        embed: bool,
    ) -> Result<Self, PackageError> {
        let related = package
            .related(Some(main))?
            .into_iter()
            .map(|part| (part.id.clone(), part))
            .collect();
        Ok(Self {
            embed,
            exported: BTreeSet::new(),
            images: Vec::new(),
            media,
            package,
            related,
        })
    }

    /// The target of the image part `name`: a data URI of `content_type` when the pictures are
    /// embedded, else `{media}/{file name}`, its bytes exported the first time.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::part_bytes`].
    fn target_of(&mut self, name: &PartName, content_type: &str) -> Result<String, PackageError> {
        if self.embed {
            let bytes = self.package.part_bytes(name)?;
            return Ok(format!(
                "data:{content_type};base64,{}",
                STANDARD.encode(bytes)
            ));
        }
        let file = name
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        if self.exported.insert(name.folded()) {
            let bytes = self.package.part_bytes(name)?;
            self.images.push((file.clone(), bytes));
        }
        Ok(format!("{}/{file}", self.media))
    }
}

impl Runs {
    /// What a piece of a shown run gives: text, a tab, or a hard line break.
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
            | TextRunContent::DrawingMLObject(_)
            | TextRunContent::EmbeddedObject(_)
            | TextRunContent::EndnoteReference(_)
            | TextRunContent::EndnoteReferenceMark(_)
            | TextRunContent::FieldCode(_)
            | TextRunContent::FootnoteEndnoteSeparatorMark(_)
            | TextRunContent::FootnoteReference(_)
            | TextRunContent::FootnoteReferenceMark(_)
            | TextRunContent::OptionalHyphenCharacter(_)
            | TextRunContent::Other(_)
            | TextRunContent::PageNumberBlock(_)
            | TextRunContent::PhoneticGuide(_)
            | TextRunContent::PositionOfLastCalculatedPageBreak(_)
            | TextRunContent::SymbolCharacter(_)
            | TextRunContent::VMLObject(_)
            | _ => {}
        }
    }

    /// The text of an equation, plain.
    fn math(&mut self, item: &Node, link: Option<usize>) {
        let mut text = String::new();
        math_text(item, &mut text);
        self.text(
            Marks {
                link,
                ..Marks::default()
            },
            &text,
        );
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

    /// Whether a paragraph of `properties` is styled `ListParagraph`.
    fn is_list_paragraph(&self, properties: Option<&ParagraphProperties>) -> bool {
        self.is(
            self.paragraph_style(properties),
            LIST_PARAGRAPH,
            LIST_PARAGRAPH_NAME,
        )
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
            link: None,
        }
    }

    /// The numbering instance and level of a paragraph of `properties`: its own `w:numPr`, else
    /// the first its style's chain gives; the level is 0 when neither names one.
    fn numbering(&self, properties: Option<&ParagraphProperties>) -> Option<(i64, usize)> {
        let own = properties.and_then(|paragraph| {
            paragraph
                .numbering_definition_instance_reference_num_pr
                .as_deref()
        });
        let styled = self
            .chain(self.paragraph_style(properties))
            .into_iter()
            .find_map(|style| {
                let general = style.paragraph_properties_p_pr_general.as_ref()?;
                general
                    .numbering_definition_instance_reference_num_pr
                    .as_deref()
            });
        let instance = own
            .and_then(numbering_instance)
            .or_else(|| styled.and_then(numbering_instance))?;
        let level = own
            .and_then(numbering_level)
            .or_else(|| styled.and_then(numbering_level))
            .unwrap_or(0);
        Some((instance, level))
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
        let Some(name) = related_part(package, main, &style_definitions::RELATIONSHIP_TYPE)? else {
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
    fn blocks(&mut self, blocks: &[BlockContent]) -> Result<(), PackageError> {
        for block in blocks {
            match block {
                BlockContent::AnchorForImportedExternalContent(chunk) => self.html(chunk)?,
                BlockContent::BlockLevelCustomXMLElement(element) => {
                    self.content_blocks(&element.content)?;
                }
                BlockContent::BlockLevelStructuredDocumentTag(tag) => {
                    if let Some(inner) = &tag.content {
                        self.content_blocks(&inner.content)?;
                    }
                }
                BlockContent::Paragraph(paragraph) => self.paragraph(paragraph)?,
                BlockContent::Table(table) => self.table(table)?,
                BlockContent::BookmarkEnd(_)
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
        Ok(())
    }

    /// A table cell's blocks, converted by these same rules into a markdown block of their own.
    fn cell(&mut self, cell: &TableCell) -> Result<ArkyTableCell, PackageError> {
        let outer = mem::take(&mut self.output);
        let walked = self.blocks(&cell.content);
        let inner = mem::replace(&mut self.output, outer);
        walked?;
        Ok(ArkyTableCell::new(inner.finish()))
    }

    /// The paragraph's pieces of `kind`, as the block they give at `place`: the list whose last
    /// item they go into, or the sections when `None`.
    fn content(
        &mut self,
        kind: Kind,
        pieces: Vec<Piece>,
        properties: Option<&ParagraphProperties>,
        place: Option<usize>,
    ) {
        let has_text = has_text(&pieces);
        match kind {
            Kind::Code => self.output.gather(Gathering::Code, place, plain(pieces)),
            Kind::Heading(level) if level <= DEEPEST => {
                self.output.flush();
                if has_text {
                    let heading = Heading::new(heading_inlines(pieces, &self.links));
                    self.output.heading(level, heading);
                }
            }
            Kind::Heading(_) => {
                self.output.flush();
                if has_text {
                    let bold = pieces.into_iter().map(|piece| match piece {
                        Piece::Text(marks, text) => Piece::Text(
                            Marks {
                                bold: true,
                                ..marks
                            },
                            text,
                        ),
                        Piece::Break | Piece::Image(..) => piece,
                    });
                    let paragraph = ArkyParagraph::new(lines(bold, &self.links));
                    self.output.put(Block::Paragraph(paragraph), place);
                }
            }
            Kind::Plain => {
                self.output.flush();
                let block = if has_text || has_picture(&pieces) {
                    Some(Block::Paragraph(ArkyParagraph::new(lines(
                        pieces,
                        &self.links,
                    ))))
                } else {
                    is_rule(properties).then(|| Block::ThematicBreak(ThematicBreak::default()))
                };
                if let Some(rule_or_paragraph) = block {
                    self.output.put(rule_or_paragraph, place);
                }
            }
            Kind::Quote => self.output.gather(Gathering::Quote, place, plain(pieces)),
        }
    }

    /// The blocks of a content control or a custom XML element.
    fn content_blocks(&mut self, blocks: &[ContentBlockContent]) -> Result<(), PackageError> {
        for block in blocks {
            match block {
                ContentBlockContent::BlockLevelCustomXMLElement(element) => {
                    self.content_blocks(&element.content)?;
                }
                ContentBlockContent::BlockLevelStructuredDocumentTag(tag) => {
                    if let Some(inner) = &tag.content {
                        self.content_blocks(&inner.content)?;
                    }
                }
                ContentBlockContent::Paragraph(paragraph) => self.paragraph(paragraph)?,
                ContentBlockContent::Table(table) => self.table(table)?,
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
        Ok(())
    }

    /// The pictures and the text boxes of `node`, an object a run holds, the pictures inside
    /// `link`.
    fn embedded(
        &mut self,
        node: &Node,
        link: Option<usize>,
        runs: &mut Runs,
    ) -> Result<(), PackageError> {
        text_boxes(node, &mut runs.boxes);
        let mut found = Vec::new();
        drawings(node, &mut found);
        for drawing in found {
            if let Some(image) = self.parts.picture(drawing)? {
                runs.pieces.push(Piece::Image(link, image));
            }
        }
        Ok(())
    }

    /// A `w:fldChar`: `begin` opens a field, `separate` starts its result, a link when its
    /// instruction is `HYPERLINK`, `end` closes it.
    fn field_character(&mut self, character: &ComplexFieldCharacter) {
        let Some(Ok(kind)) = character
            .fld_char_type
            .as_ref()
            .map(ComplexFieldCharacterType::value)
        else {
            return;
        };
        match kind {
            ComplexFieldCharacterTypeMember::Begin => self.fields.push(Field {
                instruction: String::new(),
                link: None,
                result: false,
            }),
            ComplexFieldCharacterTypeMember::End => {
                self.fields.pop();
            }
            ComplexFieldCharacterTypeMember::Separate => {
                let target = self
                    .fields
                    .last()
                    .and_then(|field| hyperlink_target(&field.instruction));
                let link = target.map(|name| self.links.index(name, None));
                if let Some(field) = self.fields.last_mut() {
                    field.link = link;
                    field.result = true;
                }
            }
            _ => {}
        }
    }

    /// The HTML block of a `w:altChunk` naming an HTML part.
    fn html(&mut self, chunk: &AnchorForImportedExternalContent) -> Result<(), PackageError> {
        if let Some(id) = &chunk.id
            && let Some(html) = self.parts.html(id)?
        {
            self.output.block(Block::Html(Html::new(html)));
        }
        Ok(())
    }

    /// The link of a `w:hyperlink`: its relationship's target, else `#` and its anchor; its
    /// tooltip as the title.
    fn hyperlink(&mut self, hyperlink: &Hyperlink) -> Option<usize> {
        let target = hyperlink
            .id
            .as_deref()
            .and_then(|id| self.parts.external(id))
            .or_else(|| hyperlink.anchor.as_deref().map(anchor_target))?;
        let title = hyperlink
            .tooltip
            .clone()
            .filter(|tooltip| !tooltip.is_empty());
        Some(self.links.index(target, title))
    }

    /// The text of a complex field's instruction, while it is read.
    fn instruction(&mut self, text: &str) {
        if let Some(field) = self.fields.last_mut()
            && !field.result
        {
            field.instruction.push_str(text);
        }
    }

    /// The link the text read now is in: the outermost of the `w:hyperlink` and the `HYPERLINK`
    /// fields open.
    fn link(&self) -> Option<usize> {
        let open = self.hyperlink.map_or(self.fields.len(), |(_, fields)| {
            fields.min(self.fields.len())
        });
        let (outer, inner) = self.fields.split_at(open);
        outer
            .iter()
            .find_map(|field| field.link)
            .or_else(|| self.hyperlink.map(|(link, _)| link))
            .or_else(|| inner.iter().find_map(|field| field.link))
    }

    /// `content`, the text of `link` unless a link is already being read, whose text it then
    /// joins.
    fn linked(
        &mut self,
        link: Option<usize>,
        content: &[ParagraphContent],
        runs: &mut Runs,
    ) -> Result<(), PackageError> {
        let outer = self.hyperlink;
        if outer.is_none()
            && let Some(index) = link
        {
            self.hyperlink = Some((index, self.fields.len()));
        }
        let walked = self.paragraph_content(content, runs);
        self.hyperlink = outer;
        walked
    }

    /// The number an item of `instance` at `level` shows: one past the last item of that instance
    /// and level, wherever it was, else `start`; none for bullets. The instance's deeper levels
    /// start again.
    fn number(&mut self, instance: i64, level: usize, start: Option<u64>) -> Option<u64> {
        self.numbers
            .retain(|&(other, deeper), _| other != instance || deeper <= level);
        let first = start?;
        let shown = self
            .numbers
            .get(&(instance, level))
            .map_or(first, |last| last.saturating_add(1));
        self.numbers.insert((instance, level), shown);
        Some(shown)
    }

    /// A paragraph as the node its kind gives: a list item, a later block of the list item before
    /// it, or a block of its own; then the paragraphs of its text boxes.
    fn paragraph(&mut self, paragraph: &Paragraph) -> Result<(), PackageError> {
        let mut runs = Runs::default();
        self.paragraph_content(&paragraph.content, &mut runs)?;
        let Runs { boxes, pieces } = runs;
        let properties = paragraph.properties.as_deref();
        let kind = self.styles.kind(properties);
        let numbered = if matches!(kind, Kind::Heading(_)) {
            Numbered::Unnumbered
        } else {
            self.styles
                .numbering(properties)
                .map_or(Numbered::Unnumbered, |(instance, level)| {
                    self.numbering.of(instance, level)
                })
        };
        match numbered {
            Numbered::Hidden => {
                self.output.closing = true;
                self.content(kind, pieces, properties, None);
            }
            Numbered::Item {
                instance,
                level,
                start,
            } => {
                let shown = self.number(instance, level, start);
                let index = self.output.item(instance, level, shown);
                if has_text(&pieces) || has_picture(&pieces) {
                    let opening = ArkyParagraph::new(lines(pieces, &self.links));
                    self.output.put(Block::Paragraph(opening), Some(index));
                }
            }
            Numbered::Unnumbered => {
                let shows = has_text(&pieces) || has_picture(&pieces);
                let place = self.place(kind, shows, properties);
                self.content(kind, pieces, properties, place);
            }
        }
        for text_box in boxes {
            self.blocks(&text_box.content)?;
        }
        Ok(())
    }

    fn paragraph_content(
        &mut self,
        content: &[ParagraphContent],
        runs: &mut Runs,
    ) -> Result<(), PackageError> {
        for item in content {
            match item {
                ParagraphContent::BidirectionalEmbeddingLevel(level) => {
                    self.paragraph_content(&level.content, runs)?;
                }
                ParagraphContent::BidirectionalOverride(direction) => {
                    self.paragraph_content(&direction.content, runs)?;
                }
                ParagraphContent::Hyperlink(hyperlink) => {
                    let link = self.hyperlink(hyperlink);
                    self.linked(link, &hyperlink.content, runs)?;
                }
                ParagraphContent::InlineLevelCustomXMLElement(element) => {
                    self.paragraph_content(&element.content, runs)?;
                }
                ParagraphContent::InlineLevelSmartTag(tag) => {
                    self.paragraph_content(&tag.content, runs)?;
                }
                ParagraphContent::InlineLevelStructuredDocumentTag(tag) => {
                    if let Some(inner) = &tag.content {
                        self.paragraph_content(&inner.content, runs)?;
                    }
                }
                ParagraphContent::InsertedRunContent(insertion) => {
                    self.tracked(&insertion.content, runs)?;
                }
                ParagraphContent::MoveDestinationRunContent(destination) => {
                    self.tracked(&destination.content, runs)?;
                }
                ParagraphContent::OfficeMath(_) | ParagraphContent::OfficeMathParagraph(_) => {
                    if self.shown() {
                        runs.math(&officekit_markup::generic(item), self.link());
                    }
                }
                ParagraphContent::SimpleField(field) => {
                    let target = field.instr.as_deref().and_then(hyperlink_target);
                    let link = target.map(|name| self.links.index(name, None));
                    self.linked(link, &field.content, runs)?;
                }
                ParagraphContent::TextRun(run) => self.run(run, runs)?,
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
        Ok(())
    }

    /// Where a paragraph of `kind` and `properties` with no numbering goes while a list is open:
    /// the list whose last item it is a later block of, a plain paragraph indented to where the
    /// text of an open list's items starts, a `ListParagraph` paragraph, or a code, quote or rule
    /// paragraph indented to a list level; else it goes into the sections, and the lists close
    /// before it when it writes a block.
    fn place(
        &mut self,
        kind: Kind,
        shows: bool,
        properties: Option<&ParagraphProperties>,
    ) -> Option<usize> {
        if self.output.lists.is_empty() {
            return None;
        }
        let level = indent_level(properties);
        let later = match kind {
            Kind::Code | Kind::Quote => level.is_some().then_some(level),
            Kind::Heading(_) => None,
            Kind::Plain => self.under(properties).map(Some).or_else(|| {
                (self.styles.is_list_paragraph(properties)
                    || (level.is_some() && !shows && is_rule(properties)))
                .then_some(level)
            }),
        };
        let Some(wanted) = later else {
            self.output.closing = true;
            return None;
        };
        Some(self.output.later(wanted))
    }

    /// A run's pieces, of the run's marks inside the link open, the pictures and text boxes of
    /// the objects it holds, and of the nodes the schemas do not declare in it, such as
    /// `mc:AlternateContent`; a field's instruction, and a result its field does not show, give
    /// none.
    fn run(&mut self, run: &TextRun, runs: &mut Runs) -> Result<(), PackageError> {
        let own = self.styles.marks(run.properties.as_deref());
        for item in &run.content {
            if let TextRunContent::ComplexFieldCharacter(character) = item {
                self.field_character(character);
                continue;
            }
            if let TextRunContent::FieldCode(code) = item {
                self.instruction(&code.value);
                continue;
            }
            if !self.shown() {
                continue;
            }
            let marks = Marks {
                link: self.link(),
                ..own
            };
            if let TextRunContent::DrawingMLObject(_)
            | TextRunContent::EmbeddedObject(_)
            | TextRunContent::Other(_)
            | TextRunContent::VMLObject(_) = item
            {
                self.embedded(&officekit_markup::generic(item), marks.link, runs)?;
            } else {
                runs.add(item, marks);
            }
        }
        if self.shown() {
            let link = self.link();
            for extra in &run.unknown.children {
                self.embedded(&extra.node, link, runs)?;
            }
        }
        Ok(())
    }

    /// Whether the text read now shows: every field open is in its result.
    fn shown(&self) -> bool {
        self.fields.iter().all(|field| field.result)
    }

    /// A table: its rows marked `w:tblHeader` as head rows, each column aligned by its first
    /// row's cell, every row one cell per column, in pipe form when it has one head row and arky
    /// writes every cell on one line, in grid form otherwise.
    fn table(&mut self, table: &Table) -> Result<(), PackageError> {
        let mut rows = Vec::new();
        table_rows(&table.content_row_content, &mut rows);
        let mut alignments = Vec::new();
        let mut head = Vec::new();
        let mut body = Vec::new();
        for (index, row) in rows.into_iter().enumerate() {
            let cells = self.table_row(row, (index == 0).then_some(&mut alignments))?;
            if is_head_row(row) {
                head.push(cells);
            } else {
                body.push(cells);
            }
        }
        let width = head.iter().chain(&body).map(Vec::len).max().unwrap_or(0);
        if width == 0 {
            return Ok(());
        }
        alignments.resize(width, Alignment::Default);
        let one_line = head
            .iter()
            .chain(&body)
            .flatten()
            .all(|cell| is_one_line(&cell.body));
        let format = if head.len() == 1 && one_line {
            TableFormat::Pipe
        } else {
            TableFormat::Grid
        };
        let padded = |mut cells: Vec<ArkyTableCell>| {
            cells.resize_with(width, empty_cell);
            ArkyTableRow::new(cells)
        };
        let converted = ArkyTable::new(
            format,
            alignments,
            head.into_iter().map(padded).collect(),
            body.into_iter().map(padded).collect(),
        );
        self.output.block(Block::Table(converted));
        Ok(())
    }

    /// The cells of a row: a cell spanning `n` columns is its cell and `n - 1` empty ones, a cell
    /// continuing a vertical merge is empty. The alignment of each column it covers goes into
    /// `alignments`, when given.
    fn table_row(
        &mut self,
        row: &TableRow,
        mut alignments: Option<&mut Vec<Alignment>>,
    ) -> Result<Vec<ArkyTableCell>, PackageError> {
        let mut word = Vec::new();
        row_cells(&row.content, &mut word);
        let mut cells = Vec::new();
        for cell in word {
            let span = grid_span(cell);
            if let Some(columns) = alignments.as_mut() {
                columns.push(cell_alignment(cell));
                columns.extend(iter::repeat_n(Alignment::Default, span - 1));
            }
            if continues_merge(cell) {
                cells.push(empty_cell());
            } else {
                cells.push(self.cell(cell)?);
            }
            cells.extend(iter::repeat_with(empty_cell).take(span - 1));
        }
        Ok(cells)
    }

    /// The content of an insertion or a move's destination, as if the change were accepted.
    fn tracked(
        &mut self,
        content: &[TrackedRunContent],
        runs: &mut Runs,
    ) -> Result<(), PackageError> {
        for item in content {
            match item {
                TrackedRunContent::BidirectionalEmbeddingLevel(level) => {
                    self.paragraph_content(&level.content, runs)?;
                }
                TrackedRunContent::BidirectionalOverride(direction) => {
                    self.paragraph_content(&direction.content, runs)?;
                }
                TrackedRunContent::InlineLevelCustomXMLElement(element) => {
                    self.paragraph_content(&element.content, runs)?;
                }
                TrackedRunContent::InlineLevelSmartTag(tag) => {
                    self.paragraph_content(&tag.content, runs)?;
                }
                TrackedRunContent::InlineLevelStructuredDocumentTag(tag) => {
                    if let Some(inner) = &tag.content {
                        self.paragraph_content(&inner.content, runs)?;
                    }
                }
                TrackedRunContent::InsertedRunContent(insertion) => {
                    self.tracked(&insertion.content, runs)?;
                }
                TrackedRunContent::MoveDestinationRunContent(destination) => {
                    self.tracked(&destination.content, runs)?;
                }
                TrackedRunContent::OfficeMath(_) | TrackedRunContent::OfficeMathParagraph(_) => {
                    if self.shown() {
                        runs.math(&officekit_markup::generic(item), self.link());
                    }
                }
                TrackedRunContent::TextRun(run) => self.run(run, runs)?,
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
        Ok(())
    }

    /// The level of the innermost open list whose items' text starts, by its numbering, at the
    /// own left indent of a paragraph of `properties`.
    fn under(&self, properties: Option<&ParagraphProperties>) -> Option<usize> {
        let indentation = properties.and_then(|own| own.paragraph_indentation.as_deref())?;
        let left = left_indent(indentation)?;
        self.output
            .lists
            .iter()
            .rev()
            .find(|list| self.numbering.indent(list.instance, list.level) == Some(left))
            .map(|list| list.level)
    }
}

/// The target of a link to the bookmark `name`: `#` and the name with each `_` written `-`, the
/// `_` that markdown to Word puts before a name starting with a digit dropped first.
fn anchor_target(name: &str) -> String {
    let named = name
        .strip_prefix('_')
        .filter(|rest| rest.starts_with(char::is_numeric))
        .unwrap_or(name);
    format!("#{}", named.replace('_', "-"))
}

/// The font of a run's `w:rFonts/@w:ascii`, when `property` gives one.
fn ascii_font(property: &RunPropertiesContent) -> Option<&str> {
    if let RunPropertiesContent::RunFonts(fonts) = property {
        fonts.ascii.as_deref()
    } else {
        None
    }
}

/// The value of `element`'s attribute `local` in the namespace the namespace table names
/// `prefix`.
fn attribute(element: &Element, prefix: &str, local: &str) -> Option<String> {
    element
        .attributes
        .iter()
        .find(|candidate| {
            candidate.name.local == local && table_prefix(&candidate.name.namespace) == Some(prefix)
        })
        .map(|found| found.value.clone())
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

/// The alignment of a column whose first row's cell is `cell`, from the `w:jc` of its first
/// paragraph.
fn cell_alignment(cell: &TableCell) -> Alignment {
    let justification = cell
        .content
        .iter()
        .find_map(|block| {
            if let BlockContent::Paragraph(paragraph) = block {
                Some(paragraph)
            } else {
                None
            }
        })
        .and_then(|paragraph| paragraph.properties.as_deref())
        .and_then(|properties| properties.paragraph_alignment.as_deref())
        .and_then(|jc| jc.val.as_ref())
        .map(HorizontalAlignmentType::value);
    let Some(Ok(member)) = justification else {
        return Alignment::Default;
    };
    match member {
        HorizontalAlignmentTypeMember::Center => Alignment::Center,
        HorizontalAlignmentTypeMember::End | HorizontalAlignmentTypeMember::Right => {
            Alignment::Right
        }
        HorizontalAlignmentTypeMember::Left | HorizontalAlignmentTypeMember::Start => {
            Alignment::Left
        }
        HorizontalAlignmentTypeMember::Both
        | HorizontalAlignmentTypeMember::Distribute
        | HorizontalAlignmentTypeMember::HighKashida
        | HorizontalAlignmentTypeMember::LowKashida
        | HorizontalAlignmentTypeMember::MediumKashida
        | HorizontalAlignmentTypeMember::NumTab
        | HorizontalAlignmentTypeMember::ThaiDistribute
        | _ => Alignment::Default,
    }
}

/// Whether `cell` continues a vertical merge: a `w:vMerge` whose value is not `restart`.
fn continues_merge(cell: &TableCell) -> bool {
    cell.properties
        .as_ref()
        .and_then(|properties| properties.vertically_merged_cell.as_ref())
        .is_some_and(|merge| {
            !matches!(
                merge.val.as_ref().map(MergedCellType::value),
                Some(Ok(MergedCellTypeMember::Restart))
            )
        })
}

/// The first element under `element` named `local` in the namespace the namespace table names
/// `prefix`, in document order, outside text boxes.
fn descendant<'tree>(element: &'tree Element, prefix: &str, local: &str) -> Option<&'tree Element> {
    element.child_elements().find_map(|child| {
        if is_named(child, prefix, local) {
            Some(child)
        } else if is_named(child, "w", "txbxContent") {
            None
        } else {
            descendant(child, prefix, local)
        }
    })
}

/// The picture of a `w:drawing`, when its `a:blip` names one.
fn drawing(element: &Element) -> Option<Drawing> {
    let blip = descendant(element, "a", "blip")?;
    let properties = descendant(element, "wp", "docPr");
    let unprefixed = |local: &str| {
        properties
            .and_then(|found| found.attribute(&Namespace::None, local))
            .unwrap_or_default()
            .to_owned()
    };
    let title = unprefixed("title");
    Some(Drawing {
        alt: unprefixed("descr"),
        embed: attribute(blip, "r", "embed"),
        link: attribute(blip, "r", "link"),
        svg: descendant(blip, "asvg", "svgBlip").and_then(|svg| attribute(svg, "r", "embed")),
        title: (!title.is_empty()).then_some(title),
    })
}

/// The picture of a VML `v:shape`, when a `v:imagedata` child names one by `r:id`: the id read as
/// a part or as an address, the shape's `alt` attribute as its alt text.
fn vml_picture(shape: &Element) -> Option<Drawing> {
    let id = shape
        .child_elements()
        .find(|child| is_named(child, "v", "imagedata"))
        .and_then(|data| attribute(data, "r", "id"))?;
    Some(Drawing {
        alt: shape
            .attribute(&Namespace::None, "alt")
            .unwrap_or_default()
            .to_owned(),
        embed: Some(id.clone()),
        link: Some(id),
        svg: None,
        title: None,
    })
}

/// Every picture of a `w:drawing` or a VML `v:shape` under `node`, outside text boxes; of an
/// `mc:AlternateContent`, those of its first branch that holds any.
fn drawings(node: &Node, found: &mut Vec<Drawing>) {
    let Node::Element(element) = node else {
        return;
    };
    if is_named(element, "w", "txbxContent") {
        return;
    }
    if is_named(element, "w", "drawing") {
        found.extend(drawing(element));
    } else if is_named(element, "v", "shape") {
        found.extend(vml_picture(element));
    } else if mc::is_mc(&element.name.namespace) && element.name.local == "AlternateContent" {
        for branch in &element.children {
            let before = found.len();
            drawings(branch, found);
            if found.len() > before {
                break;
            }
        }
    } else {
        for child in &element.children {
            drawings(child, found);
        }
    }
}

/// A cell with no block.
const fn empty_cell() -> ArkyTableCell {
    ArkyTableCell::new(MarkdownBlock::new(Vec::new()))
}

/// The arky document of `root` and `frontmatter`, its styles, numbering and parts read from
/// `styles`, `numbering` and `parts`, and the pictures it names.
///
/// # Errors
///
/// The errors of [`Package::part_bytes`] for a picture or an HTML part.
pub fn exported(
    root: &Document,
    styles: &Styles,
    numbering: &Numbering,
    parts: Parts<'_>,
    frontmatter: Option<Mapping>,
) -> Result<Exported, PackageError> {
    let mut walk = Walk {
        fields: Vec::new(),
        hyperlink: None,
        links: Links::default(),
        numbering,
        numbers: BTreeMap::new(),
        output: Output::default(),
        parts,
        styles,
    };
    if let Some(body) = &root.body {
        walk.blocks(&body.content)?;
    }
    Ok(Exported {
        document: ArkyDocument::new(walk.output.finish(), frontmatter),
        images: walk.parts.images,
    })
}

/// The words of a field instruction: each run of characters between spaces, or between `"`.
fn field_words(instruction: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut rest = instruction.trim_start();
    while !rest.is_empty() {
        let (word, after) = rest.strip_prefix('"').map_or_else(
            || rest.split_once(char::is_whitespace).unwrap_or((rest, "")),
            |quoted| quoted.split_once('"').unwrap_or((quoted, "")),
        );
        words.push(word);
        rest = after.trim_start();
    }
    words
}

/// The number of columns `cell` spans, at least 1.
fn grid_span(cell: &TableCell) -> usize {
    cell.properties
        .as_ref()
        .and_then(|properties| properties.grid_columns_spanned.as_ref())
        .and_then(|span| number(span.val.as_ref()))
        .and_then(|span| usize::try_from(span).ok())
        .unwrap_or(1)
        .max(1)
}

/// Whether `pieces` hold a picture.
fn has_picture(pieces: &[Piece]) -> bool {
    pieces.iter().any(|piece| matches!(piece, Piece::Image(..)))
}

/// Whether `pieces` hold a character other than white space.
fn has_text(pieces: &[Piece]) -> bool {
    pieces.iter().any(|piece| match piece {
        Piece::Break | Piece::Image(..) => false,
        Piece::Text(_, text) => text.chars().any(|character| !character.is_whitespace()),
    })
}

/// The inline nodes of a heading: one line, each hard line break a space.
fn heading_inlines(pieces: Vec<Piece>, links: &Links) -> Vec<Inline> {
    let spaced = pieces.into_iter().map(|piece| match piece {
        Piece::Break => Piece::Text(Marks::default(), " ".to_owned()),
        Piece::Image(..) | Piece::Text(..) => piece,
    });
    lines(spaced, links)
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

/// The HTML between `<body …>` and `</body>` of `page`, in any case; the whole page when it has
/// no `<body`.
fn html_body(page: &str) -> &str {
    let lower = page.to_ascii_lowercase();
    let Some(open) = lower.find("<body") else {
        return page;
    };
    let start = lower[open..]
        .find('>')
        .map_or(page.len(), |close| open + close + 1);
    let end = lower
        .rfind("</body>")
        .filter(|end| *end >= start)
        .unwrap_or(start);
    &page[start..end]
}

/// The link target of a field whose instruction is `HYPERLINK`: its address, followed by `#` and
/// the location its `\l` switch names when it has both; the location alone as [`anchor_target`]
/// names a bookmark; `None` when it names neither. The arguments of `\o` and `\t` are skipped.
fn hyperlink_target(instruction: &str) -> Option<String> {
    let mut words = field_words(instruction).into_iter();
    if !words.next()?.eq_ignore_ascii_case("HYPERLINK") {
        return None;
    }
    let mut address = None;
    let mut location = None;
    let mut argument_of = None;
    for word in words {
        if let Some(switch) = argument_of.take() {
            if switch == "l" {
                location.get_or_insert(word);
            }
        } else if let Some(named) = word.strip_prefix('\\') {
            let switch = named.to_ascii_lowercase();
            if matches!(switch.as_str(), "l" | "o" | "t") {
                argument_of = Some(switch);
            }
        } else {
            address.get_or_insert(word);
        }
    }
    match (address, location) {
        (Some(found), Some(place)) => Some(format!("{found}#{place}")),
        (Some(found), None) => Some(found.to_owned()),
        (None, Some(place)) => Some(anchor_target(place)),
        (None, None) => None,
    }
}

/// The list level whose indent is the own `w:ind w:left`, or `w:start`, of a paragraph of
/// `properties`.
fn indent_level(properties: Option<&ParagraphProperties>) -> Option<usize> {
    let indentation = properties.and_then(|own| own.paragraph_indentation.as_deref())?;
    let written = left_indent(indentation)?.to_string();
    (0..=DEEPEST_LEVEL).find(|level| parts::level_indent(*level) == written)
}

/// The `w:left`, or `w:start`, of `indentation`, in twentieths of a point.
fn left_indent(indentation: &ParagraphIndentation) -> Option<i64> {
    let left = indentation.left.as_ref().or(indentation.start.as_ref())?;
    let Ok(SignedMeasurementInTwentiethsOfPointMember::Integer(twips)) = left.value() else {
        return None;
    };
    Some(twips)
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

/// Whether `row` is marked `w:tblHeader`.
fn is_head_row(row: &TableRow) -> bool {
    row.properties.as_ref().is_some_and(|properties| {
        properties.content.iter().any(|property| {
            if let TableRowPropertiesContent::RepeatTableRow(repeat) = property {
                repeat
                    .val
                    .as_ref()
                    .is_none_or(|value| value.value().unwrap_or(true))
            } else {
                false
            }
        })
    })
}

/// Whether `element` is named `local` in the namespace the namespace table names `prefix`.
fn is_named(element: &Element, prefix: &str, local: &str) -> bool {
    element.name.local == local && table_prefix(&element.name.namespace) == Some(prefix)
}

/// Whether arky writes `body`, a table cell's content, on one line; a cell arky cannot write
/// is not.
fn is_one_line(body: &MarkdownBlock) -> bool {
    let document = RawDocument::Valid(ArkyDocument::new(body.clone(), None));
    writer::write(&document, &WriteOptions::default())
        .is_ok_and(|text| !text.trim_end_matches('\n').contains('\n'))
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

/// A list level from a `w:ilvl` value, from 0.
fn level_number(value: Option<&DecimalNumberValue>) -> Option<usize> {
    let level = number(value)?;
    usize::try_from(level).ok()
}

/// The lines of `pieces`: each hard line break ends one, and the breaks that end the paragraph go.
fn lines<I>(pieces: I, links: &Links) -> Vec<Line>
where
    I: IntoIterator<Item = Piece>,
{
    let mut built = Lines::new(links);
    for piece in pieces {
        match piece {
            Piece::Break => built.line_break(),
            Piece::Image(link, image) => built.image(link, image),
            Piece::Text(marks, text) => built.text(marks, &text),
        }
    }
    built.finish()
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

/// The number a decimal value spells.
fn number(value: Option<&DecimalNumberValue>) -> Option<i64> {
    value?.value().ok()
}

/// The numbering instance a `w:numPr` names.
fn numbering_instance(reference: &NumberingDefinitionInstanceReferenceNumPr) -> Option<i64> {
    let instance = reference
        .numbering_definition_instance_reference_num_id
        .as_ref()?;
    number(instance.val.as_ref())
}

/// The list level a `w:numPr` names.
fn numbering_level(reference: &NumberingDefinitionInstanceReferenceNumPr) -> Option<usize> {
    let level = reference.numbering_level_reference.as_ref()?;
    level_number(level.val.as_ref())
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
            Piece::Image(..) => {}
            Piece::Text(_, value) => text.push_str(&value),
        }
    }
    text
}

/// The first part of the package `main` relates to by `relationship_type`.
///
/// # Errors
///
/// The errors of [`Package::related_by_type`].
fn related_part(
    package: &Package,
    main: &PartName,
    relationship_type: &RelationshipType,
) -> Result<Option<PartName>, PackageError> {
    Ok(package
        .related_by_type(Some(main), relationship_type)?
        .into_iter()
        .find_map(|related| {
            if let Target::Part(name) = related.target {
                Some(name)
            } else {
                None
            }
        }))
}

/// The cells of a row, inside custom XML elements and content controls too, in order.
fn row_cells<'row>(content: &'row [ContentCellContent], cells: &mut Vec<&'row TableCell>) {
    for item in content {
        match item {
            ContentCellContent::CellLevelCustomXMLElement(element) => {
                row_cells(&element.content, cells);
            }
            ContentCellContent::CellLevelStructuredDocumentTag(tag) => {
                if let Some(inner) = &tag.content {
                    row_cells(&inner.content, cells);
                }
            }
            ContentCellContent::TableCell(cell) => cells.push(cell),
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

/// The prefix the namespace table gives `namespace`, when it knows it.
fn table_prefix(namespace: &Namespace) -> Option<&'static str> {
    if let Namespace::Known(id) = *namespace {
        NAMESPACES.get(usize::from(id)).map(|row| row.prefix)
    } else {
        None
    }
}

/// The rows of a table, inside custom XML elements and content controls too, in order.
fn table_rows<'table>(content: &'table [ContentRowContent], rows: &mut Vec<&'table TableRow>) {
    for item in content {
        match item {
            ContentRowContent::RowLevelCustomXMLElement(element) => {
                table_rows(&element.content, rows);
            }
            ContentRowContent::RowLevelStructuredDocumentTag(tag) => {
                if let Some(inner) = &tag.content {
                    table_rows(&inner.content, rows);
                }
            }
            ContentRowContent::TableRow(row) => rows.push(row),
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
