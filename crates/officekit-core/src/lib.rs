//! `officekit-core` - Office Open XML (ECMA-376) document model for Rust.
//!
//! The layering follows the Open XML SDK:
//!
//! - [`packaging`] ports `DocumentFormat.OpenXml.Packaging` and the Open
//!   Packaging Conventions underneath it (ECMA-376 Part 2): part names,
//!   content types, relationships, and the zip container.
//! - [`xml`] ports the root `DocumentFormat.OpenXml` namespace: the element
//!   and attribute model, simple types, and the streaming reader and writer.
//! - [`parts`] holds one generated module per part type: its content type,
//!   its relationship type in both classes, and the load and store of its
//!   root.
//! - [`validation`] ports `DocumentFormat.OpenXml.Validation`.
//! - [`docx`], [`xlsx`], and [`pptx`] hold the file types: a package with
//!   its main part, its class, and its kind, the saves in either class, and
//!   the operations over the generated types.
//! - `transitional` and `strict`, each behind the feature of its name, read
//!   and write every node of the typed tree in that conformance class.
//!
//! Every byte that reaches or leaves the operating system does so through an
//! [`os_shim::System`], so callers choose between the real filesystem and an
//! in-memory one, and tests never touch disk.

extern crate alloc;

#[cfg(any(feature = "strict", feature = "transitional"))]
mod class_rules;
pub mod compact;
pub mod docx;
pub mod error;
mod file;
pub mod folder;
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
