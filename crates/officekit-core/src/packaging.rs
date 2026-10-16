//! Open Packaging Conventions (ECMA-376 Part 2): the container underneath every Office document.
//!
//! Ports `DocumentFormat.OpenXml.Packaging` together with the
//! `System.IO.Packaging` layer the SDK builds on. A package is a zip archive
//! whose entries are parts addressed by a [`PartName`](part_name::PartName).
//! The `[Content_Types].xml` stream maps each part to a content type, and
//! `.rels` parts hold the relationships between parts.
//!
//! Present today: part names and the container ([`package`]). Still to port:
//! content types, relationships, and the typed part classes.

pub mod package;
pub mod part_name;
