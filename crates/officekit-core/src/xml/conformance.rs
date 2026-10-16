//! The two conformance classes of ECMA-376.

#[cfg(test)]
mod tests;

use crate::parts::main_document;

/// The two conformance classes (D21).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConformanceClass {
    Strict,
    Transitional,
}

impl ConformanceClass {
    /// Whether this build carries the class: its feature is on.
    #[must_use]
    pub const fn enabled(self) -> bool {
        match self {
            Self::Strict => cfg!(feature = "strict"),
            Self::Transitional => cfg!(feature = "transitional"),
        }
    }

    /// The class a package declares through the type of its relationship to
    /// its main part: either spelling of the `officeDocument` type; `None` for
    /// any other type.
    #[must_use]
    pub fn from_office_document_relationship(type_uri: &str) -> Option<Self> {
        let office_document = main_document::RELATIONSHIP_TYPE;
        if type_uri == office_document.strict {
            Some(Self::Strict)
        } else if type_uri == office_document.transitional {
            Some(Self::Transitional)
        } else {
            None
        }
    }
}
