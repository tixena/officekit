//! `officekit-markup-ext` holds the other namespaces of officekit's typed tree, its root hooks
//! [`Context`] and [`Visitor`], and the hook traits of every namespace.
//!
//! Most code does not name this crate. The `officekit-markup` facade gathers it with
//! `officekit-markup-base` and `officekit-markup-core` under one path per namespace,
//! `officekit_markup::<namespace>`, and `officekit` builds on that facade.
//!
//! # What is here
//!
//! - The namespaces that neither `officekit-markup-base` nor `officekit-markup-core` hold,
//!   mostly the `office_*` extensions that Microsoft Office adds to the
//!   main languages, such as threaded comments, slicers and the newer chart types.
//! - For every namespace of the three crates, a module of the same name with that namespace's
//!   hook traits `Context` and `Visitor`. A module of `officekit-markup-base` or
//!   `officekit-markup-core` is re-exported here, so the nodes and the hooks of a namespace
//!   are on one page.
//! - The root traits [`Context`] and [`Visitor`], at the crate root.
//!
//! # Hooks
//!
//! A hook is a method with a default body that does nothing. A program implements the ones it
//! cares about, and `officekit-traversal` calls them while it walks a typed tree. Reading and
//! writing a node alone never calls a hook.
//!
//! A [`Context`] changes or writes nodes:
//!
//! - A tree hook, one per node type, such as `wordprocessing::Context::paragraph`. It returns
//!   `Some(node)` to replace the node, or `None` to keep it and go on to its children.
//! - A list hook, one per enum, such as `wordprocessing::Context::paragraph_content_items`.
//!   It returns `Some(items)` to replace an item with any number of items, none included, or
//!   `None` to leave the item alone.
//! - A write hook, one per node type, such as `wordprocessing::Context::write_paragraph`. It
//!   writes the node to a `PartWriter` itself and answers [`Written::Yes`], or answers
//!   [`Written::No`] to let the node be written the usual way.
//!
//! A [`Visitor`] only looks. Its hook, such as `wordprocessing::Visitor::paragraph`, answers
//! [`Descend::Yes`] to walk into the children of the node and [`Descend::No`] to skip them.
//!
//! The root [`Context`] and [`Visitor`] have one accessor per namespace, such as
//! `wordprocessing()`, that returns `None`. A context opts into the hooks of a namespace by
//! implementing that namespace's `Context` trait and returning `Some(self)` from its accessor.
//! A namespace whose accessor returns `None` is never asked. Both root traits also have an
//! `element` hook for generic elements, those the typed tree holds as unknown content.
//!
//! # Features
//!
//! - `binary-format`: every type also derives the `rkyv` archive traits, for officekit's
//!   binary form.
//!
//! This crate builds no doctest. The runnable example of reading and writing a node is on the
//! `officekit-markup-base` page.

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
