//! `officekit-markup-ext` is the typed tree's other namespace modules, the root hooks `Context`
//! and `Visitor`, and for each namespace of `officekit-markup-base` and `officekit-markup-core` a
//! module with its hook traits beside its nodes; `officekit-markup` gathers them under its paths.

#![recursion_limit = "256"]

// The namespace modules, generated in a file of their own: module declarations
// go before the imports, and `include!` places them where it stands.
include!("modules.rs");

use std::io;

use officekit_markup_base::{Descend, Written};
use officekit_xml::element::Element;
use officekit_xml::stream::writer::PartWriter;

// Not a module: its traits are the crate root's `Context` and `Visitor`, which every
// namespace's hook traits extend as `crate::Context` and `crate::Visitor`.
include!("root.rs");
