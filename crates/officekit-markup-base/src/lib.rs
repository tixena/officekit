//! `officekit-markup-base` is the typed tree's shared runtime, with the namespace modules the
//! cycle of `officekit-markup-core` depends on; `officekit-markup` gathers it under its paths.
//!
//! The crate root is the runtime: where unknown content goes when a node is read and written,
//! and which prefix a written element takes. It holds the reader and writer every generated node
//! goes through, and the parsers its simple-type newtypes call.
//!
//! - [`class_rules`]: what the two conformance classes' reads and writes share.

#![recursion_limit = "256"]
#![allow(
    clippy::exhaustive_structs,
    clippy::exhaustive_enums,
    reason = "core and ext build `Shape`, `Accept`, `Facets` and `Bounds` by struct literal and match the enums their `From` impls convert arm by arm, which `#[non_exhaustive]` forbids across crates"
)]

extern crate alloc;

// The namespace modules, generated in a file of their own: module declarations
// go before the imports, and `include!` places them where it stands.
include!("modules.rs");

pub mod class_rules;

use alloc::collections::VecDeque;
use core::num::ParseFloatError;
use core::{mem, slice};

use officekit_xml::element::{Attribute, Element, NamespaceDeclaration, Node};
use officekit_xml::name::table::NAMESPACES;
use officekit_xml::name::{Namespace, NamespaceId, OfficeVersion, QName, tag_of};
use officekit_xml::values::{DateTime, Measure, ValueError, matches_pattern, parse};
use officekit_xml::write::{Scope, is_whitespace_text};

#[cfg(test)]
mod tests;

/// The tags an enum of the typed tree reads: its arms', and the namespaces a
/// wildcard among them allows.
#[derive(Debug)]
pub struct Accept {
    pub any: Option<Wildcard>,
    pub tags: &'static [Tag],
}

/// The bounds of a floating-point simple type.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
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

/// Counts a node's declared positions as a traversal reaches them: the
/// generated code reaches each one once, in schema order.
#[derive(Debug, Default)]
pub struct Positions {
    next: usize,
}

/// Reads a typed node's attributes and children in document order.
///
/// Each declared one goes to the generated reader by its index in the node's
/// `Shape`; what the schemas do not declare stays at its place.
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

/// How a typed node is read and written: its tag, its declared attributes and
/// child positions in schema order, and whether it holds text.
#[derive(Debug)]
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
    /// One child the schema requires: the reader records its absence.
    Required,
    /// One optional child.
    Single,
}

/// A tag a typed node declares: its namespace's id, `None` when unqualified,
/// and its local name.
pub type Tag = (Option<NamespaceId>, &'static str);

/// What every node carries besides its declared attributes and children.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Unknown {
    /// The positions, in its shape's `slots`, of the required single children
    /// the read element did not hold; a node built in code has none.
    pub absent: Vec<usize>,
    /// Attributes the schemas do not declare, `mc:Ignorable` among them.
    pub attributes: Vec<Attribute>,
    /// Unknown children at positions no list covers.
    pub children: Vec<Extra>,
    pub namespace_declarations: Vec<NamespaceDeclaration>,
}

/// Whether a write hook wrote its node, or leaves it to the tree hook and the
/// node's own writing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Written {
    No,
    Yes,
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
    Only(&'static [NamespaceId]),
}

/// Writes a typed node: its declared attributes in schema order, then the
/// unknown ones, and its declared children in schema order with each list's
/// `Other` items among them and each `Extra` at its place.
#[derive(Debug)]
pub struct Writer<'scope, 'node> {
    children: Children<'node>,
    element: Element,
    positions: Positions,
    scope: &'scope mut Scope,
    shape: &'static Shape,
    /// Whether a single field's node is being written: its `Other` node
    /// takes a declared child's place.
    single: bool,
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

/// A declared child's field as the traversals see it: one node boxed, an
/// optional one, or a list.
pub trait Nodes {
    type Node;

    /// Whether the field is one node equal to its type's default: a required
    /// child the read element did not hold stays so until code sets it.
    fn is_default(&self) -> bool;

    /// The nodes the field holds, in document order.
    fn nodes(&self) -> &[Self::Node];
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

/// A node of the typed tree written at its place: a struct as its element,
/// an enum's item as its arm's element or its `Other` node.
pub trait Write {
    /// Write the node into the element `writer` writes.
    fn write_into(&self, writer: &mut Writer<'_, '_>);
}

impl<'extra> Children<'extra> {
    /// A node at the next declared child's place, after the `Extra`s placed
    /// before it: it counts as a declared child.
    fn declared(&mut self, node: Node) {
        let known = self.known;
        self.out.extend(
            self.extra
                .iter()
                .filter(|extra| extra.after == known)
                .map(|extra| extra.node.clone()),
        );
        self.out.push(node);
        self.known += 1;
    }

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
        self.declared(Node::Element(node));
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

impl Extra {
    /// An unknown node placed after the `after`-th declared child of its
    /// parent.
    #[must_use]
    pub const fn new(after: usize, node: Node) -> Self {
        Self { after, node }
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
        writer.single(self.as_ref());
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
            writer.single(node.as_ref());
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

impl<T> Nodes for Box<T>
where
    T: Default + PartialEq,
{
    type Node = T;

    fn is_default(&self) -> bool {
        **self == T::default()
    }

    fn nodes(&self) -> &[T] {
        slice::from_ref(self)
    }
}

impl<T> Nodes for Option<Box<T>> {
    type Node = T;

    fn is_default(&self) -> bool {
        false
    }

    fn nodes(&self) -> &[T] {
        self.as_deref().map_or(&[], slice::from_ref)
    }
}

impl<T> Nodes for Vec<T> {
    type Node = T;

    fn is_default(&self) -> bool {
        false
    }

    fn nodes(&self) -> &[T] {
        self.as_slice()
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
    /// text is dropped, as both layouts write it.
    pub fn unknown(&mut self, node: Node) {
        if !is_whitespace_text(&node) {
            self.pending.push(node);
        }
    }
}

impl Positions {
    /// The next declared position, now reached.
    pub const fn advance(&mut self) -> usize {
        let position = self.next;
        self.next += 1;
        position
    }

    /// Whether a traversal skips `field`, at the next declared position, now
    /// reached: the read element did not hold its child and the field still
    /// holds the default node.
    pub fn skips<F>(&mut self, absent: &[usize], field: &F) -> bool
    where
        F: Nodes,
    {
        absent.contains(&self.advance()) && field.is_default()
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
                SlotOccurs::Required | SlotOccurs::Single => {
                    self.filled.get(index).copied().unwrap_or(false)
                }
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
    /// at positions no list covers, the node's own namespace declarations,
    /// and the required single positions the element left empty.
    #[must_use]
    pub fn finish(mut self) -> Unknown {
        while self.next().is_some() {}
        self.unknown.absent = self
            .shape
            .slots
            .iter()
            .zip(&self.filled)
            .enumerate()
            .filter(|(_, (slot, filled))| slot.occurs == SlotOccurs::Required && !**filled)
            .map(|(position, _)| position)
            .collect();
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
                absent: Vec::new(),
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

    /// An optional single position of an enum type.
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

    /// A required single position of one struct type.
    #[must_use]
    pub const fn required(tag: Tag) -> Self {
        Self {
            accept: Accepted::Tag(tag),
            occurs: SlotOccurs::Required,
        }
    }

    /// A required single position of an enum type.
    #[must_use]
    pub const fn required_choice(accept: &'static Accept) -> Self {
        Self {
            accept: Accepted::Enum(accept),
            occurs: SlotOccurs::Required,
        }
    }

    /// An optional single position of one struct type.
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

impl Unknown {
    /// Unknown content with these absent positions, attributes, children, and declarations.
    #[must_use]
    pub const fn new(
        absent: Vec<usize>,
        attributes: Vec<Attribute>,
        children: Vec<Extra>,
        namespace_declarations: Vec<NamespaceDeclaration>,
    ) -> Self {
        Self {
            absent,
            attributes,
            children,
            namespace_declarations,
        }
    }
}

impl Wildcard {
    /// Whether an element of `namespace` is allowed.
    fn allows(self, namespace: &Namespace) -> bool {
        match self {
            Self::Any => true,
            Self::Local => *namespace == Namespace::None,
            Self::Not(excluded) => {
                matches!(namespace, Namespace::Other(_))
                    || matches!(namespace, Namespace::Known(id) if *id != excluded)
            }
            Self::Only(ids) => matches!(namespace, Namespace::Known(id) if ids.contains(id)),
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

    /// Write a declared child field's nodes, none while it holds the default
    /// node of a child the read element did not hold.
    pub fn child<F>(&mut self, field: &F)
    where
        F: ChildField + Nodes,
    {
        if !self.positions.skips(&self.unknown.absent, field) {
            field.write(self);
        }
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
            positions: Positions::default(),
            scope,
            shape,
            single: false,
            unknown,
        }
    }

    /// Write an enum's `Other` node: a list's does not count as a declared
    /// child, a single field's takes a declared child's place.
    pub fn other(&mut self, node: Node) {
        if self.single {
            self.children.declared(node);
        } else {
            self.children.other(node);
        }
    }

    /// The scope a child's element is written in.
    pub const fn scope(&mut self) -> &mut Scope {
        self.scope
    }

    /// Write a single field's node, whose `Other` node takes a declared
    /// child's place.
    fn single<T>(&mut self, node: &T)
    where
        T: Write,
    {
        self.single = true;
        node.write_into(self);
        self.single = false;
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
    ValueError::new(simple_type, text.to_owned())
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
    tag.1 == name.local && name.namespace == tag.0.map_or(Namespace::None, Namespace::Known)
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
    tag_of(&element.name)
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

/// The Transitional URI of a namespace id.
#[must_use]
pub fn uri(id: NamespaceId) -> &'static str {
    NAMESPACES.get(usize::from(id)).map_or("", |row| row.uri)
}
