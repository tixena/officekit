//! The validation tables: SDK 3.5.1's schema data converted into the rows of
//! `officekit::validation`'s `generated`.
//!
//! Every element, attribute, and enumeration value only Transitional allows is
//! marked, and every attribute whose Strict type is not its Transitional type
//! carries the Strict type's checks.

pub(crate) mod render;
mod strict;

use alloc::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use os_shim::{System, WriteMode};

use self::strict::StrictChecks;
use crate::generate_schema::compare::{self, Additions};
use crate::generate_schema::emit::namespaces;
use crate::generate_schema::inputs::Inputs;
use crate::generate_schema::model::{
    AttributeUse, Content, Derivation, Method, Name, Particle as SchemaParticle, ParticleKind,
    SchemaSet, TypeRef,
};
use crate::generate_schema::sdk_data::{
    DataAttribute, DataChildKind, DataPart, DataParticle, DataType, DataVersion, Enumeration,
    SdkData, Validator,
};

#[cfg(test)]
mod tests;

/// Where the module of the tables is written, from the workspace root; its
/// five files go into the folder of the same name.
pub const GENERATED_PATH: &str = "crates/officekit/src/validation/generated.rs";

/// The hand-maintained table of simple types, from the workspace root.
pub const SIMPLE_TYPES_PATH: &str = "crates/xtask/data/simple_types.toml";

/// The hand-maintained table of part roots, from the workspace root.
pub const PART_ROOTS_PATH: &str = "crates/xtask/data/part_roots.toml";

/// The hand-maintained table of corrections to SDK 3.5.1's data, from the
/// workspace root.
pub const SDK_CORRECTIONS_PATH: &str = "crates/xtask/data/sdk_corrections.toml";

/// The parts SDK 3.5.1 gives a root element and no typed root.
const NO_TYPED_ROOT: [&str; 2] = ["CoreFilePropertiesPart", "XmlSignaturePart"];

/// The targets, Office 2010 to Microsoft 365.
const TARGETS: u8 = 6;

/// The namespaces an `xsd:any` particle accepts.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AnyNamespace {
    Any,
    Local,
    Other,
    TargetNamespace,
    Uri(String),
}

/// An attribute of a type and the checks of its value.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct AttributeRow {
    pub checks: Vec<ValueCheck>,
    pub class: ValueClass,
    /// The local name.
    pub name: String,
    /// The namespace prefix; empty for an unprefixed attribute.
    pub prefix: String,
    /// The checks of its Strict type, when that is not its Transitional type.
    pub strict_checks: Option<Vec<ValueCheck>>,
    pub transitional_only: bool,
}

/// A number check's bounds, the data's whole numbers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Bounds {
    pub max_exclusive: Option<i64>,
    pub max_inclusive: Option<i64>,
    pub min_exclusive: Option<i64>,
    pub min_inclusive: Option<i64>,
    pub non_negative: bool,
    pub positive: bool,
}

/// One check of a value.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Check {
    Enum {
        class: ValueClass,
    },
    List {
        item: Box<Self>,
    },
    Number {
        bounds: Bounds,
        class: ValueClass,
        list: bool,
    },
    OfficeVersion,
    Required,
    String {
        /// The class of the validator's `Type`: `HexBinary` or none; a
        /// Strict check's names the class of its built-in type.
        class: Option<ValueClass>,
        kind: StringKind,
        length: Option<u64>,
        max_length: Option<u64>,
        min_length: Option<u64>,
        pattern: Option<String>,
    },
}

/// A child part a part type may relate to.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ChildPartRow {
    pub fixed_content: bool,
    pub kind: DataChildKind,
    pub many: bool,
    /// A part type's name, or a data part reference's.
    pub name: String,
    pub required: bool,
}

/// A child element a type declares.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ChildRow {
    pub local: String,
    pub prefix: String,
    pub transitional_only: bool,
    /// The child's type, `w:CT_TblWidth/w:top`.
    pub type_name: String,
}

/// How a composite particle combines its items.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CompositeKind {
    All,
    Choice,
    Group,
    Sequence,
}

/// An enumeration and its members.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct EnumerationRow {
    /// Each value, the targets it is a member at, and whether only Transitional allows it.
    pub members: Vec<(String, Versions, bool)>,
    pub simple_type: String,
}

/// What only Transitional declares, by Transitional complex type.
///
/// The child elements and attributes the type, its bases, and its groups
/// declare only in Transitional, each with the comparison items that make it
/// so; the enumeration values; the complex types and namespaces only
/// Transitional has; and the Strict checks of the attributes whose types
/// differ. With them, what finds the Transitional type of an SDK type the SDK
/// renamed: each type's child elements and attributes with their types.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Marks {
    /// By complex type: each attribute whose type is a simple type, and that type.
    attribute_types: BTreeMap<Name, BTreeMap<Name, Name>>,
    attributes: BTreeMap<Name, BTreeMap<Name, BTreeSet<String>>>,
    complex_types: BTreeSet<Name>,
    /// By complex type: each child element declared with one complex type, and that type.
    element_types: BTreeMap<Name, BTreeMap<Name, Name>>,
    elements: BTreeMap<Name, BTreeMap<Name, BTreeSet<String>>>,
    /// Every comparison item a mark can come from, described.
    items: BTreeSet<String>,
    simple_types: BTreeSet<Name>,
    strict_checks: StrictChecks,
    /// The namespace of every Transitional schema file with no Strict twin.
    transitional_only_namespaces: BTreeSet<String>,
    /// The complex types only Transitional declares.
    transitional_only_types: BTreeSet<Name>,
    values: BTreeSet<(Name, String)>,
}

/// A part type's rule.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct PartRow {
    pub children: Vec<ChildPartRow>,
    pub content_type: Option<String>,
    pub name: String,
    pub relationship_type: Option<String>,
    /// The root element's type, `w:CT_Document/w:document`.
    pub root: Option<String>,
    /// The targets the part type exists at.
    pub versions: Versions,
}

/// A content model at one target; `max` 0 is unbounded.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Particle {
    Any {
        max: u32,
        min: u32,
        namespace: AnyNamespace,
    },
    Composite {
        items: Vec<Self>,
        kind: CompositeKind,
        max: u32,
        min: u32,
    },
    Element {
        max: u32,
        min: u32,
        type_name: String,
    },
}

/// The lexical kind a string check requires.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum StringKind {
    Id,
    NcName,
    Plain,
    QName,
    Token,
    Uri,
}

/// What `emit` wrote, and the conversion the semantic table is built on.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Emitted {
    /// The data the validation tables were converted from: `data` with the
    /// table of corrections applied.
    pub corrected: SdkData,
    pub data: SdkData,
    /// The namespace table's rows; a row's index is its namespace id.
    pub namespaces: Vec<namespaces::Row>,
    pub summary: Summary,
    pub tables: Tables,
}

/// What was converted and written.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Summary {
    /// Attribute rows, a derived type's inherited ones counted again.
    pub attribute_rows: usize,
    /// Checks of attributes and of text.
    pub checks: usize,
    /// Content models over the six targets, equal ones merged.
    pub content_models: usize,
    pub marked_attributes: usize,
    pub marked_children: usize,
    pub marked_values: usize,
    /// Child-part entries of the part rules.
    pub part_children: usize,
    /// Child-part entries allowed once at most.
    pub part_once: usize,
    /// Child-part entries required.
    pub part_required: usize,
    pub parts: usize,
    /// Distinct patterns of the string checks.
    pub patterns: usize,
    /// Attribute rows that carry Strict checks.
    pub strict_checks: usize,
    /// The prefix and URI of each namespace only Transitional has.
    pub transitional_only_namespaces: Vec<(String, String)>,
    pub types: usize,
    /// The comparison items no row of the data carries.
    pub unmatched: Vec<String>,
}

/// Every table, in the data's own names.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Tables {
    /// Sorted by simple type.
    pub enumerations: Vec<EnumerationRow>,
    /// The comparison items some row carries a mark from.
    pub marked_items: BTreeSet<String>,
    /// Sorted by name.
    pub parts: Vec<PartRow>,
    /// The hand-maintained table, sorted by simple type.
    pub simple_types: Vec<(String, ValueClass)>,
    /// The prefix and URI of each namespace only Transitional has, sorted by prefix.
    pub transitional_only_namespaces: Vec<(String, String)>,
    /// Sorted by name; a type's id is its index.
    pub types: Vec<TypeRow>,
}

/// A type of the data and what it allows.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct TypeRow {
    pub attributes: Vec<AttributeRow>,
    /// Sorted by prefix and local name.
    pub children: Vec<ChildRow>,
    pub name: String,
    /// The content model at each target, equal ones merged.
    pub particles: Vec<(Versions, Option<Particle>)>,
    pub text_checks: Vec<ValueCheck>,
    /// The targets the element type exists at.
    pub versions: Versions,
}

/// A check, the targets it runs at, and the union it belongs to.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ValueCheck {
    pub check: Check,
    pub union: Option<u8>,
    pub versions: Versions,
}

/// The class a value reads as.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ValueClass {
    Base64Binary,
    Boolean,
    Byte,
    DateTime,
    Decimal,
    Double,
    /// By simple type, `w:ST_TblWidth`.
    Enum(String),
    HexBinary,
    Int16,
    Int32,
    Int64,
    Integer,
    List(Box<Self>),
    OnOff,
    SByte,
    Single,
    String,
    TrueFalse,
    TrueFalseBlank,
    UInt16,
    UInt32,
    UInt64,
}

/// The targets a row applies at: bit 0 is Office 2010, bit 5 Microsoft 365.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Versions(pub u8);

/// The conversion's state: lookups over the data, and the marks it used.
struct Converter<'data> {
    data: &'data SdkData,
    /// The Transitional simple type of each enumeration of the data that has one.
    ecma_simple_types: BTreeMap<&'data str, Name>,
    /// The Transitional complex type of each schema type of the data that has one.
    ecma_types: BTreeMap<&'data str, Name>,
    /// `(C# namespace, enumeration name)` to the enumeration's simple type.
    enumerations: BTreeMap<(&'data str, &'data str), &'data str>,
    marks: &'data Marks,
    /// The root type of each abstract part base the data names in `Base`.
    part_roots: &'data BTreeMap<String, String>,
    simple_types: &'data BTreeMap<String, String>,
    /// Prefix to namespace URI.
    uris: BTreeMap<&'data str, &'data str>,
    used: BTreeSet<String>,
}

/// Where a mark is looked up.
#[derive(Clone, Copy)]
enum Marked {
    Attribute,
    Element,
}

/// The walk of one Transitional complex type that finds its marks.
struct Walk<'set> {
    attribute_types: BTreeMap<Name, BTreeSet<Name>>,
    attributes: BTreeMap<Name, BTreeSet<String>>,
    element_types: BTreeMap<Name, BTreeSet<Name>>,
    elements: BTreeMap<Name, BTreeSet<String>>,
    only: &'set Additions,
    set: &'set SchemaSet,
}

impl<'data> Converter<'data> {
    fn attribute(
        &mut self,
        attribute: &DataAttribute,
        ecma: Option<&Name>,
    ) -> Result<AttributeRow> {
        let class = self.class(&attribute.type_name)?;
        let mut checks = attribute
            .validators
            .iter()
            .filter_map(|validator| self.check(validator, &class).transpose())
            .collect::<Result<Vec<ValueCheck>>>()?;
        if let Some(version) = attribute.version
            && rank(version).is_some()
            && !checks
                .iter()
                .any(|check| check.check == Check::OfficeVersion)
        {
            checks.insert(
                0,
                ValueCheck {
                    check: Check::OfficeVersion,
                    union: None,
                    versions: Versions::since(version),
                },
            );
        }
        checks.push(ValueCheck {
            check: class_check(&class),
            union: None,
            versions: Versions::ALL,
        });
        let (prefix, local) = attribute
            .qname
            .split_once(':')
            .with_context(|| format!("the attribute {} has no `:`", attribute.qname))?;
        let transitional_only = self.marked(Marked::Attribute, ecma, prefix, local)?;
        let strict_checks =
            match ecma.and_then(|definition| self.marks.strict_checks.get(definition)) {
                Some(by_name) => by_name
                    .get(&Name::new(self.namespace(prefix)?, local))
                    .cloned(),
                None => None,
            };
        Ok(AttributeRow {
            checks,
            class,
            name: local.to_owned(),
            prefix: prefix.to_owned(),
            strict_checks,
            transitional_only,
        })
    }

    /// One validator as a check; `None` for a `RequiredValidator` whose
    /// `IsRequired` is `False`, which requires nothing.
    fn check(&self, validator: &Validator, own: &ValueClass) -> Result<Option<ValueCheck>> {
        let typed = || {
            validator
                .type_name
                .as_deref()
                .map_or_else(|| Ok(own.clone()), |name| self.simple_type_class(name))
        };
        let (check, versions) = match validator.name.as_str() {
            "EnumValidator" => {
                known_arguments(validator, &[])?;
                (
                    Check::Enum { class: typed()? },
                    validator_versions(validator),
                )
            }
            "NumberValidator" => {
                known_arguments(
                    validator,
                    &[
                        "IsNonNegative",
                        "IsPositive",
                        "MaxExclusive",
                        "MaxInclusive",
                        "MinExclusive",
                        "MinInclusive",
                    ],
                )?;
                let check = Check::Number {
                    bounds: Bounds {
                        max_exclusive: number(validator, "MaxExclusive")?,
                        max_inclusive: number(validator, "MaxInclusive")?,
                        min_exclusive: number(validator, "MinExclusive")?,
                        min_inclusive: number(validator, "MinInclusive")?,
                        non_negative: flag(validator, "IsNonNegative")?,
                        positive: flag(validator, "IsPositive")?,
                    },
                    class: typed()?,
                    list: validator.is_list,
                };
                (check, validator_versions(validator))
            }
            "OfficeVersionValidator" => (Check::OfficeVersion, office_version(validator)?),
            "RequiredValidator" => {
                known_arguments(validator, &["IsRequired"])?;
                if !flag_default_true(validator, "IsRequired")? {
                    return Ok(None);
                }
                (Check::Required, validator_versions(validator))
            }
            "StringValidator" => {
                let class = validator
                    .type_name
                    .as_deref()
                    .map(|name| self.string_class(name))
                    .transpose()?;
                (
                    string_check(validator, class)?,
                    validator_versions(validator),
                )
            }
            other => bail!("unknown validator {other}"),
        };
        Ok(Some(ValueCheck {
            check,
            union: validator.union_id,
            versions,
        }))
    }

    /// `root` when the data has that type and its element's local name is `element`.
    fn checked_root(&self, root: &str, element: &str) -> Result<String> {
        if !self.data.types.contains_key(root) {
            bail!("the root type {root} names no type");
        }
        let local = root
            .split_once('/')
            .and_then(|(_, qualified)| qualified.split_once(':'))
            .map(|(_, local)| local);
        if local != Some(element) {
            bail!("the root type {root} is not of the element {element}");
        }
        Ok(root.to_owned())
    }

    /// The child elements of a type: those the data declares and those its
    /// content model names, sorted by prefix and local name.
    fn children(&mut self, data_type: &DataType, ecma: Option<&Name>) -> Result<Vec<ChildRow>> {
        let mut names: BTreeSet<&str> = data_type.children.iter().map(String::as_str).collect();
        if let Some(particle) = &data_type.particle {
            element_names(particle, &mut names);
        }
        let mut rows = Vec::with_capacity(names.len());
        for type_name in names {
            if !self.data.types.contains_key(type_name) {
                bail!("the child {type_name} names no type");
            }
            let (prefix, local) = type_name
                .split_once('/')
                .and_then(|(_, element)| element.split_once(':'))
                .with_context(|| format!("the child {type_name} names no prefixed element"))?;
            let transitional_only = self.marked(Marked::Element, ecma, prefix, local)?
                || self.transitional_only_type(type_name);
            rows.push(ChildRow {
                local: local.to_owned(),
                prefix: prefix.to_owned(),
                transitional_only,
                type_name: type_name.to_owned(),
            });
        }
        rows.sort_by(|left, right| (&left.prefix, &left.local).cmp(&(&right.prefix, &right.local)));
        if let Some((first, second)) = rows
            .iter()
            .zip(rows.iter().skip(1))
            .find(|(left, right)| left.prefix == right.prefix && left.local == right.local)
        {
            bail!(
                "the child {}:{} is both {} and {}",
                first.prefix,
                first.local,
                first.type_name,
                second.type_name,
            );
        }
        Ok(rows)
    }

    /// The value class of an attribute's `Type`, or of the hand-maintained table's.
    fn class(&self, type_name: &str) -> Result<ValueClass> {
        if let Some(item) = type_name
            .strip_prefix("ListValue<")
            .and_then(|rest| rest.strip_suffix('>'))
        {
            return Ok(ValueClass::List(Box::new(self.class(item)?)));
        }
        if let Some(enumeration) = type_name
            .strip_prefix("EnumValue<")
            .and_then(|rest| rest.strip_suffix('>'))
        {
            let simple_type = enumeration
                .rsplit_once('.')
                .and_then(|(namespace, name)| self.enumerations.get(&(namespace, name)))
                .with_context(|| format!("no enumeration is {enumeration}"))?;
            return Ok(ValueClass::Enum((*simple_type).to_owned()));
        }
        Ok(match type_name {
            "Base64BinaryValue" => ValueClass::Base64Binary,
            "BooleanValue" => ValueClass::Boolean,
            "ByteValue" => ValueClass::Byte,
            "DateTimeValue" => ValueClass::DateTime,
            "DecimalValue" => ValueClass::Decimal,
            "DoubleValue" => ValueClass::Double,
            "HexBinaryValue" => ValueClass::HexBinary,
            "Int16Value" => ValueClass::Int16,
            "Int32Value" => ValueClass::Int32,
            "Int64Value" => ValueClass::Int64,
            "IntegerValue" => ValueClass::Integer,
            "OnOffValue" => ValueClass::OnOff,
            "SByteValue" => ValueClass::SByte,
            "SingleValue" => ValueClass::Single,
            "StringValue" => ValueClass::String,
            "TrueFalseBlankValue" => ValueClass::TrueFalseBlank,
            "TrueFalseValue" => ValueClass::TrueFalse,
            "UInt16Value" => ValueClass::UInt16,
            "UInt32Value" => ValueClass::UInt32,
            "UInt64Value" => ValueClass::UInt64,
            _ => bail!("the type {type_name} names no value class"),
        })
    }

    fn enumeration(&mut self, simple_type: &str, enumeration: &Enumeration) -> EnumerationRow {
        let ecma = self.ecma_simple_types.get(simple_type).cloned();
        let members = enumeration
            .facets
            .iter()
            .map(|facet| {
                let transitional_only = ecma.as_ref().is_some_and(|name| {
                    self.marks
                        .values
                        .contains(&(name.clone(), facet.value.clone()))
                });
                if let (true, Some(name)) = (transitional_only, &ecma) {
                    self.used.insert(value_item(name, &facet.value));
                }
                let versions = facet.version.map_or(Versions::ALL, Versions::since);
                (facet.value.clone(), versions, transitional_only)
            })
            .collect();
        EnumerationRow {
            members,
            simple_type: simple_type.to_owned(),
        }
    }

    /// Whether `prefix:local` is marked in the Transitional type `ecma`; the
    /// comparison items behind the mark are counted as used.
    fn marked(
        &mut self,
        kind: Marked,
        ecma: Option<&Name>,
        prefix: &str,
        local: &str,
    ) -> Result<bool> {
        let Some(definition) = ecma else {
            return Ok(false);
        };
        let namespace = self.namespace(prefix)?;
        let marks = self.marks;
        let by_type = match kind {
            Marked::Attribute => &marks.attributes,
            Marked::Element => &marks.elements,
        };
        let Some(items) = by_type
            .get(definition)
            .and_then(|marked| marked.get(&Name::new(namespace, local)))
        else {
            return Ok(false);
        };
        self.used.extend(items.iter().cloned());
        Ok(true)
    }

    /// The namespace URI of a prefix of the data; `""` for none.
    fn namespace(&self, prefix: &str) -> Result<&'data str> {
        if prefix.is_empty() {
            return Ok("");
        }
        self.uris
            .get(prefix)
            .copied()
            .with_context(|| format!("no namespace has the prefix {prefix}"))
    }

    fn new(
        data: &'data SdkData,
        simple_types: &'data BTreeMap<String, String>,
        part_roots: &'data BTreeMap<String, String>,
        marks: &'data Marks,
    ) -> Result<Self> {
        let uris: BTreeMap<&str, &str> = data
            .namespaces
            .iter()
            .filter(|namespace| !namespace.uri.is_empty())
            .map(|namespace| (namespace.prefix.as_str(), namespace.uri.as_str()))
            .collect();
        let mut enumerations: BTreeMap<(&str, &str), &str> = BTreeMap::new();
        for (simple_type, enumeration) in &data.enumerations {
            let (prefix, _) = simple_type
                .split_once(':')
                .with_context(|| format!("the simple type {simple_type} has no prefix"))?;
            let Some(namespace) = data.typed_namespaces.get(prefix) else {
                continue;
            };
            let key = (namespace.as_str(), enumeration.name.as_str());
            if enumerations.insert(key, simple_type.as_str()).is_some() {
                bail!("two enumerations are {namespace}.{}", enumeration.name);
            }
        }
        let ecma_types = ecma_types(data, &uris, marks);
        let ecma_simple_types = ecma_simple_types(data, &uris, &enumerations, &ecma_types, marks);
        Ok(Self {
            data,
            ecma_simple_types,
            ecma_types,
            enumerations,
            marks,
            part_roots,
            simple_types,
            uris,
            used: BTreeSet::new(),
        })
    }

    /// A part's rule; a part with a root element and no `Root` takes the root
    /// of its base from the table of part roots.
    fn part(&self, part: &DataPart) -> Result<PartRow> {
        let root = match (&part.root, &part.root_element) {
            (Some(root), Some(element)) => Some(self.root_type(root, element)?),
            (Some(root), None) => bail!("the root {root} has no root element"),
            (None, Some(element)) => match part
                .base
                .as_deref()
                .and_then(|base| self.part_roots.get(base))
            {
                Some(root) => Some(self.checked_root(root, element)?),
                None if NO_TYPED_ROOT.contains(&part.name.as_str()) => None,
                None => bail!(
                    "the part {} has the root element {element} and no root type",
                    part.name
                ),
            },
            (None, None) => None,
        };
        Ok(PartRow {
            children: part
                .children
                .iter()
                .map(|child| ChildPartRow {
                    fixed_content: child.fixed_content,
                    kind: child.kind,
                    many: child.many,
                    name: child.name.clone(),
                    required: child.required,
                })
                .collect(),
            content_type: part.content_type.clone(),
            name: part.name.clone(),
            relationship_type: part.relationship_type.clone(),
            root,
            versions: part.version.map_or(Versions::ALL, Versions::since),
        })
    }

    /// The element type of a part's root: the element type of the schema type
    /// `root` whose local name is `element`, and when two are, the one in the
    /// schema type's namespace.
    fn root_type(&self, root: &str, element: &str) -> Result<String> {
        let start = format!("{root}/");
        let found: Vec<(&str, &String)> = self
            .data
            .types
            .range(start.clone()..)
            .map(|(name, _)| name)
            .take_while(|name| name.starts_with(&start))
            .filter_map(|name| {
                let (prefix, local) = name[start.len()..].split_once(':')?;
                (local == element).then_some((prefix, name))
            })
            .collect();
        let own = root.split_once(':').map_or("", |(prefix, _)| prefix);
        match found.as_slice() {
            [(_, one)] => Ok((*one).clone()),
            _ => found
                .iter()
                .find(|(prefix, _)| *prefix == own)
                .map(|(_, name)| (*name).clone())
                .with_context(|| {
                    format!(
                        "the root {root} has {} element types {element}",
                        found.len()
                    )
                }),
        }
    }

    fn simple_type_class(&self, simple_type: &str) -> Result<ValueClass> {
        if let Some(class) = self.simple_types.get(simple_type) {
            return self.class(class);
        }
        if self.data.enumerations.contains_key(simple_type) {
            return Ok(ValueClass::Enum(simple_type.to_owned()));
        }
        bail!(
            "the simple type {simple_type} is neither in the table of simple types nor an enumeration"
        )
    }

    /// The class of a string validator's `Type`: only `HexBinary`, whose
    /// lengths count bytes, has a rule here.
    fn string_class(&self, simple_type: &str) -> Result<ValueClass> {
        let class = self.simple_type_class(simple_type)?;
        if class != ValueClass::HexBinary {
            bail!(
                "the string validator's type {simple_type} reads as {class:?}, which has no rule \
                 on a string validator"
            );
        }
        Ok(class)
    }

    /// Whether the type the data names `type_name` is, in Transitional, a
    /// complex type only Transitional declares.
    fn transitional_only_type(&self, type_name: &str) -> bool {
        self.ecma_types
            .get(schema_type_of(type_name))
            .is_some_and(|ecma| self.marks.transitional_only_types.contains(ecma))
    }

    /// One type's row: a derived type takes the attributes of its schema type
    /// (`w:CT_TblWidthDxaNil/`) first, as SDK 3.5.1's classes inherit them.
    fn type_row(&mut self, name: &str, data_type: &DataType) -> Result<TypeRow> {
        let (schema_type, _) = name
            .split_once('/')
            .with_context(|| format!("the type {name} has no `/`"))?;
        let ecma = self.ecma_types.get(schema_type).cloned();
        let base_name = format!("{schema_type}/");
        let mut attributes = Vec::new();
        if data_type.is_derived
            && base_name != name
            && let Some(base) = self.data.types.get(&base_name)
        {
            for attribute in &base.attributes {
                attributes.push(self.attribute(attribute, ecma.as_ref())?);
            }
        }
        for attribute in &data_type.attributes {
            attributes.push(self.attribute(attribute, ecma.as_ref())?);
        }
        let text_checks = data_type
            .validators
            .iter()
            .filter_map(|validator| self.check(validator, &ValueClass::String).transpose())
            .collect::<Result<Vec<ValueCheck>>>()?;
        let children = self.children(data_type, ecma.as_ref())?;
        Ok(TypeRow {
            attributes,
            children,
            name: name.to_owned(),
            particles: particles(data_type.particle.as_ref())?,
            text_checks,
            versions: data_type.version.map_or(Versions::ALL, Versions::since),
        })
    }
}

impl Marks {
    /// The marks of every complex type of `transitional`, from `only`, what
    /// Transitional declares and Strict lacks.
    #[must_use]
    pub fn new(transitional: &SchemaSet, only: &Additions) -> Self {
        let mut marks = Self::default();
        for name in transitional.complex_types.keys() {
            let mut walk = Walk {
                attribute_types: BTreeMap::new(),
                attributes: BTreeMap::new(),
                element_types: BTreeMap::new(),
                elements: BTreeMap::new(),
                only,
                set: transitional,
            };
            walk.complex_type(name, true, &mut Vec::new());
            if !walk.attributes.is_empty() {
                marks.attributes.insert(name.clone(), walk.attributes);
            }
            if !walk.elements.is_empty() {
                marks.elements.insert(name.clone(), walk.elements);
            }
            marks
                .attribute_types
                .insert(name.clone(), singles(walk.attribute_types));
            marks
                .element_types
                .insert(name.clone(), singles(walk.element_types));
        }
        marks.complex_types = transitional.complex_types.keys().cloned().collect();
        marks.simple_types = transitional.simple_types.keys().cloned().collect();
        marks.items = only
            .elements
            .iter()
            .map(|(definition, element)| element_item(definition, element))
            .chain(
                only.attributes
                    .iter()
                    .map(|(definition, attribute)| attribute_item(definition, attribute)),
            )
            .chain(only.attribute_groups.iter().map(attribute_group_item))
            .chain(only.groups.iter().map(group_item))
            .chain(
                only.group_references
                    .iter()
                    .map(|(definition, group)| reference_item(definition, group)),
            )
            .chain(
                only.enumeration_values
                    .iter()
                    .map(|(simple_type, value)| value_item(simple_type, value)),
            )
            .collect();
        marks.values.clone_from(&only.enumeration_values);
        marks
            .transitional_only_types
            .clone_from(&only.complex_types);
        marks
    }

    /// The marks with what the Strict set gives them: the Strict checks of
    /// the attributes whose Strict type is not their Transitional type, and
    /// the namespaces of the Transitional files with no Strict file of their
    /// name.
    ///
    /// # Errors
    ///
    /// A Strict type built from a shape the conversion does not know, named.
    pub fn with_strict(mut self, transitional: &SchemaSet, strict: &SchemaSet) -> Result<Self> {
        self.strict_checks = strict::strict_checks(transitional, strict)?;
        let twins = compare::twins(transitional, strict);
        let twinned: BTreeSet<&String> = twins.values().collect();
        self.transitional_only_namespaces = transitional
            .namespaces
            .keys()
            .filter(|namespace| !twinned.contains(namespace))
            .cloned()
            .collect();
        Ok(self)
    }
}

impl Tables {
    /// The counts of the tables, and the items of `marks` no row carries.
    #[must_use]
    pub fn summary(&self, marks: &Marks) -> Summary {
        let attributes = self.types.iter().flat_map(|row| &row.attributes);
        let part_children = self.parts.iter().flat_map(|part| &part.children);
        let mut patterns = BTreeSet::new();
        for check in self.types.iter().flat_map(|row| {
            row.attributes
                .iter()
                .flat_map(|attribute| &attribute.checks)
                .chain(&row.text_checks)
        }) {
            if let Check::String {
                class: _class,
                kind: _kind,
                length: _length,
                max_length: _max_length,
                min_length: _min_length,
                pattern: Some(pattern),
            } = &check.check
            {
                patterns.insert(pattern.as_str());
            }
        }
        Summary {
            attribute_rows: attributes.clone().count(),
            checks: self
                .types
                .iter()
                .map(|row| {
                    row.attributes
                        .iter()
                        .map(|attribute| attribute.checks.len())
                        .sum::<usize>()
                        + row.text_checks.len()
                })
                .sum(),
            content_models: self
                .types
                .iter()
                .map(|row| {
                    row.particles
                        .iter()
                        .filter(|(_, model)| model.is_some())
                        .count()
                })
                .sum(),
            marked_attributes: attributes
                .filter(|attribute| attribute.transitional_only)
                .count(),
            marked_children: self
                .types
                .iter()
                .flat_map(|row| &row.children)
                .filter(|child| child.transitional_only)
                .count(),
            marked_values: self
                .enumerations
                .iter()
                .flat_map(|enumeration| &enumeration.members)
                .filter(|(_, _, transitional_only)| *transitional_only)
                .count(),
            part_children: part_children.clone().count(),
            part_once: part_children.clone().filter(|child| !child.many).count(),
            part_required: part_children.filter(|child| child.required).count(),
            parts: self.parts.len(),
            patterns: patterns.len(),
            strict_checks: self
                .types
                .iter()
                .flat_map(|row| &row.attributes)
                .filter(|attribute| attribute.strict_checks.is_some())
                .count(),
            transitional_only_namespaces: self.transitional_only_namespaces.clone(),
            types: self.types.len(),
            unmatched: marks
                .items
                .difference(&self.marked_items)
                .cloned()
                .collect(),
        }
    }
}

impl Versions {
    pub const ALL: Self = Self(0b11_1111);
    pub const NONE: Self = Self(0);

    /// `version` alone; Office 2007, below every target, is no target.
    #[must_use]
    pub const fn only(version: DataVersion) -> Self {
        match rank(version) {
            Some(target) => Self(1 << target),
            None => Self::NONE,
        }
    }

    /// `version` and every later target; Office 2007 gives every target.
    #[must_use]
    pub const fn since(version: DataVersion) -> Self {
        match rank(version) {
            Some(target) => Self(Self::ALL.0 & !((1 << target) - 1)),
            None => Self::ALL,
        }
    }
}

impl Walk<'_> {
    fn attribute(&mut self, definition: &Name, attribute: &AttributeUse) {
        if let TypeRef::Simple {
            anonymous: _anonymous,
            name: simple_type,
        } = &attribute.type_ref
        {
            self.attribute_types
                .entry(attribute.name.clone())
                .or_default()
                .insert(simple_type.clone());
        }
        let mut reasons = BTreeSet::new();
        match &attribute.group {
            None => {
                if self
                    .only
                    .attributes
                    .contains(&(definition.clone(), attribute.name.clone()))
                {
                    reasons.insert(attribute_item(definition, &attribute.name));
                }
            }
            Some(group) => {
                if self
                    .only
                    .group_references
                    .contains(&(definition.clone(), group.clone()))
                {
                    reasons.insert(reference_item(definition, group));
                }
                self.group_attribute(group, &attribute.name, &mut reasons, &mut Vec::new());
            }
        }
        if !reasons.is_empty() {
            self.attributes
                .entry(attribute.name.clone())
                .or_default()
                .extend(reasons);
        }
    }

    /// A complex type's own content and attributes, then its base's: the
    /// whole base for an extension, its attributes for a restriction.
    fn complex_type(&mut self, name: &Name, content: bool, seen: &mut Vec<Name>) {
        if seen.contains(name) {
            return;
        }
        seen.push(name.clone());
        let set = self.set;
        let Some(complex_type) = set.complex_types.get(name) else {
            return;
        };
        if let (true, Content::Particle(particle)) = (content, &complex_type.content) {
            self.particle(name, particle, &BTreeSet::new(), &mut Vec::new());
        }
        for attribute in &complex_type.attributes {
            self.attribute(name, attribute);
        }
        if let Some(Derivation {
            base:
                TypeRef::Complex {
                    anonymous: _anonymous,
                    name: base,
                },
            method,
        }) = &complex_type.base
        {
            let extension = content && *method == Method::Extension;
            self.complex_type(base, extension, seen);
        }
    }

    /// An attribute that an attribute group brought in: marked when the group
    /// is Transitional-only, or declares it, or brings it in through a
    /// reference, only in Transitional.
    fn group_attribute(
        &self,
        group: &Name,
        attribute: &Name,
        reasons: &mut BTreeSet<String>,
        seen: &mut Vec<Name>,
    ) {
        if seen.contains(group) {
            return;
        }
        seen.push(group.clone());
        if self.only.attribute_groups.contains(group) {
            reasons.insert(attribute_group_item(group));
        }
        let Some(inner) = self.set.attribute_groups.get(group).and_then(|declared| {
            declared
                .attributes
                .iter()
                .find(|used| used.name == *attribute)
        }) else {
            return;
        };
        match &inner.group {
            None => {
                if self
                    .only
                    .attributes
                    .contains(&(group.clone(), attribute.clone()))
                {
                    reasons.insert(attribute_item(group, attribute));
                }
            }
            Some(nested) => {
                if self
                    .only
                    .group_references
                    .contains(&(group.clone(), nested.clone()))
                {
                    reasons.insert(reference_item(group, nested));
                }
                self.group_attribute(nested, attribute, reasons, seen);
            }
        }
    }

    /// The elements of a content model, with the reasons inherited from the
    /// group references that led to it.
    fn particle(
        &mut self,
        definition: &Name,
        particle: &SchemaParticle,
        inherited: &BTreeSet<String>,
        groups: &mut Vec<Name>,
    ) {
        match &particle.kind {
            ParticleKind::All(items)
            | ParticleKind::Choice(items)
            | ParticleKind::Sequence(items) => {
                for item in items {
                    self.particle(definition, item, inherited, groups);
                }
            }
            ParticleKind::Any {
                namespace: _namespace,
                process_contents: _process_contents,
            } => {}
            ParticleKind::Element { name, type_ref } => {
                if let TypeRef::Complex {
                    anonymous: _anonymous,
                    name: complex_type,
                } = type_ref
                {
                    self.element_types
                        .entry(name.clone())
                        .or_default()
                        .insert(complex_type.clone());
                }
                let mut reasons = inherited.clone();
                if self
                    .only
                    .elements
                    .contains(&(definition.clone(), name.clone()))
                {
                    reasons.insert(element_item(definition, name));
                }
                if !reasons.is_empty() {
                    self.elements
                        .entry(name.clone())
                        .or_default()
                        .extend(reasons);
                }
            }
            ParticleKind::GroupRef(group) => {
                if groups.contains(group) {
                    return;
                }
                let mut reasons = inherited.clone();
                if self
                    .only
                    .group_references
                    .contains(&(definition.clone(), group.clone()))
                {
                    reasons.insert(reference_item(definition, group));
                }
                if self.only.groups.contains(group) {
                    reasons.insert(group_item(group));
                }
                let set = self.set;
                if let Some(declared) = set.groups.get(group) {
                    groups.push(group.clone());
                    self.particle(group, &declared.particle, &reasons, groups);
                    groups.pop();
                }
            }
        }
    }
}

fn attribute_group_item(group: &Name) -> String {
    format!("attribute group {group}")
}

fn attribute_item(definition: &Name, attribute: &Name) -> String {
    format!("attribute {attribute} in {definition}")
}

/// A particle at one target, as SDK 3.5.1 builds it: each `Occurs` entry an
/// alternative with its version (the entry's with `IncludeVersion`, else the
/// particle's `InitialVersion`), left out below it; among a composite's items,
/// a later alternative for the same element type, or for any group when the
/// composite has `RequireFilter`, replaces an earlier one of a lower version.
fn build(particle: &DataParticle, target: u8) -> Result<Vec<(Particle, Option<u8>)>> {
    let alternatives: Vec<(u32, u32, Option<DataVersion>)> = if particle.occurs.is_empty() {
        vec![(1, 1, particle.initial_version)]
    } else {
        particle
            .occurs
            .iter()
            .map(|occurs| {
                let version = if occurs.include_version {
                    occurs.version
                } else {
                    particle.initial_version
                };
                (occurs.min, occurs.max, version)
            })
            .collect()
    };
    let mut built = Vec::with_capacity(alternatives.len());
    for (min, max, version) in alternatives {
        let from = version.and_then(rank);
        if from.is_some_and(|first| first > target) {
            continue;
        }
        let row = match particle.kind.as_deref() {
            None => Particle::Element {
                max,
                min,
                type_name: particle
                    .name
                    .clone()
                    .context("an element particle has no name")?,
            },
            Some("Any") => Particle::Any {
                max,
                min,
                namespace: any_namespace(particle.namespace.as_deref()),
            },
            Some(kind) => Particle::Composite {
                items: composite_items(particle, target)?,
                kind: composite_kind(kind)?,
                max,
                min,
            },
        };
        built.push((row, from));
    }
    Ok(built)
}

fn any_namespace(namespace: Option<&str>) -> AnyNamespace {
    match namespace {
        None | Some("##any") => AnyNamespace::Any,
        Some("##local") => AnyNamespace::Local,
        Some("##other") => AnyNamespace::Other,
        Some("##targetNamespace") => AnyNamespace::TargetNamespace,
        Some(uri) => AnyNamespace::Uri(uri.to_owned()),
    }
}

/// An optional argument of a validator, by name.
fn argument<'data>(validator: &'data Validator, name: &str) -> Option<&'data str> {
    validator
        .arguments
        .iter()
        .find(|(_, argument, _)| argument.as_deref() == Some(name))
        .map(|(_, _, value)| value.as_str())
}

/// The check SDK 3.5.1 adds to every attribute by its class: a plain string
/// check for `String`, an enumeration check for `Enum`, `OnOff`, and
/// `TrueFalseBlank`, an item check for `List`, a number check for the rest.
fn class_check(class: &ValueClass) -> Check {
    match class {
        ValueClass::String => Check::String {
            class: None,
            kind: StringKind::Plain,
            length: None,
            max_length: None,
            min_length: None,
            pattern: None,
        },
        ValueClass::Enum(_) | ValueClass::OnOff | ValueClass::TrueFalseBlank => Check::Enum {
            class: class.clone(),
        },
        ValueClass::List(item) => Check::List {
            item: Box::new(class_check(item)),
        },
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
        | ValueClass::SByte
        | ValueClass::Single
        | ValueClass::TrueFalse
        | ValueClass::UInt16
        | ValueClass::UInt32
        | ValueClass::UInt64 => Check::Number {
            bounds: Bounds::default(),
            class: class.clone(),
            list: false,
        },
    }
}

/// A composite's items at one target, later alternatives replacing earlier ones.
fn composite_items(particle: &DataParticle, target: u8) -> Result<Vec<Particle>> {
    let mut items: Vec<(Particle, Option<u8>)> = Vec::new();
    for item in &particle.items {
        for (child, from) in build(item, target)? {
            items.retain(|(other, other_from)| {
                !(same(other, &child, particle.require_filter) && from > *other_from)
            });
            items.push((child, from));
        }
    }
    Ok(items.into_iter().map(|(item, _)| item).collect())
}

fn composite_kind(kind: &str) -> Result<CompositeKind> {
    Ok(match kind {
        "All" => CompositeKind::All,
        "Choice" => CompositeKind::Choice,
        "Group" => CompositeKind::Group,
        "Sequence" => CompositeKind::Sequence,
        _ => bail!("unknown particle kind {kind}"),
    })
}

/// Convert the data into the tables, by the mapping of blueprint section 3.6.
///
/// # Errors
///
/// An attribute type with no value class, a validator or argument the
/// conversion does not know, a simple type neither in `simple_types` nor an
/// enumeration, a string validator whose `Type` is not a `HexBinaryValue`, a
/// child, root, or particle that names no type, a part with a root element
/// whose root type neither the data nor `part_roots` gives, or a namespace
/// only Transitional has that no prefix of the data names.
pub fn convert(
    data: &SdkData,
    simple_types: &BTreeMap<String, String>,
    part_roots: &BTreeMap<String, String>,
    marks: &Marks,
) -> Result<Tables> {
    let mut converter = Converter::new(data, simple_types, part_roots, marks)?;
    let types = data
        .types
        .iter()
        .map(|(name, data_type)| {
            converter
                .type_row(name, data_type)
                .with_context(|| format!("in the type {name}"))
        })
        .collect::<Result<Vec<TypeRow>>>()?;
    let enumerations = data
        .enumerations
        .iter()
        .map(|(simple_type, enumeration)| converter.enumeration(simple_type, enumeration))
        .collect();
    let parts = data
        .parts
        .iter()
        .map(|part| {
            converter
                .part(part)
                .with_context(|| format!("in the part {}", part.name))
        })
        .collect::<Result<Vec<PartRow>>>()?;
    let classes = simple_types
        .iter()
        .map(|(simple_type, class)| {
            Ok((
                simple_type.clone(),
                converter
                    .class(class)
                    .with_context(|| format!("in the simple type {simple_type}"))?,
            ))
        })
        .collect::<Result<Vec<(String, ValueClass)>>>()?;
    let mut transitional_only_namespaces = marks
        .transitional_only_namespaces
        .iter()
        .map(|namespace| {
            let prefix = converter
                .uris
                .iter()
                .find(|(_, uri)| **uri == namespace.as_str())
                .map(|(prefix, _)| (*prefix).to_owned())
                .with_context(|| format!("no prefix of the data is the namespace {namespace}"))?;
            Ok((prefix, namespace.clone()))
        })
        .collect::<Result<Vec<(String, String)>>>()?;
    transitional_only_namespaces.sort();
    Ok(Tables {
        enumerations,
        marked_items: converter.used,
        parts,
        simple_types: classes,
        transitional_only_namespaces,
        types,
    })
}

/// The Transitional simple type of each enumeration of the data: the one of
/// its name, else the one the Transitional type of an attribute of its class
/// declares, when every such attribute agrees.
fn ecma_simple_types<'data>(
    data: &'data SdkData,
    uris: &BTreeMap<&str, &str>,
    enumerations: &BTreeMap<(&str, &str), &'data str>,
    ecma_types: &BTreeMap<&str, Name>,
    marks: &Marks,
) -> BTreeMap<&'data str, Name> {
    let mut found: BTreeMap<&'data str, Name> = BTreeMap::new();
    for simple_type in data.enumerations.keys() {
        if let Some(ecma) = qualified_name(uris, simple_type)
            && marks.simple_types.contains(&ecma)
        {
            found.insert(simple_type, ecma);
        }
    }
    let mut candidates: BTreeMap<&'data str, BTreeSet<Name>> = BTreeMap::new();
    for (name, data_type) in &data.types {
        let schema_type = schema_type_of(name);
        let Some(declared) = ecma_types
            .get(schema_type)
            .and_then(|ecma| marks.attribute_types.get(ecma))
        else {
            continue;
        };
        let base = data
            .types
            .get(&format!("{schema_type}/"))
            .filter(|_| data_type.is_derived)
            .map(|base_type| &base_type.attributes);
        for attribute in base.into_iter().flatten().chain(&data_type.attributes) {
            let Some(simple_type) = enumeration_of(enumerations, &attribute.type_name) else {
                continue;
            };
            if found.contains_key(simple_type) {
                continue;
            }
            let Some(ecma) = attribute.qname.split_once(':').and_then(|(prefix, local)| {
                let namespace = if prefix.is_empty() {
                    Some("")
                } else {
                    uris.get(prefix).copied()
                };
                declared.get(&Name::new(namespace?, local))
            }) else {
                continue;
            };
            candidates
                .entry(simple_type)
                .or_default()
                .insert(ecma.clone());
        }
    }
    found.extend(singles(candidates));
    found
}

/// The Transitional complex type of each schema type of the data: the one of
/// its name, else the one its element is declared with in the Transitional
/// type of a parent, when every parent agrees; parents found so are used in
/// turn until no more are found.
fn ecma_types<'data>(
    data: &'data SdkData,
    uris: &BTreeMap<&str, &str>,
    marks: &Marks,
) -> BTreeMap<&'data str, Name> {
    let mut found: BTreeMap<&'data str, Name> = BTreeMap::new();
    for name in data.types.keys() {
        let schema_type = schema_type_of(name);
        if let Some(ecma) = qualified_name(uris, schema_type)
            && marks.complex_types.contains(&ecma)
        {
            found.insert(schema_type, ecma);
        }
    }
    loop {
        let mut candidates: BTreeMap<&'data str, BTreeSet<Name>> = BTreeMap::new();
        for (name, data_type) in &data.types {
            let Some(declared) = found
                .get(schema_type_of(name))
                .and_then(|parent| marks.element_types.get(parent))
            else {
                continue;
            };
            let mut children: BTreeSet<&'data str> =
                data_type.children.iter().map(String::as_str).collect();
            if let Some(particle) = &data_type.particle {
                element_names(particle, &mut children);
            }
            for child in children {
                let child_type = schema_type_of(child);
                if found.contains_key(child_type) {
                    continue;
                }
                if let Some(ecma) = child
                    .split_once('/')
                    .and_then(|(_, element)| qualified_name(uris, element))
                    .and_then(|element| declared.get(&element))
                {
                    candidates
                        .entry(child_type)
                        .or_default()
                        .insert(ecma.clone());
                }
            }
        }
        let before = found.len();
        found.extend(singles(candidates));
        if found.len() == before {
            return found;
        }
    }
}

fn element_item(definition: &Name, element: &Name) -> String {
    format!("element {element} in {definition}")
}

/// Every element type a content model names.
fn element_names<'data>(particle: &'data DataParticle, names: &mut BTreeSet<&'data str>) {
    if let (None, Some(name)) = (&particle.kind, &particle.name) {
        names.insert(name);
    }
    for item in &particle.items {
        element_names(item, names);
    }
}

/// The simple type of an attribute's `EnumValue` class, or of its list's items.
fn enumeration_of<'data>(
    enumerations: &BTreeMap<(&str, &str), &'data str>,
    type_name: &str,
) -> Option<&'data str> {
    let item = type_name
        .strip_prefix("ListValue<")
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap_or(type_name);
    let (namespace, name) = item
        .strip_prefix("EnumValue<")
        .and_then(|rest| rest.strip_suffix('>'))?
        .rsplit_once('.')?;
    enumerations.get(&(namespace, name)).copied()
}

/// Read the inputs, convert the data, and write the module of the tables to
/// `out` and its five files to the folder of the same name; the conversion
/// comes back for the semantic table.
///
/// # Errors
///
/// An input that cannot be read, data the conversion refuses, a table past
/// the range of its ids, or an output that cannot be written.
pub fn emit<S>(
    system: &S,
    inputs: &Inputs,
    simple_types_path: &Path,
    part_roots_path: &Path,
    sdk_corrections_path: &Path,
    out: &Path,
) -> Result<Emitted>
where
    S: System + ?Sized,
{
    let data = SdkData::read(system, &inputs.sdk_data)?;
    let sdk_corrections_text = system
        .read_to_string(sdk_corrections_path)
        .with_context(|| format!("cannot read {}", sdk_corrections_path.display()))?;
    let transitional = SchemaSet::read(system, &inputs.transitional_schemas)?;
    let mut corrected = data.clone();
    corrected.correct(&sdk_corrections_text, &transitional)?;
    let strict = SchemaSet::read(system, &inputs.strict_schemas)?;
    let open_specifications: Vec<&PathBuf> = inputs
        .open_specifications
        .iter()
        .map(|(_, path)| path)
        .collect();
    let open = SchemaSet::read(system, &open_specifications)?;
    let namespace_rows = namespaces::rows(&data, &transitional, &strict, &open)?;
    let marks = Marks::new(
        &transitional,
        &compare::transitional_only(&transitional, &strict),
    )
    .with_strict(&transitional, &strict)?;
    let simple_types_text = system
        .read_to_string(simple_types_path)
        .with_context(|| format!("cannot read {}", simple_types_path.display()))?;
    let part_roots_text = system
        .read_to_string(part_roots_path)
        .with_context(|| format!("cannot read {}", part_roots_path.display()))?;
    let simple_types = simple_types(&simple_types_text)?;
    let part_roots = part_roots(&part_roots_text)?;
    let tables = convert(&corrected, &simple_types, &part_roots, &marks)?;

    let mut read: Vec<PathBuf> = inputs.strict_schemas.clone();
    read.extend(inputs.transitional_schemas.iter().cloned());
    read.extend(open_specifications.into_iter().cloned());
    read.push(inputs.sdk_data.join("namespaces.json"));
    read.push(inputs.sdk_data.join("typed").join("namespaces.json"));
    read.extend(json_files(system, &inputs.sdk_data.join("schemas"))?);
    read.extend(json_files(system, &inputs.sdk_data.join("parts"))?);
    let header = namespaces::header(
        inputs.checksums(),
        inputs.tools(),
        &read,
        &[
            (part_roots_path, part_roots_text.as_bytes()),
            (sdk_corrections_path, sdk_corrections_text.as_bytes()),
            (simple_types_path, simple_types_text.as_bytes()),
        ],
    )?;
    let prefixes: Vec<&str> = namespace_rows
        .iter()
        .map(|row| row.prefix.as_str())
        .collect();

    let folder = out.with_extension("");
    system
        .create_dir_all(&folder)
        .with_context(|| format!("cannot create {}", folder.display()))?;
    for (name, text) in render::files(&header, &tables, &prefixes)? {
        let path = if name == render::MODULE {
            out.to_path_buf()
        } else {
            folder.join(name)
        };
        system
            .write(&path, text.as_bytes(), WriteMode::Replace)
            .with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(Emitted {
        corrected,
        data,
        namespaces: namespace_rows,
        summary: tables.summary(&marks),
        tables,
    })
}

/// A validator's flag, `"True"` or `"False"`; absent is false.
fn flag(validator: &Validator, name: &str) -> Result<bool> {
    match argument(validator, name) {
        None | Some("False" | "false") => Ok(false),
        Some("True" | "true") => Ok(true),
        Some(other) => bail!("the argument {name} of {} is {other}", validator.name),
    }
}

/// A validator's flag, `"True"` or `"False"`; absent is true.
fn flag_default_true(validator: &Validator, name: &str) -> Result<bool> {
    match argument(validator, name) {
        None | Some("True" | "true") => Ok(true),
        Some("False" | "false") => Ok(false),
        Some(other) => bail!("the argument {name} of {} is {other}", validator.name),
    }
}

fn group_item(group: &Name) -> String {
    format!("group {group}")
}

/// The `.json` files of a folder, sorted.
fn json_files<S>(system: &S, folder: &Path) -> Result<Vec<PathBuf>>
where
    S: System + ?Sized,
{
    let mut found: Vec<PathBuf> = system
        .read_dir(folder)
        .with_context(|| format!("cannot read {}", folder.display()))?
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    found.sort();
    Ok(found)
}

/// Refuse an argument the conversion does not read.
fn known_arguments(validator: &Validator, known: &[&str]) -> Result<()> {
    for (_, argument, _) in &validator.arguments {
        let name = argument.as_deref().unwrap_or_default();
        if !known.contains(&name) {
            bail!(
                "{} has an argument the conversion does not know: {name:?}",
                validator.name
            );
        }
    }
    Ok(())
}

/// A bound of a number validator, a whole number.
fn number(validator: &Validator, name: &str) -> Result<Option<i64>> {
    argument(validator, name)
        .map(|value| {
            value
                .parse()
                .with_context(|| format!("the argument {name} is not a whole number: {value}"))
        })
        .transpose()
}

/// The versions of an `OfficeVersionValidator`: its argument's version and every later one.
fn office_version(validator: &Validator) -> Result<Versions> {
    let [(kind, None, value)] = validator.arguments.as_slice() else {
        bail!("an OfficeVersionValidator does not have one unnamed argument");
    };
    if kind != "Version" {
        bail!("the argument of an OfficeVersionValidator is a {kind}");
    }
    let version = match value.as_str() {
        "Microsoft365" => DataVersion::Microsoft365,
        "Office2007" => DataVersion::Office2007,
        "Office2010" => DataVersion::Office2010,
        "Office2013" => DataVersion::Office2013,
        "Office2016" => DataVersion::Office2016,
        "Office2019" => DataVersion::Office2019,
        "Office2021" => DataVersion::Office2021,
        _ => bail!("unknown Office version {value}"),
    };
    Ok(Versions::since(version))
}

/// Read the hand-maintained table of part roots: each abstract part base the
/// data names in `Base`, and the root type its parts take.
///
/// # Errors
///
/// Text that is not TOML, or a value that is not a string.
pub fn part_roots(text: &str) -> Result<BTreeMap<String, String>> {
    let table: toml::Table = text
        .parse()
        .context("the table of part roots is not TOML")?;
    table
        .into_iter()
        .map(|(base, value)| {
            let root = value
                .as_str()
                .with_context(|| format!("the part base {base} maps to no root type"))?
                .to_owned();
            Ok((base, root))
        })
        .collect()
}

/// The content model of a type at each target, equal ones merged.
fn particles(particle: Option<&DataParticle>) -> Result<Vec<(Versions, Option<Particle>)>> {
    let mut merged: Vec<(Versions, Option<Particle>)> = Vec::new();
    for target in 0..TARGETS {
        let model = match particle {
            Some(content) => build(content, target)?
                .into_iter()
                .next()
                .map(|(row, _)| row),
            None => None,
        };
        let bit = 1 << target;
        if let Some((versions, _)) = merged.iter_mut().find(|(_, other)| *other == model) {
            versions.0 |= bit;
        } else {
            merged.push((Versions(bit), model));
        }
    }
    Ok(merged)
}

/// `prefix:local` with the prefix's namespace URI.
fn qualified_name(uris: &BTreeMap<&str, &str>, qualified: &str) -> Option<Name> {
    let (prefix, local) = qualified.split_once(':')?;
    let uri = uris.get(prefix)?;
    Some(Name::new(uri, local))
}

/// The target of a version, Office 2010 being 0; `None` below every target.
const fn rank(version: DataVersion) -> Option<u8> {
    match version {
        DataVersion::Office2007 => None,
        DataVersion::Office2010 => Some(0),
        DataVersion::Office2013 => Some(1),
        DataVersion::Office2016 => Some(2),
        DataVersion::Office2019 => Some(3),
        DataVersion::Office2021 => Some(4),
        DataVersion::Microsoft365 => Some(5),
    }
}

fn reference_item(definition: &Name, group: &Name) -> String {
    format!("reference to {group} in {definition}")
}

/// Whether a later alternative replaces an earlier one: the same element
/// type, or two groups when the composite has `RequireFilter`.
fn same(earlier: &Particle, later: &Particle, require_filter: bool) -> bool {
    match (earlier, later) {
        (
            Particle::Element {
                max: _earlier_max,
                min: _earlier_min,
                type_name: first,
            },
            Particle::Element {
                max: _later_max,
                min: _later_min,
                type_name: second,
            },
        ) => first == second,
        (
            Particle::Composite {
                items: _earlier_items,
                kind: CompositeKind::Group,
                max: _earlier_max,
                min: _earlier_min,
            },
            Particle::Composite {
                items: _later_items,
                kind: CompositeKind::Group,
                max: _later_max,
                min: _later_min,
            },
        ) => require_filter,
        _ => false,
    }
}

/// The schema type a type of the data names, `w:CT_TblWidth` of `w:CT_TblWidth/w:top`.
fn schema_type_of(name: &str) -> &str {
    name.split_once('/')
        .map_or(name, |(schema_type, _)| schema_type)
}

/// Read the hand-maintained table: each simple type and the class SDK 3.5.1's
/// generator reads it as, in the data's spelling of an attribute's `Type`.
///
/// # Errors
///
/// Text that is not TOML, or a value that is not a string.
pub fn simple_types(text: &str) -> Result<BTreeMap<String, String>> {
    let table: toml::Table = text
        .parse()
        .context("the table of simple types is not TOML")?;
    table
        .into_iter()
        .map(|(simple_type, value)| {
            let class = value
                .as_str()
                .with_context(|| format!("the simple type {simple_type} maps to no class name"))?
                .to_owned();
            Ok((simple_type, class))
        })
        .collect()
}

/// Each key whose set holds one value, with that value.
fn singles<K>(found: BTreeMap<K, BTreeSet<Name>>) -> BTreeMap<K, Name>
where
    K: Ord,
{
    found
        .into_iter()
        .filter_map(|(key, names)| {
            let mut values = names.into_iter();
            match (values.next(), values.next()) {
                (Some(only), None) => Some((key, only)),
                _ => None,
            }
        })
        .collect()
}

/// A string validator's check: the class of its `Type`, the most specific
/// kind its flags name (an ID is an `NCName`, an `NCName` a token), and its
/// facets.
fn string_check(validator: &Validator, class: Option<ValueClass>) -> Result<Check> {
    known_arguments(
        validator,
        &[
            "IsId",
            "IsNcName",
            "IsQName",
            "IsToken",
            "IsUri",
            "Length",
            "MaxLength",
            "MinLength",
            "Pattern",
        ],
    )?;
    let kind = match (
        flag(validator, "IsId")?,
        flag(validator, "IsNcName")?,
        flag(validator, "IsQName")?,
        flag(validator, "IsToken")?,
        flag(validator, "IsUri")?,
    ) {
        (false, false, false, false, false) => StringKind::Plain,
        (true, _, false, _, false) => StringKind::Id,
        (false, true, false, _, false) => StringKind::NcName,
        (false, false, true, false, false) => StringKind::QName,
        (false, false, false, true, false) => StringKind::Token,
        (false, false, false, false, true) => StringKind::Uri,
        _ => bail!("a string validator's flags name no one kind"),
    };
    let length = |name: &str| -> Result<Option<u64>> {
        argument(validator, name)
            .map(|value| {
                value
                    .parse()
                    .with_context(|| format!("the argument {name} is not a length: {value}"))
            })
            .transpose()
    };
    Ok(Check::String {
        class,
        kind,
        length: length("Length")?,
        max_length: length("MaxLength")?,
        min_length: length("MinLength")?,
        pattern: argument(validator, "Pattern").map(str::to_owned),
    })
}

/// The targets a validator runs at: its `Version` alone, with
/// `IsInitialVersion` that target and every later one, and without a
/// `Version` every target.
const fn validator_versions(validator: &Validator) -> Versions {
    match validator.version {
        None => Versions::ALL,
        Some(version) if validator.is_initial_version => Versions::since(version),
        Some(version) => Versions::only(version),
    }
}

fn value_item(simple_type: &Name, value: &str) -> String {
    format!("value {value} of {simple_type}")
}
