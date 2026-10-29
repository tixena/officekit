//! The hand-maintained table of corrections to SDK 3.5.1's data, applied to
//! the types read before anything is emitted.

use anyhow::{Context as _, Result, bail};

use super::{DataParticle, DataType, SdkData};

#[cfg(test)]
mod tests;

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

/// Apply every row of the table of corrections to `data`.
pub(super) fn apply(data: &mut SdkData, text: &str) -> Result<()> {
    let table: toml::Table = text
        .parse()
        .context("the table of corrections is not TOML")?;
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
    Ok(())
}

fn known<'data>(data: &'data SdkData, name: &str) -> Result<&'data DataType> {
    data.types
        .get(name)
        .with_context(|| format!("the data has no `{name}`"))
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
