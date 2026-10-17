//! `officekit-core` reads, edits, writes, and validates Office Open XML files (`.docx`, `.xlsx`,
//! `.pptx`) in either conformance class, over the typed tree of `officekit_markup`.
//!
//! - [`parts`]: one module per part type, with its content type, its relationship type in both
//!   classes' spelling, and the load and store of its root.
//! - [`part_root`]: a part's typed root, and its load from and store into a package.
//! - `transitional` and `strict`, each behind the feature of its name: the read and write of
//!   every node of the typed tree in that conformance class.
//! - [`docx`], [`xlsx`], and [`pptx`]: the file types. Each opens a package with its main part,
//!   its class, and its kind, saves it in either class, and holds the operations over the typed
//!   tree.
//! - [`embedded`]: a `.docx`, `.xlsx`, or `.pptx` stored in a part of another document.
//! - [`compact`]: the merge of neighbouring runs that render the same.
//! - [`folder`]: a `.docx` as a folder of its parts, and back.
//!
//! Every byte that reaches or leaves the operating system does so through an
//! [`os_shim::System`], so callers choose between the real filesystem and an in-memory one, and
//! tests never touch disk.

extern crate alloc;

pub mod compact;
pub mod docx;
pub mod embedded;
mod file;
pub mod folder;
mod new_parts;
pub mod part_root;
pub mod parts;
pub mod pptx;
#[cfg(feature = "strict")]
pub mod strict;
#[cfg(feature = "transitional")]
pub mod transitional;
pub mod xlsx;
