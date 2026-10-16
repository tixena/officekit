//! The write-through form of `Transform` (D37): a node's result goes to a
//! `PartWriter` as it is made, each hook asked on the way.

use std::io;

use super::writer::PartWriter;
use crate::markup::{Context, Unknown};
use crate::xml::element::{Element, Node};
use crate::xml::write::{is_whitespace_text, preserves_space, qualified};

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

/// A context with no hooks: writing through it writes a node as it is.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct Unchanged;

/// Whether a write hook wrote its node, or leaves it to the tree hook and the
/// node's own writing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Written {
    No,
    Yes,
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

impl Context for Unchanged {
    fn element(&mut self, _node: &Element) -> Option<Element> {
        None
    }

    fn write_element(&mut self, _node: &Element, _out: &mut PartWriter<'_>) -> io::Result<Written> {
        Ok(Written::No)
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
