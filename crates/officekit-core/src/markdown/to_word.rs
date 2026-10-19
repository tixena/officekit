//! archie's blocks and inline nodes as Word paragraphs and runs.

use alloc::boxed::Box;
use alloc::vec::Vec;

use archie_core::parser::ast::{
    Block, Inline, Line, LinkInline, MarkdownBlock, Paragraph as ArchieParagraph, Section,
};
use archie_core::parser::extension::Extension;
use officekit_markup::wordprocessing::{
    BlockContent, Bold, BorderStylesMember, Break, Italics, Paragraph,
    ParagraphBorderBelowIdenticalParagraphs, ParagraphBorders, ParagraphContent,
    ParagraphProperties, ReferencedCharacterStyle, ReferencedParagraphStyle, RunProperties,
    RunPropertiesContent, TabCharacter, Text, TextRun, TextRunContent,
};

use super::parts::{self, CODE, DEEPEST_HEADING, PREFORMATTED, QUOTE};

/// The Word blocks written so far.
#[derive(Default)]
struct Body {
    blocks: Vec<BlockContent>,
}

impl Body {
    fn block<E>(&mut self, block: &Block<E>)
    where
        E: Extension,
    {
        match block {
            Block::BlockQuote(quote) => self.lines_styled(QUOTE, &quote.value),
            Block::CodeBlock(code) => self.lines_styled(PREFORMATTED, &code.value),
            Block::Paragraph(paragraph) => self.paragraph(paragraph),
            Block::ThematicBreak(_) => self.thematic_break(),
            Block::Html(_) | Block::List(_) | Block::Table(_) | _ => {}
        }
    }

    /// One paragraph styled `style` per line of `text`; an empty line is an empty paragraph.
    fn lines_styled(&mut self, style: &str, text: &str) {
        for line in text.split('\n') {
            let mut runs = Runs::default();
            runs.text(line.strip_suffix('\r').unwrap_or(line), Format::default());
            self.push(Some(style), runs.content);
        }
    }

    /// One paragraph of `paragraph`'s lines, each after the one before and a space, or no space
    /// when the one before ends in a hard line break.
    fn paragraph<E>(&mut self, paragraph: &ArchieParagraph<E>)
    where
        E: Extension,
    {
        let mut runs = Runs::default();
        let mut previous: Option<&Line<E>> = None;
        for line in &paragraph.lines {
            if previous.is_some_and(|before| !ends_in_break(&before.children)) {
                runs.text(" ", Format::default());
            }
            runs.inlines(&line.children, Format::default());
            previous = Some(line);
        }
        self.push(None, runs.content);
    }

    /// A paragraph of `content`, styled `style`.
    fn push(&mut self, style: Option<&str>, content: Vec<ParagraphContent>) {
        let mut paragraph = Paragraph::default();
        if let Some(id) = style {
            let mut reference = ReferencedParagraphStyle::default();
            reference.val = Some(id.to_owned());
            let mut properties = ParagraphProperties::default();
            properties.referenced_paragraph_style = Some(Box::new(reference));
            paragraph.properties = Some(Box::new(properties));
        }
        paragraph.content = content;
        self.blocks.push(BlockContent::Paragraph(paragraph));
    }

    /// The section's heading at `depth`, then its blocks, then its subsections one level deeper.
    fn section<E>(&mut self, section: &Section<E>, depth: usize)
    where
        E: Extension,
    {
        if let Some(heading) = &section.heading {
            let mut runs = Runs::default();
            runs.inlines(&heading.children, Format::default());
            let style = parts::heading(depth.min(DEEPEST_HEADING));
            self.push(Some(&style), runs.content);
        }
        for block in &section.blocks {
            self.block(block);
        }
        for subsection in &section.subsections {
            self.section(subsection, depth + 1);
        }
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
        let mut properties = ParagraphProperties::default();
        properties.paragraph_borders = Some(Box::new(borders));
        let mut paragraph = Paragraph::default();
        paragraph.properties = Some(Box::new(properties));
        self.blocks.push(BlockContent::Paragraph(paragraph));
    }
}

/// The formatting the inline nodes around a run give it.
#[derive(Clone, Copy, Default)]
struct Format {
    bold: bool,
    code: bool,
    italic: bool,
}

impl Format {
    /// The run's properties, `None` when it has no formatting.
    fn properties(self) -> Option<Box<RunProperties>> {
        let mut content = Vec::new();
        if self.code {
            let mut style = ReferencedCharacterStyle::default();
            style.val = Some(CODE.to_owned());
            content.push(RunPropertiesContent::ReferencedCharacterStyle(style));
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

/// The runs of one paragraph, written so far.
#[derive(Default)]
struct Runs {
    content: Vec<ParagraphContent>,
}

impl Runs {
    fn inline<E>(&mut self, inline: &Inline<E>, format: Format)
    where
        E: Extension,
    {
        match inline {
            Inline::Code(code) => self.text(
                &code.value,
                Format {
                    code: true,
                    ..format
                },
            ),
            Inline::Emphasis(emphasis) => self.inlines(
                &emphasis.children,
                Format {
                    italic: true,
                    ..format
                },
            ),
            Inline::LineBreak(_) => self.line_break(format),
            Inline::Link(link) => {
                for child in &link.children {
                    self.link_inline(child, format);
                }
            }
            Inline::Strong(strong) => self.inlines(
                &strong.children,
                Format {
                    bold: true,
                    ..format
                },
            ),
            Inline::Text(text) => self.text(&text.value, format),
            Inline::Html(_) | Inline::Image(_) | _ => {}
        }
    }

    fn inlines<E>(&mut self, inlines: &[Inline<E>], format: Format)
    where
        E: Extension,
    {
        for inline in inlines {
            self.inline(inline, format);
        }
    }

    fn line_break(&mut self, format: Format) {
        self.run(alloc::vec![TextRunContent::Break(Break::default())], format);
    }

    /// A node of a link's text, as plain runs.
    fn link_inline<E>(&mut self, inline: &LinkInline<E>, format: Format)
    where
        E: Extension,
    {
        match inline {
            LinkInline::Code(code) => self.text(
                &code.value,
                Format {
                    code: true,
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

    fn run(&mut self, content: Vec<TextRunContent>, format: Format) {
        let mut run = TextRun::default();
        run.content = content;
        run.properties = format.properties();
        self.content.push(ParagraphContent::TextRun(run));
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

/// The Word blocks of `body`, its sections from depth 1.
pub fn blocks<E>(body: &MarkdownBlock<E>) -> Vec<BlockContent>
where
    E: Extension,
{
    let mut writer = Body::default();
    for section in &body.sections {
        writer.section(section, 1);
    }
    writer.blocks
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
