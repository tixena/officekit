//! The newtypes of simple types: each holds the text the file carried,
//! and its `value` method parses it by the type's kind, section 3.4's table:
//! an enumeration gives its member enum, a union the first member type that
//! matches, a list the newtypes of its items, `ST_OnOff` a `bool`, a date an
//! `officekit_xml::values::DateTime`, a string its text once its facets hold.

use alloc::collections::BTreeMap;
use core::fmt::Write as _;
use core::slice;

use anyhow::{Context as _, Result, bail};

use super::Data;
use super::names::{self, Overrides, Titles};
use super::render::layout::{self, Expr};
use super::render::{BODY_LINES, Names, Uses};
use crate::generate_schema::model::{Facets, Name, SimpleType, TypeRef};
use crate::generate_schema::sdk_data::DataVersion;

/// The built-in types whose value is a whole number, with their range.
const INTEGERS: [(&str, i128, i128); 13] = [
    ("byte", -128, 127),
    ("int", -0x8000_0000, 0x7FFF_FFFF),
    ("integer", i64::MIN as i128, i64::MAX as i128),
    ("long", i64::MIN as i128, i64::MAX as i128),
    ("negativeInteger", i64::MIN as i128, -1),
    ("nonNegativeInteger", 0, i64::MAX as i128),
    ("nonPositiveInteger", i64::MIN as i128, 0),
    ("positiveInteger", 1, i64::MAX as i128),
    ("short", -0x8000, 0x7FFF),
    ("unsignedByte", 0, 255),
    ("unsignedInt", 0, 0xFFFF_FFFF),
    ("unsignedLong", 0, i64::MAX as i128),
    ("unsignedShort", 0, 0xFFFF),
];

/// A simple type the generator gives a newtype.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Key {
    /// An enumeration of SDK 3.5.1's data, `w14:ST_OnOff`.
    Data(String),
    /// A simple type of the Transitional schemas.
    Xsd(Name),
}

/// What a value of a built-in base is, once its facets hold.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Atom {
    Boolean,
    DateTime,
    /// `xsd:double`, `xsd:float`, `xsd:decimal`, with their bounds.
    Double {
        max_exclusive: Option<f64>,
        max_inclusive: Option<f64>,
        min_exclusive: Option<f64>,
        min_inclusive: Option<f64>,
    },
    /// Hexadecimal bytes of a fixed length, as an unsigned number of that many bytes.
    Hex {
        bytes: u32,
    },
    /// A whole number in a range, with the values an enumeration allows.
    Integer {
        max: i128,
        min: i128,
        values: Vec<String>,
    },
    /// A universal measure, `2.5cm`: a string whose pattern ends in a unit.
    Measure {
        patterns: Vec<String>,
    },
    /// A percentage, `50%`: a string whose pattern ends in `%`.
    Percentage {
        patterns: Vec<String>,
    },
    /// Text, with its length and pattern facets.
    Text {
        length: Option<u32>,
        max_length: Option<u32>,
        min_length: Option<u32>,
        patterns: Vec<String>,
    },
}

/// What a newtype's `value` returns.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Kind {
    Atom(Atom),
    Date,
    Enumeration(Vec<Member>),
    /// A list of the item type: its newtype, or `None` for text.
    List(Option<Key>),
    OnOff,
    Union(Vec<UnionMember>),
}

/// A member of an enumeration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Member {
    pub introduced: Option<&'static str>,
    pub value: String,
    pub variant: String,
}

/// A newtype of a simple type.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Simple {
    pub doc: String,
    pub key: Key,
    pub kind: Kind,
    pub module: usize,
    pub name: String,
    pub uri: String,
}

/// A member type of a union, after nested unions are flattened.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct UnionMember {
    /// The named type it is, whose name names the variant.
    pub origin: Option<Name>,
    pub value: MemberValue,
    pub variant: String,
}

/// What a union member is read as.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum MemberValue {
    /// A built-in or anonymous type, checked in place.
    Atom(Atom),
    /// A named type with its own newtype.
    Simple(Key),
    /// A named string type any text is a value of.
    Text,
}

/// One member type a union's `value` tries: the binding it reads into, the
/// test of that binding, and the member enum's variant it gives.
struct Trial {
    binding: String,
    owned: &'static str,
    read: Expr,
    test: String,
    variant: String,
}

impl Atom {
    /// Whether the value is text, which `value` borrows from the newtype.
    const fn is_text(&self) -> bool {
        matches!(
            self,
            Self::Text {
                length: _length,
                max_length: _max_length,
                min_length: _min_length,
                patterns: _patterns,
            }
        )
    }

    /// The Rust type the value reads as, borrowed for text, and the import
    /// that type needs.
    const fn value_type(&self) -> (&'static str, Option<&'static str>) {
        match self {
            Self::Boolean => ("bool", None),
            Self::DateTime => ("DateTime", Some("officekit_xml::values::DateTime")),
            Self::Double {
                max_exclusive: _max_exclusive,
                max_inclusive: _max_inclusive,
                min_exclusive: _min_exclusive,
                min_inclusive: _min_inclusive,
            } => ("f64", None),
            Self::Hex { bytes } => match bytes {
                1 => ("u8", None),
                2 => ("u16", None),
                3 | 4 => ("u32", None),
                _ => ("u64", None),
            },
            Self::Integer {
                max: _max,
                min: _min,
                values: _values,
            } => ("i64", None),
            Self::Measure {
                patterns: _patterns,
            } => ("Measure", Some("officekit_xml::values::Measure")),
            Self::Percentage {
                patterns: _patterns,
            } => ("f64", None),
            Self::Text {
                length: _length,
                max_length: _max_length,
                min_length: _min_length,
                patterns: _patterns,
            } => ("&str", None),
        }
    }
}

impl Simple {
    /// Whether the newtype has a member enum: an enumeration or a union.
    #[must_use]
    pub const fn has_member(&self) -> bool {
        matches!(self.kind, Kind::Enumeration(_) | Kind::Union(_))
    }

    /// The namespace of the simple type.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.uri
    }

    /// The simple types its value reads through.
    #[must_use]
    pub fn references(&self) -> Vec<Key> {
        match &self.kind {
            Kind::List(Some(item)) => vec![item.clone()],
            Kind::Union(members) => members
                .iter()
                .filter_map(|member| match &member.value {
                    MemberValue::Simple(key) => Some(key.clone()),
                    MemberValue::Atom(_) | MemberValue::Text => None,
                })
                .collect(),
            Kind::Atom(_) | Kind::Date | Kind::Enumeration(_) | Kind::List(None) | Kind::OnOff => {
                Vec::new()
            }
        }
    }

    /// The schema name a `ValueError` carries, `ST_Jc`.
    #[must_use]
    pub fn xsd_name(&self) -> &str {
        match &self.key {
            Key::Data(name) => name
                .split_once(':')
                .map_or(name.as_str(), |(_, local)| local),
            Key::Xsd(name) => &name.local,
        }
    }
}

/// The newtype of a simple type: `None` for a string any text is a value of,
/// which fields hold as `String`.
///
/// # Errors
///
/// A simple type of a kind the generator does not know.
pub fn describe(
    data: &Data<'_>,
    key: &Key,
    strict_values: &BTreeMap<Name, Vec<String>>,
) -> Result<Option<Simple>> {
    match key {
        Key::Data(name) => {
            let enumeration = data
                .sdk
                .enumerations
                .get(name)
                .with_context(|| format!("no enumeration {name} in the data"))?;
            let (prefix, _) = name
                .split_once(':')
                .context("an enumeration's type has no prefix")?;
            let uri = data
                .uri(prefix)
                .with_context(|| format!("no namespace for {prefix}"))?
                .to_owned();
            let members = enumeration
                .facets
                .iter()
                .map(|facet| Member {
                    introduced: version(facet.version.or(enumeration.version)),
                    value: facet.value.clone(),
                    variant: names::variant(&facet.value),
                })
                .collect();
            Ok(Some(Simple {
                doc: String::new(),
                key: key.clone(),
                kind: Kind::Enumeration(disambiguate(members)),
                module: 0,
                name: String::new(),
                uri,
            }))
        }
        Key::Xsd(name) => {
            let simple_type = data
                .transitional
                .simple_types
                .get(name)
                .with_context(|| format!("no simple type {name}"))?;
            let Some(kind) = kind(data, simple_type, strict_values)? else {
                return Ok(None);
            };
            Ok(Some(Simple {
                doc: String::new(),
                key: key.clone(),
                kind,
                module: 0,
                name: String::new(),
                uri: name.namespace.clone(),
            }))
        }
    }
}

/// Name a newtype by its title, an override winning; a type with no
/// title, or one of the data, takes its local name without `ST_`.
///
/// # Errors
///
/// A name that is not an identifier.
pub fn name(
    simple: &mut Simple,
    file: Option<&str>,
    prefix: &str,
    titles: &Titles,
    overrides: &Overrides,
) -> Result<()> {
    let local = simple.xsd_name().to_owned();
    let heading = file.and_then(|stem| {
        titles
            .simple_types
            .get(&(stem.to_owned(), local.clone()))
            .and_then(|headings| headings.first())
    });
    let overridden = overrides
        .simple_types
        .iter()
        .find(|row| row.prefix == prefix && row.type_local == local);
    let fallback = local.strip_prefix("ST_").unwrap_or(&local).to_owned();
    simple.name = match (overridden, heading) {
        (Some(row), _) => row.name.clone(),
        (None, Some(found)) => names::pascal(&names::title_words(&found.title)),
        (None, None) => names::pascal(&names::local_words(&fallback)),
    };
    if !simple.name.starts_with(|c: char| c.is_ascii_uppercase()) {
        bail!("the simple type {local} is named {}", simple.name);
    }
    simple.doc = heading.map_or_else(
        || format!("`{local}`: the text the file carried."),
        |found| {
            format!(
                "{} `{local}` ({}): the text the file carried.",
                found.section, found.title
            )
        },
    );
    Ok(())
}

/// The kind of an XSD simple type; `None` for a string any text is a value of.
fn kind(
    data: &Data<'_>,
    simple_type: &SimpleType,
    strict_values: &BTreeMap<Name, Vec<String>>,
) -> Result<Option<Kind>> {
    match simple_type {
        SimpleType::List { item, name: _name } => Ok(Some(Kind::List(simple_key(data, item)?))),
        SimpleType::Union { members, name } => union(data, name, members),
        SimpleType::Restriction {
            base,
            enumeration,
            facets,
            name,
        } => {
            if !enumeration.is_empty() {
                if let Some(Atom::Integer {
                    max,
                    min,
                    values: _values,
                }) = atom(data, base, facets)?
                {
                    return Ok(Some(Kind::Atom(Atom::Integer {
                        max,
                        min,
                        values: enumeration.clone(),
                    })));
                }
                let mut values = enumeration.clone();
                if let Some(extra) = strict_values.get(name) {
                    values.extend(extra.iter().cloned());
                }
                return Ok(Some(Kind::Enumeration(members(data, name, &values))));
            }
            if base_name(base).as_deref() == Some("dateTime") && facets == &Facets::default() {
                return Ok(Some(Kind::Date));
            }
            if let TypeRef::Simple {
                anonymous: false,
                name: base_type,
            } = base
                && let Some(SimpleType::Union {
                    members,
                    name: _base_name,
                }) = data.transitional.simple_types.get(base_type)
                && facets == &Facets::default()
            {
                return union(data, name, members);
            }
            match atom(data, base, facets)? {
                Some(Atom::Text {
                    length: None,
                    max_length: None,
                    min_length: None,
                    patterns,
                }) if patterns.is_empty() => Ok(None),
                Some(found) => Ok(Some(Kind::Atom(found))),
                None => Ok(None),
            }
        }
    }
}

/// A union, flattened; `ST_OnOff`'s `xsd:boolean` and `on`/`off` read as a `bool`.
fn union(data: &Data<'_>, name: &Name, members: &[TypeRef]) -> Result<Option<Kind>> {
    let mut flat: Vec<UnionMember> = Vec::new();
    let mut on_off = (false, false);
    flatten_union(data, members, &mut flat, &mut on_off)?;
    if on_off == (true, true) && flat.len() == 2 {
        return Ok(Some(Kind::OnOff));
    }
    if flat.is_empty() {
        bail!("the union {name} has no member");
    }
    Ok(Some(Kind::Union(flat)))
}

fn flatten_union(
    data: &Data<'_>,
    members: &[TypeRef],
    flat: &mut Vec<UnionMember>,
    on_off: &mut (bool, bool),
) -> Result<()> {
    for member in members {
        match member {
            TypeRef::Simple { anonymous, name } => {
                let simple_type = data
                    .transitional
                    .simple_types
                    .get(name)
                    .with_context(|| format!("no simple type {name}"))?;
                if let SimpleType::Union {
                    members: inner,
                    name: _name,
                } = simple_type
                {
                    flatten_union(data, inner, flat, on_off)?;
                    continue;
                }
                if let SimpleType::Restriction {
                    base: _base,
                    enumeration,
                    facets: _facets,
                    name: _name,
                } = simple_type
                    && !enumeration.is_empty()
                    && enumeration
                        .iter()
                        .all(|value| value == "on" || value == "off")
                {
                    on_off.1 = true;
                }
                if *anonymous {
                    let position = flat.len() + 1;
                    let SimpleType::Restriction {
                        base,
                        enumeration,
                        facets,
                        name: _name,
                    } = simple_type
                    else {
                        bail!("an anonymous list or union in a union is not read");
                    };
                    let value = match atom(data, base, facets)? {
                        Some(Atom::Integer {
                            max,
                            min,
                            values: _values,
                        }) => MemberValue::Atom(Atom::Integer {
                            max,
                            min,
                            values: enumeration.clone(),
                        }),
                        Some(found) => MemberValue::Atom(found),
                        None => MemberValue::Text,
                    };
                    flat.push(UnionMember {
                        origin: None,
                        value,
                        variant: format!("Member{position}"),
                    });
                } else {
                    let key = Key::Xsd(name.clone());
                    let value = if kind(data, simple_type, &BTreeMap::new())?.is_some() {
                        MemberValue::Simple(key)
                    } else {
                        MemberValue::Text
                    };
                    flat.push(UnionMember {
                        origin: Some(name.clone()),
                        value,
                        variant: String::new(),
                    });
                }
            }
            TypeRef::BuiltIn(local) => {
                if local == "boolean" {
                    on_off.0 = true;
                }
                let value = atom(data, member, &Facets::default())?
                    .map_or(MemberValue::Text, MemberValue::Atom);
                flat.push(UnionMember {
                    origin: None,
                    value,
                    variant: names::pascal(slice::from_ref(local)),
                });
            }
            TypeRef::Complex {
                anonymous: _anonymous,
                name: _name,
            } => flat.push(text_member(flat.len())),
            TypeRef::Outside(_) | TypeRef::Unknown => flat.push(text_member(flat.len())),
        }
    }
    Ok(())
}

/// A union member any text is a value of, at `position` among those before it.
fn text_member(position: usize) -> UnionMember {
    UnionMember {
        origin: None,
        value: MemberValue::Text,
        variant: format!("Member{}", position + 1),
    }
}

/// The built-in a restriction's base comes down to, with the facets of every
/// restriction on the way; `None` when it is no built-in.
fn atom(data: &Data<'_>, base: &TypeRef, facets: &Facets) -> Result<Option<Atom>> {
    let mut collected = facets.clone();
    let mut current = base.clone();
    loop {
        match &current {
            TypeRef::BuiltIn(local) => return Ok(Some(builtin_atom(local, &collected)?)),
            TypeRef::Simple {
                anonymous: _anonymous,
                name,
            } => {
                let Some(SimpleType::Restriction {
                    base: next,
                    enumeration: _enumeration,
                    facets: more,
                    name: _name,
                }) = data.transitional.simple_types.get(name)
                else {
                    return Ok(None);
                };
                merge(&mut collected, more);
                current = next.clone();
            }
            TypeRef::Complex {
                anonymous: _anonymous,
                name: _name,
            } => return Ok(Some(builtin_atom("string", &collected)?)),
            TypeRef::Outside(_) | TypeRef::Unknown => {
                return Ok(Some(builtin_atom("string", &collected)?));
            }
        }
    }
}

/// `facets` with the base's facets where it has none of its own.
fn merge(facets: &mut Facets, base: &Facets) {
    facets.length = facets.length.or(base.length);
    facets.max_exclusive = facets
        .max_exclusive
        .take()
        .or_else(|| base.max_exclusive.clone());
    facets.max_inclusive = facets
        .max_inclusive
        .take()
        .or_else(|| base.max_inclusive.clone());
    facets.max_length = facets.max_length.or(base.max_length);
    facets.min_exclusive = facets
        .min_exclusive
        .take()
        .or_else(|| base.min_exclusive.clone());
    facets.min_inclusive = facets
        .min_inclusive
        .take()
        .or_else(|| base.min_inclusive.clone());
    facets.min_length = facets.min_length.or(base.min_length);
    if facets.patterns.is_empty() {
        facets.patterns.clone_from(&base.patterns);
    }
}

fn builtin_atom(local: &str, facets: &Facets) -> Result<Atom> {
    if let Some((_, min, max)) = INTEGERS.iter().find(|(name, _, _)| *name == local) {
        let bound = |text: &Option<String>| -> Result<Option<i128>> {
            text.as_ref()
                .map(|value| {
                    value
                        .parse::<i128>()
                        .with_context(|| format!("the facet {value} is no whole number"))
                })
                .transpose()
        };
        let low = [
            Some(*min),
            bound(&facets.min_inclusive)?,
            bound(&facets.min_exclusive)?.map(|value| value + 1),
        ];
        let high = [
            Some(*max),
            bound(&facets.max_inclusive)?,
            bound(&facets.max_exclusive)?.map(|value| value - 1),
        ];
        return Ok(Atom::Integer {
            max: high.iter().flatten().copied().min().unwrap_or(*max),
            min: low.iter().flatten().copied().max().unwrap_or(*min),
            values: Vec::new(),
        });
    }
    let real = |text: &Option<String>| -> Result<Option<f64>> {
        text.as_ref()
            .map(|value| {
                value
                    .parse::<f64>()
                    .with_context(|| format!("the facet {value} is no number"))
            })
            .transpose()
    };
    Ok(match local {
        "boolean" => Atom::Boolean,
        "dateTime" => Atom::DateTime,
        "decimal" | "double" | "float" => Atom::Double {
            max_exclusive: real(&facets.max_exclusive)?,
            max_inclusive: real(&facets.max_inclusive)?,
            min_exclusive: real(&facets.min_exclusive)?,
            min_inclusive: real(&facets.min_inclusive)?,
        },
        "hexBinary" => match facets.length {
            Some(bytes) if bytes <= 8 => Atom::Hex { bytes },
            _ => text_atom(facets),
        },
        _ => {
            let patterns = &facets.patterns;
            if !patterns.is_empty() && patterns.iter().all(|pattern| pattern.ends_with('%')) {
                Atom::Percentage {
                    patterns: patterns.clone(),
                }
            } else if !patterns.is_empty()
                && patterns
                    .iter()
                    .all(|pattern| pattern.ends_with("(mm|cm|in|pt|pc|pi)"))
            {
                Atom::Measure {
                    patterns: patterns.clone(),
                }
            } else {
                text_atom(facets)
            }
        }
    })
}

fn text_atom(facets: &Facets) -> Atom {
    Atom::Text {
        length: facets.length,
        max_length: facets.max_length,
        min_length: facets.min_length,
        patterns: facets.patterns.clone(),
    }
}

fn base_name(type_ref: &TypeRef) -> Option<String> {
    match type_ref {
        TypeRef::BuiltIn(local) => Some(local.clone()),
        TypeRef::Complex {
            anonymous: _anonymous,
            name: _name,
        }
        | TypeRef::Simple {
            anonymous: _anonymous,
            name: _name,
        } => None,
        TypeRef::Outside(_) | TypeRef::Unknown => None,
    }
}

/// The newtype key of a list's item type; `None` for text.
fn simple_key(data: &Data<'_>, type_ref: &TypeRef) -> Result<Option<Key>> {
    let TypeRef::Simple {
        anonymous: false,
        name,
    } = type_ref
    else {
        return Ok(None);
    };
    let simple_type = data
        .transitional
        .simple_types
        .get(name)
        .with_context(|| format!("no simple type {name}"))?;
    Ok(kind(data, simple_type, &BTreeMap::new())?.map(|_| Key::Xsd(name.clone())))
}

/// The members of an XSD enumeration, each with the Office version the data
/// dates it to.
fn members(data: &Data<'_>, name: &Name, values: &[String]) -> Vec<Member> {
    let prefix = data
        .sdk
        .namespaces
        .iter()
        .find(|namespace| namespace.uri == name.namespace)
        .map(|namespace| namespace.prefix.clone());
    let enumeration = prefix.and_then(|found| {
        data.sdk
            .enumerations
            .get(&format!("{found}:{}", name.local))
    });
    let found = values
        .iter()
        .map(|value| Member {
            introduced: enumeration.and_then(|known| {
                known
                    .facets
                    .iter()
                    .find(|facet| &facet.value == value)
                    .and_then(|facet| version(facet.version))
            }),
            value: value.clone(),
            variant: names::variant(value),
        })
        .collect();
    disambiguate(found)
}

/// Members whose variants collide take their position in the schema's order.
fn disambiguate(mut members: Vec<Member>) -> Vec<Member> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for member in &members {
        *counts.entry(member.variant.clone()).or_default() += 1;
    }
    for (position, member) in members.iter_mut().enumerate() {
        if counts.get(&member.variant).copied().unwrap_or(0) > 1 {
            let _ = write!(member.variant, "{}", position + 1);
        }
    }
    members
}

/// The table's spelling of an Office version; Office 2007 is ECMA-376's own.
const fn version(version: Option<DataVersion>) -> Option<&'static str> {
    match version {
        None | Some(DataVersion::Office2007) => None,
        Some(DataVersion::Microsoft365) => Some("Microsoft365"),
        Some(DataVersion::Office2010) => Some("Office2010"),
        Some(DataVersion::Office2013) => Some("Office2013"),
        Some(DataVersion::Office2016) => Some("Office2016"),
        Some(DataVersion::Office2019) => Some("Office2019"),
        Some(DataVersion::Office2021) => Some("Office2021"),
    }
}

/// Name every union's variants after its member types' newtypes, a member
/// with no newtype after its local name; colliding variants take their
/// position in the schema's order.
pub fn name_variants(simples: &mut [Simple]) {
    let names_by_key: BTreeMap<Key, String> = simples
        .iter()
        .map(|simple| (simple.key.clone(), simple.name.clone()))
        .collect();
    for simple in simples.iter_mut() {
        let Kind::Union(members) = &mut simple.kind else {
            continue;
        };
        for member in members.iter_mut() {
            if let Some(origin) = &member.origin {
                member.variant = names_by_key
                    .get(&Key::Xsd(origin.clone()))
                    .cloned()
                    .unwrap_or_else(|| {
                        let local = origin.local.strip_prefix("ST_").unwrap_or(&origin.local);
                        names::pascal(&names::local_words(local))
                    });
            }
        }
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for member in members.iter() {
            *counts.entry(member.variant.clone()).or_default() += 1;
        }
        for (position, member) in members.iter_mut().enumerate() {
            if counts.get(&member.variant).copied().unwrap_or(0) > 1 {
                let _ = write!(member.variant, "{}", position + 1);
            }
        }
    }
}

/// A simple type's newtype, member enum, and their implementations, and the
/// statics its value reads with.
///
/// # Errors
///
/// A member type with no newtype in the catalog.
pub fn render(
    names: &Names<'_>,
    simple: &Simple,
    uses: &mut Uses,
) -> Result<(String, String, String)> {
    let mut declared = layout::doc(0, &simple.doc);
    declared.push_str("#[derive(Clone, Debug, Default, Eq, PartialEq)]\n#[non_exhaustive]\n");
    let _ = writeln!(declared, "pub struct {}(pub String);", simple.name);
    let conversions = format!(
        "\n{}    fn as_ref(&self) -> &str {{\n        &self.0\n    }}\n}}\n\
         \n{}    fn from(text: String) -> Self {{\n        Self(text)\n    }}\n}}\n",
        layout::impl_header("AsRef<str>", &simple.name),
        layout::impl_header("From<String>", &simple.name)
    );
    uses.add("officekit_xml::values::ValueError");
    let mut parts = [declared, conversions, String::new()];
    let [output, body, methods] = value_body(names, simple, uses, &mut parts)?;
    let [definitions, mut implementations, statics] = parts;
    let _ = write!(
        implementations,
        "\nimpl {} {{\n    /// The value the text spells.\n    ///\n    /// # Errors\n    ///\n    /// `ValueError` when the text is no value of `{}`; the text stays stored.\n",
        simple.name,
        simple.xsd_name()
    );
    implementations.push_str(&layout::signature(
        4,
        "pub fn value",
        &["&self".to_owned()],
        &format!("Result<{output}, ValueError>"),
    ));
    implementations.push_str(&body);
    implementations.push_str("    }\n");
    implementations.push_str(&methods);
    implementations.push_str("}\n");
    Ok((definitions, implementations, statics))
}

/// A newtype's `value`: its output type, its body, and the helper methods
/// the body calls; it adds to `parts`, the definitions, implementations,
/// and statics, what its kind needs.
fn value_body(
    names: &Names<'_>,
    simple: &Simple,
    uses: &mut Uses,
    parts: &mut [String; 3],
) -> Result<[String; 3]> {
    let [definitions, implementations, statics] = parts;
    let screaming = names::screaming(&simple.name);
    let schema = simple.xsd_name().to_owned();
    let member_name = format!("{}Member", simple.name);
    let text_call = |function: &str| {
        Expr::call(
            function,
            vec![Expr::atom("&self.0"), Expr::Atom(layout::string(&schema))],
        )
    };
    Ok(match &simple.kind {
        Kind::Enumeration(members) => {
            definitions.push_str(&member_enum(simple, members));
            implementations.push_str(&member_impls(simple, members, uses));
            let call = Expr::call(
                "markup::member",
                vec![
                    Expr::Atom(format!("{member_name}::MEMBERS")),
                    Expr::atom("&self.0"),
                    Expr::Atom(layout::string(&schema)),
                ],
            );
            [member_name, expression(&call), String::new()]
        }
        Kind::OnOff => {
            let _ = write!(
                implementations,
                "\n{}    fn from(value: bool) -> Self {{\n        Self(value.to_string())\n    }}\n}}\n",
                layout::impl_header("From<bool>", &simple.name)
            );
            [
                "bool".to_owned(),
                expression(&text_call("markup::on_off")),
                String::new(),
            ]
        }
        Kind::Date => {
            uses.add("officekit_xml::values::DateTime");
            [
                "DateTime".to_owned(),
                expression(&text_call("markup::date_time")),
                String::new(),
            ]
        }
        Kind::Atom(atom) => {
            let (output, call) = atom_call(atom, &screaming, &schema, statics, uses, true);
            [output, expression(&call), String::new()]
        }
        Kind::List(item) => {
            let (output, body) = list_value(names, item.as_ref())?;
            [output, body, String::new()]
        }
        Kind::Union(members) => {
            let [definition, body, helpers] =
                union_text(names, simple, members, [&screaming, &schema], statics, uses)?;
            definitions.push_str(&definition);
            [member_name, body, helpers]
        }
    })
}

/// A list's `value`: its output type and body, each item read as its
/// newtype, or as text.
fn list_value(names: &Names<'_>, item: Option<&Key>) -> Result<(String, String)> {
    let Some(key) = item else {
        return Ok((
            "Vec<String>".to_owned(),
            "        Ok(parse::list(&self.0).map(str::to_owned).collect())\n".to_owned(),
        ));
    };
    let found = names
        .catalog
        .simples
        .iter()
        .find(|other| &other.key == key)
        .context("a list's item type has no newtype")?;
    let path = names.path(found.module, &found.name);
    Ok((
        format!("Vec<{path}>"),
        format!(
            "        let mut items = Vec::new();\n        for item in parse::list(&self.0) {{\n            let found = {path}(item.to_owned());\n            found.value()?;\n            items.push(found);\n        }}\n        Ok(items)\n"
        ),
    ))
}

/// A function body that is one expression.
fn expression(call: &Expr) -> String {
    let mut out = String::new();
    layout::line("        ", call, "", &mut out);
    out
}

/// The member enum of an enumeration.
fn member_enum(simple: &Simple, members: &[Member]) -> String {
    let mut out = layout::doc(
        0,
        &format!(
            "The members of `{}`, in alphabetical order.",
            simple.xsd_name()
        ),
    );
    out.push_str("#[derive(Clone, Copy, Debug, Eq, PartialEq)]\n#[non_exhaustive]\n");
    let _ = writeln!(out, "pub enum {}Member {{", simple.name);
    let mut variants: Vec<&str> = members
        .iter()
        .map(|member| member.variant.as_str())
        .collect();
    variants.sort_unstable();
    for variant in variants {
        let _ = writeln!(out, "    {variant},");
    }
    out.push_str("}\n");
    out
}

/// The member enum's table, `as_str`, and `introduced`, and `From` the member.
fn member_impls(simple: &Simple, members: &[Member], uses: &mut Uses) -> String {
    let name = &simple.name;
    let mut out = String::new();
    let _ = write!(
        out,
        "\n{}    fn from(member: {name}Member) -> Self {{\n        Self(member.as_str().to_owned())\n    }}\n}}\n",
        layout::impl_header(&format!("From<{name}Member>"), name)
    );
    let _ = writeln!(out, "\nimpl {name}Member {{");
    let entries: Vec<Expr> = members
        .iter()
        .map(|member| {
            let version = member.introduced.map_or_else(
                || Expr::atom("None"),
                |found| {
                    uses.add("officekit_xml::name::OfficeVersion");
                    Expr::call("Some", vec![Expr::Atom(format!("OfficeVersion::{found}"))])
                },
            );
            Expr::Tuple(vec![
                Expr::Atom(format!("Self::{}", member.variant)),
                Expr::Atom(layout::string(&member.value)),
                version,
            ])
        })
        .collect();
    uses.add("officekit_xml::name::OfficeVersion");
    layout::statement(
        "    const MEMBERS: &markup::Members<Self> = ",
        &Expr::Slice(entries),
        &mut out,
    );
    out.push_str(
        "\n    /// The text the member stands for.\n    #[must_use]\n    pub fn as_str(self) -> &'static str {\n        markup::member_text(Self::MEMBERS, self)\n    }\n\
         \n    /// The Office version that brought the member, from SDK 3.5.1's data; `None` for\n    /// ECMA-376's own.\n    #[must_use]\n    pub fn introduced(self) -> Option<OfficeVersion> {\n        markup::member_version(Self::MEMBERS, self)\n    }\n}\n",
    );
    out
}

/// The call an atom's value reads through, and its owned or borrowed type;
/// its statics are named after `screaming`.
fn atom_call(
    atom: &Atom,
    screaming: &str,
    schema: &str,
    statics: &mut String,
    uses: &mut Uses,
    borrowed: bool,
) -> (String, Expr) {
    let (shown, import) = atom.value_type();
    if let Some(path) = import {
        uses.add(path);
    }
    let output = if atom.is_text() && !borrowed {
        "String"
    } else {
        shown
    };
    let text = Expr::atom("&self.0");
    let name = Expr::Atom(layout::string(schema));
    let call = match atom {
        Atom::Boolean => Expr::call("markup::boolean", vec![text, name]),
        Atom::DateTime => Expr::call("markup::date_time", vec![text, name]),
        Atom::Double {
            max_exclusive,
            max_inclusive,
            min_exclusive,
            min_inclusive,
        } => {
            let bound = |value: &Option<f64>| {
                value.map_or_else(
                    || Expr::atom("None"),
                    |found| Expr::call("Some", vec![Expr::Atom(layout::real(found))]),
                )
            };
            let bounds = Expr::Struct(
                "markup::Bounds".to_owned(),
                vec![
                    ("max_exclusive".to_owned(), bound(max_exclusive)),
                    ("max_inclusive".to_owned(), bound(max_inclusive)),
                    ("min_exclusive".to_owned(), bound(min_exclusive)),
                    ("min_inclusive".to_owned(), bound(min_inclusive)),
                ],
            );
            let path = static_item(screaming, "BOUNDS: markup::Bounds", &bounds, statics);
            Expr::call(
                "markup::double",
                vec![text, Expr::Atom(format!("&{path}")), name],
            )
        }
        Atom::Hex { bytes } => Expr::call(
            "markup::hex",
            vec![text, Expr::Atom(bytes.to_string()), name],
        ),
        Atom::Integer { max, min, values } if values.is_empty() => {
            let range = [min, max].map(|bound| Expr::Atom(layout::integer(*bound)));
            let [low, high] = range;
            Expr::call("markup::integer", vec![text, low, high, name])
        }
        Atom::Integer {
            max: _max,
            min: _min,
            values,
        } => Expr::call("markup::integer_in", vec![text, strings(values), name]),
        Atom::Measure { patterns } => {
            let path = static_item(screaming, "PATTERNS: &[&str]", &strings(patterns), statics);
            Expr::call("markup::measure", vec![text, Expr::Atom(path), name])
        }
        Atom::Percentage { patterns } => {
            let path = static_item(screaming, "PATTERNS: &[&str]", &strings(patterns), statics);
            Expr::call("markup::percentage", vec![text, Expr::Atom(path), name])
        }
        Atom::Text {
            length,
            max_length,
            min_length,
            patterns,
        } => {
            let count = |value: &Option<u32>| {
                value.map_or_else(
                    || Expr::atom("None"),
                    |found| Expr::call("Some", vec![Expr::Atom(found.to_string())]),
                )
            };
            let facets = Expr::Struct(
                "markup::Facets".to_owned(),
                vec![
                    ("length".to_owned(), count(length)),
                    ("max_length".to_owned(), count(max_length)),
                    ("min_length".to_owned(), count(min_length)),
                    ("patterns".to_owned(), strings(patterns)),
                ],
            );
            let path = static_item(screaming, "FACETS: markup::Facets", &facets, statics);
            Expr::call(
                "markup::text",
                vec![text, Expr::Atom(format!("&{path}")), name],
            )
        }
    };
    (output.to_owned(), call)
}

/// A static the value reads with, `{screaming}_{declared}` holding `value`;
/// returns its name.
fn static_item(screaming: &str, declared: &str, value: &Expr, statics: &mut String) -> String {
    let (suffix, _) = declared.split_once(':').unwrap_or((declared, ""));
    statics.push('\n');
    layout::statement(&format!("static {screaming}_{declared} = "), value, statics);
    format!("{screaming}_{suffix}")
}

/// An array of string literals.
fn strings(values: &[String]) -> Expr {
    Expr::Slice(
        values
            .iter()
            .map(|value| Expr::Atom(layout::string(value)))
            .collect(),
    )
}

/// A union's member enum, its `value` body, and the helper methods that body
/// calls when it is too long for one function: each member type tried in the
/// schema's order, the first that matches winning.
fn union_text(
    names: &Names<'_>,
    simple: &Simple,
    members: &[UnionMember],
    [screaming, schema]: [&str; 2],
    statics: &mut String,
    uses: &mut Uses,
) -> Result<[String; 3]> {
    let member_name = format!("{}Member", simple.name);
    let mut variants: Vec<(String, String)> = Vec::new();
    let mut trials: Vec<Trial> = Vec::new();
    let mut text_member: Option<String> = None;
    let mut eq = true;
    let width = members.len().to_string().len();
    for (position, member) in members.iter().enumerate() {
        // Zero-padded, so clippy's `similar_names` reads `member_02` and
        // `member_12` as numbered alike.
        let binding = format!("member_{:0width$}", position + 1);
        match &member.value {
            MemberValue::Atom(atom) => {
                let prefix = format!("{screaming}_{}", names::screaming(&member.variant));
                let (output, call) = atom_call(atom, &prefix, schema, statics, uses, false);
                eq &= !matches!(output.as_str(), "f64" | "Measure");
                variants.push((member.variant.clone(), output));
                if text_member.is_none() {
                    trials.push(Trial {
                        test: binding.clone(),
                        binding,
                        owned: if atom.is_text() { ".to_owned()" } else { "" },
                        read: call,
                        variant: member.variant.clone(),
                    });
                }
            }
            MemberValue::Simple(key) => {
                let found = names
                    .catalog
                    .simples
                    .iter()
                    .find(|other| &other.key == key)
                    .context("a union's member type has no newtype")?;
                let path = names.path(found.module, &found.name);
                let (output, borrowed) = owned_type(names, found, uses)?;
                eq &= !matches!(output.as_str(), "f64" | "Measure")
                    && !matches!(&found.kind, Kind::Union(_));
                variants.push((member.variant.clone(), output));
                if text_member.is_none() {
                    trials.push(Trial {
                        test: format!("{binding}.value()"),
                        binding,
                        owned: if borrowed { ".to_owned()" } else { "" },
                        read: Expr::call(&path, vec![Expr::atom("self.0.clone()")]),
                        variant: member.variant.clone(),
                    });
                }
            }
            MemberValue::Text => {
                variants.push((member.variant.clone(), "String".to_owned()));
                if text_member.is_none() {
                    text_member = Some(member.variant.clone());
                }
            }
        }
    }
    let [body, methods] = union_value(&member_name, schema, &trials, text_member.as_deref());
    variants.sort();
    let mut definition = layout::doc(
        0,
        &format!(
            "The member types of `{}`, in alphabetical order; `value` tries them in the schema's order.",
            simple.xsd_name()
        ),
    );
    let derives = if eq {
        "Clone, Debug, Eq, PartialEq"
    } else {
        "Clone, Debug, PartialEq"
    };
    let _ = writeln!(definition, "#[derive({derives})]\n#[non_exhaustive]");
    let _ = writeln!(definition, "pub enum {member_name} {{");
    for (variant, output) in variants {
        let _ = writeln!(definition, "    {variant}({output}),");
    }
    definition.push_str("}\n");
    Ok([definition, body, methods])
}

/// A union's `value` body, and the helper methods it calls when its trials
/// take more than `BODY_LINES` lines: each helper tries a run of the member
/// types and gives `None` when none matches.
fn union_value(
    member_name: &str,
    schema: &str,
    trials: &[Trial],
    text_member: Option<&str>,
) -> [String; 2] {
    let text_item = |wrapper: &str| {
        text_member.map(|variant| {
            let call = Expr::call(
                &format!("{member_name}::{variant}"),
                vec![Expr::atom("self.0.clone()")],
            );
            Expr::call(wrapper, vec![call])
        })
    };
    let invalid = Expr::call(
        "markup::invalid",
        vec![Expr::atom("&self.0"), Expr::Atom(layout::string(schema))],
    );
    let mut body = String::new();
    for trial in trials {
        trial_text(member_name, trial, "Ok", &mut body);
    }
    let last = text_item("Ok").unwrap_or_else(|| Expr::call("Err", vec![invalid.clone()]));
    layout::line("        ", &last, "", &mut body);
    if body.lines().count() <= BODY_LINES {
        return [body, String::new()];
    }
    let mut chunks: Vec<String> = Vec::new();
    let mut chunk = String::new();
    for trial in trials {
        let mut text = String::new();
        trial_text(member_name, trial, "Some", &mut text);
        if !chunk.is_empty() && chunk.lines().count() + text.lines().count() >= BODY_LINES {
            chunk.push_str("        None\n");
            chunks.push(chunk);
            chunk = String::new();
        }
        chunk.push_str(&text);
    }
    if let Some(end) = text_item("Some") {
        layout::line("        ", &end, "", &mut chunk);
    } else {
        chunk.push_str("        None\n");
    }
    chunks.push(chunk);
    let mut methods = String::new();
    let mut calls = Vec::new();
    for (index, text) in chunks.iter().enumerate() {
        let number = index + 1;
        let _ = write!(
            methods,
            "\n    fn value_{number}(&self) -> Option<{member_name}> {{\n{text}    }}\n"
        );
        if number > 1 {
            calls.push(format!(".or_else(|| self.value_{number}())"));
        }
    }
    calls.push(format!(".ok_or_else(|| {})", invalid.one_line()));
    [layout::chain(8, "self.value_1()", &calls), methods]
}

/// One member type's trial: its binding, and the early return of the member
/// it gives, wrapped in `wrapper` (`Ok` or `Some`).
fn trial_text(member_name: &str, trial: &Trial, wrapper: &str, body: &mut String) {
    layout::statement(
        &format!("        let {} = ", trial.binding),
        &trial.read,
        body,
    );
    body.push_str(&layout::if_let(
        8,
        &Expr::pattern("Ok", "value"),
        &trial.test,
    ));
    let value = Expr::call(
        &format!("{member_name}::{}", trial.variant),
        vec![Expr::Atom(format!("value{}", trial.owned))],
    );
    layout::line(
        "            return ",
        &Expr::call(wrapper, vec![value]),
        ";",
        body,
    );
    body.push_str("        }\n");
}

/// A newtype's value type, owned, and whether its `value` borrows it.
fn owned_type(names: &Names<'_>, simple: &Simple, uses: &mut Uses) -> Result<(String, bool)> {
    Ok(match &simple.kind {
        Kind::Atom(atom) => {
            let (shown, import) = atom.value_type();
            if let Some(path) = import {
                uses.add(path);
            }
            let owned = if atom.is_text() { "String" } else { shown };
            (owned.to_owned(), atom.is_text())
        }
        Kind::OnOff => ("bool".to_owned(), false),
        Kind::Date => {
            uses.add("officekit_xml::values::DateTime");
            ("DateTime".to_owned(), false)
        }
        Kind::Enumeration(_) | Kind::Union(_) => (
            names.path(simple.module, &format!("{}Member", simple.name)),
            false,
        ),
        Kind::List(item) => {
            let inner = match item {
                Some(key) => {
                    let found = names
                        .catalog
                        .simples
                        .iter()
                        .find(|other| &other.key == key)
                        .context("a list's item type has no newtype")?;
                    names.path(found.module, &found.name)
                }
                None => "String".to_owned(),
            };
            (format!("Vec<{inner}>"), false)
        }
    })
}
