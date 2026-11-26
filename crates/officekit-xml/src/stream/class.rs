//! `ClassRules`: what a conformance class changes in a part written through a `PartWriter`.
//!
//! A class writes most nodes as the writer both classes share writes them, and changes a few:
//! a namespace takes the class's URI, a value takes the class's spelling, a node the class has no
//! form for is refused. A part writer given a class's rules asks them about each node. A typed
//! node the rules do not change is written tag by tag, its start tag given to
//! [`ClassRules::open`]; one they change is written as one element, which
//! [`ClassRules::convert`] turns into the nodes the class writes in its place.

use crate::element::{Attribute, Element, Node};
use crate::error::WriteError;
use crate::name::{NamespaceId, QName};
use crate::write::Scope;

/// Where a node written through a part writer stands.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Site<'writer> {
    /// The innermost element open around the node; `None` for the part's root.
    pub parent: Option<&'writer QName>,
    /// The namespace declarations of the elements open around the node.
    pub scope: &'writer Scope,
}

/// What a conformance class changes in the nodes written through a part writer.
pub trait ClassRules {
    /// The nodes the class writes for `element`, an element written whole at `site`.
    ///
    /// # Errors
    ///
    /// [`WriteError::NotInClass`] for the first content the class has no form for.
    fn convert(&mut self, element: Element, site: &Site<'_>) -> Result<Vec<Node>, WriteError>;

    /// Whether a typed node named `tag`, whose shape declares the attributes `declared` and which
    /// holds the unknown attributes `unknown`, is written as one element for
    /// [`convert`](Self::convert); `false` writes it tag by tag.
    fn converts(
        &self,
        tag: (Option<NamespaceId>, &str),
        declared: &[(Option<NamespaceId>, &'static str)],
        unknown: &[Attribute],
    ) -> bool;

    /// Change in place, as the class writes it, the start tag of an element written tag by tag
    /// at `site`; it has no children.
    ///
    /// # Errors
    ///
    /// [`WriteError::NotInClass`] for content the class has no form for.
    fn open(&mut self, element: &mut Element, site: &Site<'_>) -> Result<(), WriteError>;
}

impl Site<'_> {
    /// Whether the node is the part's root.
    #[must_use = "this only answers the question"]
    pub const fn is_root(&self) -> bool {
        self.parent.is_none()
    }
}
