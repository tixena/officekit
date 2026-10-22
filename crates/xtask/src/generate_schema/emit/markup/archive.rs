//! What rkyv archives the typed tree through: the derives of each struct and
//! newtype, the bounds a type that holds itself through its fields takes, and
//! each namespace's private `archive` module, in the namespace's own crate,
//! whose mirrors archive its enums.
//!
//! An enum does not derive rkyv's traits itself: the resolver enum the derive
//! writes beside it would be public and exhaustive. Its mirror, a remote derive
//! in the private module, holds the archived enum and the resolver, and the
//! enum's `Archive`, `Serialize` and `Deserialize` hand the work to it.
//! officekit-parts' `PartModel` archives through a mirror written the same way.

use core::fmt::Write as _;

use super::render::layout::{self, Expr, MAX_WIDTH};
use super::render::{BODY_LINES, Names, Uses};
use super::structs::{Body, Catalog, EnumDef, Target};

/// The bounds of a type that holds itself through its fields: they stand in
/// for the field bounds `omit_bounds` drops, whose proof would never end.
const BOUNDS: &str = "#[rkyv(serialize_bounds(__S: Allocator + Sharing + Writer, __S::Error: Source))]\n\
#[rkyv(deserialize_bounds(__D: Pooling, __D::Error: Source))]\n\
#[rkyv(bytecheck(bounds(__C: ArchiveContext + SharedContext, __C::Error: Source)))]\n";

/// The widest derive attribute rustfmt keeps on one line.
const DERIVE_WIDTH: usize = 96;

/// The names `BOUNDS` uses, for a module's imports.
pub const BOUND_NAMES: [&str; 7] = [
    "rkyv::de::Pooling",
    "rkyv::rancor::Source",
    "rkyv::ser::Allocator",
    "rkyv::ser::Sharing",
    "rkyv::ser::Writer",
    "rkyv::validation::ArchiveContext",
    "rkyv::validation::SharedContext",
];

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
    #[must_use]
    pub fn arm(&self, found: usize, arm: usize) -> bool {
        self.components[self.nodes + found] == self.components[arm]
    }

    /// Whether a struct's child field holding `target` closes a cycle.
    #[must_use]
    pub fn child(&self, node: usize, target: Target) -> bool {
        self.components[node] == self.components[vertex(self.nodes, target)]
    }

    /// The components of `catalog`'s types.
    #[must_use]
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

/// The derive and rkyv attributes of a struct or newtype deriving `own`, with
/// `BOUNDS` when one of its fields closes a cycle.
#[must_use]
pub fn attributes(own: &str, bounded: bool) -> String {
    let derives = format!("{own}, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize");
    let mut out = if derives.len() + "#[derive()]".len() <= DERIVE_WIDTH {
        format!("#[derive({derives})]\n")
    } else {
        format!("#[derive(\n    {derives},\n)]\n")
    };
    if bounded {
        out.push_str(BOUNDS);
    }
    out.push_str("#[rkyv(attr(non_exhaustive))]\n#[non_exhaustive]\n");
    out
}

/// The text of a namespace's `archive.rs`, `None` when it has no enum.
/// `names` has given no path yet: the modules of other crates its mirrors
/// name are the file's imports.
#[must_use]
pub fn text(header: &str, names: &Names<'_>, cycles: &Cycles) -> Option<String> {
    let catalog = names.catalog;
    let mut enums: Vec<(usize, &EnumDef)> = catalog
        .enums
        .iter()
        .enumerate()
        .filter(|(_, found)| found.module == names.module)
        .collect();
    if enums.is_empty() {
        return None;
    }
    enums.sort_by(|left, right| left.1.name.cmp(&right.1.name));
    let mut mirrors = String::new();
    let mut impls = String::new();
    let mut bounded = false;
    for (index, found) in enums {
        let arms = mirror_arms(names, cycles, index, found);
        let recursive = arms.iter().any(|arm| arm.recursive);
        bounded |= recursive;
        mirrors.push('\n');
        mirrors.push_str(&mirror(&found.name, &arms, recursive));
        impls.push_str(&from(names.runtime(), &found.name, &arms));
        impls.push_str(&handoffs(&found.name));
    }
    let mut out = String::from(header);
    let _ = writeln!(
        out,
        "//! The mirrors rkyv archives the enums of `crate::{}` through: the archived\n\
         //! enums and resolvers it writes stay in this private module.\n",
        catalog.modules[names.module].name
    );
    let mut uses = Uses::default();
    names.imports(&mut uses);
    out.push_str(&uses.text());
    out.push_str("use officekit_xml::element::Node;\nuse rkyv::de::Pooling;\n");
    out.push_str("use rkyv::rancor::{Fallible, Source};\n");
    out.push_str("use rkyv::ser::{Allocator, Sharing, Writer};\n");
    if bounded {
        out.push_str("use rkyv::validation::{ArchiveContext, SharedContext};\n");
    }
    out.push_str("use rkyv::with::{ArchiveWith as _, DeserializeWith as _, SerializeWith as _};\n");
    out.push_str(&mirrors);
    out.push_str(&impls);
    Some(out)
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
#[must_use]
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
#[must_use]
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
#[must_use]
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

/// The enum's `Archive`, `Serialize` and `Deserialize`, each handing the work
/// to its mirror.
#[must_use]
pub fn handoffs(name: &str) -> String {
    let mut out = String::from("\n");
    out.push_str(&layout::impl_header(
        "rkyv::Archive",
        &format!("super::{name}"),
    ));
    let _ = write!(
        out,
        "    type Archived = Archived{name};\n    type Resolver = {name}Resolver;\n\n    \
         fn resolve(&self, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {{\n"
    );
    layout::line(
        "        ",
        &Expr::call(
            &format!("{name}::resolve_with"),
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
        "impl<S> rkyv::Serialize<S>",
        &format!("super::{name}"),
    ));
    out.push_str(
        "    S: Fallible + Allocator + Sharing + Writer + ?Sized,\n    S::Error: Source,\n{\n",
    );
    out.push_str(&layout::signature(
        4,
        "fn serialize",
        &["&self".to_owned(), "serializer: &mut S".to_owned()],
        "Result<Self::Resolver, S::Error>",
    ));
    layout::line(
        "        ",
        &Expr::call(
            &format!("{name}::serialize_with"),
            vec![Expr::atom("self"), Expr::atom("serializer")],
        ),
        "",
        &mut out,
    );
    out.push_str("    }\n}\n\n");
    out.push_str(&where_header(
        &format!("impl<D> rkyv::Deserialize<super::{name}, D>"),
        &format!("Archived{name}"),
    ));
    out.push_str("    D: Fallible + Pooling + ?Sized,\n    D::Error: Source,\n{\n");
    out.push_str(&layout::signature(
        4,
        "fn deserialize",
        &["&self".to_owned(), "deserializer: &mut D".to_owned()],
        &format!("Result<super::{name}, D::Error>"),
    ));
    layout::line(
        "        ",
        &Expr::call(
            &format!("{name}::deserialize_with"),
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
