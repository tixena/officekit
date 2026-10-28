//! The Transitional conformance class of officekit (ECMA-376 Part 1 with the migration features
//! of Part 4): it reads and writes every node of the typed tree in Transitional.
//!
//! Office Open XML has two conformance classes. Transitional is what Word, Excel and `PowerPoint`
//! have always written, and the one most files are in. Strict is the other, with other
//! namespace URIs and some other spellings. A file says which one it is in the relationship that
//! names its main part.
//!
//! Most code does not name this module. The feature `transitional`, which is on by default, turns
//! it on, and the `save_transitional` and `to_bytes_transitional` of the file types write a whole
//! file in Transitional. Use this module to write one node, or one part, in Transitional.
//!
//! # The [`Transitional`] trait
//!
//! [`Transitional`] is implemented for every node of every namespace of `officekit_markup`.
//!
//! - [`Transitional::from_transitional`] reads a node from a generic element. Both classes read
//!   through the same reader, so a Strict paragraph and a Transitional one read into equal
//!   structs. Only the tag is checked.
//! - [`Transitional::to_transitional`] writes the node with the writer both classes share. It
//!   keeps the stored text and gives every known namespace it declares its Transitional URI. A
//!   main part's root loses `conformance="strict"`, since an absent attribute means
//!   transitional.
//!
//! The one value Transitional has no place for is a date cell, `t="d"`, which Strict has. The
//! write refuses it with [`WriteError::NotInClass`], whose tag is `c/@t`.
//!
//! # Date cells
//!
//! The [`dates`] module turns the date cells of a worksheet into number cells before a save.
//! [`dates::convert_date_cells`] changes each one that holds a date and returns a
//! [`dates::SaveWarning`] for it. A date cell with no value loses its type with no warning. A date
//! cell whose text is not a date stays, and a Transitional write refuses it.
//! [`dates::serial_date_time`] is the number a date becomes.
//!
//! # Example
//!
//! Write a Strict document in Transitional, and convert a date cell so that a worksheet can be
//! written.
//!
//! ```
//! use officekit_markup::spreadsheet::Worksheet;
//! use officekit_markup::wordprocessing::Document;
//! use officekit_xml::packaging::part_name::PartName;
//! use officekit::transitional::Transitional as _;
//! use officekit::transitional::dates::convert_date_cells;
//! use officekit_xml::element::{Document as Part, Element};
//! use officekit_xml::error::WriteError;
//! use officekit_xml::parse::parse;
//! use officekit_xml::write::{Layout, WriteOptions};
//!
//! const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
//! const W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
//! const X: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
//!
//! # fn element(text: &str) -> Element {
//! #     parse(text.as_bytes()).expect("well-formed XML").root
//! # }
//! # fn text(root: Element) -> String {
//! #     let mut part = Part::new("", root);
//! #     part.declaration = None;
//! #     part.prolog.clear();
//! #     String::from_utf8(part.to_xml(WriteOptions::new(Layout::Compact, false))).expect("UTF-8")
//! # }
//! let document = Document::from_transitional(&element(&format!(
//!     "<w:document xmlns:w=\"{W_STRICT}\" w:conformance=\"strict\"><w:body/></w:document>"
//! )))
//! .expect("a document");
//! assert_eq!(
//!     text(document.to_transitional().expect("Transitional has a form for it")),
//!     format!("<w:document xmlns:w=\"{W}\"><w:body/></w:document>")
//! );
//!
//! let mut sheet = Worksheet::from_transitional(&element(&format!(
//!     "<x:worksheet xmlns:x=\"{X}\"><x:sheetData><x:row r=\"1\">\
//!      <x:c r=\"A1\" t=\"d\"><x:v>2026-10-14T00:00:00</x:v></x:c></x:row></x:sheetData></x:worksheet>"
//! )))
//! .expect("a worksheet");
//! assert!(matches!(
//!     sheet.to_transitional(),
//!     Err(WriteError::NotInClass { tag: "c/@t", .. })
//! ));
//!
//! let part = PartName::new("/xl/worksheets/sheet1.xml").expect("a part name");
//! let warnings = convert_date_cells(&mut sheet, false, &part);
//! assert_eq!(warnings.len(), 1);
//! assert_eq!(
//!     warnings[0].to_string(),
//!     "/xl/worksheets/sheet1.xml A1: the date 2026-10-14T00:00:00 was written as the number 46309"
//! );
//! let written = text(sheet.to_transitional().expect("no date cell is left"));
//! assert!(written.contains("<x:c r=\"A1\"><x:v>46309</x:v></x:c>"), "{written}");
//! ```

// The namespace modules, `officekit::transitional::<namespace>`, generated in a
// file of their own.
include!("transitional/modules.rs");

pub mod dates;

use core::cmp::Ordering;

use officekit_markup::Tag;
use officekit_markup::class_rules::{is, order};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::Element;
use officekit_xml::error::{ReadError, WriteError};
use officekit_xml::name::NamespaceId;

#[cfg(test)]
mod tests;

// The table of the check, generated: `STRICT_ONLY`.
include!("transitional/rules.rs");

/// A node of the typed tree read from, and written to, the Transitional class.
///
/// It is implemented for every node of every namespace of `officekit_markup`, and is in
/// scope as `Trait as _` to call its two methods.
pub trait Transitional: Sized {
    /// The node `element` holds, read as either class reads it.
    ///
    /// # Errors
    ///
    /// [`ReadError::UnexpectedTag`] when `element` is not the node's tag.
    fn from_transitional(element: &Element) -> Result<Self, ReadError>;

    /// The node as a Transitional element: its stored text, with the
    /// Transitional URI of every known namespace it declares.
    ///
    /// # Errors
    ///
    /// [`WriteError::NotInClass`] naming the first value only Strict has: a
    /// date cell, which only a save's conversion turns into a number cell.
    fn to_transitional(&self) -> Result<Element, WriteError>;
}

/// Fail on a value only Strict has, in `element` or below it.
fn check(element: &Element) -> Result<(), WriteError> {
    if let Some(owner) = officekit_markup::name(element) {
        let start =
            STRICT_ONLY.partition_point(|(key, _, _, _)| order(*key, owner) == Ordering::Less);
        let end =
            STRICT_ONLY.partition_point(|(key, _, _, _)| order(*key, owner) != Ordering::Greater);
        for (_, attribute, values, label) in STRICT_ONLY.get(start..end).unwrap_or_default() {
            let refused = element
                .attributes
                .iter()
                .any(|found| is(*attribute, &found.name) && values.contains(&found.value.as_str()));
            if refused {
                return Err(WriteError::NotInClass {
                    class: ConformanceClass::Transitional,
                    tag: label,
                });
            }
        }
    }
    for child in element.child_elements() {
        check(child)?;
    }
    Ok(())
}

/// The shared writer's element, checked for values only Strict has, with the
/// Transitional URI of every known namespace it declares.
fn write(mut element: Element) -> Result<Element, WriteError> {
    check(&element)?;
    element.set_class(ConformanceClass::Transitional);
    Ok(element)
}

/// A main part's root written in Transitional, without the attribute
/// `conformance` when it says `strict`: an absent one means transitional.
fn write_root(element: Element, conformance: Tag) -> Result<Element, WriteError> {
    let mut written = write(element)?;
    written
        .attributes
        .retain(|found| !(is(conformance, &found.name) && found.value == "strict"));
    Ok(written)
}
