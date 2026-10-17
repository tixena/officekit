//! `officekit-traversal` holds the traversals of officekit's typed tree. Every node of
//! `officekit_markup::<namespace>` has its `Transform`, which builds an owned node from a
//! reference to one, the context deciding what changes; its `Visit`, which shows it to a visitor;
//! and its `WriteThrough`, whose result goes to a part writer as it is made. Every enum has its
//! `Item`, with the list hooks a list of its items asks.
//!
//! The crate root is the runtime the generated traversals call: the walk of a node's children,
//! the rebase of its unknown content, the write-through writer, and the functions that ask the
//! hooks of `officekit_markup::Context` and `officekit_markup::Visitor`.

// The traversal modules, generated in a file of their own: module declarations
// go before the imports, and `include!` places them where it stands.
include!("modules.rs");

use core::iter;
use std::io;

use officekit_markup::{
    AttributeValue, Context, Descend, Extra, Nodes, Positions, Shape, Unknown, Visitor, Written,
    uri,
};
use officekit_xml::element::{Element, Node};
use officekit_xml::stream::writer::PartWriter;
use officekit_xml::write::{is_whitespace_text, preserves_space, qualified};

#[cfg(test)]
mod tests;

/// Writes a node's unknown children at their places while its declared
/// children are written: each `Extra` before the declared child its `after`
/// names, and the rest after the last one.
#[derive(Debug)]
pub struct Place<'unknown> {
    known: usize,
    unknown: &'unknown Unknown,
}

/// How many declared children each original declared child of a node became
/// in its transform, in document order.
#[derive(Debug, Default)]
pub struct Rebase {
    /// The node's `unknown.absent`.
    absent: Vec<usize>,
    counts: Vec<usize>,
    /// Declared items given in place of `Other` items before the first original declared child.
    leading: usize,
    reached: Positions,
}

/// Writes a typed node through a `PartWriter`.
///
/// Its declared attributes go in schema order, then the unknown ones, and its
/// declared children in schema order, each written through, with each list's
/// `Other` items among them and each `Extra` at its place. An element
/// nothing is written into ends as an empty tag, as the generic tree writes it.
pub struct Through<'out, 'sink, 'node> {
    element: Element,
    opened: bool,
    out: &'out mut PartWriter<'sink>,
    place: Place<'node>,
    positions: Positions,
    shape: &'static Shape,
    unknown: &'node Unknown,
}

/// A context with no hooks: writing through it writes a node as it is.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct Unchanged;

/// Walks a typed node's children for a visitor in document order: its
/// declared children in schema order, with each `Extra` before the declared
/// child its place names and the rest after the last one.
#[derive(Debug)]
pub struct Walk<'node> {
    known: usize,
    positions: Positions,
    unknown: &'node Unknown,
}

/// Every enum of the typed tree: whether an item is one of its arms, and the
/// list hooks a list of its items asks.
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

/// Every node type: from a reference to a node, an owned node, with the
/// context deciding what changes.
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

/// A declared child's field rebuilt: each of its nodes transformed.
pub trait TransformNodes: Nodes {
    /// The field rebuilt with each of its nodes transformed.
    #[must_use]
    fn transform_nodes(&self, ctx: &mut dyn Context) -> Self
    where
        Self: Sized,
        Self::Node: Transform;
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

/// Every node type: written straight to a `PartWriter`, each hook asked on
/// the way.
pub trait WriteThrough {
    /// Write this node with its children written through, without asking the
    /// context about this node.
    ///
    /// # Errors
    ///
    /// The sink's.
    fn write_children_through(
        &self,
        ctx: &mut dyn Context,
        out: &mut PartWriter<'_>,
    ) -> io::Result<()>;

    /// Ask the context's write hook, then its tree hook, about this node; when
    /// both answer no, write the node with its children written through.
    ///
    /// # Errors
    ///
    /// The sink's.
    fn write_through(&self, ctx: &mut dyn Context, out: &mut PartWriter<'_>) -> io::Result<()>;
}

impl<'unknown> Place<'unknown> {
    /// Call before each declared child: writes the unknown nodes placed
    /// before it.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn before_known(
        &mut self,
        ctx: &mut dyn Context,
        out: &mut PartWriter<'_>,
    ) -> io::Result<()> {
        let known = self.known;
        for extra in self
            .unknown
            .children
            .iter()
            .filter(|extra| extra.after == known)
        {
            extra.node.write_through(ctx, out)?;
        }
        self.known += 1;
        Ok(())
    }

    /// Call after the last declared child: writes the unknown nodes placed
    /// after it.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn finish(self, ctx: &mut dyn Context, out: &mut PartWriter<'_>) -> io::Result<()> {
        for extra in self
            .unknown
            .children
            .iter()
            .filter(|extra| extra.after >= self.known)
        {
            extra.node.write_through(ctx, out)?;
        }
        Ok(())
    }

    #[must_use]
    pub const fn new(unknown: &'unknown Unknown) -> Self {
        Self { known: 0, unknown }
    }
}

impl<T> TransformNodes for Box<T>
where
    T: Default + PartialEq,
{
    fn transform_nodes(&self, ctx: &mut dyn Context) -> Self
    where
        T: Transform,
    {
        Self::new(self.as_ref().transform(ctx))
    }
}

impl<T> TransformNodes for Option<Box<T>> {
    fn transform_nodes(&self, ctx: &mut dyn Context) -> Self
    where
        T: Transform,
    {
        self.as_deref().map(|node| Box::new(node.transform(ctx)))
    }
}

impl<T> TransformNodes for Vec<T> {
    fn transform_nodes(&self, ctx: &mut dyn Context) -> Self
    where
        T: Transform,
    {
        self.iter().map(|node| node.transform(ctx)).collect()
    }
}

impl Rebase {
    /// Count the `became` declared children an original child became: its
    /// own count when it is declared, else added to the count before it.
    fn count(&mut self, declared: bool, became: usize) {
        if declared {
            self.counts.push(became);
        } else {
            *self.counts.last_mut().unwrap_or(&mut self.leading) += became;
        }
    }

    /// The rebase of a node whose unknown content is `unknown`.
    #[must_use]
    pub fn new(unknown: &Unknown) -> Self {
        Self {
            absent: unknown.absent.clone(),
            ..Self::default()
        }
    }

    /// The place, in the rebuilt node, of an `Extra` placed after the
    /// `after`-th original declared child.
    #[must_use]
    pub fn place(&self, after: usize) -> usize {
        self.leading
            + self.counts.iter().take(after).sum::<usize>()
            + after.saturating_sub(self.counts.len())
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
    /// through after the `Extra`s placed before it, none while it holds the
    /// default node of a child the read element did not hold.
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
        if self.positions.skips(&self.unknown.absent, field) {
            return Ok(());
        }
        for node in field.nodes() {
            self.place.before_known(ctx, self.out)?;
            node.write_through(ctx, self.out)?;
        }
        Ok(())
    }

    /// Write a single declared child of an enum type: written through after
    /// the `Extra`s placed before it, an `Other` node as a declared one.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn choice<F>(&mut self, field: &F, ctx: &mut dyn Context) -> io::Result<()>
    where
        F: Nodes,
        F::Node: Item + WriteThrough,
    {
        self.child(field, ctx)
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
    /// written as they are, then the item written through.
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
        self.positions.advance();
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
            positions: Positions::default(),
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

impl Context for Unchanged {}

impl Transform for Element {
    fn transform(&self, ctx: &mut dyn Context) -> Self {
        ctx.element(self)
            .unwrap_or_else(|| self.transform_children(ctx))
    }

    fn transform_children(&self, ctx: &mut dyn Context) -> Self {
        let mut element = Self::new(self.name.clone());
        element.attributes.clone_from(&self.attributes);
        element.children = self
            .children
            .iter()
            .map(|child| child.transform(ctx))
            .collect();
        element
            .namespace_declarations
            .clone_from(&self.namespace_declarations);
        element
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

impl WriteThrough for Node {
    /// An element with element children is written tag by tag, each child
    /// written through; any other node is written as it is.
    fn write_children_through(
        &self,
        ctx: &mut dyn Context,
        out: &mut PartWriter<'_>,
    ) -> io::Result<()> {
        let Self::Element(element) = self else {
            return out.node(self);
        };
        if !element
            .children
            .iter()
            .any(|child| matches!(child, Self::Element(_)))
        {
            return out.element(element);
        }
        let name = qualified(&element.name);
        let named: Vec<(String, &str)> = element
            .attributes
            .iter()
            .map(|attribute| (qualified(&attribute.name), attribute.value.as_str()))
            .collect();
        let attributes: Vec<(&str, &str)> = named
            .iter()
            .map(|(attribute, value)| (attribute.as_str(), *value))
            .collect();
        out.start(&name, &element.namespace_declarations, &attributes)?;
        let preserved = preserves_space(element, false);
        for child in &element.children {
            if preserved || !is_whitespace_text(child) {
                child.write_through(ctx, out)?;
            }
        }
        out.end(&name)
    }

    /// An element goes to `Context::write_element`, then to
    /// `Context::element`, whose result is written as it is; any other node is
    /// written as it is.
    fn write_through(&self, ctx: &mut dyn Context, out: &mut PartWriter<'_>) -> io::Result<()> {
        let Self::Element(element) = self else {
            return out.node(self);
        };
        if ctx.write_element(element, out)? == Written::Yes {
            return Ok(());
        }
        match ctx.element(element) {
            Some(replaced) => out.element(&replaced),
            None => self.write_children_through(ctx, out),
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
    /// `Extra`s placed before it, none while it holds the default node of a
    /// child the read element did not hold.
    pub fn child<F>(&mut self, field: &F, visitor: &mut dyn Visitor)
    where
        F: Nodes,
        F::Node: Visit,
    {
        if self.positions.skips(&self.unknown.absent, field) {
            return;
        }
        for node in field.nodes() {
            self.before_known(visitor);
            node.visit(visitor);
        }
    }

    /// Visit a single declared child of an enum type: after the `Extra`s
    /// placed before it, an `Other` node as a declared one.
    pub fn choice<F>(&mut self, field: &F, visitor: &mut dyn Visitor)
    where
        F: Nodes,
        F::Node: Item + Visit,
    {
        self.child(field, visitor);
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
        self.positions.advance();
        for item in field.nodes() {
            if item.is_declared() {
                self.before_known(visitor);
            }
            item.visit(visitor);
        }
    }

    #[must_use]
    pub fn new(unknown: &'node Unknown) -> Self {
        Self {
            known: 0,
            positions: Positions::default(),
            unknown,
        }
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
///
/// The default node of a child the read element did not hold is copied
/// without asking any hook, and counts none.
#[must_use]
pub fn transform_child<F>(field: &F, ctx: &mut dyn Context, rebase: &mut Rebase) -> F
where
    F: Clone + TransformNodes,
    F::Node: Transform,
{
    if rebase.reached.skips(&rebase.absent, field) {
        return field.clone();
    }
    let rebuilt = field.transform_nodes(ctx);
    rebase.counts.extend(iter::repeat_n(1, field.nodes().len()));
    rebuilt
}

/// A single declared child of an enum type rebuilt with its node
/// transformed; the node counts one declared child in `rebase`, an `Other`
/// node as a declared one.
#[must_use]
pub fn transform_choice<F>(field: &F, ctx: &mut dyn Context, rebase: &mut Rebase) -> F
where
    F: Clone + TransformNodes,
    F::Node: Item + Transform,
{
    transform_child(field, ctx, rebase)
}

/// A list of an enum type rebuilt: each item's list hook first, whose
/// `Some` puts any number of items, none included, in its place, and the
/// item's `transform` otherwise.
///
/// A declared item counts in `rebase` the declared items it became; an
/// `Other` item adds the declared items it became to the count before it.
pub fn transform_items<T>(items: &[T], ctx: &mut dyn Context, rebase: &mut Rebase) -> Vec<T>
where
    T: Item + Transform,
{
    rebase.reached.advance();
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let start = out.len();
        match item.items(ctx) {
            Some(spliced) => out.extend(spliced),
            None => out.push(item.transform(ctx)),
        }
        let became = out
            .iter()
            .skip(start)
            .filter(|node| node.is_declared())
            .count();
        rebase.count(item.is_declared(), became);
    }
    out
}

/// A node's unknown content rebuilt: each `Extra`'s node transformed through
/// the root `element` hook, the attributes, declarations, and absent
/// positions as they are.
///
/// Each `Extra` is placed, by `rebase`, after what the declared children
/// before it became.
pub fn transform_unknown(unknown: &Unknown, ctx: &mut dyn Context, rebase: &Rebase) -> Unknown {
    let children = unknown
        .children
        .iter()
        .map(|extra| Extra::new(rebase.place(extra.after), extra.node.transform(ctx)))
        .collect();
    Unknown::new(
        unknown.absent.clone(),
        unknown.attributes.clone(),
        children,
        unknown.namespace_declarations.clone(),
    )
}
