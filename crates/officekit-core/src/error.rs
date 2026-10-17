//! Failures raised by the packaging layer, and the refusals of a class's
//! reader and writer.

use std::io;

use crate::image::ImageError;
use crate::packaging::part_name::PartName;
use crate::xml::conformance::ConformanceClass;
use crate::xml::element::XmlError;

/// Failure while opening, reading, editing, or writing a package.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PackageError {
    #[error("the {0:?} conformance class is not enabled in this build")]
    ClassNotEnabled(ConformanceClass),

    #[error("the part {part} has the content type {}, not {expected}", or_none(.found.as_deref()))]
    ContentType {
        expected: &'static str,
        found: Option<String>,
        part: PartName,
    },

    #[error("{} already has a relationship with the id {id}", source_label(.part.as_ref()))]
    DuplicateRelationship {
        id: String,
        /// The source part, `None` for the package itself.
        part: Option<PartName>,
    },

    #[error(transparent)]
    Image(ImageError),

    /// An input or output stream failed, the zip archive included.
    #[error(transparent)]
    Io(#[from] io::Error),

    #[error("the package has no part {0}")]
    MissingPart(PartName),

    #[error("{} has no relationship with the id {id}", source_label(.part.as_ref()))]
    MissingRelationship {
        id: String,
        /// The source part, `None` for the package itself.
        part: Option<PartName>,
    },

    #[error("the part {part} does not hold a package: {reason}")]
    NotAPackage { part: PartName, reason: String },

    /// A name breaks the part-name rules of ECMA-376 Part 2.
    #[error("invalid part name '{name}': {reason}")]
    PartName { name: String, reason: &'static str },

    #[error("cannot read the part {part}: {reason}")]
    Read { part: PartName, reason: String },

    /// A typed root holds content its class has no form for; nothing is written.
    #[error("cannot write the part {part}: {error}")]
    Write { error: WriteError, part: PartName },

    /// A part is not well-formed XML, or its markup compatibility could not be
    /// processed.
    #[error("the part {part}: {error}")]
    Xml { error: XmlError, part: PartName },
}

/// Why a node cannot be written in a conformance class.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WriteError {
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
