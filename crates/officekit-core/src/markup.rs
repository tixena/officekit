//! The typed tree's shared runtime (D8, D12): where unknown content goes when
//! a node is read and written, and which prefix a written element takes.
//!
//! It holds the reader and writer every generated node goes through, the
//! parsers its simple-type newtypes call, and the `Transform` and `Visit`
//! traits with their root hooks.

// The namespace modules, generated in a file of their own: module declarations
// go before the imports, and `include!` places them where it stands.
include!("markup/modules.rs");

use alloc::collections::VecDeque;
use core::num::ParseFloatError;
use core::{iter, mem, slice};
use std::io;

use crate::stream::transform::{Place, Unchanged, WriteThrough, Written};
use crate::stream::writer::PartWriter;
use crate::validation::schema::matches_pattern;
use crate::xml;
use crate::xml::element::{Attribute, Element, NamespaceDeclaration, Node};
use crate::xml::name::table::NAMESPACES;
use crate::xml::name::{Namespace, NamespaceId, OfficeVersion, QName};
use crate::xml::values::{DateTime, Measure, ValueError, parse};
use crate::xml::write::{is_whitespace_text, qualified};

#[cfg(test)]
mod tests;

/// The prefix declared for a namespace the table does not know.
const OTHER_PREFIX: &str = "ns";

/// The namespace the `xml` prefix is bound to without a declaration.
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

// Not a module: `pub use` is denied, and its traits are `markup::Context` and `markup::Visitor`.
include!("markup/root.rs");

/// The tags an enum of the typed tree reads: its arms', and the namespaces a
/// wildcard among them allows.
#[derive(Debug)]
#[non_exhaustive]
pub struct Accept {
    pub any: Option<Wildcard>,
    pub tags: &'static [Tag],
}

/// The bounds of a floating-point simple type.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Bounds {
    pub max_exclusive: Option<f64>,
    pub max_inclusive: Option<f64>,
    pub min_exclusive: Option<f64>,
    pub min_inclusive: Option<f64>,
}

/// What a declared child position takes.
#[derive(Debug)]
enum Accepted {
    Enum(&'static Accept),
    Tag(Tag),
}

/// Writes declared children in schema order, with each `Extra` before the
/// declared child its place names.
#[derive(Debug)]
pub struct Children<'extra> {
    extra: &'extra [Extra],
    known: usize,
    out: Vec<Node>,
}

/// Whether a visitor walks into the children of the node it was just shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Descend {
    No,
    Yes,
}

/// A node the schemas do not declare, at a position no list covers: after
/// the `after`-th declared child of its parent, 0 being before the first.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Extra {
    pub after: usize,
    pub node: Node,
}

/// The length and pattern facets of a string simple type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Facets {
    pub length: Option<usize>,
    pub max_length: Option<usize>,
    pub min_length: Option<usize>,
    /// The text matches one of them whole, when there are any.
    pub patterns: &'static [&'static str],
}

/// One declared attribute or child a typed node's reader is given, in
/// document order, by its index in the node's `Shape`.
#[derive(Debug)]
#[non_exhaustive]
pub enum Field<'element> {
    /// A declared child element.
    Element(&'element Element),
    /// A node the schemas do not declare, which joins the list of an enum
    /// type at this position as its `Other` item.
    Node(Node),
    /// A declared attribute's value, or the text of simple content.
    Text(String),
}

/// The members of an enumeration newtype: each member, the text it stands
/// for, and the Office version that brought it (`None` for ECMA-376's own).
pub type Members<M> = [(M, &'static str, Option<OfficeVersion>)];

/// Where pending unknown nodes go: into the list whose item is next to them,
/// or into the node's `unknown.children`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Pending {
    Extra(Vec<Extra>),
    List(Vec<Node>),
}

/// Where the unknown nodes met since the last declared child go when the next
/// declared child arrives.
#[derive(Debug, Default)]
pub struct Placement {
    known: usize,
    last_was_item: bool,
    pending: Vec<Node>,
}

/// Reads a typed node's attributes and children in document order.
///
/// Each declared one goes to the generated reader by its index in the node's
/// `Shape`; what the schemas do not declare stays at its place (D8).
#[derive(Debug)]
pub struct Reader<'element> {
    attributes: slice::Iter<'element, Attribute>,
    children: slice::Iter<'element, Node>,
    /// The position of the last declared child read.
    cursor: usize,
    element: &'element Element,
    ended: bool,
    /// Which single positions hold their child.
    filled: Vec<bool>,
    /// The list of an enum type the last declared child joined.
    last_items: Option<usize>,
    /// The tags each `xsd:all` list holds.
    once: Vec<(usize, Tag)>,
    placement: Placement,
    queue: VecDeque<(usize, Field<'element>)>,
    shape: &'static Shape,
    unknown: Unknown,
}

/// How many declared children each original declared child of a node became
/// in its transform, in document order (D8, D20).
#[derive(Debug, Default)]
pub struct Rebase {
    counts: Vec<usize>,
}

/// The namespace declarations in scope while a typed tree is written: an
/// element takes a prefix bound to its namespace in either class's URI, or
/// declares the table's prefix where none is bound.
#[derive(Debug, Default)]
pub struct Scope {
    frames: Vec<Vec<NamespaceDeclaration>>,
}

/// How a typed node is read and written: its tag, its declared attributes and
/// child positions in schema order, and whether it holds text.
#[derive(Debug)]
#[non_exhaustive]
pub struct Shape {
    pub attributes: &'static [Tag],
    pub name: Tag,
    pub slots: &'static [Slot],
    /// Simple content: the element's text is its value.
    pub text: bool,
}

/// A declared child position of a typed node.
#[derive(Debug)]
pub struct Slot {
    accept: Accepted,
    occurs: SlotOccurs,
}

/// How many children a declared position takes, and where unknown nodes
/// around them go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotOccurs {
    /// A list of an enum type: unknown nodes among its items join it.
    Items,
    /// A list of one struct type: unknown nodes around it are `Extra`s.
    List,
    /// An `xsd:all` list: like `Items`, and a tag it already holds is unknown.
    Once,
    /// One child.
    Single,
}

/// A tag a typed node declares: its namespace's id, `None` when unqualified,
/// and its local name.
pub type Tag = (Option<NamespaceId>, &'static str);

/// Writes a typed node through a `PartWriter` (D37).
///
/// Its declared attributes go in schema order, then the unknown ones, and its
/// declared children in schema order, each written through, with each list's
/// `Other` items among them and each `Extra` at its place (D8). An element
/// nothing is written into ends as an empty tag, as the generic tree writes it.
pub struct Through<'out, 'sink, 'node> {
    element: Element,
    opened: bool,
    out: &'out mut PartWriter<'sink>,
    place: Place<'node>,
    shape: &'static Shape,
    unknown: &'node Unknown,
}

/// What every node carries besides its declared attributes and children (D8).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Unknown {
    /// Attributes the schemas do not declare, `mc:Ignorable` among them.
    pub attributes: Vec<Attribute>,
    /// Unknown children at positions no list covers.
    pub children: Vec<Extra>,
    pub namespace_declarations: Vec<NamespaceDeclaration>,
}

/// Walks a typed node's children for a visitor in document order: its
/// declared children in schema order, with each `Extra` before the declared
/// child its place names and the rest after the last one (D8).
#[derive(Debug)]
pub struct Walk<'node> {
    known: usize,
    unknown: &'node Unknown,
}

/// The namespaces a wildcard position (`xsd:any`) allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Wildcard {
    /// Any namespace, none included.
    Any,
    /// No namespace.
    Local,
    /// A namespace other than this one, none excluded.
    Not(NamespaceId),
    /// These namespaces.
    Only(&'static [NamespaceId]),
}

/// Writes a typed node: its declared attributes in schema order, then the
/// unknown ones, and its declared children in schema order with each list's
/// `Other` items among them and each `Extra` at its place (D8).
#[derive(Debug)]
pub struct Writer<'scope, 'node> {
    children: Children<'node>,
    element: Element,
    scope: &'scope mut Scope,
    shape: &'static Shape,
    unknown: &'node Unknown,
}

/// A declared attribute's field: its text, when it has one.
pub trait AttributeValue {
    /// The text to write; `None` leaves the attribute out.
    fn text(&self) -> Option<&str>;
}

/// A declared child's field: one node, boxed, an optional one, or a list.
pub trait ChildField {
    /// Take a declared child, or an unknown node read at the field's position.
    fn take(&mut self, field: Field<'_>);

    /// Write the field's nodes at their place.
    fn write(&self, writer: &mut Writer<'_, '_>);
}

/// An unsigned number a fixed count of hexadecimal bytes reads into.
pub trait Hex: Sized {
    /// The number `text` spells in hexadecimal digits.
    fn from_hex(text: &str) -> Option<Self>;
}

/// Every enum of the typed tree: whether an item is one of its arms, and the
/// list hooks a list of its items asks (D20, D37).
pub trait Item: Sized {
    /// Whether the item is one of the enum's arms, not its `Other` node.
    #[must_use]
    fn is_declared(&self) -> bool;

    /// The list hook: `Some` puts any number of items, none included, in the
    /// item's place; `None`, the default and the answer for an enum no list
    /// holds, leaves the item to its own `transform`.
    fn items(&self, _ctx: &mut dyn Context) -> Option<Vec<Self>> {
        None
    }

    /// The list write hook: `Written::Yes` means it wrote any number of items,
    /// none included, in the item's place.
    ///
    /// # Errors
    ///
    /// The sink's.
    fn write_items(
        &self,
        _ctx: &mut dyn Context,
        _out: &mut PartWriter<'_>,
    ) -> io::Result<Written> {
        Ok(Written::No)
    }
}

/// A declared child's field as the traversals see it: one node boxed, an
/// optional one, or a list.
pub trait Nodes {
    type Node;

    /// The nodes the field holds, in document order.
    fn nodes(&self) -> &[Self::Node];

    /// The field rebuilt with each of its nodes transformed.
    #[must_use]
    fn transform_nodes(&self, ctx: &mut dyn Context) -> Self
    where
        Self: Sized,
        Self::Node: Transform;
}

/// A node of the typed tree read from a generic element.
pub trait Read: Sized {
    /// The `Other` item holding a node the schemas do not declare, for an
    /// enum; `None` for a struct, which has no such item.
    #[must_use]
    fn other(_node: Node) -> Option<Self> {
        None
    }

    /// The node `element` holds.
    fn read(element: &Element) -> Self;
}

/// Every node type: from a reference to a node, an owned node, with the
/// context deciding what changes (D12).
pub trait Transform: Sized {
    /// Ask the context about this node; rebuild from transformed children if
    /// it answers `None`.
    #[must_use]
    fn transform(&self, ctx: &mut dyn Context) -> Self;

    /// Rebuild from transformed children without asking the context about
    /// this node.
    #[must_use]
    fn transform_children(&self, ctx: &mut dyn Context) -> Self;
}

/// Every node type: shown to a visitor, which decides whether to walk into
/// its children.
pub trait Visit {
    /// Show this node to the visitor, then walk its children unless the
    /// visitor answers `Descend::No`.
    fn visit(&self, visitor: &mut dyn Visitor);

    /// Walk this node's children without showing the node to the visitor.
    fn visit_children(&self, visitor: &mut dyn Visitor);
}

/// A node of the typed tree written at its place: a struct as its element,
/// an enum's item as its arm's element or its `Other` node.
pub trait Write {
    /// Write the node into the element `writer` writes.
    fn write_into(&self, writer: &mut Writer<'_, '_>);
}

impl<'extra> Children<'extra> {
    /// The children written so far, then every `Extra` whose place is past
    /// the last declared child.
    #[must_use]
    pub fn finish(mut self) -> Vec<Node> {
        let known = self.known;
        self.out.extend(
            self.extra
                .iter()
                .filter(|extra| extra.after >= known)
                .map(|extra| extra.node.clone()),
        );
        self.out
    }

    /// The next declared child, after the `Extra`s placed before it.
    pub fn known(&mut self, node: Element) {
        let known = self.known;
        self.out.extend(
            self.extra
                .iter()
                .filter(|extra| extra.after == known)
                .map(|extra| extra.node.clone()),
        );
        self.out.push(Node::Element(node));
        self.known += 1;
    }

    #[must_use]
    pub const fn new(extra: &'extra [Extra]) -> Self {
        Self {
            extra,
            known: 0,
            out: Vec::new(),
        }
    }

    /// A list's `Other` item: it does not count as a declared child.
    pub fn other(&mut self, node: Node) {
        self.out.push(node);
    }
}

impl Field<'_> {
    /// Set a declared attribute's field from its text.
    pub fn attribute<T>(self, slot: &mut Option<T>)
    where
        T: From<String>,
    {
        if let Self::Text(text) = self {
            *slot = Some(T::from(text));
        }
    }

    /// Give a declared child, or an unknown node at its position, to its field.
    pub fn child<F>(self, slot: &mut F)
    where
        F: ChildField,
    {
        slot.take(self);
    }

    /// Set simple content's field from the element's text.
    pub fn text<T>(self, slot: &mut T)
    where
        T: From<String>,
    {
        if let Self::Text(text) = self {
            *slot = T::from(text);
        }
    }
}

impl<T> AttributeValue for Option<T>
where
    T: AsRef<str>,
{
    fn text(&self) -> Option<&str> {
        self.as_ref().map(AsRef::as_ref)
    }
}

impl<T> ChildField for Box<T>
where
    T: Read + Write,
{
    fn take(&mut self, field: Field<'_>) {
        if let Field::Element(element) = field {
            **self = T::read(element);
        }
    }

    fn write(&self, writer: &mut Writer<'_, '_>) {
        self.as_ref().write_into(writer);
    }
}

impl<T> ChildField for Option<Box<T>>
where
    T: Read + Write,
{
    fn take(&mut self, field: Field<'_>) {
        if let Field::Element(element) = field {
            *self = Some(Box::new(T::read(element)));
        }
    }

    fn write(&self, writer: &mut Writer<'_, '_>) {
        if let Some(node) = self {
            node.write_into(writer);
        }
    }
}

impl<T> ChildField for Vec<T>
where
    T: Read + Write,
{
    fn take(&mut self, field: Field<'_>) {
        match field {
            Field::Element(element) => self.push(T::read(element)),
            Field::Node(node) => self.extend(T::other(node)),
            Field::Text(_) => {}
        }
    }

    fn write(&self, writer: &mut Writer<'_, '_>) {
        for node in self {
            node.write_into(writer);
        }
    }
}

impl<T> Nodes for Box<T> {
    type Node = T;

    fn nodes(&self) -> &[T] {
        slice::from_ref(self)
    }

    fn transform_nodes(&self, ctx: &mut dyn Context) -> Self
    where
        T: Transform,
    {
        Self::new(self.as_ref().transform(ctx))
    }
}

impl<T> Nodes for Option<Box<T>> {
    type Node = T;

    fn nodes(&self) -> &[T] {
        self.as_deref().map_or(&[], slice::from_ref)
    }

    fn transform_nodes(&self, ctx: &mut dyn Context) -> Self
    where
        T: Transform,
    {
        self.as_deref().map(|node| Box::new(node.transform(ctx)))
    }
}

impl<T> Nodes for Vec<T> {
    type Node = T;

    fn nodes(&self) -> &[T] {
        self.as_slice()
    }

    fn transform_nodes(&self, ctx: &mut dyn Context) -> Self
    where
        T: Transform,
    {
        self.iter().map(|node| node.transform(ctx)).collect()
    }
}

impl Hex for u16 {
    fn from_hex(text: &str) -> Option<Self> {
        Self::from_str_radix(text, 16).ok()
    }
}

impl Hex for u32 {
    fn from_hex(text: &str) -> Option<Self> {
        Self::from_str_radix(text, 16).ok()
    }
}

impl Hex for u64 {
    fn from_hex(text: &str) -> Option<Self> {
        Self::from_str_radix(text, 16).ok()
    }
}

impl Hex for u8 {
    fn from_hex(text: &str) -> Option<Self> {
        Self::from_str_radix(text, 16).ok()
    }
}

impl Placement {
    /// No more children: what waits goes where `single` would send it.
    pub fn end(&mut self) -> Pending {
        self.take()
    }

    /// A list item arrives: what waits joins its list, before it.
    pub fn item(&mut self) -> Vec<Node> {
        let pending = mem::take(&mut self.pending);
        self.known += 1;
        self.last_was_item = true;
        pending
    }

    /// A declared single child arrives: what waits joins the list when the
    /// declared child before it was a list item, else becomes `Extra`s after
    /// the declared children read so far.
    pub fn single(&mut self) -> Pending {
        let pending = self.take();
        self.known += 1;
        self.last_was_item = false;
        pending
    }

    fn take(&mut self) -> Pending {
        let pending = mem::take(&mut self.pending);
        if self.last_was_item {
            Pending::List(pending)
        } else {
            let after = self.known;
            Pending::Extra(
                pending
                    .into_iter()
                    .map(|node| Extra { after, node })
                    .collect(),
            )
        }
    }

    /// An unknown node: it waits for the next declared child. Whitespace-only
    /// text is dropped, as the layouts of D29 write it.
    pub fn unknown(&mut self, node: Node) {
        if !is_whitespace_text(&node) {
            self.pending.push(node);
        }
    }
}

impl<'element> Reader<'element> {
    /// The declared child `element` is when one of the positions from the
    /// last one read on takes it: the first that does, a single position only
    /// while it holds no child and an `xsd:all` list only while it lacks the tag.
    fn child(&mut self, node: &'element Node) {
        let Node::Element(element) = node else {
            self.placement.unknown(node.clone());
            return;
        };
        let tag = (known(&element.name.namespace), element.name.local.as_str());
        let base = self.shape.attributes.len();
        for (index, slot) in self.shape.slots.iter().enumerate().skip(self.cursor) {
            if !slot.accepts(element) {
                continue;
            }
            let taken = match slot.occurs {
                SlotOccurs::Items | SlotOccurs::List => false,
                SlotOccurs::Once => self.once.iter().any(|(position, held)| {
                    *position == index && held.0 == tag.0 && held.1 == tag.1
                }),
                SlotOccurs::Single => self.filled.get(index).copied().unwrap_or(false),
            };
            if taken {
                continue;
            }
            self.cursor = index;
            if let Some(filled) = self.filled.get_mut(index) {
                *filled = true;
            }
            if matches!(slot.occurs, SlotOccurs::Items | SlotOccurs::Once) {
                for waiting in self.placement.item() {
                    self.queue.push_back((base + index, Field::Node(waiting)));
                }
                self.last_items = Some(index);
                if let (SlotOccurs::Once, Some(namespace)) = (slot.occurs, slot.tag_of(element)) {
                    self.once.push((index, namespace));
                }
            } else {
                let pending = self.placement.single();
                self.pending(pending);
            }
            self.queue
                .push_back((base + index, Field::Element(element)));
            return;
        }
        self.placement.unknown(node.clone());
    }

    /// The unknown content: attributes the shape does not declare, children
    /// at positions no list covers, and the node's own namespace declarations.
    #[must_use]
    pub fn finish(mut self) -> Unknown {
        while self.next().is_some() {}
        self.unknown
    }

    /// What `element`'s reader is given, by `shape`.
    #[must_use]
    pub fn new(element: &'element Element, shape: &'static Shape) -> Self {
        Self {
            attributes: element.attributes.iter(),
            children: element.children.iter(),
            cursor: 0,
            element,
            ended: false,
            filled: vec![false; shape.slots.len()],
            last_items: None,
            once: Vec::new(),
            placement: Placement::default(),
            queue: VecDeque::new(),
            shape,
            unknown: Unknown {
                attributes: Vec::new(),
                children: Vec::new(),
                namespace_declarations: element.namespace_declarations.clone(),
            },
        }
    }

    fn pending(&mut self, pending: Pending) {
        match pending {
            Pending::Extra(extra) => self.unknown.children.extend(extra),
            Pending::List(nodes) => {
                let base = self.shape.attributes.len();
                let position = self.last_items.unwrap_or_default();
                for node in nodes {
                    self.queue.push_back((base + position, Field::Node(node)));
                }
            }
        }
    }

    /// Simple content: its text and CDATA, joined, and every other node kept
    /// after it.
    fn text(&mut self) -> String {
        let mut text = String::new();
        for node in &self.element.children {
            if let Node::CData(part) | Node::Text(part) = node {
                text.push_str(part);
            } else {
                self.unknown.children.push(Extra {
                    after: 0,
                    node: node.clone(),
                });
            }
        }
        text
    }
}

impl<'element> Iterator for Reader<'element> {
    type Item = (usize, Field<'element>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(item) = self.queue.pop_front() {
                return Some(item);
            }
            if let Some(attribute) = self.attributes.next() {
                let declared = self
                    .shape
                    .attributes
                    .iter()
                    .position(|tag| is_tag(*tag, &attribute.name));
                if let Some(index) = declared {
                    return Some((index, Field::Text(attribute.value.clone())));
                }
                self.unknown.attributes.push(attribute.clone());
                continue;
            }
            if self.ended {
                return None;
            }
            if self.shape.text {
                self.ended = true;
                let text = self.text();
                return Some((self.shape.attributes.len(), Field::Text(text)));
            }
            if let Some(node) = self.children.next() {
                self.child(node);
                continue;
            }
            self.ended = true;
            let pending = self.placement.end();
            self.pending(pending);
        }
    }
}

impl Rebase {
    /// The place, in the rebuilt node, of an `Extra` placed after the
    /// `after`-th original declared child.
    #[must_use]
    pub fn place(&self, after: usize) -> usize {
        self.counts.iter().take(after).sum::<usize>() + after.saturating_sub(self.counts.len())
    }
}

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

impl Slot {
    /// Whether the position takes `element`.
    fn accepts(&self, element: &Element) -> bool {
        match self.accept {
            Accepted::Enum(accept) => {
                accept.tags.iter().any(|tag| is_tag(*tag, &element.name))
                    || accept
                        .any
                        .is_some_and(|wildcard| wildcard.allows(&element.name.namespace))
            }
            Accepted::Tag(tag) => is_tag(tag, &element.name),
        }
    }

    /// A single position of an enum type.
    #[must_use]
    pub const fn choice(accept: &'static Accept) -> Self {
        Self {
            accept: Accepted::Enum(accept),
            occurs: SlotOccurs::Single,
        }
    }

    /// A list of an enum type: unknown nodes among its items join it.
    #[must_use]
    pub const fn items(accept: &'static Accept) -> Self {
        Self {
            accept: Accepted::Enum(accept),
            occurs: SlotOccurs::Items,
        }
    }

    /// A list of one struct type.
    #[must_use]
    pub const fn list(tag: Tag) -> Self {
        Self {
            accept: Accepted::Tag(tag),
            occurs: SlotOccurs::List,
        }
    }

    /// The list of an `xsd:all` type: each tag at most once.
    #[must_use]
    pub const fn once(accept: &'static Accept) -> Self {
        Self {
            accept: Accepted::Enum(accept),
            occurs: SlotOccurs::Once,
        }
    }

    /// A single position of one struct type.
    #[must_use]
    pub const fn single(tag: Tag) -> Self {
        Self {
            accept: Accepted::Tag(tag),
            occurs: SlotOccurs::Single,
        }
    }

    /// The declared tag `element` is, for an `xsd:all` list.
    fn tag_of(&self, element: &Element) -> Option<Tag> {
        let Accepted::Enum(accept) = self.accept else {
            return None;
        };
        accept
            .tags
            .iter()
            .find(|tag| is_tag(**tag, &element.name))
            .copied()
    }
}

impl<'out, 'sink, 'node> Through<'out, 'sink, 'node> {
    /// Write the declared attribute at `index` of the shape, when it has a
    /// value; every attribute comes before the first child.
    pub fn attribute<V>(&mut self, index: usize, value: &V)
    where
        V: AttributeValue,
    {
        if let (Some(text), Some(tag)) = (value.text(), self.shape.attributes.get(index)) {
            self.out
                .scope()
                .attribute(&mut self.element, tag.0.map(uri), tag.1, text);
        }
    }

    /// Write a declared child field of a struct type: each node written
    /// through after the `Extra`s placed before it.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn child<F>(&mut self, field: &F, ctx: &mut dyn Context) -> io::Result<()>
    where
        F: Nodes,
        F::Node: WriteThrough,
    {
        self.open()?;
        for node in field.nodes() {
            self.place.before_known(ctx, self.out)?;
            node.write_through(ctx, self.out)?;
        }
        Ok(())
    }

    /// Write a single declared child of an enum type: written through after
    /// the `Extra`s placed before it, unless it is an `Other` node.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn choice<F>(&mut self, field: &F, ctx: &mut dyn Context) -> io::Result<()>
    where
        F: Nodes,
        F::Node: Item + WriteThrough,
    {
        self.open()?;
        for node in field.nodes() {
            if node.is_declared() {
                self.place.before_known(ctx, self.out)?;
            }
            node.write_through(ctx, self.out)?;
        }
        Ok(())
    }

    /// Write the `Extra`s placed after the last declared child, and the end
    /// tag; an empty tag when nothing was written into the element.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn finish(mut self, ctx: &mut dyn Context) -> io::Result<()> {
        self.open()?;
        self.place.finish(ctx, self.out)?;
        self.out.end(&qualified(&self.element.name))
    }

    /// Write a list of an enum type: for each item, after the `Extra`s placed
    /// before it, its list write hook, then its list hook, whose items are
    /// written as they are, then the item written through (D37).
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn items<F>(&mut self, field: &F, ctx: &mut dyn Context) -> io::Result<()>
    where
        F: Nodes,
        F::Node: Item + WriteThrough,
    {
        self.open()?;
        for item in field.nodes() {
            if item.is_declared() {
                self.place.before_known(ctx, self.out)?;
            }
            if item.write_items(ctx, self.out)? == Written::Yes {
                continue;
            }
            if let Some(spliced) = item.items(ctx) {
                for node in &spliced {
                    node.write_through(&mut Unchanged, self.out)?;
                }
            } else {
                item.write_through(ctx, self.out)?;
            }
        }
        Ok(())
    }

    /// Start writing a node of `shape` with the unknown content it keeps:
    /// its element takes the prefix `out` binds to its namespace, or declares
    /// the table's.
    #[must_use]
    pub fn new(
        out: &'out mut PartWriter<'sink>,
        shape: &'static Shape,
        unknown: &'node Unknown,
    ) -> Self {
        let (namespace, local) = shape.name;
        let element = out.scope().element(
            namespace.map_or("", uri),
            local,
            &unknown.namespace_declarations,
        );
        Self {
            element,
            opened: false,
            out,
            place: Place::new(unknown),
            shape,
            unknown,
        }
    }

    /// Hand the start tag to the part writer, which holds it back until the
    /// element's first content, once every attribute is known.
    fn open(&mut self) -> io::Result<()> {
        if !self.opened {
            self.opened = true;
            self.element
                .attributes
                .extend(self.unknown.attributes.iter().cloned());
            self.out.open(&self.element)?;
        }
        Ok(())
    }

    /// Write simple content's text, before the nodes kept after it.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn text<T>(&mut self, text: &T) -> io::Result<()>
    where
        T: AsRef<str> + ?Sized,
    {
        self.open()?;
        let value = text.as_ref();
        if value.is_empty() {
            return Ok(());
        }
        self.out.text(value)
    }
}

impl Transform for Element {
    fn transform(&self, ctx: &mut dyn Context) -> Self {
        ctx.element(self)
            .unwrap_or_else(|| self.transform_children(ctx))
    }

    fn transform_children(&self, ctx: &mut dyn Context) -> Self {
        Self {
            attributes: self.attributes.clone(),
            children: self
                .children
                .iter()
                .map(|child| child.transform(ctx))
                .collect(),
            name: self.name.clone(),
            namespace_declarations: self.namespace_declarations.clone(),
        }
    }
}

impl Transform for Node {
    /// An element goes to `Context::element`; any other node comes back as it
    /// is.
    fn transform(&self, ctx: &mut dyn Context) -> Self {
        if let Self::Element(element) = self {
            Self::Element(element.transform(ctx))
        } else {
            self.clone()
        }
    }

    fn transform_children(&self, ctx: &mut dyn Context) -> Self {
        if let Self::Element(element) = self {
            Self::Element(element.transform_children(ctx))
        } else {
            self.clone()
        }
    }
}

impl Visit for Node {
    fn visit(&self, visitor: &mut dyn Visitor) {
        if let Self::Element(element) = self
            && visitor.element(element) == Descend::Yes
        {
            self.visit_children(visitor);
        }
    }

    fn visit_children(&self, visitor: &mut dyn Visitor) {
        if let Self::Element(element) = self {
            for child in &element.children {
                child.visit(visitor);
            }
        }
    }
}

impl<'node> Walk<'node> {
    /// Show the visitor the `Extra`s placed before the next declared child.
    fn before_known(&mut self, visitor: &mut dyn Visitor) {
        let known = self.known;
        for extra in self
            .unknown
            .children
            .iter()
            .filter(|extra| extra.after == known)
        {
            extra.node.visit(visitor);
        }
        self.known += 1;
    }

    /// Visit a declared child field of a struct type: each node after the
    /// `Extra`s placed before it.
    pub fn child<F>(&mut self, field: &F, visitor: &mut dyn Visitor)
    where
        F: Nodes,
        F::Node: Visit,
    {
        for node in field.nodes() {
            self.before_known(visitor);
            node.visit(visitor);
        }
    }

    /// Visit a single declared child of an enum type: after the `Extra`s
    /// placed before it, unless it is an `Other` node.
    pub fn choice<F>(&mut self, field: &F, visitor: &mut dyn Visitor)
    where
        F: Nodes,
        F::Node: Item + Visit,
    {
        for node in field.nodes() {
            if node.is_declared() {
                self.before_known(visitor);
            }
            node.visit(visitor);
        }
    }

    /// Visit the `Extra`s placed after the last declared child.
    pub fn finish(self, visitor: &mut dyn Visitor) {
        let known = self.known;
        for extra in self
            .unknown
            .children
            .iter()
            .filter(|extra| extra.after >= known)
        {
            extra.node.visit(visitor);
        }
    }

    /// Visit a list of an enum type: each item after the `Extra`s placed
    /// before it, unless it is an `Other` node.
    pub fn items<F>(&mut self, field: &F, visitor: &mut dyn Visitor)
    where
        F: Nodes,
        F::Node: Item + Visit,
    {
        self.choice(field, visitor);
    }

    #[must_use]
    pub const fn new(unknown: &'node Unknown) -> Self {
        Self { known: 0, unknown }
    }
}

impl Wildcard {
    /// Whether an element of `namespace` is allowed.
    fn allows(self, namespace: &Namespace) -> bool {
        match (self, namespace) {
            (Self::Any, _)
            | (Self::Local, Namespace::None)
            | (Self::Not(_), Namespace::Other(_)) => true,
            (Self::Not(excluded), Namespace::Known(id)) => *id != excluded,
            (Self::Only(ids), Namespace::Known(id)) => ids.contains(id),
            (Self::Local, Namespace::Known(_) | Namespace::Other(_))
            | (Self::Not(_) | Self::Only(_), Namespace::None)
            | (Self::Only(_), Namespace::Other(_)) => false,
        }
    }
}

impl<'scope, 'node> Writer<'scope, 'node> {
    /// Write the declared attribute at `index` of the shape, when it has a value.
    pub fn attribute<V>(&mut self, index: usize, value: &V)
    where
        V: AttributeValue,
    {
        if let (Some(text), Some(tag)) = (value.text(), self.shape.attributes.get(index)) {
            self.scope
                .attribute(&mut self.element, tag.0.map(uri), tag.1, text);
        }
    }

    /// Write a declared child field's nodes.
    pub fn child<F>(&mut self, field: &F)
    where
        F: ChildField,
    {
        field.write(self);
    }

    /// The element: its unknown attributes after the declared ones, and its
    /// children with each `Extra` at its place.
    #[must_use]
    pub fn finish(mut self) -> Element {
        self.element
            .attributes
            .extend(self.unknown.attributes.iter().cloned());
        self.element.children = self.children.finish();
        self.scope.end();
        self.element
    }

    /// Write a declared child's element.
    pub fn known(&mut self, element: Element) {
        self.children.known(element);
    }

    /// Start writing a node of `shape`, with its own namespace declarations
    /// and the unknown content it keeps.
    #[must_use]
    pub fn new(scope: &'scope mut Scope, shape: &'static Shape, unknown: &'node Unknown) -> Self {
        let (namespace, local) = shape.name;
        let element = scope.element(
            namespace.map_or("", uri),
            local,
            &unknown.namespace_declarations,
        );
        Self {
            children: Children::new(&unknown.children),
            element,
            scope,
            shape,
            unknown,
        }
    }

    /// Write a list's `Other` item: it does not count as a declared child.
    pub fn other(&mut self, node: Node) {
        self.children.other(node);
    }

    /// The scope a child's element is written in.
    pub const fn scope(&mut self) -> &mut Scope {
        self.scope
    }

    /// Write simple content's text, before the nodes kept after it.
    pub fn text<T>(&mut self, text: &T)
    where
        T: AsRef<str> + ?Sized,
    {
        let value = text.as_ref();
        if !value.is_empty() {
            self.children.other(Node::Text(value.to_owned()));
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

/// An enumeration member's value: `ValueError` when the text names none.
///
/// # Errors
///
/// The text names no member.
pub fn member<M>(
    members: &Members<M>,
    text: &str,
    simple_type: &'static str,
) -> Result<M, ValueError>
where
    M: Copy,
{
    members
        .iter()
        .find(|(_, spelled, _)| *spelled == text)
        .map(|(found, _, _)| *found)
        .ok_or_else(|| invalid(text, simple_type))
}

/// The text a member stands for.
#[must_use]
pub fn member_text<M>(members: &Members<M>, member: M) -> &'static str
where
    M: Copy + PartialEq,
{
    members
        .iter()
        .find(|(found, _, _)| *found == member)
        .map_or("", |(_, text, _)| text)
}

/// The Office version that brought a member, from SDK 3.5.1's data; `None`
/// for ECMA-376's own.
#[must_use]
pub fn member_version<M>(members: &Members<M>, member: M) -> Option<OfficeVersion>
where
    M: Copy + PartialEq,
{
    members
        .iter()
        .find(|(found, _, _)| *found == member)
        .and_then(|(_, _, version)| *version)
}

/// `ST_OnOff`: `true`, `1`, `on` are true; `false`, `0`, `off` false.
///
/// # Errors
///
/// Any other text.
pub fn on_off(text: &str, simple_type: &'static str) -> Result<bool, ValueError> {
    parse::boolean(text)
        .or(match text {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        })
        .ok_or_else(|| invalid(text, simple_type))
}

/// `xsd:boolean`.
///
/// # Errors
///
/// Text that is not one.
pub fn boolean(text: &str, simple_type: &'static str) -> Result<bool, ValueError> {
    parse::boolean(text).ok_or_else(|| invalid(text, simple_type))
}

/// `xsd:dateTime`.
///
/// # Errors
///
/// Text that is not one.
pub fn date_time(text: &str, simple_type: &'static str) -> Result<DateTime, ValueError> {
    parse::date_time(text).ok_or_else(|| invalid(text, simple_type))
}

/// A floating-point number within `bounds`.
///
/// # Errors
///
/// Text that is no number, or a number out of the bounds.
pub fn double(text: &str, bounds: &Bounds, simple_type: &'static str) -> Result<f64, ValueError> {
    let value: f64 = text
        .parse()
        .map_err(|_error: ParseFloatError| invalid(text, simple_type))?;
    let within = bounds.min_inclusive.is_none_or(|bound| value >= bound)
        && bounds.min_exclusive.is_none_or(|bound| value > bound)
        && bounds.max_inclusive.is_none_or(|bound| value <= bound)
        && bounds.max_exclusive.is_none_or(|bound| value < bound);
    if within {
        Ok(value)
    } else {
        Err(invalid(text, simple_type))
    }
}

/// `bytes` hexadecimal bytes as an unsigned number.
///
/// # Errors
///
/// Text of another length, or with a character that is no hexadecimal digit.
pub fn hex<T>(text: &str, bytes: usize, simple_type: &'static str) -> Result<T, ValueError>
where
    T: Hex,
{
    let valid = text.len() == bytes * 2 && text.bytes().all(|byte| byte.is_ascii_hexdigit());
    valid
        .then(|| T::from_hex(text))
        .flatten()
        .ok_or_else(|| invalid(text, simple_type))
}

/// A whole number from `min` to `max`.
///
/// # Errors
///
/// Text that is no whole number, or one out of the range.
pub fn integer(
    text: &str,
    min: i64,
    max: i64,
    simple_type: &'static str,
) -> Result<i64, ValueError> {
    parse::integer(text)
        .filter(|value| (min..=max).contains(value))
        .ok_or_else(|| invalid(text, simple_type))
}

/// A whole number spelled as one of `values`.
///
/// # Errors
///
/// Text that is none of them.
pub fn integer_in(
    text: &str,
    values: &[&str],
    simple_type: &'static str,
) -> Result<i64, ValueError> {
    values
        .contains(&text)
        .then(|| parse::integer(text))
        .flatten()
        .ok_or_else(|| invalid(text, simple_type))
}

/// The error of a text that is no value of its simple type.
#[must_use]
pub fn invalid(text: &str, simple_type: &'static str) -> ValueError {
    ValueError {
        simple_type,
        text: text.to_owned(),
    }
}

/// The namespace id of a name in the table; `None` for no namespace and for
/// one outside the table.
const fn known(namespace: &Namespace) -> Option<NamespaceId> {
    if let Namespace::Known(id) = namespace {
        Some(*id)
    } else {
        None
    }
}

/// Whether `name` is the tag `tag`.
fn is_tag(tag: Tag, name: &QName) -> bool {
    tag.1 == name.local
        && match (tag.0, &name.namespace) {
            (None, Namespace::None) => true,
            (Some(id), Namespace::Known(found)) => id == *found,
            (None, Namespace::Known(_) | Namespace::Other(_))
            | (Some(_), Namespace::None | Namespace::Other(_)) => false,
        }
}

/// A universal measure, `2.5cm`, matching one of `patterns`.
///
/// # Errors
///
/// Text that matches none of them.
pub fn measure(
    text: &str,
    patterns: &[&str],
    simple_type: &'static str,
) -> Result<Measure, ValueError> {
    patterns
        .iter()
        .any(|pattern| matches_pattern(pattern, text).unwrap_or(false))
        .then(|| parse::measure(text))
        .flatten()
        .ok_or_else(|| invalid(text, simple_type))
}

/// An element's tag as the typed tree matches it; `None` for a namespace the
/// table does not know.
#[must_use]
pub const fn name(element: &Element) -> Option<(Option<NamespaceId>, &str)> {
    let local = element.name.local.as_str();
    match &element.name.namespace {
        Namespace::Known(id) => Some((Some(*id), local)),
        Namespace::None => Some((None, local)),
        Namespace::Other(_) => None,
    }
}

/// A percentage, `50%` being 50.0, matching one of `patterns`.
///
/// # Errors
///
/// Text that matches none of them.
pub fn percentage(
    text: &str,
    patterns: &[&str],
    simple_type: &'static str,
) -> Result<f64, ValueError> {
    patterns
        .iter()
        .any(|pattern| matches_pattern(pattern, text).unwrap_or(false))
        .then(|| parse::decimal_with_suffix(text, "%"))
        .flatten()
        .ok_or_else(|| invalid(text, simple_type))
}

/// The generic node an item of the typed tree writes as: an arm's element
/// with the namespace declarations it needs on its own, or the `Other` node.
#[must_use]
pub fn generic<T>(item: &T) -> Node
where
    T: Write,
{
    static BARE: Shape = Shape {
        attributes: &[],
        name: (None, ""),
        slots: &[],
        text: false,
    };
    let mut scope = Scope::default();
    let unknown = Unknown::default();
    let mut writer = Writer::new(&mut scope, &BARE, &unknown);
    item.write_into(&mut writer);
    let mut bare = writer.finish();
    if bare.children.len() == 1
        && let Some(node) = bare.children.pop()
    {
        return node;
    }
    Node::Element(bare)
}

/// The node of `T` an element holds.
#[must_use]
pub fn read<T>(element: &Element) -> T
where
    T: Read,
{
    T::read(element)
}

/// A string within its length and pattern facets.
///
/// # Errors
///
/// Text out of the facets.
pub fn text<'text>(
    text: &'text str,
    facets: &Facets,
    simple_type: &'static str,
) -> Result<&'text str, ValueError> {
    let length = text.chars().count();
    let within = facets.length.is_none_or(|bound| length == bound)
        && facets.min_length.is_none_or(|bound| length >= bound)
        && facets.max_length.is_none_or(|bound| length <= bound)
        && (facets.patterns.is_empty()
            || facets
                .patterns
                .iter()
                .any(|pattern| matches_pattern(pattern, text).unwrap_or(false)));
    if within {
        Ok(text)
    } else {
        Err(invalid(text, simple_type))
    }
}

/// A tree or list hook's answer about `node`, from the hooks a namespace
/// accessor gave: `None` when it gave none.
pub fn ask<H, T, R, F>(hooks: Option<&mut H>, hook: F, node: &T) -> Option<R>
where
    H: ?Sized,
    F: FnOnce(&mut H, &T) -> Option<R>,
{
    let found = hooks?;
    hook(found, node)
}

/// A write hook's answer about `node`, from the hooks a namespace accessor
/// gave: `Written::No` when it gave none.
///
/// # Errors
///
/// The sink's.
pub fn ask_write<H, T, F>(
    hooks: Option<&mut H>,
    hook: F,
    node: &T,
    out: &mut PartWriter<'_>,
) -> io::Result<Written>
where
    H: ?Sized,
    F: FnOnce(&mut H, &T, &mut PartWriter<'_>) -> io::Result<Written>,
{
    hooks.map_or(Ok(Written::No), |found| hook(found, node, out))
}

/// A visitor hook's answer about `node`, from the hooks a namespace accessor
/// gave: `Descend::Yes` when it gave none.
pub fn show<H, T, F>(hooks: Option<&mut H>, hook: F, node: &T) -> Descend
where
    H: ?Sized,
    F: FnOnce(&mut H, &T) -> Descend,
{
    hooks.map_or(Descend::Yes, |found| hook(found, node))
}

/// A declared child field of a struct type rebuilt with each node
/// transformed; each node counts one declared child in `rebase`.
#[must_use]
pub fn transform_child<F>(field: &F, ctx: &mut dyn Context, rebase: &mut Rebase) -> F
where
    F: Nodes,
    F::Node: Transform,
{
    let rebuilt = field.transform_nodes(ctx);
    rebase.counts.extend(iter::repeat_n(1, field.nodes().len()));
    rebuilt
}

/// A single declared child of an enum type rebuilt with its node transformed.
///
/// A declared node counts one declared child in `rebase`, or none when its
/// hook answers an `Other` item; an `Other` node counts nothing.
#[must_use]
pub fn transform_choice<F>(field: &F, ctx: &mut dyn Context, rebase: &mut Rebase) -> F
where
    F: Nodes,
    F::Node: Item + Transform,
{
    let rebuilt = field.transform_nodes(ctx);
    for (node, became) in field.nodes().iter().zip(rebuilt.nodes()) {
        if node.is_declared() {
            rebase.counts.push(usize::from(became.is_declared()));
        }
    }
    rebuilt
}

/// A list of an enum type rebuilt (D20): each item's list hook first, whose
/// `Some` puts any number of items, none included, in its place, and the
/// item's `transform` otherwise.
///
/// A declared item counts in `rebase` the declared items it became; an
/// `Other` item counts nothing.
pub fn transform_items<T>(items: &[T], ctx: &mut dyn Context, rebase: &mut Rebase) -> Vec<T>
where
    T: Item + Transform,
{
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let start = out.len();
        match item.items(ctx) {
            Some(spliced) => out.extend(spliced),
            None => out.push(item.transform(ctx)),
        }
        if item.is_declared() {
            let became = out
                .iter()
                .skip(start)
                .filter(|node| node.is_declared())
                .count();
            rebase.counts.push(became);
        }
    }
    out
}

/// A node's unknown content rebuilt: each `Extra`'s node transformed through
/// the root `element` hook, the attributes and declarations as they are.
///
/// Each `Extra` is placed, by `rebase`, after what the declared children
/// before it became.
pub fn transform_unknown(unknown: &Unknown, ctx: &mut dyn Context, rebase: &Rebase) -> Unknown {
    Unknown {
        attributes: unknown.attributes.clone(),
        children: unknown
            .children
            .iter()
            .map(|extra| Extra {
                after: rebase.place(extra.after),
                node: extra.node.transform(ctx),
            })
            .collect(),
        namespace_declarations: unknown.namespace_declarations.clone(),
    }
}

/// The Transitional URI of a namespace id.
fn uri(id: NamespaceId) -> &'static str {
    NAMESPACES.get(usize::from(id)).map_or("", |row| row.uri)
}
