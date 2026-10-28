//! `officekit-markup` is the typed tree of officekit: a Rust type for each element of Office Open
//! XML, in one module for each namespace.
//!
//! A namespace is the group an element name belongs to. `wordprocessing` holds the elements of Word
//! files, `spreadsheet` those of Excel files, `presentation` those of slide decks, and
//! `drawing` and `chart` the pictures, shapes and charts all three share. The modules are
//! `officekit_markup::<namespace>`.
//!
//! # A namespace module
//!
//! A struct stands for an element with attributes and children, and its fields are the attributes
//! and children the schema declares. An enum stands for a choice among elements, such as the
//! content of a paragraph. A newtype stands for a simple type, a value written as text. It keeps the
//! text as it was read, and `value()` checks it and gives the matching member or an error.
//!
//! To get the text of a Word paragraph, loop over the `content` of the `wordprocessing::Paragraph`,
//! match `ParagraphContent::TextRun`, loop over the `content` of the `TextRun`, and match
//! `TextRunContent::Text`. Its `value` is the text.
//!
//! Each module also has a `read` module, with one function that reads an element into each struct
//! or enum, and a `write` module, with one function that writes each back to an element. The
//! elements are those of `officekit-xml`. Strict and Transitional files read the same way.
//! `officekit-strict` and `officekit::transitional` write a node in one class.
//!
//! # Content the schemas do not declare
//!
//! The struct of an element has an `unknown` field, an [`Unknown`], and the enum of a choice among
//! elements an `Other` item. Elements, attributes and namespace declarations the schemas do not
//! declare go there when a node is read. A write puts them back where they were, so unfamiliar
//! content survives an edit. A newtype keeps any text, so a value that is no member of its simple
//! type stays in it, and `value()` gives an error for it.
//!
//! Every struct is `#[non_exhaustive]`, and so is every enum but a few choices among elements that
//! this crate's hooks match. Build a struct with `Default` and assign its fields, and give a
//! `match` on an enum a wildcard arm.
//!
//! # Hooks
//!
//! A hook is a method with a default body that does nothing. A program implements the ones it
//! cares about, and `officekit_strict::traversal` calls them while it walks a typed tree. Reading
//! and writing a node alone never calls a hook.
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
//! # Where the items live
//!
//! The runtime is at the root of this crate: the reader, the writer, [`Unknown`] and the simple-type
//! parsers. It comes from `officekit-markup-base`, with the namespaces the cycle of
//! `officekit-markup-core` depends on. `officekit-markup-core` holds the namespaces that refer to
//! each other. This crate holds the other namespaces and, in the module of every namespace, its
//! nodes and its hook traits. Depend on this crate, not on those two.
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

#[doc(no_inline)]
pub use officekit_markup_base::*;

use std::io;

use officekit_xml::element::Element;
use officekit_xml::stream::writer::PartWriter;

// Not a module: its traits are the crate root's `Context` and `Visitor`, which every
// namespace's hook traits extend as `crate::Context` and `crate::Visitor`.
include!("root.rs");
