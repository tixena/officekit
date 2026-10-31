//! Failures raised by the packaging layer.

use std::io;

use os_shim::error::StorageError;

use crate::conformance::ConformanceClass;
use crate::element::XmlError;
use crate::error::WriteError;

use crate::packaging::image::ImageError;
use crate::packaging::part_name::PartName;

/// Failure while opening, reading, editing, or writing a package.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PackageError {
    /// The file is in a class whose Cargo feature, `strict` or `transitional`, is off.
    #[error("the {0:?} conformance class is not enabled in this build")]
    ClassNotEnabled(ConformanceClass),

    /// A part has a content type other than the one the operation needs.
    #[error("the part {part} has the content type {}, not {expected}", or_none(.found.as_deref()))]
    ContentType {
        /// The content type the operation needs.
        expected: &'static str,
        /// The part's content type, `None` when it has none.
        found: Option<String>,
        /// The part.
        part: PartName,
    },

    /// A relationship id is already taken.
    #[error("{} already has a relationship with the id {id}", source_label(.part.as_ref()))]
    DuplicateRelationship {
        /// The relationship id.
        id: String,
        /// The source part, `None` for the package itself.
        part: Option<PartName>,
    },

    /// Bytes are not an image the crate recognises.
    #[error(transparent)]
    Image(ImageError),

    /// An input or output stream failed, the zip archive included.
    #[error(transparent)]
    Io(#[from] io::Error),

    /// The package has no part of that name.
    #[error("the package has no part {0}")]
    MissingPart(PartName),

    /// A source has no relationship with that id.
    #[error("{} has no relationship with the id {id}", source_label(.part.as_ref()))]
    MissingRelationship {
        /// The relationship id.
        id: String,
        /// The source part, `None` for the package itself.
        part: Option<PartName>,
    },

    /// A part was opened as an embedded package and is not a zip archive, such as an OLE object.
    #[error("the part {part} does not hold a package: {reason}")]
    NotAPackage {
        /// The part.
        part: PartName,
        /// Why it is not a package.
        reason: String,
    },

    /// A name breaks the part-name rules of ECMA-376 Part 2.
    #[error("invalid part name '{name}': {reason}")]
    PartName {
        /// The name as given.
        name: String,
        /// The rule it breaks.
        reason: &'static str,
    },

    /// A part holds XML that the operation could not read as what it should be.
    #[error("cannot read the part {part}: {reason}")]
    Read {
        /// The part.
        part: PartName,
        /// What could not be read.
        reason: String,
    },

    /// A typed root holds content its class has no form for; nothing is written.
    #[error("cannot write the part {part}: {error}")]
    Write {
        /// What has no form in the class.
        error: WriteError,
        /// The part.
        part: PartName,
    },

    /// A part is not well-formed XML, or its markup compatibility could not be
    /// processed.
    #[error("the part {part}: {error}")]
    Xml {
        /// The XML failure.
        error: XmlError,
        /// The part.
        part: PartName,
    },
}

impl From<StorageError> for PackageError {
    fn from(error: StorageError) -> Self {
        Self::Io(error.into())
    }
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
