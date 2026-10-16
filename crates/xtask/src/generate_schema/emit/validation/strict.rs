//! The Strict schema's rules for a value: the checks of each attribute whose
//! Strict type is not its Transitional one, converted from the Strict schema
//! model (D35).
//!
//! A type converts to clauses that must all pass, each a list of checks one
//! of which must pass: a union of members that are several checks each is
//! distributed over them, so that every clause is one check or one union.

use alloc::collections::BTreeMap;

use anyhow::{Context as _, Result, bail};

use super::{Bounds, Check, StringKind, ValueCheck, ValueClass, Versions, class_check};
use crate::generate_schema::compare::{translate, twins};
use crate::generate_schema::model::{Facets, Name, SchemaSet, SimpleType, TypeRef};

#[cfg(test)]
mod tests;

/// By Transitional complex type, each attribute both classes declare whose
/// Strict type is not its Transitional type, with the Strict type's checks.
pub type StrictChecks = BTreeMap<Name, BTreeMap<Name, Vec<ValueCheck>>>;

/// Checks that must all pass, each a list of alternatives one of which must pass.
type Clauses = Vec<Vec<Check>>;

/// The two schema sets, and each Strict namespace's Transitional twin.
struct Sets<'set> {
    strict: &'set SchemaSet,
    transitional: &'set SchemaSet,
    twins: BTreeMap<String, String>,
}

impl Sets<'_> {
    /// Whether the Strict type `strict` is the Transitional type
    /// `transitional`: the same name once translated, unless both are
    /// anonymous, and the same definition.
    fn same(&self, strict: &TypeRef, transitional: &TypeRef) -> bool {
        match (strict, transitional) {
            (TypeRef::BuiltIn(left), TypeRef::BuiltIn(right)) => left == right,
            (
                TypeRef::Simple {
                    anonymous: strict_anonymous,
                    name: strict_name,
                },
                TypeRef::Simple {
                    anonymous: transitional_anonymous,
                    name: transitional_name,
                },
            ) => {
                let named = !*strict_anonymous || !*transitional_anonymous;
                if named && translate(strict_name, &self.twins) != *transitional_name {
                    return false;
                }
                match (
                    self.strict.simple_types.get(strict_name),
                    self.transitional.simple_types.get(transitional_name),
                ) {
                    (Some(left), Some(right)) => self.same_definition(left, right),
                    _ => false,
                }
            }
            (TypeRef::Outside(left), TypeRef::Outside(right)) => {
                translate(left, &self.twins) == *right
            }
            (TypeRef::Unknown, TypeRef::Unknown) => true,
            _ => false,
        }
    }

    /// Whether two definitions, a Strict one and a Transitional one, are the
    /// same once the Strict names are translated.
    fn same_definition(&self, strict: &SimpleType, transitional: &SimpleType) -> bool {
        match (strict, transitional) {
            (
                SimpleType::List {
                    item: strict_item,
                    name: _strict_name,
                },
                SimpleType::List {
                    item: transitional_item,
                    name: _transitional_name,
                },
            ) => self.same(strict_item, transitional_item),
            (
                SimpleType::Restriction {
                    base: strict_base,
                    enumeration: strict_values,
                    facets: strict_facets,
                    name: _strict_name,
                },
                SimpleType::Restriction {
                    base: transitional_base,
                    enumeration: transitional_values,
                    facets: transitional_facets,
                    name: _transitional_name,
                },
            ) => {
                let mut left = strict_values.clone();
                let mut right = transitional_values.clone();
                left.sort();
                right.sort();
                self.same(strict_base, transitional_base)
                    && left == right
                    && strict_facets == transitional_facets
            }
            (
                SimpleType::Union {
                    members: strict_members,
                    name: _strict_name,
                },
                SimpleType::Union {
                    members: transitional_members,
                    name: _transitional_name,
                },
            ) => {
                strict_members.len() == transitional_members.len()
                    && strict_members
                        .iter()
                        .zip(transitional_members)
                        .all(|(left, right)| self.same(left, right))
            }
            _ => false,
        }
    }
}

/// Whether a check passes every value: a string check of characters with no facet.
fn always_passes(check: &Check) -> bool {
    *check
        == Check::String {
            class: Some(ValueClass::String),
            kind: StringKind::Plain,
            length: None,
            max_length: None,
            min_length: None,
            pattern: None,
        }
}

/// The check of a built-in type: its class's check, with its range.
fn built_in(local: &str) -> Result<Check> {
    let sign = |bounds: Bounds| Check::Number {
        bounds,
        class: ValueClass::Integer,
        list: false,
    };
    Ok(match local {
        "boolean" => Check::Enum {
            class: ValueClass::Boolean,
        },
        "ID" => string(StringKind::Id),
        "NCName" => string(StringKind::NcName),
        "QName" => string(StringKind::QName),
        "anyURI" => string(StringKind::Uri),
        "string" => string(StringKind::Plain),
        "token" => string(StringKind::Token),
        "negativeInteger" => sign(Bounds {
            max_exclusive: Some(0),
            ..Bounds::default()
        }),
        "nonNegativeInteger" => sign(Bounds {
            non_negative: true,
            ..Bounds::default()
        }),
        "nonPositiveInteger" => sign(Bounds {
            max_inclusive: Some(0),
            ..Bounds::default()
        }),
        "positiveInteger" => sign(Bounds {
            positive: true,
            ..Bounds::default()
        }),
        other => class_check(&built_in_class(other)?),
    })
}

/// The class a built-in type reads as.
fn built_in_class(local: &str) -> Result<ValueClass> {
    Ok(match local {
        "base64Binary" => ValueClass::Base64Binary,
        "boolean" => ValueClass::Boolean,
        "byte" => ValueClass::SByte,
        "dateTime" => ValueClass::DateTime,
        "decimal" => ValueClass::Decimal,
        "double" => ValueClass::Double,
        "float" => ValueClass::Single,
        "hexBinary" => ValueClass::HexBinary,
        "int" => ValueClass::Int32,
        "integer" | "negativeInteger" | "nonNegativeInteger" | "nonPositiveInteger"
        | "positiveInteger" => ValueClass::Integer,
        "long" => ValueClass::Int64,
        "short" => ValueClass::Int16,
        "ID" | "NCName" | "QName" | "anyURI" | "string" | "token" => ValueClass::String,
        "unsignedByte" => ValueClass::Byte,
        "unsignedInt" => ValueClass::UInt32,
        "unsignedLong" => ValueClass::UInt64,
        "unsignedShort" => ValueClass::UInt16,
        other => bail!("the built-in type xsd:{other} has no check"),
    })
}

/// The checks of the Strict type `type_ref` of `strict`: a clause of one
/// check as that check, a clause of several as one union.
///
/// # Errors
///
/// A type built from a shape the conversion does not know, named.
pub fn checks(strict: &SchemaSet, type_ref: &TypeRef) -> Result<Vec<ValueCheck>> {
    let mut found = Vec::new();
    let mut unions: u8 = 0;
    for clause in clauses(strict, type_ref)? {
        let union = if let [_] = clause.as_slice() {
            None
        } else {
            let id = unions;
            unions = unions
                .checked_add(1)
                .with_context(|| format!("{type_ref} has more unions than a union id holds"))?;
            Some(id)
        };
        found.extend(clause.into_iter().map(|check| ValueCheck {
            check,
            union,
            versions: Versions::ALL,
        }));
    }
    Ok(found)
}

/// The clauses of `type_ref`, a clause that always passes left out.
fn clauses(set: &SchemaSet, type_ref: &TypeRef) -> Result<Clauses> {
    let name = match type_ref {
        TypeRef::BuiltIn(local) => return Ok(kept(vec![vec![built_in(local)?]])),
        TypeRef::Simple {
            anonymous: _anonymous,
            name,
        } => name,
        TypeRef::Complex {
            anonymous: _anonymous,
            name: _name,
        } => bail!("the complex type {type_ref} is no simple type"),
        TypeRef::Outside(_) | TypeRef::Unknown => {
            bail!("the type {type_ref} is not a simple type of the Strict set")
        }
    };
    let simple_type = set
        .simple_types
        .get(name)
        .with_context(|| format!("no simple type {name}"))?;
    Ok(kept(match simple_type {
        SimpleType::List { item, name: _name } => {
            let item_clauses = clauses(set, item)?;
            let [only] = item_clauses.as_slice() else {
                bail!("the list {name}'s item type {item} is not one check");
            };
            let [check] = only.as_slice() else {
                bail!("the list {name}'s item type {item} is a union");
            };
            vec![vec![Check::List {
                item: Box::new(check.clone()),
            }]]
        }
        SimpleType::Restriction {
            base,
            enumeration,
            facets,
            name: _name,
        } => {
            let mut found = clauses(set, base)?;
            found.extend(
                restriction(set, base, enumeration, facets)
                    .with_context(|| format!("in the restriction {name}"))?,
            );
            found
        }
        SimpleType::Union {
            members,
            name: _name,
        } => {
            let mut product: Clauses = vec![Vec::new()];
            for member in members {
                let member_clauses = clauses(set, member)?;
                product = product
                    .iter()
                    .flat_map(|clause| {
                        member_clauses.iter().map(move |alternatives| {
                            let mut joined = clause.clone();
                            for check in alternatives {
                                if !joined.contains(check) {
                                    joined.push(check.clone());
                                }
                            }
                            joined
                        })
                    })
                    .collect();
            }
            product
        }
    }))
}

/// The class a restriction of `type_ref` reads its value as: its built-in root's.
fn class(set: &SchemaSet, type_ref: &TypeRef) -> Result<ValueClass> {
    match type_ref {
        TypeRef::BuiltIn(local) => built_in_class(local),
        TypeRef::Simple {
            anonymous: _anonymous,
            name,
        } => {
            if let Some(SimpleType::Restriction {
                base,
                enumeration: _enumeration,
                facets: _facets,
                name: _name,
            }) = set.simple_types.get(name)
            {
                class(set, base)
            } else {
                bail!("{type_ref} has facets and is no restriction of a built-in type")
            }
        }
        TypeRef::Complex {
            anonymous: _anonymous,
            name: _name,
        } => bail!("the complex type {type_ref} is no simple type"),
        TypeRef::Outside(_) | TypeRef::Unknown => {
            bail!("the type {type_ref} is not a simple type of the Strict set")
        }
    }
}

/// The clauses with those that always pass, and repeated ones, left out.
fn kept(clauses: Clauses) -> Clauses {
    let mut found: Clauses = Vec::with_capacity(clauses.len());
    for clause in clauses {
        if !clause.iter().any(always_passes) && !found.contains(&clause) {
            found.push(clause);
        }
    }
    found
}

/// The clauses of a restriction's facets, in the order of the design's
/// table: its patterns (one of which must match), its enumeration, its
/// lengths, its bounds.
fn restriction(
    set: &SchemaSet,
    base: &TypeRef,
    enumeration: &[String],
    facets: &Facets,
) -> Result<Clauses> {
    let mut found = Vec::new();
    if enumeration.is_empty() && *facets == Facets::default() {
        return Ok(found);
    }
    let class = class(set, base)?;
    let facet = |pattern: Option<String>,
                 length: Option<u32>,
                 max_length: Option<u32>,
                 min_length: Option<u32>| Check::String {
        class: Some(class.clone()),
        kind: StringKind::Plain,
        length: length.map(u64::from),
        max_length: max_length.map(u64::from),
        min_length: min_length.map(u64::from),
        pattern,
    };
    if !facets.patterns.is_empty() {
        found.push(
            facets
                .patterns
                .iter()
                .map(|pattern| facet(Some(pattern.clone()), None, None, None))
                .collect(),
        );
    }
    if !enumeration.is_empty() {
        let members: Vec<String> = enumeration.iter().map(|value| escaped(value)).collect();
        found.push(vec![facet(Some(members.join("|")), None, None, None)]);
    }
    if facets.length.is_some() || facets.max_length.is_some() || facets.min_length.is_some() {
        found.push(vec![facet(
            None,
            facets.length,
            facets.max_length,
            facets.min_length,
        )]);
    }
    let bounds = Bounds {
        max_exclusive: whole(facets.max_exclusive.as_deref())?,
        max_inclusive: whole(facets.max_inclusive.as_deref())?,
        min_exclusive: whole(facets.min_exclusive.as_deref())?,
        min_inclusive: whole(facets.min_inclusive.as_deref())?,
        non_negative: false,
        positive: false,
    };
    if bounds != Bounds::default() {
        found.push(vec![Check::Number {
            bounds,
            class,
            list: false,
        }]);
    }
    Ok(found)
}

/// An enumeration value as a pattern that matches it alone.
fn escaped(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        if "\\.+*?()|[]{}^$-".contains(character) {
            out.push('\\');
        }
        out.push(character);
    }
    out
}

const fn string(kind: StringKind) -> Check {
    Check::String {
        class: Some(ValueClass::String),
        kind,
        length: None,
        max_length: None,
        min_length: None,
        pattern: None,
    }
}

/// The Strict checks of every attribute both classes declare whose Strict
/// type is not its Transitional type.
///
/// # Errors
///
/// A Strict type built from a shape the conversion does not know, named.
pub fn strict_checks(transitional: &SchemaSet, strict: &SchemaSet) -> Result<StrictChecks> {
    let sets = Sets {
        strict,
        transitional,
        twins: twins(transitional, strict),
    };
    let strict_namespaces: BTreeMap<&str, &str> = sets
        .twins
        .iter()
        .map(|(strict_namespace, namespace)| (namespace.as_str(), strict_namespace.as_str()))
        .collect();
    let mut found = StrictChecks::new();
    for (name, complex_type) in &transitional.complex_types {
        let Some(twin) = strict_namespaces
            .get(name.namespace.as_str())
            .and_then(|namespace| strict.complex_types.get(&Name::new(namespace, &name.local)))
        else {
            continue;
        };
        let strict_types: BTreeMap<Name, TypeRef> = strict
            .effective(twin)
            .attributes
            .into_iter()
            .map(|attribute| (translate(&attribute.name, &sets.twins), attribute.type_ref))
            .collect();
        for attribute in transitional.effective(complex_type).attributes {
            let Some(strict_type) = strict_types.get(&attribute.name) else {
                continue;
            };
            if sets.same(strict_type, &attribute.type_ref) {
                continue;
            }
            let converted = checks(strict, strict_type).with_context(|| {
                format!(
                    "in the Strict type {strict_type} of the attribute {} of {name}",
                    attribute.name
                )
            })?;
            found
                .entry(name.clone())
                .or_default()
                .insert(attribute.name, converted);
        }
    }
    Ok(found)
}

/// A bound facet's value, a whole number.
fn whole(value: Option<&str>) -> Result<Option<i64>> {
    value
        .map(|text| {
            text.parse()
                .with_context(|| format!("the bound {text} is not a whole number"))
        })
        .transpose()
}
