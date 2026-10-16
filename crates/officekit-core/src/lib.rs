//! `officekit-core` - Office Open XML (ECMA-376) document model for Rust.
//!
//! The layering follows the Open XML SDK:
//!
//! - [`packaging`] ports `DocumentFormat.OpenXml.Packaging` and the Open
//!   Packaging Conventions underneath it (ECMA-376 Part 2): part names,
//!   content types, relationships, and the zip container.
//! - [`xml`] ports the root `DocumentFormat.OpenXml` namespace: the element
//!   and attribute model, simple types, and the streaming reader and writer.
//! - [`validation`] ports `DocumentFormat.OpenXml.Validation`.
//! - [`wordprocessing`], [`spreadsheet`], and [`presentation`] port the
//!   typed document classes and the generated schema namespaces behind them.
//!
//! Every byte that reaches or leaves the operating system does so through an
//! [`os_shim::System`], so callers choose between the real filesystem and an
//! in-memory one, and tests never touch disk.

extern crate alloc;

pub mod compact;
pub mod error;
pub mod folder;
pub mod markup;
pub mod packaging;
pub mod presentation;
pub mod spreadsheet;
pub mod stream;
pub mod validation;
pub mod wordprocessing;
pub mod xml;
