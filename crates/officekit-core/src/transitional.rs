//! The Transitional conformance class (ECMA-376 Part 1 with the migration
//! features of Part 4), behind the feature `transitional`, on by default.
//!
//! Every node reads Transitional with the reader both classes share and
//! writes the stored text with the shared writer, with the Transitional
//! URI of every known namespace it declares. The one Strict form Transitional
//! has no place for is a date cell (`t="d"`): the write refuses it, and
//! [`dates::convert_date_cells`] turns each one into a number cell before a
//! Transitional save.

// The namespace modules, `transitional::<namespace>`, generated in a file of
// their own.
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
