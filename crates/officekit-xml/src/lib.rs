//! The XML layer of officekit: a generic element tree, qualified names, and a part read or
//! written as XML.
//!
//! Every part of an Office file that is not an image or another binary is XML. This crate parses
//! one into a [`Document`](element::Document) and writes it back. It also holds what the
//! parts share: the table of Office namespaces, the parsers of simple-type values such as `2.5cm`,
//! markup compatibility, and the two conformance classes of ECMA-376, Strict and Transitional.
//!
//! The crate is the bottom of officekit. `officekit-packaging` stores parsed parts as
//! [`Document`](element::Document)s, and the typed tree in `officekit-markup` is read from and
//! written to [`Element`](element::Element)s. Use it directly to read or change a part without a
//! typed model, or to find out what a name, a value or a class means.
//!
//! - [`element`][]: the tree: [`Document`](element::Document), [`Element`](element::Element),
//!   [`Node`](element::Node), and [`XmlError`](element::XmlError).
//! - [`parse`][]: [`parse`](parse::parse), a part's bytes into a [`Document`](element::Document).
//! - [`mod@write`][]: [`WriteOptions`](write::WriteOptions) and [`Layout`](write::Layout), the
//!   options of [`Document::to_xml`](element::Document::to_xml), and the namespace declarations in
//!   scope while a typed tree is written.
//! - [`name`][]: [`QName`](name::QName), [`Namespace`](name::Namespace), the generated table of
//!   known namespaces, and [`OfficeVersion`](name::OfficeVersion).
//! - [`values`][]: the parsers of simple-type values.
//! - [`mc`][]: markup compatibility, which resolves `mc:AlternateContent` for an Office version.
//! - [`conformance`][]: [`ConformanceClass`](conformance::ConformanceClass).
//! - [`stream`][]: a part read forward as events, and a part written one element at a time.
//! - [`error`][]: [`WriteError`](error::WriteError) and [`ReadError`](error::ReadError), the
//!   refusals of a class's writer and reader.
//!
//! A namespace has one value for its Strict and its Transitional URI. A tree parsed from a Strict
//! file and one from a Transitional file hold the same
//! [`Namespace::Known`](name::Namespace::Known) ids; the URI is chosen when the tree is written.
//!
//! # Features
//!
//! - `binary-format`: derives the `rkyv` archive of the tree and the names, which officekit's
//!   binary form stores. It adds no function.
//!
//! # Parse, change and write a part
//!
//! ```
//! use officekit_xml::element::Attribute;
//! use officekit_xml::name::{Namespace, QName};
//! use officekit_xml::parse::parse;
//! use officekit_xml::write::{Layout, WriteOptions};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let word = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
//! let xml = format!(r#"<w:p xmlns:w="{word}"><w:r><w:t>Dear Sam,</w:t></w:r></w:p>"#);
//! let mut document = parse(xml.as_bytes())?;
//! assert_eq!(document.root.name.local, "p");
//!
//! let run = document.root.child_elements().next().ok_or("no run")?;
//! assert_eq!(run.name, QName::prefixed("w", "r", word));
//!
//! let name = QName::prefixed("w", "rsidR", word);
//! document.root.attributes.push(Attribute::new(name, "00A1".to_owned()));
//! assert_eq!(
//!     document.root.attribute(&Namespace::from_uri(word), "rsidR"),
//!     Some("00A1")
//! );
//!
//! let text = String::from_utf8(document.to_xml(WriteOptions::new(Layout::Compact, false)))?;
//! assert_eq!(
//!     text,
//!     format!(r#"<w:p xmlns:w="{word}" w:rsidR="00A1"><w:r><w:t>Dear Sam,</w:t></w:r></w:p>"#)
//! );
//! # Ok(())
//! # }
//! ```
//!
//! # Names, classes and values
//!
//! ```
//! use officekit_xml::conformance::ConformanceClass;
//! use officekit_xml::name::Namespace;
//! use officekit_xml::values::MeasureUnit;
//! use officekit_xml::values::parse::{boolean, measure};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let transitional =
//!     Namespace::from_uri("http://schemas.openxmlformats.org/wordprocessingml/2006/main");
//! let strict = Namespace::from_uri("http://purl.oclc.org/ooxml/wordprocessingml/main");
//! assert_eq!(transitional, strict);
//! assert_eq!(
//!     strict.uri(ConformanceClass::Transitional),
//!     "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
//! );
//! assert_eq!(
//!     transitional.uri(ConformanceClass::Strict),
//!     "http://purl.oclc.org/ooxml/wordprocessingml/main"
//! );
//!
//! assert_eq!(boolean("1"), Some(true));
//! let width = measure("2.5cm").ok_or("not a measure")?;
//! assert_eq!(width.unit, MeasureUnit::Centimeters);
//! assert_eq!(width.value, 2.5);
//! # Ok(())
//! # }
//! ```

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
