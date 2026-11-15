//! The text of the typed tree's files: one module per namespace with its
//! structs, enums, and newtypes, its readers in `read.rs`, writers in
//! `write.rs` and its enums' rkyv mirrors in `archive.rs` beside it, in the
//! folder of its crate; each crate's `modules.rs` declaring them; and in
//! `officekit-markup`, `root.rs` and the hook modules of base's and core's
//! namespaces.

pub mod layout;

use alloc::collections::{BTreeMap, BTreeSet};
use core::cell::RefCell;
use core::fmt::Write as _;
use core::iter;
use core::mem;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use officekit_xml::ready::ready_now;
use os_shim::System;

use self::layout::{Expr, MAX_WIDTH, doc, line, statement};
use super::archive::{self, Cycles};
use super::names;
use super::structs::{
    Body, Catalog, Child, EnumDef, EnumKind, Node, Occurs, Target, Value, Wildcard,
};
use super::{Crate, readers, traversal, user_data, writers};

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
    /// The other modules `path` was asked for.
    pub named: RefCell<BTreeSet<usize>>,
    /// Each struct's reader and writer function.
    pub node_functions: BTreeMap<usize, String>,
}

impl Names<'_> {
    /// The path from the module's `archive` module to an item of `module`
    /// named `name`: through `super` in this crate, as `path` from another.
    #[must_use = "this only returns the value"]
    pub fn archived(&self, module: usize, name: &str) -> String {
        let path = self.path(module, name);
        if self.crosses(module) {
            path
        } else {
            format!("super::{path}")
        }
    }

    /// Whether `module` is written to another crate than this module.
    #[must_use = "this only answers the question"]
    pub fn crosses(&self, module: usize) -> bool {
        self.catalog.crates.get(module) != self.catalog.crates.get(self.module)
    }

    /// The imports of the modules of other crates `path` gave paths into.
    pub fn imports(&self, uses: &mut Uses) {
        for module in self.named.borrow().iter() {
            if let Some(krate) = self.catalog.crates.get(*module)
                && self.crosses(*module)
            {
                uses.add(&format!(
                    "{}::{}",
                    krate.name(),
                    self.catalog.modules[*module].name
                ));
            }
        }
    }

    /// The constructor of a newtype of `module` named `name`: its tuple
    /// constructor in this crate, its `From<String>` from another, where
    /// `#[non_exhaustive]` keeps the constructor private.
    #[must_use = "this only returns the value"]
    pub fn newtype(&self, module: usize, name: &str) -> String {
        let path = self.path(module, name);
        if self.crosses(module) {
            format!("{path}::from")
        } else {
            path
        }
    }

    /// The path from this module to an item of `module` named `name`: through
    /// `super` in this crate, through the module, which the file imports,
    /// from another.
    #[must_use = "this only returns the value"]
    pub fn path(&self, module: usize, name: &str) -> String {
        if module == self.module {
            return name.to_owned();
        }
        self.named.borrow_mut().insert(module);
        let other = &self.catalog.modules[module].name;
        if self.crosses(module) {
            format!("{other}::{name}")
        } else {
            format!("super::{other}::{name}")
        }
    }

    /// The path from this module to the runtime: `crate` in
    /// `officekit-markup-base`, the crate's name from core and ext.
    #[must_use = "this only returns the value"]
    pub fn runtime(&self) -> &'static str {
        match self.catalog.crates.get(self.module) {
            Some(Crate::Core | Crate::Ext) => Crate::Base.name(),
            Some(Crate::Base) | None => "crate",
        }
    }

    /// The `SCREAMING_SNAKE_CASE` name of a struct's shape, after its reader.
    #[must_use = "this only returns the value"]
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
    #[must_use = "this only returns the value"]
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
    /// The larger enum's variant of each struct.
    large_variants: BTreeMap<usize, &'pair str>,
    /// The path to the runtime from the larger's module.
    runtime: &'static str,
    small: &'pair EnumDef,
    /// The path to the smaller enum from the larger's module.
    small_path: String,
}

impl Pair<'_> {
    /// The smaller enum's arm for `arm` as the larger's arm of the same
    /// struct, `own` naming the larger enum.
    fn converted(&self, arm: usize, own: &str) -> Expr {
        let value = moved(
            self.small.boxed.contains(&arm),
            self.large.boxed.contains(&arm),
        );
        Expr::call(
            &format!(
                "{own}::{}",
                self.large_variants.get(&arm).copied().unwrap_or_default()
            ),
            vec![value],
        )
    }

    /// `From<smaller> for larger`: each arm into the larger enum's arm of the
    /// same struct.
    fn impl_from(&self, helpers: &mut String) -> String {
        let (large, small, small_path) = (self.large, self.small, &self.small_path);
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
        if let Some(matched) = self.match_from() {
            out.push_str(&matched);
        } else {
            let steps: Vec<(String, Expr)> = small
                .arms
                .iter()
                .map(|(variant, arm)| {
                    (
                        format!("{small_path}::{variant}"),
                        self.converted(*arm, &large.name),
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
                    &format!(
                        "unwrap_or_else(|rest| Self::Other({}::generic(&*rest)))",
                        self.runtime
                    ),
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

    /// `From<smaller>`'s match on the item, arm by arm, when one function
    /// holds it.
    fn match_from(&self) -> Option<String> {
        let small_path = &self.small_path;
        let mut arms: Vec<(String, Expr)> = self
            .small
            .arms
            .iter()
            .map(|(variant, arm)| {
                (
                    format!("{small_path}::{variant}"),
                    self.converted(*arm, "Self"),
                )
            })
            .chain(iter::once((
                format!("{small_path}::Other"),
                Expr::call("Self::Other", vec![Expr::atom("node")]),
            )))
            .collect();
        arms.sort_by(|left, right| left.0.cmp(&right.0));
        let matched = match_text(&arms);
        (matched.lines().count() < BODY_LINES).then_some(matched)
    }
}

/// The imports a module's items need: a trait for its methods
/// (`crate::Write as _`), a module of another crate
/// (`officekit_markup_base::common_simple_types`), or one name
/// (`officekit_xml::element::Element`), which joins the other names of its
/// module in one `use`.
#[derive(Default)]
pub struct Uses {
    paths: BTreeSet<String>,
}

impl Uses {
    /// Import `path`.
    pub fn add(&mut self, path: &str) {
        self.paths.insert(path.to_owned());
    }

    /// The imports of a file whose items are `body`, `runtime` the path to
    /// the runtime: the names of `officekit_xml` it uses, and the `Write`
    /// trait when it calls `write_into` outside an `impl` of that trait.
    #[must_use = "this only returns the value"]
    pub fn of(runtime: &str, body: &str) -> String {
        let mut uses = Self::default();
        uses.scan(runtime, body, body);
        uses.text()
    }

    /// Import the names of `officekit_xml` `items` uses, and the `Write`
    /// trait of `runtime` when `helpers`, free functions, call `write_into`.
    fn scan(&mut self, runtime: &str, items: &str, helpers: &str) {
        if helpers.contains("mem::take(") {
            self.add("core::mem");
        }
        if helpers.contains(".write_into(") {
            self.add(&format!("{runtime}::Write as _"));
        }
        for (text, path) in XML_NAMES {
            if items.contains(text) {
                self.add(path);
            }
        }
    }

    /// The `use` lines, in rustfmt's order.
    #[must_use = "this only returns the value"]
    pub fn text(&self) -> String {
        // Each line by its path's segments, which rustfmt sorts the lines by.
        let mut lines: Vec<(Vec<&str>, String)> = Vec::new();
        use_lines(&self.paths, &mut lines);
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
    /// The items of `names.module`, each list in alphabetical order; the
    /// enums of `opened` are not `#[non_exhaustive]`.
    fn collect(
        names: &Names<'_>,
        cycles: &Cycles,
        uses: &mut Uses,
        opened: &BTreeSet<usize>,
    ) -> Result<Self> {
        let catalog = names.catalog;
        let mut parts = Self::default();
        for (index, node) in catalog.nodes.iter().enumerate() {
            if node.module != names.module {
                continue;
            }
            uses.add(&format!("{}::UserData", names.runtime()));
            parts
                .items
                .push((node.name.clone(), struct_text(names, cycles, index, node)));
            let shape = shape_text(names, index, node, &mut parts.namespaces);
            parts.statics.push_str(&shape);
            let mut methods = without_children_text(node);
            let opening = methods.find("{\n").map_or(0, |at| at + "{\n".len());
            let converted = into_user_data_text(names, cycles, index, node, &mut parts.helpers);
            methods.insert_str(
                opening,
                &format!("{}\n", converted.trim_start_matches('\n')),
            );
            parts.impls.push((node.name.clone(), methods));
            parts
                .impls
                .push((node.name.clone(), node_impls(names, index, node)));
        }
        for (index, found) in catalog.enums.iter().enumerate() {
            if found.module != names.module {
                continue;
            }
            uses.add(&format!("{}::UserData", names.runtime()));
            parts.items.push((
                found.name.clone(),
                enum_text(names, found, opened.contains(&index)),
            ));
            let tags = tags_text(names, found, &mut parts.namespaces);
            parts.statics.push_str(&tags);
            parts.impls.push((
                found.name.clone(),
                enum_into_user_data(found, names.runtime()),
            ));
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

/// How a struct's fields are written: in the public generic struct, or in
/// its `archive` mirror, which names the plain tree.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Form {
    Mirror,
    Public,
}

/// What `into_user_data` does with a field.
pub enum Conversion {
    /// The child's occurrence and the path of its generic type: the child converts.
    Child(Occurs, String),
    /// The field moves as it is.
    Move,
    /// The slot: the new kind's default.
    Slot,
}

/// A field of a struct.
pub struct Field {
    pub conversion: Conversion,
    pub doc: String,
    pub name: String,
    /// Whether the field closes a cycle of types.
    pub recursive: bool,
    pub shown: String,
}

/// The name of the generic type of a struct or an enum.
#[must_use = "this only returns the value"]
pub fn generic_name(name: &str) -> String {
    format!("{name}With")
}

/// The type an attribute or simple content holds.
fn held_value(names: &Names<'_>, value: Value, form: Form) -> String {
    match value {
        Value::Simple(index) => {
            let simple = &names.catalog.simples[index];
            if form == Form::Mirror {
                names.archived(simple.module, &simple.name)
            } else {
                names.path(simple.module, &simple.name)
            }
        }
        Value::Text => "String".to_owned(),
    }
}

/// The type a child field holds, as the doc names it and as the form writes it.
fn held_child(
    names: &Names<'_>,
    index: usize,
    node: &Node,
    target: Target,
    form: Form,
) -> (String, String) {
    let mirror = form == Form::Mirror;
    let (module, base) = match target {
        Target::Node(child) if child == index => {
            let held = if mirror {
                names.archived(node.module, &node.name)
            } else {
                "Self".to_owned()
            };
            return ("Self".to_owned(), held);
        }
        Target::Node(child) => {
            let found = &names.catalog.nodes[child];
            (found.module, &found.name)
        }
        Target::Enum(found) => {
            let definition = &names.catalog.enums[found];
            (definition.module, &definition.name)
        }
    };
    let documented = names.path(module, base);
    let held = if mirror {
        names.archived(module, base)
    } else {
        format!("{}<X>", names.path(module, &generic_name(base)))
    };
    (documented, held)
}

/// A struct's child field.
fn child_field(
    names: &Names<'_>,
    cycles: &Cycles,
    index: usize,
    node: &Node,
    child: &Child,
    form: Form,
) -> Field {
    let (target, held) = held_child(names, index, node, child.target, form);
    let shown = match child.occurs {
        Occurs::List => format!("Vec<{held}>"),
        Occurs::Optional => format!("Option<Box<{held}>>"),
        Occurs::Required => format!("Box<{held}>"),
    };
    let noun = match child.target {
        Target::Node(_) => format!("`{target}`"),
        Target::Enum(_) => format!("an item of `{target}`"),
    };
    let doc = match child.occurs {
        Occurs::List => format!("The children that are {noun}, in order."),
        Occurs::Optional => format!("The child that is {noun}, when there is one."),
        Occurs::Required => format!("The child that is {noun}, which the schema requires."),
    };
    let generic = match child.target {
        Target::Node(child_index) => {
            let found = &names.catalog.nodes[child_index];
            names.path(found.module, &generic_name(&found.name))
        }
        Target::Enum(found) => {
            let definition = &names.catalog.enums[found];
            names.path(definition.module, &generic_name(&definition.name))
        }
    };
    Field {
        conversion: Conversion::Child(child.occurs, generic),
        doc,
        name: child.field.clone(),
        recursive: cycles.child(index, child.target),
        shown,
    }
}

/// A struct's fields in the order they are written, `user_data` among them.
pub fn struct_fields(
    names: &Names<'_>,
    cycles: &Cycles,
    index: usize,
    node: &Node,
    form: Form,
) -> Vec<Field> {
    let mut fields: Vec<Field> = Vec::new();
    for attribute in &node.attributes {
        let written = attribute
            .namespace
            .and_then(|id| names.catalog.prefixes.get(usize::from(id)));
        let tag = written.map_or_else(
            || attribute.local.clone(),
            |prefix| format!("{prefix}:{}", attribute.local),
        );
        fields.push(Field {
            conversion: Conversion::Move,
            doc: format!("The attribute `{tag}`, when the element has it."),
            name: attribute.field.clone(),
            recursive: false,
            shown: format!("Option<{}>", held_value(names, attribute.value, form)),
        });
    }
    match &node.body {
        Body::Text(text) => fields.push(Field {
            conversion: Conversion::Move,
            doc: "The element's text.".to_owned(),
            name: "value".to_owned(),
            recursive: false,
            shown: held_value(names, *text, form),
        }),
        Body::Children(children) => {
            for child in children {
                fields.push(child_field(names, cycles, index, node, child, form));
            }
        }
    }
    fields.push(Field {
        conversion: Conversion::Move,
        doc: "What a read found that the schemas do not declare, which a write gives back."
            .to_owned(),
        name: "unknown".to_owned(),
        recursive: false,
        shown: format!("{}::Unknown", names.runtime()),
    });
    fields.push(Field {
        conversion: Conversion::Slot,
        doc: "What `X` holds for this node. For the tree a read gives, nothing.".to_owned(),
        name: "user_data".to_owned(),
        recursive: false,
        shown: if form == Form::Mirror {
            "()".to_owned()
        } else {
            format!("X::{}", user_data::name(names.catalog, node))
        },
    });
    fields.sort_by(|left, right| bare(&left.name).cmp(bare(&right.name)));
    fields
}

/// Every file of the typed tree, with its path and text: each namespace
/// module in the folder of its crate, with the hook module of a base or core
/// namespace in `officekit-markup`; each crate's `modules.rs`; and `root.rs`
/// in `officekit-markup`.
///
/// # Errors
///
/// A name that cannot be written, a module with no crate, or a folder that
/// cannot be read.
pub fn files<S>(system: &S, header: &str, catalog: &Catalog) -> Result<Vec<(PathBuf, String)>>
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
    let opened = opened(catalog)?;
    let cycles = Cycles::new(catalog);
    let ext = Path::new(Crate::Ext.folder());
    let mut files = Vec::new();
    for module in &order {
        let names = functions(catalog, *module)?;
        let name = &catalog.modules[*module].name;
        let home = *catalog
            .crates
            .get(*module)
            .with_context(|| format!("{name} has no crate"))?;
        let folder = Path::new(home.folder());
        let tests = ready_now(system.is_file(&folder.join(name).join("tests.rs"))).unwrap_or(false);
        files.push((
            folder.join(format!("{name}.rs")),
            module_text(header, &names, &cycles, [&listed, &opened], tests)?,
        ));
        if let Some(text) = archive::text(header, &functions(catalog, *module)?, &cycles) {
            files.push((folder.join(name).join("archive.rs"), text));
        }
        files.push((
            folder.join(name).join("read.rs"),
            readers::text(header, &names),
        ));
        files.push((
            folder.join(name).join("write.rs"),
            writers::text(header, &names),
        ));
        if home != Crate::Ext {
            files.push((
                ext.join(format!("{name}.rs")),
                traversal::hooks_text(header, &names, &listed, home),
            ));
        }
    }
    files.push((
        Path::new(Crate::Base.folder()).join("user_data.rs"),
        user_data::text(header, catalog)?,
    ));
    files.push((
        Path::new(user_data::MACROS_FOLDER).join("kinds.rs"),
        user_data::kinds_text(header, catalog)?,
    ));
    for krate in Crate::ALL {
        let mut modules = String::from(header);
        modules.push_str(
            "// The namespace modules, `crate::<namespace>`, brought into the crate root\n\
             // with `include!` ahead of its imports.\n\n",
        );
        for module in &order {
            // Ext has a module for every namespace: base's and core's for their hooks.
            if krate == Crate::Ext || catalog.crates.get(*module) == Some(&krate) {
                let _ = writeln!(modules, "pub mod {};", catalog.modules[*module].name);
            }
        }
        files.push((Path::new(krate.folder()).join("modules.rs"), modules));
    }
    let mut root = String::from(header);
    root.push_str(&traversal::root_text(catalog, &order));
    files.push((ext.join("root.rs"), root));
    Ok(files)
}

/// The modules each module's items name, by the paths `Names::path` gives
/// them: through its structs' fields, its enums' arms, the `From` of the
/// enums it converts from, its newtypes' member and item types, and the tag
/// tables its shapes read. Its hook traits name none.
///
/// # Errors
///
/// A name that cannot be written.
pub fn references(catalog: &Catalog) -> Result<Vec<BTreeSet<usize>>> {
    let cycles = Cycles::new(catalog);
    (0..catalog.modules.len())
        .map(|module| {
            let names = functions(catalog, module)?;
            ModuleItems::collect(&names, &cycles, &mut Uses::default(), &BTreeSet::new())?;
            Ok(names.named.take())
        })
        .collect()
}

/// The enums a `From` of another crate matches arm by arm: such a match on
/// a `#[non_exhaustive]` enum would need the `_` arm clippy's
/// `wildcard_enum_match_arm` refuses.
fn opened(catalog: &Catalog) -> Result<BTreeSet<usize>> {
    let mut opened = BTreeSet::new();
    for (smaller, larger) in &catalog.conversions {
        let names = functions(catalog, catalog.enums[*larger].module)?;
        if names.crosses(catalog.enums[*smaller].module)
            && pair(&names, *smaller, *larger).match_from().is_some()
        {
            opened.insert(*smaller);
        }
    }
    Ok(opened)
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
        named: RefCell::default(),
        node_functions,
    })
}

/// A namespace module's file; one of `officekit-markup` holds its hook
/// traits. `listed` are the enums a list holds, `opened` those that are not
/// `#[non_exhaustive]`.
fn module_text(
    header: &str,
    names: &Names<'_>,
    cycles: &Cycles,
    [listed, opened]: [&BTreeSet<usize>; 2],
    tests: bool,
) -> Result<String> {
    let catalog = names.catalog;
    let module = &catalog.modules[names.module];
    let mut out = String::from(header);
    let hooks = if catalog.crates.get(names.module) == Some(&Crate::Ext) {
        "\n//!\n//! `Context` and `Visitor` hold the hooks that replace or look at each node."
    } else {
        ""
    };
    let _ = writeln!(
        out,
        "//! The typed tree of the namespace `{}`, which files write with the prefix `{}`.\n\
         //!\n\
         //! The module has the structs, enums and simple-type newtypes of the namespace's\n\
         //! elements. The `read` module has a function that reads an element into each, and the\n\
         //! `write` module one that writes each back to an element.{hooks}\n",
        module.uri, module.prefix
    );
    if catalog
        .enums
        .iter()
        .any(|found| found.module == names.module)
        || catalog.nodes.iter().any(|node| node.module == names.module)
    {
        out.push_str("#[cfg(feature = \"binary-format\")]\nmod archive;\n");
    }
    out.push_str("pub mod read;\npub mod write;\n");
    if tests {
        out.push_str("\n#[cfg(test)]\nmod tests;\n");
    }
    let mut uses = Uses::default();
    let mut parts = ModuleItems::collect(names, cycles, &mut uses, opened)?;
    if catalog.crates.get(names.module) == Some(&Crate::Ext) {
        parts.items.push((
            "Context".to_owned(),
            traversal::context_trait(names, listed),
        ));
        parts
            .items
            .push(("Visitor".to_owned(), traversal::visitor_trait(names)));
        parts.items.sort_by(|left, right| left.0.cmp(&right.0));
        traversal::hook_uses(names, &mut uses);
    }
    let mut rest = parts.statics.clone();
    for (_, text) in parts.items.iter().chain(&parts.impls) {
        rest.push_str(text);
    }
    rest.push_str(&parts.helpers);
    uses.scan(names.runtime(), &rest, &parts.helpers);
    names.imports(&mut uses);
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
#[must_use = "this only returns the value"]
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
    Expr::Tuple(vec![
        Expr::Atom(space(names, namespace, namespaces)),
        Expr::Atom(layout::string(local)),
    ])
}

/// A tag's namespace, `NS_W`, or `None`.
fn space(
    names: &Names<'_>,
    namespace: Option<u8>,
    namespaces: &mut BTreeSet<Option<u8>>,
) -> String {
    namespaces.insert(namespace);
    namespace.map_or_else(
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
    )
}

/// The tag literals an enum reads a struct's arm by: its own, then each of its other names.
fn arm_tags(names: &Names<'_>, node: &Node, namespaces: &mut BTreeSet<Option<u8>>) -> Vec<Expr> {
    iter::once((node.namespace, node.key.tag.local.as_str()))
        .chain(node.also.iter().map(|(id, local)| (*id, local.as_str())))
        .map(|(id, local)| tag(names, id, local, namespaces))
        .collect()
}

/// The pattern an enum's read matches a struct's arm by, its other names nested as clippy's
/// `unnested_or_patterns` asks: `Some((NS_WP | NS_WPS, "wsp"))`.
fn arm_pattern(names: &Names<'_>, node: &Node, namespaces: &mut BTreeSet<Option<u8>>) -> String {
    let mut by_local: Vec<(&str, Vec<String>)> = Vec::new();
    for (id, local) in iter::once((node.namespace, node.key.tag.local.as_str()))
        .chain(node.also.iter().map(|(id, local)| (*id, local.as_str())))
    {
        let constant = space(names, id, namespaces);
        if let Some((_, spaces)) = by_local.iter_mut().find(|(known, _)| *known == local) {
            spaces.push(constant);
        } else {
            by_local.push((local, vec![constant]));
        }
    }
    let alternatives: Vec<String> = by_local
        .iter()
        .map(|(local, spaces)| format!("({}, {})", spaces.join(" | "), layout::string(local)))
        .collect();
    format!("Some({})", alternatives.join(" | "))
}

/// A struct's definition, `NameWith<X>`, with the alias `Name` for the tree a
/// read gives and its `Eq`.
fn struct_text(names: &Names<'_>, cycles: &Cycles, index: usize, node: &Node) -> String {
    let with = generic_name(&node.name);
    let mut out = doc(0, &node.doc);
    out.push_str("#[derive(Clone, Debug, Default, PartialEq)]\n#[non_exhaustive]\n");
    let _ = writeln!(out, "pub struct {with}<X: UserData> {{");
    for field in struct_fields(names, cycles, index, node, Form::Public) {
        let (name, shown) = (field.name, field.shown);
        out.push_str(&layout::doc(4, &field.doc));
        let line = format!("    pub {name}: {shown},");
        if line.len() <= MAX_WIDTH {
            out.push_str(&line);
            out.push('\n');
        } else {
            let _ = writeln!(out, "    pub {name}:\n        {shown},");
        }
    }
    out.push_str("}\n\n");
    let prefix = &names.catalog.modules[node.module].prefix;
    out.push_str(&layout::doc(
        0,
        &format!(
            "The `{prefix}:{}` element of the tree a read gives: [`{with}`] holding nothing in `user_data`.",
            node.key.tag.local
        ),
    ));
    out.push_str(&alias(&node.name, &with));
    out
}

/// A struct's `into_user_data` method: the node with each `user_data`, its own and every
/// descendant's, at the new kind's default and every other field moved; past
/// `BODY_LINES` lines, the fields are moved by helpers.
fn into_user_data_text(
    names: &Names<'_>,
    cycles: &Cycles,
    index: usize,
    node: &Node,
    helpers: &mut String,
) -> String {
    let with = generic_name(&node.name);
    let runtime = names.runtime();
    let fields = struct_fields(names, cycles, index, node, Form::Public);
    let literal = Expr::Struct(
        with.clone(),
        fields
            .iter()
            .map(|field| (field.name.clone(), moved_value(runtime, field)))
            .collect(),
    );
    let mut body = String::new();
    line("        ", &literal, "", &mut body);
    for field in &fields {
        if matches!(field.conversion, Conversion::Move) {
            let label = &field.name;
            body = body.replace(
                &format!("{label}:\n                self.{label},"),
                &format!("{label}: self\n                .{label},"),
            );
        }
    }
    let flat = body.lines().count() + 2 < BODY_LINES;
    let takes_option = fields
        .iter()
        .any(|field| matches!(field.conversion, Conversion::Child(Occurs::Optional, _)));
    let summary = "This node and every node under it with each `user_data` at `Y`'s default; every other \
                   field moves as it is.";
    let mut out = String::from("\n");
    out.push_str(&layout::doc(4, summary));
    out.push_str(
        "    #[must_use = \"the tree moves into the one returned; dropping it drops the tree\"]\n",
    );
    out.push_str(&layout::signature_where(
        4,
        "pub fn into_user_data<Y>",
        &[if flat && !takes_option {
            "self"
        } else {
            "mut self"
        }
        .to_owned()],
        &format!("{with}<Y>"),
        &["Y: UserData"],
    ));
    if flat {
        out.push_str(&body);
    } else {
        let function = names
            .node_functions
            .get(&index)
            .cloned()
            .unwrap_or_default();
        let _ = writeln!(out, "        let mut node = {with}::<Y>::default();");
        let mut chunks: Vec<String> = Vec::new();
        let mut chunk = String::new();
        for field in &fields {
            let mut text = String::new();
            if let Some(value) = assigned_value(runtime, field) {
                statement(&format!("    node.{} = ", field.name), &value, &mut text);
            }
            if !chunk.is_empty() && chunk.lines().count() + text.lines().count() >= BODY_LINES - 4 {
                chunks.push(mem::take(&mut chunk));
            }
            chunk.push_str(&text);
        }
        chunks.push(chunk);
        for (number, text) in chunks.iter().enumerate() {
            let helper = format!("into_user_data_{function}_{}", number + 1);
            let _ = writeln!(out, "        {helper}(&mut self, &mut node);");
            let _ = write!(
                helpers,
                "\n{}",
                layout::signature_where(
                    0,
                    &format!("fn {helper}<X, Y>"),
                    &[
                        format!("from: &mut {with}<X>"),
                        format!("node: &mut {with}<Y>")
                    ],
                    "",
                    &["X: UserData", "Y: UserData"],
                )
            );
            helpers.push_str(text);
            helpers.push_str("}\n");
        }
        out.push_str("        node\n");
    }
    out.push_str("    }\n");
    out
}

/// A field of the struct `into_user_data` builds, converted from `self`.
fn moved_value(runtime: &str, field: &Field) -> Expr {
    let label = &field.name;
    match &field.conversion {
        Conversion::Move => Expr::atom(&format!("self.{label}")),
        Conversion::Slot => Expr::atom("Default::default()"),
        Conversion::Child(Occurs::Optional, generic) => converted(
            runtime,
            Occurs::Optional,
            generic,
            Expr::atom(&format!("&mut self.{label}")),
        ),
        Conversion::Child(Occurs::Required, generic) => converted(
            runtime,
            Occurs::Required,
            generic,
            Expr::atom(&format!("*self.{label}")),
        ),
        Conversion::Child(occurs, generic) => converted(
            runtime,
            *occurs,
            generic,
            Expr::atom(&format!("self.{label}")),
        ),
    }
}

/// A field of the struct `into_user_data` builds, converted from `from` by a helper; `None` for the
/// slot, which `Default` already left at its default.
fn assigned_value(runtime: &str, field: &Field) -> Option<Expr> {
    let label = &field.name;
    let taken = Expr::call("mem::take", vec![Expr::atom(&format!("&mut from.{label}"))]);
    match &field.conversion {
        Conversion::Slot => None,
        Conversion::Move => Some(taken),
        Conversion::Child(Occurs::Optional, generic) => Some(converted(
            runtime,
            Occurs::Optional,
            generic,
            Expr::atom(&format!("&mut from.{label}")),
        )),
        Conversion::Child(Occurs::Required, generic) => Some(converted(
            runtime,
            Occurs::Required,
            generic,
            Expr::Atom(format!("*{}", taken.one_line())),
        )),
        Conversion::Child(occurs, generic) => Some(converted(runtime, *occurs, generic, taken)),
    }
}

/// `value`, the content of a child field, converted through the generic type `generic`.
fn converted(runtime: &str, occurs: Occurs, generic: &str, value: Expr) -> Expr {
    let each = Expr::atom(&format!("{generic}::into_user_data"));
    let function = match occurs {
        Occurs::List => "into_user_data_list",
        Occurs::Optional => "into_user_data_optional",
        Occurs::Required => "into_user_data_required",
    };
    Expr::call(&format!("{runtime}::{function}"), vec![value, each])
}

/// An enum's `into_user_data`: each arm converted into the new kind's arm, `Other` as it is; past
/// `BODY_LINES` lines, the runtime's `into_user_data_match!` called with every variant, which expands
/// to the match.
fn enum_into_user_data(found: &EnumDef, runtime: &str) -> String {
    let with = generic_name(&found.name);
    let mut arms: Vec<(String, Expr)> = found
        .arms
        .iter()
        .map(|(variant, arm)| {
            let converted = if found.boxed.contains(arm) {
                Expr::call(
                    "Box::new",
                    vec![Expr::call("(*node).into_user_data", Vec::new())],
                )
            } else {
                Expr::call("node.into_user_data", Vec::new())
            };
            (variant.clone(), converted)
        })
        .collect();
    arms.push(("Other".to_owned(), Expr::atom("node")));
    arms.sort_by(|left, right| left.0.cmp(&right.0));
    let summary = "This item and every node under it with each `user_data` at `Y`'s default.";
    let mut out = format!("\nimpl<X: UserData> {with}<X> {{\n");
    out.push_str(&layout::doc(4, summary));
    out.push_str(
        "    #[must_use = \"the tree moves into the one returned; dropping it drops the tree\"]\n",
    );
    out.push_str(&layout::signature_where(
        4,
        "pub fn into_user_data<Y>",
        &["self".to_owned()],
        &format!("{with}<Y>"),
        &["Y: UserData"],
    ));
    let mut matched = String::from("        match self {\n");
    for (variant, converted) in &arms {
        let body = if variant == "Other" {
            Expr::call(&format!("{with}::Other"), vec![Expr::atom("node")])
        } else {
            Expr::call(&format!("{with}::{variant}"), vec![converted.clone()])
        };
        matched.push_str(&layout::arm(
            12,
            &Expr::pattern(&format!("Self::{variant}"), "node"),
            &body,
            false,
        ));
    }
    matched.push_str("        }\n");
    if matched.lines().count() + 2 < BODY_LINES {
        out.push_str(&matched);
    } else {
        let _ = writeln!(
            out,
            "        {runtime}::into_user_data_match!(self, {with};"
        );
        for (label, boxed) in [("plain", false), ("boxed", true)] {
            let mut row = String::new();
            let _ = writeln!(out, "            {label} [");
            for (variant, arm) in &found.arms {
                if found.boxed.contains(arm) != boxed {
                    continue;
                }
                let item = format!("{variant},");
                if !row.is_empty() && 16 + row.len() + 1 + item.len() > MAX_WIDTH {
                    let _ = writeln!(out, "                {row}");
                    row.clear();
                }
                if !row.is_empty() {
                    row.push(' ');
                }
                row.push_str(&item);
            }
            if !row.is_empty() {
                let _ = writeln!(out, "                {row}");
            }
            out.push_str("            ]\n");
        }
        out.push_str("        )\n");
    }
    out.push_str("    }\n}\n");
    out
}

/// The alias `name` of the generic type `with` for the tree a read gives, and its `Eq`.
fn alias(name: &str, with: &str) -> String {
    let flat = format!("pub type {name} = {with}<()>;");
    let declared = if flat.len() <= MAX_WIDTH {
        flat
    } else {
        format!("pub type {name} =\n    {with}<()>;")
    };
    format!("{declared}\n\nimpl Eq for {with}<()> {{}}\n")
}

/// A struct's `without_children`: a struct literal of its attributes and its
/// text value copied, each declared child and `unknown` at their default.
fn without_children_text(node: &Node) -> String {
    let mut copied: Vec<&str> = node
        .attributes
        .iter()
        .map(|attribute| attribute.field.as_str())
        .collect();
    let summary = match node.body {
        Body::Text(_) => {
            copied.push("value");
            "This node's attributes and value, copied, with no unknown content."
        }
        Body::Children(_) => {
            "This node's attributes, copied, with no declared child and no unknown content."
        }
    };
    copied.sort_by(|left, right| bare(left).cmp(bare(right)));
    let mut out = format!(
        "\nimpl<X: UserData> {}<X> {{\n    /// {summary}\n    #[must_use = \"this returns a copy; the node is unchanged\"]\n    \
         pub fn without_children(&self) -> Self {{\n",
        generic_name(&node.name)
    );
    if copied.is_empty() {
        out.push_str("        Self::default()\n");
    } else {
        out.push_str("        Self {\n");
        for field in copied {
            let flat = format!("            {field}: self.{field}.clone(),\n");
            if flat.len() - 1 <= MAX_WIDTH {
                out.push_str(&flat);
            } else {
                // rustfmt breaks the chain a field this long reads.
                let _ = write!(
                    out,
                    "            {field}: self\n                .{field}\n                .clone(),\n"
                );
            }
        }
        out.push_str("            ..Self::default()\n        }\n");
    }
    out.push_str("    }\n}\n");
    out
}

/// A field's name without `r#`, for ordering.
fn bare(field: &str) -> &str {
    field.strip_prefix("r#").unwrap_or(field)
}

/// A struct's shape: its tag, declared attributes, and child positions.
fn shape_text(
    names: &Names<'_>,
    index: usize,
    node: &Node,
    namespaces: &mut BTreeSet<Option<u8>>,
) -> String {
    let catalog = names.catalog;
    let runtime = names.runtime();
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
                            Occurs::List => "list",
                            Occurs::Optional => "single",
                            Occurs::Required => "required",
                        };
                        let found = &catalog.nodes[child_index];
                        let mut arguments = vec![tag(
                            names,
                            found.namespace,
                            &found.key.tag.local,
                            namespaces,
                        )];
                        let suffix = if child.also.is_empty() {
                            ""
                        } else {
                            arguments.push(Expr::Slice(
                                child
                                    .also
                                    .iter()
                                    .map(|(id, local)| tag(names, *id, local, namespaces))
                                    .collect(),
                            ));
                            "_also"
                        };
                        Expr::call(&format!("{runtime}::Slot::{function}{suffix}"), arguments)
                    }
                    (Target::Enum(found), occurs) => {
                        let function = match (catalog.enums[found].kind, occurs) {
                            (EnumKind::All, _) => "once",
                            (_, Occurs::List) => "items",
                            (_, Occurs::Optional) => "choice",
                            (_, Occurs::Required) => "required_choice",
                        };
                        Expr::call(
                            &format!("{runtime}::Slot::{function}"),
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
        format!("{runtime}::Shape"),
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
        &layout::static_start(&names.shape(index), &format!("{runtime}::Shape")),
        &expr,
        &mut out,
    );
    out
}

/// An enum's tags: those of its arms, each followed by its other names, and its wildcard's
/// namespaces.
fn tags_text(names: &Names<'_>, found: &EnumDef, namespaces: &mut BTreeSet<Option<u8>>) -> String {
    let catalog = names.catalog;
    let runtime = names.runtime();
    let tags: Vec<Expr> = if found.kind == EnumKind::Wildcard {
        Vec::new()
    } else {
        found
            .arms
            .iter()
            .flat_map(|(_, arm)| arm_tags(names, &catalog.nodes[*arm], namespaces))
            .collect()
    };
    let any = found.wildcard.as_ref().map_or_else(
        || Expr::atom("None"),
        |wildcard| {
            let inner = match wildcard {
                Wildcard::Any => Expr::Atom(format!("{runtime}::Wildcard::Any")),
                Wildcard::Local => Expr::Atom(format!("{runtime}::Wildcard::Local")),
                Wildcard::Not(id) => {
                    namespaces.insert(Some(*id));
                    Expr::call(
                        &format!("{runtime}::Wildcard::Not"),
                        vec![Expr::atom(&layout::integer(i128::from(*id)))],
                    )
                }
                Wildcard::Only(ids) => Expr::call(
                    &format!("{runtime}::Wildcard::Only"),
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
        format!("{runtime}::Accept"),
        vec![
            ("any".to_owned(), any),
            ("tags".to_owned(), Expr::Slice(tags)),
        ],
    );
    let mut out = String::from("\n");
    statement(
        &layout::static_start(
            &format!("{}_TAGS", names::screaming(&found.name)),
            &format!("{runtime}::Accept"),
        ),
        &expr,
        &mut out,
    );
    out
}

/// An enum's definition, `#[non_exhaustive]` unless `open`.
fn enum_text(names: &Names<'_>, found: &EnumDef, open: bool) -> String {
    let mut variants: Vec<(String, String, String)> = found
        .arms
        .iter()
        .map(|(variant, arm)| {
            let node = &names.catalog.nodes[*arm];
            let path = format!("{}<X>", names.path(node.module, &generic_name(&node.name)));
            let shown = if found.boxed.contains(arm) {
                format!("Box<{path}>")
            } else {
                path
            };
            let prefix = &names.catalog.modules[node.module].prefix;
            let item = format!(
                "The element `{prefix}:{}`, a `{}`.",
                node.key.tag.local, node.name
            );
            (variant.clone(), shown, item)
        })
        .collect();
    variants.push((
        "Other".to_owned(),
        "Node".to_owned(),
        "An element or node the schemas do not declare at this position.".to_owned(),
    ));
    variants.sort();
    let mut out = doc(0, &found.doc);
    out.push_str("#[derive(Clone, Debug, PartialEq)]\n");
    if !open {
        out.push_str("#[non_exhaustive]\n");
    }
    let with = generic_name(&found.name);
    let _ = writeln!(out, "pub enum {with}<X: UserData> {{");
    for (variant, shown, item) in variants {
        out.push_str(&layout::doc(4, &item));
        let line = format!("    {variant}({shown}),");
        if line.len() <= MAX_WIDTH {
            out.push_str(&line);
            out.push('\n');
        } else {
            let _ = writeln!(out, "    {variant}(\n        {shown},\n    ),");
        }
    }
    out.push_str("}\n\n");
    out.push_str(&layout::doc(
        0,
        &format!(
            "[`{with}`] of the tree a read gives, where no node holds anything in `user_data`."
        ),
    ));
    out.push_str(&alias(&found.name, &with));
    out
}

/// A struct's `Read` and `Write` implementations, which call its reader and writer.
fn node_impls(names: &Names<'_>, index: usize, node: &Node) -> String {
    let function = names
        .node_functions
        .get(&index)
        .cloned()
        .unwrap_or_default();
    let runtime = names.runtime();
    let mut out = String::from("\n");
    out.push_str(&layout::impl_header(
        &format!("{runtime}::Read"),
        &node.name,
    ));
    out.push_str("    fn read(element: &Element) -> Self {\n");
    line(
        "        ",
        &Expr::call(&format!("read::{function}"), vec![Expr::atom("element")]),
        "",
        &mut out,
    );
    out.push_str("    }\n}\n\n");
    out.push_str(&layout::impl_header(
        &format!("{runtime}::Write"),
        &node.name,
    ));
    let _ = writeln!(
        out,
        "    fn write_into(&self, writer: &mut {runtime}::Writer<'_, '_>) {{"
    );
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
    out.push_str(&enum_write(names.runtime(), found, &function, &mut helpers));
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
            layout::generic_impl_header(
                "<X: UserData>",
                "Default",
                &format!("{}<X>", generic_name(&found.name))
            )
        );
        let value = if found.boxed.contains(arm) {
            "Box::default()".to_owned()
        } else {
            format!(
                "{}::default()",
                names.path(node.module, &generic_name(&node.name))
            )
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
/// `CHUNK` arms, or `BODY_LINES` lines, the match is split into helpers tried
/// in turn, each of at most `CHUNK / 2` arms.
fn enum_read(
    names: &Names<'_>,
    found: &EnumDef,
    function: &str,
    namespaces: &mut BTreeSet<Option<u8>>,
    helpers: &mut String,
) -> String {
    let catalog = names.catalog;
    let runtime = names.runtime();
    let mut out = String::from("\n");
    out.push_str(&layout::impl_header(
        &format!("{runtime}::Read"),
        &found.name,
    ));
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
            let pattern = arm_pattern(names, &catalog.nodes[*arm], namespaces);
            let read = Expr::call(&format!("{runtime}::read"), vec![Expr::atom("element")]);
            let value = if found.boxed.contains(arm) {
                Expr::call("Box::new", vec![read])
            } else {
                read
            };
            (pattern, variant.clone(), value)
        })
        .collect();
    let arm_text =
        |indent: usize, own: &str, (pattern, variant, value): &(String, String, Expr)| {
            let body = Expr::call(&format!("{own}::{variant}"), vec![value.clone()]);
            layout::arm(indent, &Expr::Atom(pattern.clone()), &body, false)
        };
    let matched = (arm_lines.len() <= CHUNK).then(|| {
        let mut text = format!("        match {runtime}::name(element) {{\n");
        for arm in &arm_lines {
            text.push_str(&arm_text(12, "Self", arm));
        }
        text.push_str(&layout::arm(12, &Expr::atom("_"), &other, false));
        text.push_str("        }\n");
        text
    });
    if arm_lines.is_empty() {
        line("        ", &other, "", &mut out);
    } else if let Some(text) = matched.filter(|text| text.lines().count() < BODY_LINES) {
        out.push_str(&text);
    } else {
        let mut chunks: Vec<String> = Vec::new();
        let mut chunk = String::new();
        let mut count = 0;
        for arm in &arm_lines {
            let text = arm_text(8, &found.name, arm);
            if !chunk.is_empty()
                && (count == CHUNK / 2
                    || chunk.lines().count() + text.lines().count() >= BODY_LINES)
            {
                chunks.push(mem::take(&mut chunk));
                count = 0;
            }
            chunk.push_str(&text);
            count += 1;
        }
        chunks.push(chunk);
        let mut calls = Vec::new();
        for (index, text) in chunks.iter().enumerate() {
            let helper = format!("read_{function}_{}", index + 1);
            if index > 0 {
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
            let _ = writeln!(helpers, "    let item = match {runtime}::name(element) {{");
            helpers.push_str(text);
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
/// arms, the arms are written by helpers. `runtime` is the path to the runtime.
fn enum_write(runtime: &str, found: &EnumDef, function: &str, helpers: &mut String) -> String {
    let mut out = String::from("\n");
    out.push_str(&layout::impl_header(
        &format!("{runtime}::Write"),
        &found.name,
    ));
    let _ = writeln!(
        out,
        "    fn write_into(&self, writer: &mut {runtime}::Writer<'_, '_>) {{"
    );
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
                        format!("writer: &mut {runtime}::Writer<'_, '_>")
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

/// The pair of enums `smaller` and `larger`, seen from the larger's module.
fn pair<'pair>(names: &Names<'pair>, smaller: usize, larger: usize) -> Pair<'pair> {
    let catalog = names.catalog;
    Pair {
        large: &catalog.enums[larger],
        large_variants: variants(&catalog.enums[larger]),
        runtime: names.runtime(),
        small: &catalog.enums[smaller],
        small_path: names.path(catalog.enums[smaller].module, &catalog.enums[smaller].name),
    }
}

/// `From<smaller> for larger` and `TryFrom<larger> for smaller`, and the
/// helpers either calls when its match is too long for one function.
fn conversion(names: &Names<'_>, smaller: usize, larger: usize) -> (String, String) {
    let pair = pair(names, smaller, larger);
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
            chunks.push(mem::take(&mut chunk));
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

/// The `use` lines of `paths`, with their sort keys:
/// the names of one module join in one line.
fn use_lines<'paths>(paths: &'paths BTreeSet<String>, lines: &mut Vec<(Vec<&'paths str>, String)>) {
    let mut grouped: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for path in paths {
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
}

#[cfg(test)]
mod tests;
