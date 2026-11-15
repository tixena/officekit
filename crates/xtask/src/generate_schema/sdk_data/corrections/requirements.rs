//! What ECMA-376 requires and SDK 3.5.1's data does not check, taken from the Transitional
//! schemas: each attribute they make required, each content model whose order or required
//! children the data loosens, and each wildcard child the data makes optional; and the `uri` of
//! the extensions they leave optional, which the data requires.

use alloc::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result, bail};

use super::collect_leaves;
use super::standard::prefix_of;
use crate::generate_schema::model::{
    ComplexType, Content, Name, Particle, ParticleKind, SchemaSet, TypeRef,
};
use crate::generate_schema::sdk_data::{DataParticle, Occurs, SdkData, Validator};

#[cfg(test)]
mod tests;

/// What the table keeps as the data has it: `(type, attribute)` pairs an `[[optional]]` row
/// leaves optional, types an `[[any_order]]` row frees, `(type, attribute)` pairs an
/// `[[undeclared_required]]` row names, and `(type, element)` pairs an `[[undeclared_child]]` row
/// names.
pub(super) struct Kept {
    pub free: Vec<String>,
    pub optional: Vec<(String, String)>,
    pub undeclared: Vec<(String, String)>,
    pub undeclared_children: Vec<(String, String)>,
}

/// Add to `data` what the schemas of `standard` require: the required check of each attribute
/// they make required, the content model of each type whose order or required children the
/// data loosens, its children outside the standard allowed anywhere, and each wildcard child
/// they require; and take off the required check of each extension `uri` they leave optional.
///
/// # Errors
///
/// A required attribute or child the data does not declare and no row names, a row naming one
/// the data declares, a content model the data cannot hold, or no extension with an optional
/// `uri`.
pub(super) fn require(data: &mut SdkData, standard: &SchemaSet, kept: &Kept) -> Result<()> {
    let mut problems = Vec::new();
    let names: Vec<String> = data.types.keys().cloned().collect();
    let mut undeclared = BTreeSet::new();
    let mut undeclared_children = BTreeSet::new();
    for name in &names {
        let Some(declared) = schema_type(data, standard, name) else {
            continue;
        };
        let effective = standard.effective(declared);
        for attribute in effective.attributes.iter().filter(|found| found.required) {
            let qname = format!(
                "{}:{}",
                prefix_of(data, &attribute.name.namespace)?,
                attribute.name.local
            );
            let key = (name.clone(), qname.clone());
            if kept.optional.contains(&key) {
                continue;
            }
            if !require_attribute(data, name, &qname)? {
                if kept.undeclared.contains(&key) {
                    undeclared.insert(key);
                } else {
                    problems.push(format!(
                        "ECMA-376 requires the attribute {qname} of {name}, which the data does \
                         not declare"
                    ));
                }
            }
        }
        let Content::Particle(particle) = &effective.content else {
            continue;
        };
        let rows: Vec<&(String, String)> = kept
            .undeclared_children
            .iter()
            .filter(|(type_name, _)| type_name == name)
            .collect();
        if !rows.is_empty() {
            for row in rows {
                if holds_undeclared(data, standard, name, particle, &row.1)? {
                    undeclared_children.insert(row.clone());
                }
            }
            continue;
        }
        if !kept.free.contains(name)
            && let Err(error) = standard_content(data, standard, name, particle)
        {
            problems.push(format!("in the content of {name}: {error:#}"));
        }
    }
    for (name, element) in &kept.undeclared_children {
        if !undeclared_children.contains(&(name.clone(), element.clone())) {
            problems.push(format!(
                "the `undeclared_child` row of {name} names {element}, which the data holds or \
                 ECMA-376 does not declare there"
            ));
        }
    }
    for (name, attribute) in &kept.undeclared {
        if !undeclared.contains(&(name.clone(), attribute.clone())) {
            problems.push(format!(
                "the `undeclared_required` row of {name} names {attribute}, which the data \
                 declares or ECMA-376 does not require"
            ));
        }
    }
    if !problems.is_empty() {
        bail!("what ECMA-376 requires:\n  {}", problems.join("\n  "));
    }
    let types = standard_types(data, standard);
    require_wildcards(data, standard, &types);
    optional_extension_uris(data, standard, &types)
}

/// The data's form of `particle`, the schema's content model: each element the data leaf of the
/// same element, with the schema's occurrences.
fn converted(
    data: &SdkData,
    standard: &SchemaSet,
    particle: &Particle,
    leaves: &[DataParticle],
) -> Result<DataParticle> {
    let composite = |kind: &str, items: &[Particle]| -> Result<DataParticle> {
        Ok(DataParticle {
            initial_version: None,
            items: items
                .iter()
                .map(|item| converted(data, standard, item, leaves))
                .collect::<Result<_>>()?,
            kind: Some(kind.to_owned()),
            name: None,
            namespace: None,
            occurs: occurs(particle.min, particle.max),
            require_filter: false,
        })
    };
    match &particle.kind {
        ParticleKind::All(items) => composite("All", items),
        ParticleKind::Choice(items) => composite("Choice", items),
        ParticleKind::Sequence(items) => composite("Sequence", items),
        ParticleKind::Element {
            name,
            type_ref: _type_ref,
        } => {
            let element = format!("{}:{}", prefix_of(data, &name.namespace)?, name.local);
            let mut leaf = leaves
                .iter()
                .find(|leaf| element_of(leaf) == Some(element.as_str()))
                .cloned()
                .with_context(|| format!("the data holds no {element}"))?;
            leaf.occurs = occurs(particle.min, particle.max);
            Ok(leaf)
        }
        ParticleKind::GroupRef(group) => {
            let found = standard
                .groups
                .get(group)
                .with_context(|| format!("the standard has no group {group}"))?;
            Ok(DataParticle {
                initial_version: None,
                items: vec![converted(data, standard, &found.particle, leaves)?],
                kind: Some("Group".to_owned()),
                name: None,
                namespace: None,
                occurs: occurs(particle.min, particle.max),
                require_filter: false,
            })
        }
        ParticleKind::Any {
            namespace: _namespace,
            process_contents: _process_contents,
        } => bail!("a wildcard in the content model"),
    }
}

/// The elements a data particle requires: those every match holds.
fn data_required(particle: &DataParticle) -> BTreeSet<String> {
    let minimum = particle.occurs.iter().map(|found| found.min).min();
    if minimum == Some(0) {
        return BTreeSet::new();
    }
    if let Some(element) = element_of(particle) {
        return BTreeSet::from([element.to_owned()]);
    }
    let mut each = particle.items.iter().map(data_required);
    if particle.kind.as_deref() == Some("Choice") {
        let first = each.next().unwrap_or_default();
        each.fold(first, |kept, item| &kept & &item)
    } else {
        each.flatten().collect()
    }
}

/// The element of a data leaf, `w:alias` of `w:CT_String/w:alias`.
fn element_of(leaf: &DataParticle) -> Option<&str> {
    leaf.name.as_deref().and_then(element_named)
}

/// The element of a data type's name, `w:alias` of `w:CT_String/w:alias`.
fn element_named(name: &str) -> Option<&str> {
    name.split_once('/').map(|(_, element)| element)
}

/// Each element `particle` declares with a complex type, and that type, when it has one only;
/// the elements of the groups it refers to too.
fn element_types(standard: &SchemaSet, particle: &Particle) -> BTreeMap<Name, Name> {
    fn walk(
        standard: &SchemaSet,
        particle: &Particle,
        groups: &mut Vec<Name>,
        found: &mut BTreeMap<Name, BTreeSet<Name>>,
    ) {
        match &particle.kind {
            ParticleKind::All(items)
            | ParticleKind::Choice(items)
            | ParticleKind::Sequence(items) => {
                for item in items {
                    walk(standard, item, groups, found);
                }
            }
            ParticleKind::Element {
                name,
                type_ref:
                    TypeRef::Complex {
                        anonymous: _anonymous,
                        name: complex_type,
                    },
            } => {
                found
                    .entry(name.clone())
                    .or_default()
                    .insert(complex_type.clone());
            }
            ParticleKind::GroupRef(group) => {
                if groups.contains(group) {
                    return;
                }
                if let Some(defined) = standard.groups.get(group) {
                    groups.push(group.clone());
                    walk(standard, &defined.particle, groups, found);
                    groups.pop();
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
    let mut found = BTreeMap::new();
    walk(standard, particle, &mut Vec::new(), &mut found);
    found
        .into_iter()
        .filter_map(|(element, types)| {
            let mut each = types.into_iter();
            match (each.next(), each.next()) {
                (Some(only), None) => Some((element, only)),
                _ => None,
            }
        })
        .collect()
}

/// Each element a schema particle names, as the data spells it.
fn elements(
    data: &SdkData,
    standard: &SchemaSet,
    particle: &Particle,
    found: &mut BTreeSet<String>,
) -> Result<()> {
    match &particle.kind {
        ParticleKind::All(items) | ParticleKind::Choice(items) | ParticleKind::Sequence(items) => {
            for item in items {
                elements(data, standard, item, found)?;
            }
        }
        ParticleKind::Element {
            name,
            type_ref: _type_ref,
        } => {
            found.insert(format!(
                "{}:{}",
                prefix_of(data, &name.namespace)?,
                name.local
            ));
        }
        ParticleKind::GroupRef(group) => {
            if let Some(defined) = standard.groups.get(group) {
                elements(data, standard, &defined.particle, found)?;
            }
        }
        ParticleKind::Any {
            namespace: _namespace,
            process_contents: _process_contents,
        } => {}
    }
    Ok(())
}

/// Whether the schema's content model `particle` of the type `name` declares `element` and the
/// data's holds no leaf of it.
fn holds_undeclared(
    data: &SdkData,
    standard: &SchemaSet,
    name: &str,
    particle: &Particle,
    element: &str,
) -> Result<bool> {
    let mut declared = BTreeSet::new();
    elements(data, standard, particle, &mut declared)?;
    let mut leaves = Vec::new();
    if let Some(held) = data
        .types
        .get(name)
        .and_then(|found| found.particle.as_ref())
    {
        collect_leaves(held, None, &mut leaves);
    }
    Ok(declared.contains(element) && !leaves.iter().any(|leaf| element_of(leaf) == Some(element)))
}

/// `particle` with each child the standard does not declare allowed before, between and after
/// the items of its sequence, in any order and any number.
fn interleaved(particle: DataParticle, foreign: Vec<DataParticle>) -> Result<DataParticle> {
    if particle.kind.as_deref() != Some("Sequence") {
        bail!("a content model holding children ECMA-376 does not declare, not a sequence");
    }
    let free = DataParticle {
        initial_version: None,
        items: foreign
            .into_iter()
            .map(|mut leaf| {
                leaf.occurs.clear();
                leaf
            })
            .collect(),
        kind: Some("Choice".to_owned()),
        name: None,
        namespace: None,
        occurs: occurs(0, None),
        require_filter: false,
    };
    let mut items = vec![free.clone()];
    for item in particle.items {
        items.push(item);
        items.push(free.clone());
    }
    Ok(DataParticle { items, ..particle })
}

/// The occurrences of the data for `min` to `max`, unbounded for `None`: none for exactly once.
fn occurs(min: u32, max: Option<u32>) -> Vec<Occurs> {
    if (min, max) == (1, Some(1)) {
        return Vec::new();
    }
    vec![Occurs {
        include_version: false,
        max: max.unwrap_or(0),
        min,
        version: None,
    }]
}

/// Add the required check to the attribute `qname` of the type `name`, or of the base it derives
/// its attributes from; whether either declares it.
fn require_attribute(data: &mut SdkData, name: &str, qname: &str) -> Result<bool> {
    let data_type = data
        .types
        .get(name)
        .with_context(|| format!("the data has no `{name}`"))?;
    let owner = if data_type
        .attributes
        .iter()
        .any(|attribute| attribute.qname == qname)
    {
        name.to_owned()
    } else if data_type.is_derived
        && let Some((schema_type, _)) = name.split_once('/')
    {
        format!("{schema_type}/")
    } else {
        return Ok(false);
    };
    let Some(attribute) = data.types.get_mut(&owner).and_then(|found| {
        found
            .attributes
            .iter_mut()
            .find(|attribute| attribute.qname == qname)
    }) else {
        return Ok(false);
    };
    if !attribute
        .validators
        .iter()
        .any(|validator| validator.name == "RequiredValidator")
    {
        attribute.validators.push(Validator {
            arguments: Vec::new(),
            is_initial_version: false,
            is_list: false,
            name: "RequiredValidator".to_owned(),
            type_name: None,
            union_id: None,
            version: None,
        });
    }
    Ok(true)
}

/// The Transitional complex type of the data type `name`, when the schemas declare one.
fn schema_type<'set>(
    data: &SdkData,
    standard: &'set SchemaSet,
    name: &str,
) -> Option<&'set ComplexType> {
    let (schema_type, _) = name.split_once('/')?;
    standard
        .complex_types
        .get(&standard_name(data, schema_type)?)
}

/// The name `prefix:local` of the data, in the namespace the data declares the prefix for.
fn standard_name(data: &SdkData, qualified: &str) -> Option<Name> {
    let (prefix, local) = qualified.split_once(':')?;
    let namespace = data
        .namespaces
        .iter()
        .find(|namespace| namespace.prefix == prefix)?;
    Some(Name::new(&namespace.uri, local))
}

/// The Transitional complex type of each schema type of the data: the one of its name, else
/// the one its element is declared with in the type of every parent found so, in turn until no
/// more are found. `c:CT_NumRefExtension`, the data's own, is `CT_Extension` so.
fn standard_types(data: &SdkData, standard: &SchemaSet) -> BTreeMap<String, Name> {
    let mut found: BTreeMap<String, Name> = BTreeMap::new();
    for name in data.types.keys() {
        if let Some(declared) = schema_type(data, standard, name) {
            found.insert(schema_type_of(name).to_owned(), declared.name.clone());
        }
    }
    loop {
        let mut candidates: BTreeMap<String, BTreeSet<Name>> = BTreeMap::new();
        for (name, data_type) in &data.types {
            let Some(parent) = found
                .get(schema_type_of(name))
                .and_then(|parent| standard.complex_types.get(parent))
            else {
                continue;
            };
            let Content::Particle(particle) = standard.effective(parent).content else {
                continue;
            };
            let declared = element_types(standard, &particle);
            let mut children: BTreeSet<String> = data_type.children.iter().cloned().collect();
            if let Some(held) = &data_type.particle {
                let mut leaves = Vec::new();
                collect_leaves(held, None, &mut leaves);
                children.extend(leaves.into_iter().filter_map(|leaf| leaf.name));
            }
            for child in &children {
                let child_type = schema_type_of(child);
                if found.contains_key(child_type) {
                    continue;
                }
                if let Some(ecma) = element_named(child)
                    .and_then(|element| standard_name(data, element))
                    .and_then(|element| declared.get(&element))
                {
                    candidates
                        .entry(child_type.to_owned())
                        .or_default()
                        .insert(ecma.clone());
                }
            }
        }
        let before = found.len();
        for (child_type, types) in candidates {
            let mut each = types.into_iter();
            if let (Some(only), None) = (each.next(), each.next()) {
                found.insert(child_type, only);
            }
        }
        if found.len() == before {
            return found;
        }
    }
}

/// The schema type part of a data type's name, `w:CT_String` of `w:CT_String/w:alias`.
fn schema_type_of(name: &str) -> &str {
    name.split_once('/')
        .map_or(name, |(schema_type, _)| schema_type)
}

/// Whether every match of a schema particle holds an element a wildcard takes.
fn requires_wildcard(standard: &SchemaSet, particle: &Particle) -> bool {
    if particle.min == 0 {
        return false;
    }
    match &particle.kind {
        ParticleKind::Any {
            namespace: _namespace,
            process_contents: _process_contents,
        } => true,
        ParticleKind::All(items) | ParticleKind::Sequence(items) => {
            items.iter().any(|item| requires_wildcard(standard, item))
        }
        ParticleKind::Choice(items) => {
            !items.is_empty() && items.iter().all(|item| requires_wildcard(standard, item))
        }
        ParticleKind::Element {
            name: _name,
            type_ref: _type_ref,
        } => false,
        ParticleKind::GroupRef(group) => standard
            .groups
            .get(group)
            .is_some_and(|defined| requires_wildcard(standard, &defined.particle)),
    }
}

/// Each wildcard of a data particle at least once.
fn raise_wildcards(particle: &mut DataParticle) {
    if particle.kind.as_deref() == Some("Any") {
        for occurs in &mut particle.occurs {
            occurs.min = occurs.min.max(1);
        }
    }
    for item in &mut particle.items {
        raise_wildcards(item);
    }
}

/// Require the wildcards of each data type whose Transitional type, of `types`, requires a
/// wildcard child, as `CT_Extension` does, where the data makes them optional.
fn require_wildcards(data: &mut SdkData, standard: &SchemaSet, types: &BTreeMap<String, Name>) {
    for (name, data_type) in &mut data.types {
        let Some(declared) = types
            .get(schema_type_of(name))
            .and_then(|found| standard.complex_types.get(found))
        else {
            continue;
        };
        let requires = match &standard.effective(declared).content {
            Content::Particle(particle) => requires_wildcard(standard, particle),
            Content::Empty | Content::Simple(_) => false,
        };
        if requires && let Some(held) = &mut data_type.particle {
            raise_wildcards(held);
        }
    }
}

/// Take the required check off the `uri` of each data type whose Transitional type, of `types`,
/// is a `CT_Extension` leaving it optional: the chart's and the spreadsheet's.
///
/// # Errors
///
/// No data type has such a type, as when the data's names change.
fn optional_extension_uris(
    data: &mut SdkData,
    standard: &SchemaSet,
    types: &BTreeMap<String, Name>,
) -> Result<()> {
    let mut optional = Vec::new();
    for name in data.types.keys() {
        let Some(declared) = types
            .get(schema_type_of(name))
            .and_then(|found| standard.complex_types.get(found))
            .filter(|found| found.name.local == "CT_Extension")
        else {
            continue;
        };
        if let Some(uri) = standard
            .effective(declared)
            .attributes
            .iter()
            .find(|attribute| attribute.name.local == "uri" && !attribute.required)
        {
            let qname = format!("{}:uri", prefix_of(data, &uri.name.namespace)?);
            optional.push((name.clone(), qname));
        }
    }
    if optional.is_empty() {
        bail!("no data type has a Transitional CT_Extension whose uri is optional");
    }
    for (name, qname) in optional {
        if let Some(attribute) = data.types.get_mut(&name).and_then(|found| {
            found
                .attributes
                .iter_mut()
                .find(|attribute| attribute.qname == qname)
        }) {
            attribute
                .validators
                .retain(|validator| validator.name != "RequiredValidator");
        }
    }
    Ok(())
}

/// The elements a schema particle requires: those every match holds.
fn schema_required(
    data: &SdkData,
    standard: &SchemaSet,
    particle: &Particle,
) -> Result<BTreeSet<String>> {
    if particle.min == 0 {
        return Ok(BTreeSet::new());
    }
    Ok(match &particle.kind {
        ParticleKind::All(items) | ParticleKind::Sequence(items) => {
            let mut found = BTreeSet::new();
            for item in items {
                found.extend(schema_required(data, standard, item)?);
            }
            found
        }
        ParticleKind::Choice(items) => {
            let mut each = items
                .iter()
                .map(|item| schema_required(data, standard, item))
                .collect::<Result<Vec<_>>>()?
                .into_iter();
            let first = each.next().unwrap_or_default();
            each.fold(first, |kept, item| &kept & &item)
        }
        ParticleKind::Element {
            name,
            type_ref: _type_ref,
        } => BTreeSet::from([format!(
            "{}:{}",
            prefix_of(data, &name.namespace)?,
            name.local
        )]),
        ParticleKind::GroupRef(group) => standard
            .groups
            .get(group)
            .map(|defined| schema_required(data, standard, &defined.particle))
            .transpose()?
            .unwrap_or_default(),
        ParticleKind::Any {
            namespace: _namespace,
            process_contents: _process_contents,
        } => BTreeSet::new(),
    })
}

/// Give the type `name` the schema's content model `particle` when the data's accepts another
/// order where the schema has a sequence, or leaves out a child the schema requires.
fn standard_content(
    data: &mut SdkData,
    standard: &SchemaSet,
    name: &str,
    particle: &Particle,
) -> Result<()> {
    let Some(held) = data
        .types
        .get(name)
        .and_then(|found| found.particle.as_ref())
    else {
        return Ok(());
    };
    let looser_order = matches!(particle.kind, ParticleKind::Sequence(_))
        && matches!(held.kind.as_deref(), Some("Choice" | "All"));
    let required = schema_required(data, standard, particle)?;
    let looser_children = !required.is_subset(&data_required(held));
    if !looser_order && !looser_children {
        return Ok(());
    }
    let mut leaves = Vec::new();
    collect_leaves(held, None, &mut leaves);
    let mut declared = BTreeSet::new();
    elements(data, standard, particle, &mut declared)?;
    let foreign: Vec<DataParticle> = leaves
        .iter()
        .filter(|leaf| element_of(leaf).is_none_or(|element| !declared.contains(element)))
        .cloned()
        .collect();
    let mut model = converted(data, standard, particle, &leaves)?;
    if !foreign.is_empty() {
        model = interleaved(model, foreign)?;
    }
    if let Some(found) = data.types.get_mut(name) {
        found.particle = Some(model);
    }
    Ok(())
}
