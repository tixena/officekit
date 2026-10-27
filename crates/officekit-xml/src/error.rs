//! The refusals of a class's reader and writer.

use crate::conformance::ConformanceClass;

/// Why a node cannot be written in a conformance class.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WriteError {
    /// The node, an element or an attribute value, has no form in the class.
    #[error("{tag} has no form in {class:?}")]
    NotInClass {
        /// The class the node was written in.
        class: ConformanceClass,
        /// The node's tag in the namespace table, such as `c/@t`.
        tag: &'static str,
    },
}

/// Why an element could not be read as a node of the typed tree.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReadError {
    /// The element has another tag than the node reads.
    #[error("expected the element {expected}, found {found}")]
    UnexpectedTag {
        /// The tag the node reads.
        expected: String,
        /// The tag the element has.
        found: String,
    },
}
