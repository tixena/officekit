//! The refusals of a class's reader and writer.

use crate::conformance::ConformanceClass;

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
