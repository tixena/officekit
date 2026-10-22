//! `officekit-core` reads, edits, writes, and validates Office Open XML files (`.docx`, `.xlsx`,
//! `.pptx`) in either conformance class, over the typed tree of `officekit_markup`. The classes
//! are `officekit_transitional` and `officekit_strict`, each turned on by the feature of its name;
//! the part modules are `officekit_parts`.
//!
//! - [`docx`], [`xlsx`], and [`pptx`]: the file types. Each opens a package with its main part,
//!   its class, and its kind, saves it in either class, and holds the operations over the typed
//!   tree.
//! - [`embedded`]: a `.docx`, `.xlsx`, or `.pptx` stored in a part of another document.
//! - [`compact`]: the merge of neighbouring runs that render the same.
//! - [`folder`]: a `.docx` as a folder of its parts, and back.
//! - [`office_file`]: a `.docx`, `.xlsx`, or `.pptx` opened from its bytes without knowing which.
//! - [`binary`]: officekit's binary form, a file's whole model archived so the next call parses no
//!   XML.
#![cfg_attr(
    feature = "markdown",
    doc = "- [`markdown`]: a Word document from an archie document, and back, with the feature `markdown`."
)]
#![cfg_attr(
    not(feature = "markdown"),
    doc = "- `markdown`: a Word document from an archie document, and back, with the feature `markdown`."
)]
//!
//! Every byte that reaches or leaves the operating system does so through an
//! [`os_shim::System`], so callers choose between the real filesystem and an in-memory one, and
//! tests never touch disk.

extern crate alloc;

pub mod binary;
pub mod compact;
pub mod docx;
pub mod embedded;
mod file;
pub mod folder;
#[cfg(feature = "markdown")]
pub mod markdown;
mod new_parts;
pub mod office_file;
pub mod pptx;
pub mod xlsx;
