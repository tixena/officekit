//! `PartWriter`: one part's XML written in order, straight into its sink.

use std::io::{self, Write};

use quick_xml::Writer;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};

use crate::markup::Scope;
use crate::xml::element::{Element, NamespaceDeclaration, Node};
use crate::xml::write::{
    Layout, escape, is_whitespace, preserves_space, qualified, start_tag, write_element, write_node,
};

#[cfg(test)]
mod tests;

/// The XML declaration Office writes, and the line break after it.
const DECLARATION: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

/// Writes one part's XML in order, straight into its sink.
///
/// It writes through `quick-xml`'s writer, in D29's layout: compact, or
/// pretty, which puts each tag on a line of its own, indented by two spaces,
/// unless text comes before it. A start tag waits for the element's first
/// content, so an element nothing is written into ends as an empty tag. It
/// keeps the namespace declarations of the elements it has started and not
/// ended, which a typed node written through it takes its prefixes from, and
/// their `xml:space` state, so the compact layout drops whitespace-only text
/// outside a preserve scope as the generic tree's writer does.
pub struct PartWriter<'sink> {
    layout: Layout,
    /// The start tag waiting for its element's first content.
    pending: Option<BytesStart<'static>>,
    /// Whether whitespace is preserved inside each open element.
    preserve: Vec<bool>,
    scope: Scope,
    writer: Writer<&'sink mut dyn Write>,
}

impl<'sink> PartWriter<'sink> {
    /// `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>` and the `\r\n`
    /// Office writes after it.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn declaration(&mut self) -> io::Result<()> {
        self.flush()?;
        // Past the writer, so the pretty layout does not break the line before the root.
        self.writer.get_mut().write_all(DECLARATION)
    }

    /// A whole element of the generic tree, or a typed node written to one.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn element(&mut self, element: &Element) -> io::Result<()> {
        self.flush()?;
        let preserve = self.preserved();
        write_element(&mut self.writer, element, Layout::Compact, 0, preserve)
    }

    /// `<name a="v"/>`, values escaped as in [`Self::start`].
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn empty(&mut self, name: &str, attributes: &[(&str, &str)]) -> io::Result<()> {
        self.flush()?;
        self.writer
            .write_event(Event::Empty(tag(name, &[], attributes)))
    }

    /// `</name>`, or `<name/>` in place of the start tag when nothing was
    /// written into the element; its declarations leave the scope.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn end(&mut self, name: &str) -> io::Result<()> {
        self.scope.end();
        self.preserve.pop();
        match self.pending.take() {
            Some(start) => self.writer.write_event(Event::Empty(start)),
            None => self.writer.write_event(Event::End(BytesEnd::new(name))),
        }
    }

    /// Write the start tag waiting for its element's first content.
    fn flush(&mut self) -> io::Result<()> {
        match self.pending.take() {
            Some(start) => self.writer.write_event(Event::Start(start)),
            None => Ok(()),
        }
    }

    #[must_use]
    pub fn new(sink: &'sink mut dyn Write, layout: Layout) -> Self {
        let writer = match layout {
            Layout::Compact => Writer::new(sink),
            Layout::Pretty => Writer::new_with_indent(sink, b' ', 2),
        };
        Self {
            layout,
            pending: None,
            preserve: Vec::new(),
            scope: Scope::default(),
            writer,
        }
    }

    /// A node of the generic tree: an element, text, a comment, CDATA, or a
    /// processing instruction.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn node(&mut self, node: &Node) -> io::Result<()> {
        self.flush()?;
        let preserve = self.preserved();
        write_node(&mut self.writer, node, Layout::Compact, 0, preserve)
    }

    /// The start tag of an element whose declarations the scope already
    /// holds, a typed node's: its name, declarations, and attributes, as
    /// [`Self::start`] writes them.
    pub(crate) fn open(&mut self, element: &Element) -> io::Result<()> {
        self.flush()?;
        let preserve = preserves_space(element, self.preserved());
        self.preserve.push(preserve);
        let name = qualified(&element.name);
        self.pending = Some(BytesStart::from_content(
            start_tag(element, &name),
            name.len(),
        ));
        Ok(())
    }

    /// Whether whitespace is preserved around the next node.
    fn preserved(&self) -> bool {
        self.preserve.last().copied().unwrap_or(false)
    }

    /// Markup written as it is.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn raw(&mut self, markup: &[u8]) -> io::Result<()> {
        self.flush()?;
        self.writer.get_mut().write_all(markup)
    }

    /// The namespace declarations in scope.
    pub(crate) const fn scope(&mut self) -> &mut Scope {
        &mut self.scope
    }

    /// `<name xmlns:p="..." a="v">`: the namespace declarations first, then the
    /// attributes, values escaped as in the generic tree's writer. The
    /// declarations stay in scope until the element's `end`.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn start(
        &mut self,
        name: &str,
        declarations: &[NamespaceDeclaration],
        attributes: &[(&str, &str)],
    ) -> io::Result<()> {
        self.flush()?;
        self.scope.start(declarations);
        let preserve = attributes
            .iter()
            .find(|&&(attribute, _)| attribute == "xml:space")
            .map_or_else(|| self.preserved(), |&(_, value)| value == "preserve");
        self.preserve.push(preserve);
        self.pending = Some(tag(name, declarations, attributes));
        Ok(())
    }

    /// Text, escaped as in the generic tree's writer.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn text(&mut self, value: &str) -> io::Result<()> {
        if self.layout == Layout::Compact && !self.preserved() && is_whitespace(value) {
            return Ok(());
        }
        self.flush()?;
        self.writer
            .write_event(Event::Text(BytesText::from_escaped(escape(value, false))))
    }
}

/// The content of a start tag: the name, the namespace declarations, then the
/// attributes.
fn tag(
    name: &str,
    declarations: &[NamespaceDeclaration],
    attributes: &[(&str, &str)],
) -> BytesStart<'static> {
    let mut content = name.to_owned();
    for declaration in declarations {
        content.push_str(" xmlns");
        if !declaration.prefix.is_empty() {
            content.push(':');
            content.push_str(&declaration.prefix);
        }
        content.push_str("=\"");
        content.push_str(&escape(&declaration.uri, true));
        content.push('"');
    }
    for &(attribute, value) in attributes {
        content.push(' ');
        content.push_str(attribute);
        content.push_str("=\"");
        content.push_str(&escape(value, true));
        content.push('"');
    }
    BytesStart::from_content(content, name.len())
}
