//! Writing the generic element tree, over `quick-xml`'s `Writer`: the
//! escaping officekit does itself, the compact and pretty layouts, and the
//! namespace cleanup.
//!
//! `Scope` holds the namespace declarations in scope while a typed tree is
//! written.

use alloc::borrow::Cow;
use alloc::collections::{BTreeMap, BTreeSet};
use std::io;

use quick_xml::Writer;
use quick_xml::events::{BytesCData, BytesDecl, BytesEnd, BytesPI, BytesStart, BytesText, Event};

use super::element::{Attribute, Document, Element, NamespaceDeclaration, Node};
use super::mc::{is_mc, requires};
use super::name::table::NAMESPACES;
use super::name::{Namespace, QName};
use super::parse::split_qualified;

#[cfg(test)]
mod tests;

/// The prefix declared for a namespace the table does not know.
const OTHER_PREFIX: &str = "ns";

/// The `mc` lists whose items are prefixes.
const PREFIX_LISTS: [&str; 2] = ["Ignorable", "MustUnderstand"];

/// The `mc` lists whose items are qualified names.
const QUALIFIED_LISTS: [&str; 3] = ["PreserveAttributes", "PreserveElements", "ProcessContent"];

/// The namespace the `xml` prefix is bound to without a declaration.
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

/// How a part is laid out when written.
///
/// Both layouts write the prolog, the epilog, and the XML declaration as read, and neither changes
/// whitespace inside an `xml:space="preserve"` scope, which an inner `xml:space="default"` ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Layout {
    /// Every whitespace-only text node inside the root is dropped.
    #[default]
    Compact,
    /// Each child of an element whose children include no text or CDATA goes on a line of its
    /// own, indented by two spaces; text passes through.
    Pretty,
}

/// The namespace declarations in scope while a typed tree is written: an
/// element takes a prefix bound to its namespace in either class's URI, or
/// declares the table's prefix where none is bound.
#[derive(Debug, Default)]
pub struct Scope {
    frames: Vec<Vec<NamespaceDeclaration>>,
}

/// The two options of writing a part from a tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct WriteOptions {
    pub layout: Layout,
    /// Remove every namespace declaration nothing in its scope uses, and its
    /// prefix from the markup-compatibility lists.
    pub namespace_cleanup: bool,
}

/// Which declarations of an element the cleanup removed, by the element's
/// index in document order.
type Removed = BTreeMap<usize, BTreeSet<String>>;

impl Scope {
    /// An attribute of `element` in the namespace `uri` (`None`: unqualified),
    /// with a prefix in scope, else the table's prefix declared on `element`.
    /// Called between `element` and `end`.
    pub fn attribute(
        &mut self,
        element: &mut Element,
        uri: Option<&str>,
        local: &str,
        value: &str,
    ) {
        let declared = uri.unwrap_or_default();
        let namespace = Namespace::from_uri(declared);
        let prefix = if namespace == Namespace::None {
            String::new()
        } else {
            self.bound(&namespace, false)
                .unwrap_or_else(|| self.declare(element, &namespace, declared))
        };
        element.attributes.push(Attribute {
            name: QName {
                local: local.to_owned(),
                namespace,
                prefix,
            },
            value: value.to_owned(),
        });
    }

    /// The innermost prefix bound to `namespace` that no inner declaration
    /// rebinds; `xml` is bound without a declaration. For an element, the
    /// empty prefix of a default namespace counts.
    fn bound(&self, namespace: &Namespace, element: bool) -> Option<String> {
        if binds(XML_NAMESPACE, namespace) {
            return Some("xml".to_owned());
        }
        let mut seen: Vec<&str> = Vec::new();
        for declaration in self.frames.iter().rev().flatten() {
            let prefix = declaration.prefix.as_str();
            if seen.contains(&prefix) {
                continue;
            }
            if (element || !prefix.is_empty()) && binds(&declaration.uri, namespace) {
                return Some(prefix.to_owned());
            }
            seen.push(prefix);
        }
        None
    }

    /// Declare the table's prefix for `namespace`, as `uri`, on `element` and
    /// in the innermost frame; returns the prefix.
    fn declare(&mut self, element: &mut Element, namespace: &Namespace, uri: &str) -> String {
        let prefix = match namespace {
            Namespace::Known(id) => NAMESPACES
                .get(usize::from(*id))
                .map_or(OTHER_PREFIX, |row| row.prefix),
            Namespace::None => "",
            Namespace::Other(_) => OTHER_PREFIX,
        };
        let declaration = NamespaceDeclaration {
            prefix: prefix.to_owned(),
            uri: uri.to_owned(),
        };
        if let Some(frame) = self.frames.last_mut() {
            frame.push(declaration.clone());
        }
        element.namespace_declarations.push(declaration);
        prefix.to_owned()
    }

    /// An element of the namespace `uri` named `local`, with the node's own
    /// `declarations`, and the prefix in scope for its namespace, else the
    /// table's prefix declared on it. Opens a frame `end` closes.
    pub fn element(
        &mut self,
        uri: &str,
        local: &str,
        declarations: &[NamespaceDeclaration],
    ) -> Element {
        self.frames.push(declarations.to_vec());
        let namespace = Namespace::from_uri(uri);
        let mut element = Element::new(QName {
            local: local.to_owned(),
            namespace: namespace.clone(),
            prefix: String::new(),
        });
        element.namespace_declarations = declarations.to_vec();
        element.name.prefix = self
            .bound(&namespace, true)
            .unwrap_or_else(|| self.declare(&mut element, &namespace, uri));
        element
    }

    /// Close the frame the last `element` or `start` opened.
    pub fn end(&mut self) {
        self.frames.pop();
    }

    /// Open a frame with an element's own declarations, for an element
    /// written by its name; `end` closes it.
    pub fn start(&mut self, declarations: &[NamespaceDeclaration]) {
        self.frames.push(declarations.to_vec());
    }
}

impl WriteOptions {
    #[must_use]
    pub const fn new(layout: Layout, namespace_cleanup: bool) -> Self {
        Self {
            layout,
            namespace_cleanup,
        }
    }
}

impl Default for WriteOptions {
    /// Compact, with the cleanup on.
    fn default() -> Self {
        Self {
            layout: Layout::Compact,
            namespace_cleanup: true,
        }
    }
}

/// Whether a declaration of `uri` binds `namespace`: either class's URI of a
/// namespace the table knows, else the URI itself.
fn binds(uri: &str, namespace: &Namespace) -> bool {
    match namespace {
        Namespace::Known(id) => NAMESPACES
            .get(usize::from(*id))
            .is_some_and(|row| row.uri == uri || row.strict_uri == Some(uri)),
        Namespace::None => uri.is_empty(),
        Namespace::Other(other) => **other == *uri,
    }
}

/// Remove every namespace declaration whose prefix no element or attribute in
/// its scope uses and no `mc:Choice`'s `Requires` names, and the same prefix
/// from the `mc` lists where the declaration was in scope. Repeats until
/// nothing changes: a dropped list can leave `mc` itself unused.
fn clean_namespaces(root: &mut Element) {
    loop {
        let mut removed = Removed::new();
        strip_unused(root, &mut 0, &mut removed);
        if removed.is_empty() {
            return;
        }
        drop_list_entries(root, &mut 0, &removed, &mut Vec::new());
    }
}

/// The bytes of `document` written with `options`.
pub(crate) fn document(document: &Document, options: WriteOptions) -> Vec<u8> {
    let root = if options.namespace_cleanup {
        let mut cleaned = document.root.clone();
        clean_namespaces(&mut cleaned);
        Cow::Owned(cleaned)
    } else {
        Cow::Borrowed(&document.root)
    };
    let mut writer = Writer::new(Vec::new());
    // A `Vec<u8>` takes every write, so this is always `Ok`.
    let _: io::Result<()> = write_document(&mut writer, document, &root, options.layout);
    writer.into_inner()
}

/// Remove the removed prefixes from the `mc` lists of `element` and its
/// subtree, wherever the removed declaration is the one in scope; a list left
/// empty goes.
fn drop_list_entries(
    element: &mut Element,
    index: &mut usize,
    removed: &Removed,
    scope: &mut Vec<Vec<(String, bool)>>,
) {
    let position = *index;
    *index += 1;
    let mut frame: Vec<(String, bool)> = element
        .namespace_declarations
        .iter()
        .map(|declaration| (declaration.prefix.clone(), false))
        .collect();
    if let Some(gone) = removed.get(&position) {
        frame.extend(gone.iter().map(|prefix| (prefix.clone(), true)));
    }
    scope.push(frame);
    element
        .attributes
        .retain_mut(|attribute| keep_list(attribute, scope));
    for child in &mut element.children {
        if let Node::Element(child_element) = child {
            drop_list_entries(child_element, index, removed, scope);
        }
    }
    scope.pop();
}

/// The escaping `quick-xml`'s writer is handed: `&` and `<` always; `>` in
/// text; in an attribute value `"`, and the tab and line feed a reader would
/// turn into spaces; a carriage return everywhere, which a reader would
/// otherwise turn into a space or a line feed.
pub(crate) fn escape(value: &str, attribute: bool) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' if !attribute => escaped.push_str("&gt;"),
            '"' if attribute => escaped.push_str("&quot;"),
            '\t' if attribute => escaped.push_str("&#x9;"),
            '\n' if attribute => escaped.push_str("&#xA;"),
            '\r' => escaped.push_str("&#xD;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Whether `prefix`'s declaration in scope is one the cleanup removed.
fn is_removed(prefix: &str, scope: &[Vec<(String, bool)>]) -> bool {
    scope
        .iter()
        .rev()
        .find_map(|frame| {
            frame
                .iter()
                .find(|(declared, _)| declared == prefix)
                .map(|&(_, removed)| removed)
        })
        .unwrap_or(false)
}

/// Whether a string is of whitespace only: spaces, tabs, and line breaks.
pub(crate) fn is_whitespace(text: &str) -> bool {
    text.chars()
        .all(|character| matches!(character, ' ' | '\t' | '\n' | '\r'))
}

/// Whether a node is text of whitespace only.
#[must_use]
pub fn is_whitespace_text(node: &Node) -> bool {
    if let Node::Text(text) = node {
        is_whitespace(text)
    } else {
        false
    }
}

/// Drop the removed prefixes from an `mc` list attribute; whether the
/// attribute stays. A list nothing was dropped from is kept as read.
fn keep_list(attribute: &mut Attribute, scope: &[Vec<(String, bool)>]) -> bool {
    if !is_mc(&attribute.name.namespace) {
        return true;
    }
    let local = attribute.name.local.as_str();
    let qualified = QUALIFIED_LISTS.contains(&local);
    if !qualified && !PREFIX_LISTS.contains(&local) {
        return true;
    }
    let items: Vec<&str> = attribute.value.split_whitespace().collect();
    let kept: Vec<&str> = items
        .iter()
        .copied()
        .filter(|item| {
            let prefix = if qualified {
                split_qualified(item).0
            } else {
                item
            };
            !is_removed(prefix, scope)
        })
        .collect();
    if kept.len() == items.len() {
        return true;
    }
    attribute.value = kept.join(" ");
    !attribute.value.is_empty()
}

/// Whether whitespace is preserved inside `element`, given whether it is
/// around it.
#[must_use]
pub fn preserves_space(element: &Element, around: bool) -> bool {
    element
        .attributes
        .iter()
        .find(|attribute| attribute.name.prefix == "xml" && attribute.name.local == "space")
        .map_or(around, |attribute| attribute.value == "preserve")
}

/// A name as written: `prefix:local`, or `local` with no prefix.
#[must_use]
pub fn qualified(name: &QName) -> String {
    if name.prefix.is_empty() {
        name.local.clone()
    } else {
        format!("{}:{}", name.prefix, name.local)
    }
}

/// The content of a start tag: the name, the namespace declarations, then the
/// attributes.
pub(crate) fn start_tag(element: &Element, tag: &str) -> String {
    let mut content = tag.to_owned();
    for declaration in &element.namespace_declarations {
        content.push_str(" xmlns");
        if !declaration.prefix.is_empty() {
            content.push(':');
            content.push_str(&declaration.prefix);
        }
        content.push_str("=\"");
        content.push_str(&escape(&declaration.uri, true));
        content.push('"');
    }
    for attribute in &element.attributes {
        content.push(' ');
        content.push_str(&qualified(&attribute.name));
        content.push_str("=\"");
        content.push_str(&escape(&attribute.value, true));
        content.push('"');
    }
    content
}

/// Remove the declarations of `element` and its subtree that nothing in their
/// scope uses, recording them in `removed`; returns the prefixes the subtree
/// uses and does not declare.
fn strip_unused(
    element: &mut Element,
    index: &mut usize,
    removed: &mut Removed,
) -> BTreeSet<String> {
    let position = *index;
    *index += 1;
    let mut used = BTreeSet::from([element.name.prefix.clone()]);
    used.extend(
        element
            .attributes
            .iter()
            .filter(|attribute| !attribute.name.prefix.is_empty())
            .map(|attribute| attribute.name.prefix.clone()),
    );
    if is_mc(&element.name.namespace) && element.name.local == "Choice" {
        used.extend(requires(element).split_whitespace().map(str::to_owned));
    }
    for child in &mut element.children {
        if let Node::Element(child_element) = child {
            used.extend(strip_unused(child_element, index, removed));
        }
    }
    let unused: BTreeSet<String> = element
        .namespace_declarations
        .iter()
        .filter(|declaration| !used.contains(&declaration.prefix))
        .map(|declaration| declaration.prefix.clone())
        .collect();
    element
        .namespace_declarations
        .retain(|declaration| !unused.contains(&declaration.prefix));
    for declaration in &element.namespace_declarations {
        used.remove(&declaration.prefix);
    }
    if !unused.is_empty() {
        removed.insert(position, unused);
    }
    used
}

fn write_document<W>(
    writer: &mut Writer<W>,
    document: &Document,
    root: &Element,
    layout: Layout,
) -> io::Result<()>
where
    W: io::Write,
{
    if let Some(declaration) = &document.declaration {
        let content = BytesStart::from_content(declaration.as_str(), "xml".len());
        writer.write_event(Event::Decl(BytesDecl::from_start(content)))?;
    }
    for node in &document.prolog {
        write_outside_root(writer, node)?;
    }
    write_element(writer, root, layout, 0, false)?;
    for node in &document.epilog {
        write_outside_root(writer, node)?;
    }
    Ok(())
}

/// Write an element and its subtree, at `depth` below the root, `preserve`
/// telling whether whitespace is preserved around it.
pub(crate) fn write_element<W>(
    writer: &mut Writer<W>,
    element: &Element,
    layout: Layout,
    depth: usize,
    preserve: bool,
) -> io::Result<()>
where
    W: io::Write,
{
    let preserved = preserves_space(element, preserve);
    let tag = qualified(&element.name);
    let start = BytesStart::from_content(start_tag(element, &tag), tag.len());
    let children: Vec<&Node> = element
        .children
        .iter()
        .filter(|child| layout == Layout::Pretty || preserved || !is_whitespace_text(child))
        .collect();
    if children.is_empty() {
        return writer.write_event(Event::Empty(start));
    }
    writer.write_event(Event::Start(start))?;
    let indented = layout == Layout::Pretty
        && !preserved
        && children
            .iter()
            .all(|child| !matches!(child, Node::Text(_) | Node::CData(_)));
    for child in children {
        if indented {
            write_line_break(writer, depth + 1)?;
        }
        write_node(writer, child, layout, depth + 1, preserved)?;
    }
    if indented {
        write_line_break(writer, depth)?;
    }
    writer.write_event(Event::End(BytesEnd::new(tag)))
}

/// A line break and the indentation of `depth`.
fn write_line_break<W>(writer: &mut Writer<W>, depth: usize) -> io::Result<()>
where
    W: io::Write,
{
    let line = format!("\n{}", "  ".repeat(depth));
    writer.write_event(Event::Text(BytesText::from_escaped(line)))
}

pub(crate) fn write_node<W>(
    writer: &mut Writer<W>,
    node: &Node,
    layout: Layout,
    depth: usize,
    preserve: bool,
) -> io::Result<()>
where
    W: io::Write,
{
    match node {
        Node::CData(content) => writer.write_event(Event::CData(BytesCData::new(content.as_str()))),
        Node::Comment(content) => {
            writer.write_event(Event::Comment(BytesText::from_escaped(content.as_str())))
        }
        Node::Element(element) => write_element(writer, element, layout, depth, preserve),
        Node::ProcessingInstruction { content, target } => {
            let instruction = if content.is_empty() {
                target.clone()
            } else {
                format!("{target} {content}")
            };
            writer.write_event(Event::PI(BytesPI::new(instruction)))
        }
        Node::Text(content) => {
            writer.write_event(Event::Text(BytesText::from_escaped(escape(content, false))))
        }
    }
}

/// Outside the root, text is whitespace written as read, line endings
/// included, so the prolog and the epilog come out as they were read.
fn write_outside_root<W>(writer: &mut Writer<W>, node: &Node) -> io::Result<()>
where
    W: io::Write,
{
    if let Node::Text(text) = node {
        writer.write_event(Event::Text(BytesText::from_escaped(text.as_str())))
    } else {
        write_node(writer, node, Layout::Compact, 0, false)
    }
}
