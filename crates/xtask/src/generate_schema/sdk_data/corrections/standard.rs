//! The elements ECMA-376 declares and SDK 3.5.1's data leaves out, built from
//! the Transitional schemas.

use anyhow::{Context as _, Result, bail};

use super::{known, text_field, texts};
use crate::generate_schema::model::{
    AttributeUse, ComplexType, Content, Name, Particle, ParticleKind, SchemaSet, TypeRef,
};
use crate::generate_schema::sdk_data::{
    DataAttribute, DataParticle, DataType, Occurs, SdkData, Validator,
};

/// New types built from a Transitional complex type each, one of them a member
/// of the choice that holds `before`, and the others placed in `parents`.
pub(super) struct FromStandard {
    before: String,
    content_of: String,
    member: String,
    parents: Vec<String>,
    types: Vec<String>,
}

impl FromStandard {
    pub(super) fn apply(&self, data: &mut SdkData, standard: &SchemaSet) -> Result<()> {
        for name in &self.types {
            if data.types.contains_key(name) {
                bail!("the data already has `{name}`");
            }
        }
        if !self.types.contains(&self.member) {
            bail!("`member` is not one of `types`: {}", self.member);
        }
        self.insert_member(data)?;
        let content = self.content(data)?;
        for name in &self.types {
            let built = build(data, standard, name, &content, &self.types)?;
            data.types.insert(name.clone(), built);
        }
        for parent in &self.parents {
            place(data, standard, parent, &self.types, (0, None))?;
        }
        Ok(())
    }

    /// The particle the data inlines for the group ECMA-376 repeats in every
    /// type it builds: the last item of `content_of`'s content model.
    fn content(&self, data: &SdkData) -> Result<DataParticle> {
        let model = known(data, &self.content_of)?
            .particle
            .as_ref()
            .with_context(|| format!("`{}` has no content model", self.content_of))?;
        model
            .items
            .last()
            .cloned()
            .with_context(|| format!("`{}` has an empty content model", self.content_of))
    }

    pub(super) fn describe(&self) -> String {
        self.member.clone()
    }

    fn insert_member(&self, data: &mut SdkData) -> Result<()> {
        known(data, &self.before)?;
        let mut found = 0_usize;
        for data_type in data.types.values_mut() {
            if let Some(particle) = &mut data_type.particle {
                found += insert_before(particle, &self.before, &self.member);
            }
        }
        if found == 0 {
            bail!("no content model holds `{}`", self.before);
        }
        Ok(())
    }

    pub(super) fn rows(rows: Vec<&toml::Table>) -> Result<Vec<Self>> {
        rows.into_iter()
            .map(|row| {
                Ok(Self {
                    before: text_field(row, "before")?,
                    content_of: text_field(row, "content_of")?,
                    member: text_field(row, "member")?,
                    parents: texts(row, "parents")?,
                    types: texts(row, "types")?,
                })
            })
            .collect()
    }
}

/// Put each of `new` that the standard declares in `parent`'s sequence ahead of
/// the first element the standard declares after it that the data's sequence
/// holds, else at the end, occurring `occurs.0` to `occurs.1` times.
pub(super) fn place(
    data: &mut SdkData,
    standard: &SchemaSet,
    parent: &str,
    new: &[String],
    occurs: (u32, Option<u32>),
) -> Result<()> {
    let (schema_type, _) = parent
        .split_once('/')
        .with_context(|| format!("`{parent}` has no `/`"))?;
    let declared = declaration(data, standard, schema_type)?;
    let Content::Particle(content) = &declared.content else {
        bail!("`{parent}` has no content model in the standard");
    };
    let ParticleKind::Sequence(elements) = &content.kind else {
        bail!("`{parent}` is not a sequence in the standard");
    };
    let mut names = Vec::new();
    for element in elements {
        let ParticleKind::Element {
            name,
            type_ref: _type_ref,
        } = &element.kind
        else {
            bail!("`{parent}` holds an item other than an element in the standard");
        };
        names.push(resolve(data, name, new).ok());
    }
    let wanted: Vec<usize> = names
        .iter()
        .enumerate()
        .filter(|(_, resolved)| resolved.as_ref().is_some_and(|found| new.contains(found)))
        .map(|(index, _)| index)
        .collect();
    if wanted.is_empty() {
        bail!("the standard places none of the new types in `{parent}`");
    }
    let target = data
        .types
        .get_mut(parent)
        .with_context(|| format!("the data has no `{parent}`"))?;
    let particle = target
        .particle
        .as_mut()
        .with_context(|| format!("`{parent}` has no content model"))?;
    for index in wanted {
        let Some(Some(name)) = names.get(index) else {
            continue;
        };
        let after = names
            .get(index + 1..)
            .into_iter()
            .flatten()
            .flatten()
            .find(|following| {
                particle
                    .items
                    .iter()
                    .any(|item| item.name.as_deref() == Some(following.as_str()))
            });
        let position = after
            .and_then(|following| {
                particle
                    .items
                    .iter()
                    .position(|item| item.name.as_deref() == Some(following.as_str()))
            })
            .unwrap_or(particle.items.len());
        particle
            .items
            .insert(position, once(name, occurs.0, occurs.1));
    }
    Ok(())
}

fn attribute(data: &SdkData, attribute: &AttributeUse, schema_type: &str) -> Result<DataAttribute> {
    let TypeRef::Simple {
        name,
        anonymous: _anonymous,
    } = &attribute.type_ref
    else {
        bail!(
            "the attribute {} of {schema_type} is not of a simple type",
            attribute.name
        );
    };
    if name.local != "ST_String" && name.local != "ST_XmlName" {
        bail!(
            "the attribute {} of {schema_type} has the type {}, which the table cannot read",
            attribute.name,
            name.local
        );
    }
    let prefix = prefix_of(data, &attribute.name.namespace)?;
    let validators = if attribute.required {
        vec![Validator {
            arguments: Vec::new(),
            is_initial_version: false,
            is_list: false,
            name: "RequiredValidator".to_owned(),
            type_name: None,
            union_id: None,
            version: None,
        }]
    } else {
        Vec::new()
    };
    Ok(DataAttribute {
        property_name: String::new(),
        qname: format!("{prefix}:{}", attribute.name.local),
        type_name: "StringValue".to_owned(),
        validators,
        version: None,
    })
}

fn build(
    data: &SdkData,
    standard: &SchemaSet,
    name: &str,
    content: &DataParticle,
    types: &[String],
) -> Result<DataType> {
    let (schema_type, _) = name
        .split_once('/')
        .with_context(|| format!("`{name}` has no `/`"))?;
    let declared = declaration(data, standard, schema_type)?;
    let attributes = declared
        .attributes
        .iter()
        .map(|use_| attribute(data, use_, schema_type))
        .collect::<Result<Vec<_>>>()?;
    let particle = match &declared.content {
        Content::Empty => None,
        Content::Particle(particle) => Some(
            convert(data, particle, content, types)
                .with_context(|| format!("in the content of {schema_type}"))?,
        ),
        Content::Simple(_) => bail!("{schema_type} holds text"),
    };
    Ok(DataType {
        attributes,
        children: Vec::new(),
        is_derived: false,
        is_leaf_text: false,
        name: name.to_owned(),
        particle,
        validators: Vec::new(),
        version: None,
    })
}

fn convert(
    data: &SdkData,
    particle: &Particle,
    content: &DataParticle,
    types: &[String],
) -> Result<DataParticle> {
    match &particle.kind {
        ParticleKind::Sequence(items) => Ok(DataParticle {
            initial_version: None,
            items: items
                .iter()
                .map(|item| convert(data, item, content, types))
                .collect::<Result<_>>()?,
            kind: Some("Sequence".to_owned()),
            name: None,
            namespace: None,
            occurs: Vec::new(),
            require_filter: false,
        }),
        ParticleKind::Element {
            name,
            type_ref: _type_ref,
        } => {
            let type_name = resolve(data, name, types)?;
            Ok(once(&type_name, particle.min, particle.max))
        }
        ParticleKind::GroupRef(_) => Ok(content.clone()),
        ParticleKind::All(_) | ParticleKind::Choice(_) => {
            bail!("a content model other than a sequence of elements and a group")
        }
        ParticleKind::Any {
            namespace: _namespace,
            process_contents: _process_contents,
        } => bail!("a wildcard in the content model"),
    }
}

fn declaration<'set>(
    data: &SdkData,
    standard: &'set SchemaSet,
    schema_type: &str,
) -> Result<&'set ComplexType> {
    let (prefix, local) = schema_type
        .split_once(':')
        .with_context(|| format!("`{schema_type}` has no `:`"))?;
    let namespace = data
        .namespaces
        .iter()
        .find(|namespace| namespace.prefix == prefix)
        .with_context(|| format!("the data has no namespace `{prefix}`"))?;
    standard
        .complex_types
        .get(&Name::new(&namespace.uri, local))
        .with_context(|| format!("the standard has no `{schema_type}`"))
}

/// Point every choice that holds `before` at `member`, ahead of it.
fn insert_before(particle: &mut DataParticle, before: &str, member: &str) -> usize {
    let mut found = 0;
    if particle.kind.as_deref() == Some("Choice")
        && let Some(position) = particle
            .items
            .iter()
            .position(|item| item.name.as_deref() == Some(before))
    {
        particle.items.insert(position, once(member, 1, Some(1)));
        found += 1;
    }
    for item in &mut particle.items {
        found += insert_before(item, before, member);
    }
    found
}

/// An element item that occurs `min` to `max` times, unbounded for `None`.
fn once(name: &str, min: u32, max: Option<u32>) -> DataParticle {
    let occurs = match (min, max) {
        (1, Some(1)) => Vec::new(),
        _ => vec![Occurs {
            include_version: false,
            max: max.unwrap_or(0),
            min,
            version: None,
        }],
    };
    DataParticle {
        initial_version: None,
        items: Vec::new(),
        kind: None,
        name: Some(name.to_owned()),
        namespace: None,
        occurs,
        require_filter: false,
    }
}

fn prefix_of<'data>(data: &'data SdkData, uri: &str) -> Result<&'data str> {
    if uri.is_empty() {
        return Ok("");
    }
    data.namespaces
        .iter()
        .find(|namespace| namespace.uri == uri)
        .map(|namespace| namespace.prefix.as_str())
        .with_context(|| format!("the data has no namespace {uri}"))
}

/// The data's type for the element the standard names: the one of the
/// `candidates` whose element it is, else the only type of the data with it.
fn resolve(data: &SdkData, element: &Name, candidates: &[String]) -> Result<String> {
    let suffix = format!(
        "/{}:{}",
        prefix_of(data, &element.namespace)?,
        element.local
    );
    if let Some(found) = candidates.iter().find(|name| name.ends_with(&suffix)) {
        return Ok(found.clone());
    }
    let mut found = data.types.keys().filter(|name| name.ends_with(&suffix));
    match (found.next(), found.next()) {
        (Some(only), None) => Ok(only.clone()),
        (None, _) => bail!("the data has no type for the element {element}"),
        (Some(_), Some(_)) => bail!("the data has several types for the element {element}"),
    }
}
