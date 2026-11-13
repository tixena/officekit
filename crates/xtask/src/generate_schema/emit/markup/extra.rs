//! `node_extra.rs` of `officekit-markup-base`: the `NodeExtra` trait, with one associated type for
//! each element struct of the typed tree, and the names those types take.

use alloc::collections::{BTreeMap, BTreeSet};
use core::fmt::Write as _;

use anyhow::{Result, bail};

use super::names;
use super::render::generic_name;
use super::structs::{Body, Catalog, Node};

/// The folder of the `officekit-markup-macros` crate's sources, from the workspace root.
pub const MACROS_FOLDER: &str = "crates/officekit-markup-macros/src";

/// The field every element struct holds for the slot, which no attribute or child may take.
const FIELD: &str = "extra";

/// The associated type of a struct: the namespace module's name and the struct's, both in
/// `UpperCamelCase`.
#[must_use]
pub fn name(catalog: &Catalog, node: &Node) -> String {
    let words: Vec<String> = catalog.modules[node.module]
        .name
        .split('_')
        .map(str::to_owned)
        .collect();
    format!("{}{}", names::pascal(&words), node.name)
}

/// Whether the catalog can be written: no two structs with one associated type name, no generic
/// type named like another type of its module, and no struct with a field named `extra`.
///
/// # Errors
///
/// The first thing that cannot.
pub fn check(catalog: &Catalog) -> Result<()> {
    let mut taken: BTreeMap<String, usize> = BTreeMap::new();
    for (index, node) in catalog.nodes.iter().enumerate() {
        let associated = name(catalog, node);
        if let Some(other) = taken.insert(associated.clone(), index) {
            bail!(
                "{} and {} both take the associated type {associated}",
                catalog.nodes[other].name,
                node.name
            );
        }
        let fields = node
            .attributes
            .iter()
            .map(|attribute| attribute.field.as_str());
        let children = match &node.body {
            Body::Children(children) => children.iter().map(|child| child.field.as_str()).collect(),
            Body::Text(_) => Vec::new(),
        };
        if fields.chain(children).any(|field| field == FIELD) {
            bail!(
                "{}::{} has a field named {FIELD}",
                catalog.modules[node.module].name,
                node.name
            );
        }
    }
    let mut types: BTreeSet<(usize, &str)> = BTreeSet::new();
    types.extend(
        catalog
            .nodes
            .iter()
            .map(|node| (node.module, node.name.as_str())),
    );
    types.extend(
        catalog
            .enums
            .iter()
            .map(|found| (found.module, found.name.as_str())),
    );
    types.extend(
        catalog
            .simples
            .iter()
            .map(|simple| (simple.module, simple.name.as_str())),
    );
    let generic = catalog
        .nodes
        .iter()
        .map(|node| (node.module, node.name.as_str()))
        .chain(
            catalog
                .enums
                .iter()
                .map(|found| (found.module, found.name.as_str())),
        );
    for (module, base) in generic {
        let with = generic_name(base);
        if types.contains(&(module, with.as_str())) {
            bail!(
                "{with} of {} is also the name of another type",
                catalog.modules[module].name
            );
        }
    }
    Ok(())
}

/// The structs with their associated types, in the alphabetical order the trait declares them.
fn ordered(catalog: &Catalog) -> Vec<(String, &Node)> {
    let mut order: Vec<(String, &Node)> = catalog
        .nodes
        .iter()
        .map(|node| (name(catalog, node), node))
        .collect();
    order.sort_by(|left, right| left.0.cmp(&right.0));
    order
}

/// The text of `kinds.rs` of `officekit-markup-macros`: the associated types of `NodeExtra`, in
/// its order.
///
/// # Errors
///
/// A catalog `check` refuses.
pub fn kinds_text(header: &str, catalog: &Catalog) -> Result<String> {
    check(catalog)?;
    let mut out = String::from(header);
    out.push_str(
        "//! The kinds `#[node_extra]` knows.\n\n\
         /// The associated types of `officekit_markup::NodeExtra`, in its order: one per element struct.\n\
         pub const KINDS: &[&str] = &[\n",
    );
    for (associated, _) in ordered(catalog) {
        let _ = writeln!(out, "    \"{associated}\",");
    }
    out.push_str("];\n");
    Ok(out)
}

/// The text of `node_extra.rs`.
///
/// # Errors
///
/// A catalog `check` refuses.
pub fn text(header: &str, catalog: &Catalog) -> Result<String> {
    check(catalog)?;
    let order = ordered(catalog);
    let mut out = String::from(header);
    out.push_str(
        "//! What each kind of node of the typed tree holds in its `extra` field.\n\n\
         use core::fmt::Debug;\n\n\
         /// A type the `extra` field of a node can hold.\n\
         pub trait ExtraSlot: Clone + Debug + Default + PartialEq {}\n\n\
         impl<T: Clone + Debug + Default + PartialEq> ExtraSlot for T {}\n\n\
         /// What each kind of node of the typed tree holds in its `extra` field: one associated type\n\
         /// for each element struct, named after the struct's namespace module and the struct.\n\
         ///\n\
         /// `()` holds nothing in every node; it is the tree a read gives.\n\
         pub trait NodeExtra: Clone + Debug + Default + PartialEq {\n",
    );
    for (associated, node) in &order {
        let _ = write!(
            out,
            "    /// What a `{}::{}With` holds.\n    type {associated}: ExtraSlot;\n",
            catalog.modules[node.module].name, node.name
        );
    }
    out.push_str("}\n\nimpl NodeExtra for () {\n");
    for (associated, _) in &order {
        let _ = writeln!(out, "    type {associated} = ();");
    }
    out.push_str("}\n");
    Ok(out)
}

#[cfg(test)]
mod tests;
