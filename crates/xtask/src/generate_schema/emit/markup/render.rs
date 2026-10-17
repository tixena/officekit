//! The text of the typed tree's files: one module per namespace with its
//! structs, enums, and newtypes, its readers in `read.rs` and writers in
//! `write.rs` beside it, and `root.rs` declaring them.

pub mod layout;

use alloc::collections::{BTreeMap, BTreeSet};
use core::fmt::Write as _;
use core::iter;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use os_shim::System;

use self::layout::{Expr, MAX_WIDTH, doc, line, statement};
use super::names;
use super::structs::{Body, Catalog, EnumDef, EnumKind, Node, Occurs, Target, Value, Wildcard};
use super::{readers, traversal, writers};

/// The lines a generated function body holds before its work is split into
/// helpers, below clippy's `too_many_lines`.
pub const BODY_LINES: usize = 90;

/// The enum arms one function holds before its match is split.
pub const CHUNK: usize = 80;

/// The names a module takes from `officekit_xml`, each by a text only its
/// items' use of that name writes.
const XML_NAMES: [(&str, &str); 6] = [
    ("-> Element {", "officekit_xml::element::Element"),
    ("Node::Element(", "officekit_xml::element::Node"),
    ("Other(Node)", "officekit_xml::element::Node"),
    ("element: &Element", "officekit_xml::element::Element"),
    ("parse::list(", "officekit_xml::values::parse"),
    ("scope: &mut Scope", "officekit_xml::write::Scope"),
];

/// The names one module's items take.
pub struct Names<'catalog> {
    pub catalog: &'catalog Catalog,
    /// Each enum's reader and writer function.
    pub enum_functions: BTreeMap<usize, String>,
    pub module: usize,
    /// Each struct's reader and writer function.
    pub node_functions: BTreeMap<usize, String>,
}

impl Names<'_> {
    /// The path from this module to a type of `module` named `name`.
    #[must_use]
    pub fn path(&self, module: usize, name: &str) -> String {
        if module == self.module {
            name.to_owned()
        } else {
            format!("super::{}::{name}", self.catalog.modules[module].name)
        }
    }

    /// The `SCREAMING_SNAKE_CASE` name of a struct's shape, after its reader.
    #[must_use]
    pub fn shape(&self, node: usize) -> String {
        let function = self.node_functions.get(&node).map_or("", String::as_str);
        format!(
            "{}_SHAPE",
            function
                .strip_prefix("r#")
                .unwrap_or(function)
                .to_ascii_uppercase()
        )
    }

    /// The path from this module to an enum's tags.
    #[must_use]
    pub fn tags(&self, found: usize) -> String {
        let definition = &self.catalog.enums[found];
        self.path(
            definition.module,
            &format!("{}_TAGS", names::screaming(&definition.name)),
        )
    }
}

/// Two enums one of whose arm sets holds the other's.
struct Pair<'pair> {
    large: &'pair EnumDef,
    small: &'pair EnumDef,
    /// The path to the smaller enum from the larger's module.
    small_path: String,
}

impl Pair<'_> {
    /// `From<smaller> for larger`: each arm into the larger enum's arm of the
    /// same struct.
    fn impl_from(&self, helpers: &mut String) -> String {
        let (large, small, small_path) = (self.large, self.small, &self.small_path);
        let large_variant = variants(large);
        let mut out = String::from("\n");
        out.push_str(&layout::impl_header(
            &format!("From<{small_path}>"),
            &large.name,
        ));
        out.push_str(&layout::signature(
            4,
            "fn from",
            &[format!("item: {small_path}")],
            "Self",
        ));
        let converted = |arm: &usize, own: &str| {
            let value = moved(small.boxed.contains(arm), large.boxed.contains(arm));
            Expr::call(
                &format!(
                    "{own}::{}",
                    large_variant.get(arm).copied().unwrap_or_default()
                ),
                vec![value],
            )
        };
        let mut arms: Vec<(String, Expr)> = small
            .arms
            .iter()
            .map(|(variant, arm)| (format!("{small_path}::{variant}"), converted(arm, "Self")))
            .chain(iter::once((
                format!("{small_path}::Other"),
                Expr::call("Self::Other", vec![Expr::atom("node")]),
            )))
            .collect();
        arms.sort_by(|left, right| left.0.cmp(&right.0));
        let matched = match_text(&arms);
        if matched.lines().count() < BODY_LINES {
            out.push_str(&matched);
        } else {
            let steps: Vec<(String, Expr)> = small
                .arms
                .iter()
                .map(|(variant, arm)| {
                    (
                        format!("{small_path}::{variant}"),
                        converted(arm, &large.name),
                    )
                })
                .chain(iter::once((
                    format!("{small_path}::Other"),
                    Expr::call(&format!("{}::Other", large.name), vec![Expr::atom("node")]),
                )))
                .collect();
            let functions = steps_helpers(
                &format!("{}_into", snake(&small.name)),
                &snake(&large.name),
                [small_path, &large.name],
                &steps,
                helpers,
            );
            // Every item has an arm in the larger enum; an item none of the
            // helpers takes would join it as `Other`, in its generic form.
            out.push_str(&layout::chain(
                8,
                &format!("{}(Box::new(item))", functions[0]),
                &chained(
                    &functions,
                    "unwrap_or_else(|rest| Self::Other(markup::generic(&*rest)))",
                ),
            ));
        }
        out.push_str("    }\n}\n");
        out
    }

    /// `TryFrom<larger> for smaller`: each arm the smaller enum has into it,
    /// the item back for the others.
    fn impl_try_from(&self, helpers: &mut String) -> String {
        let (large, small, small_path) = (self.large, self.small, &self.small_path);
        let small_variant = variants(small);
        let mut out = String::from("\n");
        out.push_str(&layout::impl_header(
            &format!("TryFrom<{}>", large.name),
            small_path,
        ));
        let _ = writeln!(out, "    type Error = {};\n", large.name);
        out.push_str(&layout::signature(
            4,
            "fn try_from",
            &[format!("item: {}", large.name)],
            "Result<Self, Self::Error>",
        ));
        let mut arms: Vec<(String, Expr)> = large
            .arms
            .iter()
            .map(|(variant, arm)| {
                let own = format!("{}::{variant}", large.name);
                let body = small_variant.get(arm).map_or_else(
                    || Expr::call("Err", vec![Expr::call(&own, vec![Expr::atom("arm")])]),
                    |found| {
                        let value = moved(large.boxed.contains(arm), small.boxed.contains(arm));
                        Expr::call(
                            "Ok",
                            vec![Expr::call(&format!("Self::{found}"), vec![value])],
                        )
                    },
                );
                (own, body)
            })
            .chain(iter::once((
                format!("{}::Other", large.name),
                Expr::call(
                    "Ok",
                    vec![Expr::call("Self::Other", vec![Expr::atom("node")])],
                ),
            )))
            .collect();
        arms.sort_by(|left, right| left.0.cmp(&right.0));
        let matched = match_text(&arms);
        if matched.lines().count() < BODY_LINES {
            out.push_str(&matched);
        } else {
            let large_variant = variants(large);
            let steps: Vec<(String, Expr)> = small
                .arms
                .iter()
                .map(|(variant, arm)| {
                    let value = moved(large.boxed.contains(arm), small.boxed.contains(arm));
                    let own = large_variant.get(arm).copied().unwrap_or_default();
                    (
                        format!("{}::{own}", large.name),
                        Expr::call(&format!("{small_path}::{variant}"), vec![value]),
                    )
                })
                .chain(iter::once((
                    format!("{}::Other", large.name),
                    Expr::call(&format!("{small_path}::Other"), vec![Expr::atom("node")]),
                )))
                .collect();
            let functions = steps_helpers(
                &format!("{}_into", snake(&large.name)),
                &snake(&small.name),
                [&large.name, small_path],
                &steps,
                helpers,
            );
            out.push_str(&layout::chain(
                8,
                &format!("{}(Box::new(item))", functions[0]),
                &chained(&functions, "map_err(|boxed| *boxed)"),
            ));
        }
        out.push_str("    }\n}\n");
        out
    }
}

/// The imports a module's items need: a module (`crate::markup`), a trait
/// for its methods (`crate::markup::Write as _`), or one name
/// (`officekit_xml::element::Element`), which joins the other names of its
/// module in one `use`.
#[derive(Default)]
pub struct Uses {
    paths: BTreeSet<&'static str>,
}

impl Uses {
    /// Import `path`.
    pub fn add(&mut self, path: &'static str) {
        self.paths.insert(path);
    }

    /// The imports of a file whose items are `body`: the modules and the
    /// names of `officekit_xml` it uses, and the `Write` trait when it calls
    /// `write_into` outside an `impl` of that trait.
    #[must_use]
    pub fn of(body: &str) -> String {
        let mut uses = Self::default();
        uses.scan(body, body);
        uses.text()
    }

    /// Import the modules and the names of `officekit_xml` `items` uses, and
    /// the `Write` trait when `helpers`, free functions, call `write_into`.
    fn scan(&mut self, items: &str, helpers: &str) {
        if items.contains("markup::") {
            self.add("crate::markup");
        }
        if helpers.contains(".write_into(") {
            self.add("crate::markup::Write as _");
        }
        for (text, path) in XML_NAMES {
            if items.contains(text) {
                self.add(path);
            }
        }
    }

    /// The `use` lines, in rustfmt's order.
    fn text(&self) -> String {
        let mut grouped: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        // Each line by its path's segments, which rustfmt sorts the lines by.
        let mut lines: Vec<(Vec<&str>, String)> = Vec::new();
        for path in &self.paths {
            match path.rsplit_once("::") {
                Some((parent, name)) if parent.contains("::") && !name.contains(' ') => {
                    grouped.entry(parent).or_default().push(name);
                }
                _ => lines.push((path.split("::").collect(), format!("use {path};\n"))),
            }
        }
        for (parent, names) in grouped {
            let line = if let [only] = names.as_slice() {
                format!("use {parent}::{only};\n")
            } else {
                format!("use {parent}::{{{}}};\n", names.join(", "))
            };
            let mut key: Vec<&str> = parent.split("::").collect();
            key.extend(names.first());
            lines.push((key, line));
        }
        lines.sort();
        lines.into_iter().map(|(_, line)| line).collect()
    }
}

/// A namespace module's items: its statics, its type definitions and their
/// implementations by name, the free helpers those call, and the namespaces
/// its tags use.
#[derive(Default)]
struct ModuleItems {
    helpers: String,
    impls: Vec<(String, String)>,
    items: Vec<(String, String)>,
    namespaces: BTreeSet<Option<u8>>,
    statics: String,
}

impl ModuleItems {
    /// The items of `names.module`, each list in alphabetical order.
    fn collect(names: &Names<'_>, uses: &mut Uses) -> Result<Self> {
        let catalog = names.catalog;
        let mut parts = Self::default();
        for (index, node) in catalog.nodes.iter().enumerate() {
            if node.module != names.module {
                continue;
            }
            parts
                .items
                .push((node.name.clone(), struct_text(names, index, node)));
            let shape = shape_text(names, index, node, &mut parts.namespaces);
            parts.statics.push_str(&shape);
            parts
                .impls
                .push((node.name.clone(), node_impls(names, index, node)));
        }
        for (index, found) in catalog.enums.iter().enumerate() {
            if found.module != names.module {
                continue;
            }
            parts
                .items
                .push((found.name.clone(), enum_text(names, found)));
            let tags = tags_text(names, found, &mut parts.namespaces);
            parts.statics.push_str(&tags);
            let (text, chunks) = enum_impls(names, index, found, &mut parts.namespaces);
            parts.impls.push((found.name.clone(), text));
            parts.helpers.push_str(&chunks);
        }
        for (smaller, larger) in &catalog.conversions {
            if catalog.enums[*larger].module == names.module {
                let (text, chunks) = conversion(names, *smaller, *larger);
                parts
                    .impls
                    .push((catalog.enums[*larger].name.clone(), text));
                parts.helpers.push_str(&chunks);
            }
        }
        for simple in &catalog.simples {
            if simple.module != names.module {
                continue;
            }
            let (definitions, implementations, constants) =
                super::simple_types::render(names, simple, uses)?;
            parts.items.push((simple.name.clone(), definitions));
            parts.impls.push((simple.name.clone(), implementations));
            parts.statics.push_str(&constants);
        }
        parts.items.sort();
        parts.impls.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(parts)
    }
}

/// Every file of the typed tree, with its path and text.
///
/// # Errors
///
/// A name that cannot be written, or a folder that cannot be read.
pub fn files<S>(
    system: &S,
    header: &str,
    catalog: &Catalog,
    folder: &Path,
) -> Result<Vec<(PathBuf, String)>>
where
    S: System + ?Sized,
{
    let mut order: Vec<usize> = (0..catalog.modules.len()).collect();
    order.sort_by(|left, right| {
        catalog.modules[*left]
            .name
            .cmp(&catalog.modules[*right].name)
    });
    let listed = traversal::listed(catalog);
    let mut files = Vec::new();
    for module in &order {
        let names = functions(catalog, *module)?;
        let name = &catalog.modules[*module].name;
        let tests = system
            .is_file(&folder.join(name).join("tests.rs"))
            .unwrap_or(false);
        files.push((
            folder.join(format!("{name}.rs")),
            module_text(header, &names, &listed, tests)?,
        ));
        files.push((
            folder.join(name).join("read.rs"),
            readers::text(header, &names),
        ));
        if traversal::has_nodes(&names) {
            files.push((
                folder.join(name).join("traversal.rs"),
                traversal::text(header, &names, &listed),
            ));
        }
        files.push((
            folder.join(name).join("write.rs"),
            writers::text(header, &names),
        ));
    }
    let mut modules = String::from(header);
    modules.push_str(
        "// The namespace modules, `markup::<namespace>`, brought into `markup` with\n\
         // `include!` ahead of its imports.\n\n",
    );
    for module in &order {
        let _ = writeln!(modules, "pub mod {};", catalog.modules[*module].name);
    }
    files.push((folder.join("modules.rs"), modules));
    let mut root = String::from(header);
    root.push_str(&traversal::root_text(catalog, &order));
    files.push((folder.join("root.rs"), root));
    Ok(files)
}

/// The reader and writer function names of a module's structs and enums:
/// a struct's name in snake case, an enum's without its trailing `Content`
/// unless that is a struct's.
///
/// # Errors
///
/// A name that is no Rust identifier, or two enums of one module that read
/// under one name.
pub fn functions(catalog: &Catalog, module: usize) -> Result<Names<'_>> {
    let mut node_functions = BTreeMap::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    for (index, node) in catalog.nodes.iter().enumerate() {
        if node.module != module {
            continue;
        }
        let mut function = names::identifier(&names::snake_words(&names::name_words(&node.name)))?;
        let mut count = 1_u32;
        while used.contains(&function) {
            count += 1;
            function = names::identifier(&format!(
                "{}_{count}",
                names::snake_words(&names::name_words(&node.name))
            ))?;
        }
        used.insert(function.clone());
        node_functions.insert(index, function);
    }
    let mut enum_functions = BTreeMap::new();
    for (index, found) in catalog.enums.iter().enumerate() {
        if found.module != module {
            continue;
        }
        let words = names::name_words(&found.name);
        let short = words
            .strip_suffix(&["Content".to_owned()])
            .filter(|rest| !rest.is_empty())
            .map(|rest| names::identifier(&names::snake_words(rest)))
            .transpose()?;
        let full = names::identifier(&names::snake_words(&words))?;
        let function = match short {
            Some(candidate) if !used.contains(&candidate) => candidate,
            _ => full,
        };
        if !used.insert(function.clone()) {
            bail!(
                "two enums of {} read as {function}",
                catalog.modules[module].name
            );
        }
        enum_functions.insert(index, function);
    }
    Ok(Names {
        catalog,
        enum_functions,
        module,
        node_functions,
    })
}

/// A namespace module's file.
fn module_text(
    header: &str,
    names: &Names<'_>,
    listed: &BTreeSet<usize>,
    tests: bool,
) -> Result<String> {
    let catalog = names.catalog;
    let module = &catalog.modules[names.module];
    let mut out = String::from(header);
    let _ = writeln!(
        out,
        "//! The typed tree of the `{}` namespace: its structs, enums, and\n\
         //! simple-type newtypes, with the readers in `read` and the writers in\n\
         //! `write`.\n//!\n//! The namespace is `{}`.\n",
        module.prefix, module.uri
    );
    if traversal::has_nodes(names) {
        out.push_str("pub mod read;\nmod traversal;\npub mod write;\n");
    } else {
        out.push_str("pub mod read;\npub mod write;\n");
    }
    if tests {
        out.push_str("\n#[cfg(test)]\nmod tests;\n");
    }
    let mut uses = Uses::default();
    let mut parts = ModuleItems::collect(names, &mut uses)?;
    parts.items.push((
        "Context".to_owned(),
        traversal::context_trait(names, listed),
    ));
    parts
        .items
        .push(("Visitor".to_owned(), traversal::visitor_trait(names)));
    parts.items.sort_by(|left, right| left.0.cmp(&right.0));
    if traversal::has_nodes(names) {
        uses.add("crate::stream::transform::Written");
        uses.add("officekit_xml::stream::writer::PartWriter");
        uses.add("std::io");
    }
    let mut rest = parts.statics.clone();
    for (_, text) in parts.items.iter().chain(&parts.impls) {
        rest.push_str(text);
    }
    rest.push_str(&parts.helpers);
    uses.scan(&rest, &parts.helpers);
    if parts.namespaces.iter().any(Option::is_some) {
        uses.add("officekit_xml::name::NamespaceId");
    }
    let use_text = uses.text();
    if !use_text.is_empty() {
        out.push('\n');
        out.push_str(&use_text);
    }
    let constants = namespace_constants(catalog, &parts.namespaces)?;
    if !constants.is_empty() {
        out.push('\n');
        out.push_str(&constants);
    }
    out.push_str(&parts.statics);
    for (_, text) in &parts.items {
        out.push('\n');
        out.push_str(text);
    }
    for (_, text) in &parts.impls {
        out.push_str(text);
    }
    out.push_str(&parts.helpers);
    Ok(out)
}

/// The `NS_` constants of the namespaces a module's tags use.
fn namespace_constants(catalog: &Catalog, namespaces: &BTreeSet<Option<u8>>) -> Result<String> {
    let mut constants = String::new();
    for namespace in namespaces.iter().flatten() {
        let prefix = catalog
            .namespace_rows()
            .get(usize::from(*namespace))
            .with_context(|| format!("no namespace {namespace}"))?;
        let _ = writeln!(
            constants,
            "const {}: Option<NamespaceId> = Some({namespace});",
            namespace_constant(prefix)
        );
    }
    Ok(constants)
}

/// The constant of a namespace id, `NS_W`.
#[must_use]
pub fn namespace_constant(prefix: &str) -> String {
    format!("NS_{}", prefix.to_ascii_uppercase())
}

/// A tag literal, `(NS_W, "p")`.
fn tag(
    names: &Names<'_>,
    namespace: Option<u8>,
    local: &str,
    namespaces: &mut BTreeSet<Option<u8>>,
) -> Expr {
    namespaces.insert(namespace);
    let space = namespace.map_or_else(
        || "None".to_owned(),
        |id| {
            namespace_constant(
                &names
                    .catalog
                    .namespace_rows()
                    .get(usize::from(id))
                    .map_or_else(String::new, Clone::clone),
            )
        },
    );
    Expr::Tuple(vec![Expr::Atom(space), Expr::Atom(layout::string(local))])
}

/// A struct's definition.
fn struct_text(names: &Names<'_>, index: usize, node: &Node) -> String {
    let mut fields: Vec<(String, String)> = Vec::new();
    for attribute in &node.attributes {
        let value = value_type(names, attribute.value);
        fields.push((attribute.field.clone(), format!("Option<{value}>")));
    }
    match &node.body {
        Body::Text(value) => fields.push(("value".to_owned(), value_type(names, *value))),
        Body::Children(children) => {
            for child in children {
                let target = match child.target {
                    Target::Node(child_index) if child_index == index => "Self".to_owned(),
                    Target::Node(child_index) => {
                        let child_node = &names.catalog.nodes[child_index];
                        names.path(child_node.module, &child_node.name)
                    }
                    Target::Enum(found) => {
                        let definition = &names.catalog.enums[found];
                        names.path(definition.module, &definition.name)
                    }
                };
                let shown = match child.occurs {
                    Occurs::List => format!("Vec<{target}>"),
                    Occurs::Optional => format!("Option<Box<{target}>>"),
                    Occurs::Required => format!("Box<{target}>"),
                };
                fields.push((child.field.clone(), shown));
            }
        }
    }
    fields.push(("unknown".to_owned(), "markup::Unknown".to_owned()));
    fields.sort_by(|left, right| bare(&left.0).cmp(bare(&right.0)));
    let mut out = doc(0, &node.doc);
    out.push_str("#[derive(Clone, Debug, Default, Eq, PartialEq)]\n#[non_exhaustive]\n");
    let _ = writeln!(out, "pub struct {} {{", node.name);
    for (field, shown) in fields {
        let line = format!("    pub {field}: {shown},");
        if line.len() <= MAX_WIDTH {
            out.push_str(&line);
            out.push('\n');
        } else {
            let _ = writeln!(out, "    pub {field}:\n        {shown},");
        }
    }
    out.push_str("}\n");
    out
}

/// A field's name without `r#`, for ordering.
fn bare(field: &str) -> &str {
    field.strip_prefix("r#").unwrap_or(field)
}

/// The Rust type of an attribute's or of simple content's value.
fn value_type(names: &Names<'_>, value: Value) -> String {
    match value {
        Value::Simple(index) => {
            let simple = &names.catalog.simples[index];
            names.path(simple.module, &simple.name)
        }
        Value::Text => "String".to_owned(),
    }
}

/// A struct's shape: its tag, declared attributes, and child positions.
fn shape_text(
    names: &Names<'_>,
    index: usize,
    node: &Node,
    namespaces: &mut BTreeSet<Option<u8>>,
) -> String {
    let catalog = names.catalog;
    let own = tag(names, node.namespace, &node.key.tag.local, namespaces);
    let attributes: Vec<Expr> = node
        .attributes
        .iter()
        .map(|attribute| tag(names, attribute.namespace, &attribute.local, namespaces))
        .collect();
    let mut slots = Vec::new();
    let text = match &node.body {
        Body::Text(_) => true,
        Body::Children(children) => {
            for child in children {
                let slot = match (child.target, child.occurs) {
                    (Target::Node(child_index), occurs) => {
                        let function = match occurs {
                            Occurs::List => "markup::Slot::list",
                            Occurs::Optional => "markup::Slot::single",
                            Occurs::Required => "markup::Slot::required",
                        };
                        let found = &catalog.nodes[child_index];
                        Expr::call(
                            function,
                            vec![tag(
                                names,
                                found.namespace,
                                &found.key.tag.local,
                                namespaces,
                            )],
                        )
                    }
                    (Target::Enum(found), occurs) => {
                        let function = match (catalog.enums[found].kind, occurs) {
                            (EnumKind::All, _) => "markup::Slot::once",
                            (_, Occurs::List) => "markup::Slot::items",
                            (_, Occurs::Optional) => "markup::Slot::choice",
                            (_, Occurs::Required) => "markup::Slot::required_choice",
                        };
                        Expr::call(
                            function,
                            vec![Expr::reference(Expr::Atom(names.tags(found)))],
                        )
                    }
                };
                slots.push(slot);
            }
            false
        }
    };
    let expr = Expr::Struct(
        "markup::Shape".to_owned(),
        vec![
            ("attributes".to_owned(), Expr::Slice(attributes)),
            ("name".to_owned(), own),
            ("slots".to_owned(), Expr::Slice(slots)),
            (
                "text".to_owned(),
                Expr::atom(if text { "true" } else { "false" }),
            ),
        ],
    );
    let mut out = String::from("\n");
    statement(
        &format!("static {}: markup::Shape = ", names.shape(index)),
        &expr,
        &mut out,
    );
    out
}

/// An enum's tags: those of its arms, and its wildcard's namespaces.
fn tags_text(names: &Names<'_>, found: &EnumDef, namespaces: &mut BTreeSet<Option<u8>>) -> String {
    let catalog = names.catalog;
    let tags: Vec<Expr> = if found.kind == EnumKind::Wildcard {
        Vec::new()
    } else {
        found
            .arms
            .iter()
            .map(|(_, arm)| {
                let node = &catalog.nodes[*arm];
                tag(names, node.namespace, &node.key.tag.local, namespaces)
            })
            .collect()
    };
    let any = found.wildcard.as_ref().map_or_else(
        || Expr::atom("None"),
        |wildcard| {
            let inner = match wildcard {
                Wildcard::Any => Expr::atom("markup::Wildcard::Any"),
                Wildcard::Local => Expr::atom("markup::Wildcard::Local"),
                Wildcard::Not(id) => {
                    namespaces.insert(Some(*id));
                    Expr::call(
                        "markup::Wildcard::Not",
                        vec![Expr::atom(&layout::integer(i128::from(*id)))],
                    )
                }
                Wildcard::Only(ids) => Expr::call(
                    "markup::Wildcard::Only",
                    vec![Expr::Slice(
                        ids.iter()
                            .map(|id| Expr::atom(&layout::integer(i128::from(*id))))
                            .collect(),
                    )],
                ),
            };
            Expr::call("Some", vec![inner])
        },
    );
    let expr = Expr::Struct(
        "markup::Accept".to_owned(),
        vec![
            ("any".to_owned(), any),
            ("tags".to_owned(), Expr::Slice(tags)),
        ],
    );
    let mut out = String::from("\n");
    statement(
        &format!(
            "pub(crate) static {}_TAGS: markup::Accept = ",
            names::screaming(&found.name)
        ),
        &expr,
        &mut out,
    );
    out
}

/// An enum's definition.
fn enum_text(names: &Names<'_>, found: &EnumDef) -> String {
    let mut variants: Vec<(String, String)> = found
        .arms
        .iter()
        .map(|(variant, arm)| {
            let node = &names.catalog.nodes[*arm];
            let path = names.path(node.module, &node.name);
            let shown = if found.boxed.contains(arm) {
                format!("Box<{path}>")
            } else {
                path
            };
            (variant.clone(), shown)
        })
        .collect();
    variants.push(("Other".to_owned(), "Node".to_owned()));
    variants.sort();
    let mut out = doc(0, &found.doc);
    out.push_str("#[derive(Clone, Debug, Eq, PartialEq)]\n#[non_exhaustive]\n");
    let _ = writeln!(out, "pub enum {} {{", found.name);
    for (variant, shown) in variants {
        let line = format!("    {variant}({shown}),");
        if line.len() <= MAX_WIDTH {
            out.push_str(&line);
            out.push('\n');
        } else {
            let _ = writeln!(out, "    {variant}(\n        {shown},\n    ),");
        }
    }
    out.push_str("}\n");
    out
}

/// A struct's `Read` and `Write` implementations, which call its reader and writer.
fn node_impls(names: &Names<'_>, index: usize, node: &Node) -> String {
    let function = names
        .node_functions
        .get(&index)
        .cloned()
        .unwrap_or_default();
    let mut out = String::from("\n");
    out.push_str(&layout::impl_header("markup::Read", &node.name));
    out.push_str("    fn read(element: &Element) -> Self {\n");
    line(
        "        ",
        &Expr::call(&format!("read::{function}"), vec![Expr::atom("element")]),
        "",
        &mut out,
    );
    out.push_str("    }\n}\n\n");
    out.push_str(&layout::impl_header("markup::Write", &node.name));
    out.push_str("    fn write_into(&self, writer: &mut markup::Writer<'_, '_>) {\n");
    statement(
        "        let element = ",
        &Expr::call(
            &format!("write::{function}"),
            vec![Expr::atom("self"), Expr::atom("writer.scope()")],
        ),
        &mut out,
    );
    out.push_str("        writer.known(element);\n    }\n}\n");
    out
}

/// An enum's `Default` (at a required position), `Read`, and `Write`
/// implementations, and the chunks of a match too long for one function.
fn enum_impls(
    names: &Names<'_>,
    index: usize,
    found: &EnumDef,
    namespaces: &mut BTreeSet<Option<u8>>,
) -> (String, String) {
    let function = names
        .enum_functions
        .get(&index)
        .cloned()
        .unwrap_or_default();
    let mut out = enum_default(names, found);
    let mut helpers = String::new();
    out.push_str(&enum_read(
        names,
        found,
        &function,
        namespaces,
        &mut helpers,
    ));
    out.push_str(&enum_write(found, &function, &mut helpers));
    (out, helpers)
}

/// An enum's `Default`, its first arm's default, when a required single
/// position uses it.
fn enum_default(names: &Names<'_>, found: &EnumDef) -> String {
    let mut out = String::new();
    if found.default
        && let Some((variant, arm)) = found.arms.first()
    {
        let node = &names.catalog.nodes[*arm];
        let _ = write!(
            out,
            "\n{}    fn default() -> Self {{\n",
            layout::impl_header("Default", &found.name)
        );
        let value = if found.boxed.contains(arm) {
            "Box::default()".to_owned()
        } else {
            format!("{}::default()", names.path(node.module, &node.name))
        };
        line(
            "        ",
            &Expr::call(&format!("Self::{variant}"), vec![Expr::Atom(value)]),
            "",
            &mut out,
        );
        out.push_str("    }\n}\n");
    }
    out
}

/// An enum's `Read`: the arm of the element's tag, else `Other`; past
/// `CHUNK` arms, the match is split into helpers tried in turn.
fn enum_read(
    names: &Names<'_>,
    found: &EnumDef,
    function: &str,
    namespaces: &mut BTreeSet<Option<u8>>,
    helpers: &mut String,
) -> String {
    let catalog = names.catalog;
    let mut out = String::from("\n");
    out.push_str(&layout::impl_header("markup::Read", &found.name));
    out.push_str(
        "    fn other(node: Node) -> Option<Self> {\n        Some(Self::Other(node))\n    }\n\n",
    );
    out.push_str("    fn read(element: &Element) -> Self {\n");
    let other = Expr::call(
        "Self::Other",
        vec![Expr::call(
            "Node::Element",
            vec![Expr::atom("element.clone()")],
        )],
    );
    let arm_lines: Vec<(String, String, Expr)> = found
        .arms
        .iter()
        .map(|(variant, arm)| {
            let node = &catalog.nodes[*arm];
            let key = tag(names, node.namespace, &node.key.tag.local, namespaces);
            let read = Expr::call("markup::read", vec![Expr::atom("element")]);
            let value = if found.boxed.contains(arm) {
                Expr::call("Box::new", vec![read])
            } else {
                read
            };
            (format!("Some({})", key.one_line()), variant.clone(), value)
        })
        .collect();
    if arm_lines.is_empty() {
        line("        ", &other, "", &mut out);
    } else if arm_lines.len() <= CHUNK {
        out.push_str("        match markup::name(element) {\n");
        for (pattern, variant, value) in &arm_lines {
            let body = Expr::call(&format!("Self::{variant}"), vec![value.clone()]);
            out.push_str(&layout::arm(12, &Expr::Atom(pattern.clone()), &body, false));
        }
        out.push_str(&layout::arm(12, &Expr::atom("_"), &other, false));
        out.push_str("        }\n");
    } else {
        let mut calls = Vec::new();
        for (chunk, lines) in arm_lines.chunks(CHUNK / 2).enumerate() {
            let helper = format!("read_{function}_{}", chunk + 1);
            if chunk > 0 {
                calls.push(format!(".or_else(|| {helper}(element))"));
            }
            let _ = write!(
                helpers,
                "\n{}",
                layout::signature(
                    0,
                    &format!("fn {helper}"),
                    &["element: &Element".to_owned()],
                    &format!("Option<{}>", found.name)
                )
            );
            helpers.push_str("    let item = match markup::name(element) {\n");
            for (pattern, variant, value) in lines {
                let body = Expr::call(&format!("{}::{variant}", found.name), vec![value.clone()]);
                helpers.push_str(&layout::arm(8, &Expr::Atom(pattern.clone()), &body, false));
            }
            helpers.push_str("        _ => return None,\n    };\n    Some(item)\n}\n");
        }
        calls.push(format!(".unwrap_or_else(|| {})", other.one_line()));
        out.push_str(&layout::chain(
            8,
            &format!("read_{function}_1(element)"),
            &calls,
        ));
    }
    out.push_str("    }\n}\n");
    out
}

/// An enum's `Write`: its arm's element, or its `Other` node; past `CHUNK`
/// arms, the arms are written by helpers.
fn enum_write(found: &EnumDef, function: &str, helpers: &mut String) -> String {
    let mut out = String::from("\n");
    out.push_str(&layout::impl_header("markup::Write", &found.name));
    out.push_str("    fn write_into(&self, writer: &mut markup::Writer<'_, '_>) {\n");
    if found.arms.is_empty() {
        out.push_str(
            "        let Self::Other(node) = self;\n        writer.other(node.clone());\n",
        );
    } else if found.arms.len() <= CHUNK {
        out.push_str("        match self {\n");
        let mut arms: Vec<(String, Expr)> = found
            .arms
            .iter()
            .map(|(variant, _)| {
                (
                    format!("Self::{variant}"),
                    Expr::call("arm.write_into", vec![Expr::atom("writer")]),
                )
            })
            .collect();
        arms.push((
            "Self::Other".to_owned(),
            Expr::call("writer.other", vec![Expr::atom("node.clone()")]),
        ));
        arms.sort_by(|left, right| left.0.cmp(&right.0));
        for (path, body) in &arms {
            let binding = if path == "Self::Other" { "node" } else { "arm" };
            out.push_str(&layout::arm(12, &Expr::pattern(path, binding), body, true));
        }
        out.push_str("        }\n");
    } else {
        out.push_str(
            "        if let Self::Other(node) = self {\n            writer.other(node.clone());\n        }\n",
        );
        for (chunk, arms) in found.arms.chunks(CHUNK / 4).enumerate() {
            let helper = format!("write_{function}_{}", chunk + 1);
            let _ = writeln!(out, "        {helper}(self, writer);");
            let _ = write!(
                helpers,
                "\n{}",
                layout::signature(
                    0,
                    &format!("fn {helper}"),
                    &[
                        format!("item: &{}", found.name),
                        "writer: &mut markup::Writer<'_, '_>".to_owned()
                    ],
                    ""
                )
            );
            for (variant, _) in arms {
                let _ = write!(
                    helpers,
                    "    if let {}::{variant}(arm) = item {{\n        arm.write_into(writer);\n    }}\n",
                    found.name
                );
            }
            helpers.push_str("}\n");
        }
    }
    out.push_str("    }\n}\n");
    out
}

/// `From<smaller> for larger` and `TryFrom<larger> for smaller`, and the
/// helpers either calls when its match is too long for one function.
fn conversion(names: &Names<'_>, smaller: usize, larger: usize) -> (String, String) {
    let catalog = names.catalog;
    let pair = Pair {
        large: &catalog.enums[larger],
        small: &catalog.enums[smaller],
        small_path: names.path(catalog.enums[smaller].module, &catalog.enums[smaller].name),
    };
    let mut helpers = String::new();
    let mut out = pair.impl_from(&mut helpers);
    out.push_str(&pair.impl_try_from(&mut helpers));
    (out, helpers)
}

/// Each arm's struct to its variant.
fn variants(found: &EnumDef) -> BTreeMap<usize, &str> {
    found
        .arms
        .iter()
        .map(|(variant, arm)| (*arm, variant.as_str()))
        .collect()
}

/// A name in snake case, for a helper's name.
fn snake(name: &str) -> String {
    names::snake_words(&names::name_words(name))
}

/// A conversion's match on `item`, its arms laid out at `indent` 12.
fn match_text(arms: &[(String, Expr)]) -> String {
    let mut out = String::from("        match item {\n");
    for (path, body) in arms {
        let binding = if path.ends_with("::Other") {
            "node"
        } else {
            "arm"
        };
        out.push_str(&layout::arm(12, &Expr::pattern(path, binding), body, false));
    }
    out.push_str("        }\n");
    out
}

/// An arm's struct moved from one enum's arm into another's: unboxed or
/// boxed as the arm it goes to holds it.
fn moved(from_boxed: bool, to_boxed: bool) -> Expr {
    match (from_boxed, to_boxed) {
        (true, false) => Expr::atom("*arm"),
        (false, true) => Expr::call("Box::new", vec![Expr::atom("arm")]),
        (true, true) | (false, false) => Expr::atom("arm"),
    }
}

/// The calls after a conversion's first helper: `.or_else` each other
/// helper, then `last`.
fn chained(functions: &[String], last: &str) -> Vec<String> {
    functions
        .iter()
        .skip(1)
        .map(|function| format!(".or_else({function})"))
        .chain(iter::once(format!(".{last}")))
        .collect()
}

/// The helpers of a conversion too long for one function: each takes the
/// item boxed, gives the converted item for its run of `steps` (an arm's path
/// and the value it converts to), and gives the item back otherwise. Returns
/// their names.
fn steps_helpers(
    prefix: &str,
    target: &str,
    [from, to]: [&str; 2],
    steps: &[(String, Expr)],
    helpers: &mut String,
) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut chunk = String::new();
    for (pattern, value) in steps {
        let binding = if pattern.ends_with("::Other") {
            "node"
        } else {
            "arm"
        };
        let mut text = layout::if_let(4, &Expr::pattern(pattern, binding), "*item");
        line(
            "        return ",
            &Expr::call("Ok", vec![value.clone()]),
            ";",
            &mut text,
        );
        text.push_str("    }\n");
        if !chunk.is_empty() && chunk.lines().count() + text.lines().count() >= BODY_LINES {
            chunks.push(chunk);
            chunk = String::new();
        }
        chunk.push_str(&text);
    }
    chunks.push(chunk);
    let mut functions = Vec::new();
    for (index, text) in chunks.iter().enumerate() {
        let function = format!("{prefix}_{target}_{}", index + 1);
        let _ = write!(
            helpers,
            "\n{}{text}    Err(item)\n}}\n",
            layout::signature(
                0,
                &format!("fn {function}"),
                &[format!("item: Box<{from}>")],
                &format!("Result<{to}, Box<{from}>>"),
            )
        );
        functions.push(function);
    }
    functions
}
