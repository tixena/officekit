//! `officekit-core` reads, edits, writes, and validates Office Open XML files (`.docx`, `.xlsx`,
//! `.pptx`) in either conformance class.
//!
//! - [`packaging`]: the package, its parts, content types, and relationships, held in an os-shim
//!   in-memory filesystem.
//! - [`xml`]: the generic element tree, qualified names and the namespace table, the parsers of
//!   simple-type values, and markup compatibility.
//! - [`markup`]: the typed tree, one module per namespace, with the `Transform` and `Visit` traits
//!   and the write-through form of a transform.
//! - [`parts`]: one module per part type, with its content type, its relationship type in both
//!   classes' spelling, and the load and store of its root.
//! - `transitional` and `strict`, each behind the feature of its name: the read and write of
//!   every node of the typed tree in that conformance class.
//! - [`docx`], [`xlsx`], and [`pptx`]: the file types. Each opens a package with its main part,
//!   its class, and its kind, saves it in either class, and holds the operations over the typed
//!   tree.
//! - [`stream`]: a part read forward as events, a part written one element at a time, and a
//!   package written part by part into a sink that cannot seek.
//! - [`image`]: image parts as images: a format, the bytes, and the pixel size.
//! - [`embedded`]: a `.docx`, `.xlsx`, or `.pptx` stored in a part of another document.
//! - [`compact`]: the merge of neighbouring runs that render the same.
//! - [`folder`]: a `.docx` as a folder of its parts, and back.
//! - [`validation`]: the checks of a package against the schema of an Office version, and the
//!   semantic constraints, as static tables and the validator that walks them.
//! - [`error`]: the failures of the packaging layer and the refusals of a class's reader and
//!   writer.
//!
//! Every byte that reaches or leaves the operating system does so through an
//! [`os_shim::System`], so callers choose between the real filesystem and an in-memory one, and
//! tests never touch disk.

#![recursion_limit = "256"]

extern crate alloc;

#[cfg(any(feature = "strict", feature = "transitional"))]
mod class_rules;
pub mod compact;
pub mod docx;
pub mod embedded;
pub mod error;
mod file;
pub mod folder;
pub mod image;
pub mod markup;
mod new_parts;
pub mod packaging;
pub mod parts;
pub mod pptx;
pub mod stream;
#[cfg(feature = "strict")]
pub mod strict;
#[cfg(feature = "transitional")]
pub mod transitional;
pub mod validation;
pub mod xlsx;
pub mod xml;
