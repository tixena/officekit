//! Failures raised by the packaging layer.

use std::io;

/// Failure while opening, reading, or writing a package.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PackageError {
    /// The container is not a zip archive, or the archive is corrupt.
    #[error("invalid zip container: {0}")]
    Container(String),

    /// An input or output stream failed.
    #[error(transparent)]
    Io(#[from] io::Error),

    /// A name breaks the part-name rules of ECMA-376 Part 2.
    #[error("invalid part name '{name}': {reason}")]
    PartName {
        /// The offending name, as given.
        name: String,
        /// The rule it breaks.
        reason: &'static str,
    },
}
