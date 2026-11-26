//! `PartWriter`: one part's XML written in order, straight into its sink.

use core::mem;
use std::io::{self, Write};

use quick_xml::Writer;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};

use super::class::{ClassRules, Site};
use super::reader::PartEvent;
use crate::element::{Attribute, Element, NamespaceDeclaration, Node};
use crate::name::{Namespace, NamespaceId, QName};
use crate::parse::split_qualified;
use crate::write::{
    Layout, Scope, escape, is_whitespace, preserves_space, qualified, start_tag, write_element,
    write_node,
};

#[cfg(test)]
mod tests;

/// The XML declaration Office writes, and the line break after it.
const DECLARATION: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

/// Whether an element's children include text or CDATA, which the pretty layout indents by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Children {
    /// No child is text or CDATA: the pretty layout puts each child on a line of its own.
    Markup,
    /// A child is text or CDATA: the children are written as they are.
    Text,
}

/// Writes one part's XML in order, straight into its sink.
///
/// It writes through `quick-xml`'s writer, in the compact layout or the
/// pretty one, which, as the generic tree's writer does, puts each child of an element on a line
/// of its own, indented by two spaces, unless the element's start said its [`Children`] include
/// text or CDATA, or whitespace is preserved inside it. A start tag waits for the element's first
/// content, so an element nothing is written into ends as an empty tag. It
/// keeps the names and namespace declarations of the elements it has started
/// and not ended, which a typed node written through it takes its prefixes
/// from, and their `xml:space` state, so the compact layout drops
/// whitespace-only text outside a preserve scope, unless whitespace is all its
/// element holds, as the generic tree's writer does.
///
/// Given a class's [`ClassRules`], it writes each typed node as that class writes it.
pub struct PartWriter<'sink> {
    /// Whitespace-only text written since the start tag still waiting in `pending`.
    held: String,
    /// Whether the pretty layout indents the children of each open element.
    indented: Vec<bool>,
    layout: Layout,
    /// The qualified names of the elements started and not ended.
    names: Vec<String>,
    /// The names of the elements started and not ended, kept while `rules` are set.
    parents: Vec<Option<QName>>,
    /// The start tag waiting for its element's first content.
    pending: Option<BytesStart<'static>>,
    /// Whether whitespace is preserved inside each open element.
    preserve: Vec<bool>,
    /// Whether a typed root written through it takes the spelling of the class it is written in,
    /// as [`WriteOptions::respell`](crate::write::WriteOptions::respell) says.
    respell: bool,
    /// The rules of the class the part is written in.
    rules: Option<Box<dyn ClassRules>>,
    scope: Scope,
    writer: Writer<&'sink mut dyn Write>,
}

impl Children {
    /// What `nodes` are: [`Children::Text`] when one of them is CDATA or text that is not empty.
    #[must_use = "this only answers the question"]
    pub fn of<'node, I>(nodes: I) -> Self
    where
        I: IntoIterator<Item = &'node Node>,
    {
        let text = nodes.into_iter().any(|node| {
            matches!(node, Node::CData(_)) || matches!(node, Node::Text(text) if !text.is_empty())
        });
        if text { Self::Text } else { Self::Markup }
    }
}

impl<'sink> PartWriter<'sink> {
    /// Write the start tag waiting for its element's first content, then, inside an element whose
    /// children the pretty layout indents, the line break before the next child.
    fn child(&mut self) -> io::Result<()> {
        self.flush()?;
        if self.indented.last() == Some(&true) {
            self.line_break(self.indented.len())?;
        }
        Ok(())
    }

    /// End the innermost element, written as `name`.
    fn close(&mut self, name: &str) -> io::Result<()> {
        self.scope.end();
        self.preserve.pop();
        self.parents.pop();
        let indented = self.indented.pop() == Some(true);
        match self.pending.take() {
            Some(start) if self.held.is_empty() => self.writer.write_event(Event::Empty(start)),
            Some(start) => {
                self.writer.write_event(Event::Start(start))?;
                let held = mem::take(&mut self.held);
                self.writer
                    .write_event(Event::Text(BytesText::from_escaped(escape(&held, false))))?;
                self.writer.write_event(Event::End(BytesEnd::new(name)))
            }
            None => {
                if indented {
                    self.line_break(self.indented.len())?;
                }
                self.writer.write_event(Event::End(BytesEnd::new(name)))
            }
        }
    }

    /// An element written whole at the writer's place: as it is, or, with a class's rules set,
    /// as the nodes the rules convert it to.
    ///
    /// # Errors
    ///
    /// The sink's, and a [`WriteError`](crate::error::WriteError) the rules refuse the element
    /// with, carried as the error's inner error.
    pub fn converted(&mut self, element: Element) -> io::Result<()> {
        let Some(rules) = self.rules.as_mut() else {
            return self.element(&element);
        };
        let site = Site {
            parent: self.parents.last().and_then(Option::as_ref),
            scope: &self.scope,
        };
        let nodes = rules.convert(element, &site).map_err(io::Error::other)?;
        for node in &nodes {
            self.node(node)?;
        }
        Ok(())
    }

    /// Whether the class's rules write a typed node named `tag`, with the declared attributes
    /// `declared` and the unknown ones `unknown`, as one element through
    /// [`converted`](Self::converted); `false` with no rules set.
    #[must_use = "this only answers the question"]
    pub fn converts(
        &self,
        tag: (Option<NamespaceId>, &str),
        declared: &[(Option<NamespaceId>, &'static str)],
        unknown: &[Attribute],
    ) -> bool {
        self.rules
            .as_ref()
            .is_some_and(|rules| rules.converts(tag, declared, unknown))
    }

    /// `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>` and the `\r\n`
    /// Office writes after it.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn declaration(&mut self) -> io::Result<()> {
        self.flush()?;
        self.writer.get_mut().write_all(DECLARATION)
    }

    /// A whole element of the generic tree, or a typed node written to one.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn element(&mut self, element: &Element) -> io::Result<()> {
        self.child()?;
        let preserve = self.preserved();
        let depth = self.indented.len();
        write_element(&mut self.writer, element, self.layout, depth, preserve)
    }

    /// `<name a="v"/>`, values escaped as in [`Self::start`].
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn empty(&mut self, name: &str, attributes: &[(&str, &str)]) -> io::Result<()> {
        self.child()?;
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
        self.names.pop();
        self.close(name)
    }

    /// Write the start tag waiting for its element's first content; the whitespace held for it
    /// was layout.
    fn flush(&mut self) -> io::Result<()> {
        self.held.clear();
        match self.pending.take() {
            Some(start) => self.writer.write_event(Event::Start(start)),
            None => Ok(()),
        }
    }

    /// Whether a class's rules are set.
    #[must_use = "this only answers the question"]
    pub const fn has_rules(&self) -> bool {
        self.rules.is_some()
    }

    /// The layout the part is written in.
    #[must_use = "this only returns the value"]
    pub const fn layout(&self) -> Layout {
        self.layout
    }

    /// The line break before a child at `depth` below the root, and its indentation.
    fn line_break(&mut self, depth: usize) -> io::Result<()> {
        let sink = self.writer.get_mut();
        sink.write_all(b"\n")?;
        for _ in 0..depth {
            sink.write_all(b"  ")?;
        }
        Ok(())
    }

    /// A writer into `sink`, in `layout`, before anything is written.
    #[must_use = "the result is returned and kept nowhere else"]
    pub fn new(sink: &'sink mut dyn Write, layout: Layout) -> Self {
        Self {
            held: String::new(),
            indented: Vec::new(),
            layout,
            names: Vec::new(),
            parents: Vec::new(),
            pending: None,
            preserve: Vec::new(),
            respell: false,
            rules: None,
            scope: Scope::default(),
            writer: Writer::new(sink),
        }
    }

    /// A node of the generic tree: an element, text, a comment, CDATA, or a
    /// processing instruction. Text goes through [`Self::text`].
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn node(&mut self, node: &Node) -> io::Result<()> {
        if let Node::Text(value) = node {
            return self.text(value);
        }
        self.child()?;
        let preserve = self.preserved();
        let depth = self.indented.len();
        write_node(&mut self.writer, node, self.layout, depth, preserve)
    }

    /// The start tag of an element whose declarations the scope already
    /// holds, a typed node's: its name, declarations, and attributes, as
    /// [`Self::start`] writes them, with what its `children` are.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn open(&mut self, element: &Element, children: Children) -> io::Result<()> {
        self.child()?;
        let preserve = preserves_space(element, self.preserved());
        self.push_indented(preserve, children);
        self.preserve.push(preserve);
        let name = qualified(&element.name);
        self.pending = Some(BytesStart::from_content(
            start_tag(element, &name),
            name.len(),
        ));
        self.names.push(name);
        self.parents
            .push(self.rules.is_some().then(|| element.name.clone()));
        Ok(())
    }

    /// The start tag of a typed node's element, as [`Self::open`] writes it, after the class's
    /// rules, when set, change it in place.
    ///
    /// # Errors
    ///
    /// The sink's, and a [`WriteError`](crate::error::WriteError) the rules refuse the element
    /// with, carried as the error's inner error.
    pub fn open_node(&mut self, element: &mut Element, children: Children) -> io::Result<()> {
        if let Some(rules) = self.rules.as_mut() {
            let site = Site {
                parent: self.parents.last().and_then(Option::as_ref),
                scope: &self.scope,
            };
            rules.open(element, &site).map_err(io::Error::other)?;
        }
        self.open(element, children)
    }

    /// Whether whitespace is preserved around the next node.
    fn preserved(&self) -> bool {
        self.preserve.last().copied().unwrap_or(false)
    }

    /// Whether the pretty layout indents the children of the element just started.
    fn push_indented(&mut self, preserve: bool, children: Children) {
        self.indented
            .push(self.layout == Layout::Pretty && !preserve && children == Children::Markup);
    }

    /// Markup written as it is.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn raw(&mut self, markup: &[u8]) -> io::Result<()> {
        self.child()?;
        self.writer.get_mut().write_all(markup)
    }

    /// Whether a typed root written through it takes the spelling of the class it is written in.
    #[must_use = "this only answers the question"]
    pub const fn respells(&self) -> bool {
        self.respell
    }

    /// The namespace declarations in scope, which a typed node written through this writer takes
    /// its prefixes from.
    pub const fn scope(&mut self) -> &mut Scope {
        &mut self.scope
    }

    /// Say whether a typed root written through it takes the spelling of the class it is written
    /// in, as a save from the other class writes it.
    pub const fn set_respell(&mut self, respell: bool) {
        self.respell = respell;
    }

    /// Set the rules of the class the nodes written next are in, `None` for none; the rules
    /// set before are returned. Set them before a root and take them back after it.
    pub fn set_rules(&mut self, rules: Option<Box<dyn ClassRules>>) -> Option<Box<dyn ClassRules>> {
        mem::replace(&mut self.rules, rules)
    }

    /// `<name xmlns:p="..." a="v">`: the namespace declarations first, then the
    /// attributes, values escaped as in the generic tree's writer. The
    /// declarations stay in scope until the element's `end`; `children` says what the element's
    /// children are.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn start(
        &mut self,
        name: &str,
        declarations: &[NamespaceDeclaration],
        attributes: &[(&str, &str)],
        children: Children,
    ) -> io::Result<()> {
        self.child()?;
        self.scope.start(declarations);
        let preserve = attributes
            .iter()
            .find(|&&(attribute, _)| attribute == "xml:space")
            .map_or_else(|| self.preserved(), |&(_, value)| value == "preserve");
        self.push_indented(preserve, children);
        self.preserve.push(preserve);
        self.pending = Some(tag(name, declarations, attributes));
        self.names.push(name.to_owned());
        let parent = self.rules.is_some().then(|| {
            let (prefix, local) = split_qualified(name);
            let namespace = self
                .scope
                .uri_of(prefix)
                .map_or(Namespace::None, Namespace::from_uri);
            QName::new(prefix.to_owned(), local.to_owned(), namespace)
        });
        self.parents.push(parent);
        Ok(())
    }

    /// Text, escaped as in the generic tree's writer. With no element open,
    /// before or after the root, it is written as it is, as the generic
    /// tree's writer writes the prolog and the epilog. Empty text is no content.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn text(&mut self, value: &str) -> io::Result<()> {
        if self.preserve.is_empty() {
            return self
                .writer
                .write_event(Event::Text(BytesText::from_escaped(value)));
        }
        if value.is_empty() {
            return Ok(());
        }
        if self.layout == Layout::Compact && !self.preserved() && is_whitespace(value) {
            if self.pending.is_some() {
                self.held.push_str(value);
            }
            return Ok(());
        }
        self.child()?;
        self.writer
            .write_event(Event::Text(BytesText::from_escaped(escape(value, false))))
    }

    /// An event of a part read forward, copied as read: a `Declaration` writes
    /// `<?text?>` and nothing after it, a `Start` writes the
    /// element's start tag with its declarations and attributes, its children written as they
    /// are in the pretty layout too, an `End`
    /// ends the element the last unmatched start began, and a `Node` goes
    /// through [`Self::node`].
    ///
    /// # Errors
    ///
    /// The sink's, and [`io::ErrorKind::InvalidInput`] for an `End` with no
    /// element open.
    pub fn write_event(&mut self, event: PartEvent) -> io::Result<()> {
        match event {
            PartEvent::Declaration(text) => {
                self.flush()?;
                self.writer
                    .get_mut()
                    .write_all(format!("<?{text}?>").as_bytes())
            }
            PartEvent::End => {
                let name = self.names.pop().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "an end with no element open")
                })?;
                self.close(&name)
            }
            PartEvent::Node(node) => self.node(&node),
            PartEvent::Start(element) => {
                self.scope.start(&element.namespace_declarations);
                self.open(&element, Children::Text)
            }
        }
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
