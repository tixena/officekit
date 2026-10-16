//! The typed tree (D12): one module per namespace, `markup::<namespace>`.
//!
//! Each is written to `crates/officekit-core/src/markup/<namespace>.rs` with
//! its readers, writers, and traversals beside it, from the schema model of the
//! Transitional schemas, SDK 3.5.1's extension types and placements, and the
//! standard's titles.
//!
//! Each module holds one struct per distinct pair of tag and schema type,
//! one flat enum per distinct set of tags a choice position allows, one
//! newtype per simple type with its member enum, the `From` and `TryFrom`
//! conversions between enums one of whose tag sets holds the other's, and
//! the hook traits of its traversals.

mod enums;
pub(crate) mod names;
mod readers;
pub(crate) mod render;
mod simple_types;
pub mod structs;
mod traversal;
mod writers;

use alloc::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use os_shim::System;

use super::namespaces;
use super::validation::Emitted;
use crate::generate_schema::inputs::Inputs;
use crate::generate_schema::model::{Name, SchemaSet, TypeRef};
use crate::generate_schema::sdk_data::{DataVersion, SdkData};

#[cfg(test)]
mod tests;

/// The folder of the namespace modules, from the workspace root.
pub const MARKUP_PATH: &str = "crates/officekit-core/src/markup";

/// The hand-written table of enum names, from the workspace root.
pub const ENUM_NAMES_PATH: &str = "crates/xtask/data/enum_names.toml";

/// The hand-written table of name overrides, from the workspace root.
pub const NAME_OVERRIDES_PATH: &str = "crates/xtask/data/name_overrides.toml";

/// A struct's identity: its tag and the schema type its content comes from.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct NodeKey {
    pub source: Source,
    pub tag: Name,
}

/// Where a struct's content is defined.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Source {
    /// A type of SDK 3.5.1's data, by its name there, `w14:CT_SdtCheckbox/w14:checkbox`.
    Data(String),
    /// A type of the Transitional schemas.
    Xsd(TypeRef),
}

/// SDK 3.5.1's data, with the lookups the typed tree needs.
#[non_exhaustive]
pub struct Data<'data> {
    /// The ECMA tags of the Transitional schemas, each with its types.
    pub ecma_tags: BTreeMap<Name, Vec<TypeRef>>,
    /// The prefixes of the namespaces dated Office 2010 to Microsoft 365.
    pub extension_prefixes: BTreeSet<String>,
    pub sdk: &'data SdkData,
    pub transitional: &'data SchemaSet,
    /// Prefix to URI, as `namespaces.json` gives them.
    pub uris: BTreeMap<String, String>,
}

/// What `generate-schema` wrote of the typed tree.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Summary {
    /// Enums of the `xsd:all` types.
    pub all_enums: usize,
    /// Choice enums: ECMA's, the new choices the data adds, and the extension types'.
    pub choice_enums: usize,
    /// `From` and `TryFrom` pairs between enums.
    pub conversions: usize,
    /// Structs of the Transitional schemas: one per pair of tag and type.
    pub ecma_structs: usize,
    /// Distinct qualified tags of the ECMA structs.
    pub ecma_tags: usize,
    /// ECMA choice enums the data's extension tags join.
    pub extended_enums: usize,
    /// Structs of Microsoft's extension types.
    pub extension_structs: usize,
    /// List hooks: one per enum a list holds.
    pub list_hooks: usize,
    /// Member enums of the enumeration and union newtypes.
    pub member_enums: usize,
    pub modules: usize,
    /// Simple-type newtypes.
    pub newtypes: usize,
    /// Tree hooks: one per struct and enum.
    pub tree_hooks: usize,
    /// Visitor hooks: one per struct and enum.
    pub visitor_hooks: usize,
    /// Enums of the wildcard positions.
    pub wildcard_enums: usize,
    /// Wildcard positions, `xsd:any` particles of one sequence that repeats counted once.
    pub wildcard_positions: usize,
    /// Write hooks: one per tree hook and per list hook.
    pub write_hooks: usize,
}

impl<'data> Data<'data> {
    /// Whether `prefix` is a namespace dated Office 2010 to Microsoft 365.
    #[must_use]
    pub fn is_extension(&self, prefix: &str) -> bool {
        self.extension_prefixes.contains(prefix)
    }

    /// The name `prefix:local` stands for.
    #[must_use]
    pub fn name(&self, prefix: &str, local: &str) -> Option<Name> {
        Some(Name::new(self.uri(prefix)?, local))
    }

    /// The lookups over the data and the Transitional schemas.
    #[must_use]
    pub fn new(sdk: &'data SdkData, transitional: &'data SchemaSet) -> Self {
        let mut ecma_tags: BTreeMap<Name, Vec<TypeRef>> = BTreeMap::new();
        for (tag, type_ref) in transitional.element_tags() {
            ecma_tags
                .entry(tag.clone())
                .or_default()
                .push(type_ref.clone());
        }
        Self {
            ecma_tags,
            extension_prefixes: sdk
                .namespaces
                .iter()
                .filter(|namespace| {
                    !matches!(namespace.version, None | Some(DataVersion::Office2007))
                })
                .map(|namespace| namespace.prefix.clone())
                .collect(),
            sdk,
            transitional,
            uris: sdk
                .namespaces
                .iter()
                .map(|namespace| (namespace.prefix.clone(), namespace.uri.clone()))
                .collect(),
        }
    }

    /// The struct a data type's name stands for: an ECMA tag's own pair when
    /// the schemas give the tag one type, or give it the data's type; else the
    /// data's type.
    #[must_use]
    pub fn resolve(&self, data_name: &str) -> Option<NodeKey> {
        let (type_prefix, type_local, element_prefix, element_local) = enums::split(data_name)?;
        let tag = self.name(element_prefix, element_local)?;
        let type_name = self.name(type_prefix, type_local);
        if let Some(types) = self.ecma_tags.get(&tag) {
            let named = types
                .iter()
                .find(|type_ref| type_ref.name() == type_name && type_name.is_some());
            if let Some(found) = named.or_else(|| (types.len() == 1).then(|| &types[0])) {
                return Some(NodeKey {
                    source: Source::Xsd(found.clone()),
                    tag,
                });
            }
        }
        if !self.is_extension(type_prefix)
            && let Some(name) = type_name
        {
            if self.transitional.complex_types.contains_key(&name) {
                return Some(NodeKey {
                    source: Source::Xsd(TypeRef::Complex {
                        anonymous: false,
                        name,
                    }),
                    tag,
                });
            }
            if self.transitional.simple_types.contains_key(&name) {
                return Some(NodeKey {
                    source: Source::Xsd(TypeRef::Simple {
                        anonymous: false,
                        name,
                    }),
                    tag,
                });
            }
        }
        Some(NodeKey {
            source: Source::Data(data_name.to_owned()),
            tag,
        })
    }

    /// The URI of `prefix`.
    #[must_use]
    pub fn uri(&self, prefix: &str) -> Option<&str> {
        self.uris.get(prefix).map(String::as_str)
    }
}

/// Read the inputs, build the typed tree, and write its modules; the catalog
/// comes back for the part modules.
///
/// # Errors
///
/// An input that cannot be read, a name the tables cannot give, a tag set
/// with no row in `enum_names.toml` or a row with no set, or an output that
/// cannot be written.
pub fn emit<S>(system: &S, inputs: &Inputs, emitted: &Emitted) -> Result<structs::Catalog>
where
    S: System + ?Sized,
{
    let transitional = SchemaSet::read(system, &inputs.transitional_schemas)?;
    let strict = SchemaSet::read(system, &inputs.strict_schemas)?;
    let part_1 = system
        .read_to_string(&inputs.part1_text)
        .with_context(|| format!("cannot read {}", inputs.part1_text.display()))?;
    let part_4 = system
        .read_to_string(&inputs.part4_text)
        .with_context(|| format!("cannot read {}", inputs.part4_text.display()))?;
    let enum_names_text = system
        .read_to_string(Path::new(ENUM_NAMES_PATH))
        .with_context(|| format!("cannot read {ENUM_NAMES_PATH}"))?;
    let overrides_text = system
        .read_to_string(Path::new(NAME_OVERRIDES_PATH))
        .with_context(|| format!("cannot read {NAME_OVERRIDES_PATH}"))?;
    let data = Data::new(&emitted.data, &transitional);
    let catalog = structs::Catalog::build(
        &data,
        &strict,
        &emitted.namespaces,
        &names::Titles::read(&part_1, &part_4),
        &names::Overrides::parse(&overrides_text)?,
        &enums::rows(&enum_names_text)?,
    )?;

    let mut read: Vec<PathBuf> = inputs.strict_schemas.clone();
    read.extend(inputs.transitional_schemas.iter().cloned());
    read.push(inputs.part1_text.clone());
    read.push(inputs.part4_text.clone());
    read.push(inputs.sdk_data.join("namespaces.json"));
    let mut schemas: Vec<PathBuf> = system
        .read_dir(&inputs.sdk_data.join("schemas"))
        .with_context(|| format!("cannot read {}", inputs.sdk_data.display()))?
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    schemas.sort();
    read.extend(schemas);
    let header = namespaces::header(
        inputs.checksums(),
        inputs.tools(),
        &read,
        &[
            (Path::new(ENUM_NAMES_PATH), enum_names_text.as_bytes()),
            (Path::new(NAME_OVERRIDES_PATH), overrides_text.as_bytes()),
        ],
    )?;
    let folder = Path::new(MARKUP_PATH);
    for (path, text) in render::files(system, &header, &catalog, folder)? {
        if let Some(parent) = path.parent() {
            system
                .create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        system
            .write(&path, text.as_bytes())
            .with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(catalog)
}
