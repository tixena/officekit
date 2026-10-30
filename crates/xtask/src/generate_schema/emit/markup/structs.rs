//! The catalog of the typed tree: every struct with its fields, every enum
//! with its arms, every simple-type newtype, and their names.

use alloc::collections::{BTreeMap, BTreeSet};
use core::cmp::{Ordering, Reverse};
use core::mem;

use anyhow::{Context as _, Result, bail};
use officekit_markup::Unknown;
use officekit_xml::element::Node as XmlNode;

use super::enums::{self, Entry, Placements, Position, Row};
use super::names::{self, Heading, Overrides, Titles};
use super::simple_types::{self, Simple};
use super::{Crate, Data, NodeKey, Source, Summary};
use crate::generate_schema::compare;
use crate::generate_schema::emit::namespaces::Row as NamespaceRow;
use crate::generate_schema::model::{
    AttributeUse, Content, Name, Particle, ParticleKind, SchemaSet, TypeRef,
};
use crate::generate_schema::sdk_data::{DataAttribute, DataParticle, DataType, ExtensionContent};

/// clippy's largest difference in bytes between an enum's two largest arms
/// (`large_enum_variant`).
const VARIANT_SIZE_DIFFERENCE: usize = 200;

/// The names the generated modules use, Rust's prelude's and the hook traits':
/// a struct or a simple type of one of them takes another name, as a
/// collision does.
const RESERVED: [&str; 21] = [
    "AsRef",
    "Box",
    "Clone",
    "Context",
    "Copy",
    "Debug",
    "Default",
    "Eq",
    "Err",
    "From",
    "None",
    "Ok",
    "Option",
    "PartialEq",
    "Result",
    "Self",
    "Some",
    "String",
    "TryFrom",
    "Vec",
    "Visitor",
];

/// An attribute field of a struct.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Attribute {
    pub field: String,
    pub local: String,
    /// The namespace id; `None` for an unqualified attribute.
    pub namespace: Option<u8>,
    pub value: Value,
}

/// What a struct holds besides its attributes.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Body {
    /// Declared children, in schema order.
    Children(Vec<Child>),
    /// Text: simple content, or an element of a simple type.
    Text(Value),
}

/// Everything the typed tree's modules hold.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Catalog {
    /// Enum pairs `(smaller, larger)`: every arm of the first is an arm of the second.
    pub conversions: Vec<(usize, usize)>,
    /// Each module's crate, by its index.
    pub crates: Vec<Crate>,
    pub enums: Vec<EnumDef>,
    /// Extended ECMA enums, for the summary.
    pub extended: usize,
    pub modules: Vec<Module>,
    pub nodes: Vec<Node>,
    /// The namespace table's prefixes; a prefix's index is its namespace id.
    pub prefixes: Vec<String>,
    pub simples: Vec<Simple>,
    /// Wildcard positions, for the summary.
    pub wildcard_positions: usize,
}

/// A child field of a struct.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Child {
    pub field: String,
    pub occurs: Occurs,
    pub target: Target,
}

/// An enum: its arms, sorted by variant, and how it reads.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct EnumDef {
    /// `(variant, struct)`, alphabetical.
    pub arms: Vec<(String, usize)>,
    /// The structs its arms hold boxed, as clippy's `large_enum_variant`
    /// asks of an arm much larger than every other.
    pub boxed: BTreeSet<usize>,
    /// Whether a required single position uses it, so it gets `Default`.
    pub default: bool,
    pub doc: String,
    pub kind: EnumKind,
    pub module: usize,
    pub name: String,
    /// The tags the enum reads: its arms', in the order of `arms`.
    pub wildcard: Option<Wildcard>,
}

/// Where an enum's set comes from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EnumKind {
    /// The children of an `xsd:all` type, each allowed once.
    All,
    /// A choice position.
    Choice,
    /// A wildcard position.
    Wildcard,
}

/// A namespace module.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Module {
    pub id: u8,
    pub name: String,
    pub prefix: String,
    pub uri: String,
}

/// A struct: one tag with one schema type.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Node {
    pub attributes: Vec<Attribute>,
    pub body: Body,
    pub doc: String,
    pub key: NodeKey,
    pub module: usize,
    pub name: String,
    /// The namespace id of its tag.
    pub namespace: Option<u8>,
    /// Where the standard titles it: the part, 1 or 4, and the section.
    pub section: Option<(u8, String)>,
    pub words: Vec<String>,
}

/// How many children a field holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Occurs {
    List,
    Optional,
    Required,
}

/// What a child field holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Target {
    Enum(usize),
    Node(usize),
}

/// The text of an attribute or of simple content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Value {
    /// A simple type's newtype.
    Simple(usize),
    /// The text as it is: a built-in type, or a string any text is a value of.
    Text,
}

/// The namespaces a wildcard position allows.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Wildcard {
    Any,
    Local,
    Not(u8),
    Only(Vec<u8>),
}

/// A child position before its field is named.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Slot {
    All(BTreeSet<Entry>),
    Any(Vec<(String, String)>, Occurs),
    Choice(BTreeSet<Entry>, Occurs),
    Element(NodeKey, Occurs),
}

/// The walk's context: whether an enclosing particle repeats or may be absent.
#[derive(Clone, Copy, Debug, Default)]
struct Context {
    optional: bool,
    repeated: bool,
}

/// What `Catalog::build` works with.
struct Builder<'data> {
    catalog: Catalog,
    data: &'data Data<'data>,
    /// Each enum's full set of entries.
    enum_entries: Vec<BTreeSet<Entry>>,
    /// Each enum's owner types, for the table.
    enum_owners: Vec<Vec<(Name, String)>>,
    /// An ECMA set, plain or extended, to its enum.
    enum_sets: BTreeMap<BTreeSet<Entry>, usize>,
    /// Each enum's sources, for the table.
    enum_sources: Vec<Vec<String>>,
    global_elements: Vec<NodeKey>,
    keys: BTreeMap<NodeKey, usize>,
    module_by_uri: BTreeMap<String, usize>,
    placements: Placements,
    /// Every name the rules cannot give, reported together.
    problems: Vec<String>,
    rows: &'data [NamespaceRow],
    simple_index: BTreeMap<simple_types::Key, usize>,
    /// The slots of each node, before naming.
    slots: Vec<Vec<Slot>>,
    strict_values: BTreeMap<Name, Vec<String>>,
    /// Each node's child positions, resolved to what they hold.
    targets: Vec<Vec<(Target, Occurs)>>,
}

impl Catalog {
    /// Box the arms clippy's `large_enum_variant` would flag: while an enum's
    /// largest arm is more than `VARIANT_SIZE_DIFFERENCE` bytes larger than
    /// the next, that arm holds its struct boxed.
    fn box_large_arms(&mut self) {
        let sizes: Vec<usize> = self.nodes.iter().map(struct_size).collect();
        for found in &mut self.enums {
            let mut variants: Vec<(usize, Option<usize>)> = found
                .arms
                .iter()
                .map(|(_, arm)| (sizes.get(*arm).copied().unwrap_or(0), Some(*arm)))
                .collect();
            variants.push((mem::size_of::<XmlNode>(), None));
            found.boxed = large_variants(variants);
        }
    }

    /// The structs among `arms` that an enum with one variant per arm holds
    /// boxed, by the rule `box_large_arms` boxes the typed tree's arms with.
    #[must_use]
    pub fn boxed_arms(&self, arms: &[usize]) -> BTreeSet<usize> {
        large_variants(
            arms.iter()
                .map(|arm| (self.nodes.get(*arm).map_or(0, struct_size), Some(*arm)))
                .collect(),
        )
    }

    /// Build the catalog: structs, enums, newtypes, and their names.
    ///
    /// # Errors
    ///
    /// A name the rules and tables cannot give, a collision, a set with no
    /// row of `enum_names.toml`, or a row with no set.
    pub fn build(
        data: &Data<'_>,
        strict: &SchemaSet,
        rows: &[NamespaceRow],
        titles: &Titles,
        overrides: &Overrides,
        enum_rows: &[Row],
    ) -> Result<Self> {
        let mut builder = Builder::new(data, strict, rows);
        builder.register_enums();
        builder.register_nodes()?;
        builder.resolve_slots()?;
        builder.name_nodes(titles, overrides)?;
        builder.name_enums(enum_rows)?;
        builder.name_simples(titles, overrides)?;
        builder.disambiguate();
        simple_types::name_variants(&mut builder.catalog.simples);
        builder.name_fields()?;
        builder.check_names();
        if !builder.problems.is_empty() {
            bail!(
                "the typed tree's names:\n  {}",
                builder.problems.join("\n  ")
            );
        }
        builder.mark_defaults()?;
        builder.conversions();
        builder.catalog.box_large_arms();
        Ok(builder.catalog)
    }

    fn count(&self, kind: EnumKind) -> usize {
        self.enums.iter().filter(|found| found.kind == kind).count()
    }

    /// Whether a node's tag is in a namespace of the Transitional schemas.
    fn ecma(&self, node: &Node) -> bool {
        self.modules
            .get(node.module)
            .is_some_and(|module| !module.uri.starts_with("http://schemas.microsoft.com/"))
    }

    /// The prefix of each namespace id.
    #[must_use]
    pub fn namespace_rows(&self) -> &[String] {
        &self.prefixes
    }

    /// The counts `generate-schema` prints.
    #[must_use]
    pub fn summary(&self) -> Summary {
        let ecma: Vec<&Node> = self
            .nodes
            .iter()
            .filter(|node| matches!(node.key.source, Source::Xsd(_)) && self.ecma(node))
            .collect();
        let tags: BTreeSet<&Name> = ecma.iter().map(|node| &node.key.tag).collect();
        let tree_hooks = self.nodes.len() + self.enums.len();
        let list_hooks = super::traversal::listed(self).len();
        Summary {
            all_enums: self.count(EnumKind::All),
            choice_enums: self.count(EnumKind::Choice),
            conversions: self.conversions.len(),
            crate_modules: Crate::ALL.map(|krate| {
                self.crates
                    .iter()
                    .filter(|module| **module == krate)
                    .count()
            }),
            ecma_structs: ecma.len(),
            ecma_tags: tags.len(),
            extended_enums: self.extended,
            extension_structs: self.nodes.len() - ecma.len(),
            list_hooks,
            member_enums: self
                .simples
                .iter()
                .filter(|simple| simple.has_member())
                .count(),
            modules: self.modules.len(),
            newtypes: self.simples.len(),
            tree_hooks,
            visitor_hooks: tree_hooks,
            wildcard_enums: self.count(EnumKind::Wildcard),
            wildcard_positions: self.wildcard_positions,
            write_hooks: tree_hooks + list_hooks,
        }
    }
}

impl<'data> Builder<'data> {
    fn add_owner(&mut self, index: usize, owner: &Name, source: &str) {
        if let Some(owners) = self.enum_owners.get_mut(index)
            && !owners.iter().any(|(known, _)| known == owner)
        {
            owners.push((owner.clone(), source.to_owned()));
        }
        if let Some(sources) = self.enum_sources.get_mut(index)
            && !sources.iter().any(|known| known == source)
        {
            sources.push(source.to_owned());
        }
    }

    /// The global elements of the Transitional schemas a wildcard allows.
    fn allowed_globals(&self, wildcard: &Wildcard) -> Vec<NodeKey> {
        self.global_elements
            .iter()
            .filter(|key| {
                let id = self.namespace_id(&key.tag.namespace).ok().flatten();
                match wildcard {
                    Wildcard::Any => true,
                    Wildcard::Local => id.is_none(),
                    Wildcard::Not(target) => id.is_some_and(|found| found != *target),
                    Wildcard::Only(ids) => id.is_some_and(|found| ids.contains(&found)),
                }
            })
            .cloned()
            .collect()
    }

    /// Variant names, and the checks no collision passes.
    fn check_names(&mut self) {
        for index in 0..self.catalog.enums.len() {
            let module = self.catalog.enums[index].module;
            let mut arms: Vec<(String, usize)> = Vec::new();
            for (_, arm) in self.catalog.enums[index].arms.clone() {
                let node = &self.catalog.nodes[arm];
                arms.push((node.name.clone(), arm));
            }
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for (name, _) in &arms {
                *counts.entry(name.clone()).or_default() += 1;
            }
            for (name, arm) in &mut arms {
                let node = &self.catalog.nodes[*arm];
                if counts.get(name).copied().unwrap_or(0) > 1 || name == "Other" {
                    let module_name = &self.catalog.modules[node.module].name;
                    if node.module != module || name == "Other" {
                        let mut words = names::title_words(&module_name.replace('_', " "));
                        words.extend(node.words.iter().cloned());
                        *name = names::pascal(&words);
                    }
                }
            }
            arms.sort();
            let repeated = arms.windows(2).find_map(|pair| match pair {
                [left, right] if left.0 == right.0 => Some(left.0.clone()),
                _ => None,
            });
            if let Some(variant) = repeated {
                self.problems.push(format!(
                    "two arms of the enum {} are named {variant}",
                    self.catalog.enums[index].name
                ));
            }
            if let Some(found) = self.catalog.enums.get_mut(index) {
                found.arms = arms;
            }
        }
        let mut seen: BTreeMap<(usize, String), String> = BTreeMap::new();
        let mut problems: Vec<String> = Vec::new();
        let mut claim = |module: usize, name: &str, what: String| {
            if let Some(earlier) = seen.insert((module, name.to_owned()), what.clone()) {
                problems.push(format!("{what} and {earlier} are both named {name}"));
            }
        };
        for node in &self.catalog.nodes {
            claim(
                node.module,
                &node.name,
                format!("the struct of {}", node.doc),
            );
        }
        for found in &self.catalog.enums {
            claim(
                found.module,
                &found.name,
                format!("the enum {}", found.name),
            );
        }
        for simple in &self.catalog.simples {
            claim(
                simple.module,
                &simple.name,
                format!("the newtype of {}", simple.xsd_name()),
            );
            if simple.has_member() {
                claim(
                    simple.module,
                    &format!("{}Member", simple.name),
                    format!("the member enum of {}", simple.xsd_name()),
                );
            }
        }
        problems.extend(field_problems(&self.catalog.nodes));
        self.problems.extend(problems);
    }

    /// A wildcard's namespaces as `enum_names.toml` writes them.
    fn constraint(&self, wildcard: &Wildcard) -> String {
        let prefix = |id: &u8| {
            self.catalog
                .prefixes
                .get(usize::from(*id))
                .cloned()
                .unwrap_or_default()
        };
        match wildcard {
            Wildcard::Any => "##any".to_owned(),
            Wildcard::Local => "##local".to_owned(),
            Wildcard::Not(id) => format!("##other {}", prefix(id)),
            Wildcard::Only(ids) => ids.iter().map(prefix).collect::<Vec<_>>().join(" "),
        }
    }

    /// A struct's attributes, its body, and its child positions.
    fn content(&mut self, key: &NodeKey) -> Result<(Vec<Attribute>, Body, Vec<Slot>)> {
        match &key.source {
            Source::Xsd(TypeRef::Complex {
                anonymous: _anonymous,
                name,
            }) => self.xsd_content(key, name),
            Source::Xsd(type_ref) => {
                let value = self.xsd_value(type_ref)?;
                Ok((Vec::new(), Body::Text(value), Vec::new()))
            }
            Source::Data(name) => self.data_content(name),
        }
    }

    /// The `From` and `TryFrom` pairs: every arm of one enum is an arm of another.
    fn conversions(&mut self) {
        let sets: Vec<BTreeSet<usize>> = self
            .catalog
            .enums
            .iter()
            .map(|found| found.arms.iter().map(|(_, arm)| *arm).collect())
            .collect();
        for (smaller, small) in sets.iter().enumerate() {
            if small.is_empty() {
                continue;
            }
            for (larger, large) in sets.iter().enumerate() {
                if smaller != larger && small.len() < large.len() && small.is_subset(large) {
                    self.catalog.conversions.push((smaller, larger));
                }
            }
        }
    }

    fn data_attribute(&mut self, attribute: &DataAttribute) -> Result<Attribute> {
        let (prefix, local) = attribute
            .qname
            .split_once(':')
            .unwrap_or(("", attribute.qname.as_str()));
        let namespace = if prefix.is_empty() {
            None
        } else {
            let uri = self
                .data
                .uri(prefix)
                .with_context(|| format!("the prefix of {} has no namespace", attribute.qname))?
                .to_owned();
            self.namespace_id(&uri)?
        };
        let value = self.data_value(&attribute.type_name)?;
        Ok(Attribute {
            field: String::new(),
            local: local.to_owned(),
            namespace,
            value,
        })
    }

    fn data_content(&mut self, name: &str) -> Result<(Vec<Attribute>, Body, Vec<Slot>)> {
        let data = self.data;
        let data_type: &DataType = data
            .sdk
            .types
            .get(name)
            .with_context(|| format!("no type {name} in the data"))?;
        let mut attributes = Vec::new();
        for attribute in &data_type.attributes {
            attributes.push(self.data_attribute(attribute)?);
        }
        if data_type.is_leaf_text {
            return Ok((attributes, Body::Text(Value::Text), Vec::new()));
        }
        let mut slots = Vec::new();
        if let Some(particle) = &data_type.particle {
            data_slots(data, particle, Context::default(), &mut slots);
        }
        Ok((attributes, Body::Children(Vec::new()), slots))
    }

    /// The data's name for an ECMA struct's type and tag, `w:CT_P/w:p`.
    fn data_name(&self, key: &NodeKey) -> Option<String> {
        let Source::Xsd(TypeRef::Complex {
            anonymous: false,
            name,
        }) = &key.source
        else {
            return None;
        };
        let type_prefix = self.prefix(&name.namespace)?;
        let element_prefix = self.prefix(&key.tag.namespace)?;
        Some(format!(
            "{type_prefix}:{}/{element_prefix}:{}",
            name.local, key.tag.local
        ))
    }

    /// The value of an attribute of the data, by its class: an enumeration's
    /// newtype, `ST_OnOff`'s for `OnOffValue`, else the text.
    fn data_value(&mut self, class: &str) -> Result<Value> {
        if class == "OnOffValue" {
            let shared = "http://schemas.openxmlformats.org/officeDocument/2006/sharedTypes";
            return self.simple(&simple_types::Key::Xsd(Name::new(shared, "ST_OnOff")));
        }
        let Some(inner) = class
            .strip_prefix("EnumValue<")
            .and_then(|rest| rest.strip_suffix('>'))
        else {
            return Ok(Value::Text);
        };
        let (namespace, name) = inner.rsplit_once('.').unwrap_or(("", inner));
        let data = self.data;
        let found = data
            .sdk
            .enumerations
            .iter()
            .find(|(simple_type, enumeration)| {
                enumeration.name == name
                    && simple_type.split_once(':').is_some_and(|(prefix, _)| {
                        data.sdk.typed_namespaces.get(prefix).map(String::as_str) == Some(namespace)
                    })
            })
            .map(|(simple_type, _)| simple_type.clone());
        let Some(simple_type) = found else {
            return Ok(Value::Text);
        };
        let (prefix, local) = simple_type
            .split_once(':')
            .context("an enumeration's type has no prefix")?;
        if !data.is_extension(prefix)
            && let Some(xsd_name) = data.name(prefix, local)
            && data.transitional.simple_types.contains_key(&xsd_name)
        {
            return self.simple(&simple_types::Key::Xsd(xsd_name));
        }
        self.simple(&simple_types::Key::Data(simple_type))
    }

    /// The collision rule, for the collisions the override table does not
    /// cover: within one module, the struct or simple type whose local name
    /// is the name keeps it (a struct before a simple type); else the only
    /// struct; a struct that loses takes its local name after the name, a
    /// simple type `St` and its local name. Structs of one tag tell apart by
    /// their schema type, the one the standard titles first keeping the name.
    fn disambiguate(&mut self) {
        for _ in 0..4_u8 {
            let mut groups: BTreeMap<(usize, String), Vec<(bool, usize)>> = BTreeMap::new();
            for (index, node) in self.catalog.nodes.iter().enumerate() {
                groups
                    .entry((node.module, node.name.clone()))
                    .or_default()
                    .push((true, index));
            }
            for (index, simple) in self.catalog.simples.iter().enumerate() {
                groups
                    .entry((simple.module, simple.name.clone()))
                    .or_default()
                    .push((false, index));
            }
            let collisions: Vec<(String, Vec<(bool, usize)>)> = groups
                .into_iter()
                .filter(|((_, name), members)| {
                    members.len() > 1 || RESERVED.contains(&name.as_str())
                })
                .map(|((_, name), members)| (name, members))
                .collect();
            if collisions.is_empty() {
                return;
            }
            for (name, members) in collisions {
                self.rename(&name, &members);
            }
        }
    }

    /// An enum's module, the types whose children it is as the table writes
    /// them, and its sources.
    fn enum_identity(&self, index: usize) -> Result<(usize, Vec<String>, Vec<String>)> {
        let owners = &self.enum_owners[index];
        let sources = &self.enum_sources[index];
        let (first_owner, _) = owners
            .first()
            .with_context(|| format!("the enum {index} has no owner"))?;
        let group_source = sources
            .iter()
            .find(|source| !source.contains(' '))
            .and_then(|group| {
                self.data
                    .transitional
                    .groups
                    .keys()
                    .find(|name| &name.local == group)
                    .map(|name| name.namespace.clone())
            });
        let one_namespace = owners
            .iter()
            .all(|(owner, _)| owner.namespace == first_owner.namespace);
        let home_uri = if one_namespace {
            first_owner.namespace.clone()
        } else {
            group_source.unwrap_or_else(|| first_owner.namespace.clone())
        };
        let module = *self
            .module_by_uri
            .get(&home_uri)
            .with_context(|| format!("no module for {home_uri}"))?;
        let mut types: Vec<String> = Vec::new();
        for (owner, _) in owners {
            let derived = if self.data.transitional.complex_types.contains_key(owner) {
                enums::with_derived(self.data.transitional, owner)
            } else {
                vec![owner.clone()]
            };
            for name in derived {
                let shown = if name.namespace == home_uri {
                    name.local
                } else {
                    format!(
                        "{}:{}",
                        self.prefix(&name.namespace).unwrap_or("?"),
                        name.local
                    )
                };
                if !types.contains(&shown) {
                    types.push(shown);
                }
            }
        }
        Ok((module, types, sources.clone()))
    }

    /// The enum of an `xsd:all` type's children, registered on first use.
    fn extra_enum(&mut self, kind: EnumKind, set: &BTreeSet<Entry>, owner: usize) -> Result<usize> {
        if let Some(found) = self.enum_sets.get(set) {
            return Ok(*found);
        }
        let index = self.new_enum(kind, set.clone());
        self.enum_sets.insert(set.clone(), index);
        let type_name = self
            .owner_type(owner)
            .context("an xsd:all type with no name")?;
        self.add_owner(
            index,
            &type_name,
            &format!("xsd:all in {}", type_name.local),
        );
        self.set_arms(index, set)?;
        Ok(index)
    }

    /// The field of a child: the child's name without the parent's; an enum's field is `content`
    /// when it is the struct's only one.
    fn field_name(&mut self, parent: usize, target: Target, enum_slots: usize) -> Result<String> {
        let node = &self.catalog.nodes[parent];
        let words = match target {
            Target::Node(child) => {
                names::field_words(&node.words, &self.catalog.nodes[child].words)
            }
            Target::Enum(_) if enum_slots == 1 => vec!["content".to_owned()],
            Target::Enum(found) => names::field_words(
                &node.words,
                &names::name_words(&self.catalog.enums[found].name),
            ),
        };
        if words.is_empty() {
            let child = match target {
                Target::Node(child) => format!(
                    "{} ({:?})",
                    self.catalog.nodes[child].name, self.catalog.nodes[child].key
                ),
                Target::Enum(found) => self.catalog.enums[found].name.clone(),
            };
            self.problems.push(format!(
                "the field of the child {child} of {} ({:?}) is empty once the parent's name is dropped",
                node.name, node.key
            ));
            return Ok("empty".to_owned());
        }
        names::identifier(&names::snake_words(&words))
            .with_context(|| format!("a child field of {}", node.name))
    }

    /// Enums used at a required single position get `Default`.
    fn mark_defaults(&mut self) -> Result<()> {
        let mut required: BTreeSet<usize> = BTreeSet::new();
        for node in &self.catalog.nodes {
            if let Body::Children(children) = &node.body {
                for child in children {
                    if let (Target::Enum(found), Occurs::Required) = (child.target, child.occurs) {
                        required.insert(found);
                    }
                }
            }
        }
        for found in required {
            let entry = self
                .catalog
                .enums
                .get_mut(found)
                .context("a required enum out of range")?;
            if entry.arms.is_empty() {
                bail!(
                    "the enum {} is required but has no arm to default to",
                    entry.name
                );
            }
            entry.default = true;
        }
        Ok(())
    }

    /// The module of a namespace, created on first use.
    fn module(&mut self, uri: &str) -> Result<usize> {
        if let Some(found) = self.module_by_uri.get(uri) {
            return Ok(*found);
        }
        let (id, row) = self
            .rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.uri == uri)
            .with_context(|| format!("the namespace {uri} has no row in the namespace table"))?;
        let name = if let Some(file) = self.data.transitional.namespaces.get(uri) {
            let stem = file
                .path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .with_context(|| format!("the schema file of {uri} has no name"))?;
            names::ecma_module(stem)
        } else {
            names::extension_module(uri)
                .with_context(|| format!("the namespace {uri} gives no module name"))?
        };
        let index = self.catalog.modules.len();
        self.catalog.modules.push(Module {
            id: u8::try_from(id).context("a namespace id past u8")?,
            name,
            prefix: row.prefix.clone(),
            uri: uri.to_owned(),
        });
        self.module_by_uri.insert(uri.to_owned(), index);
        Ok(index)
    }

    /// Every enum's name, from the row of `enum_names.toml` its set matches.
    fn name_enums(&mut self, rows: &[Row]) -> Result<()> {
        let mut matched: Vec<bool> = vec![false; rows.len()];
        let mut failures: Vec<String> = Vec::new();
        for index in 0..self.catalog.enums.len() {
            let (module, types, sources) = self.enum_identity(index)?;
            let tags: BTreeSet<String> = self.catalog.enums[index]
                .arms
                .iter()
                .filter_map(|(_, arm)| self.qualified(*arm))
                .collect();
            let wildcard = (self.catalog.enums[index].kind == EnumKind::Wildcard)
                .then(|| {
                    self.catalog.enums[index]
                        .wildcard
                        .as_ref()
                        .map(|found| self.constraint(found))
                })
                .flatten();
            let module_name = &self.catalog.modules[module].name;
            let found_rows = matching_rows(
                rows,
                module_name,
                [&types, &sources],
                &tags,
                wildcard.as_ref(),
            );
            let &[chosen] = found_rows.as_slice() else {
                failures.push(format!(
                    "{} rows for the enum of module {module_name} (wildcard {}, types {}, sources {}, tags {})",
                    found_rows.len(),
                    wildcard.unwrap_or_default(),
                    types.join(", "),
                    sources.join(", "),
                    tags.iter().cloned().collect::<Vec<_>>().join(" "),
                ));
                continue;
            };
            if matched[chosen] {
                failures.push(format!(
                    "the row {} of enum_names.toml matches two enums",
                    rows[chosen].name
                ));
                continue;
            }
            matched[chosen] = true;
            let home = if wildcard.is_some() {
                self.catalog
                    .modules
                    .iter()
                    .position(|found| found.name == rows[chosen].module)
                    .with_context(|| format!("no module {}", rows[chosen].module))?
            } else {
                module
            };
            let doc = enum_doc(&types, &rows[chosen].source, wildcard.is_some());
            if let Some(found) = self.catalog.enums.get_mut(index) {
                found.name.clone_from(&rows[chosen].name);
                found.module = home;
                found.doc = doc;
            }
        }
        for (row_index, found) in matched.iter().enumerate() {
            if !found {
                failures.push(format!(
                    "the row {} of module {} in enum_names.toml matches no set",
                    rows[row_index].name, rows[row_index].module
                ));
            }
        }
        if !failures.is_empty() {
            bail!("enum_names.toml:\n  {}", failures.join("\n  "));
        }
        Ok(())
    }

    /// Every field's name, and each struct's body from its positions.
    fn name_fields(&mut self) -> Result<()> {
        for index in 0..self.catalog.nodes.len() {
            let targets = self.targets[index].clone();
            let enum_slots = targets
                .iter()
                .filter(|(target, _)| matches!(target, Target::Enum(_)))
                .count();
            let mut children: Vec<Child> = Vec::new();
            for (target, occurs) in targets {
                let field = self.field_name(index, target, enum_slots)?;
                children.push(Child {
                    field,
                    occurs,
                    target,
                });
            }
            let node = &self.catalog.nodes[index];
            let mut attributes = node.attributes.clone();
            for attribute in &mut attributes {
                attribute.field = names::identifier(&names::snake(&attribute.local))
                    .with_context(|| format!("an attribute of {}", node.name))?;
            }
            self.separate_fields(index, &mut attributes, &mut children)?;
            let body = match &node.body {
                Body::Text(value) => Body::Text(*value),
                Body::Children(_) => Body::Children(children),
            };
            if let Some(node_mut) = self.catalog.nodes.get_mut(index) {
                node_mut.attributes = attributes;
                node_mut.body = body;
            }
        }
        Ok(())
    }

    /// Every struct's name: its title, an override winning; an extension
    /// type's local name.
    fn name_nodes(&mut self, titles: &Titles, overrides: &Overrides) -> Result<()> {
        let mut types_of: BTreeMap<Name, BTreeSet<String>> = BTreeMap::new();
        for node in &self.catalog.nodes {
            types_of
                .entry(node.key.tag.clone())
                .or_default()
                .insert(type_local(&node.key));
        }
        let mut used: BTreeSet<usize> = BTreeSet::new();
        for index in 0..self.catalog.nodes.len() {
            let node = &self.catalog.nodes[index];
            let module = &self.catalog.modules[node.module];
            let type_local = type_local(&node.key);
            let file = self
                .data
                .transitional
                .namespaces
                .get(&node.key.tag.namespace)
                .and_then(|file| file.path.file_stem())
                .and_then(|stem| stem.to_str())
                .map(str::to_owned);
            let siblings = types_of.get(&node.key.tag).cloned().unwrap_or_default();
            let heading = file.as_deref().and_then(|stem| {
                title_of(titles, stem, &node.key.tag.local, &type_local, &siblings)
            });
            let local_words = names::local_words(&node.key.tag.local);
            let (words, doc, section) = heading.map_or_else(
                || {
                    let source = if file.is_some() {
                        ""
                    } else {
                        ", declared by the schema data of the Open XML SDK, version 3.5.1"
                    };
                    (
                        local_words.clone(),
                        format!(
                            "The `{}:{}` element{source}. Its schema type is `{type_local}`.",
                            module.prefix, node.key.tag.local
                        ),
                        None,
                    )
                },
                |found| {
                    (
                        names::title_words(&found.title),
                        format!(
                            "The `{}:{}` element, {} (ECMA-376 Part {}, section {}). Its schema type is `{type_local}`.",
                            module.prefix,
                            node.key.tag.local,
                            found.title,
                            found.part,
                            found.section
                        ),
                        Some((found.part, found.section.clone())),
                    )
                },
            );
            let overridden = overrides.elements.iter().enumerate().find(|(_, row)| {
                row.prefix == module.prefix
                    && row.local == node.key.tag.local
                    && row.type_local == type_local
            });
            let (name, final_words) = if let Some((row_index, row)) = overridden {
                used.insert(row_index);
                (row.name.clone(), names::name_words(&row.name))
            } else {
                (names::pascal(&words), words)
            };
            if let Some(node_mut) = self.catalog.nodes.get_mut(index) {
                node_mut.name = name;
                node_mut.words = final_words;
                node_mut.doc = doc;
                node_mut.section = section;
            }
        }
        if let Some((_, unused)) = overrides
            .elements
            .iter()
            .enumerate()
            .find(|(index, _)| !used.contains(index))
        {
            bail!(
                "the override {}:{} of type {} names no struct",
                unused.prefix,
                unused.local,
                unused.type_local
            );
        }
        Ok(())
    }

    fn name_simples(&mut self, titles: &Titles, overrides: &Overrides) -> Result<()> {
        for index in 0..self.catalog.simples.len() {
            let module = self.catalog.simples[index].module;
            let prefix = self.catalog.modules[module].prefix.clone();
            let file = self
                .data
                .transitional
                .namespaces
                .get(self.catalog.simples[index].namespace())
                .and_then(|file| file.path.file_stem())
                .and_then(|stem| stem.to_str())
                .map(str::to_owned);
            simple_types::name(
                &mut self.catalog.simples[index],
                file.as_deref(),
                &prefix,
                titles,
                overrides,
            )?;
        }
        Ok(())
    }

    /// The namespace id of a URI; `None` for no namespace.
    fn namespace_id(&self, uri: &str) -> Result<Option<u8>> {
        if uri.is_empty() {
            return Ok(None);
        }
        let id = self
            .rows
            .iter()
            .position(|row| row.uri == uri || row.strict_uri.as_deref() == Some(uri))
            .with_context(|| format!("the namespace {uri} has no row in the namespace table"))?;
        Ok(Some(u8::try_from(id).context("a namespace id past u8")?))
    }

    fn new(data: &'data Data<'data>, strict: &SchemaSet, rows: &'data [NamespaceRow]) -> Self {
        let mut strict_values: BTreeMap<Name, Vec<String>> = BTreeMap::new();
        for (owner, value) in compare::strict_only(data.transitional, strict).enumeration_values {
            strict_values.entry(owner).or_default().push(value);
        }
        let global_elements = data
            .transitional
            .elements
            .values()
            .map(|element| NodeKey {
                source: Source::Xsd(element.type_ref.clone()),
                tag: element.name.clone(),
            })
            .collect();
        let catalog = Catalog {
            prefixes: rows.iter().map(|row| row.prefix.clone()).collect(),
            ..Catalog::default()
        };
        Self {
            catalog,
            data,
            enum_entries: Vec::new(),
            enum_owners: Vec::new(),
            enum_sets: BTreeMap::new(),
            enum_sources: Vec::new(),
            global_elements,
            keys: BTreeMap::new(),
            module_by_uri: BTreeMap::new(),
            placements: Placements::default(),
            rows,
            simple_index: BTreeMap::new(),
            slots: Vec::new(),
            strict_values,
            targets: Vec::new(),
            problems: Vec::new(),
        }
    }

    fn new_enum(&mut self, kind: EnumKind, entries: BTreeSet<Entry>) -> usize {
        let index = self.catalog.enums.len();
        self.catalog.enums.push(EnumDef {
            arms: Vec::new(),
            boxed: BTreeSet::new(),
            default: false,
            doc: String::new(),
            kind,
            module: usize::MAX,
            name: String::new(),
            wildcard: None,
        });
        self.enum_entries.push(entries);
        self.enum_owners.push(Vec::new());
        self.enum_sources.push(Vec::new());
        index
    }

    /// The schema type of a struct, as a name.
    fn owner_type(&self, node: usize) -> Option<Name> {
        match &self.catalog.nodes.get(node)?.key.source {
            Source::Xsd(type_ref) => type_ref.name(),
            Source::Data(name) => {
                let (type_prefix, type_local, _, _) = enums::split(name)?;
                self.data.name(type_prefix, type_local)
            }
        }
    }

    /// The slots with the extension fields and new choices the data places
    /// in the owner type, each after the ECMA tag before it.
    fn placed_slots(&self, owner: &Name, mut slots: Vec<Slot>) -> Vec<Slot> {
        let mut inserts: Vec<(Option<Name>, Slot)> = Vec::new();
        if let Some(fields) = self.placements.fields.get(owner) {
            for placed in fields {
                if let Some(key) = &placed.key {
                    let occurs = occurs_of(placed.many, placed.optional);
                    inserts.push((placed.after.clone(), Slot::Element(key.clone(), occurs)));
                }
            }
        }
        for (position, placed) in &self.placements.positions {
            if &position.owner != owner {
                continue;
            }
            let occurs = occurs_of(placed.many, true);
            let slot = if position.set.len() > 1 {
                Slot::Choice(position.set.clone(), occurs)
            } else if let Some(Entry::Element(key)) = position.set.iter().next() {
                Slot::Element(key.clone(), occurs)
            } else {
                continue;
            };
            inserts.push((placed.after.clone(), slot));
        }
        let mut last_at: BTreeMap<usize, usize> = BTreeMap::new();
        for (after, slot) in inserts {
            if slots.contains(&slot) {
                continue;
            }
            let anchor = after
                .as_ref()
                .and_then(|tag| slots.iter().rposition(|known| slot_holds(known, tag)));
            let at = anchor.map_or(0, |found| found + 1);
            let shift = last_at.get(&at).copied().unwrap_or(0);
            slots.insert(at + shift, slot);
            *last_at.entry(at).or_default() += 1;
        }
        slots
    }

    fn prefix(&self, uri: &str) -> Option<&str> {
        self.data
            .sdk
            .namespaces
            .iter()
            .find(|namespace| namespace.uri == uri)
            .map(|namespace| namespace.prefix.as_str())
    }

    /// `prefix:local` of a struct's tag.
    fn qualified(&self, node: usize) -> Option<String> {
        let found = self.catalog.nodes.get(node)?;
        let prefix = self.prefix(&found.key.tag.namespace)?;
        Some(format!("{prefix}:{}", found.key.tag.local))
    }

    /// The enums of the choice positions: ECMA's with the data's extension
    /// tags, the new choices the data adds to ECMA types, and the choices of
    /// the extension types.
    fn register_enums(&mut self) {
        let positions = enums::ecma_positions(self.data.transitional);
        self.placements = enums::placements(self.data, &positions);
        let mut plain: Vec<(BTreeSet<Entry>, Vec<&Position>)> = Vec::new();
        for position in positions.iter().filter(|position| position.set.len() > 1) {
            if let Some((_, users)) = plain.iter_mut().find(|(set, _)| *set == position.set) {
                users.push(position);
            } else {
                plain.push((position.set.clone(), vec![position]));
            }
        }
        for (set, users) in &plain {
            let mut full = set.clone();
            if let Some(added) = self.placements.added.get(set) {
                for tags in added.values() {
                    full.extend(tags.iter().cloned());
                }
                self.catalog.extended += 1;
            }
            let index = self.new_enum(EnumKind::Choice, full.clone());
            for user in users {
                self.add_owner(index, &user.owner, &user.source);
            }
            self.enum_sets.insert(set.clone(), index);
            self.enum_sets.insert(full, index);
        }
        let new_positions: Vec<Position> = self
            .placements
            .positions
            .iter()
            .map(|(position, _)| position.clone())
            .collect();
        for position in new_positions
            .iter()
            .filter(|position| position.set.len() > 1)
        {
            let index = if let Some(found) = self.enum_sets.get(&position.set) {
                *found
            } else {
                let created = self.new_enum(EnumKind::Choice, position.set.clone());
                self.enum_sets.insert(position.set.clone(), created);
                created
            };
            self.add_owner(index, &position.owner, &position.source);
        }
        let data = self.data;
        for (name, data_type) in &data.sdk.types {
            let Some((type_prefix, type_local, _, _)) = enums::split(name) else {
                continue;
            };
            if name.ends_with('/') || !data.is_extension(type_prefix) {
                continue;
            }
            let Some(particle) = &data_type.particle else {
                continue;
            };
            let Some(owner) = data.name(type_prefix, type_local) else {
                continue;
            };
            for set in enums::data_positions(data, particle) {
                if set.len() < 2 {
                    continue;
                }
                let index = if let Some(found) = self.enum_sets.get(&set) {
                    *found
                } else {
                    let created = self.new_enum(EnumKind::Choice, set.clone());
                    self.enum_sets.insert(set, created);
                    created
                };
                self.add_owner(index, &owner, &format!("choice in {type_local}"));
            }
        }
    }

    /// Every struct: the ECMA pairs, the extension types of the data, and
    /// every struct their content reaches.
    fn register_nodes(&mut self) -> Result<()> {
        let data = self.data;
        let mut queue: Vec<NodeKey> = data
            .transitional
            .element_tags()
            .into_iter()
            .map(|(tag, type_ref)| NodeKey {
                source: Source::Xsd(type_ref.clone()),
                tag: tag.clone(),
            })
            .collect();
        for name in data.sdk.types.keys() {
            let Some((_, _, element_prefix, _)) = enums::split(name) else {
                continue;
            };
            if !name.ends_with('/')
                && data.is_extension(element_prefix)
                && let Some(key) = data.resolve(name)
            {
                queue.push(key);
            }
        }
        let entries: Vec<Entry> = self.enum_entries.iter().flatten().cloned().collect();
        for entry in entries {
            if let Entry::Element(key) = entry {
                queue.push(key);
            }
        }
        let mut next = 0;
        while next < queue.len() {
            let key = queue[next].clone();
            next += 1;
            if self.keys.contains_key(&key) {
                continue;
            }
            let module = self.module(&key.tag.namespace)?;
            let namespace = self.namespace_id(&key.tag.namespace)?;
            let index = self.catalog.nodes.len();
            self.keys.insert(key.clone(), index);
            self.catalog.nodes.push(Node {
                attributes: Vec::new(),
                body: Body::Children(Vec::new()),
                doc: String::new(),
                key: key.clone(),
                module,
                name: String::new(),
                namespace,
                section: None,
                words: Vec::new(),
            });
            let (attributes, body, slots) = self.content(&key)?;
            for slot in &slots {
                match slot {
                    Slot::Element(child, _) => queue.push(child.clone()),
                    Slot::All(set) | Slot::Choice(set, _) => {
                        for entry in set {
                            if let Entry::Element(child) = entry {
                                queue.push(child.clone());
                            }
                        }
                    }
                    Slot::Any(_, _) => queue.extend(self.global_elements.iter().cloned()),
                }
            }
            if let Some(node) = self.catalog.nodes.get_mut(index) {
                node.attributes = attributes;
                node.body = body;
            }
            self.slots.push(slots);
        }
        for (index, set) in self.enum_entries.clone().iter().enumerate() {
            self.set_arms(index, set)?;
        }
        Ok(())
    }

    fn rename(&mut self, name: &str, members: &[(bool, usize)]) {
        let tags: BTreeSet<&Name> = members
            .iter()
            .filter(|(node, _)| *node)
            .map(|(_, index)| &self.catalog.nodes[*index].key.tag)
            .collect();
        let all_nodes = members.iter().all(|(node, _)| *node);
        if all_nodes && tags.len() == 1 {
            let mut ordered: Vec<usize> = members.iter().map(|(_, index)| *index).collect();
            ordered.sort_by(|left, right| {
                let section = |index: &usize| self.catalog.nodes[*index].section.clone();
                match (section(left), section(right)) {
                    (Some((left_part, left_section)), Some((right_part, right_section))) => {
                        left_part
                            .cmp(&right_part)
                            .then_with(|| names::section_order(&left_section, &right_section))
                    }
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => type_local(&self.catalog.nodes[*left].key)
                        .cmp(&type_local(&self.catalog.nodes[*right].key)),
                }
            });
            let keeper = ordered
                .first()
                .copied()
                .filter(|first| self.catalog.nodes[*first].section.is_some())
                .filter(|_| !RESERVED.contains(&name));
            for index in ordered {
                if Some(index) == keeper {
                    continue;
                }
                if let Some(node) = self.catalog.nodes.get_mut(index) {
                    let type_name = type_local(&node.key);
                    let trimmed = type_name.strip_prefix("CT_").unwrap_or(&type_name);
                    node.words
                        .extend(names::name_words(&names::pascal(&[trimmed.to_owned()])));
                    node.name = names::pascal(&node.words);
                }
            }
            return;
        }
        let local_of = |(node, index): &(bool, usize)| -> String {
            if *node {
                names::pascal(&[self.catalog.nodes[*index].key.tag.local.clone()])
            } else {
                let local = self.catalog.simples[*index].xsd_name();
                names::pascal(&[local.strip_prefix("ST_").unwrap_or(local).to_owned()])
            }
        };
        let qualifying: Vec<(bool, usize)> = members
            .iter()
            .copied()
            .filter(|member| local_of(member) == name)
            .collect();
        let qualifying_nodes: Vec<(bool, usize)> = qualifying
            .iter()
            .copied()
            .filter(|(node, _)| *node)
            .collect();
        let nodes: Vec<(bool, usize)> = members.iter().copied().filter(|(node, _)| *node).collect();
        let keeper = match (
            qualifying.as_slice(),
            qualifying_nodes.as_slice(),
            nodes.as_slice(),
        ) {
            ([only], _, _) | (_, [only], _) | ([], [], [only]) => Some(*only),
            _ => None,
        }
        .filter(|_| !RESERVED.contains(&name));
        let locals: Vec<String> = members.iter().map(local_of).collect();
        for (member, local) in members.iter().zip(locals) {
            if Some(*member) == keeper {
                continue;
            }
            let (is_node, index) = *member;
            if is_node {
                if let Some(node) = self.catalog.nodes.get_mut(index) {
                    node.words.extend(names::name_words(&local));
                    node.name = names::pascal(&node.words);
                }
                continue;
            }
            if let Some(simple) = self.catalog.simples.get_mut(index) {
                simple.name = format!("St{local}");
            }
        }
    }

    /// Each struct's child positions, resolved to the struct or enum they
    /// hold; the enums of the `xsd:all` types and the wildcards registered.
    fn resolve_slots(&mut self) -> Result<()> {
        for index in 0..self.catalog.nodes.len() {
            let slots = self.slots[index].clone();
            let mut resolved = Vec::with_capacity(slots.len());
            for slot in slots {
                resolved.push(match slot {
                    Slot::Element(key, occurs) => {
                        let child = *self
                            .keys
                            .get(&key)
                            .with_context(|| format!("no struct for {key:?}"))?;
                        (Target::Node(child), occurs)
                    }
                    Slot::Choice(set, occurs) => {
                        let found = *self.enum_sets.get(&set).with_context(|| {
                            format!(
                                "no enum for a choice of {:?}",
                                self.catalog.nodes[index].key
                            )
                        })?;
                        (Target::Enum(found), occurs)
                    }
                    Slot::All(set) => (
                        Target::Enum(self.extra_enum(EnumKind::All, &set, index)?),
                        Occurs::List,
                    ),
                    Slot::Any(constraints, occurs) => {
                        let allowed = self.wildcard(&constraints)?;
                        let found = self.wildcard_enum(&allowed, index)?;
                        self.catalog.wildcard_positions += 1;
                        (Target::Enum(found), occurs)
                    }
                });
            }
            self.targets.push(resolved);
        }
        Ok(())
    }

    /// Fields of one name tell apart: one from a namespace other than the
    /// struct's takes its prefix in front (`r_id`, `w14_doc_id`); an
    /// attribute that still shares its name with a child takes `_attribute`
    /// after it.
    fn separate_fields(
        &self,
        index: usize,
        attributes: &mut [Attribute],
        children: &mut [Child],
    ) -> Result<()> {
        let own = self.catalog.nodes[index].namespace;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for field in attributes
            .iter()
            .map(|attribute| &attribute.field)
            .chain(children.iter().map(|child| &child.field))
        {
            *counts.entry(field.clone()).or_default() += 1;
        }
        let shared = |field: &str| counts.get(field).copied().unwrap_or(0) > 1;
        let prefixed = |namespace: Option<u8>, field: &str| -> Result<Option<String>> {
            let Some(id) = namespace.filter(|id| Some(*id) != own) else {
                return Ok(None);
            };
            let prefix = self
                .catalog
                .prefixes
                .get(usize::from(id))
                .context("a namespace id out of the table")?;
            let bare = field.strip_prefix("r#").unwrap_or(field);
            Ok(Some(names::identifier(&format!(
                "{}_{bare}",
                names::snake(prefix)
            ))?))
        };
        for attribute in attributes.iter_mut() {
            if shared(&attribute.field)
                && let Some(renamed) = prefixed(attribute.namespace, &attribute.field)?
            {
                attribute.field = renamed;
            }
        }
        for child in children.iter_mut() {
            let namespace = match child.target {
                Target::Node(found) => self.catalog.nodes[found].namespace,
                Target::Enum(_) => own,
            };
            if shared(&child.field)
                && let Some(renamed) = prefixed(namespace, &child.field)?
            {
                child.field = renamed;
            }
        }
        let child_fields: BTreeSet<String> =
            children.iter().map(|child| child.field.clone()).collect();
        for attribute in attributes.iter_mut() {
            if child_fields.contains(&attribute.field) {
                let bare = attribute
                    .field
                    .strip_prefix("r#")
                    .unwrap_or(&attribute.field);
                attribute.field = format!("{bare}_attribute");
            }
        }
        Ok(())
    }

    /// An enum's arms: the structs of its elements, and the global elements
    /// its wildcards allow.
    fn set_arms(&mut self, index: usize, entries: &BTreeSet<Entry>) -> Result<()> {
        let mut arms: BTreeSet<usize> = BTreeSet::new();
        let mut wildcards: Vec<(String, String)> = Vec::new();
        for entry in entries {
            match entry {
                Entry::Element(key) => {
                    arms.insert(
                        *self
                            .keys
                            .get(key)
                            .with_context(|| format!("no struct for {key:?}"))?,
                    );
                }
                Entry::Any { constraint, target } => {
                    wildcards.push((constraint.clone(), target.clone()));
                }
            }
        }
        let wildcard = if wildcards.is_empty() {
            None
        } else {
            let allowed = self.wildcard(&wildcards)?;
            for key in self.allowed_globals(&allowed) {
                if let Some(found) = self.keys.get(&key) {
                    arms.insert(*found);
                }
            }
            Some(allowed)
        };
        if let Some(found) = self.catalog.enums.get_mut(index) {
            found.arms = arms.into_iter().map(|arm| (String::new(), arm)).collect();
            found.wildcard = wildcard;
        }
        Ok(())
    }

    /// The newtype of a simple type, registered on first use; `Value::Text`
    /// for a string any text is a value of.
    fn simple(&mut self, key: &simple_types::Key) -> Result<Value> {
        if let Some(found) = self.simple_index.get(key) {
            return Ok(Value::Simple(*found));
        }
        let Some(mut simple) = simple_types::describe(self.data, key, &self.strict_values)? else {
            return Ok(Value::Text);
        };
        let uri = simple.namespace().to_owned();
        simple.module = self.module(&uri)?;
        let index = self.catalog.simples.len();
        self.simple_index.insert(key.clone(), index);
        self.catalog.simples.push(simple);
        let referenced = self
            .catalog
            .simples
            .get(index)
            .map_or_default(Simple::references);
        for other in referenced {
            self.simple(&other)?;
        }
        Ok(Value::Simple(index))
    }

    /// The namespaces a list of `xsd:any` constraints allows, together.
    fn wildcard(&self, constraints: &[(String, String)]) -> Result<Wildcard> {
        let mut only: BTreeSet<u8> = BTreeSet::new();
        for (constraint, target) in constraints {
            match constraint.as_str() {
                "##any" => return Ok(Wildcard::Any),
                "##local" if constraints.len() == 1 => return Ok(Wildcard::Local),
                "##other" if constraints.len() == 1 => {
                    let id = self
                        .namespace_id(target)?
                        .context("a ##other wildcard with no target namespace")?;
                    return Ok(Wildcard::Not(id));
                }
                list => {
                    for uri in list.split_whitespace() {
                        let resolved = if uri == "##targetNamespace" {
                            target.as_str()
                        } else {
                            uri
                        };
                        if let Some(id) = self.namespace_id(resolved)? {
                            only.insert(id);
                        }
                    }
                }
            }
        }
        Ok(Wildcard::Only(only.into_iter().collect()))
    }

    /// The enum of a wildcard position, one per set of allowed namespaces.
    fn wildcard_enum(&mut self, allowed: &Wildcard, owner: usize) -> Result<usize> {
        let set: BTreeSet<Entry> = BTreeSet::from([Entry::Any {
            constraint: format!("{allowed:?}"),
            target: String::new(),
        }]);
        let type_name = self
            .owner_type(owner)
            .context("a wildcard in a type with no name")?;
        let index = if let Some(found) = self.enum_sets.get(&set) {
            *found
        } else {
            let created = self.new_enum(EnumKind::Wildcard, set.clone());
            self.enum_sets.insert(set, created);
            let arms: BTreeSet<usize> = self
                .allowed_globals(allowed)
                .iter()
                .filter_map(|key| self.keys.get(key).copied())
                .collect();
            if let Some(found) = self.catalog.enums.get_mut(created) {
                found.arms = arms.into_iter().map(|arm| (String::new(), arm)).collect();
                found.wildcard = Some(allowed.clone());
            }
            created
        };
        self.add_owner(
            index,
            &type_name,
            &format!("xsd:any in {}", type_name.local),
        );
        Ok(index)
    }

    fn xsd_attribute(&mut self, attribute: &AttributeUse) -> Result<Attribute> {
        Ok(Attribute {
            field: String::new(),
            local: attribute.name.local.clone(),
            namespace: self.namespace_id(&attribute.name.namespace)?,
            value: self.xsd_value(&attribute.type_ref)?,
        })
    }

    fn xsd_content(
        &mut self,
        key: &NodeKey,
        type_name: &Name,
    ) -> Result<(Vec<Attribute>, Body, Vec<Slot>)> {
        let transitional = self.data.transitional;
        let declared = transitional
            .complex_types
            .get(type_name)
            .with_context(|| format!("no complex type {type_name}"))?;
        let complex = transitional.effective(declared);
        let mut attributes = Vec::new();
        for attribute in &complex.attributes {
            attributes.push(self.xsd_attribute(attribute)?);
        }
        let data_name = self.data_name(key);
        if let Some(name) = &data_name {
            for extension in self.data.sdk.extensions_in(name) {
                if let ExtensionContent::Attribute(found) = extension.content {
                    let attribute = self.data_attribute(found)?;
                    if !attributes.iter().any(|known: &Attribute| {
                        known.local == attribute.local && known.namespace == attribute.namespace
                    }) {
                        attributes.push(attribute);
                    }
                }
            }
        }
        match &complex.content {
            Content::Simple(type_ref) => {
                let value = self.xsd_value(type_ref)?;
                Ok((attributes, Body::Text(value), Vec::new()))
            }
            Content::Empty => {
                let slots = self.placed_slots(type_name, Vec::new());
                Ok((attributes, Body::Children(Vec::new()), slots))
            }
            Content::Particle(particle) => {
                let mut slots = Vec::new();
                walk_slots(
                    transitional,
                    particle,
                    Context::default(),
                    &type_name.namespace,
                    &mut slots,
                );
                let placed = self.placed_slots(type_name, slots);
                Ok((attributes, Body::Children(Vec::new()), placed))
            }
        }
    }

    /// The value of an attribute or text of an XSD type.
    fn xsd_value(&mut self, type_ref: &TypeRef) -> Result<Value> {
        let TypeRef::Simple {
            anonymous: false,
            name,
        } = type_ref
        else {
            return Ok(Value::Text);
        };
        let key = simple_types::Key::Xsd(name.clone());
        self.simple(&key)
    }
}

/// Whether a slot holds an element of `tag`.
fn slot_holds(slot: &Slot, tag: &Name) -> bool {
    match slot {
        Slot::Element(key, _) => &key.tag == tag,
        Slot::All(set) | Slot::Choice(set, _) => set
            .iter()
            .any(|entry| matches!(entry, Entry::Element(key) if &key.tag == tag)),
        Slot::Any(_, _) => false,
    }
}

/// The structs of `variants`, each a variant's size and the struct it holds
/// (`None` for one that is never boxed), to hold boxed: while the largest
/// variant is more than `VARIANT_SIZE_DIFFERENCE` bytes larger than the next,
/// its struct is boxed.
fn large_variants(mut variants: Vec<(usize, Option<usize>)>) -> BTreeSet<usize> {
    let mut boxed = BTreeSet::new();
    loop {
        variants.sort_by_key(|variant| Reverse(variant.0));
        let (Some(&(largest, arm)), Some(&(second, _))) = (variants.first(), variants.get(1))
        else {
            break;
        };
        let Some(found) = arm.filter(|_| largest - second > VARIANT_SIZE_DIFFERENCE) else {
            break;
        };
        boxed.insert(found);
        if let Some(first) = variants.first_mut() {
            first.0 = mem::size_of::<Box<u8>>();
        }
    }
    boxed
}

/// The bytes a generated struct takes: its unknown content, a `String` per
/// attribute or text value, a `Vec` per list, and a `Box` per single child.
fn struct_size(node: &Node) -> usize {
    let attributes = node.attributes.len() * mem::size_of::<Option<String>>();
    let body = match &node.body {
        Body::Text(_) => mem::size_of::<String>(),
        Body::Children(children) => children
            .iter()
            .map(|child| match child.occurs {
                Occurs::List => mem::size_of::<Vec<u8>>(),
                Occurs::Optional | Occurs::Required => mem::size_of::<Box<u8>>(),
            })
            .sum(),
    };
    mem::size_of::<Unknown>() + attributes + body
}

/// The rows of `enum_names.toml` an enum matches: by its wildcard's
/// constraint, else by its module, its types, and the tag a row says it
/// holds; among several, those whose source names one of the enum's.
fn matching_rows(
    rows: &[Row],
    module_name: &str,
    [types, sources]: [&[String]; 2],
    tags: &BTreeSet<String>,
    wildcard: Option<&String>,
) -> Vec<usize> {
    let candidates: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| match (wildcard, &row.wildcard) {
            (Some(constraint), Some(listed)) => constraint == listed,
            (None, None) => {
                row.module == module_name
                    && same_types(&row.types, types)
                    && row.contains.as_ref().is_none_or(|tag| tags.contains(tag))
            }
            (Some(_), None) | (None, Some(_)) => false,
        })
        .map(|(row_index, _)| row_index)
        .collect();
    if candidates.len() > 1 {
        candidates
            .into_iter()
            .filter(|row_index| {
                sources
                    .iter()
                    .any(|source| rows[*row_index].source.contains(source))
            })
            .collect()
    } else {
        candidates
    }
}

/// An enum's doc: the wildcard position it reads, or the choice it is with its row's source, the
/// schema types that hold it, and what `Other` is.
fn enum_doc(types: &[String], source: &str, wildcard: bool) -> String {
    if wildcard {
        return format!("One element of a wildcard position: {source}, or `Other`.");
    }
    let shown: Vec<String> = types.iter().map(|name| format!("`{name}`")).collect();
    format!(
        "One child element of a choice: {source}.\n\nThe schema types that hold it: {}.\n\n`Other` \
         holds an element or node the schemas do not declare at this position, in its place.",
        shown.join(", ")
    )
}

/// The structs two of whose fields take one name.
fn field_problems(nodes: &[Node]) -> Vec<String> {
    let mut problems = Vec::new();
    for node in nodes {
        let mut fields: BTreeSet<&str> = BTreeSet::new();
        fields.insert("unknown");
        for attribute in &node.attributes {
            if !fields.insert(attribute.field.as_str()) {
                problems.push(format!(
                    "two fields of {} are named {}",
                    node.name, attribute.field
                ));
            }
        }
        match &node.body {
            Body::Children(children) => {
                for child in children {
                    if !fields.insert(child.field.as_str()) {
                        problems.push(format!(
                            "two fields of {} ({:?}) are named {}: a child's field equals another field",
                            node.name, node.key, child.field
                        ));
                    }
                }
            }
            Body::Text(_) => {
                if !fields.insert("value") {
                    problems.push(format!("two fields of {} are named value", node.name));
                }
            }
        }
    }
    problems
}

/// Whether two type lists name the same types, in any order.
fn same_types(left: &[String], right: &[String]) -> bool {
    let left_set: BTreeSet<&String> = left.iter().collect();
    let right_set: BTreeSet<&String> = right.iter().collect();
    left_set == right_set
}

const fn occurs_of(many: bool, optional: bool) -> Occurs {
    if many {
        Occurs::List
    } else if optional {
        Occurs::Optional
    } else {
        Occurs::Required
    }
}

/// The local name of a struct's schema type: `CT_P`, `ST_String`, `string`.
fn type_local(key: &NodeKey) -> String {
    match &key.source {
        Source::Data(name) => enums::split(name).map_or_else(
            || name.clone(),
            |(_, type_local, _, _)| type_local.to_owned(),
        ),
        Source::Xsd(type_ref) => type_ref
            .name()
            .map_or_else(|| "(unknown)".to_owned(), |name| name.local),
    }
}

/// The heading that titles a pair of tag and type: Part 1's first, the one
/// whose note names the type; else the first that names no other type of
/// the tag.
fn title_of<'titles>(
    titles: &'titles Titles,
    file: &str,
    local: &str,
    type_local: &str,
    siblings: &BTreeSet<String>,
) -> Option<&'titles Heading> {
    let headings = titles.elements.get(&(file.to_owned(), local.to_owned()))?;
    let typed = headings
        .iter()
        .find(|heading| heading.content_type.as_deref() == Some(type_local));
    typed.or_else(|| {
        headings.iter().find(|heading| {
            heading
                .content_type
                .as_ref()
                .is_none_or(|named| !siblings.contains(named))
        })
    })
}

/// The child positions of a particle of the Transitional schemas, in schema order.
fn walk_slots(
    set: &SchemaSet,
    particle: &Particle,
    context: Context,
    target: &str,
    slots: &mut Vec<Slot>,
) {
    let inner = Context {
        optional: context.optional || particle.min == 0,
        repeated: context.repeated || particle.max != Some(1),
    };
    match &particle.kind {
        ParticleKind::Element { name, type_ref } => slots.push(Slot::Element(
            NodeKey {
                source: Source::Xsd(type_ref.clone()),
                tag: name.clone(),
            },
            occurs_of(inner.repeated, inner.optional),
        )),
        ParticleKind::Sequence(children) => {
            let wildcards: Vec<(String, String)> = children
                .iter()
                .filter_map(|child| match &child.kind {
                    ParticleKind::Any {
                        namespace,
                        process_contents: _process_contents,
                    } => Some((namespace.clone(), target.to_owned())),
                    ParticleKind::All(_)
                    | ParticleKind::Choice(_)
                    | ParticleKind::GroupRef(_)
                    | ParticleKind::Sequence(_) => None,
                    ParticleKind::Element {
                        name: _name,
                        type_ref: _type_ref,
                    } => None,
                })
                .collect();
            if particle.max != Some(1) && !children.is_empty() && wildcards.len() == children.len()
            {
                slots.push(Slot::Any(wildcards, Occurs::List));
            } else {
                for child in children {
                    walk_slots(set, child, inner, target, slots);
                }
            }
        }
        ParticleKind::All(_) => {
            slots.push(Slot::All(enums::flatten(set, particle, target)));
        }
        ParticleKind::Choice(_) => choice_slot(set, particle, context, target, slots),
        ParticleKind::GroupRef(name) => {
            if let Some(group) = set.groups.get(name) {
                if let ParticleKind::Choice(_) = &group.particle.kind {
                    let combined = Particle {
                        kind: group.particle.kind.clone(),
                        max: product(particle.max, group.particle.max),
                        min: particle.min * group.particle.min,
                    };
                    choice_slot(set, &combined, context, &name.namespace, slots);
                } else {
                    walk_slots(set, &group.particle, inner, &name.namespace, slots);
                }
            }
        }
        ParticleKind::Any {
            namespace,
            process_contents: _process_contents,
        } => slots.push(Slot::Any(
            vec![(namespace.clone(), target.to_owned())],
            occurs_of(inner.repeated, inner.optional),
        )),
    }
}

/// A choice position: an enum slot when it allows two tags or more, else the
/// one element's or wildcard's slot.
fn choice_slot(
    set: &SchemaSet,
    particle: &Particle,
    context: Context,
    target: &str,
    slots: &mut Vec<Slot>,
) {
    let entries = enums::flatten(set, particle, target);
    let many = context.repeated || most(set, particle).is_none_or(|count| count > 1);
    let optional = context.optional || least(set, particle) == 0;
    let occurs = occurs_of(many, optional);
    if entries.len() > 1 {
        slots.push(Slot::Choice(entries, occurs));
        return;
    }
    match entries.into_iter().next() {
        Some(Entry::Element(key)) => slots.push(Slot::Element(key, occurs)),
        Some(Entry::Any {
            constraint,
            target: declared,
        }) => {
            slots.push(Slot::Any(vec![(constraint, declared)], occurs));
        }
        None => {}
    }
}

/// The most elements a particle can hold; `None` when unbounded.
fn most(set: &SchemaSet, particle: &Particle) -> Option<u32> {
    let one = match &particle.kind {
        ParticleKind::Any {
            namespace: _namespace,
            process_contents: _process_contents,
        } => Some(1),
        ParticleKind::Element {
            name: _name,
            type_ref: _type_ref,
        } => Some(1),
        ParticleKind::All(children) | ParticleKind::Sequence(children) => children
            .iter()
            .map(|child| most(set, child))
            .try_fold(0_u32, |sum, count| count.map(|found| sum + found)),
        ParticleKind::Choice(children) => children
            .iter()
            .map(|child| most(set, child))
            .try_fold(0_u32, |best, count| count.map(|found| best.max(found))),
        ParticleKind::GroupRef(name) => set
            .groups
            .get(name)
            .map_or(Some(0), |group| most(set, &group.particle)),
    };
    product(one, particle.max)
}

/// The fewest elements a particle can hold.
fn least(set: &SchemaSet, particle: &Particle) -> u32 {
    let one = match &particle.kind {
        ParticleKind::Any {
            namespace: _namespace,
            process_contents: _process_contents,
        } => 1,
        ParticleKind::Element {
            name: _name,
            type_ref: _type_ref,
        } => 1,
        ParticleKind::All(children) | ParticleKind::Sequence(children) => {
            children.iter().map(|child| least(set, child)).sum()
        }
        ParticleKind::Choice(children) => children
            .iter()
            .map(|child| least(set, child))
            .min()
            .unwrap_or(0),
        ParticleKind::GroupRef(name) => set
            .groups
            .get(name)
            .map_or(0, |group| least(set, &group.particle)),
    };
    one * particle.min
}

/// Two `maxOccurs` multiplied; `None` is unbounded.
const fn product(left: Option<u32>, right: Option<u32>) -> Option<u32> {
    match (left, right) {
        (Some(0), _) | (_, Some(0)) => Some(0),
        (Some(first), Some(second)) => Some(first * second),
        _ => None,
    }
}

/// The child positions of a particle of the data.
fn data_slots(data: &Data<'_>, particle: &DataParticle, context: Context, slots: &mut Vec<Slot>) {
    let (many, optional) = enums::occurs(particle);
    let inner = Context {
        optional: context.optional || optional,
        repeated: context.repeated || many,
    };
    match particle.kind.as_deref() {
        Some("Choice") => {
            let entries = enums::data_flatten(data, particle);
            let occurs = occurs_of(
                inner.repeated || particle.items.iter().any(|item| enums::occurs(item).0),
                inner.optional,
            );
            if entries.len() > 1 {
                slots.push(Slot::Choice(entries, occurs));
            } else {
                match entries.into_iter().next() {
                    Some(Entry::Element(key)) => slots.push(Slot::Element(key, occurs)),
                    Some(Entry::Any { constraint, target }) => {
                        slots.push(Slot::Any(vec![(constraint, target)], occurs));
                    }
                    None => {}
                }
            }
        }
        Some("Any") => slots.push(Slot::Any(
            vec![(
                particle
                    .namespace
                    .clone()
                    .unwrap_or_else(|| "##any".to_owned()),
                String::new(),
            )],
            occurs_of(inner.repeated, inner.optional),
        )),
        Some("All") => slots.push(Slot::All(enums::data_flatten(data, particle))),
        Some(_) => {
            for item in &particle.items {
                data_slots(data, item, inner, slots);
            }
        }
        None => {
            if let Some(key) = particle.name.as_deref().and_then(|name| data.resolve(name)) {
                slots.push(Slot::Element(
                    key,
                    occurs_of(inner.repeated, inner.optional),
                ));
            }
        }
    }
}
