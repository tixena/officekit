//! The conformance classes' files: every node's `Strict` and
//! `Transitional` impl, one module per namespace in `officekit-strict` and
//! `officekit-transitional`, and the tables each class's write walks.
//!
//! The tables come from the comparison of the two schema sets, which says
//! what Transitional adds and what only Strict has, and from
//! `strict_correspondences.toml`, which says what each Transitional addition
//! becomes in Strict.

pub mod correspondences;
pub mod rules;
pub mod text;

use alloc::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use os_shim::{System, WriteMode};

use self::correspondences::Correspondences;
use self::rules::{AttributeRule, ChildRule, Rules};
use self::text::{Class, Impl};
use super::markup::structs::Catalog;
use super::markup::{self, Source, render};
use super::namespaces;
use super::validation::Emitted;
use crate::generate_schema::inputs::Inputs;
use crate::generate_schema::model::SchemaSet;

#[cfg(test)]
mod tests;

/// The hand-written table of correspondences, from the workspace root.
pub const CORRESPONDENCES_PATH: &str = "crates/xtask/data/strict_correspondences.toml";

/// The folder of the Strict modules, from the workspace root.
pub const STRICT_PATH: &str = "crates/officekit-strict/src";

/// The folder of the Transitional modules, from the workspace root.
pub const TRANSITIONAL_PATH: &str = "crates/officekit-transitional/src";

/// The two classes and the folder of each one's files.
const CLASSES: [(Class, &str); 2] = [
    (Class::Strict, STRICT_PATH),
    (Class::Transitional, TRANSITIONAL_PATH),
];

/// What `generate-schema` wrote of the two classes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Summary {
    /// The Strict attribute rules, by kind.
    pub attributes: BTreeMap<&'static str, usize>,
    pub children_refused: usize,
    pub children_renamed: usize,
    /// Attribute rules that depend on the element's parent.
    pub contextual: usize,
    /// Elements a Strict write refuses wherever they stand.
    pub elements: usize,
    /// Modules per class.
    pub modules: usize,
    /// Namespaces a Strict write refuses.
    pub namespaces: usize,
    /// Nodes per class.
    pub nodes: usize,
    /// Nodes that set `conformance`: the main parts' roots.
    pub roots: usize,
    /// Attributes a Transitional write refuses a value of.
    pub strict_only: usize,
}

/// Build the tables and write both classes' files.
///
/// # Errors
///
/// An input that cannot be read, a correspondence the comparison does not
/// match, a main part's root the catalog lacks, or an output that cannot be
/// written.
pub fn emit<S>(system: &S, inputs: &Inputs, emitted: &Emitted, catalog: &Catalog) -> Result<Summary>
where
    S: System + ?Sized,
{
    let text = read(system, Path::new(CORRESPONDENCES_PATH))?;
    let table = Correspondences::parse(&text, &emitted.namespaces)?;
    let transitional = SchemaSet::read(system, &inputs.transitional_schemas)?;
    let strict = SchemaSet::read(system, &inputs.strict_schemas)?;
    let built = rules::build(&transitional, &strict, &emitted.namespaces, &table)?;
    let header = header(system, inputs, &text)?;
    let prefixes: Vec<String> = emitted
        .namespaces
        .iter()
        .map(|row| row.prefix.clone())
        .collect();

    let mut order: Vec<usize> = (0..catalog.modules.len()).collect();
    order.sort_by(|left, right| {
        catalog.modules[*left]
            .name
            .cmp(&catalog.modules[*right].name)
    });
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    let mut modules: Vec<String> = Vec::new();
    let mut summary = Summary::default();
    for module in order {
        let impls = node_impls(catalog, module)?;
        if impls.is_empty() {
            continue;
        }
        summary.nodes += impls.len();
        summary.roots += impls
            .iter()
            .filter(|node| node.conformance.is_some())
            .count();
        let name = &catalog.modules[module].name;
        for (class, folder) in CLASSES {
            files.push((
                Path::new(folder).join(format!("{name}.rs")),
                text::module(&header, class, name, &prefixes, &impls),
            ));
        }
        modules.push(name.clone());
    }
    if summary.roots != 3 {
        bail!(
            "{} nodes set `conformance`, not the 3 roots of the main parts",
            summary.roots
        );
    }
    summary.modules = modules.len();
    for (class, folder) in CLASSES {
        files.push((
            Path::new(folder).join("modules.rs"),
            text::modules(&header, class, &modules),
        ));
    }
    files.push((
        Path::new(STRICT_PATH).join("rules.rs"),
        text::strict_rules(&header, &prefixes, &built),
    ));
    files.push((
        Path::new(TRANSITIONAL_PATH).join("rules.rs"),
        text::transitional_rules(&header, &prefixes, &built),
    ));
    write(system, files)?;
    count(&built, &mut summary);
    Ok(summary)
}

/// The rules of each kind, into the summary.
fn count(built: &Rules, summary: &mut Summary) {
    for rule in built.attributes.values() {
        let kind = match rule {
            AttributeRule::CharacterSet {
                codes: _codes,
                label: _label,
                to: _to,
            } => "character sets",
            AttributeRule::Child(_) => "to a child",
            AttributeRule::Iso {
                label: _label,
                to: _to,
            } => "ISO dates",
            AttributeRule::Measure {
                per_percent: _per_percent,
                per_point: _per_point,
                percent: _percent,
                unit: _unit,
            } => "measures",
            AttributeRule::Refuse(_) => "refused",
            AttributeRule::Rename(_) => "renamed",
            AttributeRule::Unpack {
                bits: _bits,
                label: _label,
            } => "unpacked",
            AttributeRule::Values(_) => "values",
        };
        *summary.attributes.entry(kind).or_default() += 1;
    }
    for rule in built.children.values() {
        match rule {
            ChildRule::Refuse(_) => summary.children_refused += 1,
            ChildRule::Rename {
                before: _before,
                to: _to,
            } => summary.children_renamed += 1,
        }
    }
    summary.contextual = built.contextual.len();
    summary.elements = built.elements.len();
    summary.namespaces = built.namespaces.len();
    summary.strict_only = built.strict_only.len();
}

/// The header of every file: the typed tree's inputs, which the catalog
/// comes from, and the correspondences.
fn header<S>(system: &S, inputs: &Inputs, correspondences: &str) -> Result<String>
where
    S: System + ?Sized,
{
    let enum_names_text = read(system, Path::new(markup::ENUM_NAMES_PATH))?;
    let overrides_text = read(system, Path::new(markup::NAME_OVERRIDES_PATH))?;
    let mut read_inputs: Vec<PathBuf> = inputs.strict_schemas.clone();
    read_inputs.extend(inputs.transitional_schemas.iter().cloned());
    read_inputs.push(inputs.part1_text.clone());
    read_inputs.push(inputs.part4_text.clone());
    read_inputs.push(inputs.sdk_data.join("namespaces.json"));
    let in_data = inputs.sdk_data.join("schemas");
    let mut schemas: Vec<PathBuf> = system
        .read_dir(&in_data)
        .with_context(|| format!("cannot read {}", in_data.display()))?
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    schemas.sort();
    read_inputs.extend(schemas);
    namespaces::header(
        inputs.checksums(),
        inputs.tools(),
        &read_inputs,
        &[
            (
                Path::new(markup::ENUM_NAMES_PATH),
                enum_names_text.as_bytes(),
            ),
            (
                Path::new(markup::NAME_OVERRIDES_PATH),
                overrides_text.as_bytes(),
            ),
            (Path::new(CORRESPONDENCES_PATH), correspondences.as_bytes()),
        ],
    )
}

/// The impl of each struct of `module`, by struct name; a main part's root
/// carries its `conformance` attribute.
fn node_impls(catalog: &Catalog, module: usize) -> Result<Vec<Impl>> {
    let names = render::functions(catalog, module)?;
    let mut impls: Vec<Impl> = Vec::new();
    for (index, node) in catalog.nodes.iter().enumerate() {
        if node.module != module {
            continue;
        }
        let function = names
            .node_functions
            .get(&index)
            .with_context(|| format!("{} has no reader", node.name))?;
        let conformance = matches!(node.key.source, Source::Xsd(_))
            .then(|| {
                node.attributes
                    .iter()
                    .find(|attribute| attribute.local == "conformance")
                    .map(|attribute| (attribute.namespace, attribute.local.clone()))
            })
            .flatten();
        impls.push(Impl {
            conformance,
            function: function.clone(),
            name: node.name.clone(),
            tag: (node.namespace, node.key.tag.local.clone()),
        });
    }
    impls.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(impls)
}

/// The text of a file.
fn read<S>(system: &S, path: &Path) -> Result<String>
where
    S: System + ?Sized,
{
    system
        .read_to_string(path)
        .with_context(|| format!("cannot read {}", path.display()))
}

/// Write each file, creating its folder.
fn write<S>(system: &S, files: Vec<(PathBuf, String)>) -> Result<()>
where
    S: System + ?Sized,
{
    for (path, contents) in files {
        if let Some(parent) = path.parent() {
            system
                .create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        system
            .write(&path, contents.as_bytes(), WriteMode::Replace)
            .with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(())
}
