//! An element's attributes checked against its type.
//!
//! Each declared attribute's checks run in order; then come the attributes
//! the type does not declare and, when the file declares Strict, what only
//! Transitional allows: a marked attribute or one in a namespace only
//! Transitional has, a marked enumeration member, and a value its own checks
//! accept and its Strict checks refuse.

use alloc::borrow::ToOwned as _;
use alloc::string::String;
use alloc::vec::Vec;

use super::simple_types::{enumeration, failures, has_value};
use super::types::{AttributeConstraint, Check, SchemaType, ValueCheck, ValueClass};
use crate::validation::error::ErrorCode;
use crate::validation::generated::TRANSITIONAL_ONLY_NAMESPACES;
use crate::xml::element::{Attribute, Element};
use crate::xml::mc::is_mc;
use crate::xml::name::table::NAMESPACES;
use crate::xml::name::{Namespace, NamespaceId, OfficeVersion, QName};

#[cfg(test)]
mod tests;

/// The `mc` attributes that steer markup compatibility rather than describe
/// the element (ECMA-376 Part 3).
const COMPATIBILITY_ATTRIBUTES: &[&str] = &[
    "Ignorable",
    "MustUnderstand",
    "PreserveAttributes",
    "PreserveElements",
    "ProcessContent",
];

/// The namespace the `xml` prefix is bound to.
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

/// Why a value fails its checks.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Failure {
    /// A check of the value failed; what it found.
    Invalid(String),
    /// A required attribute is missing.
    Missing,
    /// No member of a union that runs at the target accepts the value.
    NotInUnion,
    /// The attribute is not declared at the target.
    Undeclared,
}

/// One error an attribute check found, with the message's arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Problem {
    pub arguments: Vec<(&'static str, String)>,
    pub code: ErrorCode,
}

/// Check every attribute of `element` against `schema_type` at `target`.
///
/// An attribute the type does not declare is an error unless `ignorable`
/// says its namespace is ignorable or it is `xml:*`. With `strict`, a present
/// attribute or value only Transitional allows is an error too, once each.
pub fn check<F>(
    element: &Element,
    schema_type: &SchemaType,
    target: OfficeVersion,
    strict: bool,
    ignorable: F,
) -> Vec<Problem>
where
    F: Fn(&Namespace) -> bool,
{
    let mut problems = Vec::new();
    for constraint in schema_type.attributes {
        let present = element
            .attributes
            .iter()
            .find(|attribute| declares(constraint, &attribute.name));
        let name = present.map_or_else(
            || declared_name(constraint),
            |attribute| qualified(&attribute.name),
        );
        let value = present.map(|attribute| attribute.value.as_str());
        let namespace = present.map_or(Namespace::None, |attribute| {
            attribute.name.namespace.clone()
        });
        let text = value.unwrap_or_default();
        let failures = run(
            constraint.checks,
            &constraint.class,
            value,
            target,
            ignorable(&namespace),
        );
        let own_checks_pass = failures.is_empty();
        for failure in failures {
            problems.push(match failure {
                Failure::Invalid(detail) => Problem {
                    arguments: vec![
                        ("attribute", name.clone()),
                        ("detail", detail),
                        ("value", text.to_owned()),
                    ],
                    code: ErrorCode::InvalidAttributeValue,
                },
                Failure::Missing => Problem {
                    arguments: vec![("attribute", name.clone())],
                    code: ErrorCode::MissingRequiredAttribute,
                },
                Failure::NotInUnion => Problem {
                    arguments: vec![("attribute", name.clone()), ("value", text.to_owned())],
                    code: ErrorCode::ValueNotInUnion,
                },
                Failure::Undeclared => Problem {
                    arguments: vec![("attribute", name.clone())],
                    code: ErrorCode::UndeclaredAttribute,
                },
            });
        }
        if strict && let Some(found) = value {
            if constraint.transitional_only || transitional_only_namespace(&namespace).is_some() {
                problems.push(Problem {
                    arguments: vec![("attribute", name.clone())],
                    code: ErrorCode::TransitionalOnlyAttribute,
                });
            }
            let strict_refuses = || {
                constraint.strict_checks.is_some_and(|checks| {
                    !run(checks, &constraint.class, Some(found), target, false).is_empty()
                })
            };
            if transitional_only_value(constraint, found, target)
                || (own_checks_pass && strict_refuses())
            {
                problems.push(Problem {
                    arguments: vec![("attribute", name), ("value", found.to_owned())],
                    code: ErrorCode::TransitionalOnlyValue,
                });
            }
        }
    }
    for attribute in &element.attributes {
        if !undeclared(attribute, schema_type) {
            continue;
        }
        let namespace = &attribute.name.namespace;
        let allowed = (*namespace != Namespace::None && ignorable(namespace)) || is_xml(namespace);
        if !allowed {
            problems.push(Problem {
                arguments: vec![("attribute", qualified(&attribute.name))],
                code: ErrorCode::UndeclaredAttribute,
            });
        }
        if strict && transitional_only_namespace(namespace).is_some() {
            problems.push(Problem {
                arguments: vec![("attribute", qualified(&attribute.name))],
                code: ErrorCode::TransitionalOnlyAttribute,
            });
        }
    }
    problems
}

/// The name of a declared attribute the file does not hold, with the
/// namespace table's prefix.
fn declared_name(constraint: &AttributeConstraint) -> String {
    constraint
        .namespace
        .and_then(|namespace| NAMESPACES.get(usize::from(namespace)))
        .map_or_else(
            || constraint.name.to_owned(),
            |row| format!("{}:{}", row.prefix, constraint.name),
        )
}

/// Whether `constraint` is the attribute `name`.
fn declares(constraint: &AttributeConstraint, name: &QName) -> bool {
    constraint.name == name.local
        && match (constraint.namespace, &name.namespace) {
            (None, &Namespace::None) => true,
            (Some(own), &Namespace::Known(found)) => own == found,
            (None, &(Namespace::Known(_) | Namespace::Other(_)))
            | (Some(_), &(Namespace::None | Namespace::Other(_))) => false,
        }
}

/// Whether `namespace` is the `xml` namespace.
fn is_xml(namespace: &Namespace) -> bool {
    if let Namespace::Known(id) = *namespace {
        NAMESPACES
            .get(usize::from(id))
            .is_some_and(|row| row.uri == XML_NAMESPACE)
    } else {
        false
    }
}

/// Whether `text` is a member of the enumeration that only Transitional allows.
fn marked_member(simple_type: &str, text: &str) -> bool {
    enumeration(simple_type).is_some_and(|found| {
        found
            .members
            .iter()
            .any(|(member, _, transitional_only)| *transitional_only && *member == text)
    })
}

/// Whether `text`, read as `class`, is or holds an enumeration member only
/// Transitional allows.
fn marked_value(class: &ValueClass, text: &str) -> bool {
    match *class {
        ValueClass::Enum(simple_type) => marked_member(simple_type, text),
        ValueClass::List(item) => text
            .split(char::is_whitespace)
            .any(|each| marked_value(item, each)),
        ValueClass::Base64Binary
        | ValueClass::Boolean
        | ValueClass::Byte
        | ValueClass::DateTime
        | ValueClass::Decimal
        | ValueClass::Double
        | ValueClass::HexBinary
        | ValueClass::Int16
        | ValueClass::Int32
        | ValueClass::Int64
        | ValueClass::Integer
        | ValueClass::OnOff
        | ValueClass::SByte
        | ValueClass::Single
        | ValueClass::String
        | ValueClass::TrueFalse
        | ValueClass::TrueFalseBlank
        | ValueClass::UInt16
        | ValueClass::UInt32
        | ValueClass::UInt64 => false,
    }
}

/// A name as the file wrote it, `w:val`.
#[must_use]
pub fn qualified(name: &QName) -> String {
    if name.prefix.is_empty() {
        name.local.clone()
    } else {
        format!("{}:{}", name.prefix, name.local)
    }
}

/// Run a value's checks at `target`, in order, stopping at the first that fails.
///
/// A missing value fails only `Required`; a union passes when one member that
/// runs at `target` passes, or none runs there; `OfficeVersion` makes an
/// attribute undeclared before its version unless its namespace is `ignorable`.
#[must_use]
pub fn run(
    checks: &[ValueCheck],
    class: &ValueClass,
    value: Option<&str>,
    target: OfficeVersion,
    ignorable: bool,
) -> Vec<Failure> {
    let mut unions_done: Vec<u8> = Vec::new();
    for value_check in checks {
        if let Some(union) = value_check.union {
            if unions_done.contains(&union) {
                continue;
            }
            unions_done.push(union);
            let Some(text) = value else {
                continue;
            };
            let mut running = checks
                .iter()
                .filter(|member| member.union == Some(union) && member.versions.contains(target))
                .peekable();
            if running.peek().is_some()
                && running.all(|member| !failures(&member.check, class, text, target).is_empty())
            {
                return vec![Failure::NotInUnion];
            }
            continue;
        }
        let runs = value_check.versions.contains(target);
        let found: Vec<Failure> = if value_check.check == Check::OfficeVersion {
            if !runs && value.is_some_and(|text| has_value(class, text, target)) && !ignorable {
                vec![Failure::Undeclared]
            } else {
                Vec::new()
            }
        } else if value_check.check == Check::Required {
            if runs && value.is_none() {
                vec![Failure::Missing]
            } else {
                Vec::new()
            }
        } else if let Some(text) = value
            && runs
        {
            failures(&value_check.check, class, text, target)
                .into_iter()
                .map(Failure::Invalid)
                .collect()
        } else {
            Vec::new()
        };
        if !found.is_empty() {
            return found;
        }
    }
    Vec::new()
}

/// The id of `namespace` when only Transitional has it: the namespace of a
/// Transitional schema file with no Strict file of its name.
#[must_use]
pub fn transitional_only_namespace(namespace: &Namespace) -> Option<NamespaceId> {
    if let Namespace::Known(id) = *namespace {
        TRANSITIONAL_ONLY_NAMESPACES.contains(&id).then_some(id)
    } else {
        None
    }
}

/// Whether `text` is a value of `constraint` that only Transitional allows:
/// a member, marked so, of an enumeration the attribute is checked against.
fn transitional_only_value(
    constraint: &AttributeConstraint,
    text: &str,
    target: OfficeVersion,
) -> bool {
    let mut classes: Vec<&ValueClass> = vec![&constraint.class];
    for value_check in constraint.checks {
        if !value_check.versions.contains(target) {
            continue;
        }
        if let Check::Enum { class } = &value_check.check {
            classes.push(class);
        }
        if let Check::Number {
            bounds: _bounds,
            class,
            list: _list,
        } = &value_check.check
        {
            classes.push(class);
        }
    }
    classes.into_iter().any(|class| marked_value(class, text))
}

/// Whether the file's attribute is one `schema_type` does not declare, the
/// `mc` attributes of markup compatibility aside.
fn undeclared(attribute: &Attribute, schema_type: &SchemaType) -> bool {
    let compatibility = is_mc(&attribute.name.namespace)
        && COMPATIBILITY_ATTRIBUTES.contains(&attribute.name.local.as_str());
    !compatibility
        && !schema_type
            .attributes
            .iter()
            .any(|constraint| declares(constraint, &attribute.name))
}
