//! Relationships (ECMA-376 Part 2, 9.3): what each relationship part holds,
//! how a target resolves against its source, and the relationship types in
//! both conformance classes' spelling.

use core::iter;

use crate::error::PackageError;
use crate::packaging::package::DECLARATION;
use crate::packaging::part_name::PartName;
use crate::xml::element::{Attribute, Document, Element, NamespaceDeclaration, Node};
use crate::xml::name::{Namespace, QName};

#[cfg(test)]
mod tests;

/// The namespace of a relationship part.
pub(crate) const NAMESPACE: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

/// The content type of a relationship part, the `rels` extension's `Default`.
pub(crate) const RELATIONSHIPS_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-package.relationships+xml";

/// A relationship with its target resolved and the target part's content type.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RelatedPart {
    /// The target part's content type; `None` for an external target.
    pub content_type: Option<String>,
    pub id: String,
    pub target: Target,
    pub type_uri: String,
}

/// One `Relationship` element, as written.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Relationship {
    pub id: String,
    /// The `Target` attribute: relative to the source's folder when internal.
    pub target: String,
    pub target_mode: TargetMode,
    pub type_uri: String,
}

/// A relationship type in both classes' spelling; its constants and
/// methods are generated, in `relationship_type.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RelationshipType {
    pub strict: &'static str,
    pub transitional: &'static str,
}

/// Where a relationship points: a part of this package, or a URI outside it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Target {
    External(String),
    Part(PartName),
}

/// The `TargetMode` attribute of a relationship.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TargetMode {
    External,
    Internal,
}

impl Relationship {
    /// A relationship of type `type_uri` to `target`, relative to the source's
    /// folder when internal, with no id yet.
    #[must_use]
    pub fn new(type_uri: &str, target: &str, target_mode: TargetMode) -> Self {
        Self {
            id: String::new(),
            target: target.to_owned(),
            target_mode,
            type_uri: type_uri.to_owned(),
        }
    }

    /// Where the relationship points, an internal target resolved against
    /// `source_directory`.
    pub(crate) fn resolved(&self, source_directory: &str) -> Result<Target, PackageError> {
        match self.target_mode {
            TargetMode::External => Ok(Target::External(self.target.clone())),
            TargetMode::Internal => resolve(source_directory, &self.target).map(Target::Part),
        }
    }
}

/// The id a new relationship among `items` takes: `rId` and one more than the
/// highest `rIdN` they have, so an id removed earlier is not reused while
/// markup may still name it.
pub(crate) fn next_id(items: &[Relationship]) -> String {
    let highest = items
        .iter()
        .filter_map(|item| item.id.strip_prefix("rId")?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    format!("rId{}", highest + 1)
}

/// Read the relationships of a parsed relationship part, in order.
pub(crate) fn parse(document: &Document) -> Result<Vec<Relationship>, String> {
    if document.root.name.local != "Relationships" {
        return Err(format!(
            "the root is <{}>, not <Relationships>",
            document.root.name.local
        ));
    }
    document
        .root
        .child_elements()
        .map(|element| {
            let attribute = |local: &str| {
                element
                    .attribute(&Namespace::None, local)
                    .map(str::to_owned)
                    .ok_or_else(|| format!("a relationship without {local}"))
            };
            let target_mode =
                if element.attribute(&Namespace::None, "TargetMode") == Some("External") {
                    TargetMode::External
                } else {
                    TargetMode::Internal
                };
            Ok(Relationship {
                id: attribute("Id")?,
                target: attribute("Target")?,
                target_mode,
                type_uri: attribute("Type")?,
            })
        })
        .collect()
}

/// The target a relationship from `source_directory` writes for `target`:
/// `../embeddings/sheet.xlsx` from `/word/charts`, `word/document.xml` from
/// the package, whose directory is the empty string.
pub(crate) fn relative(source_directory: &str, target: &PartName) -> String {
    let from: Vec<&str> = source_directory.split('/').skip(1).collect();
    let to: Vec<&str> = target.as_str().split('/').skip(1).collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count()
        .min(to.len() - 1);
    let mut segments: Vec<&str> = iter::repeat_n("..", from.len() - common).collect();
    segments.extend(&to[common..]);
    segments.join("/")
}

/// Resolve an internal target against its source's directory, `..` segments
/// included (Part 2, 9.3.5).
pub(crate) fn resolve(source_directory: &str, target: &str) -> Result<PartName, PackageError> {
    let joined = if target.starts_with('/') {
        target.to_owned()
    } else {
        format!("{source_directory}/{target}")
    };
    let mut segments: Vec<&str> = Vec::new();
    for segment in joined.split('/').skip(1) {
        match segment {
            ".." => {
                segments.pop();
            }
            "." => {}
            other => segments.push(other),
        }
    }
    PartName::new(&format!("/{}", segments.join("/")))
}

/// The relationship part that holds `items`.
pub(crate) fn to_document(items: &[Relationship]) -> Document {
    let mut root = Element::new(QName::unprefixed("Relationships", NAMESPACE));
    root.namespace_declarations.push(NamespaceDeclaration {
        prefix: String::new(),
        uri: NAMESPACE.to_owned(),
    });
    for relationship in items {
        let mut attributes = vec![
            ("Id", relationship.id.as_str()),
            ("Type", relationship.type_uri.as_str()),
            ("Target", relationship.target.as_str()),
        ];
        if relationship.target_mode == TargetMode::External {
            attributes.push(("TargetMode", "External"));
        }
        let mut element = Element::new(QName::unprefixed("Relationship", NAMESPACE));
        element.attributes = attributes
            .into_iter()
            .map(|(name, value)| Attribute {
                name: QName::unprefixed(name, ""),
                value: value.to_owned(),
            })
            .collect();
        root.children.push(Node::Element(element));
    }
    Document::new(DECLARATION, root)
}
