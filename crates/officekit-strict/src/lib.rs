//! `officekit-strict` is the Strict conformance class (ECMA-376 Part 1),
//! which `officekit-core`'s feature `strict` turns on.
//!
//! Every node reads Strict with the reader both classes share and writes it
//! with the shared writer; one walk over the written element then applies the
//! generated tables of `rules.rs`. Transitional-only children and
//! attributes take their Strict names, bitmasks unpack into the attributes
//! they pack, values take Strict's spelling, and content Strict has no form
//! for fails the write with its name. The rules are keyed by tag, so the
//! walk also reaches content the typed tree keeps unknown, such as the
//! branches of `mc:AlternateContent`. Word's text box, Microsoft's `wps:wsp`
//! holding `w:txbxContent`, is written as ECMA's `wp:wsp` holding
//! `wp:txbxContent`, in place of the `mc:AlternateContent` around it.

// The namespace modules, `officekit_strict::<namespace>`, generated in a file of their own.
include!("modules.rs");

use core::cmp::Ordering;
use core::mem;

use officekit_markup::Tag;
use officekit_markup::class_rules::{BASE_1900, BASE_1904, is, order};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Attribute, Element, NamespaceDeclaration, Node};
use officekit_xml::error::{ReadError, WriteError};
use officekit_xml::mc;
use officekit_xml::name::table::NAMESPACES;
use officekit_xml::name::{Namespace, NamespaceId, QName, tag_of};

#[cfg(test)]
mod tests;

/// Milliseconds in a day.
const DAY: i128 = 86_400_000;

/// The most decimal places a percentage takes when its row caps none.
const MAX_PLACES: u32 = 9;

const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const MARKUP_COMPATIBILITY: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const WORDPROCESSING: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const WORDPROCESSING_DRAWING: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";

/// Microsoft's namespace of Word's shape, `wps`, and the `uri` of a `graphicData` holding one.
const WORDPROCESSING_SHAPE: &str =
    "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";

// The tables of the walk, generated: `ATTRIBUTES`, `CHILDREN`, `CONTEXTUAL`,
// `ELEMENTS`, `REFUSED_NAMESPACES`, the code tables, and the spellings.
include!("rules.rs");

/// What a Strict write does with an attribute of an element.
enum AttributeRule {
    /// A code written as the name `codes` gives it, in the attribute `to`;
    /// a code with no name has no Strict form.
    CharacterSet {
        codes: Codes,
        label: &'static str,
        to: &'static str,
    },
    /// The text written as a child element of this local name.
    Child(&'static str),
    /// A serial date-time written in ISO 8601 in the attribute `to`.
    Iso {
        label: &'static str,
        to: &'static str,
    },
    /// A bare number counting `per_percent` of a percent when the attribute
    /// `unit` says `percent`, else `per_point` of a point.
    Measure {
        per_percent: u32,
        per_point: u32,
        percent: &'static str,
        unit: &'static str,
    },
    /// No Strict form.
    Refuse(&'static str),
    /// Strict's name for the attribute.
    Rename(&'static str),
    /// A bitmask written as the attributes it packs.
    Unpack { bits: Bits, label: &'static str },
    /// Values Strict spells otherwise.
    Values(&'static [Spelling]),
}

/// How a packed attribute holds its bits.
enum Bits {
    /// A string of `0` and `1`: the attribute of each character, first first.
    Characters(&'static [&'static str]),
    /// A hexadecimal number: each attribute's bit.
    Hex(&'static [(u16, &'static str)]),
}

/// What a Strict write does with a child of an element.
enum ChildRule {
    /// No Strict form.
    Refuse(&'static str),
    /// Strict's name for the child, and the siblings it goes before.
    Rename {
        before: &'static [&'static str],
        to: &'static str,
    },
}

/// A table of codes with the name Strict writes for each.
#[derive(Clone, Copy)]
enum Codes {
    /// Two hexadecimal digits.
    Byte(&'static [(u8, &'static str)]),
    /// A decimal number.
    Number(&'static [(u32, &'static str)]),
}

/// What Strict writes for a Transitional value.
enum Spelling {
    /// Each Transitional spelling with Strict's.
    Map(&'static [(&'static str, &'static str)]),
    /// A bare whole number counting `per_percent` of a percent, written as a
    /// percentage with at most `decimals` decimal places.
    Percent {
        decimals: Option<u32>,
        per_percent: u32,
    },
}

/// A node of the typed tree read from, and written to, the Strict class.
pub trait Strict: Sized {
    /// The node `element` holds, read as either class reads it.
    ///
    /// # Errors
    ///
    /// [`ReadError::UnexpectedTag`] when `element` is not the node's tag.
    fn from_strict(element: &Element) -> Result<Self, ReadError>;

    /// The node as a Strict element, with the Strict URI of every known
    /// namespace it declares.
    ///
    /// # Errors
    ///
    /// [`WriteError::NotInClass`] naming the first content Strict has no form
    /// for.
    fn to_strict(&self) -> Result<Element, WriteError>;
}

/// Apply one attribute rule to the attribute `attribute` of `element`, when
/// the element has it.
fn apply(element: &mut Element, attribute: Tag, rule: &AttributeRule) -> Result<(), WriteError> {
    let Some(index) = element
        .attributes
        .iter()
        .position(|found| is(attribute, &found.name))
    else {
        return Ok(());
    };
    match rule {
        AttributeRule::CharacterSet { codes, label, to } => {
            if has_attribute(element, index, to) {
                element.attributes.remove(index);
                return Ok(());
            }
            let name = element
                .attributes
                .get(index)
                .and_then(|found| character_set(*codes, &found.value))
                .ok_or_else(|| not_in_class(label))?;
            set(element, index, to, name);
            Ok(())
        }
        AttributeRule::Child(local) => {
            to_child(element, index, local);
            Ok(())
        }
        AttributeRule::Iso { label, to } => {
            if has_attribute(element, index, to) {
                element.attributes.remove(index);
                return Ok(());
            }
            let text = element
                .attributes
                .get(index)
                .and_then(|found| iso_date_time(&found.value, false))
                .ok_or_else(|| not_in_class(label))?;
            set(element, index, to, &text);
            Ok(())
        }
        AttributeRule::Measure {
            per_percent,
            per_point,
            percent,
            unit,
        } => {
            let namespace = element
                .attributes
                .get(index)
                .map(|found| &found.name.namespace);
            let unit_value = element
                .attributes
                .iter()
                .find(|found| found.name.local == *unit && Some(&found.name.namespace) == namespace)
                .map_or("", |found| found.value.as_str());
            let converted = element.attributes.get(index).and_then(|found| {
                if unit_value == *percent {
                    percentage(&found.value, *per_percent, None)
                } else {
                    let (negative, magnitude) = whole_number(&found.value)?;
                    let points = decimal(negative, magnitude, u128::from(*per_point), None)?;
                    Some(format!("{points}pt"))
                }
            });
            if let (Some(text), Some(found)) = (converted, element.attributes.get_mut(index)) {
                found.value = text;
            }
            Ok(())
        }
        AttributeRule::Refuse(label) => Err(not_in_class(label)),
        AttributeRule::Rename(to) => {
            if has_attribute(element, index, to) {
                element.attributes.remove(index);
                return Ok(());
            }
            if let Some(found) = element.attributes.get_mut(index) {
                (*to).clone_into(&mut found.name.local);
            }
            Ok(())
        }
        AttributeRule::Unpack { bits, label } => unpack(element, index, bits, label),
        AttributeRule::Values(spellings) => {
            if let Some(found) = element.attributes.get_mut(index)
                && let Some(text) = spell(&found.value, spellings)
            {
                found.value = text;
            }
            Ok(())
        }
    }
}

/// The name a code table gives the code `text`.
fn character_set(codes: Codes, text: &str) -> Option<&'static str> {
    match codes {
        Codes::Byte(rows) => {
            let code = u8::from_str_radix(text, 16)
                .ok()
                .filter(|_| text.len() == 2)?;
            rows.iter()
                .find(|(found, _)| *found == code)
                .map(|(_, name)| *name)
        }
        Codes::Number(rows) => {
            let code: u32 = text.parse().ok()?;
            rows.iter()
                .find(|(found, _)| *found == code)
                .map(|(_, name)| *name)
        }
    }
}

/// The content of the Choice of the `mc:AlternateContent` `block` that holds Word's shape.
fn choice_content(block: Element) -> Vec<Node> {
    let declarations = block.namespace_declarations;
    block
        .children
        .into_iter()
        .find_map(|node| {
            if let Node::Element(branch) = node
                && word_shape_choice(&branch)
            {
                Some(branch)
            } else {
                None
            }
        })
        .map_or_else(Vec::new, |branch| mc::branch_content(declarations, branch))
}

/// The date `days` after 1970-01-01 in the proleptic Gregorian calendar:
/// year, month, and day.
const fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

/// Rename and refuse the children of `element` its tag's rules name, then
/// convert every child element.
fn convert_children(element: &mut Element) -> Result<(), WriteError> {
    unwrap_word_shapes(element);
    if let Some(owner) = officekit_markup::name(element) {
        let rules = rules_of(CHILDREN, owner);
        let mut index = 0;
        while let Some(node) = element.children.get(index) {
            let rule = if let Node::Element(child) = node {
                rules
                    .iter()
                    .find(|(_, tag, _)| is(*tag, &child.name))
                    .map(|(_, _, rule)| rule)
            } else {
                None
            };
            index = match rule {
                Some(ChildRule::Refuse(label)) => return Err(not_in_class(label)),
                Some(ChildRule::Rename { before, to }) => rename_child(element, index, before, to),
                None => index + 1,
            };
        }
    }
    let parent = tag_of(&element.name);
    for child in &mut element.children {
        if let Node::Element(inner) = child {
            convert(inner, parent)?;
        }
    }
    Ok(())
}

/// Apply every rule to `element` and its subtree; `parent` is the tag of the
/// element holding it.
fn convert(
    element: &mut Element,
    parent: Option<(Option<NamespaceId>, &str)>,
) -> Result<(), WriteError> {
    if named(element, WORDPROCESSING_DRAWING, "anchor")
        || named(element, WORDPROCESSING_DRAWING, "inline")
    {
        let prefix = element.name.prefix.clone();
        rewrite_word_shapes(element, &prefix);
    }
    refuse_element(element)?;
    for attribute in &element.attributes {
        if let Namespace::Known(id) = attribute.name.namespace
            && REFUSED_NAMESPACES.iter().any(|(refused, _)| *refused == id)
        {
            return Err(not_in_class(prefix(id)));
        }
    }
    if let Some(owner) = officekit_markup::name(element) {
        let rules = rules_of(ATTRIBUTES, owner);
        let contextual = parent.map_or(&[][..], |found| contextual_rules(found, owner));
        for (_, attribute, rule) in rules {
            apply(element, *attribute, rule)?;
        }
        for (_, _, attribute, rule) in contextual {
            apply(element, *attribute, rule)?;
        }
    }
    convert_children(element)
}

/// The rules of `CONTEXTUAL` for an element `owner` under `parent`.
fn contextual_rules(
    parent: (Option<NamespaceId>, &str),
    owner: (Option<NamespaceId>, &str),
) -> &'static [(Tag, Tag, Tag, AttributeRule)] {
    let key = |(found_parent, found_owner, _, _): &(Tag, Tag, Tag, AttributeRule)| {
        order(*found_parent, parent).then_with(|| order(*found_owner, owner))
    };
    let start = CONTEXTUAL.partition_point(|row| key(row) == Ordering::Less);
    let end = CONTEXTUAL.partition_point(|row| key(row) != Ordering::Greater);
    CONTEXTUAL.get(start..end).unwrap_or_default()
}

/// `magnitude` divided by `divisor` in decimal, negative when `negative`,
/// with as many decimal places as it needs up to `places`, the last one
/// rounded half away from zero; `None` places as many as an exact value
/// needs, up to nine. `None` when the number is too large to scale.
fn decimal(negative: bool, magnitude: u128, divisor: u128, places: Option<u32>) -> Option<String> {
    let exact = (0..=MAX_PLACES)
        .find(|count| {
            10_u128
                .checked_pow(*count)
                .and_then(|power| magnitude.checked_mul(power))
                .is_some_and(|scaled| scaled % divisor == 0)
        })
        .unwrap_or(MAX_PLACES);
    let kept = places.map_or(exact, |cap| cap.min(exact));
    let power = 10_u128.checked_pow(kept)?;
    let scaled = magnitude.checked_mul(power)?;
    let mut quotient = scaled / divisor;
    if scaled % divisor >= divisor - divisor / 2 {
        quotient += 1;
    }
    let whole = quotient / power;
    let fraction = quotient % power;
    let sign = if negative && quotient != 0 { "-" } else { "" };
    let digits = format!("{fraction:0width$}", width = usize::try_from(kept).ok()?);
    let trimmed = digits.trim_end_matches('0');
    Some(if trimmed.is_empty() {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{trimmed}")
    })
}

/// Whether `element` has an attribute named `local` in the namespace of its
/// attribute at `index`, besides that one.
fn has_attribute(element: &Element, index: usize, local: &str) -> bool {
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

/// Whether `element` or an element inside it holds Word's shape.
fn has_word_shape(element: &Element) -> bool {
    holds_word_shape(element) || element.child_elements().any(has_word_shape)
}

/// Whether `element` is a `graphicData` holding Word's shape.
fn holds_word_shape(element: &Element) -> bool {
    named(element, DRAWING, "graphicData")
        && element.attribute(&Namespace::None, "uri") == Some(WORDPROCESSING_SHAPE)
}

/// The ISO 8601 date-time of a serial date-time's text, to the millisecond.
///
/// The serial counts days from the base date of the 1900 date system, or of
/// the 1904 one when `date1904` (ECMA-376 Part 1, 18.17.4.1). `None` when the
/// text is no number or the date falls outside 0001-01-01 to 9999-12-31.
#[must_use]
pub fn iso_date_time(serial: &str, date1904: bool) -> Option<String> {
    let milliseconds = serial_milliseconds(serial)?;
    let days = i64::try_from(milliseconds.div_euclid(DAY)).ok()?;
    let time = u128::try_from(milliseconds.rem_euclid(DAY)).ok()?;
    let base = if date1904 { BASE_1904 } else { BASE_1900 };
    let (year, month, day) = civil_from_days(base.checked_add(days)?);
    if !(1..=9999).contains(&year) {
        return None;
    }
    let hour = time / 3_600_000;
    let minute = time / 60_000 % 60;
    let second = time / 1000 % 60;
    let millisecond = time % 1000;
    let mut text = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}");
    if millisecond != 0 {
        text = format!("{text}.{millisecond:03}");
    }
    Some(text)
}

/// Whether `element` is `local` in the namespace `uri`.
fn named(element: &Element, uri: &str, local: &str) -> bool {
    element.name.local == local && element.name.namespace == Namespace::from_uri(uri)
}

/// The Strict refusal naming `tag`.
const fn not_in_class(tag: &'static str) -> WriteError {
    WriteError::NotInClass {
        class: ConformanceClass::Strict,
        tag,
    }
}

/// A bare whole number counting `per_percent` of a percent, as a percentage
/// with at most `places` decimal places; `None` for any other text.
fn percentage(text: &str, per_percent: u32, places: Option<u32>) -> Option<String> {
    let (negative, magnitude) = whole_number(text)?;
    let number = decimal(negative, magnitude, u128::from(per_percent), places)?;
    Some(format!("{number}%"))
}

/// The prefix the namespace table gives a namespace.
fn prefix(id: NamespaceId) -> &'static str {
    NAMESPACES.get(usize::from(id)).map_or("", |row| row.prefix)
}

/// Fail on an element of a namespace with no Strict twin, or one no Strict
/// schema declares.
fn refuse_element(element: &Element) -> Result<(), WriteError> {
    let Namespace::Known(id) = element.name.namespace else {
        return Ok(());
    };
    let local = element.name.local.as_str();
    if let Some((_, locals)) = REFUSED_NAMESPACES
        .iter()
        .find(|(refused, _)| *refused == id)
    {
        let label = locals
            .iter()
            .find(|known| **known == local)
            .copied()
            .unwrap_or_else(|| prefix(id));
        return Err(not_in_class(label));
    }
    if let Ok(found) = ELEMENTS.binary_search_by(|tag| order(*tag, (Some(id), local)))
        && let Some(tag) = ELEMENTS.get(found)
    {
        return Err(not_in_class(tag.1));
    }
    Ok(())
}

/// Rename the child at `index` to `to`, or drop it when a sibling already
/// has that name, and move it before the first earlier sibling named in
/// `before`; the index of the next child to look at.
fn rename_child(element: &mut Element, index: usize, before: &[&str], to: &'static str) -> usize {
    let Some(Node::Element(child)) = element.children.get(index) else {
        return index + 1;
    };
    let namespace = child.name.namespace.clone();
    let sibling = |node: &Node, locals: &[&str]| {
        matches!(node, Node::Element(found)
            if found.name.namespace == namespace && locals.contains(&found.name.local.as_str()))
    };
    if element.children.iter().any(|node| sibling(node, &[to])) {
        element.children.remove(index);
        return index;
    }
    if let Some(Node::Element(renamed)) = element.children.get_mut(index) {
        to.clone_into(&mut renamed.name.local);
    }
    let earlier = element
        .children
        .iter()
        .take(index)
        .position(|node| sibling(node, before));
    if let Some(position) = earlier {
        let moved = element.children.remove(index);
        element.children.insert(position, moved);
    }
    index + 1
}

/// Write each of Word's shapes inside `element` as ECMA's `wp:wsp` in `prefix`, the prefix of
/// the `wp:anchor` or `wp:inline` around it, and give the `graphicData` holding it the `wp`
/// namespace's Strict URI.
fn rewrite_word_shapes(element: &mut Element, prefix: &str) {
    if holds_word_shape(element) {
        let uri = Namespace::from_uri(WORDPROCESSING_DRAWING)
            .uri(ConformanceClass::Strict)
            .to_owned();
        if let Some(found) = element
            .attributes
            .iter_mut()
            .find(|found| found.name.local == "uri" && found.name.namespace == Namespace::None)
        {
            found.value = uri;
        }
        for node in &mut element.children {
            if let Node::Element(shape) = node
                && named(shape, WORDPROCESSING_SHAPE, "wsp")
            {
                to_ecma(shape, prefix);
            }
        }
    }
    for node in &mut element.children {
        if let Node::Element(inner) = node {
            rewrite_word_shapes(inner, prefix);
        }
    }
}

/// The rows of a table keyed by `owner`'s tag.
fn rules_of<T>(
    table: &'static [(Tag, Tag, T)],
    owner: (Option<NamespaceId>, &str),
) -> &'static [(Tag, Tag, T)] {
    let start = table.partition_point(|(key, _, _)| order(*key, owner) == Ordering::Less);
    let end = table.partition_point(|(key, _, _)| order(*key, owner) != Ordering::Greater);
    table.get(start..end).unwrap_or_default()
}

/// A serial date-time's text in milliseconds, rounded half away from zero.
fn serial_milliseconds(text: &str) -> Option<i128> {
    let (mantissa_text, exponent) = match text.split_once(['e', 'E']) {
        Some((mantissa, power)) => (mantissa, power.parse::<i32>().ok()?),
        None => (text, 0_i32),
    };
    let (negative, unsigned) = signed(mantissa_text);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    let mut digits = format!("{whole}{fraction}");
    let mut shift = exponent.checked_sub(i32::try_from(fraction.len()).ok()?)?;
    while digits.len() > 30 {
        digits.pop();
        shift = shift.checked_add(1)?;
    }
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mantissa: u128 = if digits.is_empty() {
        0
    } else {
        digits.parse().ok()?
    };
    let scaled = mantissa.checked_mul(86_400_000)?;
    let magnitude = if shift >= 0_i32 {
        scaled.checked_mul(10_u128.checked_pow(shift.unsigned_abs())?)?
    } else {
        let divisor = 10_u128
            .checked_pow(shift.unsigned_abs())
            .unwrap_or(u128::MAX);
        let quotient = scaled / divisor;
        if (scaled % divisor) >= divisor - divisor / 2 {
            quotient + 1
        } else {
            quotient
        }
    };
    let value = i128::try_from(magnitude).ok()?;
    Some(if negative { -value } else { value })
}

/// A number's text: whether it is negative, and the text after its sign.
fn signed(text: &str) -> (bool, &str) {
    text.strip_prefix('-').map_or_else(
        || (false, text.strip_prefix('+').unwrap_or(text)),
        |rest| (true, rest),
    )
}

/// Rename the attribute at `index` to `to` and give it `value`.
fn set(element: &mut Element, index: usize, to: &str, value: &str) {
    if let Some(found) = element.attributes.get_mut(index) {
        to.clone_into(&mut found.name.local);
        value.clone_into(&mut found.value);
    }
}

/// What Strict writes for `text`: the first spelling that takes it.
fn spell(text: &str, spellings: &[Spelling]) -> Option<String> {
    spellings.iter().find_map(|spelling| match spelling {
        Spelling::Map(pairs) => pairs
            .iter()
            .find(|(from, _)| *from == text)
            .map(|(_, to)| (*to).to_owned()),
        Spelling::Percent {
            decimals,
            per_percent,
        } => percentage(text, *per_percent, *decimals),
    })
}

/// Move the attribute at `index` into a first child element named `local`,
/// in the element's namespace, unless the element has one.
fn to_child(element: &mut Element, index: usize, local: &str) {
    let attribute = element.attributes.remove(index);
    let present = element
        .child_elements()
        .any(|child| child.name.local == local && child.name.namespace == element.name.namespace);
    if present {
        return;
    }
    let mut child = Element::new(QName::new(
        element.name.prefix.clone(),
        local.to_owned(),
        element.name.namespace.clone(),
    ));
    child.children.push(Node::Text(attribute.value));
    element.children.insert(0, Node::Element(child));
}

/// Word's `wps:wsp` as ECMA's `wp:wsp` in `prefix`: each element of the `wps` namespace takes
/// `wp`, and the `w:txbxContent` of its text box becomes `wp:txbxContent`.
fn to_ecma(element: &mut Element, prefix: &str) {
    let text_box = element.name.local == "txbx";
    element.name = QName::prefixed(prefix, &element.name.local, WORDPROCESSING_DRAWING);
    for node in &mut element.children {
        if let Node::Element(child) = node {
            if child.name.namespace == Namespace::from_uri(WORDPROCESSING_SHAPE) {
                to_ecma(child, prefix);
            }
            if text_box && named(child, WORDPROCESSING, "txbxContent") {
                child.name = QName::prefixed(prefix, "txbxContent", WORDPROCESSING_DRAWING);
            }
        }
    }
}

/// Write the packed attribute at `index` as the attributes its bits name,
/// each one the element lacks.
fn unpack(
    element: &mut Element,
    index: usize,
    bits: &Bits,
    label: &'static str,
) -> Result<(), WriteError> {
    let source = element.attributes.remove(index);
    let values: Vec<(&str, bool)> = match bits {
        Bits::Characters(names) => {
            let valid = source.value.len() == names.len()
                && source.value.bytes().all(|byte| matches!(byte, b'0' | b'1'));
            if !valid {
                return Err(not_in_class(label));
            }
            names
                .iter()
                .zip(source.value.bytes())
                .map(|(name, byte)| (*name, byte == b'1'))
                .collect()
        }
        Bits::Hex(names) => {
            let mask =
                u16::from_str_radix(&source.value, 16).map_err(|_error| not_in_class(label))?;
            names
                .iter()
                .map(|(bit, name)| (*name, mask & bit != 0))
                .collect()
        }
    };
    for (local, on) in values {
        let present = element.attributes.iter().any(|found| {
            found.name.local == local && found.name.namespace == source.name.namespace
        });
        if !present {
            element.attributes.push(Attribute::new(
                QName::new(
                    source.name.prefix.clone(),
                    local.to_owned(),
                    source.name.namespace.clone(),
                ),
                if on { "1" } else { "0" }.to_owned(),
            ));
        }
    }
    Ok(())
}

/// Put, in place of each `mc:AlternateContent` child whose Choice holds Word's shape, that
/// Choice's content, so the shape is written as ECMA's and the VML fallback goes.
fn unwrap_word_shapes(element: &mut Element) {
    if !element
        .children
        .iter()
        .any(|node| matches!(node, Node::Element(block) if word_shape_block(block)))
    {
        return;
    }
    for node in mem::take(&mut element.children) {
        let Node::Element(block) = node else {
            element.children.push(node);
            continue;
        };
        if word_shape_block(&block) {
            element.children.extend(choice_content(block));
        } else {
            element.children.push(Node::Element(block));
        }
    }
}

/// A bare whole number's sign and magnitude: optional sign, digits only.
fn whole_number(text: &str) -> Option<(bool, u128)> {
    let (negative, digits) = signed(text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((negative, digits.parse().ok()?))
}

/// Whether `element` is an `mc:AlternateContent` with a Choice holding Word's shape.
fn word_shape_block(element: &Element) -> bool {
    named(element, MARKUP_COMPATIBILITY, "AlternateContent")
        && element.child_elements().any(word_shape_choice)
}

/// Whether `element` is an `mc:Choice` holding Word's shape.
fn word_shape_choice(element: &Element) -> bool {
    named(element, MARKUP_COMPATIBILITY, "Choice") && has_word_shape(element)
}

/// The shared writer's element, converted to Strict, with the Strict URI of
/// every known namespace it declares.
fn write(mut element: Element) -> Result<Element, WriteError> {
    convert(&mut element, None)?;
    element.set_class(ConformanceClass::Strict);
    Ok(element)
}

/// A main part's root written in Strict, with `conformance="strict"` in the
/// attribute `conformance`.
fn write_root(element: Element, conformance: Tag) -> Result<Element, WriteError> {
    let mut written = write(element)?;
    if let Some(found) = written
        .attributes
        .iter_mut()
        .find(|found| is(conformance, &found.name))
    {
        "strict".clone_into(&mut found.value);
        return Ok(written);
    }
    let (namespace, attribute_prefix) = conformance.0.map_or_else(
        || (Namespace::None, String::new()),
        |id| (Namespace::Known(id), declared_prefix(&mut written, id)),
    );
    written.attributes.push(Attribute::new(
        QName::new(attribute_prefix, conformance.1.to_owned(), namespace),
        "strict".to_owned(),
    ));
    Ok(written)
}

/// A prefix `element` binds to the namespace `id` for an attribute: its own
/// when not empty, one it declares, else the table's, declared on it.
fn declared_prefix(element: &mut Element, id: NamespaceId) -> String {
    let namespace = Namespace::Known(id);
    if element.name.namespace == namespace && !element.name.prefix.is_empty() {
        return element.name.prefix.clone();
    }
    if let Some(found) = element.namespace_declarations.iter().find(|declaration| {
        !declaration.prefix.is_empty() && Namespace::from_uri(&declaration.uri) == namespace
    }) {
        return found.prefix.clone();
    }
    let table_prefix = prefix(id).to_owned();
    element
        .namespace_declarations
        .push(NamespaceDeclaration::new(
            table_prefix.clone(),
            namespace.uri(ConformanceClass::Strict).to_owned(),
        ));
    table_prefix
}
