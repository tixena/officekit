//! The writers of a namespace module, `markup::<namespace>::write`: one free
//! function per struct, which hands the runtime's `Writer` each declared
//! attribute and child in schema order, and one per enum, which writes an
//! item at its place.

use core::fmt::Write as _;

use super::render::layout::{self, Expr};
use super::render::{Names, Uses};
use super::structs::Body;

/// The statements a writer function holds before it is split.
const STATEMENTS: usize = 90;

/// The text of `write.rs`.
///
#[must_use]
pub fn text(header: &str, names: &Names<'_>) -> String {
    let catalog = names.catalog;
    let module = &catalog.modules[names.module];
    let mut out = String::from(header);
    let _ = writeln!(
        out,
        "//! The writers of `markup::{}`: each struct written as its generic\n\
         //! element, its unknown content at its place.",
        module.name
    );
    let mut functions: Vec<(String, String)> = Vec::new();
    for (index, function) in &names.node_functions {
        functions.push((function.clone(), node_writer(names, *index, function)));
    }
    for (index, function) in &names.enum_functions {
        let found = &catalog.enums[*index];
        let mut text = layout::doc(
            0,
            &format!(
                "Write one item of `{}` at its place: a declared child, or an `Other` node.",
                found.name
            ),
        );
        text.push_str(&layout::signature(
            0,
            &format!("pub fn {function}"),
            &[
                format!("node: &super::{}", found.name),
                "writer: &mut markup::Writer<'_, '_>".to_owned(),
            ],
            "",
        ));
        text.push_str("    node.write_into(writer);\n}\n");
        functions.push((function.clone(), text));
    }
    functions.sort_by(|left, right| bare(&left.0).cmp(bare(&right.0)));
    let mut body = String::new();
    for (_, text) in functions {
        body.push('\n');
        body.push_str(&text);
    }
    let imports = Uses::of(&body);
    if !imports.is_empty() {
        out.push('\n');
        out.push_str(&imports);
    }
    out.push_str(&body);
    out
}

fn bare(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}

/// One struct's writer, split into helpers past `STATEMENTS` statements.
fn node_writer(names: &Names<'_>, index: usize, function: &str) -> String {
    let catalog = names.catalog;
    let node = &catalog.nodes[index];
    let mut statements: Vec<Expr> = Vec::new();
    for (position, attribute) in node.attributes.iter().enumerate() {
        statements.push(Expr::call(
            "writer.attribute",
            vec![
                Expr::Atom(position.to_string()),
                Expr::Atom(format!("&node.{}", attribute.field)),
            ],
        ));
    }
    match &node.body {
        Body::Text(_) => {
            statements.push(Expr::call("writer.text", vec![Expr::atom("&node.value")]));
        }
        Body::Children(children) => {
            for child in children {
                statements.push(Expr::call(
                    "writer.child",
                    vec![Expr::Atom(format!("&node.{}", child.field))],
                ));
            }
        }
    }
    let mut out = layout::doc(
        0,
        &format!(
            "Write a `{}` as its element, `{}:{}`.",
            node.name, catalog.modules[node.module].prefix, node.key.tag.local
        ),
    );
    out.push_str("#[must_use]\n");
    out.push_str(&layout::signature(
        0,
        &format!("pub fn {function}"),
        &[
            format!("node: &super::{}", node.name),
            "scope: &mut Scope".to_owned(),
        ],
        "Element",
    ));
    let new_writer = Expr::call(
        "markup::Writer::new",
        vec![
            Expr::atom("scope"),
            Expr::Atom(format!("&super::{}", names.shape(index))),
            Expr::atom("&node.unknown"),
        ],
    );
    if statements.is_empty() {
        if layout::fits_one_line(4, &new_writer, ".finish()".len()) {
            let _ = writeln!(out, "    {}.finish()", new_writer.one_line());
        } else {
            layout::statement("    let writer = ", &new_writer, &mut out);
            out.push_str("    writer.finish()\n");
        }
        out.push_str("}\n");
        return out;
    }
    layout::statement("    let mut writer = ", &new_writer, &mut out);
    let mut helpers = String::new();
    if statements.len() <= STATEMENTS {
        for statement in &statements {
            layout::statement("    ", statement, &mut out);
        }
    } else {
        for (chunk, lines) in statements.chunks(STATEMENTS).enumerate() {
            let helper = format!("{function}_{}", chunk + 1);
            let _ = writeln!(out, "    {helper}(node, &mut writer);");
            let _ = write!(
                helpers,
                "\n{}",
                layout::signature(
                    0,
                    &format!("fn {helper}"),
                    &[
                        format!("node: &super::{}", node.name),
                        "writer: &mut markup::Writer<'_, '_>".to_owned(),
                    ],
                    ""
                )
            );
            for statement in lines {
                layout::statement("    ", statement, &mut helpers);
            }
            helpers.push_str("}\n");
        }
    }
    out.push_str("    writer.finish()\n}\n");
    out.push_str(&helpers);
    out
}
