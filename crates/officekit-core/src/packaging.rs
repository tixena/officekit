//! Open Packaging Conventions (ECMA-376 Part 2): the container underneath every Office document.
//!
//! A package is a zip archive whose entries are parts addressed by a
//! [`PartName`](part_name::PartName). [`package::Package`] holds one in an
//! in-memory filesystem, with `[Content_Types].xml` ([`content_types`]) and
//! every relationship part ([`relationship`]) kept parsed.

pub mod content_types;
pub mod package;
mod part_allocation;
pub mod part_name;
pub mod relationship;
