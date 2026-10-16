//! The comparison of the Transitional and Strict schema sets.
//!
//! Each Strict namespace is matched to its Transitional twin by the file name
//! both sets share, and each complex type, group, attribute group, and simple
//! type is compared by what its own definition declares.

use alloc::collections::{BTreeMap, BTreeSet};

use super::model::{AttributeUse, Content, Name, Particle, ParticleKind, SchemaSet, SimpleType};

#[cfg(test)]
mod tests;

/// What one class's schemas declare and the other's lack. Every name is in
/// the Transitional namespaces; pairs are (definition, what it declares).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Additions {
    /// Whole attribute groups.
    pub attribute_groups: BTreeSet<Name>,
    /// Attributes inside definitions both sets have.
    pub attributes: BTreeSet<(Name, Name)>,
    /// Whole complex types.
    pub complex_types: BTreeSet<Name>,
    /// Child elements inside definitions both sets have.
    pub elements: BTreeSet<(Name, Name)>,
    /// Enumeration values inside simple types both sets have.
    pub enumeration_values: BTreeSet<(Name, String)>,
    /// `xsd:group` and `xsd:attributeGroup` references inside definitions both sets have.
    pub group_references: BTreeSet<(Name, Name)>,
    /// Whole groups.
    pub groups: BTreeSet<Name>,
    /// Whole simple types.
    pub simple_types: BTreeSet<Name>,
    /// Union members inside simple types both sets have.
    pub union_members: BTreeSet<(Name, Name)>,
    /// Wildcards inside definitions both sets have.
    pub wildcards: BTreeSet<(Name, Wildcard)>,
}

impl Additions {
    /// The definitions both sets have that gain child elements, attributes,
    /// group references, or enumeration values.
    #[must_use]
    pub fn extended_definitions(&self) -> BTreeSet<&Name> {
        self.elements
            .iter()
            .map(|(definition, _)| definition)
            .chain(self.attributes.iter().map(|(definition, _)| definition))
            .chain(
                self.group_references
                    .iter()
                    .map(|(definition, _)| definition),
            )
            .chain(
                self.enumeration_values
                    .iter()
                    .map(|(definition, _)| definition),
            )
            .collect()
    }

    /// Whether the set declares nothing the other lacks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// An `xsd:any`, by its `namespace` and `processContents`.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct Wildcard {
    pub namespace: String,
    pub process_contents: String,
}

/// What one definition itself declares, its names in the Transitional namespaces.
#[derive(Default)]
struct Declared {
    attributes: BTreeSet<Name>,
    elements: BTreeSet<Name>,
    enumeration: BTreeSet<String>,
    groups: BTreeSet<Name>,
    members: BTreeSet<Name>,
    wildcards: BTreeSet<Wildcard>,
}

impl Declared {
    fn attributes(&mut self, uses: &[AttributeUse], twins: &BTreeMap<String, String>) {
        for attribute in uses {
            match &attribute.group {
                Some(group) => self.groups.insert(translate(group, twins)),
                None => self.attributes.insert(translate(&attribute.name, twins)),
            };
        }
    }

    fn particle(&mut self, particle: &Particle, twins: &BTreeMap<String, String>) {
        match &particle.kind {
            ParticleKind::All(children)
            | ParticleKind::Choice(children)
            | ParticleKind::Sequence(children) => {
                for child in children {
                    self.particle(child, twins);
                }
            }
            ParticleKind::Any {
                namespace,
                process_contents,
            } => {
                let uris: Vec<&str> = namespace
                    .split_whitespace()
                    .map(|uri| twins.get(uri).map_or(uri, String::as_str))
                    .collect();
                self.wildcards.insert(Wildcard {
                    namespace: uris.join(" "),
                    process_contents: process_contents.clone(),
                });
            }
            ParticleKind::Element {
                name,
                type_ref: _type_ref,
            } => {
                self.elements.insert(translate(name, twins));
            }
            ParticleKind::GroupRef(name) => {
                self.groups.insert(translate(name, twins));
            }
        }
    }
}

/// The definitions of a set's twinned namespaces, as each declares itself.
struct Definitions {
    attribute_groups: BTreeMap<Name, Declared>,
    complex_types: BTreeMap<Name, Declared>,
    groups: BTreeMap<Name, Declared>,
    simple_types: BTreeMap<Name, Declared>,
}

impl Definitions {
    /// `twins` maps each namespace taking part to its Transitional twin.
    fn new(set: &SchemaSet, twins: &BTreeMap<String, String>) -> Self {
        let twinned = |name: &Name| twins.contains_key(&name.namespace);
        let mut definitions = Self {
            attribute_groups: BTreeMap::new(),
            complex_types: BTreeMap::new(),
            groups: BTreeMap::new(),
            simple_types: BTreeMap::new(),
        };
        for group in set
            .attribute_groups
            .values()
            .filter(|group| twinned(&group.name))
        {
            let mut declared = Declared::default();
            declared.attributes(&group.attributes, twins);
            definitions
                .attribute_groups
                .insert(translate(&group.name, twins), declared);
        }
        for complex_type in set
            .complex_types
            .values()
            .filter(|complex_type| twinned(&complex_type.name))
        {
            let mut declared = Declared::default();
            declared.attributes(&complex_type.attributes, twins);
            if let Content::Particle(particle) = &complex_type.content {
                declared.particle(particle, twins);
            }
            definitions
                .complex_types
                .insert(translate(&complex_type.name, twins), declared);
        }
        for group in set.groups.values().filter(|group| twinned(&group.name)) {
            let mut declared = Declared::default();
            declared.particle(&group.particle, twins);
            definitions
                .groups
                .insert(translate(&group.name, twins), declared);
        }
        for simple_type in set
            .simple_types
            .values()
            .filter(|simple_type| twinned(simple_type.name()))
        {
            let mut declared = Declared::default();
            match simple_type {
                SimpleType::List {
                    item: _item,
                    name: _name,
                } => {}
                SimpleType::Restriction {
                    base: _base,
                    enumeration,
                    facets: _facets,
                    name: _name,
                } => {
                    declared.enumeration = enumeration.iter().cloned().collect();
                }
                SimpleType::Union {
                    members,
                    name: _name,
                } => {
                    declared.members = members
                        .iter()
                        .filter_map(|member| Some(translate(&member.name()?, twins)))
                        .collect();
                }
            }
            definitions
                .simple_types
                .insert(translate(simple_type.name(), twins), declared);
        }
        definitions
    }
}

/// What `from` declares and `other` lacks.
fn additions(from: &Definitions, other: &Definitions) -> Additions {
    let mut found = Additions::default();
    found.attribute_groups = compare(&from.attribute_groups, &other.attribute_groups, &mut found);
    found.complex_types = compare(&from.complex_types, &other.complex_types, &mut found);
    found.groups = compare(&from.groups, &other.groups, &mut found);
    found.simple_types = compare(&from.simple_types, &other.simple_types, &mut found);
    found
}

/// The definitions of `from` that `other` lacks; what the definitions both
/// have declare in `from` alone goes into `found`.
fn compare(
    from: &BTreeMap<Name, Declared>,
    other: &BTreeMap<Name, Declared>,
    found: &mut Additions,
) -> BTreeSet<Name> {
    let mut whole = BTreeSet::new();
    for (name, declared) in from {
        let Some(twin) = other.get(name) else {
            whole.insert(name.clone());
            continue;
        };
        for attribute in declared.attributes.difference(&twin.attributes) {
            found.attributes.insert((name.clone(), attribute.clone()));
        }
        for element in declared.elements.difference(&twin.elements) {
            found.elements.insert((name.clone(), element.clone()));
        }
        for value in declared.enumeration.difference(&twin.enumeration) {
            found
                .enumeration_values
                .insert((name.clone(), value.clone()));
        }
        for group in declared.groups.difference(&twin.groups) {
            found.group_references.insert((name.clone(), group.clone()));
        }
        for member in declared.members.difference(&twin.members) {
            found.union_members.insert((name.clone(), member.clone()));
        }
        for wildcard in declared.wildcards.difference(&twin.wildcards) {
            found.wildcards.insert((name.clone(), wildcard.clone()));
        }
    }
    whole
}

/// Each Transitional twin namespace mapped to itself.
fn identity(twins: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    twins
        .values()
        .map(|namespace| (namespace.clone(), namespace.clone()))
        .collect()
}

/// What Strict declares and Transitional lacks, in the Transitional namespaces.
#[must_use]
pub fn strict_only(transitional: &SchemaSet, strict: &SchemaSet) -> Additions {
    let twins = twins(transitional, strict);
    additions(
        &Definitions::new(strict, &twins),
        &Definitions::new(transitional, &identity(&twins)),
    )
}

/// What Transitional declares and Strict lacks, in the 21 files both sets
/// share.
#[must_use]
pub fn transitional_only(transitional: &SchemaSet, strict: &SchemaSet) -> Additions {
    let twins = twins(transitional, strict);
    additions(
        &Definitions::new(transitional, &identity(&twins)),
        &Definitions::new(strict, &twins),
    )
}

/// `name` in its Transitional twin namespace, when it has one.
fn translate(name: &Name, twins: &BTreeMap<String, String>) -> Name {
    twins.get(&name.namespace).map_or_else(
        || name.clone(),
        |namespace| Name::new(namespace, &name.local),
    )
}

/// Each Strict namespace mapped to the Transitional namespace of the file with the same name.
fn twins(transitional: &SchemaSet, strict: &SchemaSet) -> BTreeMap<String, String> {
    let by_file: BTreeMap<_, _> = transitional
        .namespaces
        .iter()
        .filter_map(|(namespace, file)| Some((file.path.file_name()?, namespace)))
        .collect();
    strict
        .namespaces
        .iter()
        .filter_map(|(namespace, file)| {
            let twin = by_file.get(file.path.file_name()?)?;
            Some((namespace.clone(), (*twin).clone()))
        })
        .collect()
}
