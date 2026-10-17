//! Open Packaging Conventions (ECMA-376 Part 2): the container underneath every Office document.
//!
//! A package is a zip archive whose entries are parts addressed by a
//! [`PartName`](part_name::PartName). [`package::Package`] holds one in an
//! in-memory filesystem, with `[Content_Types].xml` ([`content_types`]) and
//! every relationship part ([`relationship`]) kept parsed.
//!
//! - [`image`]: image parts as images: a format, the bytes, and the pixel size.
//! - [`package_writer`]: a package written part by part into a sink that cannot seek.
//! - [`error`]: the failures of every package operation.
//!
//! Every byte that reaches or leaves the operating system does so through an
//! [`os_shim::System`].

extern crate alloc;

pub mod content_types;
pub mod error;
pub mod image;
pub mod package;
pub mod package_writer;
mod part_allocation;
pub mod part_name;
pub mod relationship;
mod relationship_type;
