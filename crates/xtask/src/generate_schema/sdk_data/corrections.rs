//! The hand-maintained table of corrections to SDK 3.5.1's data, applied to
//! the types read before anything is emitted.

mod standard;

use core::slice;

use anyhow::{Context as _, Result, bail};

use self::standard::{FromStandard, place};
use super::{DataParticle, DataType, Facet, SdkData};
use crate::generate_schema::model::SchemaSet;

#[cfg(test)]
mod tests;

/// One attribute whose value class the data gets wrong.
struct AttributeType {
    attribute: String,
    class: Class,
    type_name: String,
}

/// What an attribute's value is read as instead.
enum Class {
    Enumeration(String),
    String,
}

/// One type the data derives from the wrong base, given the attributes of
/// another type's base.
struct BaseType {
    like: String,
    type_name: String,
}

/// Values an enumeration of the data lacks.
struct EnumerationValues {
    add: Vec<String>,
    enumeration: String,
}

/// One attribute the data qualifies and Office writes unprefixed.
struct Attribute {
    qname: String,
    replaces: String,
    type_name: String,
}

/// One extension type the data gets wrong: the `c:ext` of an extension list.
struct Extension {
    attributes_of: String,
    children: Vec<String>,
    list: String,
    replaces: String,
    type_name: String,
}

/// One element of a type the data has, which the data leaves out of the
/// parent the standard declares it in.
struct Element {
    like: String,
    parent: String,
    type_name: String,
}

/// Apply every row of the table of corrections to `data`; `standard` is the
/// Transitional schemas the rows that build types read.
pub(super) fn apply(data: &mut SdkData, text: &str, standard: &SchemaSet) -> Result<()> {
    let table: toml::Table = text
        .parse()
        .context("the table of corrections is not TOML")?;
    for row in attribute_types(&table)? {
        retype_attribute(data, &row).with_context(|| {
            format!(
                "in the correction of `{}` of `{}`",
                row.attribute, row.type_name
            )
        })?;
    }
    for row in enumeration_values(&table)? {
        add_values(data, &row)
            .with_context(|| format!("in the correction of `{}`", row.enumeration))?;
    }
    for row in base_types(&table)? {
        rebase(data, &row).with_context(|| format!("in the correction of `{}`", row.type_name))?;
    }
    for row in extensions(&table)? {
        replace_extension(data, &row)
            .with_context(|| format!("in the correction of `{}`", row.replaces))?;
    }
    for row in attributes(&table)? {
        replace_attribute(data, &row).with_context(|| {
            format!(
                "in the correction of `{}` of `{}`",
                row.replaces, row.type_name
            )
        })?;
    }
    for row in FromStandard::rows(rows(&table, "from_standard")?)? {
        row.apply(data, standard)
            .with_context(|| format!("in the correction building `{}`", row.describe()))?;
    }
    for row in elements(&table)? {
        add_element(data, standard, &row)
            .with_context(|| format!("in the correction adding `{}`", row.type_name))?;
    }
    Ok(())
}

fn known<'data>(data: &'data SdkData, name: &str) -> Result<&'data DataType> {
    data.types
        .get(name)
        .with_context(|| format!("the data has no `{name}`"))
}

fn retype_attribute(data: &mut SdkData, row: &AttributeType) -> Result<()> {
    let type_name = match &row.class {
        Class::Enumeration(name) => {
            let (prefix, _) = name
                .split_once(':')
                .with_context(|| format!("`{name}` has no `:`"))?;
            let enumeration = data
                .enumerations
                .get(name)
                .with_context(|| format!("the data has no enumeration `{name}`"))?;
            let namespace = data
                .typed_namespaces
                .get(prefix)
                .with_context(|| format!("the data has no typed namespace `{prefix}`"))?;
            format!("EnumValue<{namespace}.{}>", enumeration.name)
        }
        Class::String => "StringValue".to_owned(),
    };
    let data_type = data
        .types
        .get_mut(&row.type_name)
        .with_context(|| format!("the data has no `{}`", row.type_name))?;
    let attribute = data_type
        .attributes
        .iter_mut()
        .find(|attribute| attribute.qname == row.attribute)
        .with_context(|| format!("the type has no attribute `{}`", row.attribute))?;
    attribute.type_name = type_name;
    Ok(())
}

fn add_values(data: &mut SdkData, row: &EnumerationValues) -> Result<()> {
    let enumeration = data
        .enumerations
        .get_mut(&row.enumeration)
        .with_context(|| format!("the data has no enumeration `{}`", row.enumeration))?;
    for value in &row.add {
        if enumeration.facets.iter().any(|facet| facet.value == *value) {
            bail!("`{}` already has the value `{value}`", row.enumeration);
        }
        enumeration.facets.push(Facet {
            value: value.clone(),
            version: None,
        });
    }
    Ok(())
}

fn rebase(data: &mut SdkData, row: &BaseType) -> Result<()> {
    let like = known(data, &row.like)?;
    let mut attributes = Vec::new();
    if like.is_derived
        && let Some((schema_type, _)) = row.like.split_once('/')
        && let Some(base) = data.types.get(&format!("{schema_type}/"))
    {
        attributes.extend(base.attributes.iter().cloned());
    }
    attributes.extend(like.attributes.iter().cloned());
    let data_type = data
        .types
        .get_mut(&row.type_name)
        .with_context(|| format!("the data has no `{}`", row.type_name))?;
    data_type.attributes = attributes;
    data_type.is_derived = false;
    Ok(())
}

fn replace_attribute(data: &mut SdkData, row: &Attribute) -> Result<()> {
    let data_type = data
        .types
        .get_mut(&row.type_name)
        .with_context(|| format!("the data has no `{}`", row.type_name))?;
    let attribute = data_type
        .attributes
        .iter_mut()
        .find(|attribute| attribute.qname == row.replaces)
        .with_context(|| format!("the type has no attribute `{}`", row.replaces))?;
    attribute.qname.clone_from(&row.qname);
    Ok(())
}

fn replace_extension(data: &mut SdkData, row: &Extension) -> Result<()> {
    let template = known(data, &row.attributes_of)?;
    let attributes = template.attributes.clone();
    let mut items = Vec::new();
    for child in &row.children {
        known(data, child)?;
        items.push(DataParticle {
            initial_version: None,
            items: Vec::new(),
            kind: None,
            name: Some(child.clone()),
            namespace: None,
            occurs: Vec::new(),
            require_filter: false,
        });
    }
    if data.types.contains_key(&row.type_name) {
        bail!("the data already has `{}`", row.type_name);
    }
    if data.types.remove(&row.replaces).is_none() {
        bail!("the data has no `{}`", row.replaces);
    }
    let list = data
        .types
        .get_mut(&row.list)
        .with_context(|| format!("the data has no `{}`", row.list))?;
    let renamed = rename(list, &row.replaces, &row.type_name);
    if !renamed {
        bail!("`{}` does not hold `{}`", row.list, row.replaces);
    }
    data.types.insert(
        row.type_name.clone(),
        DataType {
            attributes,
            children: row.children.clone(),
            is_derived: false,
            is_leaf_text: false,
            name: row.type_name.clone(),
            particle: Some(DataParticle {
                initial_version: None,
                items,
                kind: Some("Choice".to_owned()),
                name: None,
                namespace: None,
                occurs: Vec::new(),
                require_filter: false,
            }),
            validators: Vec::new(),
            version: None,
        },
    );
    Ok(())
}

fn add_element(data: &mut SdkData, standard: &SchemaSet, row: &Element) -> Result<()> {
    if data.types.contains_key(&row.type_name) {
        bail!("the data already has `{}`", row.type_name);
    }
    known(data, &row.parent)?;
    let mut built = known(data, &row.like)?.clone();
    built.name.clone_from(&row.type_name);
    data.types.insert(row.type_name.clone(), built);
    place(
        data,
        standard,
        &row.parent,
        slice::from_ref(&row.type_name),
        (0, Some(1)),
    )
}

fn elements(table: &toml::Table) -> Result<Vec<Element>> {
    rows(table, "element")?
        .into_iter()
        .map(|row| {
            Ok(Element {
                like: text_field(row, "like")?,
                parent: text_field(row, "parent")?,
                type_name: text_field(row, "type")?,
            })
        })
        .collect()
}

fn attribute_types(table: &toml::Table) -> Result<Vec<AttributeType>> {
    rows(table, "attribute_type")?
        .into_iter()
        .map(|row| {
            let class = match (row.get("enumeration"), row.get("string")) {
                (Some(_), None) => Class::Enumeration(text_field(row, "enumeration")?),
                (None, Some(toml::Value::Boolean(true))) => Class::String,
                _ => bail!("an `attribute_type` row has not exactly one of `enumeration` and `string = true`"),
            };
            Ok(AttributeType {
                attribute: text_field(row, "attribute")?,
                class,
                type_name: text_field(row, "type")?,
            })
        })
        .collect()
}

fn base_types(table: &toml::Table) -> Result<Vec<BaseType>> {
    rows(table, "base_type")?
        .into_iter()
        .map(|row| {
            Ok(BaseType {
                like: text_field(row, "like")?,
                type_name: text_field(row, "type")?,
            })
        })
        .collect()
}

fn enumeration_values(table: &toml::Table) -> Result<Vec<EnumerationValues>> {
    rows(table, "enumeration_values")?
        .into_iter()
        .map(|row| {
            Ok(EnumerationValues {
                add: texts(row, "add")?,
                enumeration: text_field(row, "enumeration")?,
            })
        })
        .collect()
}

fn texts(row: &toml::Table, name: &str) -> Result<Vec<String>> {
    row.get(name)
        .and_then(toml::Value::as_array)
        .with_context(|| format!("a row has no list `{name}`"))?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .with_context(|| format!("an item of `{name}` is not text"))
        })
        .collect()
}

fn attributes(table: &toml::Table) -> Result<Vec<Attribute>> {
    rows(table, "attribute")?
        .into_iter()
        .map(|row| {
            Ok(Attribute {
                qname: text_field(row, "qname")?,
                replaces: text_field(row, "replaces")?,
                type_name: text_field(row, "type")?,
            })
        })
        .collect()
}

fn rows<'table>(table: &'table toml::Table, name: &str) -> Result<Vec<&'table toml::Table>> {
    let Some(rows) = table.get(name) else {
        return Ok(Vec::new());
    };
    rows.as_array()
        .with_context(|| format!("`{name}` is not an array of tables"))?
        .iter()
        .map(|row| {
            row.as_table()
                .with_context(|| format!("a `{name}` row is not a table"))
        })
        .collect()
}

fn text_field(row: &toml::Table, name: &str) -> Result<String> {
    row.get(name)
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("a row has no text `{name}`"))
}

fn extensions(table: &toml::Table) -> Result<Vec<Extension>> {
    rows(table, "extension")?
        .into_iter()
        .map(|row| {
            let children = row
                .get("children")
                .and_then(toml::Value::as_array)
                .context("an `extension` row has no list `children`")?
                .iter()
                .map(|child| {
                    child
                        .as_str()
                        .map(str::to_owned)
                        .context("a child of an `extension` row is not text")
                })
                .collect::<Result<_>>()?;
            Ok(Extension {
                attributes_of: text_field(row, "attributes_of")?,
                children,
                list: text_field(row, "list")?,
                replaces: text_field(row, "replaces")?,
                type_name: text_field(row, "type")?,
            })
        })
        .collect()
}

/// Point the list's child and particle item named `from` at `to`, for every
/// version; `false` when the list holds no `from`.
fn rename(list: &mut DataType, from: &str, to: &str) -> bool {
    fn particle_items(particle: &mut DataParticle, from: &str, to: &str) -> bool {
        let mut found = particle.name.as_deref() == Some(from);
        if found {
            particle.name = Some(to.to_owned());
            particle.initial_version = None;
        }
        for item in &mut particle.items {
            found |= particle_items(item, from, to);
        }
        found
    }
    let mut found = false;
    for child in &mut list.children {
        if child == from {
            to.clone_into(child);
            found = true;
        }
    }
    if let Some(particle) = &mut list.particle {
        found |= particle_items(particle, from, to);
    }
    found
}
