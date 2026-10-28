//! The semantic constraint table.
//!
//! Every constraint SDK 3.5.1 registers on an element type, read from the
//! Schematron rules of its data and checked against the registrations `just
//! semantic-inputs` reads from its package by reflection, which also give any
//! registration no rule gives.
//!
//! SDK 3.5.1's generator registers on each element type the constraints of
//! the rules whose context is the type's element, in the rules' order, a
//! constraint equal to an earlier one left out with its versions and
//! applications.

mod schematron;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use os_shim::{System, WriteMode};
use serde_json::Value;

use super::namespaces;
use super::validation::render::{self, Expr};
use super::validation::{Emitted, Versions};
use crate::generate_schema::inputs::Inputs;
use crate::generate_schema::model::Name;
use crate::generate_schema::sdk_data::{DataVersion, SdkData};
use schematron::Names;

#[cfg(test)]
mod tests;

/// Where `generate-schema` writes the table.
pub const GENERATED_PATH: &str = "crates/officekit/src/validation/generated/semantic.rs";

/// Every application: Word, Excel, and `PowerPoint`.
const ALL_APPLICATIONS: u8 = 0b111;

/// Each kind's variant of the table's `Constraint`, and the summary's name for it.
const KINDS: [(&str, &str); 16] = [
    (
        "Constraint::AbsentUnlessValue",
        "absent when another lacks a value",
    ),
    (
        "Constraint::AbsentWhenValue",
        "absent when another has a value",
    ),
    ("Constraint::CannotBeOmitted", "cannot be omitted"),
    ("Constraint::IndexReference", "index reference"),
    ("Constraint::LessOrEqual", "less than or equal to another"),
    ("Constraint::MutuallyExclusive", "mutually exclusive"),
    ("Constraint::ReferenceExists", "reference exists"),
    ("Constraint::RelationshipExists", "relationship exists"),
    ("Constraint::RelationshipType", "relationship type"),
    (
        "Constraint::RequiredWhenValue",
        "required when another has a value",
    ),
    ("Constraint::UniqueValue", "unique value"),
    (
        "Constraint::ValueConditioned",
        "value conditioned on another",
    ),
    ("Constraint::ValueLength", "value length"),
    ("Constraint::ValuePattern", "value pattern"),
    ("Constraint::ValueRange", "value range"),
    ("Constraint::ValueSet", "value set"),
];

/// One constraint, its names by namespace URI (empty for none) and local name.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Constraint {
    AbsentUnlessValue {
        attribute: Name,
        other: Name,
        values: Vec<String>,
    },
    AbsentWhenValue {
        attribute: Name,
        other: Name,
        values: Vec<String>,
    },
    CannotBeOmitted {
        attribute: Name,
    },
    IndexReference {
        attribute: Name,
        element: Name,
        index_base: u8,
        parent: Option<Name>,
        part: String,
    },
    LessOrEqual {
        attribute: Name,
        can_equal: bool,
        other: Name,
    },
    MutuallyExclusive {
        attributes: Vec<Name>,
    },
    ReferenceExists {
        attribute: Name,
        element: Name,
        element_attribute: Name,
        /// The element as the rule writes it, `w:footnote`.
        element_name: String,
        part: String,
    },
    RelationshipExists {
        attribute: Name,
    },
    RelationshipType {
        attribute: Name,
        relationship_type: String,
    },
    RequiredWhenValue {
        attribute: Name,
        other: Name,
        value: String,
    },
    UniqueValue {
        attribute: Name,
        case_sensitive: bool,
        parent: Option<Name>,
    },
    ValueConditioned {
        attribute: Name,
        other: Name,
        other_values: Vec<String>,
        values: Vec<String>,
    },
    ValueLength {
        attribute: Name,
        max: u32,
        min: u32,
    },
    ValuePattern {
        attribute: Name,
        pattern: String,
    },
    ValueRange {
        attribute: Name,
        max: f64,
        max_inclusive: bool,
        min: f64,
        min_inclusive: bool,
    },
    ValueSet {
        attribute: Name,
        valid: bool,
        values: Vec<String>,
    },
}

/// A constraint and where it applies.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Registration {
    /// Bit 0 Word, bit 1 Excel, bit 2 `PowerPoint`.
    pub applications: u8,
    pub constraint: Constraint,
    /// The version it applies from, and every later one.
    pub versions: Versions,
}

/// One element's registrations, as every class of it in the dump has them.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Registered {
    /// The dump's classes of the element; each is one element type of the data.
    pub classes: usize,
    /// In SDK 3.5.1's order, each with where it was read.
    pub registrations: Vec<(Registration, Source)>,
}

/// Where a registration was read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Source {
    /// The reflection dump alone: no rule gives it.
    Dump,
    /// The rule of `schematrons.json` at this index.
    Rule(usize),
}

/// What was written.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Summary {
    /// Rows per kind, in the order of `Constraint`'s variants.
    pub by_kind: Vec<(&'static str, usize)>,
    /// Rows no rule gives, read from the reflection dump.
    pub from_dump: usize,
    /// Rows a Schematron rule gives.
    pub from_rules: usize,
    pub rows: usize,
    /// Element types with at least one row.
    pub types: usize,
}

/// One element class of the dump: its element and its registrations.
struct DumpClass {
    element: Name,
    registrations: Vec<Registration>,
}

/// The names a row's constraint resolves: the attributes of its element
/// type, and the namespace ids of the table.
struct Resolver<'names> {
    attributes: Vec<Name>,
    namespace_ids: &'names BTreeMap<&'names str, usize>,
}

impl Resolver<'_> {
    /// `Constraint::path { attribute, other, values }`.
    fn condition(
        &self,
        path: &'static str,
        attribute: &Name,
        other: &Name,
        values: &[String],
    ) -> Result<Expr> {
        Ok(Expr::Struct(
            path,
            vec![
                ("attribute", self.slot(attribute)?),
                ("other", self.slot(other)?),
                ("values", texts(values)),
            ],
        ))
    }

    /// A constraint's expression.
    fn constraint(&self, constraint: &Constraint) -> Result<Expr> {
        match constraint {
            Constraint::AbsentUnlessValue {
                attribute,
                other,
                values,
            } => self.condition("Constraint::AbsentUnlessValue", attribute, other, values),
            Constraint::AbsentWhenValue {
                attribute,
                other,
                values,
            } => self.condition("Constraint::AbsentWhenValue", attribute, other, values),
            Constraint::CannotBeOmitted { attribute } => {
                self.single("Constraint::CannotBeOmitted", attribute)
            }
            Constraint::IndexReference {
                attribute,
                element,
                index_base,
                parent,
                part,
            } => self.index_reference(attribute, (element, parent.as_ref()), *index_base, part),
            Constraint::LessOrEqual {
                attribute,
                can_equal,
                other,
            } => self.less_or_equal(attribute, *can_equal, other),
            Constraint::MutuallyExclusive { attributes } => self.mutually_exclusive(attributes),
            Constraint::ReferenceExists {
                attribute,
                element,
                element_attribute,
                element_name: _element_name,
                part,
            } => self.reference_exists(attribute, element, element_attribute, part),
            Constraint::RelationshipExists { attribute } => {
                self.single("Constraint::RelationshipExists", attribute)
            }
            Constraint::RelationshipType {
                attribute,
                relationship_type,
            } => self.relationship_type(attribute, relationship_type),
            Constraint::RequiredWhenValue {
                attribute,
                other,
                value,
            } => self.required_when_value(attribute, other, value),
            Constraint::UniqueValue {
                attribute,
                case_sensitive,
                parent,
            } => self.unique_value(attribute, *case_sensitive, parent.as_ref()),
            Constraint::ValueConditioned {
                attribute,
                other,
                other_values,
                values,
            } => self.value_conditioned((attribute, values), (other, other_values)),
            Constraint::ValueLength {
                attribute,
                max,
                min,
            } => self.value_length(attribute, *max, *min),
            Constraint::ValuePattern { attribute, pattern } => {
                self.value_pattern(attribute, pattern)
            }
            Constraint::ValueRange {
                attribute,
                max,
                max_inclusive,
                min,
                min_inclusive,
            } => self.value_range(attribute, (*min, *min_inclusive), (*max, *max_inclusive)),
            Constraint::ValueSet {
                attribute,
                valid,
                values,
            } => self.value_set(attribute, *valid, values),
        }
    }

    /// An element's name as the table writes it.
    fn element(&self, name: &Name) -> Result<Expr> {
        Ok(Expr::Struct(
            "ElementName",
            vec![
                ("local", render::string(&name.local)),
                ("namespace", render::integer(self.namespace_id(name)?)),
            ],
        ))
    }

    fn index_reference(
        &self,
        attribute: &Name,
        (element, parent): (&Name, Option<&Name>),
        index_base: u8,
        part: &str,
    ) -> Result<Expr> {
        Ok(Expr::Struct(
            "Constraint::IndexReference",
            vec![
                ("attribute", self.slot(attribute)?),
                ("element", self.element(element)?),
                ("index_base", render::integer(u64::from(index_base))),
                ("parent", self.optional_element(parent)?),
                ("part", render::string(part)),
            ],
        ))
    }

    fn less_or_equal(&self, attribute: &Name, can_equal: bool, other: &Name) -> Result<Expr> {
        Ok(Expr::Struct(
            "Constraint::LessOrEqual",
            vec![
                ("attribute", self.slot(attribute)?),
                ("can_equal", render::boolean(can_equal)),
                ("other", self.slot(other)?),
            ],
        ))
    }

    /// The slots of the attributes the type declares; the others are left out.
    fn mutually_exclusive(&self, attributes: &[Name]) -> Result<Expr> {
        let mut slots = Vec::new();
        for name in attributes {
            if let Some(index) = attribute_slot(&self.attributes, name)? {
                slots.push(render::integer(u64::from(index)));
            }
        }
        Ok(Expr::Struct(
            "Constraint::MutuallyExclusive",
            vec![("attributes", Expr::Slice(slots))],
        ))
    }

    fn namespace_id(&self, name: &Name) -> Result<u64> {
        self.namespace_ids
            .get(name.namespace.as_str())
            .map(|id| *id as u64)
            .with_context(|| format!("no namespace id for {name}"))
    }

    fn optional_element(&self, name: Option<&Name>) -> Result<Expr> {
        Ok(render::option(
            name.map(|found| self.element(found)).transpose()?,
        ))
    }

    fn reference_exists(
        &self,
        attribute: &Name,
        element: &Name,
        element_attribute: &Name,
        part: &str,
    ) -> Result<Expr> {
        let namespace = if element_attribute.namespace.is_empty() {
            None
        } else {
            Some(render::integer(self.namespace_id(element_attribute)?))
        };
        Ok(Expr::Struct(
            "Constraint::ReferenceExists",
            vec![
                ("attribute", self.slot(attribute)?),
                ("element", self.element(element)?),
                (
                    "element_attribute",
                    Expr::Struct(
                        "AttributeName",
                        vec![
                            ("local", render::string(&element_attribute.local)),
                            ("namespace", render::option(namespace)),
                        ],
                    ),
                ),
                ("part", render::string(part)),
            ],
        ))
    }

    fn relationship_type(&self, attribute: &Name, relationship_type: &str) -> Result<Expr> {
        Ok(Expr::Struct(
            "Constraint::RelationshipType",
            vec![
                ("attribute", self.slot(attribute)?),
                ("relationship_type", render::string(relationship_type)),
            ],
        ))
    }

    fn required_when_value(&self, attribute: &Name, other: &Name, value: &str) -> Result<Expr> {
        Ok(Expr::Struct(
            "Constraint::RequiredWhenValue",
            vec![
                ("attribute", self.slot(attribute)?),
                ("other", self.slot(other)?),
                ("value", render::string(value)),
            ],
        ))
    }

    /// `Constraint::path { attribute }`.
    fn single(&self, path: &'static str, attribute: &Name) -> Result<Expr> {
        Ok(Expr::Struct(
            path,
            vec![("attribute", self.slot(attribute)?)],
        ))
    }

    /// An attribute's slot as the table writes it; `None` when the type
    /// declares no attribute of its local name.
    fn slot(&self, name: &Name) -> Result<Expr> {
        Ok(render::option(
            attribute_slot(&self.attributes, name)?.map(|index| render::integer(u64::from(index))),
        ))
    }

    fn unique_value(
        &self,
        attribute: &Name,
        case_sensitive: bool,
        parent: Option<&Name>,
    ) -> Result<Expr> {
        Ok(Expr::Struct(
            "Constraint::UniqueValue",
            vec![
                ("attribute", self.slot(attribute)?),
                ("case_sensitive", render::boolean(case_sensitive)),
                ("parent", self.optional_element(parent)?),
            ],
        ))
    }

    fn value_conditioned(
        &self,
        (attribute, values): (&Name, &[String]),
        (other, other_values): (&Name, &[String]),
    ) -> Result<Expr> {
        Ok(Expr::Struct(
            "Constraint::ValueConditioned",
            vec![
                ("attribute", self.slot(attribute)?),
                ("other", self.slot(other)?),
                ("other_values", texts(other_values)),
                ("values", texts(values)),
            ],
        ))
    }

    fn value_length(&self, attribute: &Name, max: u32, min: u32) -> Result<Expr> {
        Ok(Expr::Struct(
            "Constraint::ValueLength",
            vec![
                ("attribute", self.slot(attribute)?),
                ("max", render::integer(u64::from(max))),
                ("min", render::integer(u64::from(min))),
            ],
        ))
    }

    fn value_pattern(&self, attribute: &Name, pattern: &str) -> Result<Expr> {
        Ok(Expr::Struct(
            "Constraint::ValuePattern",
            vec![
                ("attribute", self.slot(attribute)?),
                ("pattern", render::string(&rust_pattern(pattern)?)),
            ],
        ))
    }

    fn value_range(
        &self,
        attribute: &Name,
        (min, min_inclusive): (f64, bool),
        (max, max_inclusive): (f64, bool),
    ) -> Result<Expr> {
        Ok(Expr::Struct(
            "Constraint::ValueRange",
            vec![
                ("attribute", self.slot(attribute)?),
                ("max", render::real(max)),
                ("max_inclusive", render::boolean(max_inclusive)),
                ("min", render::real(min)),
                ("min_inclusive", render::boolean(min_inclusive)),
            ],
        ))
    }

    fn value_set(&self, attribute: &Name, valid: bool, values: &[String]) -> Result<Expr> {
        Ok(Expr::Struct(
            "Constraint::ValueSet",
            vec![
                ("attribute", self.slot(attribute)?),
                ("valid", render::boolean(valid)),
                ("values", texts(values)),
            ],
        ))
    }
}

/// The applications an `App` or `application` names: `All`, or a list such
/// as `Word, Excel`.
fn applications(text: &str) -> Result<u8> {
    let mut found = 0;
    for name in text.split(',').map(str::trim) {
        found |= match name {
            "All" => ALL_APPLICATIONS,
            "Word" => 0b001,
            "Excel" => 0b010,
            "PowerPoint" => 0b100,
            _ => bail!("unknown application {name}"),
        };
    }
    Ok(found)
}

/// A slot: the attribute of `attributes` SDK 3.5.1 finds for `name`, the one
/// of the same namespace and local name, else the last of the local name.
fn attribute_slot(attributes: &[Name], name: &Name) -> Result<Option<u8>> {
    let mut found = None;
    for (index, declared) in attributes.iter().enumerate() {
        if declared.local == name.local {
            found = Some(index);
            if declared.namespace == name.namespace {
                break;
            }
        }
    }
    found
        .map(|index| u8::try_from(index).context("an attribute slot past 255"))
        .transpose()
}

/// A dump class's registrations.
fn dump_class(class: &Value) -> Result<DumpClass> {
    let text = |name: &str| {
        class
            .get(name)
            .and_then(Value::as_str)
            .with_context(|| format!("a class without `{name}`"))
    };
    let registrations = class
        .get("constraints")
        .and_then(Value::as_array)
        .context("a class without `constraints`")?
        .iter()
        .map(dump_registration)
        .collect::<Result<Vec<Registration>>>()
        .with_context(|| format!("in {}", text("class").unwrap_or_default()))?;
    Ok(DumpClass {
        element: Name::new(text("namespace")?, text("local")?),
        registrations,
    })
}

/// A dump registration: its class's fields read into a constraint.
fn dump_registration(entry: &Value) -> Result<Registration> {
    let kind = field_text(entry, "kind")?;
    let fields = entry.get("fields").context("no `fields`")?;
    let constraint = match kind {
        "AttributeAbsentConditionToNonValue" => Constraint::AbsentUnlessValue {
            attribute: field_name(fields, "_absentAttribute")?,
            other: field_name(fields, "_conditionAttribute")?,
            values: field_texts(fields, "_values")?,
        },
        "AttributeAbsentConditionToValue" => Constraint::AbsentWhenValue {
            attribute: field_name(fields, "_absentAttribute")?,
            other: field_name(fields, "_conditionAttribute")?,
            values: field_texts(fields, "_values")?,
        },
        "AttributeCannotOmitConstraint" => Constraint::CannotBeOmitted {
            attribute: field_name(fields, "_attribute")?,
        },
        "AttributeMutualExclusive" => Constraint::MutuallyExclusive {
            attributes: fields
                .get("_attributes")
                .and_then(Value::as_array)
                .context("no `_attributes`")?
                .iter()
                .map(qualified)
                .collect::<Result<Vec<Name>>>()?,
        },
        "AttributeRequiredConditionToValue" => Constraint::RequiredWhenValue {
            attribute: field_name(fields, "_requiredAttribute")?,
            other: field_name(fields, "_conditionAttribute")?,
            value: field_text(fields, "_value")?.to_owned(),
        },
        "AttributeValueConditionToAnother" => Constraint::ValueConditioned {
            attribute: field_name(fields, "_attribute")?,
            other: field_name(fields, "_conditionAttribute")?,
            other_values: field_texts(fields, "_otherValues")?,
            values: field_texts(fields, "_values")?,
        },
        "AttributeValueLengthConstraint" => Constraint::ValueLength {
            attribute: field_name(fields, "_attribute")?,
            max: field_number(fields, "_maxLength")?,
            min: field_number(fields, "_minLength")?,
        },
        "AttributeValueLessEqualToAnother" => Constraint::LessOrEqual {
            attribute: field_name(fields, "_attribute")?,
            can_equal: field_flag(fields, "_canEqual")?,
            other: field_name(fields, "_otherAttribute")?,
        },
        "AttributeValuePatternConstraint" => Constraint::ValuePattern {
            attribute: field_name(fields, "_attribute")?,
            pattern: field_text(fields, "_pattern")?.to_owned(),
        },
        "AttributeValueRangeConstraint" => Constraint::ValueRange {
            attribute: field_name(fields, "_attribute")?,
            max: field_real(fields, "_maxValue")?,
            max_inclusive: field_flag(fields, "_maxInclusive")?,
            min: field_real(fields, "_minValue")?,
            min_inclusive: field_flag(fields, "_minInclusive")?,
        },
        "AttributeValueSetConstraint" => Constraint::ValueSet {
            attribute: field_name(fields, "_attribute")?,
            valid: field_flag(fields, "_isValidValueSet")?,
            values: field_texts(fields, "_valueSet")?,
        },
        _ => dump_reference(kind, fields)?,
    };
    Ok(Registration {
        applications: applications(field_text(entry, "application")?)?,
        constraint,
        versions: versions(Some(field_text(entry, "version")?))?,
    })
}

/// The dump's constraints that name another element, part, or relationship.
fn dump_reference(kind: &str, fields: &Value) -> Result<Constraint> {
    Ok(match kind {
        "IndexReferenceConstraint" => Constraint::IndexReference {
            attribute: field_name(fields, "_attribute")?,
            element: field_name(fields, "_refElement")?,
            index_base: field_number(fields, "_indexBase")?,
            parent: field_optional_name(fields, "_refElementParent")?,
            part: field_text(fields, "_refPartType")?.to_owned(),
        },
        "ReferenceExistConstraint" => Constraint::ReferenceExists {
            attribute: field_name(fields, "_refAttribute")?,
            element: field_name(fields, "_element")?,
            element_attribute: field_name(fields, "_attribute")?,
            element_name: field_text(fields, "_elementName")?.to_owned(),
            part: field_text(fields, "_partPath")?.to_owned(),
        },
        "RelationshipExistConstraint" => Constraint::RelationshipExists {
            attribute: field_name(fields, "_attribute")?,
        },
        "RelationshipTypeConstraint" => Constraint::RelationshipType {
            attribute: field_name(fields, "_attribute")?,
            relationship_type: field_text(fields, "_type")?.to_owned(),
        },
        "UniqueAttributeValueConstraint" => Constraint::UniqueValue {
            attribute: field_name(fields, "_attribute")?,
            case_sensitive: match field_text(fields, "_comparer")? {
                "Ordinal" => true,
                "OrdinalIgnoreCase" => false,
                other => bail!("unknown comparer {other}"),
            },
            parent: field_optional_name(fields, "_parent")?,
        },
        _ => bail!("unknown constraint class {kind}"),
    })
}

/// Read `inputs/sdk-3.5.1-semantic.json` and the data's rules, and write the
/// table: one row per registration on each element type, sorted by type id.
///
/// # Errors
///
/// An input that cannot be read, a rule or registration that does not read,
/// a rule whose constraint SDK 3.5.1 does not register, an element of the
/// dump whose class count differs from the data's types, or an output that
/// cannot be written.
pub fn emit<S>(system: &S, inputs: &Inputs, emitted: &Emitted, out: &Path) -> Result<Summary>
where
    S: System + ?Sized,
{
    let dump_text = system
        .read_to_string(&inputs.semantic_dump)
        .with_context(|| format!("cannot read {}", inputs.semantic_dump.display()))?;
    let dump: Value = serde_json::from_str(&dump_text)
        .with_context(|| format!("{} is not JSON", inputs.semantic_dump.display()))?;
    let by_element = registrations(&emitted.data, &dump)?;
    let mut read: Vec<PathBuf> = vec![inputs.sdk_data.join("namespaces.json")];
    let mut schemas: Vec<PathBuf> = system
        .read_dir(&inputs.sdk_data.join("schemas"))
        .context("cannot list the data's schemas")?
        .into_iter()
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    schemas.sort();
    read.extend(schemas);
    read.push(inputs.sdk_data.join("schematrons.json"));
    read.push(inputs.semantic_dump.clone());
    let header = namespaces::header(inputs.checksums(), inputs.tools(), &read, &[])?;
    let (text, summary) = render_table(&header, emitted, &by_element)?;
    system
        .write(out, text.as_bytes(), WriteMode::Replace)
        .with_context(|| format!("cannot write {}", out.display()))?;
    Ok(summary)
}

fn field_flag(fields: &Value, name: &str) -> Result<bool> {
    fields
        .get(name)
        .and_then(Value::as_bool)
        .with_context(|| format!("no flag `{name}`"))
}

fn field_name(fields: &Value, name: &str) -> Result<Name> {
    qualified(fields.get(name).with_context(|| format!("no `{name}`"))?)
        .with_context(|| format!("in `{name}`"))
}

fn field_number<T>(fields: &Value, name: &str) -> Result<T>
where
    T: TryFrom<u64>,
{
    fields
        .get(name)
        .and_then(Value::as_u64)
        .and_then(|number| T::try_from(number).ok())
        .with_context(|| format!("no whole number `{name}` in range"))
}

fn field_optional_name(fields: &Value, name: &str) -> Result<Option<Name>> {
    match fields.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => qualified(value).map(Some),
    }
}

/// A number, or the text `Infinity` or `-Infinity`.
fn field_real(fields: &Value, name: &str) -> Result<f64> {
    let value = fields.get(name).with_context(|| format!("no `{name}`"))?;
    match (value.as_f64(), value.as_str()) {
        (Some(number), _) => Ok(number),
        (None, Some("Infinity")) => Ok(f64::INFINITY),
        (None, Some("-Infinity")) => Ok(f64::NEG_INFINITY),
        (None, _) => bail!("`{name}` is not a number"),
    }
}

fn field_text<'value>(fields: &'value Value, name: &str) -> Result<&'value str> {
    fields
        .get(name)
        .and_then(Value::as_str)
        .with_context(|| format!("no text `{name}`"))
}

fn field_texts(fields: &Value, name: &str) -> Result<Vec<String>> {
    fields
        .get(name)
        .and_then(Value::as_array)
        .with_context(|| format!("no list `{name}`"))?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .with_context(|| format!("an item of `{name}` is not text"))
        })
        .collect()
}

/// `{"namespace": ..., "local": ...}` as a name.
fn qualified(value: &Value) -> Result<Name> {
    let text = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_str)
            .with_context(|| format!("a name without `{name}`"))
    };
    Ok(Name::new(text("namespace")?, text("local")?))
}

/// The registrations of each element, as the dump gives them, each with the
/// rule that gives it.
///
/// # Errors
///
/// A rule or registration that does not read, two classes of one element
/// with different registrations, or a rule whose constraint the dump lacks.
pub fn registrations(data: &SdkData, dump: &Value) -> Result<BTreeMap<Name, Registered>> {
    let ruled = rule_registrations(data)?;
    let mut by_element: BTreeMap<Name, Registered> = BTreeMap::new();
    for class in dump.as_array().context("the dump is not a JSON array")? {
        let read = dump_class(class)?;
        if let Some(known) = by_element.get_mut(&read.element) {
            let same = known.registrations.len() == read.registrations.len()
                && known
                    .registrations
                    .iter()
                    .zip(&read.registrations)
                    .all(|((left, _), right)| left == right);
            if !same {
                bail!(
                    "two classes of {} differ in their registrations",
                    read.element
                );
            }
            known.classes += 1;
            continue;
        }
        let from_rules = ruled.get(&read.element).map_or(&[][..], Vec::as_slice);
        let sourced = read
            .registrations
            .into_iter()
            .map(|registration| {
                let source = from_rules
                    .iter()
                    .find(|(from_rule, _)| *from_rule == registration)
                    .map_or(Source::Dump, |(_, index)| Source::Rule(*index));
                (registration, source)
            })
            .collect::<Vec<_>>();
        by_element.insert(
            read.element,
            Registered {
                classes: 1,
                registrations: sourced,
            },
        );
    }
    for (element, rules) in &ruled {
        for (registration, index) in rules {
            let registered = by_element.get(element).is_some_and(|found| {
                found
                    .registrations
                    .iter()
                    .any(|(dumped, _)| dumped == registration)
            });
            if !registered {
                bail!(
                    "the rule {index} gives {element} a constraint SDK 3.5.1 does not register: \
                     {registration:?}"
                );
            }
        }
    }
    Ok(by_element)
}

/// A table row's expression: the registration on `type_id`; and the index
/// in `KINDS` of its kind.
fn row_expr(
    type_id: usize,
    registration: &Registration,
    resolver: &Resolver<'_>,
) -> Result<(usize, Expr)> {
    let constraint = resolver.constraint(&registration.constraint)?;
    let kind = match &constraint {
        Expr::Struct(path, _) => KINDS.iter().position(|(variant, _)| variant == path),
        Expr::Atom(_) | Expr::Call(..) | Expr::Ref(_) | Expr::Slice(_) | Expr::Tuple(_) => None,
    }
    .context("a constraint of no kind")?;
    Ok((
        kind,
        Expr::Struct(
            "Registration",
            vec![
                (
                    "applications",
                    Expr::Call(
                        "Applications",
                        vec![Expr::Atom(format!("0b{:03b}", registration.applications))],
                    ),
                ),
                ("constraint", constraint),
                ("type_id", render::integer(type_id as u64)),
                ("versions", render::versions_expr(registration.versions)),
            ],
        ),
    ))
}

/// The table's text after `header`, and its counts.
fn render_table(
    header: &str,
    emitted: &Emitted,
    by_element: &BTreeMap<Name, Registered>,
) -> Result<(String, Summary)> {
    let uris: BTreeMap<&str, &str> = emitted
        .data
        .namespaces
        .iter()
        .map(|namespace| (namespace.prefix.as_str(), namespace.uri.as_str()))
        .collect();
    let mut namespace_ids: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, row) in emitted.namespaces.iter().enumerate() {
        namespace_ids.entry(row.uri.as_str()).or_insert(index);
    }
    let mut summary = Summary {
        by_kind: KINDS.iter().map(|(_, kind)| (*kind, 0)).collect(),
        ..Summary::default()
    };
    let mut classes: BTreeMap<&Name, usize> = BTreeMap::new();
    let mut rows = Vec::new();
    for (type_id, (name, type_row)) in emitted
        .data
        .types
        .keys()
        .zip(&emitted.tables.types)
        .enumerate()
    {
        let Some(element) = element_of(name, &uris) else {
            continue;
        };
        let Some((key, registered)) = by_element.get_key_value(&element) else {
            continue;
        };
        *classes.entry(key).or_default() += 1;
        let resolver = Resolver {
            attributes: type_row
                .attributes
                .iter()
                .map(|attribute| {
                    Name::new(
                        uris.get(attribute.prefix.as_str())
                            .copied()
                            .unwrap_or_default(),
                        &attribute.name,
                    )
                })
                .collect(),
            namespace_ids: &namespace_ids,
        };
        summary.types += 1;
        for (registration, source) in &registered.registrations {
            let (kind, row) = row_expr(type_id, registration, &resolver)?;
            rows.push(row);
            if let Some((_, count)) = summary.by_kind.get_mut(kind) {
                *count += 1;
            }
            match source {
                Source::Dump => summary.from_dump += 1,
                Source::Rule(_) => summary.from_rules += 1,
            }
        }
    }
    for (element, registered) in by_element {
        let types = classes.get(element).copied().unwrap_or_default();
        if types != registered.classes {
            bail!(
                "the dump has {} classes of {element}, the data {types} element types",
                registered.classes
            );
        }
    }
    summary.rows = rows.len();
    let mut text = format!(
        "{header}\n/// Every semantic constraint the Open XML SDK, version 3.5.1, registers, one row per constraint\n\
         /// on each element type, sorted by type id and in the SDK's order within a type.\n"
    );
    render::statement(
        "pub static SEMANTIC_CONSTRAINTS: &[Registration] = ",
        &Expr::Slice(rows),
        &mut text,
    );
    Ok((text, summary))
}

/// The element a type of the data names, by URI: `w:CT_Control/w:control`
/// is `control` in `WordprocessingML`.
fn element_of(type_name: &str, uris: &BTreeMap<&str, &str>) -> Option<Name> {
    let (_, element) = type_name.split_once('/')?;
    let (prefix, local) = element.split_once(':')?;
    Some(Name::new(uris.get(prefix)?, local))
}

/// Each element's registrations from the rules whose context it is, in the
/// rules' order, a constraint equal to an earlier one of the element left
/// out; each with its rule's index.
fn rule_registrations(data: &SdkData) -> Result<BTreeMap<Name, Vec<(Registration, usize)>>> {
    let uris: BTreeMap<String, String> = data
        .namespaces
        .iter()
        .map(|namespace| (namespace.prefix.clone(), namespace.uri.clone()))
        .collect();
    let names = Names { uris: &uris };
    let mut found: BTreeMap<Name, Vec<(Registration, usize)>> = BTreeMap::new();
    for (index, rule) in data.schematrons.iter().enumerate() {
        let text = |name: &str| {
            rule.get(name)
                .and_then(Value::as_str)
                .with_context(|| format!("the rule {index} has no `{name}`"))
        };
        let constraint = schematron::read(text("Test")?, &names)
            .with_context(|| format!("in the rule {index}"))?;
        let version = rule.get("Version").and_then(Value::as_str);
        let registration = Registration {
            applications: applications(text("App")?)?,
            constraint,
            versions: versions(version)?,
        };
        let element = names.element(text("Context")?)?;
        let registered = found.entry(element).or_default();
        if !registered
            .iter()
            .any(|(known, _)| known.constraint == registration.constraint)
        {
            registered.push((registration, index));
        }
    }
    Ok(found)
}

/// A list of texts as the table writes it.
fn texts(values: &[String]) -> Expr {
    Expr::Slice(values.iter().map(|value| render::string(value)).collect())
}

/// A pattern as the `regex` crate reads it: .NET's `\P{IsBasicLatin}`, only
/// ever inside a class in the data, becomes the range past ASCII.
///
/// # Errors
///
/// Another .NET block name.
fn rust_pattern(pattern: &str) -> Result<String> {
    let converted = pattern.replace("\\P{IsBasicLatin}", "\\x{80}-\\x{10FFFF}");
    if converted.contains("{Is") {
        bail!("the pattern {pattern} names a .NET block");
    }
    Ok(converted)
}

/// The targets of a registration from `version` (absent: Office 2007, every
/// target) on.
fn versions(version: Option<&str>) -> Result<Versions> {
    let found = match version.unwrap_or("Office2007") {
        "Office2007" => DataVersion::Office2007,
        "Office2010" => DataVersion::Office2010,
        "Office2013" => DataVersion::Office2013,
        "Office2016" => DataVersion::Office2016,
        "Office2019" => DataVersion::Office2019,
        "Office2021" => DataVersion::Office2021,
        "Microsoft365" => DataVersion::Microsoft365,
        other => bail!("unknown Office version {other}"),
    };
    Ok(Versions::since(found))
}
