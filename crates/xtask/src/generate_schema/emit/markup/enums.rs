//! The enums of the typed tree (D34): every choice position, flattened to the
//! set of tags it allows, and positions with the same set sharing one enum;
//! the extension tags SDK 3.5.1's data places beside ECMA tags joining those
//! sets (D24); the choices of Microsoft's extension types; and the names of
//! `enum_names.toml`.

use alloc::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result, bail};

use super::{Data, NodeKey, Source};
use crate::generate_schema::model::{Content, Method, Name, Particle, ParticleKind, SchemaSet};
use crate::generate_schema::sdk_data::DataParticle;

/// One entry of a flattened choice: an element's tag and type, or a wildcard.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Entry {
    /// An `xsd:any`: its `namespace` attribute and the namespace of the
    /// schema that declares it.
    Any {
        constraint: String,
        target: String,
    },
    Element(NodeKey),
}

/// A choice position: the type whose content holds it, its flattened set, and
/// where the choice is written.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Position {
    pub owner: Name,
    pub set: BTreeSet<Entry>,
    pub source: String,
}

/// What the SDK's data places in ECMA types (D24): extension tags that join
/// an ECMA choice, tags that become fields, and new choices of extension tags
/// only.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Placements {
    /// An ECMA set to the extension tags added to it, by owner type.
    pub added: BTreeMap<BTreeSet<Entry>, BTreeMap<Name, BTreeSet<Entry>>>,
    /// An owner type's extension tags that land in a sequence, in the data's
    /// order: the tag, the ECMA tag before it, and whether it repeats or may
    /// be absent.
    pub fields: BTreeMap<Name, Vec<Placed>>,
    /// New choice positions of extension tags only in ECMA types, with the
    /// ECMA tag before each.
    pub positions: Vec<(Position, Placed)>,
}

/// Where a placed extension item sits in its ECMA owner's content.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Placed {
    /// The ECMA tag before it in the data's order; `None` when it comes first.
    pub after: Option<Name>,
    pub key: Option<NodeKey>,
    /// Whether it may repeat.
    pub many: bool,
    /// Whether it may be absent.
    pub optional: bool,
}

/// One row of `enum_names.toml`.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Row {
    /// A tag the set holds, `c:valAx`, telling apart rows of one module,
    /// source, and types.
    pub contains: Option<String>,
    pub module: String,
    pub name: String,
    pub source: String,
    pub types: Vec<String>,
    /// The namespaces of a wildcard enum's position, `##any`, or prefixes
    /// apart by spaces: its row is found by them, and names its module.
    pub wildcard: Option<String>,
}

/// The walk of one ECMA type's particle in the data.
struct Placer<'walk, 'data> {
    after: Option<Name>,
    candidates: &'walk [&'walk BTreeSet<Entry>],
    data: &'walk Data<'data>,
    owner: &'walk Name,
    placements: &'walk mut Placements,
}

impl Placer<'_, '_> {
    fn ecma_tag(&self, name: &str) -> Option<(String, String)> {
        let (_, _, prefix, local) = split(name)?;
        if self.data.is_extension(prefix) {
            return None;
        }
        Some((self.data.uri(prefix)?.to_owned(), local.to_owned()))
    }

    fn element(
        &mut self,
        name: &str,
        inner: Option<&DataParticle>,
        outer: Option<&DataParticle>,
        many: bool,
        optional: bool,
    ) {
        let Some((_, _, element_prefix, local)) = split(name) else {
            return;
        };
        if !self.data.is_extension(element_prefix) {
            self.after = self.data.name(element_prefix, local);
            return;
        }
        let key = self.data.resolve(name);
        let placed = Placed {
            after: self.after.clone(),
            key: key.clone(),
            many,
            optional,
        };
        let (Some(inner_choice), Some(outer_choice)) = (inner, outer) else {
            self.placements
                .fields
                .entry(self.owner.clone())
                .or_default()
                .push(placed);
            return;
        };
        let direct: BTreeSet<(String, String)> = inner_choice
            .items
            .iter()
            .filter_map(|item| self.ecma_tag(item.name.as_deref()?))
            .collect();
        let context = if direct.is_empty() {
            let mut all = Vec::new();
            names(outer_choice, &mut all);
            all.iter().filter_map(|n| self.ecma_tag(n)).collect()
        } else {
            direct
        };
        if context.is_empty() {
            let mut all = Vec::new();
            names(outer_choice, &mut all);
            let set: BTreeSet<Entry> = all
                .iter()
                .filter_map(|n| self.data.resolve(n))
                .map(Entry::Element)
                .collect();
            let position = Position {
                owner: self.owner.clone(),
                set,
                source: "a choice of extension tags the SDK's schema data adds".to_owned(),
            };
            let (outer_many, outer_optional) = occurs(outer_choice);
            if !self
                .placements
                .positions
                .iter()
                .any(|(known, _)| *known == position)
            {
                self.placements.positions.push((
                    position,
                    Placed {
                        after: self.after.clone(),
                        key: None,
                        many: outer_many || many,
                        optional: outer_optional || optional,
                    },
                ));
            }
            return;
        }
        let score = |candidate: &BTreeSet<Entry>| {
            let tags = tag_pairs(candidate);
            let shared = tags.intersection(&context).count();
            let union = tags.union(&context).count();
            (shared, union)
        };
        let mut best: Option<(&BTreeSet<Entry>, (usize, usize))> = None;
        for candidate in self.candidates {
            let found = score(candidate);
            let better = best.is_none_or(|(_, (shared, union))| found.0 * union > shared * found.1);
            if better {
                best = Some((candidate, found));
            }
        }
        match (best, key) {
            (Some((set, (shared, _))), Some(found)) if shared > 0 => {
                self.placements
                    .added
                    .entry(set.clone())
                    .or_default()
                    .entry(self.owner.clone())
                    .or_default()
                    .insert(Entry::Element(found));
            }
            _ => {
                self.placements
                    .fields
                    .entry(self.owner.clone())
                    .or_default()
                    .push(placed);
            }
        }
    }

    fn place(
        &mut self,
        particle: &DataParticle,
        inner: Option<&DataParticle>,
        outer: Option<&DataParticle>,
        many: bool,
        optional: bool,
    ) {
        // An item of a choice may be absent: another alternative stands in its place.
        let alternative = particle.kind.as_deref() == Some("Choice");
        for item in &particle.items {
            let (item_many, occurs_optional) = occurs(item);
            let item_optional = occurs_optional || alternative;
            match item.kind.as_deref() {
                Some("Sequence" | "Group" | "All") => {
                    self.place(
                        item,
                        inner,
                        outer,
                        many || item_many,
                        optional || item_optional,
                    );
                }
                Some("Choice") => {
                    self.place(
                        item,
                        Some(item),
                        outer.or(Some(item)),
                        many || item_many,
                        optional || item_optional,
                    );
                }
                Some(_) => {}
                None => {
                    if let Some(name) = &item.name {
                        self.element(
                            name,
                            inner,
                            outer,
                            many || item_many,
                            optional || item_optional,
                        );
                    }
                }
            }
        }
    }
}

/// Every choice position of the set's complex types, each type's in the
/// order its content writes them.
#[must_use]
pub fn ecma_positions(set: &SchemaSet) -> Vec<Position> {
    let mut found = Vec::new();
    for (owner, complex_type) in &set.complex_types {
        if let Content::Particle(particle) = &complex_type.content {
            walk(set, owner, particle, None, &mut found);
        }
    }
    found
}

fn walk(
    set: &SchemaSet,
    owner: &Name,
    particle: &Particle,
    within: Option<&Name>,
    found: &mut Vec<Position>,
) {
    match &particle.kind {
        ParticleKind::Choice(_) => {
            let source = format!(
                "local choice in {}",
                within.map_or(owner.local.as_str(), |group| group.local.as_str())
            );
            found.push(Position {
                owner: owner.clone(),
                set: flatten(set, particle, &owner.namespace),
                source,
            });
        }
        ParticleKind::GroupRef(name) => {
            if let Some(group) = set.groups.get(name) {
                if let ParticleKind::Choice(_) = &group.particle.kind {
                    found.push(Position {
                        owner: owner.clone(),
                        set: flatten(set, &group.particle, &name.namespace),
                        source: name.local.clone(),
                    });
                } else {
                    walk(set, owner, &group.particle, Some(name), found);
                }
            }
        }
        ParticleKind::All(children) | ParticleKind::Sequence(children) => {
            for child in children {
                walk(set, owner, child, within, found);
            }
        }
        ParticleKind::Any {
            namespace: _namespace,
            process_contents: _process_contents,
        } => {}
        ParticleKind::Element {
            name: _name,
            type_ref: _type_ref,
        } => {}
    }
}

/// Every element and wildcard a particle reaches, groups and nested
/// particles expanded; `target` is the namespace of the schema writing it.
#[must_use]
pub fn flatten(set: &SchemaSet, particle: &Particle, target: &str) -> BTreeSet<Entry> {
    let mut found = BTreeSet::new();
    gather(set, particle, target, &mut found);
    found
}

fn gather(set: &SchemaSet, particle: &Particle, target: &str, found: &mut BTreeSet<Entry>) {
    match &particle.kind {
        ParticleKind::All(children)
        | ParticleKind::Choice(children)
        | ParticleKind::Sequence(children) => {
            for child in children {
                gather(set, child, target, found);
            }
        }
        ParticleKind::Any {
            namespace,
            process_contents: _process_contents,
        } => {
            found.insert(Entry::Any {
                constraint: namespace.clone(),
                target: target.to_owned(),
            });
        }
        ParticleKind::Element { name, type_ref } => {
            found.insert(Entry::Element(NodeKey {
                source: Source::Xsd(type_ref.clone()),
                tag: name.clone(),
            }));
        }
        ParticleKind::GroupRef(name) => {
            if let Some(group) = set.groups.get(name) {
                gather(set, &group.particle, &name.namespace, found);
            }
        }
    }
}

/// Every complex type that extends `owner`, directly or through others, and `owner` itself first.
#[must_use]
pub fn with_derived(set: &SchemaSet, owner: &Name) -> Vec<Name> {
    let mut found = vec![owner.clone()];
    let mut index = 0;
    while let Some(current) = found.get(index).cloned() {
        for (name, complex_type) in &set.complex_types {
            let extends = complex_type.base.as_ref().is_some_and(|derivation| {
                derivation.method == Method::Extension
                    && derivation.base.name().as_ref() == Some(&current)
                    && matches!(complex_type.content, Content::Particle(_) | Content::Empty)
            });
            if extends && !found.contains(name) {
                found.push(name.clone());
            }
        }
        index += 1;
    }
    found
}

/// The extension placements of the SDK's data in ECMA types, by the rule of
/// D24 and D34: an extension tag inside a choice of the data joins the 5th
/// edition's choice of its owner type that shares the most tags with the ECMA
/// tags beside it there; one inside no choice, or beside no ECMA tag of any
/// such choice, is a field; a choice of extension tags only is a new position.
#[must_use]
pub fn placements(data: &Data<'_>, positions: &[Position]) -> Placements {
    let mut by_owner: BTreeMap<&Name, Vec<&BTreeSet<Entry>>> = BTreeMap::new();
    for position in positions.iter().filter(|position| position.set.len() > 1) {
        by_owner
            .entry(&position.owner)
            .or_default()
            .push(&position.set);
    }
    let mut placements = Placements::default();
    for (name, data_type) in &data.sdk.types {
        if name.ends_with('/') {
            continue;
        }
        let Some(particle) = &data_type.particle else {
            continue;
        };
        let Some((type_prefix, type_local, _, _)) = split(name) else {
            continue;
        };
        if data.is_extension(type_prefix) {
            continue;
        }
        let Some(owner) = data.name(type_prefix, type_local) else {
            continue;
        };
        if !data.transitional.complex_types.contains_key(&owner) {
            continue;
        }
        let candidates = by_owner.get(&owner).cloned().unwrap_or_default();
        let top = (particle.kind.as_deref() == Some("Choice")).then_some(particle);
        let mut walker = Placer {
            after: None,
            candidates: &candidates,
            data,
            owner: &owner,
            placements: &mut placements,
        };
        walker.place(particle, top, top, false, false);
    }
    placements
}

/// Every element name under a particle of the data, in order.
fn names<'data>(particle: &'data DataParticle, found: &mut Vec<&'data str>) {
    for item in &particle.items {
        match (&item.kind, &item.name) {
            (Some(_), _) => names(item, found),
            (None, Some(name)) => found.push(name),
            (None, None) => {}
        }
    }
}

/// The (namespace, tag) pairs of a set's elements.
fn tag_pairs(set: &BTreeSet<Entry>) -> BTreeSet<(String, String)> {
    set.iter()
        .filter_map(|entry| match entry {
            Entry::Element(key) => Some((key.tag.namespace.clone(), key.tag.local.clone())),
            Entry::Any {
                constraint: _constraint,
                target: _target,
            } => None,
        })
        .collect()
}

/// Whether a particle of the data may repeat, and whether it may be absent.
#[must_use]
pub fn occurs(particle: &DataParticle) -> (bool, bool) {
    if particle.occurs.is_empty() {
        return (false, false);
    }
    let many = particle
        .occurs
        .iter()
        .any(|alternative| alternative.max != 1);
    let optional = particle
        .occurs
        .iter()
        .any(|alternative| alternative.min == 0);
    (many, optional)
}

/// The choice positions of an extension type in the data: every choice not
/// inside another choice, flattened.
#[must_use]
pub fn data_positions(data: &Data<'_>, particle: &DataParticle) -> Vec<BTreeSet<Entry>> {
    let mut found = Vec::new();
    data_walk(data, particle, &mut found);
    found
}

fn data_walk(data: &Data<'_>, particle: &DataParticle, found: &mut Vec<BTreeSet<Entry>>) {
    if particle.kind.as_deref() == Some("Choice") {
        found.push(data_flatten(data, particle));
        return;
    }
    for item in &particle.items {
        if matches!(
            item.kind.as_deref(),
            Some("Sequence" | "Choice" | "Group" | "All")
        ) {
            data_walk(data, item, found);
        }
    }
}

/// Every element and wildcard under a particle of the data.
#[must_use]
pub fn data_flatten(data: &Data<'_>, particle: &DataParticle) -> BTreeSet<Entry> {
    let mut found = BTreeSet::new();
    data_gather(data, particle, &mut found);
    found
}

fn data_gather(data: &Data<'_>, particle: &DataParticle, found: &mut BTreeSet<Entry>) {
    for item in &particle.items {
        match item.kind.as_deref() {
            Some("Any") => {
                found.insert(Entry::Any {
                    constraint: item.namespace.clone().unwrap_or_else(|| "##any".to_owned()),
                    target: String::new(),
                });
            }
            Some(_) => data_gather(data, item, found),
            None => {
                if let Some(key) = item.name.as_deref().and_then(|name| data.resolve(name)) {
                    found.insert(Entry::Element(key));
                }
            }
        }
    }
}

/// A data type's name, `w:CT_P/w:p`: the type's prefix and local name, then
/// the element's.
#[must_use]
pub fn split(name: &str) -> Option<(&str, &str, &str, &str)> {
    let (schema_type, element) = name.split_once('/')?;
    let (type_prefix, type_local) = schema_type.split_once(':')?;
    let (element_prefix, element_local) = element.split_once(':')?;
    Some((type_prefix, type_local, element_prefix, element_local))
}

/// Read `enum_names.toml`.
///
/// # Errors
///
/// Text that is not TOML, or a row missing a field.
pub fn rows(text: &str) -> Result<Vec<Row>> {
    let table: toml::Table = text
        .parse()
        .context("the table of enum names is not TOML")?;
    let Some(list) = table.get("enum") else {
        bail!("the table of enum names has no `enum` rows");
    };
    list.as_array()
        .context("`enum` is not a list of rows")?
        .iter()
        .map(|row| {
            let field = |name: &str| -> Result<String> {
                Ok(row
                    .get(name)
                    .and_then(toml::Value::as_str)
                    .with_context(|| format!("an `enum` row has no `{name}`"))?
                    .to_owned())
            };
            let types = row
                .get("types")
                .and_then(toml::Value::as_array)
                .map_or(&[][..], Vec::as_slice)
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .context("a type of an `enum` row is not a string")
                })
                .collect::<Result<Vec<String>>>()?;
            Ok(Row {
                contains: row
                    .get("contains")
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned),
                module: field("module")?,
                name: field("name")?,
                source: field("source")?,
                types,
                wildcard: row
                    .get("wildcard")
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect()
}
