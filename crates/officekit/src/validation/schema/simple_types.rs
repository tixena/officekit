//! One value checked against a check of the validation tables.
//!
//! The value is read as the class the check names, then the check's facets
//! run on it: bounds, lengths, lexical kind, pattern, and enumeration members,
//! as SDK 3.5.1's value validators read them.

use alloc::borrow::ToOwned as _;
use alloc::string::String;
use alloc::vec::Vec;
use core::iter;

use officekit_xml::name::OfficeVersion;
use officekit_xml::values::matches_pattern;
use officekit_xml::values::parse::date_time;

use super::types::{Bounds, Check, Enumeration, StringKind, ValueClass};
use crate::validation::generated::ENUMERATIONS;

#[cfg(test)]
mod tests;

/// The largest magnitude `System.Decimal` holds.
const DECIMAL_MAX: f64 = 7.922_816_251_426_434e28;

const NO_BOUNDS: Bounds = Bounds {
    max_exclusive: None,
    max_inclusive: None,
    min_exclusive: None,
    min_inclusive: None,
    non_negative: false,
    positive: false,
};

/// What a string check reads its value as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StringForm {
    /// `xsd:base64Binary`: lengths count the bytes it decodes to.
    Base64,
    /// `xsd:hexBinary`: lengths count bytes, two digits each.
    Hex,
    /// Lengths count characters (UTF-16 units, as .NET does).
    Plain,
}

/// A string check's facets.
#[derive(Clone, Copy, Debug)]
struct StringFacets {
    form: StringForm,
    kind: StringKind,
    length: Option<usize>,
    max_length: Option<usize>,
    min_length: Option<usize>,
    pattern: Option<&'static str>,
}

/// How a text reads as a class.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Reading {
    Invalid,
    Number(f64),
    Value,
}

/// The bounds a number fails, each once, in the order SDK 3.5.1 checks them;
/// `NaN` fails every bound there is.
fn bound_failures(bounds: &Bounds, number: f64) -> usize {
    let nan = number.is_nan();
    [
        bounds.non_negative && (nan || number < 0.0),
        bounds.positive && (nan || number <= 0.0),
        bounds.max_exclusive.is_some_and(|max| nan || number >= max),
        bounds.min_exclusive.is_some_and(|min| nan || number <= min),
        bounds.min_inclusive.is_some_and(|min| nan || number < min),
        bounds.max_inclusive.is_some_and(|max| nan || number > max),
    ]
    .into_iter()
    .filter(|failed| *failed)
    .count()
}

/// `xsd:base64Binary` as `Convert.FromBase64String` reads it: whitespace is
/// skipped, the rest comes in groups of four, `=` pads only the end.
fn base64(text: &str) -> bool {
    let bytes: Vec<u8> = text
        .bytes()
        .filter(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        .collect();
    if !bytes.len().is_multiple_of(4) {
        return false;
    }
    let padding = bytes.iter().rev().take_while(|byte| **byte == b'=').count();
    padding <= 2
        && bytes[..bytes.len() - padding]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
}

/// How many bytes valid base64 data decodes to.
fn base64_bytes(text: &str) -> usize {
    let digits: Vec<u8> = text
        .bytes()
        .filter(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        .collect();
    let padding = digits
        .iter()
        .rev()
        .take_while(|byte| **byte == b'=')
        .count();
    digits.len() / 4 * 3 - padding
}

/// `xsd:date` and `xsd:time` as well as `xsd:dateTime`.
fn date_or_time(text: &str) -> bool {
    if text.contains('T') {
        return date_time(text).is_some();
    }
    if text.as_bytes().get(2) == Some(&b':') {
        return date_time(&format!("2000-01-01T{text}")).is_some();
    }
    let (date, zone) = text.strip_suffix('Z').map_or_else(
        || {
            text.char_indices()
                .skip(1)
                .find(|(at, character)| *at >= 10 && matches!(character, '+' | '-'))
                .map_or((text, ""), |(at, _)| text.split_at(at))
        },
        |date| (date, "Z"),
    );
    date_time(&format!("{date}T00:00:00{zone}")).is_some()
}

/// How a number check describes what it requires, with the bounds folded in.
fn describe(class: &ValueClass, bounds: &Bounds, target: OfficeVersion) -> String {
    match class {
        ValueClass::Byte
        | ValueClass::Int16
        | ValueClass::Int32
        | ValueClass::Int64
        | ValueClass::Integer
        | ValueClass::SByte
        | ValueClass::UInt16
        | ValueClass::UInt32
        | ValueClass::UInt64 => whole_range(class, bounds),
        ValueClass::Decimal => limited("a decimal number", bounds),
        ValueClass::Double | ValueClass::Single => limited("a number", bounds),
        ValueClass::Base64Binary
        | ValueClass::Boolean
        | ValueClass::DateTime
        | ValueClass::Enum(_)
        | ValueClass::HexBinary
        | ValueClass::List(_)
        | ValueClass::OnOff
        | ValueClass::String
        | ValueClass::TrueFalse
        | ValueClass::TrueFalseBlank => kind_name(class, target),
    }
}

/// The enumeration of a simple type, `w:ST_Jc`.
#[must_use]
pub fn enumeration(simple_type: &str) -> Option<&'static Enumeration> {
    let index = ENUMERATIONS
        .binary_search_by(|row| row.simple_type.cmp(simple_type))
        .ok()?;
    ENUMERATIONS.get(index)
}

/// What a check finds wrong with a value: one detail per failure, none when it passes.
///
/// `own` is the class of the attribute or text the value belongs to; a string
/// check with no class reads the value as it. One check can fail more than
/// once, as SDK 3.5.1 raises an error per failed bound and one for a string's
/// form besides its length. A present value passes `Required` and `OfficeVersion`.
#[must_use]
pub fn failures(
    check: &Check,
    own: &ValueClass,
    value: &str,
    target: OfficeVersion,
) -> Vec<String> {
    match *check {
        Check::Enum { class } => {
            if read(&class, value, target) == Reading::Invalid {
                vec![format!("not {}", kind_name(&class, target))]
            } else {
                Vec::new()
            }
        }
        Check::List { item } => {
            let mut items = value
                .split(char::is_whitespace)
                .filter(|text| !text.is_empty());
            let Some(first) = items.next() else {
                return vec!["an empty list".to_owned()];
            };
            let item_class = if let ValueClass::List(inner) = *own {
                inner
            } else {
                own
            };
            iter::once(first)
                .chain(items)
                .find_map(|text| failures(item, item_class, text, target).into_iter().next())
                .into_iter()
                .collect()
        }
        Check::Number {
            bounds,
            class,
            list,
        } => {
            if list {
                let mut items = value
                    .split(char::is_whitespace)
                    .filter(|text| !text.is_empty());
                let Some(first) = items.next() else {
                    return vec!["an empty list".to_owned()];
                };
                iter::once(first)
                    .chain(items)
                    .map(|text| number(&bounds, &class, text, target))
                    .find(|found| !found.is_empty())
                    .unwrap_or_default()
            } else {
                number(&bounds, &class, value, target)
            }
        }
        Check::String {
            class,
            kind,
            length,
            max_length,
            min_length,
            pattern,
        } => string(
            &StringFacets {
                form: string_form(class.as_ref().unwrap_or(own)),
                kind,
                length,
                max_length,
                min_length,
                pattern,
            },
            value,
        ),
        Check::OfficeVersion | Check::Required => Vec::new(),
    }
}

/// Whether the file's text is a value of the class, as SDK 3.5.1's `HasValue`
/// says: a string always is, a list when it has an item, any other class when
/// the text reads as it.
#[must_use]
pub fn has_value(class: &ValueClass, text: &str, target: OfficeVersion) -> bool {
    match class {
        ValueClass::Base64Binary | ValueClass::HexBinary | ValueClass::String => true,
        ValueClass::List(_) => text.split(char::is_whitespace).any(|item| !item.is_empty()),
        ValueClass::Boolean
        | ValueClass::Byte
        | ValueClass::DateTime
        | ValueClass::Decimal
        | ValueClass::Double
        | ValueClass::Enum(_)
        | ValueClass::Int16
        | ValueClass::Int32
        | ValueClass::Int64
        | ValueClass::Integer
        | ValueClass::OnOff
        | ValueClass::SByte
        | ValueClass::Single
        | ValueClass::TrueFalse
        | ValueClass::TrueFalseBlank
        | ValueClass::UInt16
        | ValueClass::UInt32
        | ValueClass::UInt64 => read(class, text, target) != Reading::Invalid,
    }
}

/// `xsd:hexBinary`: an even number of hexadecimal digits.
fn hex_binary(text: &str) -> bool {
    text.len().is_multiple_of(2) && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// A whole number as `XmlConvert` reads it: white space around it, an
/// optional sign, ASCII digits.
fn integer(text: &str) -> Option<i128> {
    let trimmed = text.trim_matches(|character| {
        matches!(character, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ')
    });
    let (negative, digits) = match trimmed.as_bytes().first() {
        Some(b'-') => (true, &trimmed[1..]),
        Some(b'+') => (false, &trimmed[1..]),
        _ => (false, trimmed),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let magnitude: i128 = digits.parse().ok()?;
    Some(if negative { -magnitude } else { magnitude })
}

/// The range of an integer class.
fn integer_range(class: &ValueClass) -> Option<(i128, i128)> {
    Some(match class {
        ValueClass::Byte => (0, i128::from(u8::MAX)),
        ValueClass::Int16 => (i128::from(i16::MIN), i128::from(i16::MAX)),
        ValueClass::Int32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
        ValueClass::Int64 | ValueClass::Integer => (i128::from(i64::MIN), i128::from(i64::MAX)),
        ValueClass::SByte => (i128::from(i8::MIN), i128::from(i8::MAX)),
        ValueClass::UInt16 => (0, i128::from(u16::MAX)),
        ValueClass::UInt32 => (0, i128::from(u32::MAX)),
        ValueClass::UInt64 => (0, i128::from(u64::MAX)),
        ValueClass::Base64Binary
        | ValueClass::Boolean
        | ValueClass::DateTime
        | ValueClass::Decimal
        | ValueClass::Double
        | ValueClass::Enum(_)
        | ValueClass::HexBinary
        | ValueClass::List(_)
        | ValueClass::OnOff
        | ValueClass::Single
        | ValueClass::String
        | ValueClass::TrueFalse
        | ValueClass::TrueFalseBlank => return None,
    })
}

/// What a value of the class looks like, for a detail.
fn kind_name(class: &ValueClass, target: OfficeVersion) -> String {
    match class {
        ValueClass::Base64Binary => "base64 data".to_owned(),
        ValueClass::Boolean => "one of true, false, 1, 0".to_owned(),
        ValueClass::DateTime => "a date and time".to_owned(),
        ValueClass::Enum(simple_type) => enumeration(simple_type).map_or_else(
            || format!("a value of {simple_type}"),
            |found| {
                let members: Vec<&str> = found
                    .members
                    .iter()
                    .filter(|(_, versions, _)| versions.contains(target))
                    .map(|(member, _, _)| *member)
                    .collect();
                format!("one of {}", members.join(", "))
            },
        ),
        ValueClass::HexBinary => "an even number of hexadecimal digits".to_owned(),
        ValueClass::List(item) => format!("a list of {}", kind_name(item, target)),
        ValueClass::OnOff => "one of true, false, on, off, 1, 0".to_owned(),
        ValueClass::String => "a string".to_owned(),
        ValueClass::TrueFalse => "one of t, f, true, false".to_owned(),
        ValueClass::TrueFalseBlank => "one of t, f, true, false, or empty".to_owned(),
        ValueClass::Byte
        | ValueClass::Decimal
        | ValueClass::Double
        | ValueClass::Int16
        | ValueClass::Int32
        | ValueClass::Int64
        | ValueClass::Integer
        | ValueClass::SByte
        | ValueClass::Single
        | ValueClass::UInt16
        | ValueClass::UInt32
        | ValueClass::UInt64 => describe(class, &NO_BOUNDS, target),
    }
}

/// A number class described with its bounds: `a number at least 0`.
fn limited(name: &str, bounds: &Bounds) -> String {
    let mut limits: Vec<String> = Vec::new();
    if bounds.non_negative {
        limits.push("at least 0".to_owned());
    }
    if bounds.positive {
        limits.push("greater than 0".to_owned());
    }
    if let Some(min) = bounds.min_inclusive {
        limits.push(format!("at least {min}"));
    }
    if let Some(min) = bounds.min_exclusive {
        limits.push(format!("greater than {min}"));
    }
    if let Some(max) = bounds.max_inclusive {
        limits.push(format!("at most {max}"));
    }
    if let Some(max) = bounds.max_exclusive {
        limits.push(format!("less than {max}"));
    }
    if limits.is_empty() {
        name.to_owned()
    } else {
        format!("{name} {}", limits.join(" and "))
    }
}

/// An `NCName` as `XmlConvert.VerifyNCName` reads it.
fn ncname(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        && characters.all(|character| {
            character.is_alphanumeric()
                || matches!(character, '_' | '-' | '.' | '\u{b7}')
                || ('\u{300}'..='\u{36f}').contains(&character)
                || ('\u{203f}'..='\u{2040}').contains(&character)
        })
}

/// A number check on one value: one failure when it does not read as the
/// class, else one per bound it fails.
fn number(bounds: &Bounds, class: &ValueClass, value: &str, target: OfficeVersion) -> Vec<String> {
    let failed = match read(class, value, target) {
        Reading::Invalid => 1,
        Reading::Number(found) => bound_failures(bounds, found),
        Reading::Value => 0,
    };
    let detail = format!("not {}", describe(class, bounds, target));
    iter::repeat_n(detail, failed).collect()
}

/// A `QName`: an optional `NCName` prefix and a colon, then an `NCName`.
fn qname(text: &str) -> bool {
    match text.split_once(':') {
        None => ncname(text),
        Some((prefix, local)) => ncname(prefix) && ncname(local),
    }
}

/// How `text` reads as `class` at `target`.
fn read(class: &ValueClass, text: &str, target: OfficeVersion) -> Reading {
    let flag = |valid: bool| {
        if valid {
            Reading::Value
        } else {
            Reading::Invalid
        }
    };
    match class {
        ValueClass::Byte
        | ValueClass::Int16
        | ValueClass::Int32
        | ValueClass::Int64
        | ValueClass::Integer
        | ValueClass::SByte
        | ValueClass::UInt16
        | ValueClass::UInt32
        | ValueClass::UInt64 => match (integer(text), integer_range(class)) {
            (Some(value), Some((lowest, highest))) if (lowest..=highest).contains(&value) => {
                format!("{value}")
                    .parse()
                    .map_or(Reading::Value, Reading::Number)
            }
            _ => Reading::Invalid,
        },
        ValueClass::Base64Binary => flag(base64(text)),
        ValueClass::Boolean => flag(matches!(
            text.trim_matches([' ', '\t', '\n', '\r']),
            "true" | "false" | "1" | "0"
        )),
        ValueClass::DateTime => flag(date_or_time(text.trim_matches([' ', '\t', '\n', '\r']))),
        ValueClass::Decimal => {
            let trimmed = text.trim_matches(|character| {
                matches!(character, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ')
            });
            if decimal_lexical(trimmed) {
                trimmed
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.abs() <= DECIMAL_MAX)
                    .map_or(Reading::Invalid, Reading::Number)
            } else {
                Reading::Invalid
            }
        }
        ValueClass::Double | ValueClass::Single => real(text.trim_matches([' ', '\t', '\n', '\r']))
            .map_or(Reading::Invalid, Reading::Number),
        ValueClass::Enum(simple_type) => flag(enumeration(simple_type).is_none_or(|found| {
            found
                .members
                .iter()
                .any(|(member, versions, _)| *member == text && versions.contains(target))
        })),
        ValueClass::HexBinary => flag(hex_binary(text)),
        ValueClass::List(item) => {
            let mut items = text
                .split(char::is_whitespace)
                .filter(|value| !value.is_empty());
            let first = items.next();
            flag(first.is_some_and(|value| {
                iter::once(value)
                    .chain(items)
                    .all(|each| read(item, each, target) != Reading::Invalid)
            }))
        }
        ValueClass::OnOff => flag(matches!(text, "true" | "false" | "on" | "off" | "1" | "0")),
        ValueClass::String => Reading::Value,
        ValueClass::TrueFalse => flag(matches!(text, "t" | "f" | "true" | "false")),
        ValueClass::TrueFalseBlank => flag(matches!(text, "t" | "f" | "true" | "false" | "")),
    }
}

/// `[sign] digits [. digits] | [sign] . digits`, as `Decimal.Parse` reads it.
fn decimal_lexical(text: &str) -> bool {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (whole, fraction) = body.split_once('.').unwrap_or((body, ""));
    (!whole.is_empty() || !fraction.is_empty())
        && whole
            .bytes()
            .chain(fraction.bytes())
            .all(|byte| byte.is_ascii_digit())
}

/// `xsd:double` as `XmlConvert.ToDouble` reads it: `INF`, `-INF`, `NaN`,
/// `Infinity`, or a decimal number with an optional exponent.
fn real(text: &str) -> Option<f64> {
    match text {
        "INF" => return Some(f64::INFINITY),
        "-INF" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    let (negative, body) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if body.eq_ignore_ascii_case("nan") {
        return Some(f64::NAN);
    }
    if body.eq_ignore_ascii_case("infinity") {
        return Some(if negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    let (mantissa, exponent) = body
        .split_once(['e', 'E'])
        .map_or((body, None), |(mantissa, exponent)| {
            (mantissa, Some(exponent))
        });
    let exponent_valid = exponent.is_none_or(|signed| {
        let digits = signed.strip_prefix(['+', '-']).unwrap_or(signed);
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    });
    if !exponent_valid || !decimal_lexical(mantissa) || mantissa.starts_with(['+', '-']) {
        return None;
    }
    text.parse().ok()
}

/// A string check: its form (hexadecimal digits, lexical kind, pattern), then
/// separately its length, as SDK 3.5.1's string validator checks them.
fn string(facets: &StringFacets, value: &str) -> Vec<String> {
    if facets.form == StringForm::Hex && !hex_binary(value) {
        return vec!["not an even number of hexadecimal digits".to_owned()];
    }
    if facets.form == StringForm::Base64 && !base64(value) {
        return vec!["not base64 data".to_owned()];
    }
    let mut found = Vec::new();
    let kind_failure = match facets.kind {
        StringKind::Id => (!ncname(value)).then_some("not an ID"),
        StringKind::NcName => (!ncname(value)).then_some("not an NCName"),
        StringKind::Plain => None,
        StringKind::QName => (!qname(value)).then_some("not a QName"),
        StringKind::Token => (!token(value)).then_some("not a token"),
        StringKind::Uri => (!uri(value)).then_some("not a URI"),
    };
    if let Some(failure) = kind_failure {
        if value.is_empty() {
            return vec!["empty".to_owned()];
        }
        found.push(failure.to_owned());
    } else if facets.kind == StringKind::NcName {
        if value.encode_utf16().count() > 255 {
            found.push("longer than 255 characters".to_owned());
        }
    } else if let Some(pattern) = facets.pattern
        && matches!(matches_pattern(pattern, value), Ok(false))
    {
        found.push(format!("does not match the pattern {pattern}"));
    } else {
        // The form is right.
    }
    let (size, unit) = match facets.form {
        StringForm::Base64 => (base64_bytes(value), "bytes"),
        StringForm::Hex => (value.len() / 2, "bytes"),
        StringForm::Plain => (value.encode_utf16().count(), "characters"),
    };
    if let Some(length) = facets.length.filter(|length| *length != size) {
        found.push(format!("{size} {unit} long, not {length}"));
    } else if let Some(max) = facets.max_length.filter(|max| size > *max) {
        found.push(format!("{size} {unit} long, more than {max}"));
    } else if let Some(min) = facets.min_length.filter(|min| size < *min) {
        found.push(format!("{size} {unit} long, fewer than {min}"));
    } else {
        // The length is right.
    }
    found
}

/// How a string check of a value of `class` reads it.
const fn string_form(class: &ValueClass) -> StringForm {
    match *class {
        ValueClass::Base64Binary => StringForm::Base64,
        ValueClass::HexBinary => StringForm::Hex,
        ValueClass::Boolean
        | ValueClass::Byte
        | ValueClass::DateTime
        | ValueClass::Decimal
        | ValueClass::Double
        | ValueClass::Enum(_)
        | ValueClass::Int16
        | ValueClass::Int32
        | ValueClass::Int64
        | ValueClass::Integer
        | ValueClass::List(_)
        | ValueClass::OnOff
        | ValueClass::SByte
        | ValueClass::Single
        | ValueClass::String
        | ValueClass::TrueFalse
        | ValueClass::TrueFalseBlank
        | ValueClass::UInt16
        | ValueClass::UInt32
        | ValueClass::UInt64 => StringForm::Plain,
    }
}

/// `xsd:token`: no leading or trailing space, no tab or line break, no two
/// spaces in a row; the empty string is one.
fn token(text: &str) -> bool {
    !(text.starts_with(' ')
        || text.ends_with(' ')
        || text.contains(['\n', '\r', '\t'])
        || text.contains("  "))
}

/// `xsd:anyURI` as SDK 3.5.1 reads it: anything but white space alone or a
/// `##`.
fn uri(text: &str) -> bool {
    if text.is_empty() {
        return true;
    }
    let trimmed = text.trim_matches([' ', '\t', '\n', '\r']);
    !trimmed.is_empty() && !trimmed.contains("##")
}

/// A whole `f64` as an integer; `None` when it is not one.
fn whole(value: f64) -> Option<i128> {
    format!("{value}").parse().ok()
}

/// An integer class described with its bounds folded into its range:
/// `a whole number from 0 to 32767`.
fn whole_range(class: &ValueClass, bounds: &Bounds) -> String {
    let (lowest, highest) = integer_range(class).unwrap_or((i128::MIN, i128::MAX));
    let lower = [
        bounds.min_inclusive.and_then(|min| whole(min.ceil())),
        bounds
            .min_exclusive
            .and_then(|min| whole(min.floor()))
            .map(|min| min + 1),
        bounds.non_negative.then_some(0),
        bounds.positive.then_some(1),
    ]
    .into_iter()
    .flatten()
    .fold(lowest, i128::max);
    let upper = [
        bounds.max_inclusive.and_then(|max| whole(max.floor())),
        bounds
            .max_exclusive
            .and_then(|max| whole(max.ceil()))
            .map(|max| max - 1),
    ]
    .into_iter()
    .flatten()
    .fold(highest, i128::min);
    format!("a whole number from {lower} to {upper}")
}
