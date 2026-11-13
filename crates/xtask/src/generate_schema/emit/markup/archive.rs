//! What rkyv archives the typed tree through, under the `binary-format`
//! feature: the derives of each newtype, the bounds a type that holds itself
//! through its fields takes, and each namespace's private `archive` module, in
//! the namespace's own crate, whose mirrors archive its structs and enums.
//!
//! A struct or an enum does not derive rkyv's traits itself: the struct is
//! generic over `NodeExtra`, and the resolver enum the derive writes beside an
//! enum would be public and exhaustive. Its mirror, a remote derive in the
//! private module, holds the archived type and the resolver, and the type's
//! `Archive`, `Serialize` and `Deserialize` hand the work to it.
//! `officekit::parts`' `PartModel` archives through a mirror written the same way.

use core::fmt::Write as _;

use super::render::layout::{self, Expr, MAX_WIDTH};
use super::render::{self, BODY_LINES, Field, Form, Names, Uses};
use super::structs::{Body, Catalog, EnumDef, Node, Target};

/// The bounds of a type that holds itself through its fields: they stand in
/// for the field bounds `omit_bounds` drops, whose proof would never end.
const BOUNDS: &str = "#[rkyv(serialize_bounds(__S: Allocator + Sharing + ArchiveWriter, __S::Error: ErrorSource))]\n\
#[rkyv(deserialize_bounds(__D: Pooling, __D::Error: ErrorSource))]\n\
#[rkyv(bytecheck(bounds(__C: ArchiveContext + SharedContext, __C::Error: ErrorSource)))]\n";

/// The widest derive attribute rustfmt keeps on one line.
const DERIVE_WIDTH: usize = 96;

/// The field attribute of a field that closes a cycle of types.
pub const OMIT_BOUNDS: &str = "#[rkyv(omit_bounds)]";

/// The strongly connected components of the typed tree's types: a struct
/// leads to the types of its child fields, an enum to its arms' structs. A
/// field whose type is in its owner's component closes a cycle.
pub struct Cycles {
    /// The component of each struct, then of each enum.
    components: Vec<usize>,
    /// The count of structs, where the enums start.
    nodes: usize,
}

impl Cycles {
    /// Whether an enum's arm holding struct `arm` closes a cycle.
    #[must_use = "this only answers the question"]
    pub fn arm(&self, found: usize, arm: usize) -> bool {
        self.components[self.nodes + found] == self.components[arm]
    }

    /// Whether a struct's child field holding `target` closes a cycle.
    #[must_use = "this only answers the question"]
    pub fn child(&self, node: usize, target: Target) -> bool {
        self.components[node] == self.components[vertex(self.nodes, target)]
    }

    /// The components of `catalog`'s types.
    #[must_use = "this only builds the value"]
    pub fn new(catalog: &Catalog) -> Self {
        let nodes = catalog.nodes.len();
        let mut edges: Vec<Vec<usize>> = vec![Vec::new(); nodes + catalog.enums.len()];
        for (index, node) in catalog.nodes.iter().enumerate() {
            if let Body::Children(children) = &node.body {
                edges[index].extend(children.iter().map(|child| vertex(nodes, child.target)));
            }
        }
        for (index, found) in catalog.enums.iter().enumerate() {
            edges[nodes + index].extend(found.arms.iter().map(|(_, arm)| *arm));
        }
        Self {
            components: components(&edges),
            nodes,
        }
    }
}

/// One arm of a mirror: its variant, the type it holds as the mirror names
/// it, and whether it closes a cycle.
pub struct MirrorArm {
    pub recursive: bool,
    pub shown: String,
    pub variant: String,
}

/// The derive and rkyv attributes of a newtype deriving `own`, the rkyv ones
/// under the `binary-format` feature.
#[must_use = "this only returns the value"]
pub fn attributes(own: &str) -> String {
    let mut out = if own.len() + "#[derive()]".len() <= DERIVE_WIDTH {
        format!("#[derive({own})]\n")
    } else {
        format!("#[derive(\n    {own},\n)]\n")
    };
    out.push_str(
        "#[cfg_attr(\n    feature = \"binary-format\",\n    derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)\n)]\n",
    );
    out.push_str(
        "#[cfg_attr(feature = \"binary-format\", rkyv(attr(non_exhaustive)))]\n#[non_exhaustive]\n",
    );
    out
}

/// The text of a namespace's `archive.rs`, `None` when it has no struct and
/// no enum. `names` has given no path yet: the modules of other crates its
/// mirrors name are the file's imports.
#[must_use = "this only returns the value"]
pub fn text(header: &str, names: &Names<'_>, cycles: &Cycles) -> Option<String> {
    let catalog = names.catalog;
    let mut enums: Vec<(usize, &EnumDef)> = catalog
        .enums
        .iter()
        .enumerate()
        .filter(|(_, found)| found.module == names.module)
        .collect();
    let mut structs: Vec<(usize, &Node)> = catalog
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.module == names.module)
        .collect();
    if enums.is_empty() && structs.is_empty() {
        return None;
    }
    enums.sort_by(|left, right| left.1.name.cmp(&right.1.name));
    structs.sort_by(|left, right| left.1.name.cmp(&right.1.name));
    let holds_enums = !enums.is_empty();
    let mut mirrors = String::new();
    let mut impls = String::new();
    let mut bounded = false;
    for (index, node) in structs {
        let fields = render::struct_fields(names, cycles, index, node, Form::Mirror);
        let recursive = fields.iter().any(|field| field.recursive);
        bounded |= recursive;
        mirrors.push('\n');
        mirrors.push_str(&struct_mirror(&node.name, &fields, recursive));
        impls.push_str(&struct_from(names.runtime(), &node.name, &fields));
        impls.push_str(&handoffs(&node.name, &mirror_name(&node.name)));
    }
    for (index, found) in enums {
        let arms = mirror_arms(names, cycles, index, found);
        let recursive = arms.iter().any(|arm| arm.recursive);
        bounded |= recursive;
        mirrors.push('\n');
        mirrors.push_str(&mirror(&found.name, &arms, recursive));
        impls.push_str(&from(names.runtime(), &found.name, &arms));
        impls.push_str(&handoffs(&found.name, &found.name));
    }
    let mut out = String::from(header);
    let _ = writeln!(
        out,
        "//! The mirrors rkyv archives the structs and enums of `crate::{}` through: the archived\n\
         //! types and resolvers it writes stay in this private module.\n",
        catalog.modules[names.module].name
    );
    let mut uses = Uses::default();
    names.imports(&mut uses);
    out.push_str(&uses.text());
    if holds_enums {
        out.push_str("use officekit_xml::element::Node;\n");
    }
    out.push_str("use rkyv::de::Pooling;\n");
    out.push_str("use rkyv::rancor::{Fallible, Source as ErrorSource};\n");
    out.push_str("use rkyv::ser::{Allocator, Sharing, Writer as ArchiveWriter};\n");
    if bounded {
        out.push_str("use rkyv::validation::{ArchiveContext, SharedContext};\n");
    }
    out.push_str("use rkyv::with::{ArchiveWith as _, DeserializeWith as _, SerializeWith as _};\n");
    out.push_str(&mirrors);
    out.push_str(&impls);
    Some(out)
}

/// The name of the mirror of the struct `name`.
fn mirror_name(name: &str) -> String {
    format!("{name}Mirror")
}

/// The mirror struct of the struct `name`: the fields of the plain tree's
/// struct, `extra` as `()`.
fn struct_mirror(name: &str, fields: &[Field], recursive: bool) -> String {
    let mut out = String::from("#[derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]\n");
    let _ = writeln!(out, "#[rkyv(remote = super::{name})]");
    if recursive {
        out.push_str(BOUNDS);
    }
    out.push_str("#[rkyv(attr(non_exhaustive))]\n");
    let _ = writeln!(out, "pub struct {} {{", mirror_name(name));
    for field in fields {
        if field.recursive {
            let _ = writeln!(out, "    {OMIT_BOUNDS}");
        }
        let (label, shown) = (&field.name, &field.shown);
        let line = format!("    pub {label}: {shown},");
        if line.len() <= MAX_WIDTH {
            let _ = writeln!(out, "{line}");
        } else {
            let _ = writeln!(out, "    pub {label}:\n        {shown},");
        }
    }
    out.push_str("}\n");
    out
}

/// `From` the mirror for the struct `name`, which rkyv's remote `Deserialize`
/// calls; a struct of `BODY_LINES` fields or more is the runtime's
/// `mirror_fields!` called with every field, which expands to the literal.
fn struct_from(runtime: &str, name: &str, fields: &[Field]) -> String {
    let mirror = mirror_name(name);
    let mut out = String::from("\n");
    out.push_str(&layout::impl_header(
        &format!("From<{mirror}>"),
        &format!("super::{name}"),
    ));
    out.push_str(&layout::signature(
        4,
        "fn from",
        &[format!("item: {mirror}")],
        "Self",
    ));
    if fields.len() < BODY_LINES {
        out.push_str("        Self {\n");
        for field in fields {
            let label = &field.name;
            let line = format!("            {label}: item.{label},");
            if line.len() <= MAX_WIDTH {
                let _ = writeln!(out, "{line}");
            } else {
                let _ = writeln!(out, "            {label}: item\n                .{label},");
            }
        }
        out.push_str("        }\n");
    } else {
        let _ = writeln!(out, "        {runtime}::mirror_fields!(item;");
        let mut row = String::new();
        for field in fields {
            let label = format!("{},", field.name);
            if !row.is_empty() && 12 + row.len() + 1 + label.len() > MAX_WIDTH {
                let _ = writeln!(out, "            {row}");
                row.clear();
            }
            if !row.is_empty() {
                row.push(' ');
            }
            row.push_str(&label);
        }
        let _ = writeln!(out, "            {row}\n        )");
    }
    out.push_str("    }\n}\n");
    out
}

/// A mirror's arms, `Other` among them, in the enum's order. An arm's type of
/// this crate is named through `super`, one of another crate through its
/// module.
fn mirror_arms(
    names: &Names<'_>,
    cycles: &Cycles,
    index: usize,
    found: &EnumDef,
) -> Vec<MirrorArm> {
    let mut arms: Vec<MirrorArm> = found
        .arms
        .iter()
        .map(|(variant, arm)| {
            let node = &names.catalog.nodes[*arm];
            let from_module = names.path(node.module, &node.name);
            let path = if names.crosses(node.module) {
                from_module
            } else {
                format!("super::{from_module}")
            };
            MirrorArm {
                recursive: cycles.arm(index, *arm),
                shown: if found.boxed.contains(arm) {
                    format!("Box<{path}>")
                } else {
                    path
                },
                variant: variant.clone(),
            }
        })
        .collect();
    arms.push(MirrorArm {
        recursive: false,
        shown: "Node".to_owned(),
        variant: "Other".to_owned(),
    });
    arms.sort_by(|left, right| left.variant.cmp(&right.variant));
    arms
}

/// The mirror enum of the enum `name`.
#[must_use = "this only returns the value"]
pub fn mirror(name: &str, arms: &[MirrorArm], recursive: bool) -> String {
    let mut out = String::from("#[derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]\n");
    let _ = writeln!(out, "#[rkyv(remote = super::{name})]");
    if recursive {
        out.push_str(BOUNDS);
    }
    out.push_str("#[rkyv(attr(non_exhaustive))]\n");
    let _ = writeln!(out, "pub enum {name} {{");
    for arm in arms {
        out.push_str(&variant(arm));
    }
    out.push_str("}\n");
    out
}

/// One variant of an enum, as rustfmt lays it out: on its own line, or its
/// field on a line of its own when the variant is too wide for one.
#[must_use = "this only returns the value"]
pub fn variant(arm: &MirrorArm) -> String {
    let field = if arm.recursive {
        format!("{OMIT_BOUNDS} {}", arm.shown)
    } else {
        arm.shown.clone()
    };
    let line = format!("    {}({field}),", arm.variant);
    if line.len() <= MAX_WIDTH {
        format!("{line}\n")
    } else if arm.recursive && format!("        {field},").len() > MAX_WIDTH {
        format!(
            "    {}(\n        {OMIT_BOUNDS}\n        {},\n    ),\n",
            arm.variant, arm.shown
        )
    } else {
        format!("    {}(\n        {field},\n    ),\n", arm.variant)
    }
}

/// `From` the mirror for the enum `name`, which rkyv's remote `Deserialize`
/// calls; a match longer than `BODY_LINES` is the runtime's `mirror_into!`
/// called with every variant, which expands to the match.
#[must_use = "this only returns the value"]
pub fn from(runtime: &str, name: &str, arms: &[MirrorArm]) -> String {
    let mut out = String::from("\n");
    out.push_str(&layout::impl_header(
        &format!("From<{name}>"),
        &format!("super::{name}"),
    ));
    out.push_str(&layout::signature(
        4,
        "fn from",
        &[format!("item: {name}")],
        "Self",
    ));
    let matched: String = arms.iter().map(|arm| from_arm(name, arm)).collect();
    if matched.lines().count() < BODY_LINES {
        out.push_str("        match item {\n");
        out.push_str(&matched);
        out.push_str("        }\n");
    } else {
        let _ = writeln!(out, "        {runtime}::mirror_into!(item, {name} => Self;");
        let mut row = String::new();
        for arm in arms {
            let variant = format!("{},", arm.variant);
            if !row.is_empty() && 12 + row.len() + 1 + variant.len() > MAX_WIDTH {
                let _ = writeln!(out, "            {row}");
                row.clear();
            }
            if !row.is_empty() {
                row.push(' ');
            }
            row.push_str(&variant);
        }
        let _ = writeln!(out, "            {row}\n        )");
    }
    out.push_str("    }\n}\n");
    out
}

/// One arm of a `From` match: the mirror's variant into the enum's.
fn from_arm(name: &str, arm: &MirrorArm) -> String {
    let binding = if arm.variant == "Other" {
        "node"
    } else {
        "arm"
    };
    layout::arm(
        12,
        &Expr::pattern(&format!("{name}::{}", arm.variant), binding),
        &Expr::call(&format!("Self::{}", arm.variant), vec![Expr::atom(binding)]),
        false,
    )
}

/// The `Archive`, `Serialize` and `Deserialize` of the type `name`, each
/// handing the work to its mirror `mirror`.
#[must_use = "this only returns the value"]
pub fn handoffs(name: &str, mirror: &str) -> String {
    let mut out = String::from("\n");
    out.push_str(&layout::impl_header(
        "rkyv::Archive",
        &format!("super::{name}"),
    ));
    let _ = write!(
        out,
        "    type Archived = Archived{mirror};\n    type Resolver = {mirror}Resolver;\n\n    \
         fn resolve(&self, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {{\n"
    );
    layout::line(
        "        ",
        &Expr::call(
            &format!("{mirror}::resolve_with"),
            vec![
                Expr::atom("self"),
                Expr::atom("resolver"),
                Expr::atom("out"),
            ],
        ),
        ";",
        &mut out,
    );
    out.push_str("    }\n}\n\n");
    out.push_str(&where_header(
        "impl<__S> rkyv::Serialize<__S>",
        &format!("super::{name}"),
    ));
    out.push_str(
        "    __S: Fallible + Allocator + Sharing + ArchiveWriter + ?Sized,\n    __S::Error: ErrorSource,\n{\n",
    );
    out.push_str(&layout::signature(
        4,
        "fn serialize",
        &["&self".to_owned(), "serializer: &mut __S".to_owned()],
        "Result<Self::Resolver, __S::Error>",
    ));
    layout::line(
        "        ",
        &Expr::call(
            &format!("{mirror}::serialize_with"),
            vec![Expr::atom("self"), Expr::atom("serializer")],
        ),
        "",
        &mut out,
    );
    out.push_str("    }\n}\n\n");
    out.push_str(&where_header(
        &format!("impl<__D> rkyv::Deserialize<super::{name}, __D>"),
        &format!("Archived{mirror}"),
    ));
    out.push_str("    __D: Fallible + Pooling + ?Sized,\n    __D::Error: ErrorSource,\n{\n");
    out.push_str(&layout::signature(
        4,
        "fn deserialize",
        &["&self".to_owned(), "deserializer: &mut __D".to_owned()],
        &format!("Result<super::{name}, __D::Error>"),
    ));
    layout::line(
        "        ",
        &Expr::call(
            &format!("{mirror}::deserialize_with"),
            vec![Expr::atom("self"), Expr::atom("deserializer")],
        ),
        "",
        &mut out,
    );
    out.push_str("    }\n}\n");
    out
}

/// An `impl` header followed by `where`: `for target` on a line of its own
/// when the header is too long for one.
fn where_header(head: &str, target: &str) -> String {
    let flat = format!("{head} for {target}");
    if flat.len() <= MAX_WIDTH {
        format!("{flat}\nwhere\n")
    } else {
        format!("{head}\n    for {target}\nwhere\n")
    }
}

/// A child field's target as a vertex: a struct's index, or an enum's after
/// the structs.
const fn vertex(nodes: usize, target: Target) -> usize {
    match target {
        Target::Enum(index) => nodes + index,
        Target::Node(index) => index,
    }
}

/// Each vertex's strongly connected component, by Kosaraju's two passes, each
/// walked with a stack of its own.
fn components(edges: &[Vec<usize>]) -> Vec<usize> {
    let count = edges.len();
    let mut finished = Vec::with_capacity(count);
    let mut seen = vec![false; count];
    for start in 0..count {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![(start, 0_usize)];
        while let Some((vertex, next)) = stack.pop() {
            if let Some(&target) = edges[vertex].get(next) {
                stack.push((vertex, next + 1));
                if !seen[target] {
                    seen[target] = true;
                    stack.push((target, 0));
                }
            } else {
                finished.push(vertex);
            }
        }
    }
    let mut reversed: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (source, targets) in edges.iter().enumerate() {
        for &target in targets {
            reversed[target].push(source);
        }
    }
    let mut component = vec![usize::MAX; count];
    for &root in finished.iter().rev() {
        if component[root] != usize::MAX {
            continue;
        }
        component[root] = root;
        let mut stack = vec![root];
        while let Some(vertex) = stack.pop() {
            for &source in &reversed[vertex] {
                if component[source] == usize::MAX {
                    component[source] = root;
                    stack.push(source);
                }
            }
        }
    }
    component
}
