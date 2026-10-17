//! `officekit-xml` is the XML layer of officekit: the generic element tree every Office part parses
//! into, qualified names and the namespace table, the parsers of simple-type values, markup
//! compatibility, the conformance class, and a part read forward as events or written one element
//! at a time.
//!
//! - [`element`]: the generic element tree.
//! - [`name`]: qualified names, the namespace table, and the Office versions.
//! - [`parse`] and [`write`]: a part's bytes into the tree and back, and the namespace declarations
//!   in scope while a typed tree is written.
//! - [`values`]: the parsers of simple-type values.
//! - [`mc`]: markup compatibility.
//! - [`conformance`]: the two conformance classes of ECMA-376.
//! - [`stream`]: a part read forward as events, and a part written one element at a time.
//! - [`error`]: the refusals of a class's reader and writer.

extern crate alloc;

pub mod conformance;
pub mod element;
pub mod error;
pub mod mc;
pub mod name;
pub mod parse;
pub mod stream;
pub mod values;
pub mod write;
