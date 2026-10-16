//! The readers of a namespace module, `markup::<namespace>::read`: one free
//! function per struct, which takes each declared attribute and child the
//! runtime's `Reader` hands it, and one per enum, which reads an item by its
//! tag.

use core::fmt::Write as _;

use super::render::layout::{self, Expr};
use super::render::{Names, Uses};
use super::structs::Body;

/// The struct fields a reader function holds before it is split.
const FIELDS: usize = 85;

/// The text of `read.rs`.
///
#[must_use]
pub fn text(header: &str, names: &Names<'_>) -> String {
    let catalog = names.catalog;
    let module = &catalog.modules[names.module];
    let mut out = String::from(header);
    let _ = writeln!(
        out,
        "//! The readers of `markup::{}`: each struct read from its generic\n\
         //! element, its unknown content kept at its place (D8).",
        module.name
    );
    let mut functions: Vec<(String, String)> = Vec::new();
    for (index, function) in &names.node_functions {
        functions.push((function.clone(), node_reader(names, *index, function)));
    }
    for (index, function) in &names.enum_functions {
        let found = &catalog.enums[*index];
        let mut text = layout::doc(
            0,
            &format!(
                "Read one item of `{}` from its element: the arm of its tag, or `Other`.",
                found.name
            ),
        );
        text.push_str("#[must_use]\n");
        text.push_str(&layout::signature(
            0,
            &format!("pub fn {function}"),
            &["element: &xml::element::Element".to_owned()],
            &format!("super::{}", found.name),
        ));
        text.push_str("    markup::read(element)\n}\n");
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

/// One struct's reader, split into helpers past `FIELDS` fields.
fn node_reader(names: &Names<'_>, index: usize, function: &str) -> String {
    let catalog = names.catalog;
    let node = &catalog.nodes[index];
    let mut arms: Vec<Expr> = Vec::new();
    for attribute in &node.attributes {
        arms.push(take("field.attribute", &attribute.field));
    }
    match &node.body {
        Body::Text(_) => arms.push(take("field.text", "value")),
        Body::Children(children) => {
            for child in children {
                arms.push(take("field.child", &child.field));
            }
        }
    }
    let mut out = layout::doc(
        0,
        &format!(
            "Read `{}:{}` into a `{}`.",
            catalog.modules[node.module].prefix, node.key.tag.local, node.name
        ),
    );
    out.push_str("#[must_use]\n");
    out.push_str(&layout::signature(
        0,
        &format!("pub fn {function}"),
        &["element: &xml::element::Element".to_owned()],
        &format!("super::{}", node.name),
    ));
    let new_reader = Expr::call(
        "markup::Reader::new",
        vec![
            Expr::atom("element"),
            Expr::Atom(format!("&super::{}", names.shape(index))),
        ],
    );
    if arms.is_empty() {
        layout::statement("    let reader = ", &new_reader, &mut out);
        let _ = writeln!(
            out,
            "    super::{} {{\n        unknown: reader.finish(),\n    }}\n}}",
            node.name
        );
        return out;
    }
    let _ = writeln!(out, "    let mut node = super::{}::default();", node.name);
    layout::statement("    let mut reader = ", &new_reader, &mut out);
    out.push_str("    for (index, field) in &mut reader {\n");
    let mut helpers = String::new();
    if let [only] = arms.as_slice() {
        out.push_str("        if index == 0 {\n");
        layout::line("            ", only, ";", &mut out);
        out.push_str("        }\n");
    } else if arms.len() <= FIELDS {
        out.push_str("        match index {\n");
        for (position, arm) in arms.iter().enumerate() {
            out.push_str(&layout::arm(
                12,
                &Expr::Atom(position.to_string()),
                arm,
                true,
            ));
        }
        out.push_str("            _ => {}\n        }\n");
    } else {
        let _ = writeln!(out, "        {function}_1(&mut node, index, field);");
        helpers = reader_helpers(function, &node.name, &arms);
    }
    out.push_str("    }\n    node.unknown = reader.finish();\n    node\n}\n");
    out.push_str(&helpers);
    out
}

/// The helpers of a reader past `FIELDS` fields: each matches its run of
/// field indexes and hands the others to the next.
fn reader_helpers(function: &str, node_name: &str, arms: &[Expr]) -> String {
    let mut helpers = String::new();
    let chunks: Vec<&[Expr]> = arms.chunks(FIELDS).collect();
    for (chunk, lines) in chunks.iter().enumerate() {
        let helper = format!("{function}_{}", chunk + 1);
        let _ = write!(
            helpers,
            "\n{}",
            layout::signature(
                0,
                &format!("fn {helper}"),
                &[
                    format!("node: &mut super::{node_name}"),
                    "index: usize".to_owned(),
                    "field: markup::Field<'_>".to_owned(),
                ],
                ""
            )
        );
        helpers.push_str("    match index {\n");
        for (offset, arm) in lines.iter().enumerate() {
            let position = Expr::Atom((chunk * FIELDS + offset).to_string());
            helpers.push_str(&layout::arm(8, &position, arm, true));
        }
        if chunk + 1 < chunks.len() {
            let _ = writeln!(
                helpers,
                "        _ => {function}_{}(node, index, field),",
                chunk + 2
            );
        } else {
            helpers.push_str("        _ => {}\n");
        }
        helpers.push_str("    }\n}\n");
    }
    helpers
}

/// `method(&mut node.field)`.
fn take(method: &str, field: &str) -> Expr {
    Expr::call(method, vec![Expr::Atom(format!("&mut node.{field}"))])
}
