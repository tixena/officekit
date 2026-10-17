use core::fmt::Write as _;

use super::super::render::layout::Expr;
use super::super::structs::{Attribute, Body, Child, Node, Occurs, Target, Value};
use super::super::{NodeKey, Source};
use super::{assigned_field, hook_name, rebuild};
use crate::generate_schema::model::Name;

/// A struct with the attribute `val` and `children`, in schema order.
fn node(children: Vec<Child>) -> Node {
    Node {
        attributes: vec![Attribute {
            field: "val".to_owned(),
            local: "val".to_owned(),
            namespace: None,
            value: Value::Text,
        }],
        body: Body::Children(children),
        doc: String::new(),
        key: NodeKey {
            source: Source::Data("x:CT_Sample/x:sample".to_owned()),
            tag: Name::new("urn:x", "sample"),
        },
        module: 0,
        name: "Sample".to_owned(),
        namespace: None,
        section: None,
        words: Vec::new(),
    }
}

fn child(field: &str, target: Target, occurs: Occurs) -> Child {
    Child {
        field: field.to_owned(),
        occurs,
        target,
    }
}

/// The body of `transform_children` the emitter writes for `node`, and the
/// helpers it writes for it.
fn rebuilt(node: &Node) -> (String, String) {
    let mut out = String::new();
    let mut helpers = String::new();
    rebuild(node, "x::Sample", "sample", &mut out, &mut helpers);
    (out, helpers)
}

#[test]
fn a_hook_is_its_readers_name_unless_clippy_keeps_its_prefix_for_selfless_methods() {
    assert_eq!(hook_name("paragraph"), "paragraph");
    assert_eq!(hook_name("r#type"), "r#type");
    assert_eq!(hook_name("from_color"), "node_from_color");
    assert_eq!(hook_name("to_point"), "node_to_point");
    assert_eq!(hook_name("into_shape"), "node_into_shape");
    assert_eq!(hook_name("formula"), "formula");
}

#[test]
fn an_assigned_field_too_long_for_its_line_breaks_its_call() {
    let value = Expr::call(
        "crate::transform_child",
        vec![
            Expr::atom("&self.table_style_conditional_formatting_settings_exception_xyz"),
            Expr::atom("ctx"),
            Expr::atom("&mut rebase"),
        ],
    );
    let mut out = String::new();
    assigned_field(
        "        ",
        "table_style_conditional_formatting_settings_exception_xyz",
        &value,
        &mut out,
    );
    assert_eq!(
        out,
        "        node.table_style_conditional_formatting_settings_exception_xyz = crate::transform_child(\n            \
         &self.table_style_conditional_formatting_settings_exception_xyz,\n            ctx,\n            \
         &mut rebase,\n        );\n"
    );
}

#[test]
fn a_struct_rebuilds_its_children_in_schema_order_then_places_its_unknown_content() {
    let sample = node(vec![
        child("properties", Target::Node(0), Occurs::Optional),
        child("sup", Target::Enum(0), Occurs::Required),
        child("content", Target::Enum(1), Occurs::List),
        child("sub", Target::Node(1), Occurs::Required),
    ]);
    let (out, helpers) = rebuilt(&sample);
    assert_eq!(
        out,
        "        let mut node = self.without_children();\n        \
         let mut rebase = crate::Rebase::new(&self.unknown);\n        \
         node.properties = crate::transform_child(&self.properties, ctx, &mut rebase);\n        \
         node.sup = crate::transform_choice(&self.sup, ctx, &mut rebase);\n        \
         node.content = crate::transform_items(&self.content, ctx, &mut rebase);\n        \
         node.sub = crate::transform_child(&self.sub, ctx, &mut rebase);\n        \
         node.unknown = crate::transform_unknown(&self.unknown, ctx, &rebase);\n        \
         node\n"
    );
    assert_eq!(helpers, "");
}

#[test]
fn a_struct_without_children_places_its_unknown_content_as_read() {
    let (out, _) = rebuilt(&node(Vec::new()));
    assert_eq!(
        out,
        "        let mut node = self.without_children();\n        \
         node.unknown = crate::transform_unknown(&self.unknown, ctx, &crate::Rebase::default());\n        \
         node\n"
    );
}

#[test]
fn a_struct_too_large_for_one_function_sets_its_fields_in_the_same_order_through_helpers() {
    let children = (0..100_usize)
        .map(|index| {
            child(
                &format!("item_{index:03}"),
                Target::Node(0),
                Occurs::Required,
            )
        })
        .rev()
        .collect();
    let (out, helpers) = rebuilt(&node(children));
    assert_eq!(
        out,
        "        let mut node = self.without_children();\n        \
         let mut rebase = crate::Rebase::new(&self.unknown);\n        \
         transform_sample_1(self, &mut node, ctx, &mut rebase);\n        \
         transform_sample_2(self, &mut node, ctx, &mut rebase);\n        \
         node.unknown = crate::transform_unknown(&self.unknown, ctx, &rebase);\n        \
         node\n"
    );
    let mut first = String::from(
        "\nfn transform_sample_1(\n    from: &x::Sample,\n    node: &mut x::Sample,\n    \
         ctx: &mut dyn officekit_markup::Context,\n    rebase: &mut crate::Rebase,\n) {\n",
    );
    // Schema order: the children were given from `item_099` down.
    for index in (0..100_usize).rev().take(3) {
        let _ = writeln!(
            first,
            "    node.item_{index:03} = crate::transform_child(&from.item_{index:03}, ctx, rebase);"
        );
    }
    assert!(helpers.starts_with(&first), "{helpers}");
}
