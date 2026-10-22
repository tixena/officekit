//! The typed tree: one module per namespace, `officekit_markup::<namespace>`.
//!
//! Each is written to `<crate>/src/<namespace>.rs` of the crate the graph of
//! the modules' references assigns it, `officekit-markup-base`,
//! `officekit-markup-core` or `officekit-markup-ext`, with its readers and
//! writers beside it, and its traversals to
//! `crates/officekit-traversal/src/<namespace>.rs`, from the schema model of the
//! Transitional schemas, SDK 3.5.1's extension types and placements, and the
//! standard's titles. The facade `officekit-markup` re-exports every module.
//!
//! Each module holds one struct per distinct pair of tag and schema type,
//! one flat enum per distinct set of tags a choice position allows, one
//! newtype per simple type with its member enum, the `From` and `TryFrom`
//! conversions between enums one of whose tag sets holds the other's, and
//! the hook traits of its traversals, which `officekit-markup-ext` holds for
//! every module.

mod archive;
mod enums;
pub(crate) mod names;
mod readers;
pub(crate) mod render;
mod simple_types;
pub mod structs;
mod traversal;
mod writers;

use alloc::collections::{BTreeMap, BTreeSet};
use core::iter;
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

/// The folder of the facade, `officekit-markup`, from the workspace root.
pub const MARKUP_PATH: &str = "crates/officekit-markup/src";

/// The folder of the traversal modules, from the workspace root.
pub const TRAVERSAL_PATH: &str = "crates/officekit-traversal/src";

/// The hand-written table of enum names, from the workspace root.
pub const ENUM_NAMES_PATH: &str = "crates/xtask/data/enum_names.toml";

/// The hand-written table of name overrides, from the workspace root.
pub const NAME_OVERRIDES_PATH: &str = "crates/xtask/data/name_overrides.toml";

/// A crate of the typed tree's namespace modules.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Crate {
    /// `officekit-markup-base`: the runtime, and the modules the cycle reaches.
    Base,
    /// `officekit-markup-core`: the largest cycle of modules.
    Core,
    /// `officekit-markup-ext`: every other module, the root hooks, and the
    /// hooks of base's and core's modules.
    Ext,
}

impl Crate {
    pub const ALL: [Self; 3] = [Self::Base, Self::Core, Self::Ext];

    /// The folder of its modules, from the workspace root.
    #[must_use]
    pub const fn folder(self) -> &'static str {
        match self {
            Self::Base => "crates/officekit-markup-base/src",
            Self::Core => "crates/officekit-markup-core/src",
            Self::Ext => "crates/officekit-markup-ext/src",
        }
    }

    /// Its name in a path, `officekit_markup_base`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Base => "officekit_markup_base",
            Self::Core => "officekit_markup_core",
            Self::Ext => "officekit_markup_ext",
        }
    }
}

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
    /// The namespace modules of each crate, in the order of `Crate::ALL`.
    pub crate_modules: [usize; 3],
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

/// Each module's crate, from the modules each one's items name.
///
/// Core is the largest set of modules that reach each other, base every
/// other module core reaches, and ext the rest. No module of base reaches
/// core, or it would be in the cycle, so base names no item of core or ext.
#[must_use]
pub fn assign(references: &[BTreeSet<usize>]) -> Vec<Crate> {
    let reached: Vec<BTreeSet<usize>> = (0..references.len())
        .map(|module| reach(references, module))
        .collect();
    let mut core = BTreeSet::new();
    for (module, ahead) in reached.iter().enumerate() {
        let cycle: BTreeSet<usize> = ahead
            .iter()
            .copied()
            .filter(|other| reached[*other].contains(&module))
            .collect();
        if cycle.len() > core.len() {
            core = cycle;
        }
    }
    let base: BTreeSet<usize> = core
        .iter()
        .flat_map(|module| reached[*module].iter().copied())
        .filter(|module| !core.contains(module))
        .collect();
    (0..references.len())
        .map(|module| {
            if core.contains(&module) {
                Crate::Core
            } else if base.contains(&module) {
                Crate::Base
            } else {
                Crate::Ext
            }
        })
        .collect()
}

/// The modules `module` reaches through the references, itself among them
/// when it is on a cycle.
fn reach(references: &[BTreeSet<usize>], module: usize) -> BTreeSet<usize> {
    let mut seen = BTreeSet::new();
    let mut next: Vec<usize> = references
        .get(module)
        .into_iter()
        .flatten()
        .copied()
        .collect();
    while let Some(found) = next.pop() {
        if seen.insert(found) {
            next.extend(references.get(found).into_iter().flatten().copied());
        }
    }
    seen
}

/// Remove what an earlier run wrote of the typed tree and this run does not:
/// a namespace module's files in a folder it left, and the facade's former
/// namespace modules, `modules.rs` and `root.rs`.
fn remove_stale<S>(
    system: &S,
    catalog: &structs::Catalog,
    files: &[(PathBuf, String)],
) -> Result<()>
where
    S: System + ?Sized,
{
    let written: BTreeSet<&Path> = files.iter().map(|(path, _)| path.as_path()).collect();
    for name in iter::once(MARKUP_PATH).chain(Crate::ALL.map(Crate::folder)) {
        let folder = Path::new(name);
        let mut generated = vec![folder.join("modules.rs"), folder.join("root.rs")];
        for module in &catalog.modules {
            generated.push(folder.join(format!("{}.rs", module.name)));
            generated.push(folder.join(&module.name).join("read.rs"));
            generated.push(folder.join(&module.name).join("write.rs"));
        }
        for path in generated {
            if !written.contains(path.as_path()) && system.is_file(&path).unwrap_or(false) {
                system
                    .remove_file(&path)
                    .with_context(|| format!("cannot remove {}", path.display()))?;
            }
        }
        for module in &catalog.modules {
            let sub = folder.join(&module.name);
            if system.is_dir(&sub).unwrap_or(false)
                && system
                    .read_dir(&sub)
                    .with_context(|| format!("cannot read {}", sub.display()))?
                    .is_empty()
            {
                system
                    .remove_dir_all(&sub)
                    .with_context(|| format!("cannot remove {}", sub.display()))?;
            }
        }
    }
    Ok(())
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
    let mut catalog = structs::Catalog::build(
        &data,
        &strict,
        &emitted.namespaces,
        &names::Titles::read(&part_1, &part_4),
        &names::Overrides::parse(&overrides_text)?,
        &enums::rows(&enum_names_text)?,
    )?;
    catalog.crates = assign(&render::references(&catalog)?);

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
    let mut files = render::files(system, &header, &catalog)?;
    remove_stale(system, &catalog, &files)?;
    files.extend(traversal::files(
        &header,
        &catalog,
        Path::new(TRAVERSAL_PATH),
    )?);
    for (path, text) in files {
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
