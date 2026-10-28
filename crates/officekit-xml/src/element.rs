//! The generic element tree: every XML part parses into it, the typed tree is
//! built from it and written through it, and the validator walks it.

#[cfg(feature = "binary-format")]
mod archive;

#[cfg(feature = "binary-format")]
use rkyv::de::Pooling;
#[cfg(feature = "binary-format")]
use rkyv::rancor::Source;
#[cfg(feature = "binary-format")]
use rkyv::ser::{Allocator, Sharing, Writer};
#[cfg(feature = "binary-format")]
use rkyv::validation::{ArchiveContext, SharedContext};

use super::conformance::ConformanceClass;
use super::mc::{self, MarkupCompatibility};
use super::name::{Namespace, QName};
use super::write::{self, WriteOptions};

#[cfg(test)]
mod tests;

/// An attribute: its qualified name and its value, unescaped.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "binary-format",
    derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)
)]
#[cfg_attr(feature = "binary-format", rkyv(attr(non_exhaustive)))]
#[non_exhaustive]
pub struct Attribute {
    /// The attribute's qualified name.
    pub name: QName,
    /// The value, unescaped.
    pub value: String,
}

/// A whole part: its XML declaration as written
/// (`xml version="1.0" encoding="UTF-8" standalone="yes"`), what comes before
/// and after the root, and the root.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Document {
    /// The XML declaration's text without `<?` and `?>`; `None` when the part has none.
    pub declaration: Option<String>,
    /// The nodes after the root: whitespace, comments and processing instructions.
    pub epilog: Vec<Node>,
    /// The nodes between the declaration and the root.
    pub prolog: Vec<Node>,
    /// The root element.
    pub root: Element,
}

/// One element: attributes and namespace declarations, each in document
/// order, and children in order.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "binary-format",
    derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)
)]
#[cfg_attr(feature = "binary-format", rkyv(serialize_bounds(__S: Allocator + Sharing + Writer, __S::Error: Source)))]
#[cfg_attr(feature = "binary-format", rkyv(deserialize_bounds(__D: Pooling, __D::Error: Source)))]
#[cfg_attr(feature = "binary-format", rkyv(bytecheck(bounds(__C: ArchiveContext + SharedContext, __C::Error: Source))))]
#[cfg_attr(feature = "binary-format", rkyv(attr(non_exhaustive)))]
#[non_exhaustive]
pub struct Element {
    /// The attributes, in document order. A namespace declaration is not one of them.
    pub attributes: Vec<Attribute>,
    /// The child nodes, in order.
    #[cfg_attr(feature = "binary-format", rkyv(omit_bounds))]
    pub children: Vec<Node>,
    /// The element's qualified name.
    pub name: QName,
    /// The `xmlns` declarations this element makes, in document order.
    pub namespace_declarations: Vec<NamespaceDeclaration>,
}

/// An `xmlns` or `xmlns:prefix` attribute, with the URI exactly as written, in
/// either class.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "binary-format",
    derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)
)]
#[cfg_attr(feature = "binary-format", rkyv(attr(non_exhaustive)))]
#[non_exhaustive]
pub struct NamespaceDeclaration {
    /// The prefix; empty for a default namespace, `xmlns="..."`.
    pub prefix: String,
    /// The namespace URI as written.
    pub uri: String,
}

/// A node of the tree. Text is unescaped; a comment, a CDATA section, and a
/// processing instruction hold their content as written. With
/// `binary-format`, rkyv archives it through its mirror in `archive`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Node {
    /// A CDATA section's content.
    CData(String),
    /// A comment's content, without `<!--` and `-->`.
    Comment(String),
    /// A child element.
    Element(Element),
    /// A processing instruction.
    ProcessingInstruction {
        /// Everything after the target.
        content: String,
        /// The name after `<?`.
        target: String,
    },
    /// Text, unescaped.
    Text(String),
}

/// Why a part could not be read or its markup compatibility processed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum XmlError {
    /// The bytes are not well-formed XML, or hold a document type
    /// declaration, which a part never has.
    #[error("malformed XML: {0}")]
    Malformed(String),
    /// `mc:MustUnderstand` names a namespace the target Office version does not understand.
    #[error("mc:MustUnderstand names `{prefix}`, which the target does not understand")]
    MustUnderstand {
        /// The prefix `mc:MustUnderstand` names.
        prefix: String,
    },
    /// An element or attribute uses a prefix that no declaration in scope binds.
    #[error("undeclared namespace prefix `{0}`")]
    UndeclaredPrefix(String),
}

impl Attribute {
    /// An attribute with `name` and `value`, which is stored as given.
    #[must_use]
    pub const fn new(name: QName, value: String) -> Self {
        Self { name, value }
    }
}

impl Document {
    /// A part with `declaration` and a `\r\n` after it, as Office writes it.
    #[must_use]
    pub fn new(declaration: &str, root: Element) -> Self {
        Self {
            declaration: Some(declaration.to_owned()),
            epilog: Vec::new(),
            prolog: vec![Node::Text("\r\n".to_owned())],
            root,
        }
    }

    /// Apply markup compatibility to the tree.
    ///
    /// # Errors
    ///
    /// [`XmlError::MustUnderstand`] names a prefix the target lacks.
    pub fn process_markup_compatibility(
        &mut self,
        mode: MarkupCompatibility,
    ) -> Result<(), XmlError> {
        mc::apply(self, mode)
    }

    /// The part's bytes: the declaration, the prolog, the root and the epilog, laid out as
    /// `options` says. Writing what [`parse`](crate::parse::parse) read gives the same elements,
    /// attributes and text, but the bytes can differ: the writer puts namespace declarations before
    /// the other attributes, writes characters with its own escapes, and drops the whitespace and
    /// namespace declarations `options` removes.
    #[must_use]
    pub fn to_xml(&self, options: WriteOptions) -> Vec<u8> {
        write::document(self, options)
    }
}

impl Element {
    /// The value of the attribute named `local` in `namespace`; `None` when there is none. The
    /// prefix does not matter. Use `Namespace::None` for an attribute with no prefix.
    #[must_use]
    pub fn attribute(&self, namespace: &Namespace, local: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|attribute| {
                attribute.name.local == local && attribute.name.namespace == *namespace
            })
            .map(|attribute| attribute.value.as_str())
    }

    /// The children that are elements, in order; text, comments and the like are skipped.
    pub fn child_elements(&self) -> impl Iterator<Item = &Self> {
        self.children.iter().filter_map(|node| {
            if let Node::Element(element) = node {
                Some(element)
            } else {
                None
            }
        })
    }

    /// An element with no attributes, declarations, or children.
    #[must_use]
    pub const fn new(name: QName) -> Self {
        Self {
            attributes: Vec::new(),
            children: Vec::new(),
            name,
            namespace_declarations: Vec::new(),
        }
    }

    /// Declare every namespace of the subtree with the class's URI; a
    /// namespace with one URI for both is left alone.
    pub fn set_class(&mut self, class: ConformanceClass) {
        for declaration in &mut self.namespace_declarations {
            let namespace = Namespace::from_uri(&declaration.uri);
            if let Namespace::Known(_) = namespace {
                namespace.uri(class).clone_into(&mut declaration.uri);
            }
        }
        for child in &mut self.children {
            if let Node::Element(element) = child {
                element.set_class(class);
            }
        }
    }
}

impl NamespaceDeclaration {
    /// A declaration of `uri` under `prefix`; the prefix is empty for a default namespace.
    #[must_use]
    pub const fn new(prefix: String, uri: String) -> Self {
        Self { prefix, uri }
    }
}
