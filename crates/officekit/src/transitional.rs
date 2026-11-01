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
//!   transitional. A file's save from Strict to Transitional also writes each value in
//!   Transitional's own spelling, the reverse of each rule of a Strict save: `start` is `left`,
//!   `234pt` is `4680`, `50%` of an `a:alpha` is `50000`. Word's shape, group or canvas in Strict's form takes the form Word writes in
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
//! cell whose text is not a date, or whose date comes before the 1900 date system's range, stays,
//! and a Transitional write refuses it.
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
mod spellings;

use core::cmp::Ordering;
use core::mem;

use officekit_markup::Tag;
use officekit_markup::class_rules::{is, order};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Attribute, Element, NamespaceDeclaration, Node};
use officekit_xml::error::{ReadError, WriteError};
use officekit_xml::name::{Namespace, NamespaceId, QName, tag_of};
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::write::WriteOptions;

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

/// What a save from Strict to Transitional does with an attribute.
enum BackRule {
    /// A name of `codes` written as its code in the attribute `to`, in place of this one.
    Code {
        codes: &'static [(&'static str, &'static str)],
        to: &'static str,
    },
    /// A measure whose unit the attribute `unit` names: a percentage written in `per_percent`
    /// of a percent when the unit is `percent`, else a universal measure in `per_point` of a
    /// point.
    Measure {
        per_percent: u32,
        per_point: u32,
        percent: &'static str,
        unit: &'static str,
    },
    /// The attributes the bits name, packed into this attribute when it is absent.
    Pack(Bits),
    /// A percentage written as a bare number counting this many of a percent.
    Percent(u32),
    /// Transitional's name for the attribute.
    Rename(&'static str),
    /// An ISO 8601 date-time written as a serial date-time in the attribute named, in place of
    /// this one.
    Serial(&'static str),
    /// A universal measure written in twentieths of a point.
    Twips,
    /// Strict's values, each with Transitional's.
    Values(&'static [(&'static str, &'static str)]),
}

/// How a packed attribute's bits are laid out.
enum Bits {
    /// A string of `0` and `1`: the attribute of each character, first first.
    Characters(&'static [&'static str]),
    /// A hexadecimal number: each attribute with its bit.
    Hex(&'static [(u16, &'static str)]),
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

/// Apply `rule` to the attribute `attribute` of `element`; whether it changed. A value the rule
/// cannot read stays as it is, and so does an attribute whose target is already there.
fn apply_back(element: &mut Element, attribute: Tag, rule: &BackRule) -> bool {
    if let BackRule::Pack(bits) = rule {
        return pack(element, attribute, bits);
    }
    let Some(index) = element
        .attributes
        .iter()
        .position(|found| is(attribute, &found.name))
    else {
        return false;
    };
    let Some(value) = element
        .attributes
        .get(index)
        .map(|found| found.value.clone())
    else {
        return false;
    };
    let rewritten = match rule {
        BackRule::Code { codes, to } => {
            let code = codes
                .iter()
                .find(|(name, _)| *name == value)
                .map(|(_, code)| *code);
            return match code {
                Some(found) if !has_sibling(element, index, to) => {
                    move_attribute(element, index, to, found);
                    true
                }
                _ => false,
            };
        }
        BackRule::Measure {
            per_percent,
            per_point,
            percent,
            unit,
        } => {
            let namespace = element
                .attributes
                .get(index)
                .map(|found| found.name.namespace.clone());
            let unit_value = element
                .attributes
                .iter()
                .find(|found| {
                    found.name.local == *unit && Some(&found.name.namespace) == namespace.as_ref()
                })
                .map_or("", |found| found.value.as_str());
            if unit_value == *percent {
                bare_percent(&value, *per_percent)
            } else {
                bare_measure(&value, *per_point)
            }
        }
        BackRule::Pack(_) => None,
        BackRule::Percent(per_percent) => bare_percent(&value, *per_percent),
        BackRule::Rename(to) => {
            if has_sibling(element, index, to) {
                return false;
            }
            return move_attribute(element, index, to, &value);
        }
        BackRule::Serial(to) => {
            return match dates::serial_text(&value) {
                Some(serial) if !has_sibling(element, index, to) => {
                    move_attribute(element, index, to, &serial);
                    true
                }
                _ => false,
            };
        }
        BackRule::Twips => bare_measure(&value, 20),
        BackRule::Values(pairs) => pairs
            .iter()
            .find(|(strict, _)| *strict == value)
            .map(|(_, transitional)| (*transitional).to_owned()),
    };
    match (rewritten, element.attributes.get_mut(index)) {
        (Some(text), Some(found)) if text != found.value => {
            found.value = text;
            true
        }
        _ => false,
    }
}

/// The rows of a table keyed by `owner`'s tag.
fn back_rows<T>(
    table: &'static [(Tag, Tag, T)],
    owner: (Option<NamespaceId>, &str),
) -> &'static [(Tag, Tag, T)] {
    let start = table.partition_point(|(key, _, _)| order(*key, owner) == Ordering::Less);
    let end = table.partition_point(|(key, _, _)| order(*key, owner) != Ordering::Greater);
    table.get(start..end).unwrap_or_default()
}

/// A universal measure, such as `72pt`, `1in` or `2.54cm`, as a whole number counting `per_point`
/// of a point, rounded half away from zero; `None` for any other text.
fn bare_measure(text: &str, per_point: u32) -> Option<String> {
    let split = text.len().checked_sub(2)?;
    let (number, unit) = (text.get(..split)?, text.get(split..)?);
    let (points, of) = match unit {
        "pt" => (1, 1),
        "pc" | "pi" => (12, 1),
        "in" => (72, 1),
        "cm" => (7200, 254),
        "mm" => (720, 254),
        _ => return None,
    };
    let (negative, digits, places) = decimal_number(number)?;
    let numerator = digits
        .checked_mul(points)?
        .checked_mul(u128::from(per_point))?;
    let denominator = 10_u128.checked_pow(places)?.checked_mul(of)?;
    Some(rounded(negative, numerator, denominator))
}

/// A percentage, such as `89.5%`, as a whole number counting `per_percent` of a percent, rounded
/// half away from zero; `None` for any other text.
fn bare_percent(text: &str, per_percent: u32) -> Option<String> {
    let (negative, digits, places) = decimal_number(text.strip_suffix('%')?)?;
    let numerator = digits.checked_mul(u128::from(per_percent))?;
    Some(rounded(negative, numerator, 10_u128.checked_pow(places)?))
}

/// A child of `element` whose text `rows` move back into an attribute, when there is exactly
/// one and the attribute is absent; whether it changed.
fn child_attributes(element: &mut Element, rows: &[(Tag, Tag, Tag)]) -> bool {
    let mut changed = false;
    for (_, child, attribute) in rows {
        let matching: Vec<usize> = element
            .children
            .iter()
            .enumerate()
            .filter(|(_, node)| matches!(node, Node::Element(found) if is(*child, &found.name)))
            .map(|(position, _)| position)
            .collect();
        let present = element
            .attributes
            .iter()
            .any(|found| is(*attribute, &found.name));
        let [position] = matching.as_slice() else {
            continue;
        };
        if present {
            continue;
        }
        if let Node::Element(found) = element.children.remove(*position) {
            let text: String = found
                .children
                .iter()
                .filter_map(|node| {
                    if let Node::Text(text) = node {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            let namespace = attribute.0.map_or(Namespace::None, Namespace::Known);
            let prefix = if namespace == Namespace::None {
                String::new()
            } else {
                found.name.prefix
            };
            element.attributes.push(Attribute::new(
                QName::new(prefix, attribute.1.to_owned(), namespace),
                text,
            ));
            changed = true;
        }
    }
    changed
}

/// The rows of `CONTEXTUAL` for an element `owner` under `parent`.
fn contextual_back(
    parent: (Option<NamespaceId>, &str),
    owner: (Option<NamespaceId>, &str),
) -> &'static [(Tag, Tag, Tag, BackRule)] {
    let table = spellings::CONTEXTUAL;
    let key = |(found_parent, found_owner, _, _): &(Tag, Tag, Tag, BackRule)| {
        order(*found_parent, parent).then_with(|| order(*found_owner, owner))
    };
    let start = table.partition_point(|row| key(row) == Ordering::Less);
    let end = table.partition_point(|row| key(row) != Ordering::Greater);
    table.get(start..end).unwrap_or_default()
}

/// A decimal number's text: whether it is negative, its digits as one whole number, and how many
/// of them follow the point.
fn decimal_number(text: &str) -> Option<(bool, u128, u32)> {
    let (negative, unsigned) = text.strip_prefix('-').map_or_else(
        || (false, text.strip_prefix('+').unwrap_or(text)),
        |rest| (true, rest),
    );
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let digits = format!("{whole}{fraction}");
    if whole.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((
        negative,
        digits.parse().ok()?,
        u32::try_from(fraction.len()).ok()?,
    ))
}

/// Whether `element` has an attribute named `local` in the namespace of its attribute at
/// `index`, besides that one.
fn has_sibling(element: &Element, index: usize, local: &str) -> bool {
    let Some(source) = element.attributes.get(index) else {
        return false;
    };
    element
        .attributes
        .iter()
        .enumerate()
        .any(|(position, found)| {
            position != index
                && found.name.local == local
                && found.name.namespace == source.name.namespace
        })
}

/// Rename the attribute at `index` to `to` and give it `value`; whether it changed.
fn move_attribute(element: &mut Element, index: usize, to: &str, value: &str) -> bool {
    let Some(found) = element.attributes.get_mut(index) else {
        return false;
    };
    to.clone_into(&mut found.name.local);
    value.clone_into(&mut found.value);
    true
}

/// Pack the attributes `bits` names into the attribute `attribute`, unless `element` has it or
/// none of them; whether it changed.
fn pack(element: &mut Element, attribute: Tag, bits: &Bits) -> bool {
    if element
        .attributes
        .iter()
        .any(|found| is(attribute, &found.name))
    {
        return false;
    }
    let namespace = attribute.0.map_or(Namespace::None, Namespace::Known);
    let flag = |local: &str| {
        element
            .attributes
            .iter()
            .find(|found| found.name.local == local && found.name.namespace == namespace)
    };
    let flags: Vec<&str> = match bits {
        Bits::Characters(names) => names.to_vec(),
        Bits::Hex(names) => names.iter().map(|(_, name)| *name).collect(),
    };
    let Some(prefix) = flags
        .iter()
        .find_map(|name| flag(name))
        .map(|found| found.name.prefix.clone())
    else {
        return false;
    };
    let on = |local: &str| {
        flag(local).is_some_and(|found| matches!(found.value.as_str(), "1" | "true" | "on"))
    };
    let value = match bits {
        Bits::Characters(names) => names
            .iter()
            .map(|name| if on(name) { '1' } else { '0' })
            .collect(),
        Bits::Hex(names) => {
            let mask = names
                .iter()
                .filter(|(_, name)| on(name))
                .fold(0_u16, |mask, (bit, _)| mask | bit);
            format!("{mask:04X}")
        }
    };
    element.attributes.push(Attribute::new(
        QName::new(prefix, attribute.1.to_owned(), namespace),
        value,
    ));
    true
}

/// `numerator / denominator` rounded half away from zero, negative when `negative`.
fn rounded(negative: bool, numerator: u128, denominator: u128) -> String {
    let quotient = numerator / denominator;
    let whole = if (numerator % denominator) * 2 >= denominator {
        quotient + 1
    } else {
        quotient
    };
    if negative && whole != 0 {
        format!("-{whole}")
    } else {
        whole.to_string()
    }
}

/// Rename each child of `element` that `rows` name to Transitional's name, unless a sibling has
/// it; whether it changed.
fn rename_children(element: &mut Element, rows: &[(Tag, Tag, &str)]) -> bool {
    let mut changed = false;
    for index in 0..element.children.len() {
        let target = match element.children.get(index) {
            Some(Node::Element(child)) => rows
                .iter()
                .find(|(_, tag, _)| is(*tag, &child.name))
                .map(|(_, _, to)| (*to, child.name.namespace.clone())),
            _ => None,
        };
        let Some((to, namespace)) = target else {
            continue;
        };
        let taken = element.children.iter().any(|node| {
            matches!(node, Node::Element(found) if found.name.local == to && found.name.namespace == namespace)
        });
        if let (false, Some(Node::Element(child))) = (taken, element.children.get_mut(index)) {
            to.clone_into(&mut child.name.local);
            changed = true;
        }
    }
    changed
}

/// Write every value of `element` and below it in Transitional's spelling, as a save from Strict
/// writes it; whether anything changed. `parent` is the tag of the element holding it.
fn spell(element: &mut Element, parent: Option<(Option<NamespaceId>, &str)>) -> bool {
    let mut changed = false;
    let tables = officekit_markup::name(element).map(|owner| {
        let text_rule = spellings::TEXTS
            .binary_search_by(|(tag, _)| order(*tag, owner))
            .ok()
            .and_then(|index| spellings::TEXTS.get(index))
            .map(|(_, per_percent)| *per_percent);
        (
            back_rows(spellings::ATTRIBUTES, owner),
            parent.map_or(&[][..], |tag| contextual_back(tag, owner)),
            text_rule,
            back_rows(spellings::CHILDREN, owner),
            back_rows(spellings::CHILD_ATTRIBUTES, owner),
        )
    });
    if let Some((attributes, contextual, text_rule, children, child_rows)) = tables {
        let rows = attributes
            .iter()
            .map(|(_, attribute, rule)| (*attribute, rule))
            .chain(
                contextual
                    .iter()
                    .map(|(_, _, attribute, rule)| (*attribute, rule)),
            );
        for (attribute, rule) in rows {
            changed |= apply_back(element, attribute, rule);
        }
        if let Some(per_percent) = text_rule {
            for node in &mut element.children {
                if let Node::Text(text) = node
                    && let Some(bare) = bare_percent(text, per_percent)
                {
                    *text = bare;
                    changed = true;
                }
            }
        }
        changed |= rename_children(element, children);
        changed |= child_attributes(element, child_rows);
    }
    let tag = tag_of(&element.name);
    for node in &mut element.children {
        if let Node::Element(child) = node {
            changed |= spell(child, tag);
        }
    }
    changed
}

/// Write the part `name` with every value in Transitional's spelling: what a save from Strict
/// to Transitional does after the part's own store, the reverse of each rule of the Strict walk.
/// A part that is not XML, or that holds nothing to change, stays as it is.
///
/// # Errors
///
/// The errors of reading the part's tree and of storing it.
pub(crate) fn spell_tree(
    package: &mut Package,
    name: &PartName,
    options: WriteOptions,
) -> Result<(), PackageError> {
    let Some(content_type) = package.content_type(name).map(str::to_owned) else {
        return Ok(());
    };
    let xml = content_type.ends_with("+xml")
        || content_type == "application/xml"
        || content_type == "text/xml";
    if !xml {
        return Ok(());
    }
    let mut document = package.tree_at(name)?;
    if spell(&mut document.root, None) {
        package.store_tree(name, &content_type, &document, options)?;
    }
    Ok(())
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
