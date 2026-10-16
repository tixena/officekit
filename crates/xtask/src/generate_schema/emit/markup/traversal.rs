//! The traversals of the typed tree (D12, D17, D20, D37): each namespace's
//! hook traits, `Context` and `Visitor`; every node's `Transform`, `Visit`,
//! and `WriteThrough`, and each enum's `markup::Item`, in the namespace's
//! `traversal.rs`; and the root accessors, one per namespace.
//!
//! A struct's hook is its reader's name, its name in snake case; an enum's is
//! its name without the trailing `Content`, or the full name when that is a
//! struct's. A list hook adds `_items` to its enum's hook, and a write hook
//! puts `write_` before a tree or list hook.

use alloc::collections::BTreeSet;
use core::fmt::Write as _;
use core::iter;

use super::render::layout::{self, Expr, MAX_WIDTH};
use super::render::{BODY_LINES, CHUNK, Names};
use super::structs::{Body, Catalog, EnumDef, Node, Occurs, Target};

/// The root context's hooks for content the schemas do not declare, as the
/// runtime declared them.
const ELEMENT_HOOKS: [(&str, &str); 2] = [
    (
        "element",
        "    /// Asked for each generic element: an unknown element in a list's `Other`
    /// items, in a node's `unknown.children`, or in a wildcard position.
    /// `None` rebuilds the element from its transformed children.
    fn element(&mut self, _node: &xml::element::Element) -> Option<xml::element::Element> {
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
    fn write_element(
        &mut self,
        _node: &xml::element::Element,
        _out: &mut PartWriter<'_>,
    ) -> io::Result<Written> {
        Ok(Written::No)
    }
",
    ),
];

/// The root visitor's hook for content the schemas do not declare.
const ELEMENT_VISITOR_HOOK: &str =
    "    /// Shown each generic element; `Descend::Yes` walks into its children.
    fn element(&mut self, _node: &xml::element::Element) -> Descend {
        Descend::Yes
    }
";

/// The imports of every `traversal.rs` after `crate::markup`'s.
const TRAVERSAL_USES: &str = "use crate::stream::transform::{Unchanged, WriteThrough, Written};
use crate::stream::writer::PartWriter;
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
const WRITE_PARAMETERS: [&str; 2] = ["ctx: &mut dyn markup::Context", "out: &mut PartWriter<'_>"];

/// How a field's value is rebuilt: copied, or the value of an expression.
enum Rebuilt {
    Copied,
    Expr(Expr),
}

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
    /// The method of `markup::Walk` and `markup::Through` this step calls.
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

/// Whether a namespace module has a struct or an enum, which its
/// `traversal.rs` implements the traversals for.
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

/// A write hook, `Written::No` by default.
fn write_hook_text(name: &str, node: &str) -> String {
    let mut text = String::from("    /// # Errors\n    ///\n    /// The sink's.\n");
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
            hook_text(
                &hook,
                &["&mut self".to_owned(), own.clone()],
                &format!("Option<{node}>"),
                "None",
            ),
        ));
        let write = format!("write_{}", bare(&hook));
        entries.push((write.clone(), write_hook_text(&write, node)));
        if list {
            let items = format!("{}_items", bare(&hook));
            entries.push((
                items.clone(),
                hook_text(
                    &items,
                    &["&mut self".to_owned(), own],
                    &format!("Option<Vec<{node}>>"),
                    "None",
                ),
            ));
            let write_items = format!("write_{items}");
            entries.push((write_items.clone(), write_hook_text(&write_items, node)));
        }
    }
    let mut out = layout::doc(
        0,
        &format!(
            "The hooks of a context for the `{prefix}` namespace (D12, D20, D37).\n\nA tree hook \
             and a write hook per node kind, and a list hook and a list write hook per enum a list \
             holds. A tree hook answers `None` to rebuild the node from its transformed \
             children, or a node in its place; a list hook answers `None` to leave the item to its \
             own `transform`, or any number of items, none included, in its place; a write hook \
             answers `Written::No` to leave the node to its tree hook, or writes in its place and \
             answers `Written::Yes`. A context that does not return itself from \
             `markup::Context::{accessor}` is never asked.",
            prefix = module.prefix,
            accessor = module.name,
        ),
    );
    out.push_str(&trait_text("pub trait Context: markup::Context", entries));
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
    let entries = hooks(names, &BTreeSet::new())
        .into_iter()
        .map(|(hook, node, _)| {
            (
                bare(&hook).to_owned(),
                hook_text(
                    &hook,
                    &["&mut self".to_owned(), format!("_node: &{node}")],
                    "markup::Descend",
                    "markup::Descend::Yes",
                ),
            )
        })
        .collect();
    let mut out = layout::doc(
        0,
        &format!(
            "The hooks of a visitor for the `{prefix}` namespace (D12).\n\nOne per node kind, each \
             answering `Descend::Yes` to walk into the node's children or `Descend::No` to skip \
             them. A visitor that does not return itself from `markup::Visitor::{accessor}` is \
             never asked.",
            prefix = module.prefix,
            accessor = module.name,
        ),
    );
    out.push_str(&trait_text("pub trait Visitor: markup::Visitor", entries));
    out
}

/// The root hooks, `markup/root.rs`: the root `Context` with its hooks for
/// content the schemas do not declare and one accessor per namespace, and the
/// root `Visitor` the same way, each in alphabetical order.
#[must_use]
pub fn root_text(catalog: &Catalog, order: &[usize]) -> String {
    let accessor = |module: &str, hooks: &str| {
        hook_text(
            module,
            &["&mut self".to_owned()],
            &format!("Option<&mut dyn {module}::{hooks}>"),
            "None",
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
        "// The root hooks every namespace's `Context` and `Visitor` extend (D12),\n\
         // brought into `markup` with `include!`.\n\n",
    );
    out.push_str(&layout::doc(
        0,
        "The root of every context, which each namespace's `Context` extends and no other \
         trait.\n\nIt holds the hooks for content the schemas do not declare, and one accessor \
         per namespace, through which a context opts into that namespace's hooks by returning \
         itself.",
    ));
    out.push_str("pub trait Context {\n");
    out.push_str(&joined(context));
    out.push_str("}\n\n");
    out.push_str(&layout::doc(
        0,
        "The root of every visitor, which each namespace's `Visitor` extends and no other \
         trait.\n\nIt holds the hook for content the schemas do not declare, and one accessor \
         per namespace, through which a visitor opts into that namespace's hooks by returning \
         itself.",
    ));
    out.push_str("pub trait Visitor {\n");
    out.push_str(&joined(visitor));
    out.push_str("}\n");
    out
}

/// The text of a namespace module's `traversal.rs`.
#[must_use]
pub fn text(header: &str, names: &Names<'_>, listed: &BTreeSet<usize>) -> String {
    let catalog = names.catalog;
    let module = &catalog.modules[names.module];
    let mut out = String::from(header);
    let _ = writeln!(
        out,
        "//! The traversals of `markup::{}` (D12, D17, D20, D37): each node's\n\
         //! `Transform`, `Visit`, and `WriteThrough`, and each enum's `markup::Item`,\n\
         //! every hook asked through the namespace's accessor.\n",
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
    // import those they call the methods of.
    let mut traits = vec!["self"];
    if helpers.contains(".transform(") {
        traits.push("Transform as _");
    }
    if helpers.contains(".visit(") {
        traits.push("Visit as _");
    }
    out.push_str("use std::io;\n\n");
    if traits.len() == 1 {
        out.push_str("use crate::markup;\n");
    } else {
        let _ = writeln!(out, "use crate::markup::{{{}}};", traits.join(", "));
    }
    out.push_str(TRAVERSAL_USES);
    for (_, text) in impls {
        out.push_str(&text);
    }
    out.push_str(&helpers);
    out
}

/// `markup::<ask>(<receiver>.<module>(), super::<Hooks>::<hook>, self[, out])`.
fn ask(ask: &str, receiver: &str, module: &str, hook: &str, out: bool) -> Expr {
    let mut arguments = vec![
        Expr::call(&format!("{receiver}.{module}"), Vec::new()),
        Expr::Atom(hook.to_owned()),
        Expr::atom("self"),
    ];
    if out {
        arguments.push(Expr::atom("out"));
    }
    Expr::call(&format!("markup::{ask}"), arguments)
}

/// `transform`, which asks the node's tree hook first.
fn transform_method(module: &str, hook: &str, out: &mut String) {
    out.push_str("    fn transform(&self, ctx: &mut dyn markup::Context) -> Self {\n");
    layout::statement(
        "        let node = ",
        &ask(
            "ask",
            "ctx",
            module,
            &format!("super::Context::{hook}"),
            false,
        ),
        out,
    );
    out.push_str("        node.unwrap_or_else(|| self.transform_children(ctx))\n    }\n\n");
}

/// `visit`, which shows the node to its hook first.
fn visit_method(module: &str, hook: &str, out: &mut String) {
    out.push_str("    fn visit(&self, visitor: &mut dyn markup::Visitor) {\n");
    layout::statement(
        "        let next = ",
        &ask(
            "show",
            "visitor",
            module,
            &format!("super::Visitor::{hook}"),
            false,
        ),
        out,
    );
    out.push_str(
        "        if next == markup::Descend::Yes {\n            self.visit_children(visitor);\n        }\n    }\n\n",
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
            &format!("super::Context::write_{}", bare(hook)),
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
            &format!("super::Context::{hook}"),
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
    let target = format!("super::{}", node.name);
    let prefix = bare(hook);
    out.push('\n');
    out.push_str(&layout::impl_header("markup::Transform", &target));
    transform_method(module, hook, out);
    out.push_str("    fn transform_children(&self, ctx: &mut dyn markup::Context) -> Self {\n");
    rebuild(node, &target, prefix, out, helpers);
    out.push_str("    }\n}\n\n");
    out.push_str(&layout::impl_header("markup::Visit", &target));
    visit_method(module, hook, out);
    out.push_str("    fn visit_children(&self, visitor: &mut dyn markup::Visitor) {\n");
    walk(node, &target, prefix, out, helpers);
    out.push_str("    }\n}\n\n");
    out.push_str(&layout::impl_header("WriteThrough", &target));
    out.push_str(&write_signature("fn write_children_through"));
    through(names, index, node, &target, prefix, out, helpers);
    out.push_str("    }\n\n");
    write_through_method(module, hook, out);
    out.push_str("}\n");
}

/// One field of a node rebuilt by `transform_children`: `name` and the
/// expression its value is, `from` the node it reads.
fn rebuilt_fields(node: &Node, from: &str) -> Vec<(String, Rebuilt)> {
    let mut fields: Vec<(String, Rebuilt)> = node
        .attributes
        .iter()
        .map(|attribute| (attribute.field.clone(), Rebuilt::Copied))
        .collect();
    match &node.body {
        Body::Text(_) => fields.push(("value".to_owned(), Rebuilt::Copied)),
        Body::Children(children) => {
            for child in children {
                let function = match (child.target, child.occurs) {
                    (Target::Enum(_), Occurs::List) => "markup::transform_items",
                    (Target::Enum(_) | Target::Node(_), _) => "markup::transform_child",
                };
                fields.push((
                    child.field.clone(),
                    Rebuilt::Expr(Expr::call(
                        function,
                        vec![
                            Expr::Atom(format!("&{from}.{}", child.field)),
                            Expr::atom("ctx"),
                        ],
                    )),
                ));
            }
        }
    }
    fields.push((
        "unknown".to_owned(),
        Rebuilt::Expr(Expr::call(
            "markup::transform_unknown",
            vec![Expr::Atom(format!("&{from}.unknown")), Expr::atom("ctx")],
        )),
    ));
    fields.sort_by(|left, right| bare(&left.0).cmp(bare(&right.0)));
    fields
}

/// A field of the struct literal `transform_children` returns.
fn literal_field(name: &str, value: &Rebuilt) -> String {
    match value {
        Rebuilt::Copied => {
            let flat = format!("            {name}: self.{name}.clone(),\n");
            if flat.len() - 1 <= MAX_WIDTH {
                flat
            } else {
                // rustfmt breaks the chain a field this long reads.
                format!(
                    "            {name}: self\n                .{name}\n                .clone(),\n"
                )
            }
        }
        Rebuilt::Expr(expr) => layout::field(12, name, expr),
    }
}

/// A field set in a helper of a struct too large for one literal.
fn assigned_field(name: &str, value: &Rebuilt, out: &mut String) {
    match value {
        Rebuilt::Copied => layout::statement(
            "    ",
            &Expr::call(
                "Clone::clone_from",
                vec![
                    Expr::Atom(format!("&mut node.{name}")),
                    Expr::Atom(format!("&from.{name}")),
                ],
            ),
            out,
        ),
        Rebuilt::Expr(expr) => layout::statement(&format!("    node.{name} = "), expr, out),
    }
}

/// The body of a struct's `transform_children`: its struct literal, or, for
/// a struct too large for one function, its fields set by helpers.
fn rebuild(node: &Node, target: &str, prefix: &str, out: &mut String, helpers: &mut String) {
    let fields = rebuilt_fields(node, "self");
    let lines: Vec<String> = fields
        .iter()
        .map(|(name, value)| literal_field(name, value))
        .collect();
    if lines.iter().map(|text| text.lines().count()).sum::<usize>() + 2 < BODY_LINES {
        out.push_str("        Self {\n");
        for text in lines {
            out.push_str(&text);
        }
        out.push_str("        }\n");
        return;
    }
    let from_fields = rebuilt_fields(node, "from");
    let mut chunks: Vec<String> = Vec::new();
    let mut chunk = String::new();
    for (name, value) in &from_fields {
        let mut text = String::new();
        assigned_field(name, value, &mut text);
        if !chunk.is_empty() && chunk.lines().count() + text.lines().count() >= BODY_LINES {
            chunks.push(chunk);
            chunk = String::new();
        }
        chunk.push_str(&text);
    }
    chunks.push(chunk);
    out.push_str("        let mut node = Self::default();\n");
    for (number, text) in chunks.iter().enumerate() {
        let helper = format!("transform_{prefix}_{}", number + 1);
        let mut parameters = vec![format!("from: &{target}"), format!("node: &mut {target}")];
        if text.contains("ctx") {
            parameters.push("ctx: &mut dyn markup::Context".to_owned());
            let _ = writeln!(out, "        {helper}(self, &mut node, ctx);");
        } else {
            let _ = writeln!(out, "        {helper}(self, &mut node);");
        }
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
        out.push_str("        markup::Walk::new(&self.unknown).finish(visitor);\n");
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
    out.push_str("        let mut walk = markup::Walk::new(&self.unknown);\n");
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
                    "walk: &mut markup::Walk<'_>".to_owned(),
                    "visitor: &mut dyn markup::Visitor".to_owned(),
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
    let own = through_calls(node, "self", "        ");
    let binding = if own.is_empty() {
        "through"
    } else {
        "mut through"
    };
    layout::statement(
        &format!("        let {binding} = "),
        &Expr::call(
            "markup::Through::new",
            vec![
                Expr::atom("out"),
                Expr::Atom(format!("&super::{shape}")),
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
        "through: &mut markup::Through<'_, '_, '_>".to_owned(),
    ];
    let mut arguments = String::from("self, &mut through");
    if text.contains("ctx") {
        parameters.push("ctx: &mut dyn markup::Context".to_owned());
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

/// An enum's `markup::Item`, `Transform`, `Visit`, and `WriteThrough`, and
/// the helpers of an enum with too many arms for one function.
fn enum_impls(
    names: &Names<'_>,
    found: &EnumDef,
    hook: &str,
    list: bool,
    out: &mut String,
    helpers: &mut String,
) {
    let module = &names.catalog.modules[found.module].name;
    let target = format!("super::{}", found.name);
    let prefix = bare(hook);
    out.push('\n');
    out.push_str(&layout::impl_header("markup::Item", &target));
    out.push_str("    fn is_declared(&self) -> bool {\n");
    if found.arms.is_empty() {
        out.push_str("        false\n    }\n");
    } else {
        out.push_str("        !matches!(self, Self::Other(_))\n    }\n");
    }
    if list {
        out.push_str(
            "\n    fn items(&self, ctx: &mut dyn markup::Context) -> Option<Vec<Self>> {\n",
        );
        layout::line(
            "        ",
            &ask(
                "ask",
                "ctx",
                module,
                &format!("super::Context::{prefix}_items"),
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
                &format!("super::Context::write_{prefix}_items"),
                true,
            ),
            "",
            out,
        );
        out.push_str("    }\n");
    }
    out.push_str("}\n\n");
    let arms = arm_list(found);
    out.push_str(&layout::impl_header("markup::Transform", &target));
    transform_method(module, hook, out);
    out.push_str("    fn transform_children(&self, ctx: &mut dyn markup::Context) -> Self {\n");
    if let Some(text) = one_match(found, &arms, Traversal::Transform) {
        out.push_str(&text);
    } else {
        chunked_transform(&target, prefix, &arms, out, helpers);
    }
    out.push_str("    }\n}\n\n");
    out.push_str(&layout::impl_header("markup::Visit", &target));
    visit_method(module, hook, out);
    out.push_str("    fn visit_children(&self, visitor: &mut dyn markup::Visitor) {\n");
    if let Some(text) = one_match(found, &arms, Traversal::Visit) {
        out.push_str(&text);
    } else {
        chunked_steps(&target, prefix, &arms, Traversal::Visit, out, helpers);
    }
    out.push_str("    }\n}\n\n");
    out.push_str(&layout::impl_header("WriteThrough", &target));
    out.push_str(&write_signature("fn write_children_through"));
    if let Some(text) = one_match(found, &arms, Traversal::Write) {
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

/// An enum's arms in one match, when one function holds it.
fn one_match(
    found: &EnumDef,
    arms: &[(String, bool, bool)],
    traversal: Traversal,
) -> Option<String> {
    if found.arms.len() > CHUNK {
        return None;
    }
    let mut text = String::new();
    matched(arms, traversal, &mut text);
    (text.lines().count() + 2 < BODY_LINES).then_some(text)
}

/// An enum's arms, each taking its item through `traversal`, in one match.
fn matched(arms: &[(String, bool, bool)], traversal: Traversal, out: &mut String) {
    if let [(variant, _, true)] = arms {
        let _ = writeln!(out, "        let Self::{variant}(node) = self;");
        let body = traversal.body("Self", variant, "node", false);
        let start = if matches!(traversal, Traversal::Visit) {
            ";"
        } else {
            ""
        };
        layout::line("        ", &body, start, out);
        return;
    }
    out.push_str("        match self {\n");
    for (variant, boxed, other) in arms {
        let binding = if *other { "node" } else { "arm" };
        out.push_str(&layout::arm(
            12,
            &Expr::pattern(&format!("Self::{variant}"), binding),
            &traversal.body("Self", variant, binding, *boxed),
            matches!(traversal, Traversal::Visit),
        ));
    }
    out.push_str("        }\n");
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

/// `transform_children` of an enum with too many arms for one match: its
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
                "ctx: &mut dyn markup::Context".to_owned(),
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
/// for one match: its `Other` item, then each helper, each for a run of arms.
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
            vec!["visitor: &mut dyn markup::Visitor".to_owned()],
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
