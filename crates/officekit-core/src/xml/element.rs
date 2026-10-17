//! The generic element tree: every XML part parses into it, the typed tree is
//! built from it and written through it, and the validator walks it.

use super::conformance::ConformanceClass;
use super::mc::{self, MarkupCompatibility};
use super::name::{Namespace, QName};
use super::write::{self, WriteOptions};

#[cfg(test)]
mod tests;

/// An attribute: its qualified name and its value, unescaped.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Attribute {
    pub name: QName,
    pub value: String,
}

/// A whole part: its XML declaration as written
/// (`xml version="1.0" encoding="UTF-8" standalone="yes"`), what comes before
/// and after the root, and the root.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Document {
    pub declaration: Option<String>,
    pub epilog: Vec<Node>,
    pub prolog: Vec<Node>,
    pub root: Element,
}

/// One element: attributes and namespace declarations, each in document
/// order, and children in order.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Element {
    pub attributes: Vec<Attribute>,
    pub children: Vec<Node>,
    pub name: QName,
    pub namespace_declarations: Vec<NamespaceDeclaration>,
}

/// An `xmlns` or `xmlns:prefix` attribute, with the URI exactly as written, in
/// either class.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct NamespaceDeclaration {
    pub prefix: String,
    pub uri: String,
}

/// A node of the tree. Text is unescaped; a comment, a CDATA section, and a
/// processing instruction hold their content as written.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Node {
    CData(String),
    Comment(String),
    Element(Element),
    ProcessingInstruction { content: String, target: String },
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
    #[error("mc:MustUnderstand names `{prefix}`, which the target does not understand")]
    MustUnderstand { prefix: String },
    #[error("undeclared namespace prefix `{0}`")]
    UndeclaredPrefix(String),
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

    #[must_use]
    pub fn to_xml(&self, options: WriteOptions) -> Vec<u8> {
        write::document(self, options)
    }
}

impl Element {
    #[must_use]
    pub fn attribute(&self, namespace: &Namespace, local: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|attribute| {
                attribute.name.local == local && attribute.name.namespace == *namespace
            })
            .map(|attribute| attribute.value.as_str())
    }

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
