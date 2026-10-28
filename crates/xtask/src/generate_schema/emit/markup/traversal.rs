//! The traversals of the typed tree: each namespace's
//! hook traits, `Context` and `Visitor`, and the root accessors, one per
//! namespace, in `officekit_markup`; every node's `Transform`, `Visit`, and
//! `WriteThrough`, and each enum's `Item`, in the namespace's module of
//! `officekit_strict::traversal`.
//!
//! A struct's hook is its reader's name, its name in snake case; an enum's is
//! its name without the trailing `Content`, or the full name when that is a
//! struct's. A list hook adds `_items` to its enum's hook, and a write hook
//! puts `write_` before a tree or list hook.

use alloc::collections::BTreeSet;
use core::fmt::Write as _;
use core::iter;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::Crate;
use super::render::layout::{self, Expr, MAX_WIDTH};
use super::render::{self, BODY_LINES, Names, Uses};
use super::structs::{Body, Catalog, EnumDef, Node, Occurs, Target};

/// The root context's hooks for content the schemas do not declare, as the
/// runtime declared them.
const ELEMENT_HOOKS: [(&str, &str); 2] = [
    (
        "element",
        "    /// Asked for each generic element: an unknown element in a list's `Other`
    /// items, in a node's `unknown.children`, or in a wildcard position.
    /// `None` rebuilds the element from its transformed children.
    fn element(&mut self, _node: &Element) -> Option<Element> {
        None
    }
",
    ),
    (
        "write_element",
        "    /// Asked for each generic element written through, before `element`;
    /// `Written::Yes` means the hook wrote it.
    ///
    /// # Errors
    ///
    /// The sink's.
    fn write_element(&mut self, _node: &Element, _out: &mut PartWriter<'_>) -> io::Result<Written> {
        Ok(Written::No)
    }
",
    ),
];

/// The root visitor's hook for content the schemas do not declare.
const ELEMENT_VISITOR_HOOK: &str =
    "    /// Shown each generic element; `Descend::Yes` walks into its children.
    fn element(&mut self, _node: &Element) -> Descend {
        Descend::Yes
    }
";

/// The name prefixes clippy's `wrong_self_convention` refuses for a method
/// that takes `&mut self`.
const SELFLESS_PREFIXES: [&str; 3] = ["from_", "into_", "to_"];

/// The `if let` blocks a helper of an enum with many arms holds, below
/// clippy's `cognitive_complexity`.
const BLOCKS: usize = 20;

/// The indent of a statement in a method of an `impl`.
const BODY: usize = 8;

/// The parameters of a write-through method after `&self`.
const WRITE_PARAMETERS: [&str; 2] = [
    "ctx: &mut dyn officekit_markup::Context",
    "out: &mut PartWriter<'_>",
];

/// Which traversal an enum's arm takes its item through.
#[derive(Clone, Copy)]
enum Traversal {
    Transform,
    Visit,
    Write,
}

impl Traversal {
    /// What an arm does with the item `binding` holds: an expression of
    /// `own`, the enum's path, for `transform`, a statement otherwise.
    fn body(self, own: &str, variant: &str, binding: &str, boxed: bool) -> Expr {
        match self {
            Self::Transform => {
                let transformed =
                    Expr::call(&format!("{binding}.transform"), vec![Expr::atom("ctx")]);
                let value = if boxed {
                    Expr::call("Box::new", vec![transformed])
                } else {
                    transformed
                };
                Expr::call(&format!("{own}::{variant}"), vec![value])
            }
            Self::Visit => Expr::call(&format!("{binding}.visit"), vec![Expr::atom("visitor")]),
            Self::Write => Expr::call(
                &format!("{binding}.write_through"),
                vec![Expr::atom("ctx"), Expr::atom("out")],
            ),
        }
    }
}

/// The traversal one child field takes in a node's walk and write: one node
/// or a list of a struct type, a single child of an enum type, or a list of
/// an enum type, whose list hooks are asked.
#[derive(Clone, Copy)]
enum Step {
    Child,
    Choice,
    Items,
}

impl Step {
    /// The method of `officekit_strict::traversal::Walk` and
    /// `officekit_strict::traversal::Through` this step calls.
    const fn method(self) -> &'static str {
        match self {
            Self::Child => "child",
            Self::Choice => "choice",
            Self::Items => "items",
        }
    }

    const fn of(target: Target, occurs: Occurs) -> Self {
        match (target, occurs) {
            (Target::Node(_), _) => Self::Child,
            (Target::Enum(_), Occurs::List) => Self::Items,
            (Target::Enum(_), Occurs::Optional | Occurs::Required) => Self::Choice,
        }
    }
}

/// The enums a list holds; each has a list hook and a list write hook.
#[must_use]
pub fn listed(catalog: &Catalog) -> BTreeSet<usize> {
    catalog
        .nodes
        .iter()
        .filter_map(|node| match &node.body {
            Body::Children(children) => Some(children),
            Body::Text(_) => None,
        })
        .flatten()
        .filter(|child| child.occurs == Occurs::List)
        .filter_map(|child| match child.target {
            Target::Enum(found) => Some(found),
            Target::Node(_) => None,
        })
        .collect()
}

/// Whether a namespace module has a struct or an enum, which its module of
/// `officekit_strict::traversal` implements the traversals for.
#[must_use]
pub fn has_nodes(names: &Names<'_>) -> bool {
    !names.node_functions.is_empty() || !names.enum_functions.is_empty()
}

/// A hook's name without the `r#` a keyword takes, for the names built on it.
fn bare(hook: &str) -> &str {
    hook.strip_prefix("r#").unwrap_or(hook)
}

/// A node kind's tree hook, from its reader's name: the same name, but
/// `node_` before one that starts as clippy's `wrong_self_convention` keeps
/// for a method that takes no `&mut self` (`from_color` is
/// `node_from_color`).
fn hook_name(function: &str) -> String {
    let name = bare(function);
    if SELFLESS_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        format!("node_{name}")
    } else {
        function.to_owned()
    }
}

/// The hooks of one namespace module: each node kind's tree hook and type,
/// structs and enums, and whether a list of it is held.
fn hooks<'names>(
    names: &'names Names<'_>,
    listed: &BTreeSet<usize>,
) -> Vec<(String, &'names str, bool)> {
    let catalog = names.catalog;
    let mut out = Vec::new();
    for (index, function) in &names.node_functions {
        out.push((
            hook_name(function),
            catalog.nodes[*index].name.as_str(),
            false,
        ));
    }
    for (index, function) in &names.enum_functions {
        out.push((
            hook_name(function),
            catalog.enums[*index].name.as_str(),
            listed.contains(index),
        ));
    }
    out
}

/// A trait's hooks in alphabetical order, a blank line between them.
fn joined(mut entries: Vec<(String, String)>) -> String {
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
        .into_iter()
        .map(|(_, text)| text)
        .collect::<Vec<String>>()
        .join("\n")
}

/// A hook answering `answer` by default.
fn hook_text(name: &str, parameters: &[String], output: &str, answer: &str) -> String {
    let mut text = layout::signature(4, &format!("fn {name}"), parameters, output);
    let _ = write!(text, "        {answer}\n    }}\n");
    text
}

/// A hook preceded by its doc.
fn documented(doc: &str, text: &str) -> String {
    format!("{}{text}", layout::doc(4, doc))
}

/// A write hook, `Written::No` by default; `list` for the write hook of a list.
fn write_hook_text(name: &str, node: &str, list: bool) -> String {
    let what = if list {
        format!("a `{node}` in a list")
    } else {
        format!("a `{node}`")
    };
    let mut text = layout::doc(
        4,
        &format!(
            "Asked when {what} is written through. `Written::Yes` means the hook wrote it, and \
             `Written::No` leaves it to the tree hook.\n\n# Errors\n\nThe errors of the sink the \
             part is written to."
        ),
    );
    text.push_str(&hook_text(
        name,
        &[
            "&mut self".to_owned(),
            format!("_node: &{node}"),
            "_out: &mut PartWriter<'_>".to_owned(),
        ],
        "io::Result<Written>",
        "Ok(Written::No)",
    ));
    text
}

/// A namespace module's `Context`: one tree hook and one write hook per node
/// kind, and one list hook and one list write hook per enum a list holds.
#[must_use]
pub fn context_trait(names: &Names<'_>, listed: &BTreeSet<usize>) -> String {
    let module = &names.catalog.modules[names.module];
    let mut entries = Vec::new();
    for (hook, node, list) in hooks(names, listed) {
        let own = format!("_node: &{node}");
        entries.push((
            bare(&hook).to_owned(),
            documented(
                &format!(
                    "Asked for each `{node}`. `None` rebuilds it from its transformed children, \
                     and `Some` puts the node it holds in its place."
                ),
                &hook_text(
                    &hook,
                    &["&mut self".to_owned(), own.clone()],
                    &format!("Option<{node}>"),
                    "None",
                ),
            ),
        ));
        let write = format!("write_{}", bare(&hook));
        entries.push((write.clone(), write_hook_text(&write, node, false)));
        if list {
            let items = format!("{}_items", bare(&hook));
            entries.push((
                items.clone(),
                documented(
                    &format!(
                        "Asked for each `{node}` in a list. `None` leaves the item to its own \
                         `transform`, and `Some` puts the items it holds, any number of them, in \
                         its place."
                    ),
                    &hook_text(
                        &items,
                        &["&mut self".to_owned(), own],
                        &format!("Option<Vec<{node}>>"),
                        "None",
                    ),
                ),
            ));
            let write_items = format!("write_{items}");
            entries.push((
                write_items.clone(),
                write_hook_text(&write_items, node, true),
            ));
        }
    }
    let mut out = layout::doc(
        0,
        &format!(
            "The hooks of a context for the `{prefix}` namespace.\n\nA tree hook \
             and a write hook per node kind, and a list hook and a list write hook per enum a list \
             holds. A tree hook answers `None` to rebuild the node from its transformed \
             children, or a node in its place; a list hook answers `None` to leave the item to its \
             own `transform`, or any number of items, none included, in its place; a write hook \
             answers `Written::No` to leave the node to its tree hook, or writes in its place and \
             answers `Written::Yes`. A context that does not return itself from \
             `crate::Context::{accessor}` is never asked.",
            prefix = module.prefix,
            accessor = module.name,
        ),
    );
    out.push_str(&trait_text("pub trait Context: crate::Context", entries));
    out
}

/// A trait's declaration and its hooks, `{}` when it has none.
fn trait_text(head: &str, entries: Vec<(String, String)>) -> String {
    if entries.is_empty() {
        return format!("{head} {{}}\n");
    }
    format!("{head} {{\n{}}}\n", joined(entries))
}

/// A namespace module's `Visitor`: one hook per node kind.
#[must_use]
pub fn visitor_trait(names: &Names<'_>) -> String {
    let module = &names.catalog.modules[names.module];
    let descend = format!("{}::Descend", Crate::Base.name());
    let entries = hooks(names, &BTreeSet::new())
        .into_iter()
        .map(|(hook, node, _)| {
            (
                bare(&hook).to_owned(),
                documented(
                    &format!(
                        "Shown each `{node}`. `Descend::Yes` walks into its children, and \
                         `Descend::No` skips them."
                    ),
                    &hook_text(
                        &hook,
                        &["&mut self".to_owned(), format!("_node: &{node}")],
                        &descend,
                        &format!("{descend}::Yes"),
                    ),
                ),
            )
        })
        .collect();
    let mut out = layout::doc(
        0,
        &format!(
            "The hooks of a visitor for the `{prefix}` namespace.\n\nOne per node kind, each \
             answering `Descend::Yes` to walk into the node's children or `Descend::No` to skip \
             them. A visitor that does not return itself from `crate::Visitor::{accessor}` is \
             never asked.",
            prefix = module.prefix,
            accessor = module.name,
        ),
    );
    out.push_str(&trait_text("pub trait Visitor: crate::Visitor", entries));
    out
}

/// Import what a namespace's hook traits name besides its nodes: `Written`,
/// `PartWriter` and `io`, when it has a node.
pub fn hook_uses(names: &Names<'_>, uses: &mut Uses) {
    if has_nodes(names) {
        uses.add(&format!("{}::Written", Crate::Base.name()));
        uses.add("officekit_xml::stream::writer::PartWriter");
        uses.add("std::io");
    }
}

/// The module of `officekit-markup` that holds the hook traits of a
/// namespace of `home`, base or core, beside its nodes, re-exported.
#[must_use]
pub fn hooks_text(
    header: &str,
    names: &Names<'_>,
    listed: &BTreeSet<usize>,
    home: Crate,
) -> String {
    let module = &names.catalog.modules[names.module];
    let mut out = String::from(header);
    let _ = writeln!(
        out,
        "//! The hooks of the `{}` namespace, `Context` and `Visitor`, beside its typed\n\
         //! tree, re-exported from `{}::{}`.\n//!\n//! The namespace is `{}`. Its prefix, `{}`, is the\n\
         //! short name a file gives it in the names of its elements. `Context` replaces nodes,\n\
         //! `Visitor` looks at them, and `officekit_strict::traversal` calls both.\n",
        module.prefix,
        home.name(),
        module.name,
        module.uri,
        module.prefix
    );
    let _ = writeln!(
        out,
        "#[doc(no_inline)]\npub use {}::{}::*;",
        home.name(),
        module.name
    );
    let mut uses = Uses::default();
    hook_uses(names, &mut uses);
    let use_text = uses.text();
    if !use_text.is_empty() {
        out.push('\n');
        out.push_str(&use_text);
    }
    out.push('\n');
    out.push_str(&context_trait(names, listed));
    out.push('\n');
    out.push_str(&visitor_trait(names));
    out
}

/// The root hooks, `root.rs`: the root `Context` with its hooks for
/// content the schemas do not declare and one accessor per namespace, and the
/// root `Visitor` the same way, each in alphabetical order.
#[must_use]
pub fn root_text(catalog: &Catalog, order: &[usize]) -> String {
    let accessor = |module: &str, hooks: &str| {
        documented(
            &format!(
                "The `{module}::{hooks}` hooks. `None` until this opts in by returning `Some(self)`."
            ),
            &hook_text(
                module,
                &["&mut self".to_owned()],
                &format!("Option<&mut dyn {module}::{hooks}>"),
                "None",
            ),
        )
    };
    let mut context: Vec<(String, String)> = ELEMENT_HOOKS
        .iter()
        .map(|(name, text)| ((*name).to_owned(), (*text).to_owned()))
        .collect();
    let mut visitor = vec![("element".to_owned(), ELEMENT_VISITOR_HOOK.to_owned())];
    for module in order {
        let name = &catalog.modules[*module].name;
        context.push((name.clone(), accessor(name, "Context")));
        visitor.push((name.clone(), accessor(name, "Visitor")));
    }
    let mut out = String::from(
        "// The root hooks every namespace's `Context` and `Visitor` extend,\n\
         // brought into the crate root with `include!`.\n\n",
    );
    out.push_str(&layout::doc(
        0,
        "The root of every context, which each namespace's `Context` extends and no other \
         trait.\n\nIt holds the hooks for content the schemas do not declare, and one accessor \
         per namespace, through which a context opts into that namespace's hooks by returning \
         `Some(self)`. An accessor answers `None` by default, so a context is never asked about \
         a namespace it did not opt into.\n\nThe `traversal` module of `officekit-strict` calls the hooks: \
         `Transform` asks the tree and list hooks, and `WriteThrough` asks the write hooks. \
         `read` and `write` never do.",
    ));
    out.push_str("pub trait Context {\n");
    out.push_str(&joined(context));
    out.push_str("}\n\n");
    out.push_str(&layout::doc(
        0,
        "The root of every visitor, which each namespace's `Visitor` extends and no other \
         trait.\n\nIt holds the hook for content the schemas do not declare, and one accessor \
         per namespace, through which a visitor opts into that namespace's hooks by returning \
         `Some(self)`. An accessor answers `None` by default, so a visitor is never shown a \
         namespace it did not opt into.\n\nThe `traversal` module of `officekit-strict` calls the hooks \
         through `Visit`.",
    ));
    out.push_str("pub trait Visitor {\n");
    out.push_str(&joined(visitor));
    out.push_str("}\n");
    out
}

/// The traversal crate's files: one module per namespace module with a struct
/// or an enum, `<namespace>.rs` in `folder`, and `modules.rs` declaring them.
///
/// # Errors
///
/// A name that cannot be written.
pub fn files(header: &str, catalog: &Catalog, folder: &Path) -> Result<Vec<(PathBuf, String)>> {
    let mut order: Vec<usize> = (0..catalog.modules.len()).collect();
    order.sort_by(|left, right| {
        catalog.modules[*left]
            .name
            .cmp(&catalog.modules[*right].name)
    });
    let listed = listed(catalog);
    let mut files = Vec::new();
    let mut modules = String::from(header);
    modules.push_str(
        "// The traversal modules, one per namespace of the typed tree with a node,\n\
         // brought into the `traversal` module with `include!` ahead of its imports.\n\n",
    );
    for module in &order {
        let names = render::functions(catalog, *module)?;
        if !has_nodes(&names) {
            continue;
        }
        let name = &catalog.modules[*module].name;
        files.push((
            folder.join(format!("{name}.rs")),
            text(header, &names, &listed),
        ));
        let _ = writeln!(modules, "mod {name};");
    }
    files.push((folder.join("modules.rs"), modules));
    Ok(files)
}

/// The text of a namespace's module of `officekit_strict::traversal`.
#[must_use]
pub fn text(header: &str, names: &Names<'_>, listed: &BTreeSet<usize>) -> String {
    let catalog = names.catalog;
    let module = &catalog.modules[names.module];
    let mut out = String::from(header);
    let _ = writeln!(
        out,
        "//! The traversals of `officekit_markup::{}`: each node's `Transform`, `Visit`,\n\
         //! and `WriteThrough`, and each enum's `Item`, every hook asked through the\n\
         //! namespace's accessor.\n",
        module.name
    );
    let mut impls: Vec<(&str, String)> = Vec::new();
    let mut helpers = String::new();
    for (index, function) in &names.node_functions {
        let node = &catalog.nodes[*index];
        let mut text = String::new();
        node_impls(
            names,
            *index,
            node,
            &hook_name(function),
            &mut text,
            &mut helpers,
        );
        impls.push((node.name.as_str(), text));
    }
    for (index, function) in &names.enum_functions {
        let found = &catalog.enums[*index];
        let mut text = String::new();
        enum_impls(
            names,
            found,
            &hook_name(function),
            listed.contains(index),
            &mut text,
            &mut helpers,
        );
        impls.push((found.name.as_str(), text));
    }
    impls.sort_by(|left, right| left.0.cmp(right.0));
    // The traits an `impl` of them calls without importing them; free helpers
    // import those they call the methods of. The namespace module is imported
    // because clippy's `absolute_paths` refuses a path of three segments into
    // another crate.
    let mut runtime = Vec::new();
    if helpers.contains(".transform(") {
        runtime.push("Transform as _");
    }
    runtime.push("Unchanged");
    if helpers.contains(".visit(") {
        runtime.push("Visit as _");
    }
    runtime.push("WriteThrough");
    out.push_str("use std::io;\n\n");
    let markup = format!("Descend, Written, {}", module.name);
    let line = format!("use officekit_markup::{{{markup}}};");
    // rustfmt breaks a `use` list past 98 columns.
    if line.len() + 2 <= MAX_WIDTH {
        let _ = writeln!(out, "{line}");
    } else {
        let _ = writeln!(out, "use officekit_markup::{{\n    {markup},\n}};");
    }
    out.push_str("use officekit_xml::stream::writer::PartWriter;\n\n");
    let _ = writeln!(out, "use super::{{{}}};", runtime.join(", "));
    for (_, text) in impls {
        out.push_str(&text);
    }
    out.push_str(&helpers);
    out
}

/// `super::<ask>(<receiver>.<module>(), <module>::<Hooks>::<hook>, self[, out])`.
fn ask(ask: &str, receiver: &str, module: &str, hook: &str, out: bool) -> Expr {
    let mut arguments = vec![
        Expr::call(&format!("{receiver}.{module}"), Vec::new()),
        Expr::Atom(hook.to_owned()),
        Expr::atom("self"),
    ];
    if out {
        arguments.push(Expr::atom("out"));
    }
    Expr::call(&format!("super::{ask}"), arguments)
}

/// `transform`, which asks the node's tree hook first.
fn transform_method(module: &str, hook: &str, out: &mut String) {
    out.push_str("    fn transform(&self, ctx: &mut dyn officekit_markup::Context) -> Self {\n");
    layout::statement(
        "        let node = ",
        &ask(
            "ask",
            "ctx",
            module,
            &format!("{module}::Context::{hook}"),
            false,
        ),
        out,
    );
    out.push_str("        node.unwrap_or_else(|| self.transform_children(ctx))\n    }\n\n");
}

/// `visit`, which shows the node to its hook first.
fn visit_method(module: &str, hook: &str, out: &mut String) {
    out.push_str("    fn visit(&self, visitor: &mut dyn officekit_markup::Visitor) {\n");
    layout::statement(
        "        let next = ",
        &ask(
            "show",
            "visitor",
            module,
            &format!("{module}::Visitor::{hook}"),
            false,
        ),
        out,
    );
    out.push_str(
        "        if next == Descend::Yes {\n            self.visit_children(visitor);\n        }\n    }\n\n",
    );
}

/// `write_through`, which asks the node's write hook, then its tree hook.
fn write_through_method(module: &str, hook: &str, out: &mut String) {
    out.push_str(&write_signature("fn write_through"));
    layout::line(
        "        let written = ",
        &ask(
            "ask_write",
            "ctx",
            module,
            &format!("{module}::Context::write_{}", bare(hook)),
            true,
        ),
        "?;",
        out,
    );
    out.push_str("        if written == Written::Yes {\n            return Ok(());\n        }\n");
    layout::statement(
        "        let replaced = ",
        &ask(
            "ask",
            "ctx",
            module,
            &format!("{module}::Context::{hook}"),
            false,
        ),
        out,
    );
    out.push_str(
        "        if let Some(node) = replaced {\n            \
         return node.write_children_through(&mut Unchanged, out);\n        }\n        \
         self.write_children_through(ctx, out)\n    }\n",
    );
}

/// The signature of a write-through method of an `impl`.
fn write_signature(head: &str) -> String {
    let mut parameters = vec!["&self".to_owned()];
    parameters.extend(
        WRITE_PARAMETERS
            .iter()
            .map(|parameter| (*parameter).to_owned()),
    );
    layout::signature(4, head, &parameters, "io::Result<()>")
}

/// A struct's `Transform`, `Visit`, and `WriteThrough`, and the helpers of a
/// struct too large for one function.
fn node_impls(
    names: &Names<'_>,
    index: usize,
    node: &Node,
    hook: &str,
    out: &mut String,
    helpers: &mut String,
) {
    let module = &names.catalog.modules[node.module].name;
    let target = format!("{module}::{}", node.name);
    let prefix = bare(hook);
    out.push('\n');
    out.push_str(&layout::impl_header("super::Transform", &target));
    transform_method(module, hook, out);
    out.push_str(
        "    fn transform_children(&self, ctx: &mut dyn officekit_markup::Context) -> Self {\n",
    );
    rebuild(node, &target, prefix, out, helpers);
    out.push_str("    }\n}\n\n");
    out.push_str(&layout::impl_header("super::Visit", &target));
    visit_method(module, hook, out);
    out.push_str("    fn visit_children(&self, visitor: &mut dyn officekit_markup::Visitor) {\n");
    walk(node, &target, prefix, out, helpers);
    out.push_str("    }\n}\n\n");
    out.push_str(&layout::impl_header("WriteThrough", &target));
    out.push_str(&write_signature("fn write_children_through"));
    through(names, index, node, &target, prefix, out, helpers);
    out.push_str("    }\n\n");
    write_through_method(module, hook, out);
    out.push_str("}\n");
}

/// The declared children `transform_children` rebuilds, `from` the node it
/// reads, in schema order, the order `officekit_markup::Placement` counts
/// them in, each counting what it became in `rebase`.
fn rebuilt_children(node: &Node, from: &str, rebase: &str) -> Vec<(String, Expr)> {
    steps(node)
        .into_iter()
        .map(|(field, step)| {
            let value = Expr::call(
                &format!("super::transform_{}", step.method()),
                vec![
                    Expr::Atom(format!("&{from}.{field}")),
                    Expr::atom("ctx"),
                    Expr::atom(rebase),
                ],
            );
            (field.to_owned(), value)
        })
        .collect()
}

/// The node's unknown content rebuilt, each `Extra` placed by `rebase`.
fn rebuilt_unknown(rebase: Expr) -> Expr {
    Expr::call(
        "super::transform_unknown",
        vec![Expr::atom("&self.unknown"), Expr::atom("ctx"), rebase],
    )
}

/// A declared child set, at `indent`, on the node `transform_children`
/// rebuilds.
fn assigned_field(indent: &str, name: &str, value: &Expr, out: &mut String) {
    layout::statement(&format!("{indent}node.{name} = "), value, out);
}

/// The body of a struct's `transform_children`: the node's own fields copied
/// by its `without_children`, then its declared children rebuilt in schema
/// order, so their hooks are asked in document order, then its unknown
/// content placed by what they became; or, for a struct too large for one
/// function, its children set by helpers in the same order.
fn rebuild(node: &Node, target: &str, prefix: &str, out: &mut String, helpers: &mut String) {
    let children = rebuilt_children(node, "self", "&mut rebase");
    let rebase = if children.is_empty() {
        Expr::reference(Expr::call("super::Rebase::default", Vec::new()))
    } else {
        Expr::atom("&rebase")
    };
    let unknown = rebuilt_unknown(rebase);
    let mut body = String::from("        let mut node = self.without_children();\n");
    if !children.is_empty() {
        body.push_str("        let mut rebase = super::Rebase::new(&self.unknown);\n");
    }
    for (name, value) in &children {
        assigned_field("        ", name, value, &mut body);
    }
    layout::statement("        node.unknown = ", &unknown, &mut body);
    body.push_str("        node\n");
    if body.lines().count() < BODY_LINES {
        out.push_str(&body);
        return;
    }
    let mut chunks: Vec<String> = Vec::new();
    let mut chunk = String::new();
    for (name, value) in &rebuilt_children(node, "from", "rebase") {
        let mut text = String::new();
        assigned_field("    ", name, value, &mut text);
        if !chunk.is_empty() && chunk.lines().count() + text.lines().count() >= BODY_LINES {
            chunks.push(chunk);
            chunk = String::new();
        }
        chunk.push_str(&text);
    }
    chunks.push(chunk);
    out.push_str("        let mut node = self.without_children();\n");
    out.push_str("        let mut rebase = super::Rebase::new(&self.unknown);\n");
    let parameters = [
        format!("from: &{target}"),
        format!("node: &mut {target}"),
        "ctx: &mut dyn officekit_markup::Context".to_owned(),
        "rebase: &mut super::Rebase".to_owned(),
    ];
    for (number, text) in chunks.iter().enumerate() {
        let helper = format!("transform_{prefix}_{}", number + 1);
        let arguments = vec![
            Expr::atom("self"),
            Expr::atom("&mut node"),
            Expr::atom("ctx"),
            Expr::atom("&mut rebase"),
        ];
        layout::statement("        ", &Expr::call(&helper, arguments), out);
        helpers.push('\n');
        helpers.push_str(&layout::signature(
            0,
            &format!("fn {helper}"),
            &parameters,
            "",
        ));
        helpers.push_str(text);
        helpers.push_str("}\n");
    }
    layout::statement("        node.unknown = ", &unknown, out);
    out.push_str("        node\n");
}

/// A node's declared children in schema order, each with the step that
/// walks and writes it.
fn steps(node: &Node) -> Vec<(&str, Step)> {
    match &node.body {
        Body::Text(_) => Vec::new(),
        Body::Children(children) => children
            .iter()
            .map(|child| (child.field.as_str(), Step::of(child.target, child.occurs)))
            .collect(),
    }
}

/// The body of a struct's `visit_children`: its children walked in document
/// order, through helpers for a struct too large for one function.
fn walk(node: &Node, target: &str, prefix: &str, out: &mut String, helpers: &mut String) {
    let steps = steps(node);
    if steps.is_empty() {
        out.push_str("        super::Walk::new(&self.unknown).finish(visitor);\n");
        return;
    }
    let statement = |from: &str, indent: &str, field: &str, step: Step, text: &mut String| {
        layout::statement(
            indent,
            &Expr::call(
                &format!("walk.{}", step.method()),
                vec![
                    Expr::Atom(format!("&{from}.{field}")),
                    Expr::atom("visitor"),
                ],
            ),
            text,
        );
    };
    out.push_str("        let mut walk = super::Walk::new(&self.unknown);\n");
    let mut body = String::new();
    for (field, step) in &steps {
        statement("self", "        ", field, *step, &mut body);
    }
    if body.lines().count() + 2 < BODY_LINES {
        out.push_str(&body);
    } else {
        let mut chunks: Vec<String> = Vec::new();
        let mut chunk = String::new();
        for (field, step) in &steps {
            let mut text = String::new();
            statement("node", "    ", field, *step, &mut text);
            if !chunk.is_empty() && chunk.lines().count() + text.lines().count() >= BODY_LINES {
                chunks.push(chunk);
                chunk = String::new();
            }
            chunk.push_str(&text);
        }
        chunks.push(chunk);
        for (number, text) in chunks.iter().enumerate() {
            let helper = format!("visit_{prefix}_{}", number + 1);
            let _ = writeln!(out, "        {helper}(self, &mut walk, visitor);");
            helpers.push('\n');
            helpers.push_str(&layout::signature(
                0,
                &format!("fn {helper}"),
                &[
                    format!("node: &{target}"),
                    "walk: &mut super::Walk<'_>".to_owned(),
                    "visitor: &mut dyn officekit_markup::Visitor".to_owned(),
                ],
                "",
            ));
            helpers.push_str(text);
            helpers.push_str("}\n");
        }
    }
    out.push_str("        walk.finish(visitor);\n");
}

/// The calls of a struct's `write_children_through` that write its
/// attributes, then its text or children, each on lines at `indent`, reading
/// the node `from`.
fn through_calls(node: &Node, from: &str, indent: &str) -> Vec<String> {
    let mut texts = Vec::new();
    for (position, attribute) in node.attributes.iter().enumerate() {
        let mut text = String::new();
        layout::statement(
            indent,
            &Expr::call(
                "through.attribute",
                vec![
                    Expr::Atom(position.to_string()),
                    Expr::Atom(format!("&{from}.{}", attribute.field)),
                ],
            ),
            &mut text,
        );
        texts.push(text);
    }
    let mut written: Vec<(String, &str)> = Vec::new();
    if let Body::Text(_) = node.body {
        written.push((format!("&{from}.value"), "text"));
    }
    for (field, step) in steps(node) {
        written.push((format!("&{from}.{field}"), step.method()));
    }
    for (argument, method) in written {
        let mut arguments = vec![Expr::Atom(argument)];
        if method != "text" {
            arguments.push(Expr::atom("ctx"));
        }
        let mut text = String::new();
        layout::line(
            indent,
            &Expr::call(&format!("through.{method}"), arguments),
            "?;",
            &mut text,
        );
        texts.push(text);
    }
    texts
}

/// The body of a struct's `write_children_through`: its attributes, then
/// its text or children, each written through, through helpers for a struct
/// too large for one function.
fn through(
    names: &Names<'_>,
    index: usize,
    node: &Node,
    target: &str,
    prefix: &str,
    out: &mut String,
    helpers: &mut String,
) {
    let shape = names.shape(index);
    let module = &names.catalog.modules[node.module].name;
    let own = through_calls(node, "self", "        ");
    let binding = if own.is_empty() {
        "through"
    } else {
        "mut through"
    };
    layout::statement(
        &format!("        let {binding} = "),
        &Expr::call(
            "super::Through::new",
            vec![
                Expr::atom("out"),
                Expr::Atom(format!("&{module}::{shape}")),
                Expr::atom("&self.unknown"),
            ],
        ),
        out,
    );
    if own.iter().map(|text| text.lines().count()).sum::<usize>() + 3 < BODY_LINES {
        for text in own {
            out.push_str(&text);
        }
    } else {
        let mut chunks: Vec<String> = Vec::new();
        let mut chunk = String::new();
        for text in through_calls(node, "node", "    ") {
            if !chunk.is_empty() && chunk.lines().count() + text.lines().count() + 1 >= BODY_LINES {
                chunks.push(chunk);
                chunk = String::new();
            }
            chunk.push_str(&text);
        }
        chunks.push(chunk);
        for (number, text) in chunks.iter().enumerate() {
            through_helper(
                &format!("write_{prefix}_{}", number + 1),
                target,
                text,
                out,
                helpers,
            );
        }
    }
    out.push_str("        through.finish(ctx)\n");
}

/// A helper writing one run of a large struct's attributes and children,
/// and its call; a run of attributes alone cannot fail.
fn through_helper(helper: &str, target: &str, text: &str, out: &mut String, helpers: &mut String) {
    let mut parameters = vec![
        format!("node: &{target}"),
        "through: &mut super::Through<'_, '_, '_>".to_owned(),
    ];
    let mut arguments = String::from("self, &mut through");
    if text.contains("ctx") {
        parameters.push("ctx: &mut dyn officekit_markup::Context".to_owned());
        arguments.push_str(", ctx");
    }
    let fallible = text.contains('?');
    let (output, end) = if fallible {
        ("io::Result<()>", "?")
    } else {
        ("", "")
    };
    let _ = writeln!(out, "        {helper}({arguments}){end};");
    helpers.push('\n');
    helpers.push_str(&layout::signature(
        0,
        &format!("fn {helper}"),
        &parameters,
        output,
    ));
    helpers.push_str(text);
    if fallible {
        helpers.push_str("    Ok(())\n");
    }
    helpers.push_str("}\n");
}

/// An enum's `Item`, `Transform`, `Visit`, and `WriteThrough`, and the
/// helpers of an enum with too many arms for one function.
fn enum_impls(
    names: &Names<'_>,
    found: &EnumDef,
    hook: &str,
    list: bool,
    out: &mut String,
    helpers: &mut String,
) {
    let module = &names.catalog.modules[found.module].name;
    let target = format!("{module}::{}", found.name);
    let prefix = bare(hook);
    out.push('\n');
    out.push_str(&layout::impl_header("super::Item", &target));
    out.push_str("    fn is_declared(&self) -> bool {\n");
    if found.arms.is_empty() {
        out.push_str("        false\n    }\n");
    } else {
        out.push_str("        !matches!(self, Self::Other(_))\n    }\n");
    }
    if list {
        out.push_str(
            "\n    fn items(&self, ctx: &mut dyn officekit_markup::Context) -> Option<Vec<Self>> {\n",
        );
        layout::line(
            "        ",
            &ask(
                "ask",
                "ctx",
                module,
                &format!("{module}::Context::{prefix}_items"),
                false,
            ),
            "",
            out,
        );
        out.push_str("    }\n\n");
        let mut parameters = vec!["&self".to_owned()];
        parameters.extend(
            WRITE_PARAMETERS
                .iter()
                .map(|parameter| (*parameter).to_owned()),
        );
        out.push_str(&layout::signature(
            4,
            "fn write_items",
            &parameters,
            "io::Result<Written>",
        ));
        layout::line(
            "        ",
            &ask(
                "ask_write",
                "ctx",
                module,
                &format!("{module}::Context::write_{prefix}_items"),
                true,
            ),
            "",
            out,
        );
        out.push_str("    }\n");
    }
    out.push_str("}\n\n");
    let arms = arm_list(found);
    out.push_str(&layout::impl_header("super::Transform", &target));
    transform_method(module, hook, out);
    out.push_str(
        "    fn transform_children(&self, ctx: &mut dyn officekit_markup::Context) -> Self {\n",
    );
    if let Some(text) = one_chain(&arms, Traversal::Transform) {
        out.push_str(&text);
    } else {
        chunked_transform(&target, prefix, &arms, out, helpers);
    }
    out.push_str("    }\n}\n\n");
    out.push_str(&layout::impl_header("super::Visit", &target));
    visit_method(module, hook, out);
    out.push_str("    fn visit_children(&self, visitor: &mut dyn officekit_markup::Visitor) {\n");
    if let Some(text) = one_chain(&arms, Traversal::Visit) {
        out.push_str(&text);
    } else {
        chunked_steps(&target, prefix, &arms, Traversal::Visit, out, helpers);
    }
    out.push_str("    }\n}\n\n");
    out.push_str(&layout::impl_header("WriteThrough", &target));
    out.push_str(&write_signature("fn write_children_through"));
    if let Some(text) = one_chain(&arms, Traversal::Write) {
        out.push_str(&text);
    } else {
        chunked_steps(&target, prefix, &arms, Traversal::Write, out, helpers);
    }
    out.push_str("    }\n\n");
    write_through_method(module, hook, out);
    out.push_str("}\n");
}

/// An enum's arms, `Other` among them in alphabetical order: each variant,
/// whether it holds its struct boxed, and whether it is `Other`.
fn arm_list(found: &EnumDef) -> Vec<(String, bool, bool)> {
    let mut arms: Vec<(String, bool, bool)> = found
        .arms
        .iter()
        .map(|(variant, arm)| (variant.clone(), found.boxed.contains(arm), false))
        .chain(iter::once(("Other".to_owned(), false, true)))
        .collect();
    arms.sort_by(|left, right| left.0.cmp(&right.0));
    arms
}

/// An enum's arms as `if let` blocks, when one function holds them: a chain
/// ending in `else` for `transform` and `write`, and a block per arm for
/// `visit`, where clippy refuses both an empty last `else` and none.
///
/// No `match`: one on another crate's `#[non_exhaustive]` enum needs the `_`
/// arm clippy's `wildcard_enum_match_arm` refuses. The last `else` runs on no
/// variant the generator knows.
fn one_chain(arms: &[(String, bool, bool)], traversal: Traversal) -> Option<String> {
    if arms.len() > BLOCKS {
        return None;
    }
    let visit = matches!(traversal, Traversal::Visit);
    let mut text = String::new();
    for (number, (variant, boxed, other)) in arms.iter().enumerate() {
        let binding = if *other { "node" } else { "arm" };
        let head = layout::if_let(
            BODY,
            &Expr::pattern(&format!("Self::{variant}"), binding),
            "self",
        );
        if number == 0 || visit {
            text.push_str(&head);
        } else {
            text.push_str("        } else ");
            text.push_str(head.trim_start());
        }
        let body = traversal.body("Self", variant, binding, *boxed);
        layout::line(
            "            ",
            &body,
            if visit { ";" } else { "" },
            &mut text,
        );
        if visit {
            text.push_str("        }\n");
        }
    }
    match traversal {
        Traversal::Transform => {
            text.push_str("        } else {\n            self.clone()\n        }\n");
        }
        Traversal::Write => text.push_str("        } else {\n            Ok(())\n        }\n"),
        Traversal::Visit => {}
    }
    (text.lines().count() + 2 < BODY_LINES).then_some(text)
}

/// The `if let` blocks of an enum's declared arms, split into runs that each
/// fit one helper, `inner` writing each block's body.
fn arm_blocks<F>(target: &str, arms: &[(String, bool, bool)], inner: F) -> Vec<String>
where
    F: Fn(&str, bool, &mut String),
{
    let mut chunks: Vec<String> = Vec::new();
    let mut chunk = String::new();
    for (variant, boxed, other) in arms {
        if *other {
            continue;
        }
        let mut text = layout::if_let(
            4,
            &Expr::pattern(&format!("{target}::{variant}"), "arm"),
            "item",
        );
        inner(variant, *boxed, &mut text);
        text.push_str("    }\n");
        let full = chunk.lines().count() + text.lines().count() + 1 >= BODY_LINES
            || chunk.matches("    if let ").count() == BLOCKS;
        if !chunk.is_empty() && full {
            chunks.push(chunk);
            chunk = String::new();
        }
        chunk.push_str(&text);
    }
    chunks.push(chunk);
    chunks
}

/// `transform_children` of an enum with too many arms for one function: its
/// `Other` item, then helpers tried in turn, each for a run of arms.
fn chunked_transform(
    target: &str,
    prefix: &str,
    arms: &[(String, bool, bool)],
    out: &mut String,
    helpers: &mut String,
) {
    let chunks = arm_blocks(target, arms, |variant, boxed, text| {
        let transformed = Expr::call("arm.transform", vec![Expr::atom("ctx")]);
        let value = if boxed {
            Expr::call("Box::new", vec![transformed])
        } else {
            transformed
        };
        layout::statement("        let node = ", &value, text);
        layout::line(
            "        return ",
            &Expr::call(
                "Some",
                vec![Expr::call(
                    &format!("{target}::{variant}"),
                    vec![Expr::atom("node")],
                )],
            ),
            ";",
            text,
        );
    });
    let mut functions = Vec::new();
    for (number, text) in chunks.iter().enumerate() {
        let helper = format!("transform_{prefix}_{}", number + 1);
        helpers.push('\n');
        helpers.push_str(&layout::signature(
            0,
            &format!("fn {helper}"),
            &[
                format!("item: &{target}"),
                "ctx: &mut dyn officekit_markup::Context".to_owned(),
            ],
            &format!("Option<{target}>"),
        ));
        helpers.push_str(text);
        helpers.push_str("    None\n}\n");
        functions.push(helper);
    }
    out.push_str(
        "        if let Self::Other(node) = self {\n            \
         return Self::Other(node.transform(ctx));\n        }\n",
    );
    // Every declared arm has a helper; an item none takes is kept as it is.
    let calls: Vec<String> = functions
        .iter()
        .skip(1)
        .map(|function| format!(".or_else(|| {function}(self, ctx))"))
        .chain(iter::once(".unwrap_or_else(|| self.clone())".to_owned()))
        .collect();
    out.push_str(&layout::chain(
        BODY,
        &format!(
            "{}(self, ctx)",
            functions.first().map_or("", String::as_str)
        ),
        &calls,
    ));
}

/// `visit_children` or `write_children_through` of an enum with too many arms
/// for one function: its `Other` item, then each helper, each for a run of arms.
fn chunked_steps(
    target: &str,
    prefix: &str,
    arms: &[(String, bool, bool)],
    traversal: Traversal,
    out: &mut String,
    helpers: &mut String,
) {
    let write = matches!(traversal, Traversal::Write);
    let chunks = arm_blocks(target, arms, |variant, boxed, text| {
        let body = traversal.body(target, variant, "arm", boxed);
        if write {
            layout::line("        return ", &body, ";", text);
        } else {
            layout::statement("        ", &body, text);
        }
    });
    let (name, parameters, output, last) = if write {
        (
            "write",
            WRITE_PARAMETERS
                .iter()
                .map(|parameter| (*parameter).to_owned())
                .collect::<Vec<String>>(),
            "io::Result<()>",
            "    Ok(())\n",
        )
    } else {
        (
            "visit",
            vec!["visitor: &mut dyn officekit_markup::Visitor".to_owned()],
            "",
            "",
        )
    };
    if write {
        out.push_str(
            "        if let Self::Other(node) = self {\n            \
             return node.write_through(ctx, out);\n        }\n",
        );
    } else {
        out.push_str(
            "        if let Self::Other(node) = self {\n            node.visit(visitor);\n        }\n",
        );
    }
    let arguments = if write {
        "self, ctx, out"
    } else {
        "self, visitor"
    };
    let count = chunks.len();
    for (number, text) in chunks.iter().enumerate() {
        let helper = format!("{name}_{prefix}_{}", number + 1);
        let mut all = vec![format!("item: &{target}")];
        all.extend(parameters.iter().cloned());
        helpers.push('\n');
        helpers.push_str(&layout::signature(0, &format!("fn {helper}"), &all, output));
        helpers.push_str(text);
        helpers.push_str(last);
        helpers.push_str("}\n");
        let end = if !write {
            ";"
        } else if number + 1 == count {
            ""
        } else {
            "?;"
        };
        let _ = writeln!(out, "        {helper}({arguments}){end}");
    }
}

#[cfg(test)]
mod tests;
