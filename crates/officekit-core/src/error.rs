//! Failures raised by the packaging layer, and the refusals of a class's
//! reader and writer.

use core::convert::Infallible;
use std::io;

use crate::packaging::part_name::PartName;
use crate::xml::conformance::ConformanceClass;
use crate::xml::element::XmlError;

/// Why an image part could not be read. No value exists yet: the image
/// module gives it its cases.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct ImageError(Infallible);

/// Failure while opening, reading, editing, or writing a package.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PackageError {
    /// The file's conformance class has its Cargo feature off (D21).
    #[error("the {0:?} conformance class is not enabled in this build")]
    ClassNotEnabled(ConformanceClass),

    /// A part's content type is not the one the operation needs.
    #[error("the part {part} has the content type {}, not {expected}", or_none(.found.as_deref()))]
    ContentType {
        expected: &'static str,
        found: Option<String>,
        part: PartName,
    },

    /// `add_relationship` under an id the source part already uses.
    #[error("{} already has a relationship with the id {id}", source_label(.part.as_ref()))]
    DuplicateRelationship {
        id: String,
        /// The source part, `None` for the package itself.
        part: Option<PartName>,
    },

    /// An image part could not be read.
    #[error(transparent)]
    Image(ImageError),

    /// An input or output stream failed, the zip archive included.
    #[error(transparent)]
    Io(#[from] io::Error),

    /// The package has no part of this name.
    #[error("the package has no part {0}")]
    MissingPart(PartName),

    /// `remove_relationship` of an id the source part does not have.
    #[error("{} has no relationship with the id {id}", source_label(.part.as_ref()))]
    MissingRelationship {
        id: String,
        /// The source part, `None` for the package itself.
        part: Option<PartName>,
    },

    /// A part's bytes are not a `.docx`, `.xlsx`, or `.pptx` package.
    #[error("the part {part} does not hold a package: {reason}")]
    NotAPackage { part: PartName, reason: String },

    /// A name breaks the part-name rules of ECMA-376 Part 2.
    #[error("invalid part name '{name}': {reason}")]
    PartName {
        /// The offending name, as given.
        name: String,
        /// The rule it breaks.
        reason: &'static str,
    },

    /// A part's tree does not hold what the reader expects of it.
    #[error("cannot read the part {part}: {reason}")]
    Read { part: PartName, reason: String },

    /// A typed root holds content the class it is stored in has no form for,
    /// so nothing is written (D22).
    #[error("cannot write the part {part}: {error}")]
    Write { error: WriteError, part: PartName },

    /// A part is not well-formed XML, or its markup compatibility could not be
    /// processed.
    #[error("the part {part}: {error}")]
    Xml { error: XmlError, part: PartName },
}

/// Why a node cannot be written in a conformance class (D22).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WriteError {
    /// The element has no form in the class.
    #[error("{tag} has no form in {class:?}")]
    NotInClass {
        class: ConformanceClass,
        tag: &'static str,
    },
}

/// Why an element could not be read as a node of the typed tree.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReadError {
    /// The element is not the node's tag.
    #[error("expected the element {expected}, found {found}")]
    UnexpectedTag { expected: String, found: String },
}

/// A relationship's source as a message names it.
fn source_label(part: Option<&PartName>) -> String {
    part.map_or_else(
        || "the package".to_owned(),
        |name| format!("the part {name}"),
    )
}

/// An optional content type as a message names it.
fn or_none(content_type: Option<&str>) -> &str {
    content_type.unwrap_or("none")
}
