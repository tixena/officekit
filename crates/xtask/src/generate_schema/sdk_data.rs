//! SDK 3.5.1's schema data, the `data/` folder of the Open XML SDK at `v3.5.1`,
//! read into a model in the data's own vocabulary.

use alloc::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use os_shim::System;
use serde_json::Value;

#[cfg(test)]
mod tests;

/// An attribute of a type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DataAttribute {
    /// `PropertyName`; empty when the data gives none, as for `o:gfxdata`.
    pub property_name: String,
    /// `QName`, `w:w`; an unprefixed attribute is `:val`.
    pub qname: String,
    /// `Type`: `Int16Value`, `EnumValue<DocumentFormat.OpenXml.Wordprocessing.TableWidthValues>`.
    pub type_name: String,
    pub validators: Vec<Validator>,
    pub version: Option<DataVersion>,
}

/// How a part reaches a child part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DataChildKind {
    /// `IsDataPartReference`: media shared by reference.
    DataPart,
    Ordinary,
    /// `IsSpecialEmbeddedPart`.
    SpecialEmbedded,
}

/// One entry of a part's `Children`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DataChildPart {
    /// `HasFixedContent`.
    pub fixed_content: bool,
    /// `IsDataPartReference` or `IsSpecialEmbeddedPart`; the data sets both on no entry.
    pub kind: DataChildKind,
    /// `MaxOccursGreatThanOne`.
    pub many: bool,
    pub name: String,
    /// `MinOccursIsNonZero`.
    pub required: bool,
}

/// A namespace of `namespaces.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DataNamespace {
    pub prefix: String,
    pub uri: String,
    /// The Office version that introduced it; `None` for ECMA's own.
    pub version: Option<DataVersion>,
}

/// A part definition of `parts/*.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DataPart {
    pub children: Vec<DataChildPart>,
    pub content_type: Option<String>,
    pub name: String,
    pub relationship_type: Option<String>,
    /// The root element's type, `w:CT_Document`.
    pub root: Option<String>,
    /// The root element's local name, `document`.
    pub root_element: Option<String>,
}

/// A content model: a composite of `items`, an `Any`, or an element when `kind` is `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DataParticle {
    pub initial_version: Option<DataVersion>,
    pub items: Vec<Self>,
    /// `Sequence`, `Choice`, `Group`, `All`, `Any`; `None` for an element.
    pub kind: Option<String>,
    /// The element type it names, `w:CT_TblWidth/w:top`.
    pub name: Option<String>,
    /// An `Any`'s namespace, `##other`.
    pub namespace: Option<String>,
    /// Each entry an alternative; empty when the data gives none, which is once exactly.
    pub occurs: Vec<Occurs>,
    pub require_filter: bool,
}

/// A type of `schemas/*.json`, `w:CT_TblCellMar/w:tblCellMar`, or a schema type alone, `w:CT_TblWidth/`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DataType {
    pub attributes: Vec<DataAttribute>,
    /// The element types of `Children`, the children the SDK declares, `w:CT_TblWidth/w:top`.
    pub children: Vec<String>,
    pub is_derived: bool,
    pub is_leaf_text: bool,
    pub name: String,
    pub particle: Option<DataParticle>,
    /// The checks of a leaf element's text.
    pub validators: Vec<Validator>,
    pub version: Option<DataVersion>,
}

/// An Office version, as the data spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DataVersion {
    Microsoft365,
    Office2007,
    Office2010,
    Office2013,
    Office2016,
    Office2019,
    Office2021,
}

/// An enumeration of a schema file's `Enums`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Enumeration {
    pub facets: Vec<Facet>,
    /// The SDK's name, `TableWidthValues`, which `EnumValue<Namespace.Name>` names.
    pub name: String,
    pub version: Option<DataVersion>,
}

/// Extension content the data places in a type of another namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Extension<'data> {
    pub content: ExtensionContent<'data>,
    /// The content's namespace prefix, `w14`.
    pub namespace: &'data str,
    /// The type it is placed in.
    pub type_name: &'data str,
    /// The attribute's `Version`, or the element's `InitialVersion`.
    pub version: Option<DataVersion>,
}

/// An attribute, or an element item of a particle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtensionContent<'data> {
    Attribute(&'data DataAttribute),
    Element(&'data DataParticle),
}

/// A member of an enumeration.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Facet {
    pub value: String,
    pub version: Option<DataVersion>,
}

/// One `Occurs` alternative; `max` 0 is unbounded, and an absent `Min` or `Max` is 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Occurs {
    pub include_version: bool,
    pub max: u32,
    pub min: u32,
    pub version: Option<DataVersion>,
}

/// Everything the generator reads from SDK 3.5.1's `data/` folder.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SdkData {
    /// By simple-type name, `w:ST_Jc`.
    pub enumerations: BTreeMap<String, Enumeration>,
    pub namespaces: Vec<DataNamespace>,
    pub parts: Vec<DataPart>,
    /// The rules of `schematrons.json`, as the data writes them.
    pub schematrons: Vec<Value>,
    /// Prefix to C# namespace, `w` to `DocumentFormat.OpenXml.Wordprocessing`.
    pub typed_namespaces: BTreeMap<String, String>,
    /// By the data's name, `w:CT_TblCellMar/w:tblCellMar`.
    pub types: BTreeMap<String, DataType>,
}

/// A validator of an attribute or of a leaf element's text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Validator {
    /// Each argument's `Type`, `Name` (absent on `OfficeVersionValidator`'s), and `Value`.
    pub arguments: Vec<(String, Option<String>, String)>,
    pub is_initial_version: bool,
    /// `IsList`: a `NumberValidator` that checks each item of a list.
    pub is_list: bool,
    pub name: String,
    /// The simple type it checks against, `w:ST_DecimalNumber`.
    pub type_name: Option<String>,
    pub union_id: Option<u8>,
    pub version: Option<DataVersion>,
}

impl SdkData {
    /// The namespaces dated Office 2010 to Microsoft 365: those with types or
    /// attributes in the data, then those without.
    #[must_use]
    pub fn extension_namespaces(&self) -> (Vec<&DataNamespace>, Vec<&DataNamespace>) {
        let mut used = BTreeSet::new();
        for (name, data_type) in &self.types {
            let (schema_type, element) = name.split_once('/').unwrap_or((name, ""));
            used.insert(prefix(schema_type));
            used.insert(prefix(element));
            used.extend(
                data_type
                    .attributes
                    .iter()
                    .map(|attribute| prefix(&attribute.qname)),
            );
        }
        self.namespaces
            .iter()
            .filter(|namespace| is_extension(namespace.version))
            .partition(|namespace| used.contains(namespace.prefix.as_str()))
    }

    /// The extension content the data places in a type: each attribute and
    /// particle element of a namespace dated Office 2010 to Microsoft 365
    /// other than the type's own.
    #[must_use]
    pub fn extensions_in(&self, ecma_type: &str) -> Vec<Extension<'_>> {
        let Some((type_name, data_type)) = self.types.get_key_value(ecma_type) else {
            return Vec::new();
        };
        let own = element_namespace(type_name);
        let placed = |namespace: &str| {
            namespace != own
                && self
                    .namespaces
                    .iter()
                    .any(|known| known.prefix == namespace && is_extension(known.version))
        };
        let mut found: Vec<Extension<'_>> = data_type
            .attributes
            .iter()
            .filter(|attribute| placed(prefix(&attribute.qname)))
            .map(|attribute| Extension {
                content: ExtensionContent::Attribute(attribute),
                namespace: prefix(&attribute.qname),
                type_name,
                version: attribute.version,
            })
            .collect();
        let mut elements = Vec::new();
        if let Some(particle) = &data_type.particle {
            collect_elements(particle, &mut elements);
        }
        found.extend(elements.into_iter().filter_map(|(item, name)| {
            let namespace = element_namespace(name);
            placed(namespace).then_some(Extension {
                content: ExtensionContent::Element(item),
                namespace,
                type_name,
                version: item.initial_version,
            })
        }));
        found
    }

    /// Read `namespaces.json`, `typed/namespaces.json`, `schemas/*.json`,
    /// `parts/*.json`, and `schematrons.json` from the data folder.
    ///
    /// # Errors
    ///
    /// A file that cannot be read or is not JSON, a field of the wrong JSON
    /// type or missing where the data always gives it, an unknown Office
    /// version, or two different definitions under one name.
    pub fn read<S>(system: &S, data: &Path) -> Result<Self>
    where
        S: System + ?Sized,
    {
        let namespaces = each(&json(system, &data.join("namespaces.json"))?, namespace)
            .context("in namespaces.json")?;

        let typed_namespaces =
            typed_namespaces(&json(system, &data.join("typed").join("namespaces.json"))?)
                .context("in typed/namespaces.json")?;

        let mut enumerations = BTreeMap::new();
        let mut types = BTreeMap::new();
        for path in json_files(system, &data.join("schemas"))? {
            read_schema(&json(system, &path)?, &mut types, &mut enumerations)
                .with_context(|| format!("in {}", path.display()))?;
        }

        let mut parts = Vec::new();
        for path in json_files(system, &data.join("parts"))? {
            parts.push(
                part(&json(system, &path)?).with_context(|| format!("in {}", path.display()))?,
            );
        }

        let schematrons = array(&json(system, &data.join("schematrons.json"))?)
            .context("in schematrons.json")?
            .to_vec();

        Ok(Self {
            enumerations,
            namespaces,
            parts,
            schematrons,
            typed_namespaces,
            types,
        })
    }
}

fn argument(entry: &Value) -> Result<(String, Option<String>, String)> {
    Ok((
        required(entry, "Type")?,
        text(entry, "Name")?,
        required(entry, "Value")?,
    ))
}

fn array(value: &Value) -> Result<&[Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .context("not a JSON array")
}

fn attribute(entry: &Value) -> Result<DataAttribute> {
    Ok(DataAttribute {
        property_name: text(entry, "PropertyName")?.unwrap_or_default(),
        qname: required(entry, "QName")?,
        type_name: required(entry, "Type")?,
        validators: list(entry, "Validators", validator)?,
        version: version(entry, "Version")?,
    })
}

fn child(entry: &Value) -> Result<String> {
    required(entry, "Name")
}

fn child_part(entry: &Value) -> Result<DataChildPart> {
    let kind = match (
        flag(entry, "IsDataPartReference")?,
        flag(entry, "IsSpecialEmbeddedPart")?,
    ) {
        (false, false) => DataChildKind::Ordinary,
        (true, false) => DataChildKind::DataPart,
        (false, true) => DataChildKind::SpecialEmbedded,
        (true, true) => bail!("both `IsDataPartReference` and `IsSpecialEmbeddedPart`"),
    };
    Ok(DataChildPart {
        fixed_content: flag(entry, "HasFixedContent")?,
        kind,
        many: flag(entry, "MaxOccursGreatThanOne")?,
        name: required(entry, "Name")?,
        required: flag(entry, "MinOccursIsNonZero")?,
    })
}

/// Every element item under `particle`, with the type it names.
fn collect_elements<'data>(
    particle: &'data DataParticle,
    found: &mut Vec<(&'data DataParticle, &'data str)>,
) {
    match (&particle.kind, &particle.name) {
        (None, Some(name)) => found.push((particle, name)),
        _ => {
            for item in &particle.items {
                collect_elements(item, found);
            }
        }
    }
}

fn data_type(entry: &Value) -> Result<DataType> {
    Ok(DataType {
        attributes: list(entry, "Attributes", attribute)?,
        children: list(entry, "Children", child)?,
        is_derived: flag(entry, "IsDerived")?,
        is_leaf_text: flag(entry, "IsLeafText")?,
        name: required(entry, "Name")?,
        particle: entry.get("Particle").map(particle).transpose()?,
        validators: list(entry, "Validators", validator)?,
        version: version(entry, "Version")?,
    })
}

/// The data's name for an entry, for error messages.
fn describe(entry: &Value) -> &str {
    entry
        .get("Name")
        .and_then(Value::as_str)
        .unwrap_or("without a name")
}

fn each<T>(value: &Value, read: fn(&Value) -> Result<T>) -> Result<Vec<T>> {
    array(value)?.iter().map(read).collect()
}

/// The namespace prefix of a type name's element, or of its schema type when it names no element.
fn element_namespace(type_name: &str) -> &str {
    match type_name.split_once('/') {
        Some((_, element)) if !element.is_empty() => prefix(element),
        Some((schema_type, _)) => prefix(schema_type),
        None => prefix(type_name),
    }
}

fn enumeration(entry: &Value) -> Result<Enumeration> {
    Ok(Enumeration {
        facets: list(entry, "Facets", facet)?,
        name: required(entry, "Name")?,
        version: version(entry, "Version")?,
    })
}

fn facet(entry: &Value) -> Result<Facet> {
    Ok(Facet {
        value: required(entry, "Value")?,
        version: version(entry, "Version")?,
    })
}

/// An absent flag is `false`.
fn flag(entry: &Value, name: &str) -> Result<bool> {
    entry.get(name).map_or(Ok(false), |value| {
        value
            .as_bool()
            .with_context(|| format!("`{name}` is not a boolean"))
    })
}

fn insert<T>(map: &mut BTreeMap<String, T>, key: String, value: T, what: &str) -> Result<()>
where
    T: PartialEq,
{
    match map.entry(key) {
        Entry::Vacant(slot) => {
            slot.insert(value);
        }
        Entry::Occupied(slot) => {
            if *slot.get() != value {
                bail!("two different {what} under `{}`", slot.key());
            }
        }
    }
    Ok(())
}

/// An absent number is `None`.
fn integer<T>(entry: &Value, name: &str) -> Result<Option<T>>
where
    T: TryFrom<u64>,
{
    entry
        .get(name)
        .map(|value| {
            value
                .as_u64()
                .and_then(|number| T::try_from(number).ok())
                .with_context(|| format!("`{name}` is not a whole number in range"))
        })
        .transpose()
}

/// Dated Office 2010 to Microsoft 365.
const fn is_extension(version: Option<DataVersion>) -> bool {
    !matches!(version, None | Some(DataVersion::Office2007))
}

fn json<S>(system: &S, path: &Path) -> Result<Value>
where
    S: System + ?Sized,
{
    let text = system
        .read_to_string(path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("{} is not JSON", path.display()))
}

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

/// An absent list is empty.
fn items<'value>(entry: &'value Value, name: &str) -> Result<&'value [Value]> {
    entry.get(name).map_or(Ok(&[]), |value| {
        array(value).with_context(|| format!("`{name}` is not a list"))
    })
}

fn list<T>(entry: &Value, name: &str, read: fn(&Value) -> Result<T>) -> Result<Vec<T>> {
    items(entry, name)?
        .iter()
        .map(read)
        .collect::<Result<_>>()
        .with_context(|| format!("in `{name}`"))
}

fn namespace(entry: &Value) -> Result<DataNamespace> {
    Ok(DataNamespace {
        prefix: required(entry, "Prefix")?,
        uri: required(entry, "Uri")?,
        version: version(entry, "Version")?,
    })
}

fn occurs(entry: &Value) -> Result<Occurs> {
    Ok(Occurs {
        include_version: flag(entry, "IncludeVersion")?,
        max: integer(entry, "Max")?.unwrap_or_default(),
        min: integer(entry, "Min")?.unwrap_or_default(),
        version: version(entry, "Version")?,
    })
}

fn part(entry: &Value) -> Result<DataPart> {
    Ok(DataPart {
        children: list(entry, "Children", child_part)?,
        content_type: text(entry, "ContentType")?,
        name: required(entry, "Name")?,
        relationship_type: text(entry, "RelationshipType")?,
        root: text(entry, "Root")?,
        root_element: text(entry, "RootElement")?,
    })
}

fn particle(entry: &Value) -> Result<DataParticle> {
    Ok(DataParticle {
        initial_version: version(entry, "InitialVersion")?,
        items: list(entry, "Items", particle)?,
        kind: text(entry, "Kind")?,
        name: text(entry, "Name")?,
        namespace: text(entry, "Namespace")?,
        occurs: list(entry, "Occurs", occurs)?,
        require_filter: flag(entry, "RequireFilter")?,
    })
}

/// The prefix of a qualified name; `""` when it has none.
fn prefix(qname: &str) -> &str {
    qname.split_once(':').map_or("", |(prefix, _)| prefix)
}

fn read_schema(
    file: &Value,
    types: &mut BTreeMap<String, DataType>,
    enumerations: &mut BTreeMap<String, Enumeration>,
) -> Result<()> {
    for entry in items(file, "Types")? {
        let read = data_type(entry).with_context(|| format!("in type `{}`", describe(entry)))?;
        insert(types, read.name.clone(), read, "types")?;
    }
    for entry in items(file, "Enums")? {
        let simple_type = required(entry, "Type")?;
        let read = enumeration(entry).with_context(|| format!("in enumeration `{simple_type}`"))?;
        insert(enumerations, simple_type, read, "enumerations")?;
    }
    Ok(())
}

fn required(entry: &Value, name: &str) -> Result<String> {
    text(entry, name)?.with_context(|| format!("`{name}` is missing"))
}

/// An absent text is `None`.
fn text(entry: &Value, name: &str) -> Result<Option<String>> {
    entry
        .get(name)
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .with_context(|| format!("`{name}` is not a string"))
        })
        .transpose()
}

fn typed_namespaces(value: &Value) -> Result<BTreeMap<String, String>> {
    let mut found = BTreeMap::new();
    for entry in array(value)? {
        let key = required(entry, "Prefix")?;
        insert(
            &mut found,
            key,
            required(entry, "Namespace")?,
            "C# namespaces",
        )?;
    }
    Ok(found)
}

fn validator(entry: &Value) -> Result<Validator> {
    Ok(Validator {
        arguments: list(entry, "Arguments", argument)?,
        is_initial_version: flag(entry, "IsInitialVersion")?,
        is_list: flag(entry, "IsList")?,
        name: required(entry, "Name")?,
        type_name: text(entry, "Type")?,
        union_id: integer(entry, "UnionId")?,
        version: version(entry, "Version")?,
    })
}

/// An absent version is `None`.
fn version(entry: &Value, name: &str) -> Result<Option<DataVersion>> {
    text(entry, name)?
        .map(|text| {
            Ok(match text.as_str() {
                "Microsoft365" => DataVersion::Microsoft365,
                "Office2007" => DataVersion::Office2007,
                "Office2010" => DataVersion::Office2010,
                "Office2013" => DataVersion::Office2013,
                "Office2016" => DataVersion::Office2016,
                "Office2019" => DataVersion::Office2019,
                "Office2021" => DataVersion::Office2021,
                _ => bail!("`{name}` is an unknown Office version, `{text}`"),
            })
        })
        .transpose()
}
