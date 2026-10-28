//! `officekit-markup-core` holds the namespaces of officekit's typed tree that reference each
//! other in a cycle.
//!
//! Most code does not name this crate. The `officekit-markup` facade gathers it with
//! `officekit-markup-base` and `officekit-markup-ext` under one path per namespace,
//! `officekit_markup::<namespace>`, and `officekit` builds on that facade.
//!
//! # What is here
//!
//! One module per namespace of the three document kinds and of the drawing and equation
//! languages they share:
//!
//! - [`wordprocessing`], [`spreadsheet`] and [`presentation`]: the main parts of a `.docx`, a
//!   `.xlsx` and a `.pptx`.
//! - [`drawing`], [`picture`], [`chart`], [`diagram`], [`locked_canvas`], [`math`] and [`vml`]:
//!   the drawing, picture, chart, diagram, equation and legacy shape languages those parts embed.
//! - [`wordprocessing_drawing`], [`spreadsheet_drawing`] and [`chart_drawing`]: how a drawing
//!   sits in a Word page, an Excel sheet or a chart.
//! - The `office_*` modules: the extensions that Microsoft Office adds to
//!   those namespaces.
//!
//! A module holds the structs, enums and simple-type newtypes of its namespace, with a `read`
//! module and a `write` module for them. The runtime they use, and the way a node keeps content
//! the schemas do not declare, is described on the `officekit-markup-base` page.
//!
//! The hook traits `Context` and `Visitor` of each namespace in this crate are in
//! `officekit-markup-ext`, which re-exports these modules beside them.
//!
//! # Features
//!
//! - `binary-format`: every type also derives the `rkyv` archive traits, for officekit's
//!   binary form.
//!
//! This crate builds no doctest. The runnable example of reading and writing a node is on the
//! `officekit-markup-base` page.

#![recursion_limit = "256"]
#![allow(
    clippy::exhaustive_enums,
    reason = "ext matches the enums its `From` impls convert arm by arm, which `#[non_exhaustive]` forbids across crates"
)]

// The namespace modules, generated in a file of their own.
include!("modules.rs");
