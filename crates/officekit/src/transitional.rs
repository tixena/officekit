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
//!   transitional. Word's shape, group or canvas in Strict's form takes the form Word writes in
//!   Transitional: `wp:wsp` is `wps:wsp`, `wne:txbxContent` is `w:txbxContent`, and a rewritten
//!   `w:drawing` outside `mc:AlternateContent` moves into a Choice that requires the namespace.
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
use core::mem;

use officekit_markup::Tag;
use officekit_markup::class_rules::{is, order};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Attribute, Element, NamespaceDeclaration, Node};
use officekit_xml::error::{ReadError, WriteError};
use officekit_xml::name::{Namespace, NamespaceId, QName};

#[cfg(test)]
mod tests;

// The table of the check, generated: `STRICT_ONLY`.
include!("transitional/rules.rs");

const CANVAS: WordNamespace = (
    "wpc",
    "http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas",
);
const CONTENT_PART: WordNamespace = (
    "w14",
    "http://schemas.microsoft.com/office/word/2010/wordml",
);
const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const GROUP: WordNamespace = (
    "wpg",
    "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup",
);
const MARKUP_COMPATIBILITY: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const SHAPE: WordNamespace = (
    "wps",
    "http://schemas.microsoft.com/office/word/2010/wordprocessingShape",
);
const WORDPROCESSING: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const WORDPROCESSING_DRAWING: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const WORD_ML: &str = "http://schemas.microsoft.com/office/word/2006/wordml";

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

/// A prefix and its namespace: the `uri` of the `graphicData` that holds Word's shape, group or
/// canvas, and of the elements inside it.
type WordNamespace = (&'static str, &'static str);

/// `drawing` in an `mc:AlternateContent` whose only branch is a Choice requiring `namespace`.
fn alternate_content(drawing: Node, namespace: WordNamespace) -> Element {
    let mut choice = Element::new(QName::prefixed("mc", "Choice", MARKUP_COMPATIBILITY));
    choice
        .namespace_declarations
        .push(NamespaceDeclaration::new(
            namespace.0.to_owned(),
            namespace.1.to_owned(),
        ));
    choice.attributes.push(Attribute::new(
        QName::unprefixed("Requires", ""),
        namespace.0.to_owned(),
    ));
    choice.children.push(drawing);
    let mut block = Element::new(QName::prefixed(
        "mc",
        "AlternateContent",
        MARKUP_COMPATIBILITY,
    ));
    block.namespace_declarations.push(NamespaceDeclaration::new(
        "mc".to_owned(),
        MARKUP_COMPATIBILITY.to_owned(),
    ));
    block.children.push(Node::Element(choice));
    block
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

fn holds_word_ml_text(element: &Element) -> bool {
    named(element, WORD_ML, "txbxContent") || element.child_elements().any(holds_word_ml_text)
}

fn named(element: &Element, uri: &str, local: &str) -> bool {
    element.name.local == local && element.name.namespace == Namespace::from_uri(uri)
}

/// What a `w:drawing` in Strict's form must have understood: the namespace of its Word shape,
/// group or canvas when that holds a `wp` root or a `wne:txbxContent`.
fn strict_drawing(element: &Element) -> Option<WordNamespace> {
    if !named(element, WORDPROCESSING, "drawing") {
        return None;
    }
    element
        .child_elements()
        .flat_map(Element::child_elements)
        .filter(|found| named(found, DRAWING, "graphic"))
        .flat_map(Element::child_elements)
        .filter(|found| named(found, DRAWING, "graphicData"))
        .find_map(|data| {
            let strict = data.child_elements().any(|payload| {
                payload.name.namespace == Namespace::from_uri(WORDPROCESSING_DRAWING)
                    || holds_word_ml_text(payload)
            });
            word_namespace(data).filter(|_| strict)
        })
}

/// Word's drawings in Strict's form written as Word writes them in Transitional. `in_branch` says
/// `element` sits in an `mc:Choice` or `mc:Fallback`. A drawing in Word's Transitional form stays.
fn word_drawings(element: &mut Element, in_branch: bool) {
    if named(element, WORD_ML, "txbxContent") {
        element.name = QName::prefixed("w", "txbxContent", WORDPROCESSING);
        element
            .namespace_declarations
            .retain(|found| Namespace::from_uri(&found.uri) != Namespace::from_uri(WORD_ML));
        element
            .namespace_declarations
            .push(NamespaceDeclaration::new(
                "w".to_owned(),
                WORDPROCESSING.to_owned(),
            ));
    }
    word_payload(element);
    let inside = in_branch
        || named(element, MARKUP_COMPATIBILITY, "Choice")
        || named(element, MARKUP_COMPATIBILITY, "Fallback");
    for node in &mut element.children {
        let Node::Element(child) = node else {
            continue;
        };
        let required = if inside { None } else { strict_drawing(child) };
        word_drawings(child, inside);
        if let Some(namespace) = required {
            let drawing = mem::replace(node, Node::Text(String::new()));
            *node = Node::Element(alternate_content(drawing, namespace));
        }
    }
}

/// `element`, when it is in `wp`, in the namespace Word writes it in, and the same below it. The
/// walk enters only what it renames, so a drawing inside a text box keeps its own `wp` names.
fn word_names(element: &mut Element, inherited: WordNamespace, used: &mut Vec<WordNamespace>) {
    if element.name.namespace != Namespace::from_uri(WORDPROCESSING_DRAWING) {
        return;
    }
    let local = element.name.local.clone();
    let namespace = match local.as_str() {
        "contentPart" => CONTENT_PART,
        "grpSp" | "wgp" => GROUP,
        "wpc" => CANVAS,
        "wsp" => SHAPE,
        _ => inherited,
    };
    let inner = if local == "graphicFrame" {
        GROUP
    } else {
        namespace
    };
    element.name = QName::prefixed(namespace.0, &local, namespace.1);
    if !used.contains(&namespace) {
        used.push(namespace);
    }
    for node in &mut element.children {
        if let Node::Element(child) = node {
            word_names(child, inner, used);
        }
    }
}

/// The namespace of the Word shape, group or canvas a `graphicData` names in its `uri`.
fn word_namespace(graphic_data: &Element) -> Option<WordNamespace> {
    let uri = graphic_data.attribute(&Namespace::None, "uri")?;
    [SHAPE, GROUP, CANVAS]
        .into_iter()
        .find(|found| found.1 == uri)
}

/// The `wp` elements a `graphicData` of Word's holds under Word's names, with each prefix they
/// take declared on it.
fn word_payload(element: &mut Element) {
    if !named(element, DRAWING, "graphicData") {
        return;
    }
    let Some(namespace) = word_namespace(element) else {
        return;
    };
    let mut used = Vec::new();
    for node in &mut element.children {
        if let Node::Element(child) = node {
            word_names(child, namespace, &mut used);
        }
    }
    for (prefix, uri) in used {
        element
            .namespace_declarations
            .push(NamespaceDeclaration::new(prefix.to_owned(), uri.to_owned()));
    }
}

/// The shared writer's element, checked for values only Strict has, with the
/// Transitional URI of every known namespace it declares.
fn write(mut element: Element) -> Result<Element, WriteError> {
    check(&element)?;
    element.set_class(ConformanceClass::Transitional);
    word_drawings(&mut element, false);
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
