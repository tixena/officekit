//! `PartWriter`: one part's XML written in order, straight into its sink.

use std::io::{self, Write};

use quick_xml::Writer;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};

use crate::xml::element::{Element, NamespaceDeclaration, Node};
use crate::xml::write::{Layout, escape, write_element, write_node};

#[cfg(test)]
mod tests;

/// The XML declaration Office writes, and the line break after it.
const DECLARATION: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

/// Writes one part's XML in order, straight into its sink.
///
/// It writes through `quick-xml`'s writer, in D29's layout: compact, or
/// pretty, which puts each tag on a line of its own, indented by two spaces,
/// unless text comes before it.
pub struct PartWriter<'sink> {
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
        // Past the writer, so the pretty layout does not break the line before the root.
        self.writer.get_mut().write_all(DECLARATION)
    }

    /// A whole element of the generic tree, or a typed node written to one.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn element(&mut self, element: &Element) -> io::Result<()> {
        write_element(&mut self.writer, element, Layout::Compact, 0, false)
    }

    /// `<name a="v"/>`, values escaped as in [`Self::start`].
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn empty(&mut self, name: &str, attributes: &[(&str, &str)]) -> io::Result<()> {
        self.writer
            .write_event(Event::Empty(tag(name, &[], attributes)))
    }

    /// `</name>`.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn end(&mut self, name: &str) -> io::Result<()> {
        self.writer.write_event(Event::End(BytesEnd::new(name)))
    }

    #[must_use]
    pub fn new(sink: &'sink mut dyn Write, layout: Layout) -> Self {
        let writer = match layout {
            Layout::Compact => Writer::new(sink),
            Layout::Pretty => Writer::new_with_indent(sink, b' ', 2),
        };
        Self { writer }
    }

    /// A node of the generic tree: an element, text, a comment, CDATA, or a
    /// processing instruction.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn node(&mut self, node: &Node) -> io::Result<()> {
        write_node(&mut self.writer, node, Layout::Compact, 0, false)
    }

    /// Markup written as it is.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn raw(&mut self, markup: &[u8]) -> io::Result<()> {
        self.writer.get_mut().write_all(markup)
    }

    /// `<name xmlns:p="..." a="v">`: the namespace declarations first, then the
    /// attributes, values escaped as in the generic tree's writer.
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
        self.writer
            .write_event(Event::Start(tag(name, declarations, attributes)))
    }

    /// Text, escaped as in the generic tree's writer.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn text(&mut self, value: &str) -> io::Result<()> {
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
