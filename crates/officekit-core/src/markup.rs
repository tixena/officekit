//! The typed tree's shared runtime (D8, D12): where unknown content goes when
//! a node is read and written, which prefix a written element takes, and the
//! `Transform` and `Visit` traits with their root hooks.

use core::mem;
use std::io;

use crate::stream::transform::Written;
use crate::stream::writer::PartWriter;
use crate::xml;
use crate::xml::element::{Attribute, Element, NamespaceDeclaration, Node};
use crate::xml::name::table::NAMESPACES;
use crate::xml::name::{Namespace, QName};
use crate::xml::write::is_whitespace_text;

#[cfg(test)]
mod tests;

/// The prefix declared for a namespace the table does not know.
const OTHER_PREFIX: &str = "ns";

/// The namespace the `xml` prefix is bound to without a declaration.
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

// Not a module: `pub use` is denied, and its traits are `markup::Context` and `markup::Visitor`.
include!("markup/root.rs");

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

/// The namespace declarations in scope while a typed tree is written: an
/// element takes a prefix bound to its namespace in either class's URI, or
/// declares the table's prefix where none is bound.
#[derive(Debug, Default)]
pub struct Scope {
    frames: Vec<Vec<NamespaceDeclaration>>,
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
            self.bound(&namespace)
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
    /// rebinds; `xml` is bound without a declaration.
    fn bound(&self, namespace: &Namespace) -> Option<String> {
        if binds(XML_NAMESPACE, namespace) {
            return Some("xml".to_owned());
        }
        let mut seen: Vec<&str> = Vec::new();
        for declaration in self.frames.iter().rev().flatten() {
            let prefix = declaration.prefix.as_str();
            if prefix.is_empty() || seen.contains(&prefix) {
                continue;
            }
            if binds(&declaration.uri, namespace) {
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
            .bound(&namespace)
            .unwrap_or_else(|| self.declare(&mut element, &namespace, uri));
        element
    }

    /// Close the frame the last `element` opened.
    pub fn end(&mut self) {
        self.frames.pop();
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
