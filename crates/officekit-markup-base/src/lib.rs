//! `officekit-markup-base` is the runtime of officekit's typed tree, with the namespaces the
//! rest of the tree depends on.
//!
//! Most code does not name this crate. `officekit-markup` gathers it with `officekit-markup-core`
//! and its own namespaces under one path per namespace, `officekit_markup::<namespace>`, and
//! `officekit` builds on `officekit-markup`. Use this crate
//! directly to write a reader, a writer or a simple-type check by hand.
//!
//! # The typed tree
//!
//! An Office Open XML part is XML. `officekit_xml::element::Element` holds it as a generic tree.
//! The typed tree holds the same part as Rust structs and enums: one struct per element type
//! of the schemas, one enum per list of alternative children, and one newtype per simple type.
//! A namespace is one module. This crate has [`document_properties_custom`],
//! [`document_properties_extended`], [`bibliography`], [`common_simple_types`], the VML
//! namespaces and a few more; each module lists its tags, readers and writers.
//!
//! # Reading and writing a node
//!
//! Each node type implements [`Read`] and [`Write`]. Each namespace module also has a `read`
//! module with one function per struct, `read::<node>(&Element) -> Node`, and a `write` module
//! with `write::<node>(&Node, &mut Scope) -> Element`. The generic [`read`] function calls
//! [`Read::read`].
//!
//! A reader builds a [`Reader`] from the element and the node's [`Shape`]. The reader hands
//! out each declared attribute and child as a [`Field`] with its index in the shape. A writer
//! builds a [`Writer`] from the same shape and writes the declared attributes and children in
//! schema order.
//!
//! A node is also read from a part's events, with no generic tree of the part:
//! `read::<node>_from_events(start, events)` takes the node's start tag and an [`EventSource`],
//! as [`Events`] over a part reader, and reads each declared child from the events in turn. An
//! [`Item`] is what either read starts from.
//!
//! # Content the schemas do not declare
//!
//! A part can hold more than the schemas of one namespace declare: elements of a newer Office
//! version, vendor extensions, comments, or attributes of another namespace. A read keeps all
//! of it in place, so a write gives it back.
//!
//! - Every struct has a field `unknown` of type `Option<Box<Unknown>>`, `None` when the read
//!   found nothing to keep. An [`Unknown`] holds the attributes the shape does not declare, the
//!   namespace declarations of the element, and unknown children at positions no list covers,
//!   each as an [`Extra`] that names the declared child it follows. A node's `unknown()` reads it
//!   and `unknown_mut()` puts an empty record in place to change.
//! - Every enum that models a list of alternative children has an `Other(Box<Node>)` arm. An
//!   unknown node between list items joins the list there, in order.
//! - Whitespace-only text between children is dropped.
//! - A required single child the element lacked is recorded in `unknown.absent`, and the
//!   writer leaves it out while the field still holds its default.
//! - An element read under another name than its node's, as Word's `wps:wsp` for `wp:wsp`, keeps
//!   that name in `unknown.name`, and the writer gives it back.
//!
//! Simple types keep what was read. A simple type that lists its values or unions other types
//! is an enum: its member when the text is that member's canonical spelling, and the text as read
//! otherwise, so a write gives back every spelling it read. Any other simple type keeps its text.
//! [`Spelling`] gives the text a value is written as. Calling `.value()` checks a kept text later
//! and gives a `ValueError` for text that does not fit, so a read never fails inside a node.
//!
//! # Hooks
//!
//! [`Descend`] and [`Written`] are the answers of the hooks that `officekit-markup` defines for
//! every namespace. `officekit_strict::traversal` calls the hooks while it transforms, visits or
//! streams a tree.
//!
//! # Example
//!
//! Read a list of custom document properties that holds a node the schemas do not declare,
//! change a name, and write it again. The unknown `x:note` element keeps its place between the
//! two properties.
//!
//! ```
//! use officekit_markup_base::document_properties_custom::read::custom_file_properties;
//! use officekit_markup_base::document_properties_custom::write;
//! use officekit_xml::element::Document;
//! use officekit_xml::parse::parse;
//! use officekit_xml::write::{Layout, Scope, WriteOptions};
//!
//! const OP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties";
//!
//! let xml = format!(
//!     "<op:Properties xmlns:op=\"{OP}\" xmlns:x=\"urn:x\">\
//!      <op:property name=\"Draft\"/><x:note/><op:property name=\"Final\"/></op:Properties>"
//! );
//! let root = parse(xml.as_bytes()).expect("well-formed XML").root;
//! let mut properties = custom_file_properties(&root);
//! assert_eq!(properties.custom_file_property.len(), 2);
//! let unknown = properties.unknown().expect("the note is kept");
//! assert_eq!(unknown.children.len(), 1);
//! assert_eq!(unknown.children[0].after, 1);
//!
//! properties.custom_file_property[1].name = Some("Signed".into());
//! let element = write::custom_file_properties(&properties, &mut Scope::default());
//! let mut part = Document::new("", element);
//! part.declaration = None;
//! part.prolog.clear();
//! let options = WriteOptions::new(Layout::Compact, false);
//! let text = String::from_utf8(part.to_xml(options)).expect("UTF-8");
//! assert_eq!(text, xml.replace("Final", "Signed"));
//! ```
//!
//! # Features
//!
//! - `binary-format`: every type also derives the `rkyv` archive traits, for officekit's
//!   binary form.
//!
//! # Modules
//!
//! - [`class_rules`][]: what the Strict and Transitional classes share when they read and write a
//!   node.

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
mod user_data;

pub use user_data::{UserData, UserDataSlot};

use alloc::borrow::Cow;
use alloc::collections::VecDeque;
use alloc::vec;
use core::cell::Cell;
use core::fmt::{self, Debug, Display, Formatter, Write as _};
use core::{mem, slice};

use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Attribute, Element, NamespaceDeclaration, Node, XmlError};
use officekit_xml::name::table::NAMESPACES;
use officekit_xml::name::{Namespace, NamespaceId, OfficeVersion, QName, tag_of};
use officekit_xml::stream::reader::{PartEvent, ReadEvents};
use officekit_xml::values::{DateTime, Measure, Patterns, ValueError, parse};
use officekit_xml::write::{Scope, is_whitespace_text};

#[cfg(test)]
mod tests;

/// The `From` of an enum's rkyv mirror too long to write out: the match that
/// moves each variant of the mirror `$from` into the variant of `$to` of the
/// same name.
#[cfg(feature = "binary-format")]
#[macro_export]
macro_rules! mirror_into {
    ($item:expr, $from:ident => $to:ident; $($variant:ident),+ $(,)?) => {
        match $item {
            $($from::$variant(arm) => $to::$variant(arm),)+
        }
    };
}

/// The `From` of a struct's rkyv mirror too long to write out: the struct
/// literal that moves each field of the mirror `$item` into the field of the
/// same name.
#[cfg(feature = "binary-format")]
#[macro_export]
macro_rules! mirror_fields {
    ($item:expr; $($field:ident),+ $(,)?) => {{
        let item = $item;
        Self { $($field: item.$field),+ }
    }};
}

/// rkyv's `Archive`, `Serialize` and `Deserialize` of a simple type that keeps its member: it
/// archives as its spelling, the string the type held before it kept a member, and a read
/// parses that string again.
#[cfg(feature = "binary-format")]
#[macro_export]
macro_rules! archive_as_spelling {
    ($name:ty) => {
        impl ::rkyv::Archive for $name {
            type Archived = ::rkyv::string::ArchivedString;
            type Resolver = ::rkyv::string::StringResolver;

            fn resolve(&self, resolver: Self::Resolver, out: ::rkyv::Place<Self::Archived>) {
                ::rkyv::string::ArchivedString::resolve_from_str(
                    &$crate::archived_spelling(self),
                    resolver,
                    out,
                );
            }
        }

        impl<S> ::rkyv::Serialize<S> for $name
        where
            S: ::rkyv::rancor::Fallible + ?Sized,
            S::Error: ::rkyv::rancor::Source,
            str: ::rkyv::SerializeUnsized<S>,
        {
            fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
                ::rkyv::string::ArchivedString::serialize_from_str(
                    &$crate::archived_spelling(self),
                    serializer,
                )
            }
        }

        impl<D> ::rkyv::Deserialize<$name, D> for ::rkyv::string::ArchivedString
        where
            D: ::rkyv::rancor::Fallible + ?Sized,
        {
            fn deserialize(&self, _: &mut D) -> Result<$name, D::Error> {
                Ok(<$name>::from(self.as_str().to_owned()))
            }
        }
    };
}

/// The `into_user_data` of an enum too long to write out.
///
/// It is the match that converts each arm of `$item`, the plain ones as they are and the `boxed`
/// ones through their box, into the arm of the same name of `$enum`, and moves `Other`.
#[macro_export]
macro_rules! into_user_data_match {
    ($item:expr, $enum:ident; plain [$($plain:ident,)*] boxed [$($boxed:ident,)*]) => {
        match $item {
            $($enum::$plain(node) => $enum::$plain(node.into_user_data()),)*
            $($enum::$boxed(node) => $enum::$boxed(Box::new((*node).into_user_data())),)*
            $enum::Other(node) => $enum::Other(node),
        }
    };
}

/// The tags an enum of the typed tree reads: its arms', and the namespaces a
/// wildcard among them allows.
#[derive(Debug)]
pub struct Accept {
    /// The namespaces an `xsd:any` wildcard among the arms allows; `None` when there is none.
    pub any: Option<Wildcard>,
    /// The tag of each arm.
    pub tags: &'static [Tag],
}

/// The bounds of a floating-point simple type.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    /// The value must be below this.
    pub max_exclusive: Option<f64>,
    /// The value must be at most this.
    pub max_inclusive: Option<f64>,
    /// The value must be above this.
    pub min_exclusive: Option<f64>,
    /// The value must be at least this.
    pub min_inclusive: Option<f64>,
}

/// What a declared child position takes: an enum's tags, or one struct's tag and the other
/// tags its child is also written with.
#[derive(Debug)]
enum Accepted {
    Enum(&'static Accept),
    Tag(Tag, &'static [Tag]),
}

/// Writes declared children in schema order, with each `Extra` before the
/// declared child its place names.
#[derive(Debug)]
pub struct Children<'extra> {
    extras: &'extra [Extra],
    known: usize,
    out: Vec<Node>,
}

/// Whether a visitor walks into the children of the node it was just shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Descend {
    /// Skip the children of the node.
    No,
    /// Walk into the children of the node.
    Yes,
}

/// A node the schemas do not declare, at a position no list covers: after
/// the `after`-th declared child of its parent, 0 being before the first.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "binary-format",
    derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)
)]
#[cfg_attr(feature = "binary-format", rkyv(attr(non_exhaustive)))]
#[non_exhaustive]
pub struct Extra {
    /// How many declared children of the parent come before the node.
    pub after: usize,
    /// The unknown node.
    pub node: Node,
}

/// The length and pattern facets of a string simple type.
#[derive(Debug, Default)]
pub struct Facets {
    /// The exact length, in characters.
    pub length: Option<usize>,
    /// The most characters.
    pub max_length: Option<usize>,
    /// The fewest characters.
    pub min_length: Option<usize>,
    /// The text matches one of them whole, when there are any.
    pub patterns: Patterns,
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
    /// A declared child element the reader owns: one read from events whole.
    Owned(Element),
    /// A declared child's start tag; its content is the next events of the source, up to its end.
    Start(Element, &'element dyn EventSource),
    /// A declared attribute's value, or the text of simple content.
    Text(String),
}

/// A part's events as a typed read takes them: an [`EventSource`] over a [`ReadEvents`].
///
/// The first error the source gives is kept, and from there the read sees the part end;
/// [`Events::finish`] returns it.
pub struct Events<'source> {
    /// How deep in elements the events given so far stand.
    depth: Cell<usize>,
    error: Cell<Option<XmlError>>,
    /// How many elements started outside every other.
    roots: Cell<usize>,
    /// `None` after an error.
    source: Cell<Option<&'source mut dyn ReadEvents>>,
}

/// What a node read from events takes them from: the events after the start tag it was given.
pub trait EventSource: Debug {
    /// The element `start` began, read whole up to its end; `start` as it is once the source
    /// failed.
    #[must_use = "this reads the element; dropping it skips it"]
    fn element(&self, start: Element) -> Element;

    /// The next event; `None` at the end of the part, or once the source failed.
    #[must_use = "this reads the event; dropping it skips it"]
    fn event(&self) -> Option<PartEvent>;
}

/// What a node is read from: a generic element, borrowed or owned, or an element's start tag
/// and the events after it.
#[derive(Debug)]
#[non_exhaustive]
pub enum Item<'element> {
    /// The start tag, and the events of its content and end.
    Events(Element, &'element dyn EventSource),
    /// An element read whole.
    Owned(Element),
    /// An element of a tree.
    Tree(&'element Element),
}

/// The members of an enumeration newtype: each member, the text it stands
/// for, and the Office version that brought it (`None` for ECMA-376's own).
pub type Members<M> = [(M, &'static str, Option<OfficeVersion>)];

/// Where pending unknown nodes go: into the list whose item is next to them,
/// or into the node's `unknown.children`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Pending {
    /// The nodes become `Extra`s of the node's `unknown.children`.
    Extra(Vec<Extra>),
    /// The nodes join the list as `Other` items.
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
    attributes: Attributes<'element>,
    content: Content<'element>,
    /// The position of the last declared child read.
    cursor: usize,
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
    /// The declared attributes, in schema order.
    pub attributes: &'static [Tag],
    /// The tag of the node's element.
    pub name: Tag,
    /// The declared child positions, in schema order.
    pub slots: &'static [Slot],
    /// Simple content: the element's text is its value.
    pub text: bool,
}

impl Shape {
    /// The index in `slots` of the first position that takes `element`; `None` when no declared
    /// position does.
    #[must_use = "this only returns the value"]
    pub fn slot_of(&self, element: &Element) -> Option<usize> {
        self.slots.iter().position(|slot| slot.accepts(element))
    }
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

/// What every node carries besides its declared attributes and children: the content a read
/// found that the schemas do not declare, kept so a write gives it back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "binary-format",
    derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)
)]
#[cfg_attr(feature = "binary-format", rkyv(attr(non_exhaustive)))]
#[non_exhaustive]
pub struct Unknown {
    /// The positions, in its shape's `slots`, of the required single children
    /// the read element did not hold; a node built in code has none.
    pub absent: Vec<usize>,
    /// Attributes the schemas do not declare, `mc:Ignorable` among them.
    pub attributes: Vec<Attribute>,
    /// Unknown children at positions no list covers.
    pub children: Vec<Extra>,
    /// The name of the element the node was read from when it is not its shape's own, as Word's
    /// `wps:wsp` for the standard's `wp:wsp` (ECMA-376 Part 1, 20.4.2.37 to 20.4.2.42); a write
    /// gives it back. `None` for a node built in code or read under its own name.
    pub name: Option<Box<QName>>,
    /// The `xmlns` declarations the element carried, written back on it.
    pub namespace_declarations: Vec<NamespaceDeclaration>,
}

/// Whether a write hook wrote its node, or leaves it to the tree hook and the
/// node's own writing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Written {
    /// The hook wrote nothing; the node is written the usual way.
    No,
    /// The hook wrote the node.
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
    /// Only these namespaces.
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

/// The attributes a reader goes through.
#[derive(Debug)]
enum Attributes<'element> {
    Borrowed(slice::Iter<'element, Attribute>),
    Owned(vec::IntoIter<Attribute>),
}

/// The children a reader goes through.
#[derive(Debug)]
enum Content<'element> {
    Borrowed(slice::Iter<'element, Node>),
    Events(&'element dyn EventSource),
    Owned(vec::IntoIter<Node>),
}

/// A child a reader reached.
enum Child<'element> {
    Borrowed(&'element Node),
    Owned(Node),
    Start(Element, &'element dyn EventSource),
}

/// A declared attribute's field: its text, when it has one.
pub trait AttributeValue {
    /// The text the writer both classes share writes, Transitional's spelling, which a class's
    /// rules then respell; `None` leaves the attribute out.
    fn text(&self) -> Option<Cow<'_, str>>;
}

/// A declared child's field: one node, boxed, an optional one, or a list.
pub trait ChildField {
    /// Take a declared child, or an unknown node read at the field's position.
    fn take(&mut self, field: Field<'_>);

    /// Write the field's nodes at their place.
    fn write(&self, writer: &mut Writer<'_, '_>);
}

/// A value of a simple type, written as text: an attribute's value or an element's simple content.
pub trait Spelling {
    /// The text of the value in `class`.
    fn spelling(&self, class: ConformanceClass) -> Cow<'_, str>;
}

/// An unsigned number a fixed count of hexadecimal bytes reads into.
pub trait Hex: Sized {
    /// The number `text` spells in hexadecimal digits.
    fn from_hex(text: &str) -> Option<Self>;
}

/// A declared child's field as the traversals see it: one node boxed, an
/// optional one, or a list.
pub trait Nodes {
    /// The type of the nodes the field holds.
    type Node;

    /// Whether the field is one node equal to its type's default: a required
    /// child the read element did not hold stays so until code sets it.
    fn is_default(&self) -> bool;

    /// The nodes the field holds, in document order.
    fn nodes(&self) -> &[Self::Node];
}

/// A node of the typed tree read from a generic element, or from a part's events.
pub trait Read: Sized {
    /// The `Other` item holding a node the schemas do not declare, for an
    /// enum; `None` for a struct, which has no such item.
    #[must_use = "this only returns the value"]
    fn other(_node: Node) -> Option<Self> {
        None
    }

    /// The node `element` holds.
    #[must_use = "the node read is returned; nothing else keeps it"]
    fn read(element: &Element) -> Self {
        Self::read_item(Item::Tree(element))
    }

    /// The node `start` begins, read from the events after it up to its end.
    #[must_use = "the node read is returned; nothing else keeps it"]
    fn read_events(start: Element, events: &dyn EventSource) -> Self {
        Self::read_item(Item::Events(start, events))
    }

    /// The node `item` holds.
    fn read_item(item: Item<'_>) -> Self;
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
            self.extras
                .iter()
                .filter(|extra| extra.after == known)
                .map(|extra| extra.node.clone()),
        );
        self.out.push(node);
        self.known += 1;
    }

    /// The children written so far, then every `Extra` whose place is past
    /// the last declared child.
    #[must_use = "the value moves into the one returned"]
    pub fn finish(mut self) -> Vec<Node> {
        let known = self.known;
        self.out.extend(
            self.extras
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

    /// An empty list of children, to be written around these `Extra`s.
    #[must_use = "this only builds the value"]
    pub const fn new(extra: &'extra [Extra]) -> Self {
        Self {
            extras: extra,
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
    #[must_use = "this only builds the value"]
    pub const fn new(after: usize, node: Node) -> Self {
        Self { after, node }
    }
}

impl<'element> Field<'element> {
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

    /// What a declared child is read from; `None` for a node or text.
    fn into_item(self) -> Option<Item<'element>> {
        match self {
            Self::Element(element) => Some(Item::Tree(element)),
            Self::Owned(element) => Some(Item::Owned(element)),
            Self::Start(start, events) => Some(Item::Events(start, events)),
            Self::Node(_) | Self::Text(_) => None,
        }
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
    T: Spelling,
{
    fn text(&self) -> Option<Cow<'_, str>> {
        self.as_ref()
            .map(|value| value.spelling(ConformanceClass::Transitional))
    }
}

impl<T> ChildField for Box<T>
where
    T: Read + Write,
{
    fn take(&mut self, field: Field<'_>) {
        if let Some(item) = field.into_item() {
            **self = T::read_item(item);
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
        if let Some(item) = field.into_item() {
            *self = Some(Box::new(T::read_item(item)));
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
        if let Field::Node(node) = field {
            self.extend(T::other(node));
        } else if let Some(item) = field.into_item() {
            self.push(T::read_item(item));
        } else {
            // An attribute's text: no child.
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

impl Spelling for Box<str> {
    fn spelling(&self, _class: ConformanceClass) -> Cow<'_, str> {
        Cow::Borrowed(self)
    }
}

impl Spelling for String {
    fn spelling(&self, _class: ConformanceClass) -> Cow<'_, str> {
        Cow::Borrowed(self)
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
    /// A child the reader reached: a declared one is handed out, an unknown one waits for the
    /// next declared child.
    fn child(&mut self, child: Child<'element>) {
        match child {
            Child::Borrowed(node) => {
                if let Node::Element(element) = node
                    && let Some(index) = self.place(element)
                {
                    self.queue.push_back((index, Field::Element(element)));
                } else {
                    self.placement.unknown(node.clone());
                }
            }
            Child::Owned(Node::Element(element)) => match self.place(&element) {
                Some(index) => self.queue.push_back((index, Field::Owned(element))),
                None => self.placement.unknown(Node::Element(element)),
            },
            Child::Owned(node) => self.placement.unknown(node),
            Child::Start(start, events) => match self.place(&start) {
                Some(index) => self.queue.push_back((index, Field::Start(start, events))),
                None => self.placement.unknown(Node::Element(events.element(start))),
            },
        }
    }

    /// The unknown content: attributes the shape does not declare, children
    /// at positions no list covers, the node's own namespace declarations,
    /// and the required single positions the element left empty.
    #[must_use = "the value moves into the one returned"]
    pub fn finish(mut self) -> Option<Box<Unknown>> {
        for (_, field) in self.by_ref() {
            if let Field::Start(start, events) = field {
                drop(events.element(start));
            }
        }
        self.unknown.absent = self
            .shape
            .slots
            .iter()
            .zip(&self.filled)
            .enumerate()
            .filter(|(_, (slot, filled))| slot.occurs == SlotOccurs::Required && !**filled)
            .map(|(position, _)| position)
            .collect();
        self.unknown.absent.shrink_to_fit();
        self.unknown.attributes.shrink_to_fit();
        self.unknown.children.shrink_to_fit();
        self.unknown.boxed()
    }

    /// What `element`'s reader is given, by `shape`.
    #[must_use = "this only builds the value"]
    pub fn new(element: &'element Element, shape: &'static Shape) -> Self {
        Self::of(Item::Tree(element), shape)
    }

    /// The next child; `None` past the last.
    fn next_child(&mut self) -> Option<Child<'element>> {
        match &mut self.content {
            Content::Borrowed(children) => children.next().map(Child::Borrowed),
            Content::Owned(children) => children.next().map(Child::Owned),
            Content::Events(events) => loop {
                match events.event()? {
                    PartEvent::End => return None,
                    PartEvent::Node(node) => return Some(Child::Owned(node)),
                    PartEvent::Start(start) => return Some(Child::Start(start, *events)),
                    PartEvent::Declaration(_) | _ => {}
                }
            },
        }
    }

    /// What the reader of the node `item` holds is given, by `shape`.
    #[must_use = "this only builds the value"]
    pub fn of(item: Item<'element>, shape: &'static Shape) -> Self {
        let (attributes, content, name, namespace_declarations) = match item {
            Item::Events(start, events) => (
                Attributes::Owned(start.attributes.into_iter()),
                Content::Events(events),
                other_name(shape, start.name),
                start.namespace_declarations,
            ),
            Item::Owned(element) => (
                Attributes::Owned(element.attributes.into_iter()),
                Content::Owned(element.children.into_iter()),
                other_name(shape, element.name),
                element.namespace_declarations,
            ),
            Item::Tree(element) => (
                Attributes::Borrowed(element.attributes.iter()),
                Content::Borrowed(element.children.iter()),
                (!is_tag(shape.name, &element.name)).then(|| Box::new(element.name.clone())),
                element.namespace_declarations.clone(),
            ),
        };
        Self {
            attributes,
            content,
            cursor: 0,
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
                name,
                namespace_declarations,
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

    /// The field index of `element` when one of the positions from the last one read on takes
    /// it: the first that does, a single position only while it holds no child and an `xsd:all`
    /// list only while it lacks the tag. The unknown nodes waiting before it are queued first.
    fn place(&mut self, element: &Element) -> Option<usize> {
        let tag = (known(&element.name.namespace), &*element.name.local);
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
            return Some(base + index);
        }
        None
    }

    /// Simple content: its text and CDATA, joined, and every other node kept
    /// after it.
    fn text(&mut self) -> String {
        let mut text = String::new();
        while let Some(child) = self.next_child() {
            let node = match child {
                Child::Borrowed(Node::CData(part) | Node::Text(part)) => {
                    text.push_str(part);
                    continue;
                }
                Child::Owned(Node::CData(part) | Node::Text(part)) => {
                    text.push_str(&part);
                    continue;
                }
                Child::Borrowed(node) => node.clone(),
                Child::Owned(node) => node,
                Child::Start(start, events) => Node::Element(events.element(start)),
            };
            self.unknown.children.push(Extra { after: 0, node });
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
            let declared = |name: &QName| {
                self.shape
                    .attributes
                    .iter()
                    .position(|tag| is_tag(*tag, name))
            };
            match &mut self.attributes {
                Attributes::Borrowed(attributes) => {
                    if let Some(attribute) = attributes.next() {
                        if let Some(index) = declared(&attribute.name) {
                            return Some((index, Field::Text(attribute.value.clone())));
                        }
                        self.unknown.attributes.push(attribute.clone());
                        continue;
                    }
                }
                Attributes::Owned(attributes) => {
                    if let Some(attribute) = attributes.next() {
                        if let Some(index) = declared(&attribute.name) {
                            return Some((index, Field::Text(attribute.value)));
                        }
                        self.unknown.attributes.push(attribute);
                        continue;
                    }
                }
            }
            if self.ended {
                return None;
            }
            if self.shape.text {
                self.ended = true;
                let text = self.text();
                return Some((self.shape.attributes.len(), Field::Text(text)));
            }
            if let Some(child) = self.next_child() {
                self.child(child);
                continue;
            }
            self.ended = true;
            let pending = self.placement.end();
            self.pending(pending);
        }
    }
}

impl Debug for Events<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Events")
            .field("depth", &self.depth.get())
            .finish_non_exhaustive()
    }
}

impl<'source> Events<'source> {
    /// Drain the events the read left, and return the first error the source gave: a second
    /// element outside the root is one.
    ///
    /// # Errors
    ///
    /// The source's first error; [`XmlError::Malformed`] for more than one root element.
    pub fn finish(self) -> Result<(), XmlError> {
        while self.event().is_some() {}
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        if self.roots.get() > 1 {
            return Err(XmlError::Malformed("more than one root element".to_owned()));
        }
        Ok(())
    }

    /// A typed read over `source`, before its first event.
    #[must_use = "this only builds the value"]
    pub fn new(source: &'source mut dyn ReadEvents) -> Self {
        Self {
            depth: Cell::new(0),
            error: Cell::new(None),
            roots: Cell::new(0),
            source: Cell::new(Some(source)),
        }
    }

    /// The part's root element, past the declaration and the nodes before it: its start tag and
    /// these events, or the whole element when it is empty. `None` when the part holds no
    /// element, or the source failed before it.
    #[must_use = "this reads the root's start; dropping it skips it"]
    pub fn root(&self) -> Option<Item<'_>> {
        loop {
            match self.event()? {
                PartEvent::Start(start) => return Some(Item::Events(start, self)),
                PartEvent::Node(Node::Element(element)) => return Some(Item::Owned(element)),
                PartEvent::Declaration(_) | PartEvent::End | PartEvent::Node(_) | _ => {}
            }
        }
    }
}

impl EventSource for Events<'_> {
    fn element(&self, start: Element) -> Element {
        let mut element = start;
        let mut open: Vec<Element> = Vec::new();
        while let Some(event) = self.event() {
            let node = match event {
                PartEvent::End => match open.pop() {
                    Some(done) => Node::Element(done),
                    None => return element,
                },
                PartEvent::Node(node) => node,
                PartEvent::Start(child) => {
                    open.push(child);
                    continue;
                }
                PartEvent::Declaration(_) | _ => continue,
            };
            open.last_mut().unwrap_or(&mut element).children.push(node);
        }
        element
    }

    fn event(&self) -> Option<PartEvent> {
        let source = self.source.take()?;
        let event = match source.event() {
            Ok(event) => event,
            Err(error) => {
                self.error.set(Some(error));
                return None;
            }
        };
        self.source.set(Some(source));
        let depth = self.depth.get();
        let starts = matches!(event, Some(PartEvent::Start(_)));
        if depth == 0 && (starts || matches!(event, Some(PartEvent::Node(Node::Element(_))))) {
            self.roots.set(self.roots.get() + 1);
        }
        if starts {
            self.depth.set(depth + 1);
        } else if matches!(event, Some(PartEvent::End)) {
            self.depth.set(depth.saturating_sub(1));
        } else {
            // A node or the declaration: the depth stays.
        }
        event
    }
}

impl Item<'_> {
    /// The element: the start tag of one read from events.
    #[must_use = "this only returns the value"]
    pub const fn element(&self) -> &Element {
        match self {
            Self::Events(element, _) | Self::Owned(element) => element,
            Self::Tree(element) => element,
        }
    }

    /// The element whole, read from the events up to its end for one read from events.
    #[must_use = "this reads the element; dropping it skips it"]
    pub fn into_element(self) -> Element {
        match self {
            Self::Events(start, events) => events.element(start),
            Self::Owned(element) => element,
            Self::Tree(element) => element.clone(),
        }
    }

    /// The element as a generic node, for an enum's `Other` item.
    #[must_use = "this reads the element; dropping it skips it"]
    pub fn into_node(self) -> Node {
        Node::Element(self.into_element())
    }

    /// The element's tag as the typed tree matches it; `None` for a namespace the table does not
    /// know.
    #[must_use = "this only returns the value"]
    pub fn name(&self) -> Option<(Option<NamespaceId>, &str)> {
        name(self.element())
    }

    /// The node of `T` the item holds.
    #[must_use = "the node read is returned; nothing else keeps it"]
    pub fn read<T>(self) -> T
    where
        T: Read,
    {
        T::read_item(self)
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
            Accepted::Tag(tag, also) => {
                is_tag(tag, &element.name) || also.iter().any(|other| is_tag(*other, &element.name))
            }
        }
    }

    /// An optional single position of an enum type.
    #[must_use = "this only builds the value"]
    pub const fn choice(accept: &'static Accept) -> Self {
        Self {
            accept: Accepted::Enum(accept),
            occurs: SlotOccurs::Single,
        }
    }

    /// A list of an enum type: unknown nodes among its items join it.
    #[must_use = "this only builds the value"]
    pub const fn items(accept: &'static Accept) -> Self {
        Self {
            accept: Accepted::Enum(accept),
            occurs: SlotOccurs::Items,
        }
    }

    /// A list of one struct type.
    #[must_use = "this only builds the value"]
    pub const fn list(tag: Tag) -> Self {
        Self::list_also(tag, &[])
    }

    /// As [`Slot::single_also`], for a list of one struct type.
    #[must_use = "this only builds the value"]
    pub const fn list_also(tag: Tag, also: &'static [Tag]) -> Self {
        Self {
            accept: Accepted::Tag(tag, also),
            occurs: SlotOccurs::List,
        }
    }

    /// The list of an `xsd:all` type: each tag at most once.
    #[must_use = "this only builds the value"]
    pub const fn once(accept: &'static Accept) -> Self {
        Self {
            accept: Accepted::Enum(accept),
            occurs: SlotOccurs::Once,
        }
    }

    /// A required single position of one struct type.
    #[must_use = "this only builds the value"]
    pub const fn required(tag: Tag) -> Self {
        Self::required_also(tag, &[])
    }

    /// As [`Slot::single_also`], for a position the schema requires.
    #[must_use = "this only builds the value"]
    pub const fn required_also(tag: Tag, also: &'static [Tag]) -> Self {
        Self {
            accept: Accepted::Tag(tag, also),
            occurs: SlotOccurs::Required,
        }
    }

    /// A required single position of an enum type.
    #[must_use = "this only builds the value"]
    pub const fn required_choice(accept: &'static Accept) -> Self {
        Self {
            accept: Accepted::Enum(accept),
            occurs: SlotOccurs::Required,
        }
    }

    /// An optional single position of one struct type.
    #[must_use = "this only builds the value"]
    pub const fn single(tag: Tag) -> Self {
        Self::single_also(tag, &[])
    }

    /// A single position of one struct type whose child is also written under `also`, the names
    /// an application gives it outside the standard, as Word's `wps:cNvPr` for `wp:cNvPr`
    /// (ECMA-376 Part 1, 20.4.2.37 to 20.4.2.42).
    #[must_use = "this only builds the value"]
    pub const fn single_also(tag: Tag, also: &'static [Tag]) -> Self {
        Self {
            accept: Accepted::Tag(tag, also),
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
    /// This record boxed, as a node holds it; `None` when it holds nothing.
    #[must_use = "the value moves into the one returned"]
    pub fn boxed(self) -> Option<Box<Self>> {
        (!self.is_empty()).then(|| Box::new(self))
    }

    /// The record `None` stands for: no unknown content.
    #[must_use = "this only returns the value"]
    pub fn empty<'any>() -> &'any Self {
        static EMPTY: Unknown = Unknown {
            absent: Vec::new(),
            attributes: Vec::new(),
            children: Vec::new(),
            name: None,
            namespace_declarations: Vec::new(),
        };
        &EMPTY
    }

    /// Whether this record holds no unknown content.
    #[must_use = "this only returns the value"]
    pub const fn is_empty(&self) -> bool {
        self.absent.is_empty()
            && self.attributes.is_empty()
            && self.children.is_empty()
            && self.name.is_none()
            && self.namespace_declarations.is_empty()
    }

    /// Unknown content with these absent positions, attributes, children, and declarations.
    #[must_use = "this only builds the value"]
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
            name: None,
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
                .attribute(&mut self.element, tag.0.map(uri), tag.1, &text);
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
    #[must_use = "the value moves into the one returned"]
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
    /// and the unknown content it keeps, named as it was read; `None` keeps none.
    #[must_use = "the result is returned and kept nowhere else"]
    pub fn new(
        scope: &'scope mut Scope,
        shape: &'static Shape,
        unknown: Option<&'node Unknown>,
    ) -> Self {
        let kept = unknown.unwrap_or_else(Unknown::empty);
        let (namespace, local) = kept.name.as_deref().map_or_else(
            || (shape.name.0.map_or("", uri), shape.name.1),
            |name| {
                (
                    name.namespace.uri(ConformanceClass::Transitional),
                    &*name.local,
                )
            },
        );
        let element = scope.element(namespace, local, &kept.namespace_declarations);
        Self {
            children: Children::new(&kept.children),
            element,
            positions: Positions::default(),
            scope,
            shape,
            single: false,
            unknown: kept,
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
        T: Spelling + ?Sized,
    {
        let value = text.spelling(ConformanceClass::Transitional);
        if !value.is_empty() {
            self.children.other(Node::Text(value.into_owned()));
        }
    }
}

/// The children of a list field, each converted by `each`.
///
/// `each` is the `into_user_data` of the node a list holds.
#[must_use = "this only returns the value"]
pub fn into_user_data_list<T, U>(items: Vec<T>, each: fn(T) -> U) -> Vec<U> {
    items.into_iter().map(each).collect()
}

/// The child of an optional field, taken out of the field and converted by `each`.
///
/// `each` is the `into_user_data` of the node the field holds.
#[must_use = "the result is returned and kept nowhere else"]
pub fn into_user_data_optional<T, U>(
    item: &mut Option<Box<T>>,
    each: fn(T) -> U,
) -> Option<Box<U>> {
    item.take().map(|boxed| Box::new(each(*boxed)))
}

/// The child of a required field, converted by `each` and boxed again.
///
/// `each` is the `into_user_data` of the node the field holds.
#[must_use = "this only returns the value"]
pub fn into_user_data_required<T, U>(item: T, each: fn(T) -> U) -> Box<U> {
    Box::new(each(item))
}

/// The spelling of `value` an archive holds: Transitional's.
#[cfg(feature = "binary-format")]
#[must_use = "this only returns the value"]
pub fn archived_spelling<T>(value: &T) -> Cow<'_, str>
where
    T: Spelling + ?Sized,
{
    value.spelling(ConformanceClass::Transitional)
}

/// Whether `value` is written as exactly `text`; it compares as it formats, with no allocation.
#[must_use = "this only answers the question"]
pub fn spells<T>(value: &T, text: &str) -> bool
where
    T: Display + ?Sized,
{
    struct Rest<'text>(&'text str);

    impl fmt::Write for Rest<'_> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            self.0 = self.0.strip_prefix(s).ok_or(fmt::Error)?;
            Ok(())
        }
    }

    let mut rest = Rest(text);
    write!(rest, "{value}").is_ok() && rest.0.is_empty()
}

/// The items of a list, each in its spelling, separated by one space.
///
/// # Errors
///
/// The formatter's.
pub fn write_list<T>(formatter: &mut Formatter<'_>, items: &[T]) -> fmt::Result
where
    T: Spelling,
{
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            formatter.write_char(' ')?;
        }
        formatter.write_str(&item.spelling(ConformanceClass::Transitional))?;
    }
    Ok(())
}

/// The enumeration member the text names; `None` when it names none.
#[must_use = "this only returns the value"]
pub fn member<M>(members: &Members<M>, text: &str) -> Option<M>
where
    M: Copy,
{
    members
        .iter()
        .find(|(_, spelled, _)| *spelled == text)
        .map(|(found, _, _)| *found)
}

/// The text a member stands for.
#[must_use = "this only returns the value"]
pub fn member_text<M>(members: &Members<M>, member: M) -> &'static str
where
    M: Copy + PartialEq,
{
    members
        .iter()
        .find(|(found, _, _)| *found == member)
        .map_or("", |(_, text, _)| text)
}

/// The Office version that brought a member, from the Open XML SDK 3.5.1 data;
/// `None` for ECMA-376's own.
#[must_use = "this only returns the value"]
pub fn member_version<M>(members: &Members<M>, member: M) -> Option<OfficeVersion>
where
    M: Copy + PartialEq,
{
    members
        .iter()
        .find(|(found, _, _)| *found == member)
        .and_then(|(_, _, version)| *version)
}

/// `ST_OnOff`: `true`, `1`, `on` are true; `false`, `0`, `off` false; `None`
/// for any other text.
#[must_use = "this only returns the value"]
pub fn on_off(text: &str) -> Option<bool> {
    parse::boolean(text).or(match text {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    })
}

/// `xsd:boolean`; `None` for text that is not one.
#[must_use = "this only returns the value"]
pub fn boolean(text: &str) -> Option<bool> {
    parse::boolean(text)
}

/// `xsd:dateTime`; `None` for text that is not one.
#[must_use = "this only returns the value"]
pub fn date_time(text: &str) -> Option<DateTime> {
    parse::date_time(text)
}

/// A floating-point number within `bounds`; `None` for text that is no
/// number, or a number out of the bounds.
#[must_use = "this only returns the value"]
pub fn double(text: &str, bounds: &Bounds) -> Option<f64> {
    let value: f64 = text.parse().ok()?;
    let within = bounds.min_inclusive.is_none_or(|bound| value >= bound)
        && bounds.min_exclusive.is_none_or(|bound| value > bound)
        && bounds.max_inclusive.is_none_or(|bound| value <= bound)
        && bounds.max_exclusive.is_none_or(|bound| value < bound);
    within.then_some(value)
}

/// `bytes` hexadecimal bytes as an unsigned number; `None` for text of
/// another length, or with a character that is no hexadecimal digit.
#[must_use = "this only returns the value"]
pub fn hex<T>(text: &str, bytes: usize) -> Option<T>
where
    T: Hex,
{
    let valid = text.len() == bytes * 2 && text.bytes().all(|byte| byte.is_ascii_hexdigit());
    valid.then(|| T::from_hex(text)).flatten()
}

/// A whole number from `min` to `max`; `None` for text that is no whole
/// number, or one out of the range.
#[must_use = "this only returns the value"]
pub fn integer(text: &str, min: i64, max: i64) -> Option<i64> {
    parse::integer(text).filter(|value| (min..=max).contains(value))
}

/// A whole number spelled as one of `values`; `None` for text that is none
/// of them.
#[must_use = "this only returns the value"]
pub fn integer_in(text: &str, values: &[&str]) -> Option<i64> {
    values
        .contains(&text)
        .then(|| parse::integer(text))
        .flatten()
}

/// The error of a text that is no value of its simple type.
#[must_use = "this only returns the value"]
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

/// `name`, boxed, when it is not the tag of `shape`: the name a node read under another keeps.
fn other_name(shape: &Shape, name: QName) -> Option<Box<QName>> {
    (!is_tag(shape.name, &name)).then(|| Box::new(name))
}

/// Whether `name` is the tag `tag`.
fn is_tag(tag: Tag, name: &QName) -> bool {
    tag.1 == &*name.local && name.namespace == tag.0.map_or(Namespace::None, Namespace::Known)
}

/// A universal measure, `2.5cm`, matching one of `patterns`; `None` for text
/// that matches none of them.
#[must_use = "this only returns the value"]
pub fn measure(text: &str, patterns: &Patterns) -> Option<Measure> {
    patterns
        .matches(text)
        .then(|| parse::measure(text))
        .flatten()
}

/// An element's tag as the typed tree matches it; `None` for a namespace the
/// table does not know.
#[must_use = "this only returns the value"]
pub fn name(element: &Element) -> Option<(Option<NamespaceId>, &str)> {
    tag_of(&element.name)
}

/// A percentage, `50%` being 50.0, matching one of `patterns`; `None` for
/// text that matches none of them.
#[must_use = "this only returns the value"]
pub fn percentage(text: &str, patterns: &Patterns) -> Option<f64> {
    patterns
        .matches(text)
        .then(|| parse::decimal_with_suffix(text, "%"))
        .flatten()
}

/// The generic node an item of the typed tree writes as: an arm's element
/// with the namespace declarations it needs on its own, or the `Other` node.
#[must_use = "this only returns the value"]
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
    let mut writer = Writer::new(&mut scope, &BARE, None);
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
#[must_use = "this only returns the value"]
pub fn read<T>(element: &Element) -> T
where
    T: Read,
{
    T::read(element)
}

/// A string within its length and pattern facets; `None` for text out of
/// them.
#[must_use = "this only returns the value"]
pub fn text<'text>(text: &'text str, facets: &Facets) -> Option<&'text str> {
    let length = text.chars().count();
    let within = facets.length.is_none_or(|bound| length == bound)
        && facets.min_length.is_none_or(|bound| length >= bound)
        && facets.max_length.is_none_or(|bound| length <= bound)
        && (facets.patterns.is_empty() || facets.patterns.matches(text));
    within.then_some(text)
}

/// The Transitional URI of a namespace id.
#[must_use = "this only returns the value"]
pub fn uri(id: NamespaceId) -> &'static str {
    NAMESPACES.get(usize::from(id)).map_or("", |row| row.uri)
}
